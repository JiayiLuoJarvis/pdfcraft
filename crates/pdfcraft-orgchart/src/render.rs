//! Draw placed pages with krilla. Layout stays y-down. Glyphs use the skrifa advances.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

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
use crate::error::{Error, ErrorCode};
use crate::ids::NodeId;
use crate::measure::{FontMetrics, GlyphUse, NodeBox};
use crate::page::{CueSlot, PageSpec, PlacedNode, PointMm};
use crate::ratios::{place_ratio_labels, RatioMetrics};
use crate::units::{
    mm_to_pt, PageGeometry, ARROW_SIZE_PT, COMPANY_SHADE, CONT_CUE_CORNER_MM, CONT_CUE_DASH_OFF_PT,
    CONT_CUE_DASH_ON_PT, CONT_CUE_LINE_GAP_MM, CONT_CUE_LINE_WIDTH_PT, CONT_CUE_PAD_MM,
    CONT_CUE_TEXT_PT, EDGE_GRAY, EDGE_WIDTH_PT, FOOTER_TEXT_PT, INK_BYTE,
    LEADER_ARROW_HALF_ANGLE_DEG, LEADER_ARROW_LENGTH_FACTOR, MAIN_TITLE_PT, MAIN_TITLE_TOP_GAP_MM,
    NODE_CORNER_RADIUS_MM, NORMAL_BORDER_PT, PADDING_Y_MM, PAGE_NUMBER_INSET_MM, RATIO_FONT_PT,
    SUBJECT_BORDER_PT, SUBJECT_PADDING_Y_MM, SUB_TITLE_PT, TITLE_LINE_GAP_MM,
};

/// Paint `pages` into PDF bytes. Does not write document info.
///
/// # Errors
///
/// Returns [`ErrorCode::FontError`] when krilla rejects the face, and glyph errors.
/// A placed node with no measured box is [`ErrorCode::LayoutFailed`].
pub(crate) fn render(pages: &[PageSpec], font: &mut FontMetrics) -> Result<Vec<u8>, Error> {
    let mut document = Document::new_with(SerializeSettings {
        enable_tagging: false,
        ..SerializeSettings::default()
    });
    let face = Font::new(Data::from(Arc::clone(font.bytes())), 0)
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
    font: &mut FontMetrics,
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
            stroke_poly(&mut surface, &points, EDGE_WIDTH_PT, edge.draw_arrow);
        }
        let box_by_id: HashMap<NodeId, &NodeBox> =
            page.boxes.iter().map(|box_| (box_.id, box_)).collect();
        for node in &page.layout.nodes {
            let box_ = box_by_id.get(&node.id).copied().ok_or_else(|| {
                Error::new(
                    ErrorCode::LayoutFailed,
                    format!("placed node {} has no measured box", node.protocol_id),
                )
            })?;
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
            .find(|node| node.protocol_id == link.node_id)
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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "A4 mm-to-pt values fit in f32"
    )]
    {
        value as f32
    }
}

fn draw_titles(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    font: &mut FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let header = header_slots(page.geometry, &page.main_title, &page.sub_title, font)?;
    center_text(
        surface,
        &header.main_text,
        font,
        face,
        MAIN_TITLE_PT,
        header.left_mm,
        header.width_mm,
        header.main_top_mm,
    )?;
    center_text(
        surface,
        &header.sub_text,
        font,
        face,
        SUB_TITLE_PT,
        header.left_mm,
        header.width_mm,
        header.sub_top_mm,
    )?;
    Ok(())
}

struct HeaderSlots {
    left_mm: f64,
    width_mm: f64,
    main_top_mm: f64,
    sub_top_mm: f64,
    main_text: String,
    sub_text: String,
}

fn header_slots(
    geometry: PageGeometry,
    main_title: &str,
    sub_title: &str,
    measurer: &mut impl crate::measure::Measurer,
) -> Result<HeaderSlots, Error> {
    let width_mm = geometry.printable_width_mm();
    let main_top_mm = geometry.margin_mm + MAIN_TITLE_TOP_GAP_MM;
    let sub_top_mm = main_top_mm + measurer.line_height_mm(MAIN_TITLE_PT) + TITLE_LINE_GAP_MM;
    let main_text = fit_header_line(measurer, main_title, MAIN_TITLE_PT, width_mm)?;
    let sub_text = fit_header_line(measurer, sub_title, SUB_TITLE_PT, width_mm)?;
    Ok(HeaderSlots {
        left_mm: geometry.margin_mm,
        width_mm,
        main_top_mm,
        sub_top_mm,
        main_text,
        sub_text,
    })
}

