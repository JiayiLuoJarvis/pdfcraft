//! Draw placed pages with krilla. Layout stays y-down. Glyphs use the skrifa advances.

use std::ops::Range;

use krilla::annotation::{LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::destination::XyzDestination;
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, Stroke, StrokeDash};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::{Data, Document, SerializeSettings};

use crate::captions::{cont_cue_lines, footer};
use crate::display::LabelHug;
use crate::error::{Error, ErrorCode};
use crate::graph::OVERFLOW_ID_PREFIX;
use crate::measure::{FontMetrics, GlyphUse, NodeBox};
use crate::page::{PageSpec, PlacedNode};
use crate::units::{
    mm_to_pt, ARROW_SIZE_PT, COMPANY_SHADE, CONT_CUE_DASH_OFF_PT, CONT_CUE_DASH_ON_PT,
    CONT_CUE_LINE_GAP_MM, CONT_CUE_PAD_MM, CONT_CUE_TEXT_PT, EDGE_WIDTH_PT, FOOTER_TEXT_PT,
    MAIN_TITLE_PT, NODE_CORNER_RADIUS_MM, NORMAL_BORDER_PT, PADDING_Y_MM, PAGE_NUMBER_INSET_MM,
    RATIO_FONT_PT, RATIO_LABEL_GAP_Y_MM, RATIO_LABEL_OFFSET_X_MM, SUBJECT_BORDER_PT,
    SUBJECT_PADDING_Y_MM, SUB_TITLE_PT,
};

/// Paint `pages` into PDF bytes. Does not write document info.
///
/// # Errors
///
/// Returns [`ErrorCode::FontError`] when krilla rejects the face, and glyph errors.
pub(crate) fn render(pages: &[PageSpec], font: &FontMetrics) -> Result<Vec<u8>, Error> {
    let mut document = Document::new_with(SerializeSettings {
        enable_tagging: false,
        ..SerializeSettings::default()
    });
    let face = Font::new(Data::from(font.bytes().to_vec()), 0)
        .ok_or_else(|| Error::new(ErrorCode::FontError, "krilla rejected the font face"))?;
    for (index, page) in pages.iter().enumerate() {
        draw_page(&mut document, page, pages.len(), font, &face, index)?;
    }
    document.finish().map_err(|error| {
        Error::new(
            ErrorCode::FontError,
            format!("failed to finish PDF: {error}"),
        )
    })
}

fn draw_page(
    document: &mut Document,
    page: &PageSpec,
    page_count: usize,
    font: &FontMetrics,
    face: &Font,
    index: usize,
) -> Result<(), Error> {
    let width = pt(page.geometry.page_width_mm);
    let height = pt(page.geometry.page_height_mm);
    let settings = PageSettings::from_wh(width, height)
        .ok_or_else(|| Error::new(ErrorCode::LayoutFailed, "page size was rejected"))?;
    let (origin_x, origin_y) = origin(page);
    let mut pdf_page = document.start_page_with(settings);
    {
        let mut surface = pdf_page.surface();
        draw_titles(&mut surface, page, font, face)?;
        for edge in &page.layout.edges {
            let points: Vec<(f64, f64)> = edge
                .points
                .iter()
                .map(|point| (point.x + origin_x, point.y + origin_y))
                .collect();
            stroke_poly(&mut surface, &points, EDGE_WIDTH_PT, false);
        }
        for node in &page.layout.nodes {
            let Some(box_) = page.boxes.iter().find(|box_| box_.id == node.id) else {
                continue;
            };
            draw_node(&mut surface, node, box_, origin_x, origin_y, font, face)?;
        }
        draw_ratios(&mut surface, page, origin_x, origin_y, font, face)?;
        draw_cues(&mut surface, page, origin_x, origin_y, font, face)?;
        draw_footer(&mut surface, page, page_count, index, font, face)?;
        surface.finish();
    }
    for cue in &page.cont_cues {
        if let Some(slot) = page
            .layout
            .slots
            .iter()
            .find(|slot| slot.anchor_id == cue.anchor_id && slot.side == cue.side)
        {
            if let Some(rect) = rect_pt(
                slot.x + origin_x,
                slot.y + origin_y,
                slot.width,
                slot.height,
            ) {
                let dest = XyzDestination::new(
                    cue.target_page.saturating_sub(1),
                    Point::from_xy(0.0, 0.0),
                );
                pdf_page.add_annotation(
                    LinkAnnotation::new(rect, Target::Destination(dest.into())).into(),
                );
            }
        }
    }
    for link in &page.node_page_links {
        if let Some(node) = page
            .layout
            .nodes
            .iter()
            .find(|node| node.id == link.node_id)
        {
            if let Some(rect) = rect_pt(
                node.x + origin_x,
                node.y + origin_y,
                node.width,
                node.height,
            ) {
                let dest = XyzDestination::new(
                    link.target_page.saturating_sub(1),
                    Point::from_xy(0.0, 0.0),
                );
                pdf_page.add_annotation(
                    LinkAnnotation::new(rect, Target::Destination(dest.into())).into(),
                );
            }
        }
    }
    pdf_page.finish();
    Ok(())
}

