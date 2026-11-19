//! Paragraph wrapping, per-line boxes, and fragment drawing.

use std::num::NonZeroU8;

use crate::canvas;
#[cfg(test)]
use crate::error::ErrorBag;
use crate::error::{ErrorKind, Fallible as Result, GeometryKind};
#[cfg(test)]
use crate::fonts::FontStore;
use crate::geom::{Color, Finite, Length, NonNegative, Positive};
use crate::layout::frame::{Fragment, Frame, LayoutCtx, LayoutLink, Split};
use crate::layout::justify::{self, StretchPlan, StretchRequest};
use crate::layout::line_run::{self, BlankStroke, LinePainter, LinePiece, MeasuredInline};
use crate::layout::style::{GlyphStyle, Style, TextAlign};
use crate::text::cluster::{ClusterCursor, ClusteredText};
use crate::text::line_break::{self, Advance, LineBudget, Seg, Span, SpanLine, WidthPart};
use crate::text::measure::{LineExtent, LineMetrics};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextRun {
    pub(crate) text: String,
    pub(crate) style: GlyphStyle,
    pub(crate) link: Option<LayoutLink>,
}

impl TextRun {
    pub(crate) fn new(text: impl Into<String>, style: GlyphStyle) -> Self {
        Self {
            text: text.into(),
            style,
            link: None,
        }
    }

    pub(crate) fn with_link(mut self, link: Option<LayoutLink>) -> Self {
        self.link = link;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InlineItem {
    Text(TextRun),
    Blank(InlineBlank),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BlankWidth {
    Fixed(Length<NonNegative>),
    Em(u16),
    ToEndOfLine,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InlineBlank {
    pub(crate) width: BlankWidth,
    pub(crate) thickness: Length<Positive>,
    pub(crate) color: Option<Color>,
    pub(crate) baseline_offset: Length<NonNegative>,
}

const DEFAULT_BLANK_THICKNESS_PT: f32 = 1.0;
const DEFAULT_BLANK_BASELINE_OFFSET_PT: f32 = 1.0;

impl InlineBlank {
    pub(crate) fn new(width: BlankWidth) -> Self {
        Self {
            width,
            thickness: Length::from_trusted_pt(DEFAULT_BLANK_THICKNESS_PT),
            color: None,
            baseline_offset: Length::from_trusted_pt(DEFAULT_BLANK_BASELINE_OFFSET_PT),
        }
    }
}

/// One laid-out line. Pieces are relative to the line-box top-left.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) pieces: Vec<LinePiece>,
    pub(crate) height: Length<Positive>,
    /// Baseline measured from the line-box top, y down.
    pub(crate) baseline: Length<Finite>,
}

/// Unwrapped text. Wraps at the width of the first frame it is split against.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextNode {
    items: Vec<InlineItem>,
    style: Style,
}

impl TextNode {
    #[cfg(test)]
    pub(crate) fn new(text: impl Into<String>, glyph: GlyphStyle, style: Style) -> Self {
        let text = text.into();
        let items = if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineItem::Text(TextRun::new(text, glyph))]
        };
        Self { items, style }
    }

    pub(crate) fn from_runs(runs: Vec<TextRun>, style: Style) -> Self {
        Self::from_items(runs.into_iter().map(InlineItem::Text).collect(), style)
    }

    pub(crate) fn from_items(items: Vec<InlineItem>, style: Style) -> Self {
        Self { items, style }
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.items
            .iter()
            .map(|item| match item {
                InlineItem::Text(run) => run.text.len(),
                InlineItem::Blank(_) => 0,
            })
            .sum()
    }

    pub(crate) fn keep_with_next(&self) -> bool {
        self.style.keep_with_next
    }

    #[expect(dead_code, reason = "part of layout API for inspection")]
    pub(crate) fn orphans(&self) -> NonZeroU8 {
        self.style.orphans
    }

    #[expect(dead_code, reason = "part of layout API for inspection")]
    pub(crate) fn widows(&self) -> NonZeroU8 {
        self.style.widows
    }

    pub(crate) fn spacing_after(&self) -> Length<NonNegative> {
        self.style.paragraph_spacing
    }

    pub(crate) fn total_height(
        &self,
        ctx: &mut LayoutCtx<'_>,
        width: Length<Positive>,
    ) -> Result<Length<NonNegative>> {
        let laid = ParagraphNode::layout_runs(ctx, &self.items, &self.style, width)?;
        Ok(laid.total_height())
    }

