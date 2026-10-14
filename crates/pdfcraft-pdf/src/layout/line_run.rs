//! One unwrapped glyph run, plus metrics and alignment that paragraphs also use.

use crate::canvas::{GlyphPlacement, GlyphRun};
use crate::error::{text_snippet, Error, ErrorKind, Fallible as Result, FontFault, GeometryKind};
use crate::geom::{Finite, Length, NonNegative, Point, Positive};
use crate::layout::frame::LayoutCtx;
use crate::layout::style::{Style, TextAlign};
use crate::text::break_class::BreakClass;
use crate::text::cluster::ClusteredText;
use crate::text::measure::{line_metrics, measure_text, LineMetrics, MeasuredText};

/// One unwrapped run of `text` at origin `(0, baseline)`.
pub(crate) fn single_line_run(
    ctx: &mut LayoutCtx<'_>,
    text: &str,
    style: &Style,
    max_width: Option<Length<Positive>>,
) -> Result<GlyphRun> {
    let clustered = ClusteredText::new(text);
    if (0..clustered.len()).any(|index| clustered.cluster(index).class() == BreakClass::Hard) {
        return Err(Error::from(ErrorKind::UnexpectedLineBreakInSingleLine {
            text: text_snippet(clustered.normalized_text()),
        }));
    }
    let measured = measure_text(ctx.fonts, ctx.errors, style.font, clustered.glyph_text())?;
    let width =
        Length::<NonNegative>::from_trusted_pt(measured.advance_norm * style.font_size.as_pt());
    if let Some(max_width) = max_width {
        if width.as_pt() > max_width.as_pt() {
            return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some(text_snippet(text)),
                width: width.as_pt(),
                avail: max_width.as_pt(),
            }));
        }
    }
    let metrics = line_metrics_or_soft(ctx, style)?;
    let frame_width = max_width.unwrap_or_else(|| Length::from_trusted_pt(width.as_pt().max(1.0)));
    let mut run = glyph_run_for_line(
        &measured,
        0..measured.glyphs.len(),
        0..measured.text.len(),
        style,
        width,
        frame_width,
        metrics.baseline,
    )?;
    run.origin = Point::try_pt(0.0, metrics.baseline.as_pt())?;
    Ok(run)
}

pub(crate) fn line_metrics_or_soft(ctx: &mut LayoutCtx<'_>, style: &Style) -> Result<LineMetrics> {
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
                    ascent: Length::from_trusted_pt(size),
                    descent: Finite::ZERO,
                    leading: Finite::ZERO,
                    line_height: Length::from_trusted_pt(size * factor),
                    baseline: Length::from_trusted_pt(size),
                });
            }
            Err(error)
        }
    }
}

fn glyph_run_for_line(
    measured: &MeasuredText,
    glyph_range: std::ops::Range<usize>,
    byte_range: std::ops::Range<usize>,
    style: &Style,
    line_width: Length<NonNegative>,
    frame_width: Length<Positive>,
    baseline: Length<Finite>,
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
    Ok(GlyphRun {
        font: style.font,
        font_size: style.font_size,
        color: style.color,
        origin: Point::try_pt(x.as_pt(), baseline.as_pt())?,
        glyphs,
        text,
    })
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
