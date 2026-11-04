//! Chains and families that split across pages, and the failures when nothing fits.

use std::collections::BTreeSet;

use super::test_support::{
    caller, chain, fake_pages, id_sets, node_id, plain_box, set_of, FakeEngine, FakeMeasurer,
};
use crate::content::build_display;
use crate::font::{FallbackMeasurer, FontMetrics};
use crate::graph::NodeId;
use crate::page::{build_node_boxes, promote_to_page_subject, InboxNode, NodeBox, PageSpec};
use crate::paginate::{paginate_request as paginate, paginate_request_with as paginate_with};
use crate::units::PageGeometry;
use crate::{ChartRequest, Edge, ErrorCode, Node};

#[test]
fn never_fits_chain_is_layout_failed() {
    let error = fake_pages(&chain(&["a1", "a2"], false), -1).expect_err("fails");
    assert_eq!(error.code(), ErrorCode::LayoutFailed);
}

#[test]
fn a_page_draft_without_a_measured_box_is_layout_failed() {
    let request = chain(&["a1", "a2"], false);
    let boxes: Vec<NodeBox> = request
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.id != "a2")
        .map(|(index, node)| plain_box(index, &node.id, &node.name, node.id == request.subject_id))
        .collect();
    let error = paginate_with(
        &request,
        &boxes,
        PageGeometry::default(),
        &FakeEngine { limit: -1 },
        &mut FallbackMeasurer,
    )
    .expect_err("missing box");
    assert_eq!(error.code(), ErrorCode::LayoutFailed);
    assert_eq!(error.message(), "page draft node a2 has no measured box");
}

#[test]
fn chain_split_keeps_cut_overlap() {
    let pages = fake_pages(&chain(&["a1", "a2"], false), 2).expect("pages");
    assert!(pages.len() >= 2);
    let ids = id_sets(&pages);
    for pair in ids.windows(2) {
        assert!(pair[0].intersection(&pair[1]).next().is_some());
    }
    let mut union = BTreeSet::new();
    for page in &ids {
        union.extend(page.iter().cloned());
    }
    assert_eq!(union, set_of(&["root", "a1", "a2"]));
    assert!(pages.iter().all(|page| page.layout.fits(page.geometry)));
}

#[test]
fn an_annex_subject_is_rewrapped_with_the_supplied_face() {
    let mut request = chain(&["a1", "a2"], false);
    for node in &mut request.nodes {
        if node.id != "root" {
            node.name = "甲".repeat(25);
        }
    }
    let boxes: Vec<NodeBox> = request
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| plain_box(index, &node.id, &node.name, node.id == request.subject_id))
        .collect();
    let Some(bytes) = hc_font() else {
        return;
    };
    let mut font = FontMetrics::from_font(&bytes);
    let pages = match paginate_with(
        &request,
        &boxes,
        PageGeometry::default(),
        &FakeEngine { limit: 2 },
        &mut font,
    ) {
        Ok(pages) => pages,
        Err(error) if error.code() == ErrorCode::MissingGlyph => return,
        Err(error) => panic!("pages: {error}"),
    };
    let root = node_id(&request, "root");
    let promoted = pages
        .iter()
        .flat_map(|page| page.boxes.iter())
        .find(|box_| box_.is_subject && box_.id != root)
        .expect("annex subject");
    let source_node = &request.nodes[promoted.id.index()];
    let source = plain_box(
        promoted.id.index(),
        &source_node.id,
        &source_node.name,
        false,
    );
    let mut again = FontMetrics::from_font(&bytes);
    let expected = match promote_to_page_subject(&source, 38.0, &mut again) {
        Ok(lines) => lines,
        Err(error) if error.code() == ErrorCode::MissingGlyph => return,
        Err(error) => panic!("promote: {error}"),
    };
    assert_eq!(promoted.lines, expected.lines);
    let mut fake = FallbackMeasurer;
    let fallback = promote_to_page_subject(&source, 38.0, &mut fake).expect("fallback");
    assert_ne!(promoted.lines.len(), fallback.lines.len());
}

