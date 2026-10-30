//! Continuation-cue slots and the numbered links written after page order is final.

use crate::captions::{cont_cue_lines, overflow_detail_line, penetration_title, CueSide};
use crate::display::RatioFact;
use crate::error::{Error, ErrorCode};
use crate::measure::{Measurer, NodeBox};
use crate::page::{
    ContCue, CueSlot, LaidOutEdge, LayoutResult, NodePageLink, PageSpec, PlacedNode, PointMm,
};
use crate::units::{
    within, PageGeometry, CONT_CUE_GAP_MM, CONT_CUE_PAD_MM, CONT_CUE_TEXT_PT, FIT_EPSILON_MM,
    NAME_FONT_PT, PADDING_X_MM, ROW_GAP_MM, SHAREHOLDER_STUB_MIN_MM,
};

pub(crate) struct PendingCue {
    pub(crate) source_index: usize,
    pub(crate) target_index: usize,
    pub(crate) direct_count: usize,
    pub(crate) side: CueSide,
    pub(crate) anchor_id: String,
}

pub(crate) struct PendingLink {
    pub(crate) source_index: usize,
    pub(crate) target_index: usize,
    pub(crate) node_id: String,
}

pub(crate) struct UnnumberedPage {
    pub(crate) main_title: String,
    pub(crate) sub_title: String,
    pub(crate) boxes: Vec<NodeBox>,
    pub(crate) layout: LayoutResult,
    pub(crate) geometry: PageGeometry,
    pub(crate) penetration_name: Option<String>,
    pub(crate) ratio_facts: Vec<RatioFact>,
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

pub(crate) struct CueAnchorSize {
    pub(crate) anchor_id: String,
    pub(crate) side: CueSide,
    pub(crate) width_mm: f64,
    pub(crate) height_mm: f64,
}

pub(crate) fn absorb_cue_slots<F>(
    anchors: &[CueAnchorSize],
    mut relayout: F,
    geometry: PageGeometry,
) -> Result<Option<LayoutResult>, Error>
where
    F: FnMut(f64) -> Result<LayoutResult, Error>,
{
    if anchors.is_empty() {
        return Ok(Some(relayout(ROW_GAP_MM)?));
    }
    reject_duplicate_keys(anchors)?;
    let mut node_layout = nodes_only(relayout(ROW_GAP_MM)?)?;
    let required_gap = required_row_gap(anchors, &node_layout);
    if required_gap > ROW_GAP_MM {
        node_layout = nodes_only(relayout(required_gap)?)?;
    }
    let Some(slots) = place_all(anchors, &node_layout, geometry) else {
        return Ok(None);
    };
    Ok(Some(LayoutResult::from_parts(
        node_layout.nodes,
        node_layout.edges,
        slots,
    )?))
}

fn reject_duplicate_keys(anchors: &[CueAnchorSize]) -> Result<(), Error> {
    for (index, anchor) in anchors.iter().enumerate() {
        if anchors[..index]
            .iter()
            .any(|other| other.anchor_id == anchor.anchor_id && other.side == anchor.side)
        {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                "duplicate cue anchor identity",
            ));
        }
    }
    Ok(())
}

fn nodes_only(layout: LayoutResult) -> Result<LayoutResult, Error> {
    if !layout.slots.is_empty() {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            "node relayout must return no cue slots",
        ));
    }
    Ok(layout)
}

fn required_row_gap(anchors: &[CueAnchorSize], layout: &LayoutResult) -> f64 {
    let mut required = 0.0_f64;
    for anchor in anchors {
        let Some(node) = layout
            .nodes
            .iter()
            .find(|node| node.protocol_id == anchor.anchor_id)
        else {
            continue;
        };
        let zone = clearance(preferred_rect(anchor, node));
        let blockers: Vec<&PlacedNode> = layout
            .nodes
            .iter()
            .filter(|other| other.id != node.id && overlaps_node(&zone, other))
            .collect();
        let edge_hit = hits_edges(&zone, &layout.edges);
        if !blockers.is_empty() && !blockers.iter().all(|blocker| adjacent_row(node, blocker)) {
            continue;
        }
        if !blockers.is_empty() {
            required = required.max(anchor.height_mm + 2.0 * CONT_CUE_GAP_MM);
        }
        if edge_hit {
            required =
                required.max(SHAREHOLDER_STUB_MIN_MM + anchor.height_mm + 2.0 * CONT_CUE_GAP_MM);
        }
    }
    required
}

