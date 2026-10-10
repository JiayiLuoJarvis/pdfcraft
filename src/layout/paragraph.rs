//! Paragraph measurement, wrapping, and fragment drawing.

#![cfg_attr(not(test), allow(dead_code))]

use crate::canvas::{pt_near, GlyphPlacement, GlyphRun, LinkTarget};
use crate::error::{Error, ErrorKind, GeometryKind, Result};
use crate::fonts::{FontId, FontStore};
use crate::geom::{Color, Length, NonNegative, Paint, Path, Point, Positive, Rect, Stroke};
use crate::layout::flow::{FlowNode, Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::style::{Style, TextAlign};
use crate::text::cluster::{cluster_tagged, ClusteredText, TaggedClusteredText};
use crate::text::line_break::{break_spans, Advance, LineBudget, Seg, Span, SpanLine};
use crate::text::measure::{line_metrics, measure_text, MeasuredText};

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

impl InlineBlank {
    pub(crate) fn new(width: BlankWidth) -> Self {
        Self {
            width,
            thickness: Positive::pt(1.0).expect("1 pt is positive"),
            color: None,
            baseline_offset: NonNegative::pt(1.0).expect("1 pt is non-negative"),
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

    /// Every outcome hands back a laid-out `Paragraph`, so a page move never wraps twice.
    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> {
        ParagraphNode::layout_runs(ctx.fonts, &self.items, &self.style, avail.width)?.split(avail)
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
        Self::layout_runs(fonts, &items, style, width)
    }

    fn layout_runs(
        fonts: &mut FontStore,
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
        let prepared = prepare_inline(fonts, items, style)?;
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
        let face = fonts.metrics(style.font)?;
        let metrics = line_metrics(face, style.font_size, style.line_height_factor)?;
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

    /// Lines were wrapped at `self.width`; a narrower frame would overflow silently.
    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
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
            return Ok(Split::MoveToNextPage(FlowNode::Paragraph(self)));
        }

        let max_lines = lines_fitting(avail.height, self.line_height).min(self.lines.len());
        if max_lines == 0 {
            return Ok(Split::MoveToNextPage(FlowNode::Paragraph(self)));
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
            tail: FlowNode::Paragraph(tail),
        })
    }
}

/// One unwrapped run of `text` at origin `(0, baseline)`.
pub(crate) fn single_line_run(
    fonts: &mut FontStore,
    text: &str,
    style: &Style,
    max_width: f32,
) -> Result<GlyphRun> {
    let clustered = ClusteredText::new(text);
    let measured = measure_text(fonts, style.font, clustered.glyph_text())?;
    let width = measured.advance_norm * style.font_size.as_pt();
    if width > max_width {
        return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
            width,
            avail: max_width,
        }));
    }
    let face = fonts.metrics(style.font)?;
    let metrics = line_metrics(face, style.font_size, style.line_height_factor)?;
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

