//! Row wrapping and gaps from the production layout engine.

use crate::graph::{NodeId, NodeOrigin};
use crate::layout::{LayoutEngine, LayoutInput, WrappedEngine};
use crate::page::{EdgeRef, LayoutResult, NodeBox, PlacedNode, PointMm};
use crate::units::{FAMILY_GAP_MM, NODE_SPACING_MM, ROW_GAP_MM};

fn fixed_box(index: usize, protocol: &str, width: f64, height: f64, subject: bool) -> NodeBox {
    NodeBox {
        id: NodeId::from_index(index),
        origin: NodeOrigin::Caller {
            index,
            id: protocol.to_owned(),
            stands_in_for: None,
        },
        width_mm: width,
        height_mm: height,
        lines: Vec::new(),
        is_subject: subject,
        is_company: true,
    }
}

type Ids = std::collections::HashMap<String, NodeId>;

fn place_chart(
    boxes: &[NodeBox],
    pairs: &[(&str, &str)],
    printable_width_mm: f64,
) -> (LayoutResult, Ids) {
    let ids: Ids = boxes
        .iter()
        .map(|box_| (box_.origin.to_string(), box_.id))
        .collect();
    let edges: Vec<EdgeRef> = pairs
        .iter()
        .map(|(from, to)| EdgeRef::new(ids[*from], ids[*to]))
        .collect();
    let order: Vec<NodeId> = boxes.iter().map(|box_| box_.id).collect();
    let layout = WrappedEngine
        .layout(&LayoutInput {
            boxes,
            edges: &edges,
            node_order: &order,
            row_gap_mm: ROW_GAP_MM,
            printable_width_mm,
        })
        .expect("layout");
    (layout, ids)
}

fn node_at<'a>(layout: &'a LayoutResult, ids: &Ids, protocol: &str) -> &'a PlacedNode {
    let id = ids[protocol];
    layout
        .nodes
        .iter()
        .find(|node| node.id == id)
        .expect(protocol)
}

#[test]
fn seven_shareholders_wrap_into_three_rows_with_one_arrow() {
    let mut boxes = Vec::new();
    for index in 1..=7 {
        boxes.push(fixed_box(index, &format!("s{index}"), 42.0, 12.0, false));
    }
    boxes.push(fixed_box(0, "root", 42.0, 14.0, true));
    let (layout, ids) = place_chart(
        &boxes,
        &[
            ("s1", "root"),
            ("s2", "root"),
            ("s3", "root"),
            ("s4", "root"),
            ("s5", "root"),
            ("s6", "root"),
            ("s7", "root"),
        ],
        3.0 * 42.0 + 2.0 * NODE_SPACING_MM,
    );
    let root = node_at(&layout, &ids, "root");
    let mut rows: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for index in 1..=7 {
        let node = node_at(&layout, &ids, &format!("s{index}"));
        assert!(node.y < root.y);
        *rows.entry(node.y.to_bits()).or_default() += 1;
    }
    let lengths: Vec<usize> = rows.into_values().collect();
    assert_eq!(lengths, vec![3, 2, 2]);
    let into_root: Vec<_> = layout
        .edges
        .iter()
        .filter(|edge| edge.to_id == root.id)
        .collect();
    assert_eq!(into_root.len(), 7);
    assert_eq!(into_root.iter().filter(|edge| edge.draw_arrow).count(), 1);
    assert!(layout.width_mm <= 3.0 * 42.0 + 3.0 * NODE_SPACING_MM + 1.0);
}

