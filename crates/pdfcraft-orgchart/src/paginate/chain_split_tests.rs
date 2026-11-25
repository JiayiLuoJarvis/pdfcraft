//! Chains and families that split across pages, and the failures when nothing fits.

use std::collections::BTreeSet;

use super::test_support::{
    caller, chain, fake_pages, id_sets, node_id, plain_box, set_of, FakeEngine, FakeMeasurer,
};
use crate::captions::{other_investments_name, other_shareholders_name};
use crate::content::build_display;
use crate::font::{FallbackMeasurer, FontMetrics};
use crate::graph::{NodeId, NodeOrigin};
use crate::page::{
    build_node_boxes, overflow_box, promote_to_page_subject, InboxNode, NodeBox, PageSpec,
};
use crate::paginate::{paginate_request as paginate, paginate_request_with as paginate_with};
use crate::request::NodeWidth;
use crate::units::PageGeometry;
use crate::Chart;
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
            node.name = "甲".repeat(40);
        }
    }
    let boxes: Vec<NodeBox> = request
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| plain_box(index, &node.id, &node.name, node.id == request.subject_id))
        .collect();
    let Some(bytes) = crate::test_font::env_font() else {
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
        Err(error) if crate::test_font::skip_missing_glyph(&error) => return,
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
    let expected = match promote_to_page_subject(&source, NodeWidth::Fixed(38.0), &mut again) {
        Ok(lines) => lines,
        Err(error) if crate::test_font::skip_missing_glyph(&error) => return,
        Err(error) => panic!("promote: {error}"),
    };
    assert_eq!(promoted.lines, expected.lines);
    let mut fake = FallbackMeasurer;
    let fallback =
        promote_to_page_subject(&source, NodeWidth::Fixed(38.0), &mut fake).expect("fallback");
    assert_ne!(promoted.lines.len(), fallback.lines.len());
}

fn split_production_request() -> ChartRequest {
    let mut nodes = vec![Node::new("root", "主体公司")];
    let mut edges = Vec::new();
    let mut held = "root".to_owned();
    for index in 0..16 {
        let id = format!("a{index}");
        nodes.push(Node {
            legal_person: Some("法人".to_owned()),
            ..Node::new(&id, format!("{}上层{index}", "甲".repeat(12)))
        });
        edges.push(Edge::new(&id, &held));
        held = id;
    }
    for index in 0..12 {
        let id = format!("v{index}");
        nodes.push(Node::new(&id, format!("{}对外{index}", "乙".repeat(8))));
        edges.push(Edge::new("root", &id));
    }
    ChartRequest::new("root", nodes, edges)
}

fn unpromoted_boxes(request: &ChartRequest, font: &crate::Font) -> Vec<NodeBox> {
    let mut graph = crate::graph::prepare_graph(request).expect("graph");
    crate::graph::recheck(&mut graph).expect("recheck");
    let shown = crate::content::display_of(&graph);
    let geometry = PageGeometry::portrait(graph.style.margin_mm);
    let mut metrics = FontMetrics::from_font(font);
    let inboxes: Vec<InboxNode<'_>> = graph
        .nodes
        .iter()
        .zip(shown.nodes())
        .map(|(node, content)| InboxNode {
            id: node.id,
            origin: node.origin.clone(),
            lines: &content.lines,
            is_company: crate::content::company_fill_rec(node),
        })
        .collect();
    build_node_boxes(
        &inboxes,
        &mut metrics,
        graph.style.width,
        graph.subject,
        geometry.printable_height_mm(),
    )
    .expect("boxes")
}

fn overflow_caption(origin: &NodeOrigin, lines: &[crate::page::WrappedLine]) -> Option<String> {
    let NodeOrigin::Overflow { moved_count, .. } = origin else {
        return None;
    };
    let joined: String = lines.iter().map(|line| line.text.as_str()).collect();
    if joined.contains("投资") {
        Some(other_investments_name(*moved_count))
    } else {
        Some(other_shareholders_name(*moved_count))
    }
}

#[test]
fn to_pdf_wraps_promoted_and_overflow_boxes_like_the_helpers() {
    let Some(font) = crate::test_font::env_font() else {
        return;
    };
    let request = split_production_request();
    let chart = Chart::new(request.clone()).expect("chart");
    let pdf = match chart.to_pdf(&font) {
        Ok(pdf) => pdf,
        Err(error) if crate::test_font::skip_missing_glyph(&error) => return,
        Err(error) => panic!("pdf: {error}"),
    };
    assert!(pdf.starts_with(b"%PDF"), "production to_pdf writes a PDF");
    let pages = match chart.pages(&font) {
        Ok(pages) => pages,
        Err(error) if crate::test_font::skip_missing_glyph(&error) => return,
        Err(error) => panic!("pages: {error}"),
    };
    let subject = chart.subject_id();
    let node_width = request.style.width;
    let sources = unpromoted_boxes(&request, &font);
    let mut saw_promoted = false;
    let mut saw_overflow = false;
    for page in &pages {
        for box_ in &page.boxes {
            if box_.is_subject && box_.id != subject {
                let source = sources
                    .iter()
                    .find(|source| source.id == box_.id)
                    .expect("promoted source");
                let mut metrics = FontMetrics::from_font(&font);
                let expected = match promote_to_page_subject(source, node_width, &mut metrics) {
                    Ok(expected) => expected,
                    Err(error) if crate::test_font::skip_missing_glyph(&error) => return,
                    Err(error) => panic!("promote: {error}"),
                };
                assert_eq!(box_.lines, expected.lines, "promoted wrap {}", box_.id);
                saw_promoted = true;
            }
            if let Some(name) = overflow_caption(&box_.origin, &box_.lines) {
                let mut metrics = FontMetrics::from_font(&font);
                let expected = match overflow_box(
                    box_.id,
                    box_.origin.clone(),
                    &name,
                    &mut metrics,
                    node_width,
                ) {
                    Ok(expected) => expected,
                    Err(error) if crate::test_font::skip_missing_glyph(&error) => return,
                    Err(error) => panic!("overflow: {error}"),
                };
                assert!(
                    box_.lines.len() >= 2 && expected.lines.len() >= 2,
                    "overflow reserved line"
                );
                assert_eq!(
                    &box_.lines[..box_.lines.len() - 1],
                    &expected.lines[..expected.lines.len() - 1],
                    "overflow wrap {}",
                    box_.id
                );
                saw_overflow = true;
            }
        }
    }
    assert!(
        saw_promoted || saw_overflow,
        "fixture must promote an annex subject or create overflow; pages={}",
        pages.len()
    );
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
        NodeWidth::Fixed(38.0),
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