fn fit_header_line(
    measurer: &mut impl crate::measure::Measurer,
    text: &str,
    size_pt: f64,
    max_width_mm: f64,
) -> Result<String, Error> {
    if measurer.string_width_mm(text, size_pt)? <= max_width_mm {
        return Ok(text.to_owned());
    }
    let ellipsis = "…";
    if measurer.string_width_mm(ellipsis, size_pt)? > max_width_mm {
        return Ok(String::new());
    }
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0_usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let mut candidate: String = chars[..mid].iter().collect();
        candidate.push('…');
        if measurer.string_width_mm(&candidate, size_pt)? <= max_width_mm {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let mut fitted: String = chars[..lo].iter().collect();
    fitted.push('…');
    Ok(fitted)
}

fn draw_cues(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    origin_x: f64,
    origin_y: f64,
    font: &mut FontMetrics,
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
        stroke_cue_box(surface, x, y, slot.width, slot.height);
        if slot.needs_leader {
            if let Some(anchor) = page
                .layout
                .nodes
                .iter()
                .find(|node| node.protocol_id == slot.anchor_id)
            {
                let (from, to) = leader_segment(slot, anchor);
                stroke_leader(
                    surface,
                    (from.x + origin_x, from.y + origin_y),
                    (to.x + origin_x, to.y + origin_y),
                );
            }
        }
        let (first, second) = cont_cue_lines(cue.target_page, cue.side);
        let text_y = y + CONT_CUE_PAD_MM;
        let next_y = text_y + font.line_height_mm(CONT_CUE_TEXT_PT) + CONT_CUE_LINE_GAP_MM;
        center_text(
            surface,
            &first,
            font,
            face,
            CONT_CUE_TEXT_PT,
            x,
            slot.width,
            text_y,
        )?;
        center_text(
            surface,
            &second,
            font,
            face,
            CONT_CUE_TEXT_PT,
            x,
            slot.width,
            next_y,
        )?;
    }
    Ok(())
}

fn draw_footer(
    surface: &mut Surface<'_>,
    page: &PageSpec,
    page_count: usize,
    index: usize,
    font: &mut FontMetrics,
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
    font: &mut FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let x = placed.x + origin_x;
    let y0 = placed.y + origin_y;
    let dashed = node.is_overflow;
    let shade = if node.is_company { COMPANY_SHADE } else { 1.0 };
    let byte = gray_byte(shade);
    fill_round(
        surface,
        x,
        y0,
        node.width_mm,
        node.height_mm,
        rgb::Color::new(byte, byte, byte),
        NODE_CORNER_RADIUS_MM,
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
        NODE_CORNER_RADIUS_MM,
    );
    let pad_y = if node.is_subject {
        SUBJECT_PADDING_Y_MM
    } else {
        PADDING_Y_MM
    };
    if let Some(path) = round_path(x, y0, node.width_mm, node.height_mm, NODE_CORNER_RADIUS_MM) {
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
    font: &mut FontMetrics,
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
    font: &mut FontMetrics,
    face: &Font,
) -> Result<(), Error> {
    let metrics = RatioMetrics {
        ascent_mm: font.ascent_mm(RATIO_FONT_PT),
        descent_mm: font.descent_mm(RATIO_FONT_PT),
    };
    for label in place_ratio_labels(page, metrics) {
        let ascent = font.ascent_mm(RATIO_FONT_PT);
        draw_text(
            surface,
            &label.text,
            font,
            face,
            RATIO_FONT_PT,
            label.x_mm + origin_x,
            label.baseline_y_mm - ascent + origin_y,
        )?;
    }
    Ok(())
}

fn centered_x(left_mm: f64, width_mm: f64, text_width_mm: f64) -> f64 {
    left_mm + (width_mm - text_width_mm) / 2.0
}

fn leader_segment(slot: &CueSlot, anchor: &PlacedNode) -> (PointMm, PointMm) {
    let slot_center = PointMm {
        x: slot.x + slot.width / 2.0,
        y: slot.y + slot.height / 2.0,
    };
    let anchor_center = PointMm {
        x: anchor.x + anchor.width / 2.0,
        y: anchor.y + anchor.height / 2.0,
    };
    let dx = anchor_center.x - slot_center.x;
    let dy = anchor_center.y - slot_center.y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (
                PointMm {
                    x: slot.x + slot.width,
                    y: slot_center.y,
                },
                PointMm {
                    x: anchor.x,
                    y: anchor_center.y,
                },
            )
        } else {
            (
                PointMm {
                    x: slot.x,
                    y: slot_center.y,
                },
                PointMm {
                    x: anchor.x + anchor.width,
                    y: anchor_center.y,
                },
            )
        }
    } else if dy >= 0.0 {
        (
            PointMm {
                x: slot_center.x,
                y: slot.y + slot.height,
            },
            PointMm {
                x: anchor_center.x,
                y: anchor.y,
            },
        )
    } else {
        (
            PointMm {
                x: slot_center.x,
                y: slot.y,
            },
            PointMm {
                x: anchor_center.x,
                y: anchor.y + anchor.height,
            },
        )
    }
}