    pub(crate) fn opening_height(
        &self,
        ctx: &mut LayoutCtx<'_>,
        width: Length<Positive>,
    ) -> Result<Length<NonNegative>> {
        let laid = ParagraphNode::layout_runs(ctx, &self.items, &self.style, width)?;
        Ok(laid.opening_height())
    }

    /// Every outcome hands back a laid-out `Paragraph`, so a page move never wraps twice.
    pub(crate) fn split(
        self,
        ctx: &mut LayoutCtx<'_>,
        avail: Frame,
    ) -> Result<Split<ParagraphNode>> {
        ParagraphNode::layout_runs(ctx, &self.items, &self.style, avail.width)?.split(avail)
    }
}

/// Remaining lines of a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParagraphNode {
    pub(crate) lines: Vec<LineBox>,
    pub(crate) spacing_after: Length<NonNegative>,
    pub(crate) orphans: NonZeroU8,
    pub(crate) widows: NonZeroU8,
    pub(crate) keep_with_next: bool,
    width: Length<Positive>,
}

impl ParagraphNode {
    #[cfg(test)]
    pub(crate) fn layout(
        fonts: &mut FontStore,
        text: &str,
        style: &Style,
        width: f32,
    ) -> Result<Self> {
        let width = Positive::pt(width)?;
        let items = if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineItem::Text(TextRun::new(
                text,
                GlyphStyle::plain(style),
            ))]
        };
        let mut errors = ErrorBag::default();
        let laid = {
            let mut ctx = LayoutCtx {
                natural: false,
                fonts,
                errors: &mut errors,
            };
            Self::layout_runs(&mut ctx, &items, style, width)?
        };
        if errors.is_empty() {
            Ok(laid)
        } else {
            Err(errors
                .finish(())
                .expect_err("bag was not empty")
                .into_iter()
                .next()
                .expect("Errors keeps one item"))
        }
    }

    fn layout_runs(
        ctx: &mut LayoutCtx<'_>,
        items: &[InlineItem],
        style: &Style,
        width: Length<Positive>,
    ) -> Result<Self> {
        let indent = style.first_line_indent;
        let Some(first_width) = (width - indent).try_positive() else {
            return Err(GeometryKind::FirstLineIndent {
                indent: indent.as_pt(),
                width: width.as_pt(),
            }
            .into());
        };
        let prepared = prepare_inline(ctx, items, style)?;
        let group_refs: Vec<&ClusteredText> = prepared
            .groups
            .iter()
            .map(|group| group.text.clustered())
            .collect();
        let broken = line_break::break_spans(
            &group_refs,
            &prepared.spans,
            LineBudget {
                first: first_width,
                rest: width,
            },
            |group, cluster| {
                if let Some(g) = prepared.groups.get(group) {
                    g.ink_width(cluster.glyph_range())
                } else {
                    Ok(line_break::InkWidth {
                        body: NonNegative::ZERO,
                        gap: Finite::ZERO,
                    })
                }
            },
        )?;

        let mut lines = Vec::with_capacity(broken.len());
        let line_count = broken.len();
        for (index, line) in broken.iter().enumerate() {
            let frame_width = if index == 0 { first_width } else { width };
            let frame_x = if index == 0 {
                indent
            } else {
                NonNegative::ZERO
            };
            let metrics = line_box(ctx, &prepared.groups, line, style)?;
            let pieces = place_line(
                &prepared.groups,
                line,
                style,
                frame_width,
                frame_x,
                metrics.baseline.as_pt(),
                index + 1 == line_count,
            )?;
            lines.push(LineBox {
                pieces,
                height: metrics.line_height,
                baseline: metrics.baseline,
            });
        }

        Ok(Self {
            lines,
            spacing_after: style.paragraph_spacing,
            orphans: style.orphans,
            widows: style.widows,
            keep_with_next: style.keep_with_next,
            width,
        })
    }

    pub(crate) fn keep_with_next(&self) -> bool {
        self.keep_with_next
    }

    #[expect(dead_code, reason = "part of layout API for inspection")]
    pub(crate) fn orphans(&self) -> NonZeroU8 {
        self.orphans
    }

    #[expect(dead_code, reason = "part of layout API for inspection")]
    pub(crate) fn widows(&self) -> NonZeroU8 {
        self.widows
    }

    pub(crate) fn total_height(&self) -> Length<NonNegative> {
        self.lines
            .iter()
            .map(|line| Length::<NonNegative>::from(line.height))
            .sum()
    }

    pub(crate) fn opening_height(&self) -> Length<NonNegative> {
        let count = self.lines.len().min(self.orphans.get() as usize);
        self.lines
            .iter()
            .take(count)
            .map(|line| Length::<NonNegative>::from(line.height))
            .sum()
    }

    pub(crate) fn text_bytes(&self) -> usize {
        self.lines
            .iter()
            .flat_map(|line| &line.pieces)
            .map(|piece| match piece {
                LinePiece::Glyphs(run) => run.text.len(),
                LinePiece::Stroke(_) | LinePiece::Link { .. } => 0,
            })
            .sum()
    }

    /// Lines were wrapped at `self.width`; a narrower frame would overflow silently.
    pub(crate) fn split(self, avail: Frame) -> Result<Split<Self>> {
        if self.width.as_pt() > avail.width.as_pt() {
            return Err(GeometryKind::LaidOutWiderThanFrame {
                width: self.width.as_pt(),
                avail: avail.width.as_pt(),
            }
            .into());
        }
        let Some(first) = self.lines.first() else {
            return Ok(Split::Fit {
                height: NonNegative::ZERO,
                fragment: Fragment::empty(),
            });
        };
        let tallest = self
            .lines
            .iter()
            .map(|line| line.height.as_pt())
            .fold(0.0_f32, f32::max);
        if tallest > avail.full_height.as_pt() {
            return Err(ErrorKind::UnsplittableTallerThanFrame {
                height: tallest,
                avail: avail.full_height.as_pt(),
            }
            .into());
        }
        if avail.height.as_pt() < first.height.as_pt() {
            return Ok(Split::MoveToNextPage(self));
        }

        let total = self.lines.len();
        let tops = line_tops(&self.lines);
        let n_fit = tops
            .get(1..)
            .unwrap_or(&[])
            .iter()
            .take_while(|&&bottom| bottom <= avail.height.as_pt())
            .count();
        if n_fit == 0 {
            return Ok(Split::MoveToNextPage(self));
        }

        if n_fit >= total {
            let height = Length::from_trusted_pt(tops.last().copied().unwrap_or(0.0));
            let fragment = fragment_from_lines(&self.lines, self.spacing_after)?;
            return Ok(Split::Fit { height, fragment });
        }

        let is_empty_page = avail.height == avail.full_height.into();
        let mut n = decide_split(
            total,
            n_fit,
            self.orphans.get() as usize,
            self.widows.get() as usize,
        );
        if n == 0 && is_empty_page {
            // First abandon widows:
            n = decide_split(total, n_fit, self.orphans.get() as usize, 0);
            if n == 0 {
                // Then abandon orphans:
                n = n_fit;
            }
        }
        if n == 0 {
            return Ok(Split::MoveToNextPage(self));
        }
        if n >= total {
            let height = Length::from_trusted_pt(tops.last().copied().unwrap_or(0.0));
            let fragment = fragment_from_lines(&self.lines, self.spacing_after)?;
            return Ok(Split::Fit { height, fragment });
        }

        let mut lines = self.lines;
        let tail_lines = lines.split_off(n);
        let height = Length::from_trusted_pt(tops.get(n).copied().unwrap_or(0.0));
        let head = fragment_from_lines(&lines, NonNegative::ZERO)?;
        let tail = ParagraphNode {
            lines: tail_lines,
            spacing_after: self.spacing_after,
            orphans: self.orphans,
            widows: self.widows,
            keep_with_next: self.keep_with_next,
            width: self.width,
        };
        Ok(Split::Partial {
            head_height: height,
            head,
            tail,
        })
    }
}

