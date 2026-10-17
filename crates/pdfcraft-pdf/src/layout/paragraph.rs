//! Paragraph wrapping, per-line boxes, and fragment drawing.

use crate::canvas::{pt_near, LinkTarget};
#[cfg(test)]
use crate::error::ErrorBag;
use crate::error::{ErrorKind, Fallible as Result, GeometryKind};
#[cfg(test)]
use crate::fonts::FontStore;
use crate::geom::{Color, Length, NonNegative, Positive};
use crate::layout::frame::{Fragment, Frame, LayoutCtx, Split};
use crate::layout::line_run::{
    align_x, append_commands, face_extent, measure_inline, BlankStroke, LinePainter, LinePiece,
    MeasuredInline,
};
use crate::layout::style::{GlyphStyle, Style};
use crate::text::cluster::{ClusterCursor, ClusteredText};
use crate::text::line_break::{
    break_spans, draw_widths, Advance, LineBudget, Seg, Span, SpanLine, WidthPart,
};
use crate::text::measure::{LineExtent, LineMetrics};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextRun {
    pub(crate) text: String,
    pub(crate) style: GlyphStyle,
    pub(crate) link: Option<LinkTarget>,
}

impl TextRun {
    pub(crate) fn new(text: impl Into<String>, style: GlyphStyle) -> Self {
        Self {
            text: text.into(),
            style,
            link: None,
        }
    }

    pub(crate) fn with_link(mut self, link: Option<LinkTarget>) -> Self {
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
            thickness: Positive::pt(DEFAULT_BLANK_THICKNESS_PT)
                .expect("default blank thickness is positive"),
            color: None,
            baseline_offset: NonNegative::pt(DEFAULT_BLANK_BASELINE_OFFSET_PT)
                .expect("default blank baseline offset is non-negative"),
        }
    }
}

/// One laid-out line. Pieces are relative to the line-box top-left.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) pieces: Vec<LinePiece>,
    pub(crate) height: Length<Positive>,
}

/// Unwrapped text. Wraps at the width of the first frame it is split against.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextNode {
    items: Vec<InlineItem>,
    style: Style,
}

impl TextNode {
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
        let broken = break_spans(
            &group_refs,
            &prepared.spans,
            LineBudget {
                first: first_width,
                rest: width,
            },
            |group, cluster| prepared.groups[group].ink_width(cluster.glyph_range()),
        )?;

        let mut lines = Vec::with_capacity(broken.len());
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
            )?;
            lines.push(LineBox {
                pieces,
                height: metrics.line_height,
            });
        }

        Ok(Self {
            lines,
            spacing_after: style.paragraph_spacing,
            width,
        })
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

        let tops = line_tops(&self.lines);
        let max_lines = tops[1..]
            .iter()
            .take_while(|&&bottom| bottom <= avail.height.as_pt())
            .count();
        if max_lines == 0 {
            return Ok(Split::MoveToNextPage(self));
        }

        if max_lines >= self.lines.len() {
            let height = Length::from_trusted_pt(tops[self.lines.len()]);
            let fragment = fragment_from_lines(&self.lines, self.spacing_after)?;
            return Ok(Split::Fit { height, fragment });
        }

        let mut lines = self.lines;
        let tail_lines = lines.split_off(max_lines);
        let height = Length::from_trusted_pt(tops[max_lines]);
        let head = fragment_from_lines(&lines, NonNegative::ZERO)?;
        let tail = ParagraphNode {
            lines: tail_lines,
            spacing_after: self.spacing_after,
            width: self.width,
        };
        Ok(Split::Partial {
            head_height: height,
            head,
            tail,
        })
    }
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
                base = tops[tops.len() - 1];
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
        append_commands(&mut commands, &line.pieces, 0.0, *top, line.height.as_pt())?;
    }
    Ok(Fragment {
        height: Length::from_trusted_pt(tops[lines.len()]),
        spacing_after,
        commands,
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
    let measured = measure_inline(
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
        let group = &groups[group];
        for range in seg_clusters(group, start, end) {
            let Some(style) = group.style_of(range) else {
                continue;
            };
            let key = (style.font, style.font_size);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let face = face_extent(ctx, style.font, style.font_size)?;
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
                face_extent(ctx, face, style.font_size)?.into(),
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
) -> Result<Vec<LinePiece>> {
    let mut parts = Vec::new();
    for seg in line.segs() {
        match *seg {
            Seg::Glyphs { group, start, end } => {
                for range in seg_clusters(&groups[group], start, end) {
                    parts.push(WidthPart::Ink(groups[group].ink_width(range)?));
                }
            }
            Seg::Blank { width, .. } => parts.push(WidthPart::Blank(width)),
        }
    }
    let draws: Vec<_> = draw_widths(&parts).collect();

    let line_x = frame_x.as_pt() + align_x(frame_width, line.width(), style.align).as_pt();
    let mut painter = LinePainter::new(line_x, baseline);
    let mut at = 0usize;
    for seg in line.segs() {
        match *seg {
            Seg::Glyphs { group, start, end } => {
                let clusters: Vec<_> = seg_clusters(&groups[group], start, end).collect();
                let count = clusters.len();
                painter.text(&groups[group], &clusters, &draws[at..at + count])?;
                at += count;
            }
            Seg::Blank { width, payload } => {
                painter.blank(
                    width,
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
    debug_assert!(
        pt_near(painter.x() - line_x, line.width().as_pt()),
        "placed {} from {line_x}, line width {}",
        painter.x(),
        line.width().as_pt()
    );
    painter.finish(style.font, style.font_size, style.color)
}