fn leader_arrow_length_pt() -> f64 {
    ARROW_SIZE_PT * LEADER_ARROW_LENGTH_FACTOR
}

fn stroke_leader(surface: &mut Surface<'_>, from: (f64, f64), to: (f64, f64)) {
    let mut path = PathBuilder::new();
    path.move_to(pt(from.0), pt(from.1));
    path.line_to(pt(to.0), pt(to.1));
    let Some(path) = path.finish() else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(paint_stroke(
        pt_f32(CONT_CUE_LINE_WIDTH_PT),
        true,
        edge_color(),
    )));
    surface.draw_path(&path);
    fill_leader_arrow(surface, from, to);
}

fn fill_leader_arrow(surface: &mut Surface<'_>, from: (f64, f64), to: (f64, f64)) {
    let dx = mm_to_pt(to.0 - from.0);
    let dy = mm_to_pt(to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.01 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let length = pt_f32(leader_arrow_length_pt());
    let wing = pt_f32(leader_arrow_length_pt() * LEADER_ARROW_HALF_ANGLE_DEG.to_radians().tan());
    let tip_x = pt(to.0);
    let tip_y = pt(to.1);
    let unit_x = pt_f32(ux);
    let unit_y = pt_f32(uy);
    let base_x = tip_x - unit_x * length;
    let base_y = tip_y - unit_y * length;
    let px = -unit_y * wing;
    let py = unit_x * wing;
    let mut path = PathBuilder::new();
    path.move_to(tip_x, tip_y);
    path.line_to(base_x + px, base_y + py);
    path.line_to(base_x - px, base_y - py);
    path.close();
    let Some(path) = path.finish() else {
        return;
    };
    surface.set_fill(Some(solid(edge_color())));
    surface.set_stroke(None);
    surface.draw_path(&path);
}

fn center_text(
    surface: &mut Surface<'_>,
    text: &str,
    font: &mut FontMetrics,
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
    let x = centered_x(left_mm, width_mm, text_width);
    draw_text(surface, text, font, face, size_pt, x, y_mm)
}

fn draw_text(
    surface: &mut Surface<'_>,
    text: &str,
    font: &mut FontMetrics,
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

fn fill_round(
    surface: &mut Surface<'_>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: rgb::Color,
    radius_mm: f64,
) {
    let Some(path) = round_path(x, y, w, h, radius_mm) else {
        return;
    };
    surface.set_fill(Some(solid(color)));
    surface.set_stroke(None);
    surface.draw_path(&path);
}

fn stroke_cue_box(surface: &mut Surface<'_>, x: f64, y: f64, w: f64, h: f64) {
    let Some(path) = round_path(x, y, w, h, CONT_CUE_CORNER_MM) else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(ink_stroke(pt_f32(CONT_CUE_LINE_WIDTH_PT), true)));
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
    radius_mm: f64,
) {
    let Some(path) = round_path(x, y, w, h, radius_mm) else {
        return;
    };
    surface.set_fill(None);
    surface.set_stroke(Some(ink_stroke(width_pt, dashed)));
    surface.draw_path(&path);
}

fn stroke_poly(surface: &mut Surface<'_>, points: &[(f64, f64)], width_pt: f64, draw_arrow: bool) {
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
    surface.set_stroke(Some(edge_stroke(pt_f32(width_pt))));
    surface.draw_path(&path);
    if draw_arrow {
        arrow(surface, points);
    }
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
    surface.set_fill(Some(solid(edge_color())));
    surface.set_stroke(None);
    surface.draw_path(&path);
}

#[allow(
    clippy::many_single_char_names,
    reason = "x/y/w/h are rectangle geometry params"
)]
fn round_path(
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

fn solid(color: rgb::Color) -> Fill {
    Fill {
        paint: color.into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }
}

fn ink_color() -> rgb::Color {
    rgb::Color::new(INK_BYTE, INK_BYTE, INK_BYTE)
}

fn edge_color() -> rgb::Color {
    let byte = gray_byte(EDGE_GRAY);
    rgb::Color::new(byte, byte, byte)
}

fn ink_stroke(width_pt: f32, dashed: bool) -> Stroke {
    paint_stroke(width_pt, dashed, ink_color())
}

fn edge_stroke(width_pt: f32) -> Stroke {
    paint_stroke(width_pt, false, edge_color())
}

fn paint_stroke(width_pt: f32, dashed: bool, color: rgb::Color) -> Stroke {
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

fn rect_pt(x: f64, y: f64, w: f64, h: f64) -> Option<Rect> {
    Rect::from_xywh(pt(x), pt(y), pt(w), pt(h))
}

#[cfg(test)]
mod tests {
    use super::{
        centered_x, fit_header_line, header_slots, leader_arrow_length_pt, leader_segment,
    };
    use crate::error::Error;
    use crate::measure::Measurer;
    use crate::units::PageGeometry;

    struct Widths {
        per_char_mm: f64,
        ellipsis_mm: f64,
    }

    impl Measurer for Widths {
        fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, Error> {
            let mut width = 0.0;
            for ch in text.chars() {
                width += if ch == '…' {
                    self.ellipsis_mm
                } else {
                    self.per_char_mm
                };
            }
            Ok(width)
        }

        fn line_height_mm(&mut self, size_pt: f64) -> f64 {
            size_pt
        }

        fn ensure_glyphs(&mut self, _text: &str) -> Result<(), Error> {
            Ok(())
        }
    }

    fn near(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} vs {expected}");
    }

    #[test]
    fn main_title_slot_starts_at_the_page_margin() {
        let geometry = PageGeometry::default();
        let mut widths = Widths {
            per_char_mm: 1.0,
            ellipsis_mm: 1.0,
        };
        let slots = header_slots(geometry, "主体", "副标题", &mut widths).expect("slots");
        near(slots.main_top_mm, 8.0);
        near(slots.sub_top_mm, 8.0 + 16.0 + 1.5);
        near(slots.left_mm, 8.0);
        near(slots.width_mm, 210.0 - 16.0);
        near(
            slots.left_mm + slots.width_mm / 2.0,
            geometry.page_width_mm / 2.0,
        );
        assert_eq!(slots.main_text, "主体");
        assert_eq!(slots.sub_text, "副标题");
    }

    #[test]
    fn overlong_header_ends_with_an_ellipsis() {
        let geometry = PageGeometry::default();
        let mut widths = Widths {
            per_char_mm: 100.0,
            ellipsis_mm: 10.0,
        };
        let slots = header_slots(geometry, "甲乙丙", "短", &mut widths).expect("slots");
        assert_eq!(slots.main_text, "甲…");
        assert!(slots.main_text.ends_with('…'));
        assert_eq!(slots.sub_text, "短");
    }

    #[test]
    fn header_ellipsis_wider_than_the_page_is_blank() {
        let mut widths = Widths {
            per_char_mm: 100.0,
            ellipsis_mm: 1_000.0,
        };
        let fitted = fit_header_line(&mut widths, "甲乙", 16.0, 194.0).expect("fit");
        assert_eq!(fitted, "");
    }

    #[test]
    fn cue_text_starts_in_the_slot_not_at_the_pad() {
        let x = centered_x(10.0, 40.0, 10.0);
        near(x, 25.0);
        assert!((x - 12.0).abs() > 1.0);
    }

    #[test]
    fn a_side_leader_joins_facing_edge_midpoints() {
        let slot = crate::page::CueSlot {
            anchor_id: "a".to_owned(),
            side: crate::captions::CueSide::Shareholder,
            x: 0.0,
            y: 10.0,
            width: 20.0,
            height: 10.0,
            needs_leader: true,
        };
        let anchor = crate::page::PlacedNode {
            id: crate::ids::NodeId::from_index(0),
            protocol_id: "a".to_owned(),
            x: 40.0,
            y: 10.0,
            width: 20.0,
            height: 10.0,
        };
        let (from, to) = leader_segment(&slot, &anchor);
        near(from.x, 20.0);
        near(from.y, 15.0);
        near(to.x, 40.0);
        near(to.y, 15.0);
    }

    #[test]
    fn leader_arrow_is_four_point_five_pt_at_thirty_degrees() {
        near(leader_arrow_length_pt(), 4.5);
        let wing = crate::units::LEADER_ARROW_HALF_ANGLE_DEG.to_radians().tan();
        near(wing, 3.0_f64.sqrt().recip());
        assert!((wing - 0.45).abs() > 0.05);
    }

    #[test]
    fn continuation_box_stroke_is_one_point_and_one_millimetre() {
        near(crate::units::CONT_CUE_LINE_WIDTH_PT, 1.0);
        near(crate::units::CONT_CUE_CORNER_MM, 1.0);
    }
}
