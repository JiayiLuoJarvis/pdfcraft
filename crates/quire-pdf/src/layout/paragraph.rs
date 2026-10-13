//! Paragraph measurement, wrapping, and fragment drawing.

#![cfg_attr(not(test), allow(dead_code))]

use crate::canvas::{pt_near, GlyphPlacement, GlyphRun, LinkTarget};
use crate::error::{Error, ErrorBag, ErrorKind, Fallible as Result, FontFault, GeometryKind};
use crate::fonts::{FontId, FontStore};
use crate::geom::{Color, Length, NonNegative, Paint, Path, Point, Positive, Rect, Stroke};
use crate::layout::frame::{Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::style::{Style, TextAlign};
use crate::text::break_class::BreakClass;
use crate::text::cluster::{cluster_tagged, ClusteredText, TaggedClusteredText};
use crate::text::line_break::{break_spans, Advance, LineBudget, Seg, Span, SpanLine};
use crate::text::measure::{line_metrics, measure_text, LineMetrics, MeasuredText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Face {
    Body,
    Bold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextRun {
    pub(crate) text: String,
    pub(crate) face: Face,
    pub(crate) link: Option<LinkTarget>,
}

impl TextRun {
    pub(crate) fn new(text: impl Into<String>, face: Face) -> Self {
        Self {
            text: text.into(),
            face,
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

/// One laid-out line, coordinates relative to the paragraph fragment origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) pieces: Vec<LinePiece>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LinePiece {
    Glyphs(GlyphRun),
    Stroke(InlineStroke),
    Link {
        x0: f32,
        x1: f32,
        target: LinkTarget,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InlineStroke {
    x0: f32,
    x1: f32,
    y: f32,
    color: Color,
    thickness: Length<Positive>,
}

/// Unwrapped text. Wraps at the width of the first frame it is split against.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextNode {
    items: Vec<InlineItem>,
    style: Style,
}

impl TextNode {
    pub(crate) fn new(text: impl Into<String>, style: Style) -> Self {
        let text = text.into();
        let items = if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineItem::Text(TextRun::new(text, Face::Body))]
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
    pub(crate) line_height: f32,
    pub(crate) spacing_after: f32,
    width: f32,
}

impl ParagraphNode {
    pub(crate) fn layout(
        fonts: &mut FontStore,
        text: &str,
        style: &Style,
        width: f32,
    ) -> Result<Self> {
        let items = if text.is_empty() {
            Vec::new()
        } else {
            vec![InlineItem::Text(TextRun::new(text, Face::Body))]
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
        width: f32,
    ) -> Result<Self> {
        if !width.is_finite() || width <= 0.0 {
            return Err(GeometryKind::LineWidth { width }.into());
        }

        let indent = style.first_line_indent.as_pt();
        let first_width = width - indent;
        if !first_width.is_finite() || first_width <= 0.0 {
            return Err(GeometryKind::FirstLineIndent { indent, width }.into());
        }
        let font_size = style.font_size.as_pt();
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
            |group, cluster| {
                cluster_width_pt(&prepared.groups[group], cluster.glyph_range(), font_size)
            },
        )?;
        let metrics = line_metrics_or_soft(ctx, style)?;
        let line_height = metrics.line_height;

        let mut lines = Vec::with_capacity(broken.len());
        for (index, line) in broken.iter().enumerate() {
            let frame_width = if index == 0 { first_width } else { width };
            let frame_x = if index == 0 { indent } else { 0.0 };
            lines.push(place_line(
                &prepared.groups,
                line,
                style,
                frame_width,
                frame_x,
                metrics.baseline,
                font_size,
            )?);
        }

        Ok(Self {
            lines,
            line_height,
            spacing_after: style.paragraph_spacing.as_pt(),
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
        if self.width > avail.width {
            return Err(GeometryKind::LaidOutWiderThanFrame {
                width: self.width,
                avail: avail.width,
            }
            .into());
        }
        if self.lines.is_empty() {
            return Ok(Split::Fit {
                height: 0.0,
                fragment: Fragment::empty(),
            });
        }
        if self.line_height > avail.full_height {
            return Err(ErrorKind::UnsplittableTallerThanFrame {
                height: self.line_height,
                avail: avail.full_height,
            }
            .into());
        }
        if avail.height < self.line_height {
            return Ok(Split::MoveToNextPage(self));
        }

        let max_lines = lines_fitting(avail.height, self.line_height).min(self.lines.len());
        if max_lines == 0 {
            return Ok(Split::MoveToNextPage(self));
        }

        if max_lines >= self.lines.len() {
            let height = lines_height(self.lines.len(), self.line_height);
            let fragment = fragment_from_lines(&self.lines, self.line_height, self.spacing_after)?;
            return Ok(Split::Fit { height, fragment });
        }

        let mut lines = self.lines;
        let tail_lines = lines.split_off(max_lines);
        let height = lines_height(max_lines, self.line_height);
        let head = fragment_from_lines(&lines, self.line_height, 0.0)?;
        let tail = ParagraphNode {
            lines: tail_lines,
            line_height: self.line_height,
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

/// One unwrapped run of `text` at origin `(0, baseline)`.
pub(crate) fn single_line_run(
    ctx: &mut LayoutCtx<'_>,
    text: &str,
    style: &Style,
    max_width: f32,
) -> Result<GlyphRun> {
    let clustered = ClusteredText::new(text);
    if (0..clustered.len()).any(|index| clustered.cluster(index).class() == BreakClass::Hard) {
        return Err(Error::from(ErrorKind::UnexpectedLineBreakInSingleLine {
            text: crate::error::text_snippet(clustered.normalized_text()),
        }));
    }
    let measured = measure_text(ctx.fonts, ctx.errors, style.font, clustered.glyph_text())?;
    let width = measured.advance_norm * style.font_size.as_pt();
    if width > max_width {
        return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
            text: Some(crate::error::text_snippet(text)),
            width,
            avail: max_width,
        }));
    }
    let metrics = line_metrics_or_soft(ctx, style)?;
    let mut run = glyph_run_for_line(
        &measured,
        0..measured.glyphs.len(),
        0..measured.text.len(),
        style,
        width,
        max_width,
        metrics.baseline,
    )?;
    run.origin = Point::try_pt(0.0, metrics.baseline)?;
    Ok(run)
}

fn lines_fitting(avail_height: f32, line_height: f32) -> usize {
    if !avail_height.is_finite()
        || !line_height.is_finite()
        || line_height <= 0.0
        || avail_height < line_height
    {
        return 0;
    }
    let mut count: u16 = 1;
    while count < u16::MAX {
        let next = count + 1;
        if line_height * f32::from(next) <= avail_height {
            count = next;
        } else {
            break;
        }
    }
    usize::from(count)
}

fn lines_height(count: usize, line_height: f32) -> f32 {
    let count = u16::try_from(count).unwrap_or(u16::MAX);
    f32::from(count) * line_height
}

fn fragment_from_lines(
    lines: &[LineBox],
    line_height: f32,
    spacing_after: f32,
) -> Result<Fragment> {
    let mut commands = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let y0 = lines_height(index, line_height);
        for piece in &line.pieces {
            match piece {
                LinePiece::Glyphs(run) => {
                    let mut run = run.clone();
                    let y = y0 + run.origin.y().as_pt();
                    run.origin = Point::try_pt(run.origin.x().as_pt(), y)?;
                    commands.push(FragmentCommand::Glyphs(run));
                }
                LinePiece::Stroke(stroke) => {
                    let y = y0 + stroke.y;
                    commands.push(FragmentCommand::Stroke {
                        path: Path::line(
                            Point::try_pt(stroke.x0, y)?,
                            Point::try_pt(stroke.x1, y)?,
                        ),
                        paint: Paint::solid(stroke.color),
                        stroke: Stroke::new(stroke.thickness),
                    });
                }
                LinePiece::Link { x0, x1, target } => {
                    let width = x1 - x0;
                    if width > 0.0 {
                        commands.push(FragmentCommand::Link {
                            rect: Rect::try_pt(*x0, y0, width, line_height)?,
                            target: target.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(Fragment {
        height: lines_height(lines.len(), line_height),
        spacing_after,
        commands,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredInline {
    text: TaggedClusteredText<Face>,
    glyphs: Vec<InlineGlyph>,
    link: Option<LinkTarget>,
}

#[derive(Debug, Clone, PartialEq)]
struct InlineGlyph {
    font: FontId,
    glyph_id: u32,
    advance_norm: f32,
    byte_range: std::ops::Range<usize>,
    omit: bool,
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
                    flush_text(ctx, style.font, &mut pending, &mut groups, &mut spans)?;
                }
                pending.push(run.clone());
            }
            InlineItem::Blank(blank) => {
                flush_text(ctx, style.font, &mut pending, &mut groups, &mut spans)?;
                spans.push(Span::Blank {
                    advance: blank_advance(blank.width, style.font_size.as_pt())?,
                    payload: BlankDraw {
                        thickness: blank.thickness,
                        color: blank.color,
                        baseline_offset: blank.baseline_offset,
                    },
                });
            }
        }
    }
    flush_text(ctx, style.font, &mut pending, &mut groups, &mut spans)?;
    Ok(Prepared { groups, spans })
}

fn flush_text(
    ctx: &mut LayoutCtx<'_>,
    body: FontId,
    pending: &mut Vec<TextRun>,
    groups: &mut Vec<MeasuredInline>,
    spans: &mut Vec<Span<BlankDraw>>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let measured = measure_inline(ctx, pending, body)?;
    spans.push(Span::Group(groups.len()));
    groups.push(measured);
    pending.clear();
    Ok(())
}

fn blank_advance(width: BlankWidth, font_size: f32) -> Result<Advance> {
    match width {
        BlankWidth::Fixed(length) => Ok(Advance::Fixed(length.as_pt())),
        BlankWidth::Em(ems) => {
            let points = f32::from(ems) * font_size;
            if !points.is_finite() || points < 0.0 {
                return Err(GeometryKind::AdvanceWidth { width: points }.into());
            }
            Ok(Advance::Fixed(points))
        }
        BlankWidth::ToEndOfLine => Ok(Advance::Fill),
    }
}

fn place_line(
    groups: &[MeasuredInline],
    line: &SpanLine<BlankDraw>,
    style: &Style,
    frame_width: f32,
    frame_x: f32,
    baseline: f32,
    font_size: f32,
) -> Result<LineBox> {
    let line_x = frame_x + align_x(frame_width, line.width(), style.align);
    let mut x = line_x;
    let mut pieces = Vec::new();
    let mut saw_glyphs = false;
    for seg in line.segs() {
        match *seg {
            Seg::Glyphs { group, start, end } => {
                let range = start.glyph..end.glyph;
                if range.is_empty() {
                    continue;
                }
                let advance = cluster_width_pt(&groups[group], range.clone(), font_size);
                let runs = glyph_runs_for_line(&groups[group], range, style, x, baseline)?;
                pieces.extend(runs.into_iter().map(LinePiece::Glyphs));
                if advance > 0.0 {
                    if let Some(target) = groups[group].link.clone() {
                        pieces.push(LinePiece::Link {
                            x0: x,
                            x1: x + advance,
                            target,
                        });
                    }
                }
                x += advance;
                saw_glyphs = true;
            }
            Seg::Blank { width, payload } => {
                if width > 0.0 {
                    pieces.push(LinePiece::Stroke(InlineStroke {
                        x0: x,
                        x1: x + width,
                        y: baseline + payload.baseline_offset.as_pt(),
                        color: payload.color.unwrap_or(style.color),
                        thickness: payload.thickness,
                    }));
                }
                x += width;
            }
        }
    }
    if !saw_glyphs {
        pieces.insert(
            0,
            LinePiece::Glyphs(empty_glyph_run(style, line_x, baseline)?),
        );
    }
    debug_assert!(
        pt_near(x - line_x, line.width()),
        "placed {x} from {line_x}, line width {}",
        line.width()
    );
    Ok(LineBox { pieces })
}

fn empty_glyph_run(style: &Style, x: f32, baseline: f32) -> Result<GlyphRun> {
    Ok(GlyphRun {
        font: style.font,
        font_size: style.font_size,
        color: style.color,
        origin: Point::try_pt(x, baseline)?,
        glyphs: Vec::new(),
        text: String::new(),
    })
}

fn measure_inline(
    ctx: &mut LayoutCtx<'_>,
    runs: &[TextRun],
    body: FontId,
) -> Result<MeasuredInline> {
    let link = runs.first().and_then(|run| run.link.clone());
    debug_assert!(
        runs.iter().all(|run| run.link == link),
        "one measured group must share one link"
    );
    let text = cluster_tagged(runs.iter().map(|run| (run.face, run.text.as_str())));
    let needs_bold = runs
        .iter()
        .any(|run| run.face == Face::Bold && !run.text.is_empty());
    let bold = if needs_bold {
        if let Ok(id) = ctx.fonts.bold() {
            Some(id)
        } else {
            ctx.soft(FontFault::BoldFaceRequired);
            Some(body)
        }
    } else {
        None
    };
    let font_ids: Vec<_> = text
        .glyph_tags()
        .iter()
        .map(|face| match face {
            Face::Body => body,
            Face::Bold => bold.unwrap_or(body),
        })
        .collect();
    let glyph_text = text.clustered().glyph_text();
    let mut byte_boundaries: Vec<_> = glyph_text.char_indices().map(|(byte, _)| byte).collect();
    byte_boundaries.push(glyph_text.len());

    let mut glyphs = Vec::with_capacity(font_ids.len());
    let mut start = 0usize;
    while start < font_ids.len() {
        let font = font_ids[start];
        let mut end = start + 1;
        while end < font_ids.len() && font_ids[end] == font {
            end += 1;
        }
        let byte_start = byte_boundaries[start];
        let measured = measure_text(
            ctx.fonts,
            ctx.errors,
            font,
            &glyph_text[byte_start..byte_boundaries[end]],
        )?;
        debug_assert_eq!(measured.glyphs.len(), end - start);
        glyphs.extend(measured.glyphs.into_iter().map(|glyph| InlineGlyph {
            font,
            glyph_id: glyph.glyph_id,
            advance_norm: glyph.advance_norm,
            byte_range: (glyph.byte_range.start + byte_start)..(glyph.byte_range.end + byte_start),
            omit: glyph.omit,
        }));
        start = end;
    }

    Ok(MeasuredInline { text, glyphs, link })
}

fn line_metrics_or_soft(ctx: &mut LayoutCtx<'_>, style: &Style) -> Result<LineMetrics> {
    match ctx.fonts.metrics(style.font) {
        Ok(face) => line_metrics(face, style.font_size, style.line_height_factor),
        Err(error) => {
            if let ErrorKind::UnknownFont { id } = error.kind {
                ctx.soft(FontFault::UnknownFont { id });
                let size = style.font_size.as_pt();
                let factor = style.line_height_factor;
                if !factor.is_finite() || factor <= 0.0 {
                    return Err(GeometryKind::LineHeightFactor { factor }.into());
                }
                return Ok(LineMetrics {
                    ascent: size,
                    descent: 0.0,
                    leading: 0.0,
                    line_height: size * factor,
                    baseline: size,
                });
            }
            Err(error)
        }
    }
}

fn cluster_width_pt(
    measured: &MeasuredInline,
    glyph_range: std::ops::Range<usize>,
    font_size: f32,
) -> f32 {
    measured.glyphs[glyph_range]
        .iter()
        .map(|glyph| glyph.advance_norm * font_size)
        .sum()
}

fn glyph_runs_for_line(
    measured: &MeasuredInline,
    glyph_range: std::ops::Range<usize>,
    style: &Style,
    line_x: f32,
    baseline: f32,
) -> Result<Vec<GlyphRun>> {
    if glyph_range.is_empty() {
        return Ok(vec![GlyphRun {
            font: style.font,
            font_size: style.font_size,
            color: style.color,
            origin: Point::try_pt(line_x, baseline)?,
            glyphs: Vec::new(),
            text: String::new(),
        }]);
    }

    let mut runs = Vec::new();
    let mut x = line_x;
    let mut start = glyph_range.start;
    while start < glyph_range.end {
        let font = measured.glyphs[start].font;
        let mut end = start + 1;
        while end < glyph_range.end && measured.glyphs[end].font == font {
            end += 1;
        }
        let byte_start = measured.glyphs[start].byte_range.start;
        let byte_end = measured.glyphs[end - 1].byte_range.end;
        let glyphs: Vec<_> = measured.glyphs[start..end]
            .iter()
            .filter(|glyph| !glyph.omit)
            .map(|glyph| GlyphPlacement {
                glyph_id: glyph.glyph_id,
                advance_norm: glyph.advance_norm,
                byte_range: (glyph.byte_range.start - byte_start)
                    ..(glyph.byte_range.end - byte_start),
            })
            .collect();
        if glyphs.is_empty() {
            start = end;
            continue;
        }
        let advance = measured.glyphs[start..end]
            .iter()
            .map(|glyph| glyph.advance_norm * style.font_size.as_pt())
            .sum::<f32>();
        runs.push(GlyphRun {
            font,
            font_size: style.font_size,
            color: style.color,
            origin: Point::try_pt(x, baseline)?,
            glyphs,
            text: measured.text.clustered().glyph_text()[byte_start..byte_end].to_owned(),
        });
        x += advance;
        start = end;
    }
    Ok(runs)
}

fn glyph_run_for_line(
    measured: &MeasuredText,
    glyph_range: std::ops::Range<usize>,
    byte_range: std::ops::Range<usize>,
    style: &Style,
    line_width: f32,
    frame_width: f32,
    baseline: f32,
) -> Result<GlyphRun> {
    let text = measured.text[byte_range.clone()].to_owned();
    let byte_origin = byte_range.start;
    let glyphs = measured.glyphs[glyph_range]
        .iter()
        .filter(|glyph| !glyph.omit)
        .map(|glyph| GlyphPlacement {
            glyph_id: glyph.glyph_id,
            advance_norm: glyph.advance_norm,
            byte_range: (glyph.byte_range.start - byte_origin)
                ..(glyph.byte_range.end - byte_origin),
        })
        .collect();

    let x = align_x(frame_width, line_width, style.align);
    let y = baseline;
    Ok(GlyphRun {
        font: style.font,
        font_size: style.font_size,
        color: style.color,
        origin: Point::try_pt(x, y)?,
        glyphs,
        text,
    })
}

fn align_x(frame_width: f32, line_width: f32, align: TextAlign) -> f32 {
    match align {
        TextAlign::Start => 0.0,
        TextAlign::Center => (frame_width - line_width) / 2.0,
        TextAlign::End => frame_width - line_width,
    }
}
