//! Paragraph measurement, wrapping, and fragment drawing.

#![allow(dead_code)]

use crate::canvas::{GlyphPlacement, GlyphRun};
use crate::error::{Error, Result};
use crate::fonts::FontStore;
use crate::geom::Point;
use crate::layout::flow::{FlowNode, Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::style::{Style, TextAlign};
use crate::text::cluster::ClusteredText;
use crate::text::line_break::{break_lines, LineBudget};
use crate::text::measure::{line_metrics, measure_text, MeasuredText};

/// One laid-out line, coordinates relative to the paragraph fragment origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LineBox {
    pub(crate) run: GlyphRun,
}

/// Unwrapped text. Wraps at the width of the first frame it is split against.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextNode {
    text: String,
    style: Style,
}

impl TextNode {
    pub(crate) fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    /// Every outcome hands back a laid-out `Paragraph`, so a page move never wraps twice.
    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> {
        ParagraphNode::layout(ctx.fonts, &self.text, &self.style, avail.width)?.split(avail)
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
        if !width.is_finite() || width <= 0.0 {
            return Err(Error::InvalidGeometry);
        }

        let clustered = ClusteredText::new(text);
        let measured = measure_text(fonts, style.font, clustered.glyph_text())?;
        let font_size = style.font_size.as_pt();
        let broken = break_lines(&clustered, LineBudget::uniform(width), |cluster| {
            cluster_width_pt(&measured, cluster.glyph_range(), font_size)
        })?;
        let face = fonts.metrics(style.font)?;
        let metrics = line_metrics(face, style.font_size, style.line_height_factor)?;
        let line_height = metrics.line_height;

        let mut lines = Vec::with_capacity(broken.len());
        for line in broken.iter() {
            let run = glyph_run_for_line(
                &measured,
                line.glyph_range(),
                line.glyph_byte_range(),
                style,
                line.width(),
                width,
                metrics.baseline,
            )?;
            lines.push(LineBox { run });
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
            return Err(Error::InvalidGeometry);
        }
        if self.lines.is_empty() {
            return Ok(Split::Fit {
                height: 0.0,
                fragment: Fragment::empty(),
            });
        }
        if self.line_height > avail.full_height {
            return Err(Error::UnsplittableTallerThanFrame);
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
        return Err(Error::UnbreakableWiderThanFrame {
            width,
            avail: max_width,
        });
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
    let mut commands = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let mut run = line.run.clone();
        let y = lines_height(index, line_height) + run.origin.y().as_pt();
        run.origin = Point::try_pt(run.origin.x().as_pt(), y)?;
        commands.push(FragmentCommand::Glyphs(run));
    }
    Ok(Fragment {
        height: lines_height(lines.len(), line_height),
        spacing_after,
        commands,
    })
}

fn cluster_width_pt(
    measured: &MeasuredText,
    glyph_range: std::ops::Range<usize>,
    font_size: f32,
) -> f32 {
    measured.glyphs[glyph_range]
        .iter()
        .map(|glyph| glyph.advance_norm * font_size)
        .sum()
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
