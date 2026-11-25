//! krilla primitives: paths, strokes, colours, and glyph runs.

use std::ops::Range;

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, FillRule, Stroke, StrokeDash};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};

use super::{RoundBox, TextLine};
use crate::error::{Error, ErrorCode};
use crate::font::{FontMetrics, GlyphUse};
use crate::units::{
    mm_to_pt, pt_to_mm, ARROW_SIZE_PT, CONT_CUE_CORNER_MM, CONT_CUE_DASH_OFF_PT,
    CONT_CUE_DASH_ON_PT, CONT_CUE_LINE_WIDTH_PT, EDGE_GRAY, INK_BYTE,
};

pub(super) fn pt(mm: f64) -> f32 {
    pt_f32(mm_to_pt(mm))
}

pub(super) fn pt_f32(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "A4 mm-to-pt values fit in f32"
    )]
    {
        value as f32
    }
}

pub(super) fn gray_byte(shade: f64) -> u8 {
    let scaled = (shade * 255.0).round();
    if scaled <= 0.0 {
        0
    } else if scaled >= 255.0 {
        255
    } else {
        let mut byte = 0_u8;
        let mut cursor = 0.0;
        while cursor + 1.0 <= scaled {
            cursor += 1.0;
            byte = byte.saturating_add(1);
        }
        byte
    }
}

fn centered_x(left_mm: f64, width_mm: f64, text_width_mm: f64) -> f64 {
    left_mm + (width_mm - text_width_mm) / 2.0
}

pub(super) fn center_text(
    surface: &mut Surface<'_>,
    text: &str,
    font: &mut FontMetrics,
    face: &Font,
    size_pt: f64,
    line: &TextLine,
    color: rgb::Color,
) -> Result<(), Error> {
    let TextLine {
        left: left_mm,
        width: width_mm,
        y: y_mm,
    } = *line;
    if text.is_empty() {
        return Ok(());
    }
    let glyphs = font.glyph_run(text)?;
    let units: f64 = glyphs
        .iter()
        .map(|glyph| f64::from(glyph.advance_units))
        .sum();
    let text_width = pt_to_mm(units / font.units_per_em() * size_pt);
    let x = centered_x(left_mm, width_mm, text_width);
    paint_run(
        surface,
        text,
        &glyphs,
        font,
        face,
        size_pt,
        GlyphAt {
            origin_mm: (x, y_mm),
            color,
        },
    );
    Ok(())
}

pub(super) fn draw_text(
    surface: &mut Surface<'_>,
    text: &str,
    font: &mut FontMetrics,
    face: &Font,
    size_pt: f64,
    origin_mm: (f64, f64),
    color: rgb::Color,
) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    let glyphs = font.glyph_run(text)?;
    paint_run(
        surface,
        text,
        &glyphs,
        font,
        face,
        size_pt,
        GlyphAt { origin_mm, color },
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct GlyphAt {
    origin_mm: (f64, f64),
    color: rgb::Color,
}

fn paint_run(
    surface: &mut Surface<'_>,
    text: &str,
    glyphs: &[GlyphUse],
    font: &FontMetrics,
    face: &Font,
    size_pt: f64,
    at: GlyphAt,
) {
    let drawn = krilla_glyphs(glyphs, text, font.units_per_em());
    let baseline = at.origin_mm.1 + font.ascent_mm(size_pt);
    surface.set_fill(Some(solid(at.color)));
    surface.set_stroke(None);
    surface.draw_glyphs(
        Point::from_xy(pt(at.origin_mm.0), pt(baseline)),
        &drawn,
        face.clone(),
        text,
        pt_f32(size_pt),
        false,
    );
}

fn krilla_glyphs(glyphs: &[GlyphUse], text: &str, upem: f64) -> Vec<KrillaGlyph> {
    let mut out = Vec::new();
    let mut byte = 0;
    for (glyph, ch) in glyphs.iter().zip(text.chars()) {
        let start = byte;
        byte += ch.len_utf8();
        let advance = pt_f32(f64::from(glyph.advance_units) / upem);
        out.push(KrillaGlyph::new(
            GlyphId::new(glyph.glyph_id),
            advance,
            0.0,
            0.0,
            0.0,
            Range { start, end: byte },
            None,
        ));
    }
    out
}

pub(super) fn fill_round(
    surface: &mut Surface<'_>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: rgb::Color,
    radius_mm: f64,
) -> Result<(), Error> {
    let path = path_or_layout_failed(round_path(x, y, w, h, radius_mm))?;
    surface.set_fill(Some(solid(color)));
    surface.set_stroke(None);
    surface.draw_path(&path);
    Ok(())
}

pub(super) fn stroke_cue_box(
    surface: &mut Surface<'_>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), Error> {
    let path = path_or_layout_failed(round_path(x, y, w, h, CONT_CUE_CORNER_MM))?;
    surface.set_fill(None);
    surface.set_stroke(Some(ink_stroke(pt_f32(CONT_CUE_LINE_WIDTH_PT), true)));
    surface.draw_path(&path);
    Ok(())
}

pub(super) fn stroke_round(
    surface: &mut Surface<'_>,
    box_: &RoundBox,
    width_pt: f32,
    dashed: bool,
    radius_mm: f64,
) -> Result<(), Error> {
    let RoundBox { x, y, w, h } = *box_;
    let path = path_or_layout_failed(round_path(x, y, w, h, radius_mm))?;
    surface.set_fill(None);
    surface.set_stroke(Some(ink_stroke(width_pt, dashed)));
    surface.draw_path(&path);
    Ok(())
}

pub(super) fn stroke_poly(
    surface: &mut Surface<'_>,
    points: &[(f64, f64)],
    width_pt: f64,
    draw_arrow: bool,
) -> Result<(), Error> {
    if points.len() < 2 {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            "edge polyline needs at least two points",
        ));
    }
    let mut path = PathBuilder::new();
    let (x0, y0) = points[0];
    path.move_to(pt(x0), pt(y0));
    for (x, y) in points.iter().skip(1) {
        path.line_to(pt(*x), pt(*y));
    }
    let path = path_or_layout_failed(path.finish())?;
    surface.set_fill(None);
    surface.set_stroke(Some(edge_stroke(pt_f32(width_pt))));
    surface.draw_path(&path);
    if draw_arrow {
        arrow(surface, points)?;
    }
    Ok(())
}