fn prepare_inline(fonts: &mut FontStore, items: &[InlineItem], style: &Style) -> Result<Prepared> {
    let mut groups = Vec::new();
    let mut spans = Vec::new();
    let mut pending: Vec<TextRun> = Vec::new();
    for item in items {
        match item {
            InlineItem::Text(run) => {
                if pending.first().is_some_and(|first| first.link != run.link) {
                    flush_text(fonts, style.font, &mut pending, &mut groups, &mut spans)?;
                }
                pending.push(run.clone());
            }
            InlineItem::Blank(blank) => {
                flush_text(fonts, style.font, &mut pending, &mut groups, &mut spans)?;
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
    flush_text(fonts, style.font, &mut pending, &mut groups, &mut spans)?;
    Ok(Prepared { groups, spans })
}

fn flush_text(
    fonts: &mut FontStore,
    body: FontId,
    pending: &mut Vec<TextRun>,
    groups: &mut Vec<MeasuredInline>,
    spans: &mut Vec<Span<BlankDraw>>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let measured = measure_inline(fonts, pending, body)?;
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

fn measure_inline(fonts: &mut FontStore, runs: &[TextRun], body: FontId) -> Result<MeasuredInline> {
    let link = runs.first().and_then(|run| run.link.clone());
    debug_assert!(
        runs.iter().all(|run| run.link == link),
        "one measured group must share one link"
    );
    let text = cluster_tagged(runs.iter().map(|run| (run.face, run.text.as_str())));
    let bold = if runs
        .iter()
        .any(|run| run.face == Face::Bold && !run.text.is_empty())
    {
        Some(fonts.bold()?)
    } else {
        None
    };
    let font_ids: Vec<_> = text
        .glyph_tags()
        .iter()
        .map(|face| match face {
            Face::Body => Ok(body),
            Face::Bold => bold.ok_or(ErrorKind::BoldFaceRequired.into()),
        })
        .collect::<Result<_>>()?;
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
        let measured = measure_text(fonts, font, &glyph_text[byte_start..byte_boundaries[end]])?;
        debug_assert_eq!(measured.glyphs.len(), end - start);
        glyphs.extend(measured.glyphs.into_iter().map(|glyph| InlineGlyph {
            font,
            glyph_id: glyph.glyph_id,
            advance_norm: glyph.advance_norm,
            byte_range: (glyph.byte_range.start + byte_start)..(glyph.byte_range.end + byte_start),
        }));
        start = end;
    }

    Ok(MeasuredInline { text, glyphs, link })
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
        let glyphs = measured.glyphs[start..end]
            .iter()
            .map(|glyph| GlyphPlacement {
                glyph_id: glyph.glyph_id,
                advance_norm: glyph.advance_norm,
                byte_range: (glyph.byte_range.start - byte_start)
                    ..(glyph.byte_range.end - byte_start),
            })
            .collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{CanvasCommand, LinkTarget, RecordingCanvas};
    use crate::fonts::{fixture_bytes, FontSource};
    use crate::geom::NonNegative;
    use crate::layout::flow::{draw_fragment, Frame};
    use crate::layout::style::TextAlign;
    use crate::layout::test_util::fonts_and_style;
    use crate::text::measure::line_metrics;

    fn uri(text: &str) -> LinkTarget {
        LinkTarget::uri(text).unwrap()
    }

    fn text_item(text: &str, face: Face, link: Option<&str>) -> InlineItem {
        InlineItem::Text(TextRun::new(text, face).with_link(link.map(uri)))
    }

    fn fit(
        items: Vec<InlineItem>,
        style: &Style,
        fonts: &mut FontStore,
        width: f32,
        height: f32,
    ) -> Fragment {
        let node = TextNode::from_items(items, style.clone());
        let mut ctx = LayoutCtx { fonts };
        let Split::Fit { fragment, .. } = node
            .split(
                &mut ctx,
                Frame {
                    width,
                    height,
                    full_height: height,
                },
            )
            .unwrap()
        else {
            panic!("paragraph fits");
        };
        fragment
    }

    struct Hit {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        target: String,
    }

    fn hits(fragment: &Fragment) -> Vec<Hit> {
        fragment
            .commands
            .iter()
            .filter_map(|command| match command {
                FragmentCommand::Link { rect, target } => Some(Hit {
                    x: rect.origin().x().as_pt(),
                    y: rect.origin().y().as_pt(),
                    width: rect.size().width().as_pt(),
                    height: rect.size().height().as_pt(),
                    target: target.as_str().to_owned(),
                }),
                _ => None,
            })
            .collect()
    }

    fn glyph_spans(fragment: &Fragment) -> Vec<(String, f32, f32)> {
        fragment
            .commands
            .iter()
            .filter_map(|command| match command {
                FragmentCommand::Glyphs(run) => {
                    let width = run
                        .glyphs
                        .iter()
                        .map(|glyph| glyph.advance_norm * run.font_size.as_pt())
                        .sum();
                    Some((run.text.clone(), run.origin.x().as_pt(), width))
                }
                _ => None,
            })
            .collect()
    }

    fn body_line(fonts: &FontStore, style: &Style) -> f32 {
        let face = fonts.metrics(style.font).unwrap();
        line_metrics(face, style.font_size, style.line_height_factor)
            .unwrap()
            .line_height
    }

    #[test]
    fn centered_fill_blank_starts_at_the_frame_x() {
        let (mut fonts, mut style) = fonts_and_style();
        style.align = TextAlign::Center;
        let node = TextNode::from_items(
            vec![
                InlineItem::Text(TextRun::new("A", Face::Body)),
                InlineItem::Blank(InlineBlank::new(BlankWidth::ToEndOfLine)),
            ],
            style,
        );
        let mut ctx = LayoutCtx { fonts: &mut fonts };
        let Split::Fit { fragment, .. } = node
            .split(
                &mut ctx,
                Frame {
                    width: 120.0,
                    height: 400.0,
                    full_height: 400.0,
                },
            )
            .unwrap()
        else {
            panic!("paragraph fits");
        };
        let glyph = fragment.commands.iter().find_map(|command| match command {
            FragmentCommand::Glyphs(run) if run.text == "A" => Some(run.origin.x().as_pt()),
            _ => None,
        });
        assert_eq!(glyph.map(f32::to_bits), Some(0.0f32.to_bits()));
        let end = fragment.commands.iter().find_map(|command| match command {
            FragmentCommand::Stroke { path, .. } => path.verbs().last().map(|verb| match verb {
                crate::geom::PathVerb::LineTo(point) => point.x().as_pt(),
                _ => f32::NAN,
            }),
            _ => None,
        });
        assert_eq!(end.map(f32::to_bits), Some(120.0f32.to_bits()));
    }

    #[test]
    fn partial_link_rect_covers_only_the_linked_glyphs() {
        let (mut fonts, style) = fonts_and_style();
        let line = body_line(&fonts, &style);
        let fragment = fit(
            vec![
                text_item("A", Face::Body, None),
                text_item("BC", Face::Body, Some("https://example.com/bc")),
                text_item("D", Face::Body, None),
            ],
            &style,
            &mut fonts,
            200.0,
            80.0,
        );
        let glyphs = glyph_spans(&fragment);
        assert_eq!(
            glyphs
                .iter()
                .map(|(text, _, _)| text.as_str())
                .collect::<Vec<_>>(),
            ["A", "BC", "D"]
        );
        let links = hits(&fragment);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "https://example.com/bc");
        assert!(pt_near(links[0].x, glyphs[1].1));
        assert!(pt_near(links[0].width, glyphs[1].2));
        assert!(pt_near(glyphs[1].1, glyphs[0].2));
        assert_eq!(links[0].y.to_bits(), 0.0f32.to_bits());
        assert_eq!(links[0].height.to_bits(), line.to_bits());
    }

    #[test]
    fn wrapped_link_emits_one_rect_per_visual_line() {
        let (mut fonts, style) = fonts_and_style();
        let line = body_line(&fonts, &style);
        let fragment = fit(
            vec![text_item(
                "第第",
                Face::Body,
                Some("https://example.com/wrap"),
            )],
            &style,
            &mut fonts,
            12.0,
            80.0,
        );
        let links = hits(&fragment);
        assert_eq!(links.len(), 2);
        assert!(links
            .iter()
            .all(|hit| hit.target == "https://example.com/wrap"));
        assert_eq!(links[0].x.to_bits(), 0.0f32.to_bits());
        assert_eq!(links[0].y.to_bits(), 0.0f32.to_bits());
        assert_eq!(links[0].width.to_bits(), 12.0f32.to_bits());
        assert_eq!(links[0].height.to_bits(), line.to_bits());
        assert_eq!(links[1].x.to_bits(), 0.0f32.to_bits());
        assert_eq!(links[1].y.to_bits(), line.to_bits());
        assert_eq!(links[1].width.to_bits(), 12.0f32.to_bits());
        assert_eq!(links[1].height.to_bits(), line.to_bits());
    }

    #[test]
    fn same_link_keeps_body_and_bold_in_one_rect() {
        let (mut fonts, style) = fonts_and_style();
        let bold = fonts
            .register(FontSource {
                bytes: fixture_bytes(),
                index: 0,
            })
            .unwrap();
        fonts.set_bold(bold);
        let fragment = fit(
            vec![
                text_item("A", Face::Body, Some("u")),
                text_item("B", Face::Bold, Some("u")),
                text_item("C", Face::Body, Some("u")),
            ],
            &style,
            &mut fonts,
            200.0,
            80.0,
        );
        let glyphs = glyph_spans(&fragment);
        assert_eq!(glyphs.len(), 3);
        let links = hits(&fragment);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "u");
        assert_eq!(links[0].x.to_bits(), 0.0f32.to_bits());
        assert!(pt_near(
            links[0].width,
            glyphs.iter().map(|(_, _, width)| width).sum()
        ));
    }

    #[test]
    fn link_changes_flush_and_a_repeat_uri_stays_its_own_rect() {
        let (mut fonts, style) = fonts_and_style();
        let fragment = fit(
            vec![
                text_item("A", Face::Body, Some("a")),
                text_item("B", Face::Body, Some("b")),
                text_item("C", Face::Body, None),
                text_item("D", Face::Body, Some("a")),
            ],
            &style,
            &mut fonts,
            200.0,
            80.0,
        );
        let glyphs = glyph_spans(&fragment);
        let links = hits(&fragment);
        assert_eq!(
            links
                .iter()
                .map(|hit| hit.target.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "a"]
        );
        assert!(pt_near(links[0].x, glyphs[0].1));
        assert!(pt_near(links[0].width, glyphs[0].2));
        assert!(pt_near(links[1].x, glyphs[1].1));
        assert!(pt_near(links[1].width, glyphs[1].2));
        assert!(pt_near(links[2].x, glyphs[3].1));
        assert!(pt_near(links[2].width, glyphs[3].2));
        assert!(links[0].x + links[0].width <= links[1].x + f32::EPSILON);
        assert!(links[1].x + links[1].width < links[2].x);
    }

    #[test]
    fn adjacent_runs_with_the_same_link_share_one_rect() {
        let (mut fonts, style) = fonts_and_style();
        let fragment = fit(
            vec![
                text_item("A", Face::Body, Some("u")),
                text_item("B", Face::Body, Some("u")),
            ],
            &style,
            &mut fonts,
            200.0,
            80.0,
        );
        let glyphs = glyph_spans(&fragment);
        let links = hits(&fragment);
        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].0, "AB");
        assert_eq!(links.len(), 1);
        assert!(pt_near(links[0].width, glyphs[0].2));
    }

    #[test]
    fn blank_splits_a_link_and_is_not_inside_either_rect() {
        let (mut fonts, style) = fonts_and_style();
        let fragment = fit(
            vec![
                text_item("A", Face::Body, Some("u")),
                InlineItem::Blank(InlineBlank::new(BlankWidth::Fixed(
                    NonNegative::pt(30.0).unwrap(),
                ))),
                text_item("B", Face::Body, Some("u")),
            ],
            &style,
            &mut fonts,
            200.0,
            80.0,
        );
        let links = hits(&fragment);
        assert_eq!(links.len(), 2);
        assert!(fragment
            .commands
            .iter()
            .any(|command| matches!(command, FragmentCommand::Stroke { .. })));
        assert!(pt_near(links[1].x - (links[0].x + links[0].width), 30.0));
    }

    #[test]
    fn link_rect_follows_alignment_and_first_line_indent() {
        let (mut fonts, mut style) = fonts_and_style();
        style.align = TextAlign::Center;
        let centered = fit(
            vec![text_item("A", Face::Body, Some("u"))],
            &style,
            &mut fonts,
            120.0,
            80.0,
        );
        let center_glyphs = glyph_spans(&centered);
        let center_links = hits(&centered);
        assert!(pt_near(center_links[0].x, center_glyphs[0].1));
        assert!(center_links[0].x > 0.0);

        style.align = TextAlign::End;
        let ended = fit(
            vec![text_item("A", Face::Body, Some("u"))],
            &style,
            &mut fonts,
            120.0,
            80.0,
        );
        let end_glyphs = glyph_spans(&ended);
        let end_links = hits(&ended);
        assert!(pt_near(end_links[0].x, end_glyphs[0].1));
        assert!(pt_near(end_links[0].x + end_links[0].width, 120.0));

        style.align = TextAlign::Start;
        style.first_line_indent = NonNegative::pt(10.0).unwrap();
        let indented = fit(
            vec![text_item("A", Face::Body, Some("u"))],
            &style,
            &mut fonts,
            120.0,
            80.0,
        );
        let indent_links = hits(&indented);
        assert!(pt_near(indent_links[0].x, 10.0));
        assert!(pt_near(indent_links[0].x, glyph_spans(&indented)[0].1));
    }

    #[test]
    fn offset_and_paint_move_the_link_rect() {
        let (mut fonts, style) = fonts_and_style();
        let fragment = fit(
            vec![text_item("A", Face::Body, Some("https://example.com/a"))],
            &style,
            &mut fonts,
            80.0,
            40.0,
        );
        let local = &hits(&fragment)[0];
        let shifted = fragment.clone().with_offset(10.0, 20.0).unwrap();
        let moved = &hits(&shifted)[0];
        assert!(pt_near(moved.x, local.x + 10.0));
        assert!(pt_near(moved.y, local.y + 20.0));
        assert_eq!(moved.target, local.target);

        let mut rec = RecordingCanvas::new();
        draw_fragment(&fragment, &mut rec, Point::try_pt(3.0, 4.0).unwrap()).unwrap();
        let painted: Vec<_> = rec
            .commands()
            .iter()
            .filter_map(|command| match command {
                CanvasCommand::AnnotateLink { rect, target } => Some((
                    rect.origin().x().as_pt(),
                    rect.origin().y().as_pt(),
                    target.as_str().to_owned(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(painted.len(), 1);
        assert!(pt_near(painted[0].0, local.x + 3.0));
        assert!(pt_near(painted[0].1, local.y + 4.0));
        assert_eq!(painted[0].2, "https://example.com/a");
    }

    #[test]
    fn empty_linked_text_emits_no_rect() {
        let (mut fonts, style) = fonts_and_style();
        let fragment = fit(
            vec![text_item("", Face::Body, Some("u"))],
            &style,
            &mut fonts,
            80.0,
            40.0,
        );
        assert!(hits(&fragment).is_empty());
    }
}