fn origin(page: &PageSpec) -> (f64, f64) {
    let geo = page.geometry;
    let layout = &page.layout;
    let x = geo.margin_mm + (geo.printable_width_mm() - layout.width_mm) / 2.0 - layout.min_x;
    let y =
        geo.margin_mm + geo.header_height_mm + (geo.printable_height_mm() - layout.height_mm) / 2.0
            - layout.min_y;
    (x, y)
}

fn pt(mm: f64) -> f32 {
    pt_f32(mm_to_pt(mm))
}

fn pt_f32(value: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    {
        value as f32
    }
}

fn draw_titles(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    font: &FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let width = page.geometry.page_width_mm;
    let y = page.geometry.margin_mm + 1.0;
    center_text(
        surface,
        &page.main_title,
        font,
        face,
        MAIN_TITLE_PT,
        0.0,
        width,
        y,
    )?;
    let sub_y = y + font.line_height_mm(MAIN_TITLE_PT);
    center_text(
        surface,
        &page.sub_title,
        font,
        face,
        SUB_TITLE_PT,
        0.0,
        width,
        sub_y,
    )?;
    Ok(())
}

fn draw_cues(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    origin_x: f64,
    origin_y: f64,
    font: &FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    for cue in &page.cont_cues {
        let Some(slot) = page
            .layout
            .slots
            .iter()
            .find(|slot| slot.anchor_id == cue.anchor_id && slot.side == cue.side)
        else {
            continue;
        };
        let x = slot.x + origin_x;
        let y = slot.y + origin_y;
        stroke_round(
            surface,
            x,
            y,
            slot.width,
            slot.height,
            pt_f32(NORMAL_BORDER_PT),
            true,
        );
        let (first, second) = cont_cue_lines(cue.target_page, cue.side);
        let text_y = y + CONT_CUE_PAD_MM;
        draw_text(
            surface,
            &first,
            font,
            face,
            CONT_CUE_TEXT_PT,
            x + CONT_CUE_PAD_MM,
            text_y,
        )?;
        draw_text(
            surface,
            &second,
            font,
            face,
            CONT_CUE_TEXT_PT,
            x + CONT_CUE_PAD_MM,
            text_y + font.line_height_mm(CONT_CUE_TEXT_PT) + CONT_CUE_LINE_GAP_MM,
        )?;
    }
    Ok(())
}

fn draw_footer(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    page_count: usize,
    index: usize,
    font: &FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let text = footer(index + 1, page_count);
    let width = font.string_width_mm(&text, FOOTER_TEXT_PT)?;
    let x = page.geometry.page_width_mm - PAGE_NUMBER_INSET_MM - width;
    let y = page.geometry.page_height_mm - PAGE_NUMBER_INSET_MM - font.ascent_mm(FOOTER_TEXT_PT);
    draw_text(surface, &text, font, face, FOOTER_TEXT_PT, x, y)?;
    Ok(())
}

fn draw_node(
    surface: &mut Surface<'_>,
    placed: &PlacedNode,
    node: &NodeBox,
    origin_x: f64,
    origin_y: f64,
    font: &FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let x = placed.x + origin_x;
    let y0 = placed.y + origin_y;
    let dashed = node.id.starts_with(OVERFLOW_ID_PREFIX);
    let shade = if node.is_company { COMPANY_SHADE } else { 1.0 };
    let byte = gray_byte(shade);
    fill_round(
        surface,
        x,
        y0,
        node.width_mm,
        node.height_mm,
        rgb::Color::new(byte, byte, byte),
    );
    let width = if node.is_subject {
        SUBJECT_BORDER_PT
    } else {
        NORMAL_BORDER_PT
    };
    stroke_round(
        surface,
        x,
        y0,
        node.width_mm,
        node.height_mm,
        pt_f32(width),
        dashed,
    );
    let pad_y = if node.is_subject {
        SUBJECT_PADDING_Y_MM
    } else {
        PADDING_Y_MM
    };
    if let Some(path) = round_path(x, y0, node.width_mm, node.height_mm) {
        surface.push_clip_path(&path, &FillRule::NonZero);
        let drawn = paint_node_lines(surface, node, x, y0 + pad_y, font, face);
        surface.pop();
        drawn
    } else {
        paint_node_lines(surface, node, x, y0 + pad_y, font, face)
    }
}

fn paint_node_lines(
    surface: &mut Surface<'_>,
    node: &NodeBox,
    x: f64,
    mut y: f64,
    font: &FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    for line in &node.lines {
        if !line.text.is_empty() {
            center_text(
                surface,
                &line.text,
                font,
                face,
                line.font_size_pt,
                x,
                node.width_mm,
                y,
            )?;
        }
        y += font.line_height_mm(line.font_size_pt);
    }
    Ok(())
}