fn adjacent_row(anchor_node: &PlacedNode, blocker: &PlacedNode) -> bool {
    separated_y(anchor_node, blocker) && overlaps_x(anchor_node, blocker)
}

fn place_all(
    anchors: &[CueAnchorSize],
    layout: &LayoutResult,
    geometry: PageGeometry,
) -> Option<Vec<CueSlot>> {
    let mut ordered: Vec<&CueAnchorSize> = anchors.iter().collect();
    ordered.sort_by(|left, right| {
        let left_node = layout
            .nodes
            .iter()
            .find(|node| node.protocol_id == left.anchor_id);
        let right_node = layout
            .nodes
            .iter()
            .find(|node| node.protocol_id == right.anchor_id);
        let left_key = left_node.map_or((f64::INFINITY, f64::INFINITY), |node| (node.y, node.x));
        let right_key = right_node.map_or((f64::INFINITY, f64::INFINITY), |node| (node.y, node.x));
        left_key
            .0
            .total_cmp(&right_key.0)
            .then(left_key.1.total_cmp(&right_key.1))
            .then(left.anchor_id.cmp(&right.anchor_id))
    });
    let mut placed = Vec::new();
    for anchor in ordered {
        let node = layout
            .nodes
            .iter()
            .find(|node| node.protocol_id == anchor.anchor_id)?;
        let slot = place_one(anchor, node, layout, &placed, geometry)?;
        placed.push(slot);
    }
    Some(placed)
}

fn place_one(
    anchor: &CueAnchorSize,
    node: &PlacedNode,
    layout: &LayoutResult,
    placed: &[CueSlot],
    geometry: PageGeometry,
) -> Option<CueSlot> {
    let candidates = [
        (preferred_rect(anchor, node), false),
        (beside_rect(anchor, node, true), true),
        (beside_rect(anchor, node, false), true),
    ];
    for (rect, needs_leader) in candidates {
        if candidate_ok(&rect, anchor, layout, placed, geometry) {
            return Some(CueSlot {
                anchor_id: anchor.anchor_id.clone(),
                side: anchor.side,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                needs_leader,
            });
        }
    }
    None
}

fn preferred_rect(anchor: &CueAnchorSize, node: &PlacedNode) -> Rect {
    let x = node.x + node.width / 2.0 - anchor.width_mm / 2.0;
    let y = if anchor.side == CueSide::Shareholder {
        node.y - CONT_CUE_GAP_MM - anchor.height_mm
    } else {
        node.y + node.height + CONT_CUE_GAP_MM
    };
    Rect {
        x,
        y,
        width: anchor.width_mm,
        height: anchor.height_mm,
    }
}

fn beside_rect(anchor: &CueAnchorSize, node: &PlacedNode, left: bool) -> Rect {
    let y = node.y + node.height / 2.0 - anchor.height_mm / 2.0;
    let x = if left {
        node.x - CONT_CUE_GAP_MM - anchor.width_mm
    } else {
        node.x + node.width + CONT_CUE_GAP_MM
    };
    Rect {
        x,
        y,
        width: anchor.width_mm,
        height: anchor.height_mm,
    }
}

fn candidate_ok(
    rect: &Rect,
    anchor: &CueAnchorSize,
    layout: &LayoutResult,
    placed: &[CueSlot],
    geometry: PageGeometry,
) -> bool {
    let zone = clearance(Rect { ..*rect });
    if layout.nodes.iter().any(|node| overlaps_node(&zone, node)) {
        return false;
    }
    if placed.iter().any(|slot| overlaps_slot(&zone, slot)) {
        return false;
    }
    if hits_edges(&zone, &layout.edges) {
        return false;
    }
    let mut slots = placed.to_vec();
    slots.push(CueSlot {
        anchor_id: anchor.anchor_id.clone(),
        side: anchor.side,
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        needs_leader: false,
    });
    let Ok(union) = LayoutResult::from_parts(layout.nodes.clone(), layout.edges.clone(), slots)
    else {
        return false;
    };
    within(union.width_mm, geometry.printable_width_mm())
}

fn clearance(rect: Rect) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y - CONT_CUE_GAP_MM,
        width: rect.width,
        height: rect.height + 2.0 * CONT_CUE_GAP_MM,
    }
}