fn hc_font() -> Option<crate::Font> {
    crate::test_font::env_font()
}

#[test]
fn investment_chain_never_fits_is_layout_failed() {
    let error = fake_pages(&chain(&["v1"], true), -1).expect_err("fails");
    assert_eq!(error.code(), ErrorCode::LayoutFailed);
}

#[test]
fn group_caption_counter_increments_in_page_order() {
    let nodes = vec![
        Node::new("root", "主体"),
        Node::new("s1", "股东1"),
        Node::new("s2", "股东2"),
        Node::new("s3", "股东3"),
        Node::new("s4", "股东4"),
    ];
    let edges = vec![
        Edge::new("s1", "root"),
        Edge::new("s2", "root"),
        Edge::new("s3", "root"),
        Edge::new("s4", "root"),
    ];
    let pages = fake_pages(&ChartRequest::new("root", nodes, edges), 3).expect("pages");
    assert_eq!(
        pages
            .iter()
            .map(|page| page.sub_title.as_str())
            .collect::<Vec<_>>(),
        [
            "各股东/合伙人 → 主体",
            "各股东/合伙人 → 主体",
            "各股东/合伙人 → 主体",
        ]
    );
    assert_eq!(
        pages
            .iter()
            .map(|page| page.main_title.as_str())
            .collect::<Vec<_>>(),
        ["股权架构图: 主体", "续页2: 主体", "续页3: 主体",]
    );
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "s1", "__overflow__:root:s2"]),
            set_of(&["root", "s2", "__overflow__:root:s3"]),
            set_of(&["root", "s3", "s4"]),
        ]
    );
}

fn chain_pages(ids: &[&str], geometry: PageGeometry, height_mm: f64) -> Vec<PageSpec> {
    let request = chain(ids, false);
    let display = build_display(&request).expect("display");
    let inbox: Vec<InboxNode<'_>> = request
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| InboxNode {
            id: NodeId::from_index(index),
            origin: caller(index, &node.id),
            lines: &display.get(NodeId::from_index(index)).expect("lines").lines,
            is_company: true,
        })
        .collect();
    let mut measurer = FakeMeasurer;
    let mut boxes = build_node_boxes(
        &inbox,
        &mut measurer,
        38.0,
        node_id(&request, &request.subject_id),
        geometry.printable_height_mm(),
    )
    .expect("boxes");
    for box_ in &mut boxes {
        box_.height_mm = height_mm;
    }
    paginate(&request, &boxes, geometry, &mut measurer).expect("pages")
}

#[test]
fn a_tall_chain_stays_portrait_and_a_deep_chain_overlaps() {
    let tall = ["c1", "c2", "c3", "c4", "c5", "c6"];
    let pages = chain_pages(&tall, PageGeometry::default(), 28.0);
    assert!(pages.len() >= 2, "tall chain pages {}", pages.len());
    assert!(pages
        .iter()
        .all(|page| page.geometry.page_width_mm <= page.geometry.page_height_mm));
    assert!(pages.iter().all(|page| page.layout.fits(page.geometry)));

    let deep = ["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8"];
    let geometry = PageGeometry {
        page_width_mm: 140.0,
        page_height_mm: 180.0,
        margin_mm: 10.0,
        header_height_mm: 10.0,
        footer_height_mm: 10.0,
    };
    let deep_pages = chain_pages(&deep, geometry, 20.0);
    assert!(
        deep_pages.len() < 8,
        "deep chain pages {}",
        deep_pages.len()
    );
    assert!(deep_pages.len() >= 2);
    let ids = id_sets(&deep_pages);
    for pair in ids.windows(2) {
        assert!(pair[0].intersection(&pair[1]).next().is_some());
    }
}