fn decide_split(total: usize, n_fit: usize, orphans: usize, widows: usize) -> usize {
    if total < orphans + widows {
        return 0;
    }
    let mut n = n_fit;
    let rest = total.saturating_sub(n);
    if rest > 0 && rest < widows {
        n = total.saturating_sub(widows);
    }
    if n < orphans {
        return 0;
    }
    n
}

/// Top of each line and, last, the total height. A run of equal heights is
/// `count * height` from the run's start, so uniform lines never accumulate drift.
fn line_tops(lines: &[LineBox]) -> Vec<f32> {
    let mut tops = Vec::with_capacity(lines.len() + 1);
    tops.push(0.0_f32);
    let mut base = 0.0_f32;
    let mut run: Option<(Length<Positive>, u16)> = None;
    for line in lines {
        let count = match run {
            Some((height, count)) if height == line.height && count < u16::MAX => count + 1,
            _ => {
                base = tops.last().copied().unwrap_or(0.0);
                1
            }
        };
        run = Some((line.height, count));
        tops.push(base + f32::from(count) * line.height.as_pt());
    }
    tops
}

fn fragment_from_lines(lines: &[LineBox], spacing_after: Length<NonNegative>) -> Result<Fragment> {
    let tops = line_tops(lines);
    let mut commands = Vec::new();
    for (line, top) in lines.iter().zip(&tops) {
        line_run::append_commands(&mut commands, &line.pieces, 0.0, *top, line.height.as_pt())?;
    }
    Ok(Fragment {
        height: Length::from_trusted_pt(tops.last().copied().unwrap_or(0.0)),
        spacing_after,
        commands,
        anchors: Vec::new(),
        // The first line starts at y = 0, so its line-box baseline is the fragment baseline.
        first_baseline: lines.first().map(|line| line.baseline),
    })
}

