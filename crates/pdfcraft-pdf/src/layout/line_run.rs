//! Measured inline text and the placement of one line of it, shared by paragraphs and unwrapped text.

use std::ops::Range;

use crate::canvas::{GlyphPlacement, GlyphRun, LinkTarget};
use crate::error::{text_snippet, Error, ErrorKind, Fallible as Result, FontFault};
use crate::fonts::FontId;
use crate::geom::{Color, Finite, Length, NonNegative, Paint, Path, Point, Positive, Rect, Stroke};
use crate::layout::frame::{FragmentCommand, LayoutCtx};
use crate::layout::style::{Decoration, GlyphStyle, TextAlign};
use crate::text::break_class::BreakClass;
use crate::text::cluster::{cluster_tagged, ClusteredText, TaggedClusteredText};
use crate::text::line_break::{draw_widths, InkWidth, WidthPart};
use crate::text::measure::{measure_text, FaceExtent, LineMetrics};

/// Text of one link target, measured glyph by glyph.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredInline {
    pub(crate) text: TaggedClusteredText<GlyphStyle>,
    glyphs: Vec<InlineGlyph>,
    pub(crate) link: Option<LinkTarget>,
}

#[derive(Debug, Clone, PartialEq)]
struct InlineGlyph {
    style: GlyphStyle,
    glyph_id: u32,
    advance_norm: f32,
    byte_range: Range<usize>,
    omit: bool,
}

/// Measures `runs` as one cluster tape. A cluster that spans runs takes its first scalar's style.
pub(crate) fn measure_inline<'a>(
    ctx: &mut LayoutCtx<'_>,
    runs: impl IntoIterator<Item = (GlyphStyle, &'a str)>,
    link: Option<LinkTarget>,
) -> Result<MeasuredInline> {
    let text = cluster_tagged(runs);
    let styles = cluster_styles(&text);
    let glyph_text = text.clustered().glyph_text();
    let mut byte_boundaries: Vec<_> = glyph_text.char_indices().map(|(byte, _)| byte).collect();
    byte_boundaries.push(glyph_text.len());

    let mut glyphs = Vec::with_capacity(styles.len());
    let mut start = 0usize;
    while start < styles.len() {
        let font = styles[start].font;
        let mut end = start + 1;
        while end < styles.len() && styles[end].font == font {
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
        glyphs.extend(measured.glyphs.into_iter().zip(&styles[start..end]).map(
            |(glyph, style)| InlineGlyph {
                style: *style,
                glyph_id: glyph.glyph_id,
                advance_norm: glyph.advance_norm,
                byte_range: (glyph.byte_range.start + byte_start)
                    ..(glyph.byte_range.end + byte_start),
                omit: glyph.omit,
            },
        ));
        start = end;
    }

    Ok(MeasuredInline { text, glyphs, link })
}

fn cluster_styles(text: &TaggedClusteredText<GlyphStyle>) -> Vec<GlyphStyle> {
    let tags = text.glyph_tags();
    let clustered = text.clustered();
    let mut styles = Vec::with_capacity(tags.len());
    for index in 0..clustered.len() {
        let range = clustered.cluster(index).glyph_range();
        if !range.is_empty() {
            styles.extend(std::iter::repeat_n(tags[range.start], range.len()));
        }
    }
    styles
}

impl MeasuredInline {
    /// Style of the cluster whose glyphs are `glyphs`.
    pub(crate) fn style_of(&self, glyphs: Range<usize>) -> Option<&GlyphStyle> {
        self.glyphs.get(glyphs.start).map(|glyph| &glyph.style)
    }

    /// Width of one cluster. Its last drawn glyph receives the tracking gap.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NegativeTracking`] when that glyph's advance plus tracking is below zero.
    pub(crate) fn ink_width(&self, glyphs: Range<usize>) -> Result<InkWidth> {
        let Some(style) = self.style_of(glyphs.clone()).copied() else {
            return Ok(InkWidth {
                body: NonNegative::ZERO,
                gap: Finite::ZERO,
            });
        };
        let size = style.font_size.as_pt();
        let cluster = &self.glyphs[glyphs];
        let body: f32 = cluster.iter().map(|glyph| glyph.advance_norm * size).sum();
        let gap = match cluster.iter().rev().find(|glyph| !glyph.omit) {
            Some(receiver) => {
                let advance = receiver.advance_norm * size;
                let tracking = style.tracking.as_pt();
                if advance + tracking < 0.0 {
                    return Err(Error::from(ErrorKind::NegativeTracking {
                        advance,
                        tracking,
                    }));
                }
                style.tracking
            }
            None => Finite::ZERO,
        };
        Ok(InkWidth {
            body: Length::from_trusted_pt(body),
            gap,
        })
    }
}

/// One drawn piece of a line. Coordinates are relative to the line's own origin.
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

/// A blank's own stroke. `offset` is below the baseline.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlankStroke {
    pub(crate) offset: f32,
    pub(crate) color: Color,
    pub(crate) thickness: Length<Positive>,
}