fn hits_edges(rect: &Rect, edges: &[LaidOutEdge]) -> bool {
    edges.iter().any(|edge| {
        edge.points
            .windows(2)
            .any(|pair| segment_hits(pair[0], pair[1], rect))
    })
}

fn segment_hits(start: PointMm, end: PointMm, rect: &Rect) -> bool {
    let x0 = start.x.min(end.x);
    let x1 = start.x.max(end.x);
    let y0 = start.y.min(end.y);
    let y1 = start.y.max(end.y);
    let eps = FIT_EPSILON_MM;
    x1 > rect.x + eps
        && rect.x + rect.width > x0 + eps
        && y1 > rect.y + eps
        && rect.y + rect.height > y0 + eps
}

fn overlaps_node(rect: &Rect, node: &PlacedNode) -> bool {
    overlaps(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        node.x,
        node.y,
        node.width,
        node.height,
    )
}

fn overlaps_slot(rect: &Rect, slot: &CueSlot) -> bool {
    overlaps(
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        slot.x,
        slot.y,
        slot.width,
        slot.height,
    )
}

fn overlaps(ax: f64, ay: f64, aw: f64, ah: f64, bx: f64, by: f64, bw: f64, bh: f64) -> bool {
    let eps = FIT_EPSILON_MM;
    ax + aw > bx + eps && bx + bw > ax + eps && ay + ah > by + eps && by + bh > ay + eps
}

fn separated_y(left: &PlacedNode, right: &PlacedNode) -> bool {
    let eps = FIT_EPSILON_MM;
    left.y + left.height <= right.y + eps || right.y + right.height <= left.y + eps
}

fn overlaps_x(left: &PlacedNode, right: &PlacedNode) -> bool {
    let eps = FIT_EPSILON_MM;
    left.x + left.width > right.x + eps && right.x + right.width > left.x + eps
}

pub(crate) fn direct_count(boxes: &[NodeBox], direct_ids: &[&str]) -> usize {
    boxes
        .iter()
        .filter(|box_| direct_ids.contains(&box_.protocol_id.as_str()))
        .count()
}

pub(crate) fn finalize_pages(
    pages: &[UnnumberedPage],
    cues: &[PendingCue],
    links: &[PendingLink],
    measurer: &mut dyn Measurer,
) -> Result<Vec<PageSpec>, Error> {
    let page_count = pages.len();
    let mut cues_by_source: Vec<Vec<ContCue>> = vec![Vec::new(); page_count];
    for cue in cues {
        require_forward(
            cue.source_index,
            cue.target_index,
            page_count,
            "continuation cue",
        )?;
        cues_by_source[cue.source_index].push(ContCue {
            target_page: cue.target_index + 1,
            direct_count: cue.direct_count,
            side: cue.side,
            anchor_id: cue.anchor_id.clone(),
        });
    }
    let mut links_by_source: Vec<Vec<NodePageLink>> = vec![Vec::new(); page_count];
    let mut detail_by_source: Vec<Vec<(String, String)>> = vec![Vec::new(); page_count];
    for link in links {
        require_forward(
            link.source_index,
            link.target_index,
            page_count,
            "node page link",
        )?;
        let target_page = link.target_index + 1;
        detail_by_source[link.source_index]
            .push((link.node_id.clone(), overflow_detail_line(target_page)));
        links_by_source[link.source_index].push(NodePageLink {
            node_id: link.node_id.clone(),
            target_page,
        });
    }
    let mut out = Vec::with_capacity(page_count);
    for (index, page) in pages.iter().enumerate() {
        let mut boxes = Vec::with_capacity(page.boxes.len());
        for box_ in &page.boxes {
            if let Some((_, text)) = detail_by_source[index]
                .iter()
                .find(|(id, _)| id == &box_.protocol_id)
            {
                boxes.push(write_overflow_line(box_, text, measurer)?);
            } else {
                boxes.push(box_.clone());
            }
        }
        let main_title = if let Some(name) = &page.penetration_name {
            penetration_title(index + 1, name)
        } else {
            page.main_title.clone()
        };
        let page_cues = cues_by_source[index].clone();
        require_numbered_line_fits(&page_cues, &page.layout, measurer)?;
        let spec = PageSpec {
            main_title,
            sub_title: page.sub_title.clone(),
            boxes,
            layout: page.layout.clone(),
            geometry: page.geometry,
            cont_cues: page_cues,
            node_page_links: links_by_source[index].clone(),
            ratio_facts: page.ratio_facts.clone(),
        };
        spec.check()?;
        out.push(spec);
    }
    Ok(out)
}

