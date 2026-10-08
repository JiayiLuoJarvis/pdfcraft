//! Bulleted and numbered lists with a hanging marker column.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::num::NonZeroU32;

use crate::error::{Error, Result};
use crate::geom::{Length, Point, Positive};
use crate::layout::flow::{FlowNode, Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::lower::ItemBody;
use crate::layout::paragraph::single_line_run;
use crate::layout::style::Style;

/// Ordered marker numbering. Each label gets a trailing `.`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerKind {
    Decimal,
    UpperAlpha,
    LowerAlpha,
    UpperRoman,
    LowerRoman,
}

impl MarkerKind {
    /// Letters stop at Z and roman numerals at 3999; past that is [`Error::InvalidGeometry`].
    fn label(self, number: u32) -> Result<String> {
        let body = match self {
            MarkerKind::Decimal => number.to_string(),
            MarkerKind::UpperAlpha => alpha(number)?.to_string(),
            MarkerKind::LowerAlpha => alpha(number)?.to_ascii_lowercase().to_string(),
            MarkerKind::UpperRoman => roman(number)?,
            MarkerKind::LowerRoman => roman(number)?.to_ascii_lowercase(),
        };
        Ok(body + ".")
    }
}

fn alpha(number: u32) -> Result<char> {
    match number {
        1..=26 => Ok(char::from(
            b'A' + u8::try_from(number - 1).expect("below 26"),
        )),
        _ => Err(Error::InvalidGeometry),
    }
}