#[derive(Clone, Copy)]
struct BlankDraw {
    thickness: Length<Positive>,
    color: Option<Color>,
    baseline_offset: Length<NonNegative>,
}

struct Prepared {
    groups: Vec<MeasuredInline>,
    spans: Vec<Span<BlankDraw>>,
}

fn prepare_inline(
    ctx: &mut LayoutCtx<'_>,
    items: &[InlineItem],
    style: &Style,
) -> Result<Prepared> {
    let mut groups = Vec::new();
    let mut spans = Vec::new();
    let mut pending: Vec<TextRun> = Vec::new();
    for item in items {
        match item {
            InlineItem::Text(run) => {
                if pending.first().is_some_and(|first| first.link != run.link) {
                    flush_text(ctx, &mut pending, &mut groups, &mut spans)?;
                }
                pending.push(run.clone());
            }
            InlineItem::Blank(blank) => {
                flush_text(ctx, &mut pending, &mut groups, &mut spans)?;
                spans.push(Span::Blank {
                    advance: blank_advance(blank.width, style.font_size)?,
                    payload: BlankDraw {
                        thickness: blank.thickness,
                        color: blank.color,
                        baseline_offset: blank.baseline_offset,
                    },
                });
            }
        }
    }
    flush_text(ctx, &mut pending, &mut groups, &mut spans)?;
    Ok(Prepared { groups, spans })
}

fn flush_text(
    ctx: &mut LayoutCtx<'_>,
    pending: &mut Vec<TextRun>,
    groups: &mut Vec<MeasuredInline>,
    spans: &mut Vec<Span<BlankDraw>>,
) -> Result<()> {
    let Some(first) = pending.first() else {
        return Ok(());
    };
    let link = first.link.clone();
    debug_assert!(
        pending.iter().all(|run| run.link == link),
        "one measured group must share one link"
    );
    let measured = line_run::measure_inline(
        ctx,
        pending.iter().map(|run| (run.style, run.text.as_str())),
        link,
    )?;
    spans.push(Span::Group(groups.len()));
    groups.push(measured);
    pending.clear();
    Ok(())
}

fn blank_advance(width: BlankWidth, font_size: Length<Positive>) -> Result<Advance> {
    match width {
        BlankWidth::Fixed(length) => Ok(Advance::Fixed(length)),
        BlankWidth::Em(ems) => {
            let points = f32::from(ems) * font_size.as_pt();
            if !points.is_finite() || points < 0.0 {
                return Err(GeometryKind::AdvanceWidth { width: points }.into());
            }
            Ok(Advance::Fixed(Length::from_trusted_pt(points)))
        }
        BlankWidth::ToEndOfLine => Ok(Advance::Fill),
    }
}

/// Glyph ranges of the clusters between two cursors of one group.
fn seg_clusters(
    group: &MeasuredInline,
    start: ClusterCursor,
    end: ClusterCursor,
) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    let tape = group.text.clustered();
    (start.cluster..end.cluster)
        .map(move |index| tape.cluster(index).glyph_range())
        .filter(|range| !range.is_empty())
}

