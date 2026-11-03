//! Bulleted and numbered lists with a hanging marker column.

use std::collections::VecDeque;
use std::num::NonZeroU32;

use crate::error::{Fallible as Result, GeometryKind};
use crate::geom::{Finite, Length, NonNegative, Positive};
use crate::layout::frame::{Flowable, Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::line_run;
use crate::layout::nesting::ItemBody;
use crate::layout::style::GlyphStyle;

/// Ordered marker numbering. [`MarkerAffix`] wraps the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    Decimal,
    UpperAlpha,
    LowerAlpha,
    UpperRoman,
    LowerRoman,
}

/// Suffix or parentheses around an ordered marker body.
///
/// Bullets and custom markers do not use this.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MarkerAffix {
    #[default]
    DotSuffix,
    IdeographicComma,
    ClosingParen,
    Parens,
    Bare,
}

/// Marker position inside the marker column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MarkerAlign {
    #[default]
    Start,
    End,
}

const MIN_MARKER_NUMBER: u64 = 1;
const MAX_ALPHA_MARKER: u64 = 26;
const MAX_ROMAN_MARKER: u64 = 3999;

impl MarkerKind {
    /// Letters stop at Z and roman numerals at 3999; past that is [`GeometryKind::MarkerOutOfRange`].
    pub(crate) fn label(self, number: u64, affix: MarkerAffix) -> Result<String> {
        let body = match self {
            MarkerKind::Decimal => number.to_string(),
            MarkerKind::UpperAlpha => alpha(number)?.to_string(),
            MarkerKind::LowerAlpha => alpha(number)?.to_ascii_lowercase().to_string(),
            MarkerKind::UpperRoman => roman(number)?,
            MarkerKind::LowerRoman => roman(number)?.to_ascii_lowercase(),
        };
        Ok(match affix {
            MarkerAffix::DotSuffix => format!("{body}."),
            MarkerAffix::IdeographicComma => format!("{body}\u{3001}"),
            MarkerAffix::ClosingParen => format!("{body})"),
            MarkerAffix::Parens => format!("({body})"),
            MarkerAffix::Bare => body,
        })
    }
}

fn alpha(number: u64) -> Result<char> {
    match number {
        MIN_MARKER_NUMBER..=MAX_ALPHA_MARKER => {
            let Ok(offset) = u8::try_from(number - 1) else {
                return Err(GeometryKind::MarkerOutOfRange { number }.into());
            };
            Ok(char::from(b'A' + offset))
        }
        _ => Err(GeometryKind::MarkerOutOfRange { number }.into()),
    }
}