fn roman(mut number: u32) -> Result<String> {
    const DIGITS: [(u32, &str); 13] = [
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
    if !(1..=3999).contains(&number) {
        return Err(Error::InvalidGeometry);
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
pub(crate) struct ListSpec {
    pub(crate) kind: ListKind,
    pub(crate) marker_width: Length<Positive>,
    pub(crate) nest_indent: Option<Length<Positive>>,
    pub(crate) marker_style: Style,
    pub(crate) items: Vec<ListItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListItem {
    marker: Option<String>,
    body: ItemBody,
}

impl ListItem {
    pub(crate) fn new(content: Vec<FlowNode>) -> Result<Self> {
        Ok(Self {
            marker: None,
            body: ItemBody::new(content)?,
        })
    }

    pub(crate) fn custom(marker: impl Into<String>, content: Vec<FlowNode>) -> Result<Self> {
        let marker = marker.into();
        if marker.is_empty() {
            return Err(Error::InvalidGeometry);
        }
        Ok(Self {
            marker: Some(marker),
            body: ItemBody::new(content)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PendingItem {
    pending_marker: Option<String>,
    body: FlowNode,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ListNode {
    marker_width: f32,
    indent: f32,
    marker_style: Style,
    items: VecDeque<PendingItem>,
}

impl ListNode {
    pub(crate) fn new(spec: ListSpec) -> Result<Self> {
        let marker_width = spec.marker_width.as_pt();
        let indent = spec.nest_indent.map_or(marker_width, Length::as_pt);
        if indent < marker_width {
            return Err(Error::InvalidGeometry);
        }
        let mut items = VecDeque::with_capacity(spec.items.len());
        for (index, item) in spec.items.into_iter().enumerate() {
            let label = match (item.marker, spec.kind) {
                (Some(custom), _) => custom,
                (None, ListKind::Bullet) => ListKind::BULLET_MARKER.to_owned(),
                (None, ListKind::Ordered { style, start }) => {
                    let number = u32::try_from(index)
                        .ok()
                        .and_then(|i| start.get().checked_add(i))
                        .ok_or(Error::InvalidGeometry)?;
                    style.label(number)?
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
            items,
        })
    }

    pub(crate) fn split(mut self, ctx: &mut LayoutCtx<'_>, mut avail: Frame) -> Result<Split> {
        let body_w = avail.width - self.indent;
        if body_w <= 0.0 || self.marker_width >= avail.width {
            return Err(Error::InvalidGeometry);
        }
        let mut commands = Vec::new();
        let mut y = 0.0f32;
        let mut trailing_spacing = 0.0f32;
        let mut placed_any = false;

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
                } if head_height > 0.0 => (head_height, head, Some(tail)),
                Split::Partial { tail: node, .. } | Split::MoveToNextPage(node) => {
                    self.items.push_front(PendingItem {
                        pending_marker: item.pending_marker,
                        body: node,
                    });
                    if !placed_any {
                        return Ok(Split::MoveToNextPage(FlowNode::List(self)));
                    }
                    return Ok(partial(commands, y, self));
                }
            };

            if let Some(label) = &item.pending_marker {
                let mut run =
                    single_line_run(ctx.fonts, label, &self.marker_style, self.marker_width)?;
                let baseline = first_baseline(&fragment).unwrap_or(run.origin.y().as_pt());
                run.origin = Point::try_pt(0.0, y + baseline)?;
                commands.push(FragmentCommand::Glyphs(run));
            }
            let spacing = fragment.spacing_after;
            commands.extend(fragment.with_offset(self.indent, y)?.commands);
            y += height;
            avail.height = (avail.height - height).max(0.0);
            placed_any = true;

            if let Some(node) = tail {
                self.items.push_front(PendingItem {
                    pending_marker: None,
                    body: node,
                });
                return Ok(partial(commands, y, self));
            }
            if self.items.is_empty() {
                trailing_spacing = spacing;
            } else if spacing > 0.0 && avail.height >= spacing {
                y += spacing;
                avail.height -= spacing;
            }
        }

        Ok(Split::Fit {
            height: y,
            fragment: Fragment {
                height: y,
                spacing_after: trailing_spacing,
                commands,
            },
        })
    }
}

fn partial(commands: Vec<FragmentCommand>, y: f32, rest: ListNode) -> Split {
    Split::Partial {
        head_height: y,
        head: Fragment {
            height: y,
            spacing_after: 0.0,
            commands,
        },
        tail: FlowNode::List(rest),
    }
}

fn first_baseline(fragment: &Fragment) -> Option<f32> {
    fragment.commands.iter().find_map(|command| match command {
        FragmentCommand::Glyphs(run) => Some(run.origin.y().as_pt()),
        FragmentCommand::Stroke { .. } => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::columns::ColumnWidth;
    use crate::layout::decorate::Decorations;
    use crate::layout::table::{TableNode, TableSpec};
    use crate::layout::test_util::{
        bits, fonts_and_style, metrics, page, render, runs, text, texts,
    };

    fn spec(kind: ListKind, marker_width: f32, style: &Style, items: Vec<ListItem>) -> ListSpec {
        ListSpec {
            kind,
            marker_width: Positive::pt(marker_width).unwrap(),
            nest_indent: None,
            marker_style: style.clone(),
            items,
        }
    }

    fn ordered(style: MarkerKind, start: u32) -> ListKind {
        ListKind::Ordered {
            style,
            start: NonZeroU32::new(start).unwrap(),
        }
    }

    fn item(s: &str, style: &Style) -> ListItem {
        ListItem::new(vec![text(s, style)]).unwrap()
    }

    #[test]
    fn hanging_indent_and_marker_on_first_fragment_only() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let list = ListNode::new(spec(
            ordered(MarkerKind::Decimal, 1),
            20.0,
            &style,
            vec![item("A\nB\nC", &style), item("D", &style)],
        ))
        .unwrap();
        let config = page(200.0, m.line_height * 2.0 + 1.0);
        let (result, rec) = render(
            FlowNode::List(list),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        result.unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        let first_line = m.baseline.to_bits();
        let second_line = (m.line_height + m.baseline).to_bits();
        assert_eq!(
            runs(&pages[0]).iter().map(bits).collect::<Vec<_>>(),
            [
                ("1.", 0.0f32.to_bits(), first_line),
                ("A", 20.0f32.to_bits(), first_line),
                ("B", 20.0f32.to_bits(), second_line),
            ]
        );
        assert_eq!(
            runs(&pages[1]).iter().map(bits).collect::<Vec<_>>(),
            [
                ("C", 20.0f32.to_bits(), first_line),
                ("2.", 0.0f32.to_bits(), second_line),
                ("D", 20.0f32.to_bits(), second_line),
            ]
        );
    }

    #[test]
    fn nested_list_indents_by_its_own_marker_width() {
        let (mut fonts, style) = fonts_and_style();
        let inner = ListNode::new(spec(
            ListKind::Bullet,
            16.0,
            &style,
            vec![item("in", &style)],
        ))
        .unwrap();
        let outer = ListNode::new(spec(
            ListKind::Bullet,
            20.0,
            &style,
            vec![ListItem::new(vec![text("out", &style), FlowNode::List(inner)]).unwrap()],
        ))
        .unwrap();
        let (result, rec) = render(
            FlowNode::List(outer),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        result.unwrap();
        let xs: Vec<(String, f32)> = runs(&rec.pages()[0])
            .into_iter()
            .map(|r| (r.0, r.1))
            .collect();
        assert_eq!(
            xs,
            [
                ("\u{2022}".to_owned(), 0.0),
                ("out".to_owned(), 20.0),
                ("\u{2022}".to_owned(), 20.0),
                ("in".to_owned(), 36.0),
            ]
        );
    }

    #[test]
    fn nest_indent_moves_the_body_column() {
        let (mut fonts, style) = fonts_and_style();
        let mut s = spec(ListKind::Bullet, 14.0, &style, vec![item("A", &style)]);
        s.nest_indent = Some(Positive::pt(30.0).unwrap());
        let list = ListNode::new(s).unwrap();
        let (result, rec) = render(
            FlowNode::List(list),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        result.unwrap();
        let xs: Vec<f32> = runs(&rec.pages()[0]).iter().map(|r| r.1).collect();
        assert_eq!(xs, [0.0, 30.0]);
    }

    #[test]
    fn upper_roman_counts_from_start_and_custom_marker_keeps_its_number() {
        let (mut fonts, style) = fonts_and_style();
        let list = ListNode::new(spec(
            ordered(MarkerKind::UpperRoman, 4),
            30.0,
            &style,
            vec![
                item("a", &style),
                ListItem::custom("(x)", vec![text("b", &style)]).unwrap(),
                item("c", &style),
            ],
        ))
        .unwrap();
        let (result, rec) = render(
            FlowNode::List(list),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        result.unwrap();
        assert_eq!(texts(&rec.pages()[0]), ["IV.", "a", "(x)", "b", "VI.", "c"]);
    }

    #[test]
    fn labels_for_each_marker_kind() {
        assert_eq!(MarkerKind::Decimal.label(12).unwrap(), "12.");
        assert_eq!(MarkerKind::UpperAlpha.label(3).unwrap(), "C.");
        assert_eq!(MarkerKind::LowerAlpha.label(26).unwrap(), "z.");
        assert_eq!(MarkerKind::UpperRoman.label(1994).unwrap(), "MCMXCIV.");
        assert_eq!(MarkerKind::LowerRoman.label(3999).unwrap(), "mmmcmxcix.");
    }

    #[test]
    fn labels_past_z_or_3999_are_errors() {
        assert_eq!(
            MarkerKind::UpperAlpha.label(27),
            Err(Error::InvalidGeometry)
        );
        assert_eq!(
            MarkerKind::LowerRoman.label(4000),
            Err(Error::InvalidGeometry)
        );
        let (_, style) = fonts_and_style();
        let items = (0..2).map(|_| item("a", &style)).collect();
        let err = ListNode::new(spec(
            ordered(MarkerKind::LowerAlpha, 26),
            20.0,
            &style,
            items,
        ));
        assert_eq!(err.unwrap_err(), Error::InvalidGeometry);
    }

    #[test]
    fn custom_marker_wider_than_marker_width_errors() {
        let (mut fonts, style) = fonts_and_style();
        let list = ListNode::new(spec(
            ListKind::Bullet,
            10.0,
            &style,
            vec![ListItem::custom("(1000)", vec![text("A", &style)]).unwrap()],
        ))
        .unwrap();
        let (result, rec) = render(
            FlowNode::List(list),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        assert!(
            matches!(result, Err(Error::UnbreakableWiderThanFrame { avail, .. }) if avail.to_bits() == 10.0f32.to_bits())
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn empty_custom_marker_is_rejected() {
        let (_, style) = fonts_and_style();
        let err = ListItem::custom("", vec![text("A", &style)]).unwrap_err();
        assert_eq!(err, Error::InvalidGeometry);
    }

    #[test]
    fn item_rejects_table_and_page_break() {
        let table = TableNode::new(TableSpec::new(vec![ColumnWidth::Auto])).unwrap();
        for (node, name) in [
            (FlowNode::Table(table), "table"),
            (FlowNode::PageBreak, "page break"),
        ] {
            assert_eq!(
                ListItem::new(vec![node]).unwrap_err(),
                Error::InvalidCellContent {
                    container: "list item",
                    node: name
                }
            );
        }
    }
}