fn glyph_extent(
    ctx: &mut LayoutCtx<'_>,
    groups: &[MeasuredInline],
    line: &SpanLine<BlankDraw>,
) -> Result<Option<LineExtent>> {
    let mut extent: Option<LineExtent> = None;
    let mut seen = Vec::new();
    for seg in line.segs() {
        let Seg::Glyphs { group, start, end } = *seg else {
            continue;
        };
        let Some(group) = groups.get(group) else {
            continue;
        };
        for range in seg_clusters(group, start, end) {
            let Some(style) = group.style_of(range) else {
                continue;
            };
            let key = (style.font, style.font_size);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let face = line_run::face_extent(ctx, style.font, style.font_size)?;
            extent = Some(match extent {
                Some(folded) => folded.include(face),
                None => face.into(),
            });
        }
    }
    Ok(extent)
}

fn line_box(
    ctx: &mut LayoutCtx<'_>,
    groups: &[MeasuredInline],
    line: &SpanLine<BlankDraw>,
    style: &Style,
) -> Result<LineMetrics> {
    match glyph_extent(ctx, groups, line)? {
        Some(extent) => LineMetrics::new(extent, style.line_height_factor),
        None => match style.font {
            Some(face) => LineMetrics::new(
                line_run::face_extent(ctx, face, style.font_size)?.into(),
                style.line_height_factor,
            ),
            None => LineMetrics::fontless(style.font_size, style.line_height_factor),
        },
    }
}

fn place_line(
    groups: &[MeasuredInline],
    line: &SpanLine<BlankDraw>,
    style: &Style,
    frame_width: Length<Positive>,
    frame_x: Length<NonNegative>,
    baseline: f32,
    is_last_line: bool,
) -> Result<Vec<LinePiece>> {
    let mut parts = Vec::new();
    for seg in line.segs() {
        match *seg {
            Seg::Glyphs { group, start, end } => {
                if let Some(group_inline) = groups.get(group) {
                    for range in seg_clusters(group_inline, start, end) {
                        parts.push(WidthPart::Ink(group_inline.ink_width(range)?));
                    }
                }
            }
            Seg::Blank { width, .. } => parts.push(WidthPart::Blank(width)),
        }
    }
    let base: Vec<_> = line_break::draw_widths(&parts).collect();
    let (draws, allocated) = if style.align == TextAlign::Justify {
        let roles = justify::build_roles(groups, line.segs());
        debug_assert_eq!(
            roles.len(),
            base.len(),
            "every base width needs exactly one stretch role"
        );
        let plan = StretchPlan::for_line(StretchRequest {
            roles: &roles,
            frame_width,
            line_width: line.width(),
            is_last_line,
            ended_by_hard_break: line.ended_by_hard_break(),
            has_fill: line.has_fill(),
        });
        let allocated = plan.allocated();
        (plan.apply(&base), allocated)
    } else {
        (base, NonNegative::ZERO)
    };

    let line_x =
        frame_x.as_pt() + line_run::align_x(frame_width, line.width(), style.align).as_pt();
    let mut painter = LinePainter::new(line_x, baseline);
    let mut at = 0usize;
    for seg in line.segs() {
        match *seg {
            Seg::Glyphs { group, start, end } => {
                let Some(group_inline) = groups.get(group) else {
                    continue;
                };
                let clusters: Vec<_> = seg_clusters(group_inline, start, end).collect();
                let count = clusters.len();
                let draw_slice = draws.get(at..at + count).unwrap_or(&[]);
                painter.text(group_inline, &clusters, draw_slice)?;
                at += count;
            }
            Seg::Blank { payload, .. } => {
                let blank_draw = draws.get(at).copied().unwrap_or(NonNegative::ZERO);
                painter.blank(
                    blank_draw,
                    BlankStroke {
                        offset: payload.baseline_offset.as_pt(),
                        color: payload.color.unwrap_or(style.color),
                        thickness: payload.thickness,
                    },
                );
                at += 1;
            }
        }
    }
    let target_width = line.width().as_pt() + allocated.as_pt();
    debug_assert!(
        canvas::pt_near(painter.x() - line_x, target_width),
        "placed {} from {line_x}, painted width {target_width}",
        painter.x(),
    );
    painter.finish(style.font, style.font_size, style.color)
}
