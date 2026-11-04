//! Place ratio labels on hugging stubs. Drawing consumes [`PlacedRatioLabel`].

use std::collections::{HashMap, HashSet};

use crate::graph::NodeId;
use crate::page::{EdgeRef, LaidOutEdge, PageSpec, PlacedNode, PointMm, RatioHug};
use crate::units::{RATIO_LABEL_GAP_Y_MM, RATIO_LABEL_OFFSET_X_MM};

/// How far a stub point may sit from the box edge and still count as attached.
const ATTACH_TOLERANCE_MM: f64 = 0.05;

/// One ratio label in layout millimetres. `y` grows downward.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedRatioLabel {
    /// Node the percent belongs to.
    pub node_id: NodeId,
    /// `xx.xx%`.
    pub text: String,
    /// Stub x plus [`RATIO_LABEL_OFFSET_X_MM`].
    pub x_mm: f64,
    /// Glyph baseline. The page origin is added only when painting.
    pub baseline_y_mm: f64,
}

/// Ascent and descent at the ratio size, in mm.
///
/// `descent_mm` is the positive distance below the baseline. It excludes the
/// 2 pt line gap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RatioMetrics {
    /// Baseline to ascender.
    pub ascent_mm: f64,
    /// Baseline to descender.
    pub descent_mm: f64,
}

/// Labels for facts that hug a stub on this page.
///
/// The same `node_id` keeps the first fact. A missing node, a page subject,
/// and a fact with no hugging stub produce no label.
#[must_use]
pub(crate) fn place_ratio_labels(page: &PageSpec, metrics: RatioMetrics) -> Vec<PlacedRatioLabel> {
    let nodes: HashMap<NodeId, &PlacedNode> = page
        .layout
        .nodes
        .iter()
        .map(|node| (node.id, node))
        .collect();
    let boxed: HashMap<NodeId, bool> = page
        .boxes
        .iter()
        .map(|box_| (box_.id, box_.is_subject))
        .collect();
    let mut seen = HashSet::new();
    let mut labels = Vec::new();
    for anchor in page.ratio_anchors() {
        if !seen.insert(anchor.node_id) {
            continue;
        }
        let Some(node) = nodes.get(&anchor.node_id) else {
            continue;
        };
        if boxed.get(&anchor.node_id).copied() != Some(false) {
            continue;
        }
        let Some(stub_x) = hugging_stub_x(
            node,
            anchor.hug,
            anchor.preferred_parent.as_ref(),
            &page.layout.edges,
        ) else {
            continue;
        };
        labels.push(PlacedRatioLabel {
            node_id: anchor.node_id,
            text: anchor.text,
            x_mm: stub_x + RATIO_LABEL_OFFSET_X_MM,
            baseline_y_mm: baseline_y(node, anchor.hug, metrics),
        });
    }
    labels
}

fn baseline_y(node: &PlacedNode, hug: RatioHug, metrics: RatioMetrics) -> f64 {
    match hug {
        RatioHug::Bottom => node.y + node.height + metrics.ascent_mm + RATIO_LABEL_GAP_Y_MM,
        RatioHug::Top => node.y - metrics.descent_mm - RATIO_LABEL_GAP_Y_MM,
    }
}

fn hugging_stub_x(
    node: &PlacedNode,
    hug: RatioHug,
    preferred: Option<&EdgeRef>,
    edges: &[LaidOutEdge],
) -> Option<f64> {
    let candidates = ordered_edges(edges, node.id, hug, preferred);
    for edge in &candidates {
        if let Some(x_mm) = vertical_stub_x(node, hug, edge) {
            return Some(x_mm);
        }
    }
    if hug == RatioHug::Bottom {
        for edge in &candidates {
            if let Some(x_mm) = side_stub_x(node, edge) {
                return Some(x_mm);
            }
        }
    }
    None
}

fn ordered_edges<'a>(
    edges: &'a [LaidOutEdge],
    node_id: NodeId,
    hug: RatioHug,
    preferred: Option<&EdgeRef>,
) -> Vec<&'a LaidOutEdge> {
    let mut found: Vec<&LaidOutEdge> = edges
        .iter()
        .filter(|edge| match hug {
            RatioHug::Bottom => edge.from_id == node_id,
            RatioHug::Top => edge.to_id == node_id,
        })
        .collect();
    if preferred.is_some() {
        found.sort_by(|left, right| {
            let rank = |edge: &LaidOutEdge| u8::from(!is_preferred(edge, preferred));
            rank(left).cmp(&rank(right))
        });
    }
    found
}

fn is_preferred(edge: &LaidOutEdge, preferred: Option<&EdgeRef>) -> bool {
    preferred
        .is_some_and(|preferred| edge.from_id == preferred.from_id && edge.to_id == preferred.to_id)
}

fn vertical_stub_x(node: &PlacedNode, hug: RatioHug, edge: &LaidOutEdge) -> Option<f64> {
    let points = edge.points.as_slice();
    match hug {
        RatioHug::Bottom => {
            let (start, rest) = points.split_first()?;
            let next = *rest.first()?;
            if on_bottom_center(node, *start) && vertical_segment(*start, next) {
                Some(start.x)
            } else {
                None
            }
        }
        RatioHug::Top => {
            let (end, rest) = points.split_last()?;
            let previous = *rest.last()?;
            if on_top_center(node, *end) && vertical_segment(previous, *end) {
                Some(end.x)
            } else {
                None
            }
        }
    }
}

fn side_stub_x(node: &PlacedNode, edge: &LaidOutEdge) -> Option<f64> {
    let (start, rest) = edge.points.split_first()?;
    let next = *rest.first()?;
    if on_side_midline(node, *start) && horizontal_segment(*start, next) {
        Some(start.x)
    } else {
        None
    }
}

fn on_bottom_center(node: &PlacedNode, point: PointMm) -> bool {
    attached(point.x, node.x + node.width / 2.0) && attached(point.y, node.y + node.height)
}

fn on_top_center(node: &PlacedNode, point: PointMm) -> bool {
    attached(point.x, node.x + node.width / 2.0) && attached(point.y, node.y)
}

fn on_side_midline(node: &PlacedNode, point: PointMm) -> bool {
    let mid_y = node.y + node.height / 2.0;
    let on_edge = attached(point.x, node.x) || attached(point.x, node.x + node.width);
    attached(point.y, mid_y) && on_edge
}

fn vertical_segment(from: PointMm, to: PointMm) -> bool {
    attached(from.x, to.x) && (from.y - to.y).abs() > ATTACH_TOLERANCE_MM
}

fn horizontal_segment(from: PointMm, to: PointMm) -> bool {
    attached(from.y, to.y) && (from.x - to.x).abs() > ATTACH_TOLERANCE_MM
}

fn attached(left: f64, right: f64) -> bool {
    (left - right).abs() <= ATTACH_TOLERANCE_MM
}

#[cfg(test)]
#[path = "ratio_labels_tests.rs"]
mod ratio_labels_tests;