fn gray_byte(shade: f64) -> u8 {
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

fn draw_ratios(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    origin_x: f64,
    origin_y: f64,
    font: &FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    for fact in &page.ratio_facts {
        let Some(node) = page
            .layout
            .nodes
            .iter()
            .find(|node| node.id == fact.node_id)
        else {
            continue;
        };
        if page
            .boxes
            .iter()
            .any(|box_| box_.id == node.id && box_.is_subject)
        {
            continue;
        }
        let x = node.x + origin_x + node.width / 2.0 + RATIO_LABEL_OFFSET_X_MM;
        let y = match fact.hug {
            LabelHug::Bottom => node.y + origin_y + node.height + RATIO_LABEL_GAP_Y_MM,
            LabelHug::Top => {
                node.y + origin_y - RATIO_LABEL_GAP_Y_MM - font.line_height_mm(RATIO_FONT_PT)
            }
        };
        draw_text(surface, &fact.text, font, face, RATIO_FONT_PT, x, y)?;
    }
    Ok(())
}

fn center_text(
    surface: &mut Surface<'_>,
    text: &str,
    font: &FontMetrics,
    face: &Font,
    size_pt: f64,
    left_mm: f64,
    width_mm: f64,
    y_mm: f64,
) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    let text_width = font.string_width_mm(text, size_pt)?;
    let x = left_mm + (width_mm - text_width) / 2.0;
    draw_text(surface, text, font, face, size_pt, x, y_mm)
}

fn draw_text(
    surface: &mut Surface<'_>,
    text: &str,
    font: &FontMetrics,
    face: &Font,
    size_pt: f64,
    x_mm: f64,
    y_mm: f64,
) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    let glyphs = font.glyph_run(text)?;
    let drawn = krilla_glyphs(&glyphs, text, font.units_per_em());
    let baseline = y_mm + font.ascent_mm(size_pt);
    surface.set_fill(Some(solid(rgb::Color::new(0, 0, 0))));
    surface.set_stroke(None);
    surface.draw_glyphs(
        Point::from_xy(pt(x_mm), pt(baseline)),
        &drawn,
        face.clone(),
        text,
        pt_f32(size_pt),
        false,
    );
    Ok(())
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

fn fill_round(surface: &mut Surface<'_>, x: f64, y: f64, w: f64, h: f64, color: rgb::Color) {
    let Some(path) = round_path(x, y, w, h) else {
        return;
    };
    surface.set_fill(Some(solid(color)));
    surface.set_stroke(None);
    surface.draw_path(&path);
}

fn stroke_round(
    surface: &mut Surface<'_>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    width_pt: f32,
    dashed: bool,
) {
    let Some(path) = round_path(x, y, w, h) else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(stroke(width_pt, dashed)));
    surface.draw_path(&path);
}

fn stroke_poly(surface: &mut Surface<'_>, points: &[(f64, f64)], width_pt: f64, dashed: bool) {
    if points.len() < 2 {
        return;
    }
    let mut path = PathBuilder::new();
    let (x0, y0) = points[0];
    path.move_to(pt(x0), pt(y0));
    for (x, y) in points.iter().skip(1) {
        path.line_to(pt(*x), pt(*y));
    }
    let Some(path) = path.finish() else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(stroke(pt_f32(width_pt), dashed)));
    surface.draw_path(&path);
    arrow(surface, points);
}

fn arrow(surface: &mut Surface<'_>, points: &[(f64, f64)]) {
    let Some((end, prev)) = points.last().zip(points.iter().rev().nth(1)) else {
        return;
    };
    let dx = mm_to_pt(end.0 - prev.0);
    let dy = mm_to_pt(end.1 - prev.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.01 {
        return;
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
    let Some(path) = path.finish() else {
        return;
    };
    surface.set_fill(Some(solid(rgb::Color::new(51, 51, 51))));
    surface.set_stroke(None);
    surface.draw_path(&path);
}

#[allow(clippy::many_single_char_names)]
fn round_path(x_mm: f64, y_mm: f64, width_mm: f64, height_mm: f64) -> Option<krilla::geom::Path> {
    let radius_mm = NODE_CORNER_RADIUS_MM
        .min(width_mm / 2.0)
        .min(height_mm / 2.0);
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

fn solid(color: rgb::Color) -> Fill {
    Fill {
        paint: color.into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }
}

fn stroke(width_pt: f32, dashed: bool) -> Stroke {
    Stroke {
        paint: rgb::Color::new(20, 20, 20).into(),
        width: width_pt,
        dash: dashed.then(|| StrokeDash {
            array: vec![pt_f32(CONT_CUE_DASH_ON_PT), pt_f32(CONT_CUE_DASH_OFF_PT)],
            offset: 0.0,
        }),
        ..Stroke::default()
    }
}

fn rect_pt(x: f64, y: f64, w: f64, h: f64) -> Option<Rect> {
    Rect::from_xywh(pt(x), pt(y), pt(w), pt(h))
}
