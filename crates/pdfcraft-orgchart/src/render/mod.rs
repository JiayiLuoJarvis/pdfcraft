//! Draw placed pages with krilla. Layout stays y-down. Glyphs use the skrifa advances.

mod chrome;
mod marks;
mod paint;
mod ratio_labels;

use std::collections::HashMap;
use std::sync::Arc;

use krilla::annotation::{LinkAnnotation, Target};
use krilla::destination::XyzDestination;
use krilla::geom::Point;
use krilla::page::PageSettings;
use krilla::surface::Surface;
use krilla::text::Font;
use krilla::{Data, Document, SerializeSettings};

use crate::error::{Error, ErrorCode};
use crate::font::FontMetrics;
use crate::graph::NodeId;
use crate::page::{NodeBox, PageSpec};
use crate::units::{EDGE_WIDTH_PT, RATIO_FONT_PT};
use chrome::{draw_footer, draw_titles};
use marks::{draw_cues, draw_node};
use paint::{draw_text, pt, rect_pt, stroke_poly};
use ratio_labels::{place_ratio_labels, RatioMetrics};

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
        Error::new(ErrorCode::FontError, "failed to finish PDF").with_source(error)
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
                    format!("placed node #{} has no measured box", node.id),
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

#[derive(Clone, Copy)]
struct TextLine {
    left: f64,
    width: f64,
    y: f64,
}

#[derive(Clone, Copy)]
struct RoundBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
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

#[cfg(test)]
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} vs {expected}");
}