#[test]
fn shallow_siblings_keep_a_twelve_millimetre_gap_and_aligned_centers() {
    let boxes = vec![
        fixed_box(0, "root", 42.0, 14.0, true),
        fixed_box(1, "p1", 42.0, 12.0, false),
        fixed_box(2, "p2", 42.0, 12.0, false),
        fixed_box(3, "a1", 42.0, 12.0, false),
        fixed_box(4, "b1", 42.0, 12.0, false),
    ];
    let (layout, ids) = place_chart(
        &boxes,
        &[("p1", "root"), ("p2", "root"), ("a1", "p1"), ("b1", "p2")],
        800.0,
    );
    let p1 = node_at(&layout, &ids, "p1");
    let p2 = node_at(&layout, &ids, "p2");
    let gap = p2.x - (p1.x + p1.width);
    assert!((gap - 12.0).abs() < 0.05, "shallow gap {gap}");
    let a1 = node_at(&layout, &ids, "a1");
    let b1 = node_at(&layout, &ids, "b1");
    assert!((a1.x + a1.width / 2.0 - (p1.x + p1.width / 2.0)).abs() < 1.0);
    assert!((b1.x + b1.width / 2.0 - (p2.x + p2.width / 2.0)).abs() < 1.0);
}

#[test]
fn a_deep_child_column_stays_centered_on_its_parent() {
    let boxes = vec![
        fixed_box(0, "root", 42.0, 14.0, true),
        fixed_box(1, "p1", 42.0, 12.0, false),
        fixed_box(2, "p2", 42.0, 12.0, false),
        fixed_box(3, "a1", 42.0, 12.0, false),
        fixed_box(4, "b1", 42.0, 12.0, false),
        fixed_box(5, "b2", 42.0, 12.0, false),
        fixed_box(6, "b3", 42.0, 12.0, false),
    ];
    let (layout, ids) = place_chart(
        &boxes,
        &[
            ("p1", "root"),
            ("p2", "root"),
            ("a1", "p1"),
            ("b1", "p2"),
            ("b2", "p2"),
            ("b3", "p2"),
        ],
        800.0,
    );
    let p1 = node_at(&layout, &ids, "p1");
    let a1 = node_at(&layout, &ids, "a1");
    let b1 = node_at(&layout, &ids, "b1");
    assert!(a1.x + a1.width < b1.x);
    assert!((a1.x + a1.width / 2.0 - (p1.x + p1.width / 2.0)).abs() < 1.0);
    let p2 = node_at(&layout, &ids, "p2");
    let b2 = node_at(&layout, &ids, "b2");
    let b3 = node_at(&layout, &ids, "b3");
    assert!((b1.y - b2.y).abs() < 0.01);
    assert!((b2.y - b3.y).abs() < 0.01);
    let row_center = (b1.x + b3.x + b3.width) / 2.0;
    let parent_center = p2.x + p2.width / 2.0;
    assert!(
        (row_center - parent_center).abs() < 1.0,
        "row {row_center} parent {parent_center}"
    );
}

#[test]
fn plain_shareholders_use_a_six_millimetre_gap() {
    let boxes = vec![
        fixed_box(1, "s1", 42.0, 12.0, false),
        fixed_box(2, "s2", 42.0, 12.0, false),
        fixed_box(3, "s3", 42.0, 12.0, false),
        fixed_box(0, "root", 42.0, 14.0, true),
    ];
    let (layout, ids) = place_chart(
        &boxes,
        &[("s1", "root"), ("s2", "root"), ("s3", "root")],
        800.0,
    );
    let s1 = node_at(&layout, &ids, "s1");
    let s2 = node_at(&layout, &ids, "s2");
    let gap = s2.x - (s1.x + s1.width);
    assert!((gap - NODE_SPACING_MM).abs() < 0.05, "plain gap {gap}");
}

