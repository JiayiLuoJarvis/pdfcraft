//! Place ratio labels on hugging stubs. Drawing consumes [`PlacedRatioLabel`].

use std::collections::{HashMap, HashSet};

use crate::ids::NodeId;
use crate::page::{LaidOutEdge, PageSpec, PlacedNode, PointMm, PreferredParent, RatioHug};
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
    let names: HashMap<NodeId, &str> = page
        .layout
        .nodes
        .iter()
        .map(|node| (node.id, node.protocol_id.as_str()))
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
            &names,
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
    preferred: Option<&PreferredParent>,
    edges: &[LaidOutEdge],
    names: &HashMap<NodeId, &str>,
) -> Option<f64> {
    let candidates = ordered_edges(edges, node.id, hug, preferred, names);
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
    preferred: Option<&PreferredParent>,
    names: &HashMap<NodeId, &str>,
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
            let rank = |edge: &LaidOutEdge| u8::from(!is_preferred(edge, preferred, names));
            rank(left).cmp(&rank(right))
        });
    }
    found
}

fn is_preferred(
    edge: &LaidOutEdge,
    preferred: Option<&PreferredParent>,
    names: &HashMap<NodeId, &str>,
) -> bool {
    let Some(preferred) = preferred else {
        return false;
    };
    names.get(&edge.from_id).copied() == Some(preferred.from_id.as_str())
        && names.get(&edge.to_id).copied() == Some(preferred.to_id.as_str())
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
mod tests {
    use super::{place_ratio_labels, RatioMetrics};
    use crate::display::{LabelHug, ParentEdge, RatioFact};
    use crate::ids::NodeId;
    use crate::measure::NodeBox;
    use crate::page::{LaidOutEdge, LayoutResult, PageSpec, PlacedNode, PointMm};
    use crate::units::PageGeometry;

    const METRICS: RatioMetrics = RatioMetrics {
        ascent_mm: 4.0,
        descent_mm: 1.0,
    };

    fn near(actual: f64, expected: f64) {
        let gap = (actual - expected).abs();
        assert!(gap <= 1e-4, "{actual} vs {expected}");
    }

    fn id(index: usize) -> NodeId {
        NodeId::from_index(index)
    }

    fn node(index: usize, protocol: &str, x: f64, y: f64, width: f64, height: f64) -> PlacedNode {
        PlacedNode {
            id: id(index),
            protocol_id: protocol.to_owned(),
            x,
            y,
            width,
            height,
        }
    }

    fn edge(from: usize, to: usize, points: Vec<PointMm>) -> LaidOutEdge {
        LaidOutEdge {
            from_id: id(from),
            to_id: id(to),
            points,
            draw_arrow: true,
        }
    }

    fn point(x: f64, y: f64) -> PointMm {
        PointMm { x, y }
    }

    fn fact(
        index: usize,
        protocol: &str,
        text: &str,
        hug: LabelHug,
        preferred: Option<(&str, &str)>,
    ) -> RatioFact {
        RatioFact {
            node_id: id(index),
            protocol_id: protocol.to_owned(),
            ratio: 40.0,
            text: text.to_owned(),
            hug,
            preferred_parent: preferred.map(|(from_id, to_id)| ParentEdge {
                from_id: from_id.to_owned(),
                to_id: to_id.to_owned(),
            }),
        }
    }

    fn measured(index: usize, protocol: &str, is_subject: bool) -> NodeBox {
        NodeBox {
            id: id(index),
            protocol_id: protocol.to_owned(),
            is_overflow: false,
            width_mm: 20.0,
            height_mm: 10.0,
            lines: Vec::new(),
            is_subject,
            is_company: true,
        }
    }

    fn page(
        nodes: Vec<PlacedNode>,
        edges: Vec<LaidOutEdge>,
        boxes: Vec<NodeBox>,
        facts: Vec<RatioFact>,
    ) -> PageSpec {
        PageSpec {
            main_title: String::new(),
            sub_title: String::new(),
            boxes,
            layout: LayoutResult::from_parts(nodes, edges, Vec::new()).expect("layout"),
            geometry: PageGeometry::portrait(8.0),
            cont_cues: Vec::new(),
            node_page_links: Vec::new(),
            ratio_facts: facts,
        }
    }

    #[test]
    fn one_vertical_edge_places_both_ends_and_skips_the_subject() {
        let holder = node(0, "holder", 0.0, 0.0, 20.0, 10.0);
        let root = node(1, "root", 0.0, 30.0, 20.0, 10.0);
        let invest = node(2, "invest", 0.0, 60.0, 20.0, 10.0);
        let laid = page(
            vec![holder, root, invest],
            vec![
                edge(0, 1, vec![point(10.0, 10.0), point(10.0, 30.0)]),
                edge(1, 2, vec![point(10.0, 40.0), point(10.0, 60.0)]),
            ],
            vec![
                measured(0, "holder", false),
                measured(1, "root", true),
                measured(2, "invest", false),
            ],
            vec![
                fact(0, "holder", "40.00%", LabelHug::Bottom, None),
                fact(1, "root", "50.00%", LabelHug::Bottom, None),
                fact(2, "invest", "100.00%", LabelHug::Top, None),
            ],
        );
        let labels = place_ratio_labels(&laid, METRICS);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].text, "40.00%");
        near(labels[0].x_mm, 11.2);
        near(labels[0].baseline_y_mm, 14.4);
        assert_eq!(labels[1].text, "100.00%");
        near(labels[1].x_mm, 11.2);
        near(labels[1].baseline_y_mm, 58.6);
        assert!(labels.iter().all(|label| label.text != "50.00%"));
    }

    #[test]
    fn two_shareholders_use_their_stubs_not_the_trunk() {
        let laid = page(
            vec![
                node(0, "left", 0.0, 0.0, 20.0, 10.0),
                node(1, "right", 40.0, 0.0, 20.0, 10.0),
                node(2, "root", 20.0, 30.0, 20.0, 10.0),
            ],
            vec![
                edge(
                    0,
                    2,
                    vec![
                        point(10.0, 10.0),
                        point(10.0, 20.0),
                        point(30.0, 20.0),
                        point(30.0, 30.0),
                    ],
                ),
                edge(
                    1,
                    2,
                    vec![point(50.0, 10.0), point(50.0, 20.0), point(30.0, 20.0)],
                ),
            ],
            vec![
                measured(0, "left", false),
                measured(1, "right", false),
                measured(2, "root", true),
            ],
            vec![
                fact(0, "left", "40.00%", LabelHug::Bottom, None),
                fact(1, "right", "60.00%", LabelHug::Bottom, None),
            ],
        );
        let labels = place_ratio_labels(&laid, METRICS);
        assert_eq!(labels.len(), 2);
        near(labels[0].x_mm, 11.2);
        near(labels[1].x_mm, 51.2);
        assert!(labels.iter().all(|label| (label.x_mm - 31.2).abs() > 1e-4));
    }

    #[test]
    fn two_investments_share_the_baseline() {
        let laid = page(
            vec![
                node(0, "root", 20.0, 0.0, 20.0, 10.0),
                node(1, "left", 0.0, 40.0, 20.0, 10.0),
                node(2, "right", 40.0, 40.0, 20.0, 10.0),
            ],
            vec![
                edge(
                    0,
                    1,
                    vec![
                        point(30.0, 10.0),
                        point(30.0, 20.0),
                        point(10.0, 20.0),
                        point(10.0, 40.0),
                    ],
                ),
                edge(
                    0,
                    2,
                    vec![point(30.0, 20.0), point(50.0, 20.0), point(50.0, 40.0)],
                ),
            ],
            vec![
                measured(0, "root", true),
                measured(1, "left", false),
                measured(2, "right", false),
            ],
            vec![
                fact(1, "left", "40.00%", LabelHug::Top, None),
                fact(2, "right", "60.00%", LabelHug::Top, None),
            ],
        );
        let labels = place_ratio_labels(&laid, METRICS);
        assert_eq!(labels.len(), 2);
        near(labels[0].x_mm, 11.2);
        near(labels[1].x_mm, 51.2);
        near(labels[0].baseline_y_mm, 38.6);
        near(labels[1].baseline_y_mm, 38.6);
    }

    #[test]
    fn a_fact_with_no_hugging_stub_places_nothing() {
        let laid = page(
            vec![node(0, "holder", 0.0, 0.0, 20.0, 10.0)],
            vec![edge(0, 1, vec![point(100.0, 100.0), point(120.0, 100.0)])],
            vec![measured(0, "holder", false)],
            vec![fact(0, "holder", "40.00%", LabelHug::Bottom, None)],
        );
        assert!(place_ratio_labels(&laid, METRICS).is_empty());
    }

    #[test]
    fn a_duplicate_fact_keeps_the_first_label() {
        let laid = page(
            vec![node(0, "holder", 0.0, 0.0, 20.0, 10.0)],
            vec![edge(0, 1, vec![point(10.0, 10.0), point(10.0, 30.0)])],
            vec![measured(0, "holder", false)],
            vec![
                fact(0, "holder", "40.00%", LabelHug::Bottom, None),
                fact(0, "holder", "99.00%", LabelHug::Bottom, None),
            ],
        );
        let labels = place_ratio_labels(&laid, METRICS);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "40.00%");
    }

    #[test]
    fn a_side_exit_uses_the_segment_start() {
        let laid = page(
            vec![node(0, "holder", 0.0, 0.0, 20.0, 10.0)],
            vec![edge(0, 1, vec![point(20.0, 5.0), point(40.0, 5.0)])],
            vec![measured(0, "holder", false)],
            vec![fact(0, "holder", "40.00%", LabelHug::Bottom, None)],
        );
        let labels = place_ratio_labels(&laid, METRICS);
        assert_eq!(labels.len(), 1);
        near(labels[0].x_mm, 21.2);
        near(
            labels[0].baseline_y_mm,
            10.0 + METRICS.ascent_mm + crate::units::RATIO_LABEL_GAP_Y_MM,
        );
    }

    #[test]
    fn a_missing_node_places_nothing() {
        let laid = page(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![fact(3, "gone", "40.00%", LabelHug::Bottom, None)],
        );
        assert!(place_ratio_labels(&laid, METRICS).is_empty());
    }

    #[test]
    fn preferred_parent_is_searched_before_the_other_stub() {
        let laid = page(
            vec![
                node(0, "holder", 0.0, 0.0, 20.0, 10.0),
                node(1, "root", 0.0, 30.0, 20.0, 10.0),
                node(2, "other", 0.0, 30.0, 20.0, 10.0),
            ],
            vec![
                edge(0, 2, vec![point(10.04, 10.0), point(10.04, 30.0)]),
                edge(0, 1, vec![point(10.0, 10.0), point(10.0, 30.0)]),
            ],
            vec![
                measured(0, "holder", false),
                measured(1, "root", true),
                measured(2, "other", false),
            ],
            vec![fact(
                0,
                "holder",
                "40.00%",
                LabelHug::Bottom,
                Some(("holder", "root")),
            )],
        );
        let labels = place_ratio_labels(&laid, METRICS);
        assert_eq!(labels.len(), 1);
        near(labels[0].x_mm, 11.2);
    }

    #[test]
    fn an_investment_side_exit_places_nothing() {
        let laid = page(
            vec![node(1, "invest", 0.0, 40.0, 20.0, 10.0)],
            vec![edge(0, 1, vec![point(20.0, 45.0), point(0.0, 45.0)])],
            vec![measured(1, "invest", false)],
            vec![fact(1, "invest", "100.00%", LabelHug::Top, None)],
        );
        assert!(place_ratio_labels(&laid, METRICS).is_empty());
    }
}
