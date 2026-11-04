//! Ratio label placement on hugging stubs.

use super::{place_ratio_labels, RatioMetrics};
use crate::content::{EdgeRef, LabelHug, RatioFact};
use crate::graph::{NodeId, NodeOrigin};
use crate::page::NodeBox;
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

fn node(index: usize, x: f64, y: f64, width: f64, height: f64) -> PlacedNode {
    PlacedNode {
        id: id(index),
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

fn fact(index: usize, text: &str, hug: LabelHug, preferred: Option<(usize, usize)>) -> RatioFact {
    RatioFact {
        node_id: id(index),
        ratio: 40.0,
        text: text.to_owned(),
        hug,
        preferred_parent: preferred.map(|(from, to)| EdgeRef::new(id(from), id(to))),
    }
}

fn measured(index: usize, protocol: &str, is_subject: bool) -> NodeBox {
    NodeBox {
        id: id(index),
        origin: NodeOrigin::Caller {
            index,
            id: protocol.to_owned(),
            stands_in_for: None,
        },
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
    let holder = node(0, 0.0, 0.0, 20.0, 10.0);
    let root = node(1, 0.0, 30.0, 20.0, 10.0);
    let invest = node(2, 0.0, 60.0, 20.0, 10.0);
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
            fact(0, "40.00%", LabelHug::Bottom, None),
            fact(1, "50.00%", LabelHug::Bottom, None),
            fact(2, "100.00%", LabelHug::Top, None),
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
            node(0, 0.0, 0.0, 20.0, 10.0),
            node(1, 40.0, 0.0, 20.0, 10.0),
            node(2, 20.0, 30.0, 20.0, 10.0),
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
            fact(0, "40.00%", LabelHug::Bottom, None),
            fact(1, "60.00%", LabelHug::Bottom, None),
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
            node(0, 20.0, 0.0, 20.0, 10.0),
            node(1, 0.0, 40.0, 20.0, 10.0),
            node(2, 40.0, 40.0, 20.0, 10.0),
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
            fact(1, "40.00%", LabelHug::Top, None),
            fact(2, "60.00%", LabelHug::Top, None),
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
        vec![node(0, 0.0, 0.0, 20.0, 10.0)],
        vec![edge(0, 1, vec![point(100.0, 100.0), point(120.0, 100.0)])],
        vec![measured(0, "holder", false)],
        vec![fact(0, "40.00%", LabelHug::Bottom, None)],
    );
    assert!(place_ratio_labels(&laid, METRICS).is_empty());
}

#[test]
fn a_duplicate_fact_keeps_the_first_label() {
    let laid = page(
        vec![node(0, 0.0, 0.0, 20.0, 10.0)],
        vec![edge(0, 1, vec![point(10.0, 10.0), point(10.0, 30.0)])],
        vec![measured(0, "holder", false)],
        vec![
            fact(0, "40.00%", LabelHug::Bottom, None),
            fact(0, "99.00%", LabelHug::Bottom, None),
        ],
    );
    let labels = place_ratio_labels(&laid, METRICS);
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].text, "40.00%");
}

#[test]
fn a_side_exit_uses_the_segment_start() {
    let laid = page(
        vec![node(0, 0.0, 0.0, 20.0, 10.0)],
        vec![edge(0, 1, vec![point(20.0, 5.0), point(40.0, 5.0)])],
        vec![measured(0, "holder", false)],
        vec![fact(0, "40.00%", LabelHug::Bottom, None)],
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
        vec![fact(3, "40.00%", LabelHug::Bottom, None)],
    );
    assert!(place_ratio_labels(&laid, METRICS).is_empty());
}

#[test]
fn preferred_parent_is_searched_before_the_other_stub() {
    let laid = page(
        vec![
            node(0, 0.0, 0.0, 20.0, 10.0),
            node(1, 0.0, 30.0, 20.0, 10.0),
            node(2, 0.0, 30.0, 20.0, 10.0),
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
        vec![fact(0, "40.00%", LabelHug::Bottom, Some((0, 1)))],
    );
    let labels = place_ratio_labels(&laid, METRICS);
    assert_eq!(labels.len(), 1);
    near(labels[0].x_mm, 11.2);
}

#[test]
fn an_investment_side_exit_places_nothing() {
    let laid = page(
        vec![node(1, 0.0, 40.0, 20.0, 10.0)],
        vec![edge(0, 1, vec![point(20.0, 45.0), point(0.0, 45.0)])],
        vec![measured(1, "invest", false)],
        vec![fact(1, "100.00%", LabelHug::Top, None)],
    );
    assert!(place_ratio_labels(&laid, METRICS).is_empty());
}