fn arrow(surface: &mut Surface<'_>, points: &[(f64, f64)]) -> Result<(), Error> {
    let Some((end, prev)) = points.last().zip(points.iter().rev().nth(1)) else {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            "edge polyline needs at least two points",
        ));
    };
    let dx = mm_to_pt(end.0 - prev.0);
    let dy = mm_to_pt(end.1 - prev.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.01 {
        return Ok(());
    }
    let ux = dx / len;
    let uy = dy / len;
    let size = pt_f32(ARROW_SIZE_PT);
    let tip_x = pt(end.0);
    let tip_y = pt(end.1);
    let unit_x = pt_f32(ux);
    let unit_y = pt_f32(uy);
    let base_x = tip_x - unit_x * size;
    let base_y = tip_y - unit_y * size;
    let px = -unit_y * size * 0.45;
    let py = unit_x * size * 0.45;
    let mut path = PathBuilder::new();
    path.move_to(tip_x, tip_y);
    path.line_to(base_x + px, base_y + py);
    path.line_to(base_x - px, base_y - py);
    path.close();
    let path = path_or_layout_failed(path.finish())?;
    surface.set_fill(Some(solid(edge_color())));
    surface.set_stroke(None);
    surface.draw_path(&path);
    Ok(())
}

pub(super) fn path_or_layout_failed(
    path: Option<krilla::geom::Path>,
) -> Result<krilla::geom::Path, Error> {
    path.ok_or_else(|| Error::new(ErrorCode::LayoutFailed, "path construction failed"))
}

pub(super) fn round_path(
    x_mm: f64,
    y_mm: f64,
    width_mm: f64,
    height_mm: f64,
    radius_mm: f64,
) -> Option<krilla::geom::Path> {
    let radius_mm = radius_mm.min(width_mm / 2.0).min(height_mm / 2.0);
    let left = pt(x_mm);
    let top = pt(y_mm);
    let width = pt(width_mm);
    let height = pt(height_mm);
    let radius = pt(radius_mm);
    let kappa = 0.552_284_8 * radius;
    let mut path = PathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(left + width - radius, top);
    path.cubic_to(
        left + width - radius + kappa,
        top,
        left + width,
        top + radius - kappa,
        left + width,
        top + radius,
    );
    path.line_to(left + width, top + height - radius);
    path.cubic_to(
        left + width,
        top + height - radius + kappa,
        left + width - radius + kappa,
        top + height,
        left + width - radius,
        top + height,
    );
    path.line_to(left + radius, top + height);
    path.cubic_to(
        left + radius - kappa,
        top + height,
        left,
        top + height - radius + kappa,
        left,
        top + height - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(
        left,
        top + radius - kappa,
        left + radius - kappa,
        top,
        left + radius,
        top,
    );
    path.close();
    path.finish()
}

pub(super) fn solid(color: rgb::Color) -> Fill {
    Fill {
        paint: color.into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }
}

pub(super) fn ink_color() -> rgb::Color {
    rgb::Color::new(INK_BYTE, INK_BYTE, INK_BYTE)
}

pub(super) fn edge_color() -> rgb::Color {
    let byte = gray_byte(EDGE_GRAY);
    rgb::Color::new(byte, byte, byte)
}

fn ink_stroke(width_pt: f32, dashed: bool) -> Stroke {
    paint_stroke(width_pt, dashed, ink_color())
}

fn edge_stroke(width_pt: f32) -> Stroke {
    paint_stroke(width_pt, false, edge_color())
}

pub(super) fn paint_stroke(width_pt: f32, dashed: bool, color: rgb::Color) -> Stroke {
    Stroke {
        paint: color.into(),
        width: width_pt,
        dash: dashed.then(|| StrokeDash {
            array: vec![pt_f32(CONT_CUE_DASH_ON_PT), pt_f32(CONT_CUE_DASH_OFF_PT)],
            offset: 0.0,
        }),
        ..Stroke::default()
    }
}

pub(super) fn rect_pt(x: f64, y: f64, w: f64, h: f64) -> Option<Rect> {
    Rect::from_xywh(pt(x), pt(y), pt(w), pt(h))
}

#[cfg(test)]
mod tests {
    use super::{centered_x, path_or_layout_failed};
    use crate::error::ErrorCode;
    use crate::render::near;

    #[test]
    fn cue_text_starts_in_the_slot_not_at_the_pad() {
        let x = centered_x(10.0, 40.0, 10.0);
        near(x, 25.0);
        assert!((x - 12.0).abs() > 1.0);
    }

    #[test]
    fn a_missing_path_is_layout_failed() {
        let Err(error) = path_or_layout_failed(None) else {
            panic!("missing path succeeded");
        };
        assert_eq!(error.code(), ErrorCode::LayoutFailed);
        assert!(error.message().contains("path construction failed"));
    }
}