fn require_numbered_line_fits(
    cues: &[ContCue],
    layout: &LayoutResult,
    measurer: &mut dyn Measurer,
) -> Result<(), Error> {
    for cue in cues {
        let Some(slot) = layout
            .slots
            .iter()
            .find(|slot| slot.anchor_id == cue.anchor_id && slot.side == cue.side)
        else {
            continue;
        };
        let (first, second) = cont_cue_lines(cue.target_page, cue.side);
        let text = measurer
            .string_width_mm(&first, CONT_CUE_TEXT_PT)?
            .max(measurer.string_width_mm(&second, CONT_CUE_TEXT_PT)?);
        if text + 2.0 * CONT_CUE_PAD_MM > slot.width + crate::units::FIT_EPSILON_MM {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!(
                    "continuation cue text exceeds its slot width for {}",
                    cue.anchor_id
                ),
            ));
        }
    }
    Ok(())
}

fn require_forward(
    source_index: usize,
    target_index: usize,
    page_count: usize,
    label: &str,
) -> Result<(), Error> {
    if source_index >= page_count {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            format!("{label} source exceeds page count"),
        ));
    }
    if !(source_index < target_index && target_index < page_count) {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            format!("{label} target must follow source"),
        ));
    }
    Ok(())
}

fn write_overflow_line(
    box_: &NodeBox,
    text: &str,
    measurer: &mut dyn Measurer,
) -> Result<NodeBox, Error> {
    let Some(last) = box_.lines.last() else {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            format!("overflow box {} is missing its reserved line", box_.id),
        ));
    };
    if !last.text.is_empty() {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            format!("overflow box {} is missing its reserved line", box_.id),
        ));
    }
    measurer.ensure_glyphs(text)?;
    let max_content = box_.width_mm - 2.0 * PADDING_X_MM;
    if measurer.string_width_mm(text, NAME_FONT_PT)? > max_content {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            format!("overflow detail exceeds node width for {}", box_.id),
        ));
    }
    let mut lines = box_.lines.clone();
    let Some(last) = lines.last_mut() else {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            format!("overflow box {} is missing its reserved line", box_.id),
        ));
    };
    text.clone_into(&mut last.text);
    Ok(NodeBox {
        lines,
        ..box_.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::{absorb_cue_slots, CueAnchorSize};
    use crate::captions::CueSide;
    use crate::ids::NodeId;
    use crate::page::{LayoutResult, PlacedNode};
    use crate::units::PageGeometry;

    fn node(
        index: usize,
        protocol_id: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> PlacedNode {
        PlacedNode {
            id: NodeId::from_index(index),
            protocol_id: protocol_id.to_owned(),
            x,
            y,
            width,
            height,
        }
    }

    fn placed_slot(nodes: Vec<PlacedNode>) -> crate::page::CueSlot {
        let anchor = CueAnchorSize {
            anchor_id: "anchor".to_owned(),
            side: CueSide::Shareholder,
            width_mm: 30.0,
            height_mm: 14.0,
        };
        let layout = LayoutResult::from_parts(nodes, Vec::new(), Vec::new()).expect("layout");
        let placed = absorb_cue_slots(
            &[anchor],
            move |_gap| Ok(layout.clone()),
            PageGeometry::default(),
        )
        .expect("place")
        .expect("slot fits");
        placed.slots.into_iter().next().expect("one slot")
    }

    #[test]
    fn a_cue_above_its_anchor_does_not_need_a_leader() {
        let slot = placed_slot(vec![node(0, "anchor", 100.0, 80.0, 40.0, 20.0)]);
        assert!(!slot.needs_leader);
        assert!(slot.y < 80.0);
    }

    #[test]
    fn a_cue_beside_its_anchor_needs_a_leader() {
        let slot = placed_slot(vec![
            node(0, "anchor", 100.0, 80.0, 40.0, 20.0),
            node(1, "block", 105.0, 63.0, 30.0, 14.0),
        ]);
        assert!(slot.needs_leader);
        assert!(slot.x + slot.width < 100.0);
    }
}