fn roman(mut number: u64) -> Result<String> {
    const DIGITS: [(u64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    if !(MIN_MARKER_NUMBER..=MAX_ROMAN_MARKER).contains(&number) {
        return Err(GeometryKind::MarkerOutOfRange { number }.into());
    }
    let mut out = String::new();
    for (value, digits) in DIGITS {
        while number >= value {
            out.push_str(digits);
            number -= value;
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListKind {
    Bullet,
    Ordered {
        style: MarkerKind,
        start: NonZeroU32,
    },
}

impl ListKind {
    const BULLET_MARKER: &'static str = "\u{2022}";
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListSpec<N> {
    pub(crate) kind: ListKind,
    pub(crate) marker_width: Length<Positive>,
    pub(crate) body_indent: Option<Length<Positive>>,
    pub(crate) marker_style: GlyphStyle,
    pub(crate) item_gap: Length<NonNegative>,
    pub(crate) affix: MarkerAffix,
    pub(crate) align: MarkerAlign,
    pub(crate) items: Vec<ListItem<N>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListItem<N> {
    marker: Option<String>,
    body: ItemBody<N>,
}

impl<N: Flowable> ListItem<N> {
    pub(crate) fn new(content: impl IntoIterator<Item = N>) -> Result<Self> {
        Ok(Self {
            marker: None,
            body: ItemBody::new(content)?,
        })
    }

    pub(crate) fn custom(
        marker: impl Into<String>,
        content: impl IntoIterator<Item = N>,
    ) -> Result<Self> {
        let marker = marker.into();
        if marker.is_empty() {
            return Err(GeometryKind::EmptyListMarker.into());
        }
        Ok(Self {
            marker: Some(marker),
            body: ItemBody::new(content)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PendingItem<N> {
    pending_marker: Option<String>,
    body: N,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListNode<N> {
    marker_width: Length<Positive>,
    indent: Length<Positive>,
    marker_style: GlyphStyle,
    item_gap: Length<NonNegative>,
    align: MarkerAlign,
    items: VecDeque<PendingItem<N>>,
}

impl<N: Flowable> ListNode<N> {
    pub(crate) fn new(spec: ListSpec<N>) -> Result<Self> {
        let marker_width = spec.marker_width;
        let indent = spec.body_indent.unwrap_or(marker_width);
        if indent.as_pt() < marker_width.as_pt() {
            return Err(GeometryKind::ListIndent {
                indent: indent.as_pt(),
                marker_width: marker_width.as_pt(),
            }
            .into());
        }
        let mut next: u64 = match spec.kind {
            ListKind::Ordered { start, .. } => u64::from(start.get()),
            ListKind::Bullet => 0,
        };
        let mut items = VecDeque::with_capacity(spec.items.len());
        for item in spec.items {
            let label = match (item.marker, spec.kind) {
                (Some(custom), _) => custom,
                (None, ListKind::Bullet) => ListKind::BULLET_MARKER.to_owned(),
                (None, ListKind::Ordered { style, .. }) => {
                    let label = style.label(next, spec.affix)?;
                    next += 1;
                    label
                }
            };
            items.push_back(PendingItem {
                pending_marker: Some(label),
                body: item.body.into_node(),
            });
        }
        Ok(Self {
            marker_width,
            indent,
            marker_style: spec.marker_style,
            item_gap: spec.item_gap,
            align: spec.align,
            items,
        })
    }

    pub(crate) fn append_bodies<'a>(&'a self, out: &mut Vec<&'a N>) {
        for item in &self.items {
            out.push(&item.body);
        }
    }

    pub(crate) fn split(
        mut self,
        ctx: &mut LayoutCtx<'_>,
        mut avail: Frame,
    ) -> Result<Split<Self>> {
        let body = (avail.width - Length::<NonNegative>::from(self.indent)).as_pt();
        if body <= 0.0 || self.marker_width.as_pt() >= avail.width.as_pt() {
            return Err(GeometryKind::ListIndentWiderThanFrame {
                indent: self.indent.as_pt(),
                avail: avail.width.as_pt(),
            }
            .into());
        }
        let body_w = Length::<Positive>::from_trusted_pt(body);
        let mut commands = Vec::new();
        let mut anchors = Vec::new();
        let mut y = NonNegative::ZERO;
        let mut trailing_spacing = NonNegative::ZERO;
        let mut placed_any = false;
        let mut first_baseline = None;

        while let Some(item) = self.items.pop_front() {
            let frame = Frame {
                width: body_w,
                height: avail.height,
                full_height: avail.full_height,
            };
            let (height, fragment, tail) = match item.body.split(ctx, frame)? {
                Split::Fit { height, fragment } => (height, fragment, None),
                Split::Partial {
                    head_height,
                    head,
                    tail,
                } if head_height > NonNegative::ZERO => (head_height, head, Some(tail)),
                Split::Partial { tail: node, .. } | Split::MoveToNextPage(node) => {
                    self.items.push_front(PendingItem {
                        pending_marker: item.pending_marker,
                        body: node,
                    });
                    if !placed_any {
                        return Ok(Split::MoveToNextPage(self));
                    }
                    return Ok(partial(
                        commands,
                        anchors,
                        y,
                        NonNegative::ZERO,
                        first_baseline,
                        self,
                    ));
                }
            };

            if let Some(label) = &item.pending_marker {
                let at = self.paint_marker(
                    ctx,
                    &mut commands,
                    label,
                    fragment.first_baseline,
                    y.as_pt(),
                )?;
                if !placed_any {
                    first_baseline = Some(at);
                }
            } else if !placed_any {
                first_baseline = fragment
                    .first_baseline
                    .map(|baseline| Finite::pt(baseline.as_pt() + y.as_pt()))
                    .transpose()?;
            }
            let spacing = fragment.spacing_after;
            let offset = fragment.with_offset(self.indent.into(), y.into())?;
            commands.extend(offset.commands);
            anchors.extend(offset.anchors);
            y += height;
            avail.height = avail.height.saturating_sub(height);
            placed_any = true;

            if let Some(node) = tail {
                self.items.push_front(PendingItem {
                    pending_marker: None,
                    body: node,
                });
                return Ok(partial(commands, anchors, y, spacing, first_baseline, self));
            }
            if self.items.is_empty() {
                trailing_spacing = spacing;
            } else {
                let gap = spacing + self.item_gap;
                if gap > NonNegative::ZERO && avail.height >= gap {
                    y += gap;
                    avail.height = avail.height.sub_exact(gap);
                } else if gap > NonNegative::ZERO && ctx.natural {
                    // Counting the gap uses up the frame, so the next item stays off this page.
                    y += gap;
                    avail.height = NonNegative::ZERO;
                }
            }
        }

        Ok(Split::Fit {
            height: y,
            fragment: Fragment {
                height: y,
                spacing_after: trailing_spacing,
                commands,
                anchors,
                first_baseline,
            },
        })
    }
}

impl<N> ListNode<N> {
    /// Draws `label` and returns its baseline measured from the list fragment origin.
    fn paint_marker(
        &self,
        ctx: &mut LayoutCtx<'_>,
        commands: &mut Vec<FragmentCommand>,
        label: &str,
        body_baseline: Option<Length<Finite>>,
        y: f32,
    ) -> Result<Length<Finite>> {
        let marker =
            line_run::single_line_run(ctx, label, &self.marker_style, Some(self.marker_width))?;
        let baseline = body_baseline.unwrap_or(marker.metrics.baseline);
        let marker_x = match self.align {
            MarkerAlign::Start => 0.0,
            MarkerAlign::End => (self.marker_width - marker.width).as_pt(),
        };
        let baseline_y = y + baseline.as_pt();
        line_run::append_commands(commands, &marker.pieces, marker_x, baseline_y, 0.0)?;
        Finite::pt(baseline_y)
    }
}

fn partial<N>(
    commands: Vec<FragmentCommand>,
    anchors: Vec<crate::layout::frame::AnchorMark>,
    y: Length<NonNegative>,
    spacing_after: Length<NonNegative>,
    first_baseline: Option<Length<Finite>>,
    rest: ListNode<N>,
) -> Split<ListNode<N>> {
    Split::Partial {
        head_height: y,
        head: Fragment {
            height: y,
            spacing_after,
            commands,
            anchors,
            first_baseline,
        },
        tail: rest,
    }
}