#[derive(Debug, Clone, Copy)]
struct OpenStroke {
    x0: f32,
    x1: f32,
    font_size: Length<Positive>,
    color: Color,
    decoration: Decoration,
}

/// Paints one line left to right. Underline and strikethrough segments with the
/// same font size and color merge into one stroke placed by the first segment.
pub(crate) struct LinePainter {
    start_x: f32,
    x: f32,
    baseline: f32,
    pieces: Vec<LinePiece>,
    decorations: Vec<LinePiece>,
    underline: Option<OpenStroke>,
    strikethrough: Option<OpenStroke>,
    saw_text: bool,
}

impl LinePainter {
    pub(crate) fn new(x: f32, baseline: f32) -> Self {
        Self {
            start_x: x,
            x,
            baseline,
            pieces: Vec::new(),
            decorations: Vec::new(),
            underline: None,
            strikethrough: None,
            saw_text: false,
        }
    }

    pub(crate) fn x(&self) -> f32 {
        self.x
    }

    /// Paints consecutive clusters of `measured`. `clusters[i]` is drawn `draws[i]` wide,
    /// and any width beyond its glyphs goes to its last drawn glyph.
    pub(crate) fn text(
        &mut self,
        measured: &MeasuredInline,
        clusters: &[Range<usize>],
        draws: &[Length<NonNegative>],
    ) -> Result<()> {
        debug_assert_eq!(clusters.len(), draws.len());
        let seg_x = self.x;
        let mut run: Option<RunBuilder> = None;
        let mut cluster_x = seg_x;
        for (glyphs, draw) in clusters.iter().zip(draws) {
            let Some(style) = measured.style_of(glyphs.clone()).copied() else {
                continue;
            };
            self.saw_text = true;
            let size = style.font_size.as_pt();
            let cluster = &measured.glyphs[glyphs.clone()];
            let body: f32 = cluster.iter().map(|glyph| glyph.advance_norm * size).sum();
            let extra_norm = (draw.as_pt() - body) / size;
            if run
                .as_ref()
                .is_some_and(|open| !open.accepts(&style, glyphs.start))
            {
                self.flush(run.take(), measured)?;
            }
            let open = run.get_or_insert_with(|| RunBuilder::new(&style, glyphs.start, cluster_x));
            let receiver = cluster.iter().rposition(|glyph| !glyph.omit);
            for (offset, glyph) in cluster.iter().enumerate() {
                if glyph.omit {
                    continue;
                }
                let mut advance_norm = glyph.advance_norm;
                if Some(offset) == receiver && extra_norm != 0.0 {
                    advance_norm += extra_norm;
                }
                open.glyphs.push((glyphs.start + offset, advance_norm));
            }
            open.end = glyphs.end;
            let next_x = cluster_x + draw.as_pt();
            let baseline = self.baseline;
            extend(
                &mut self.underline,
                &mut self.decorations,
                baseline,
                style.underline,
                &style,
                cluster_x..next_x,
            );
            extend(
                &mut self.strikethrough,
                &mut self.decorations,
                baseline,
                style.strikethrough,
                &style,
                cluster_x..next_x,
            );
            cluster_x = next_x;
        }
        self.flush(run, measured)?;
        let advance = draws.iter().copied().sum::<Length<NonNegative>>().as_pt();
        if advance > 0.0 {
            if let Some(target) = measured.link.clone() {
                self.pieces.push(LinePiece::Link {
                    x0: seg_x,
                    x1: seg_x + advance,
                    target,
                });
            }
        }
        self.x = seg_x + advance;
        Ok(())
    }