#[test]
fn shareholder_outers_sit_above_leaves() {
    let boxes = vec![
        fixed_box(1, "p1", 42.0, 12.0, false),
        fixed_box(2, "p2", 42.0, 12.0, false),
        fixed_box(3, "s05", 42.0, 12.0, false),
        fixed_box(4, "s06", 42.0, 12.0, false),
        fixed_box(5, "s12", 42.0, 12.0, false),
        fixed_box(6, "s05a", 42.0, 12.0, false),
        fixed_box(7, "s05b", 42.0, 12.0, false),
        fixed_box(8, "s06a", 42.0, 12.0, false),
        fixed_box(9, "s06b", 42.0, 12.0, false),
        fixed_box(10, "s12a", 42.0, 12.0, false),
        fixed_box(11, "s12b", 42.0, 12.0, false),
        fixed_box(0, "root", 42.0, 14.0, true),
    ];
    let (layout, ids) = place_chart(
        &boxes,
        &[
            ("p1", "root"),
            ("p2", "root"),
            ("s05", "root"),
            ("s06", "root"),
            ("s12", "root"),
            ("s05a", "s05"),
            ("s05b", "s05"),
            ("s06a", "s06"),
            ("s06b", "s06"),
            ("s12a", "s12"),
            ("s12b", "s12"),
        ],
        3.0 * 42.0 + 2.0 * NODE_SPACING_MM,
    );
    let outer_bottom = ["s05", "s06", "s12"]
        .into_iter()
        .map(|id| {
            let node = node_at(&layout, &ids, id);
            node.y + node.height
        })
        .fold(f64::NEG_INFINITY, f64::max);
    let leaf_top = node_at(&layout, &ids, "p1")
        .y
        .min(node_at(&layout, &ids, "p2").y);
    assert!(
        outer_bottom + 0.5 < leaf_top,
        "{outer_bottom} vs {leaf_top}"
    );
    assert!(node_at(&layout, &ids, "s05a").y < node_at(&layout, &ids, "s05").y);
    assert!(node_at(&layout, &ids, "s05").y < node_at(&layout, &ids, "root").y);
}

fn row_counts(layout: &LayoutResult, ids: &Ids, names: &[&str]) -> Vec<usize> {
    let mut rows: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for name in names {
        let node = node_at(layout, ids, name);
        *rows.entry(node.y.to_bits()).or_default() += 1;
    }
    rows.into_values().collect()
}

fn open_segment_hits_interior(start: PointMm, end: PointMm, node: &PlacedNode) -> bool {
    let left = node.x + 0.5;
    let right = node.x + node.width - 0.5;
    let top = node.y + 0.5;
    let bottom = node.y + node.height - 0.5;
    if (start.x - end.x).abs() < 0.01 {
        let x = start.x;
        if x <= left || x >= right {
            return false;
        }
        let lo = start.y.min(end.y);
        let hi = start.y.max(end.y);
        lo < bottom && hi > top
    } else if (start.y - end.y).abs() < 0.01 {
        let y = start.y;
        if y <= top || y >= bottom {
            return false;
        }
        let lo = start.x.min(end.x);
        let hi = start.x.max(end.x);
        lo < right && hi > left
    } else {
        false
    }
}

fn wires_miss_foreign_boxes(layout: &LayoutResult) {
    for edge in &layout.edges {
        for pair in edge.points.windows(2) {
            for node in &layout.nodes {
                if node.id == edge.from_id || node.id == edge.to_id {
                    continue;
                }
                assert!(
                    !open_segment_hits_interior(pair[0], pair[1], node),
                    "edge {}->{} hits node {}",
                    edge.from_id.index(),
                    edge.to_id.index(),
                    node.id.index()
                );
            }
        }
    }
}

#[test]
fn mixed_widths_balance_by_span_not_count() {
    let boxes = vec![
        fixed_box(1, "s1", 32.0, 12.0, false),
        fixed_box(2, "s2", 32.0, 12.0, false),
        fixed_box(3, "s3", 60.0, 12.0, false),
        fixed_box(4, "s4", 32.0, 12.0, false),
        fixed_box(0, "root", 42.0, 14.0, true),
    ];
    let (layout, ids) = place_chart(
        &boxes,
        &[
            ("s1", "root"),
            ("s2", "root"),
            ("s3", "root"),
            ("s4", "root"),
        ],
        32.0 + 32.0 + NODE_SPACING_MM,
    );
    assert_eq!(
        row_counts(&layout, &ids, &["s1", "s2", "s3", "s4"]),
        vec![2, 1, 1]
    );
    let first = node_at(&layout, &ids, "s1");
    let second = node_at(&layout, &ids, "s2");
    assert!((first.y - second.y).abs() < 0.01);
    assert!(node_at(&layout, &ids, "s3").width > 50.0);
}

