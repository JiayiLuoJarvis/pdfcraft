//! Row wrapping and gaps from the production layout engine.

use crate::graph::NodeId;
use crate::layout::{LayoutEngine, LayoutInput, WrappedEngine};
use crate::page::{EdgeRef, LayoutResult, NodeBox, PlacedNode};
use crate::units::{NODE_SPACING_MM, ROW_GAP_MM};

fn fixed_box(index: usize, protocol: &str, width: f64, height: f64, subject: bool) -> NodeBox {
    NodeBox {
        id: NodeId::from_index(index),
        protocol_id: protocol.to_owned(),
        is_overflow: false,
        width_mm: width,
        height_mm: height,
        lines: Vec::new(),
        is_subject: subject,
        is_company: true,
    }
}

fn place_chart(boxes: &[NodeBox], pairs: &[(&str, &str)], cols: usize) -> LayoutResult {
    let ids: std::collections::HashMap<String, NodeId> = boxes
        .iter()
        .map(|box_| (box_.protocol_id.clone(), box_.id))
        .collect();
    let edges: Vec<EdgeRef> = pairs
        .iter()
        .map(|(from, to)| EdgeRef::new(ids[*from], ids[*to]))
        .collect();
    let order: Vec<NodeId> = boxes.iter().map(|box_| box_.id).collect();
    WrappedEngine
        .layout(&LayoutInput {
            boxes,
            edges: &edges,
            node_order: &order,
            cols,
            node_width_mm: 42.0,
            row_gap_mm: ROW_GAP_MM,
            printable_width_mm: 800.0,
        })
        .expect("layout")
}

fn node_at<'a>(layout: &'a LayoutResult, protocol: &str) -> &'a PlacedNode {
    layout
        .nodes
        .iter()
        .find(|node| node.protocol_id == protocol)
        .expect(protocol)
}

#[test]
fn seven_shareholders_wrap_into_three_rows_with_one_arrow() {
    let mut boxes = Vec::new();
    for index in 1..=7 {
        boxes.push(fixed_box(index, &format!("s{index}"), 42.0, 12.0, false));
    }
    boxes.push(fixed_box(0, "root", 42.0, 14.0, true));
    let layout = place_chart(
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
        3,
    );
    let root = node_at(&layout, "root");
    let mut rows: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for index in 1..=7 {
        let node = node_at(&layout, &format!("s{index}"));
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
    let layout = place_chart(
        &boxes,
        &[("p1", "root"), ("p2", "root"), ("a1", "p1"), ("b1", "p2")],
        3,
    );
    let p1 = node_at(&layout, "p1");
    let p2 = node_at(&layout, "p2");
    let gap = p2.x - (p1.x + p1.width);
    assert!((gap - 12.0).abs() < 0.05, "shallow gap {gap}");
    let a1 = node_at(&layout, "a1");
    let b1 = node_at(&layout, "b1");
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
    let layout = place_chart(
        &boxes,
        &[
            ("p1", "root"),
            ("p2", "root"),
            ("a1", "p1"),
            ("b1", "p2"),
            ("b2", "p2"),
            ("b3", "p2"),
        ],
        3,
    );
    let p1 = node_at(&layout, "p1");
    let a1 = node_at(&layout, "a1");
    let b1 = node_at(&layout, "b1");
    assert!(a1.x + a1.width < b1.x);
    assert!((a1.x + a1.width / 2.0 - (p1.x + p1.width / 2.0)).abs() < 1.0);
    let p2 = node_at(&layout, "p2");
    let b2 = node_at(&layout, "b2");
    let b3 = node_at(&layout, "b3");
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
    let layout = place_chart(&boxes, &[("s1", "root"), ("s2", "root"), ("s3", "root")], 3);
    let s1 = node_at(&layout, "s1");
    let s2 = node_at(&layout, "s2");
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
    let layout = place_chart(
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
        3,
    );
    let outer_bottom = ["s05", "s06", "s12"]
        .into_iter()
        .map(|id| {
            let node = node_at(&layout, id);
            node.y + node.height
        })
        .fold(f64::NEG_INFINITY, f64::max);
    let leaf_top = node_at(&layout, "p1").y.min(node_at(&layout, "p2").y);
    assert!(
        outer_bottom + 0.5 < leaf_top,
        "{outer_bottom} vs {leaf_top}"
    );
    assert!(node_at(&layout, "s05a").y < node_at(&layout, "s05").y);
    assert!(node_at(&layout, "s05").y < node_at(&layout, "root").y);
}