    /// A blank closes open decorations and draws only its own stroke.
    pub(crate) fn blank(&mut self, width: Length<NonNegative>, stroke: BlankStroke) {
        self.close_decorations();
        let width = width.as_pt();
        if width > 0.0 {
            self.pieces.push(LinePiece::Stroke(InlineStroke {
                x0: self.x,
                x1: self.x + width,
                y: self.baseline + stroke.offset,
                color: stroke.color,
                thickness: stroke.thickness,
            }));
        }
        self.x += width;
    }

    /// Ends the line. A line without glyphs records an empty glyph run only when `font` is set.
    pub(crate) fn finish(
        mut self,
        font: Option<FontId>,
        font_size: Length<Positive>,
        color: Color,
    ) -> Result<Vec<LinePiece>> {
        self.close_decorations();
        if let (false, Some(font)) = (self.saw_text, font) {
            self.pieces.insert(
                0,
                LinePiece::Glyphs(GlyphRun {
                    font,
                    font_size,
                    color,
                    origin: Point::try_pt(self.start_x, self.baseline)?,
                    glyphs: Vec::new(),
                    text: String::new(),
                }),
            );
        }
        self.pieces.append(&mut self.decorations);
        Ok(self.pieces)
    }

    fn close_decorations(&mut self) {
        close(&mut self.underline, &mut self.decorations, self.baseline);
        close(
            &mut self.strikethrough,
            &mut self.decorations,
            self.baseline,
        );
    }

    fn flush(&mut self, run: Option<RunBuilder>, measured: &MeasuredInline) -> Result<()> {
        let Some(run) = run else {
            return Ok(());
        };
        if run.glyphs.is_empty() {
            return Ok(());
        }
        let byte_start = measured.glyphs[run.start].byte_range.start;
        let byte_end = measured.glyphs[run.end - 1].byte_range.end;
        let glyphs = run
            .glyphs
            .iter()
            .map(|&(index, advance_norm)| {
                let glyph = &measured.glyphs[index];
                GlyphPlacement {
                    glyph_id: glyph.glyph_id,
                    advance_norm,
                    byte_range: (glyph.byte_range.start - byte_start)
                        ..(glyph.byte_range.end - byte_start),
                }
            })
            .collect();
        self.pieces.push(LinePiece::Glyphs(GlyphRun {
            font: run.font,
            font_size: run.font_size,
            color: run.color,
            origin: Point::try_pt(run.x, self.baseline)?,
            glyphs,
            text: measured.text.clustered().glyph_text()[byte_start..byte_end].to_owned(),
        }));
        Ok(())
    }
}

/// Glyphs that share one font, size, and color, so they fit one [`GlyphRun`].
struct RunBuilder {
    font: FontId,
    font_size: Length<Positive>,
    color: Color,
    x: f32,
    start: usize,
    end: usize,
    glyphs: Vec<(usize, f32)>,
}

impl RunBuilder {
    fn new(style: &GlyphStyle, start: usize, x: f32) -> Self {
        Self {
            font: style.font,
            font_size: style.font_size,
            color: style.color,
            x,
            start,
            end: start,
            glyphs: Vec::new(),
        }
    }

    fn accepts(&self, style: &GlyphStyle, start: usize) -> bool {
        self.font == style.font
            && self.font_size == style.font_size
            && self.color == style.color
            && self.end == start
    }
}

fn extend(
    open: &mut Option<OpenStroke>,
    out: &mut Vec<LinePiece>,
    baseline: f32,
    decoration: Option<Decoration>,
    style: &GlyphStyle,
    span: Range<f32>,
) {
    match (decoration, open.as_mut()) {
        (Some(_), Some(line)) if line.font_size == style.font_size && line.color == style.color => {
            line.x1 = span.end;
        }
        (Some(decoration), _) => {
            close(open, out, baseline);
            *open = Some(OpenStroke {
                x0: span.start,
                x1: span.end,
                font_size: style.font_size,
                color: style.color,
                decoration,
            });
        }
        (None, _) => close(open, out, baseline),
    }
}

fn close(open: &mut Option<OpenStroke>, out: &mut Vec<LinePiece>, baseline: f32) {
    if let Some(line) = open.take() {
        if line.x1 > line.x0 {
            out.push(LinePiece::Stroke(InlineStroke {
                x0: line.x0,
                x1: line.x1,
                y: baseline + line.decoration.centerline().as_pt(),
                color: line.color,
                thickness: Decoration::THICKNESS,
            }));
        }
    }
}

