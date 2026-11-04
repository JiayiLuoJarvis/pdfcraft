//! Nodes, continuation boxes, and the leaders that join them.

use krilla::color::rgb;
use krilla::geom::PathBuilder;
use krilla::paint::FillRule;
use krilla::surface::Surface;
use krilla::text::Font;

use super::paint::{
    center_text, edge_color, fill_round, gray_byte, paint_stroke, pt, pt_f32, round_path, solid,
    stroke_cue_box, stroke_round,
};
use super::{RoundBox, TextLine};
use crate::captions::cont_cue_lines;
use crate::error::Error;
use crate::font::FontMetrics;
use crate::page::{CueSlot, NodeBox, PageSpec, PlacedNode, PointMm};
use crate::units::{
    mm_to_pt, ARROW_SIZE_PT, COMPANY_SHADE, CONT_CUE_LINE_GAP_MM, CONT_CUE_LINE_WIDTH_PT,
    CONT_CUE_PAD_MM, CONT_CUE_TEXT_PT, LEADER_ARROW_HALF_ANGLE_DEG, LEADER_ARROW_LENGTH_FACTOR,
    NODE_CORNER_RADIUS_MM, NORMAL_BORDER_PT, PADDING_Y_MM, SUBJECT_BORDER_PT, SUBJECT_PADDING_Y_MM,
};

pub(super) fn draw_cues(
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
            &TextLine {
                left: x,
                width: slot.width,
                y: text_y,
            },
        )?;
        center_text(
            surface,
            &second,
            font,
            face,
            CONT_CUE_TEXT_PT,
            &TextLine {
                left: x,
                width: slot.width,
                y: next_y,
            },
        )?;
    }
    Ok(())
}

pub(super) fn draw_node(
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
        &RoundBox {
            x,
            y: y0,
            w: node.width_mm,
            h: node.height_mm,
        },
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
                &TextLine {
                    left: x,
                    width: node.width_mm,
                    y,
                },
            )?;
        }
        y += font.line_height_mm(line.font_size_pt);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{leader_arrow_length_pt, leader_segment};
    use crate::render::near;

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
            id: crate::graph::NodeId::from_index(0),
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