#[test]
fn family_bands_raise_row_counts_until_the_layer_fits() {
    let mut boxes = vec![
        fixed_box(0, "root", 42.0, 14.0, true),
        fixed_box(1, "p1", 42.0, 12.0, false),
        fixed_box(2, "p2", 42.0, 12.0, false),
    ];
    for index in 1..=3 {
        boxes.push(fixed_box(
            3 + index,
            &format!("a{index}"),
            42.0,
            12.0,
            false,
        ));
        boxes.push(fixed_box(
            6 + index,
            &format!("b{index}"),
            42.0,
            12.0,
            false,
        ));
    }
    let (layout, ids) = place_chart(
        &boxes,
        &[
            ("p1", "root"),
            ("p2", "root"),
            ("a1", "p1"),
            ("a2", "p1"),
            ("a3", "p1"),
            ("b1", "p2"),
            ("b2", "p2"),
            ("b3", "p2"),
        ],
        90.0 + FAMILY_GAP_MM + 90.0,
    );
    assert_eq!(row_counts(&layout, &ids, &["a1", "a2", "a3"]), vec![2, 1]);
    assert_eq!(row_counts(&layout, &ids, &["b1", "b2", "b3"]), vec![2, 1]);
}

#[test]
fn mixed_width_investments_do_not_route_through_boxes() {
    let boxes = vec![
        fixed_box(0, "root", 42.0, 14.0, true),
        fixed_box(1, "i1", 60.0, 20.0, false),
        fixed_box(2, "i2", 60.0, 20.0, false),
        fixed_box(3, "i3", 32.0, 12.0, false),
        fixed_box(4, "i4", 32.0, 12.0, false),
    ];
    let (layout, ids) = place_chart(
        &boxes,
        &[
            ("root", "i1"),
            ("root", "i2"),
            ("root", "i3"),
            ("root", "i4"),
        ],
        32.0 + 32.0 + NODE_SPACING_MM,
    );
    assert!(row_counts(&layout, &ids, &["i1", "i2", "i3", "i4"]).len() >= 2);
    wires_miss_foreign_boxes(&layout);
    let root = node_at(&layout, &ids, "root");
    for name in ["i3", "i4"] {
        let dest = node_at(&layout, &ids, name);
        let edge = layout
            .edges
            .iter()
            .find(|edge| edge.to_id == dest.id)
            .expect(name);
        if edge.points.len() >= 4 {
            let entry_y = edge.points[edge.points.len() - 2].y;
            assert!(
                dest.y - entry_y > 3.6,
                "{name} entry_y {entry_y} dest {}",
                dest.y
            );
        }
        assert!(dest.y > root.y + root.height);
    }
}

#[test]
fn stacked_unequal_widths_align_centers_and_miss_boxes() {
    let boxes = vec![
        fixed_box(1, "s1", 32.0, 12.0, false),
        fixed_box(2, "s2", 48.0, 12.0, false),
        fixed_box(0, "root", 48.0, 14.0, true),
    ];
    let (layout, ids) = place_chart(&boxes, &[("s1", "root"), ("s2", "root")], 50.0);
    let s1 = node_at(&layout, &ids, "s1");
    let s2 = node_at(&layout, &ids, "s2");
    assert!((s1.y - s2.y).abs() > 1.0);
    assert!(((s1.x + s1.width / 2.0) - (s2.x + s2.width / 2.0)).abs() < 0.05);
    assert!((s1.x - s2.x).abs() > 1.0);
    wires_miss_foreign_boxes(&layout);
}