/// Writes `pieces` as fragment commands, moved by `(dx, dy)`. Link boxes start at
/// `dy` and are `link_height` tall.
pub(crate) fn append_commands(
    commands: &mut Vec<FragmentCommand>,
    pieces: &[LinePiece],
    dx: f32,
    dy: f32,
    link_height: f32,
) -> Result<()> {
    for piece in pieces {
        match piece {
            LinePiece::Glyphs(run) => {
                let mut run = run.clone();
                run.origin =
                    Point::try_pt(dx + run.origin.x().as_pt(), dy + run.origin.y().as_pt())?;
                commands.push(FragmentCommand::Glyphs(run));
            }
            LinePiece::Stroke(stroke) => {
                let y = dy + stroke.y;
                commands.push(FragmentCommand::Stroke {
                    path: Path::line(
                        Point::try_pt(dx + stroke.x0, y)?,
                        Point::try_pt(dx + stroke.x1, y)?,
                    ),
                    paint: Paint::solid(stroke.color),
                    stroke: Stroke::new(stroke.thickness),
                });
            }
            LinePiece::Link { x0, x1, target } => {
                let width = x1 - x0;
                if width > 0.0 {
                    commands.push(FragmentCommand::Link {
                        rect: Rect::try_pt(dx + x0, dy, width, link_height)?,
                        target: target.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// One unwrapped line and its ascent.
pub(crate) struct SingleLine {
    /// Pieces on a baseline at y = 0, starting at x = 0.
    pub(crate) pieces: Vec<LinePiece>,
    pub(crate) baseline: f32,
}

/// One unwrapped line of `text` in `style`, with tracking and decorations.
pub(crate) fn single_line_run(
    ctx: &mut LayoutCtx<'_>,
    text: &str,
    style: &GlyphStyle,
    max_width: Option<Length<Positive>>,
) -> Result<SingleLine> {
    let clustered = ClusteredText::new(text);
    if (0..clustered.len()).any(|index| clustered.cluster(index).class() == BreakClass::Hard) {
        return Err(Error::from(ErrorKind::UnexpectedLineBreakInSingleLine {
            text: text_snippet(clustered.normalized_text()),
        }));
    }
    let measured = measure_inline(ctx, [(*style, text)], None)?;
    let tape = measured.text.clustered();
    let clusters: Vec<_> = (0..tape.len())
        .map(|index| tape.cluster(index).glyph_range())
        .filter(|range| !range.is_empty())
        .collect();
    let parts = clusters
        .iter()
        .map(|range| measured.ink_width(range.clone()).map(WidthPart::Ink))
        .collect::<Result<Vec<_>>>()?;
    let draws: Vec<_> = draw_widths(&parts).collect();
    let width: Length<NonNegative> = draws.iter().copied().sum();
    if let Some(max_width) = max_width {
        if width.as_pt() > max_width.as_pt() {
            return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some(text_snippet(text)),
                width: width.as_pt(),
                avail: max_width.as_pt(),
            }));
        }
    }
    let metrics = LineMetrics::new(face_extent(ctx, style.font, style.font_size)?.into(), 1.0)?;
    let mut painter = LinePainter::new(0.0, 0.0);
    painter.text(&measured, &clusters, &draws)?;
    Ok(SingleLine {
        pieces: painter.finish(Some(style.font), style.font_size, style.color)?,
        baseline: metrics.baseline.as_pt(),
    })
}

/// `font` at `size`. An unknown id records `UnknownFont` and uses [`FaceExtent::unknown`].
pub(crate) fn face_extent(
    ctx: &mut LayoutCtx<'_>,
    font: FontId,
    size: Length<Positive>,
) -> Result<FaceExtent> {
    match ctx.fonts.metrics(font) {
        Ok(face) => Ok(FaceExtent::scaled(face, size)),
        Err(error) => {
            if let ErrorKind::UnknownFont { id } = error.kind {
                ctx.soft(FontFault::UnknownFont { id });
                return Ok(FaceExtent::unknown(size));
            }
            Err(error)
        }
    }
}

pub(crate) fn align_x(
    frame_width: Length<Positive>,
    line_width: Length<NonNegative>,
    align: TextAlign,
) -> Length<Finite> {
    match align {
        TextAlign::Start => Finite::ZERO,
        TextAlign::Center => Length::from_trusted_pt((frame_width - line_width).as_pt() / 2.0),
        TextAlign::End => frame_width - line_width,
    }
}
