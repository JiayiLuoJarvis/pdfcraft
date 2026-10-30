//! Page structure for charts the packer can keep on one sheet.

use std::collections::BTreeSet;

use crate::captions::CueSide;
use crate::display::{build_display, ratio_facts, LineKind};
use crate::ids::NodeId;
use crate::layout::{FamilyRowMetrics, LayoutEngine, WrappedEngine};
use crate::measure::{
    build_node_boxes, promote_to_page_subject, FontMetrics, InboxNode, Measurer, NodeBox,
    WrappedLine,
};
use crate::page::{ContCue, EdgeRef, LayoutResult, NodePageLink, PageSpec, PlacedNode};
use crate::paging::{
    paginate_request as paginate, paginate_request_with as paginate_with, FallbackMeasurer,
};
use crate::units::{PageGeometry, NODE_SPACING_MM, ROW_GAP_MM, SHAREHOLDER_STUB_MIN_MM};
use crate::{document_title, ChartRequest, Edge, Error, ErrorCode, FontBytes, Node};

struct FakeMeasurer;

impl Measurer for FakeMeasurer {
    fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, Error> {
        let count = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
        Ok(f64::from(count) * 2.0)
    }

    fn line_height_mm(&mut self, _size_pt: f64) -> f64 {
        5.0
    }

    fn ensure_glyphs(&mut self, _text: &str) -> Result<(), Error> {
        Ok(())
    }
}

fn company(id: &str, name: &str) -> Node {
    Node::new(id, name)
}

fn chart(holders: usize, investments: usize) -> ChartRequest {
    let mut nodes = vec![company("root", "主体")];
    let mut edges = Vec::new();
    for index in 0..holders {
        let id = format!("h{index}");
        nodes.push(company(&id, &format!("股东{index}")));
        let mut edge = Edge::new(id, "root");
        edge.ratio = Some(10.0);
        edges.push(edge);
    }
    for index in 0..investments {
        let id = format!("i{index}");
        nodes.push(company(&id, &format!("投资{index}")));
        let mut edge = Edge::new("root", id);
        edge.ratio = Some(10.0);
        edges.push(edge);
    }
    ChartRequest::new("root", nodes, edges)
}

fn pages_of(request: &ChartRequest) -> Vec<crate::page::PageSpec> {
    let graph = crate::graph::validate(request).expect("graph");
    let display = build_display(request).expect("display");
    let facts = ratio_facts(request).expect("facts");
    let inbox: Vec<InboxNode<'_>> = request
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let id = crate::ids::NodeId::from_index(index);
            InboxNode {
                id,
                protocol_id: node.id.as_str(),
                lines: &display.get(id).expect("lines").lines,
                is_company: crate::display::company_fill(node),
            }
        })
        .collect();
    let mut measurer = FakeMeasurer;
    let boxes =
        build_node_boxes(&inbox, &mut measurer, 38.0, &request.subject_id, 257.0).expect("boxes");
    let _ = (facts, graph);
    paginate(request, &boxes, PageGeometry::default(), &mut measurer).expect("pages")
}

#[test]
fn a_subject_alone_is_one_portrait_page() {
    let pages = pages_of(&chart(0, 0));
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].main_title, "股权架构图: 主体");
    assert_eq!(pages[0].sub_title, "各股东/合伙人 → 主体");
    assert!((pages[0].geometry.page_width_mm - 210.0).abs() < 0.001);
    assert!((pages[0].geometry.page_height_mm - 297.0).abs() < 0.001);
    assert_eq!(pages[0].layout.nodes.len(), 1);
    assert!(pages[0].boxes.iter().any(|node| node.is_subject));
}

#[test]
fn a_fork_that_fits_is_one_landscape_page() {
    let pages = pages_of(&chart(2, 1));
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].main_title, "股权架构图: 主体");
    assert_eq!(pages[0].sub_title, "各股东/合伙人 → 主体 → 对外投资");
    assert!((pages[0].geometry.page_width_mm - 297.0).abs() < 0.001);
    assert!((pages[0].geometry.page_height_mm - 210.0).abs() < 0.001);
    assert_eq!(pages[0].layout.nodes.len(), 4);
    assert_eq!(pages[0].layout.edges.len(), 3);
    assert_eq!(pages[0].ratio_facts.len(), 3);
}

#[test]
fn multi_holder_bus_keeps_one_arrow_on_the_trunk() {
    let pages = pages_of(&chart(2, 0));
    assert_eq!(pages.len(), 1);
    let subject = pages[0]
        .layout
        .nodes
        .iter()
        .find(|node| node.protocol_id == "root")
        .expect("subject");
    let into_subject: Vec<_> = pages[0]
        .layout
        .edges
        .iter()
        .filter(|edge| edge.to_id == subject.id)
        .collect();
    assert_eq!(into_subject.len(), 2);
    assert_eq!(
        into_subject.iter().filter(|edge| edge.draw_arrow).count(),
        1,
        "only the shared trunk should carry an arrow tip"
    );
    let bus_y = into_subject
        .iter()
        .map(|edge| {
            if edge.draw_arrow {
                edge.points[edge.points.len() - 2].y
            } else {
                edge.points[edge.points.len() - 1].y
            }
        })
        .fold(f64::INFINITY, f64::min);
    let source_bottom = pages[0]
        .layout
        .nodes
        .iter()
        .filter(|node| node.protocol_id != "root")
        .map(|node| node.y + node.height)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        bus_y + 0.05 >= source_bottom + SHAREHOLDER_STUB_MIN_MM,
        "bus {bus_y} source bottom {source_bottom}"
    );
    for edge in into_subject.iter().filter(|edge| !edge.draw_arrow) {
        let tip = edge.points.last().expect("points");
        assert!(
            (tip.y - subject.y).abs() > 0.5,
            "feeder edges must stop on the bus, not at the subject top"
        );
    }
}

#[test]
fn a_chain_stays_portrait_and_the_holder_sits_above_the_subject() {
    let pages = pages_of(&chart(1, 0));
    assert_eq!(pages.len(), 1);
    assert!((pages[0].geometry.page_width_mm - 210.0).abs() < 0.001);
    assert_eq!(pages[0].layout.edges.len(), 1);
    let edge = &pages[0].layout.edges[0];
    assert!(edge.points.len() >= 2);
    let from = pages[0]
        .layout
        .nodes
        .iter()
        .find(|node| node.id == edge.from_id)
        .expect("from");
    let to = pages[0]
        .layout
        .nodes
        .iter()
        .find(|node| node.id == edge.to_id)
        .expect("to");
    assert!(from.y < to.y);
}

struct FakeEngine {
    limit: i32,
}

fn count_i32(len: usize) -> i32 {
    let mut count = 0_i32;
    for _ in 0..len {
        count = count.saturating_add(1);
    }
    count
}

impl LayoutEngine for FakeEngine {
    fn layout(
        &self,
        boxes: &[NodeBox],
        _edges: &[crate::page::EdgeRef],
        _node_order: &[NodeId],
        _cols: usize,
        _node_width_mm: f64,
        _row_gap_mm: f64,
        _printable_width_mm: f64,
    ) -> Result<LayoutResult, Error> {
        if self.limit >= 0 && count_i32(boxes.len()) <= self.limit {
            let mut nodes = Vec::new();
            let mut x = 0.0;
            for box_ in boxes {
                let height = box_.height_mm.max(1.0);
                nodes.push(PlacedNode {
                    id: box_.id,
                    protocol_id: box_.protocol_id.clone(),
                    x,
                    y: 0.0,
                    width: box_.width_mm,
                    height,
                });
                x += box_.width_mm + 6.0;
            }
            return LayoutResult::from_parts(nodes, Vec::new(), Vec::new());
        }
        Ok(LayoutResult {
            nodes: Vec::new(),
            edges: Vec::new(),
            min_x: 0.0,
            min_y: 0.0,
            width_mm: 10_000.0,
            height_mm: 10_000.0,
            slots: Vec::new(),
        })
    }

    fn family_row_metrics(
        &self,
        boxes: &[NodeBox],
        _edges: &[crate::page::EdgeRef],
        _node_order: &[NodeId],
        _cols: usize,
        _node_width_mm: f64,
        _printable_width_mm: f64,
    ) -> Result<FamilyRowMetrics, Error> {
        if boxes.is_empty() {
            return Ok(FamilyRowMetrics {
                max_family_rows: 0,
                extra_rows: 0,
                max_nondirect_rows: 0,
            });
        }
        if self.limit < 0 || count_i32(boxes.len()) > self.limit {
            Ok(FamilyRowMetrics {
                max_family_rows: 2,
                extra_rows: 1,
                max_nondirect_rows: 2,
            })
        } else {
            Ok(FamilyRowMetrics {
                max_family_rows: 1,
                extra_rows: 0,
                max_nondirect_rows: 1,
            })
        }
    }
}

fn plain_box(index: usize, id: &str, name: &str, subject: bool) -> NodeBox {
    NodeBox {
        id: NodeId::from_index(index),
        protocol_id: id.to_owned(),
        is_overflow: false,
        width_mm: 38.0,
        height_mm: 12.0,
        lines: vec![WrappedLine {
            kind: LineKind::Name,
            text: name.to_owned(),
            font_size_pt: 10.0,
        }],
        is_subject: subject,
        is_company: true,
    }
}

fn fake_pages(request: &ChartRequest, limit: i32) -> Result<Vec<PageSpec>, Error> {
    let boxes: Vec<NodeBox> = request
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| plain_box(index, &node.id, &node.name, node.id == request.subject_id))
        .collect();
    paginate_with(
        request,
        &boxes,
        PageGeometry::default(),
        &FakeEngine { limit },
        &mut FallbackMeasurer,
    )
}

fn chain(ids: &[&str], investment: bool) -> ChartRequest {
    let mut nodes = vec![Node::new("root", "主体")];
    for id in ids {
        nodes.push(Node::new(*id, format!("名-{id}")));
    }
    let mut edges = Vec::new();
    if investment {
        edges.push(Edge::new("root", ids[0]));
        for pair in ids.windows(2) {
            edges.push(Edge::new(pair[0], pair[1]));
        }
    } else {
        edges.push(Edge::new(ids[0], "root"));
        for pair in ids.windows(2) {
            edges.push(Edge::new(pair[1], pair[0]));
        }
    }
    ChartRequest::new("root", nodes, edges)
}

fn id_sets(pages: &[PageSpec]) -> Vec<BTreeSet<String>> {
    pages
        .iter()
        .map(|page| {
            page.boxes
                .iter()
                .map(|box_| box_.protocol_id.clone())
                .collect()
        })
        .collect()
}

fn set_of(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn never_fits_chain_is_layout_failed() {
    let error = fake_pages(&chain(&["a1", "a2"], false), -1).expect_err("fails");
    assert_eq!(error.code, ErrorCode::LayoutFailed);
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
    assert_eq!(error.code, ErrorCode::LayoutFailed);
    assert_eq!(error.message, "page draft node a2 has no measured box");
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
    let mut font = FontMetrics::open(&bytes).expect("font");
    let pages = match paginate_with(
        &request,
        &boxes,
        PageGeometry::default(),
        &FakeEngine { limit: 2 },
        &mut font,
    ) {
        Ok(pages) => pages,
        Err(error) if error.code == ErrorCode::MissingGlyph => return,
        Err(error) => panic!("pages: {error}"),
    };
    let promoted = pages
        .iter()
        .flat_map(|page| page.boxes.iter())
        .find(|box_| box_.is_subject && box_.protocol_id != "root")
        .expect("annex subject");
    let source_name = request
        .nodes
        .iter()
        .find(|node| node.id == promoted.protocol_id)
        .expect("source")
        .name
        .clone();
    let source = plain_box(
        promoted.id.index(),
        &promoted.protocol_id,
        &source_name,
        false,
    );
    let mut again = FontMetrics::open(&bytes).expect("font");
    let expected = match promote_to_page_subject(&source, 38.0, &mut again) {
        Ok(lines) => lines,
        Err(error) if error.code == ErrorCode::MissingGlyph => return,
        Err(error) => panic!("promote: {error}"),
    };
    assert_eq!(promoted.lines, expected.lines);
    let mut fake = FallbackMeasurer;
    let fallback = promote_to_page_subject(&source, 38.0, &mut fake).expect("fallback");
    assert_ne!(promoted.lines.len(), fallback.lines.len());
}

fn hc_font() -> Option<FontBytes> {
    crate::test_font::env_font()
}

#[test]
fn investment_chain_never_fits_is_layout_failed() {
    let error = fake_pages(&chain(&["v1"], true), -1).expect_err("fails");
    assert_eq!(error.code, ErrorCode::LayoutFailed);
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

#[test]
fn empty_cones_fall_back_to_subject_page() {
    let pages = fake_pages(
        &ChartRequest::new("root", vec![Node::new("root", "主体")], vec![]),
        1,
    )
    .expect("pages");
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].main_title, "股权架构图: 主体");
    assert_eq!(pages[0].sub_title, "各股东/合伙人 → 主体");
    assert_eq!(id_sets(&pages), vec![set_of(&["root"])]);
}

#[test]
fn options_title_does_not_override_page_header() {
    let mut request = ChartRequest::new("root", vec![Node::new("root", "主体")], vec![]);
    request.options.title = Some("自定义标题".to_owned());
    let pages = fake_pages(&request, 10).expect("pages");
    assert_eq!(pages[0].main_title, "股权架构图: 主体");
    assert_eq!(document_title(&request).expect("title"), "自定义标题");
    assert_eq!(pages[0].sub_title, "各股东/合伙人 → 主体");
    assert!(pages[0].cont_cues.is_empty());
}

#[test]
fn whole_chart_with_investments_extends_the_arrow() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("s1", "股东甲"),
            Node::new("v1", "被投"),
        ],
        vec![Edge::new("s1", "root"), Edge::new("root", "v1")],
    );
    let pages = fake_pages(&request, 10).expect("pages");
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].sub_title, "各股东/合伙人 → 主体 → 对外投资");
    assert_eq!(pages[0].main_title, "股权架构图: 主体");
}

#[test]
fn shallow_cones_pack_ahead_of_a_deeper_cone() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("deep", "深"),
            Node::new("d1", "上层"),
            Node::new("d2", "顶层"),
            Node::new("s1", "浅1"),
            Node::new("s2", "浅2"),
        ],
        vec![
            Edge::new("deep", "root"),
            Edge::new("d1", "deep"),
            Edge::new("d2", "d1"),
            Edge::new("s1", "root"),
            Edge::new("s2", "root"),
        ],
    );
    let pages = fake_pages(&request, 4).expect("pages");
    assert_eq!(id_sets(&pages)[0], set_of(&["root", "deep", "s1", "s2"]));
    assert_eq!(
        pages[0].cont_cues,
        vec![ContCue {
            target_page: 2,
            direct_count: 1,
            side: CueSide::Shareholder,
            anchor_id: "deep".to_owned(),
        }]
    );
    assert_eq!(id_sets(&pages)[1], set_of(&["deep", "d1", "d2"]));
}

#[test]
fn shareholder_pages_cue_the_next_direct_page_not_investments() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("s1", "股东1"),
            Node::new("s2", "股东2"),
            Node::new("s3", "股东3"),
            Node::new("s4", "股东4"),
            Node::new("v1", "投资"),
        ],
        vec![
            Edge::new("s1", "root"),
            Edge::new("s2", "root"),
            Edge::new("s3", "root"),
            Edge::new("s4", "root"),
            Edge::new("root", "v1"),
        ],
    );
    let pages = fake_pages(&request, 3).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "s1", "__overflow__:root:s2"]),
            set_of(&["root", "s2", "__overflow__:root:s3"]),
            set_of(&["root", "s3", "s4"]),
            set_of(&["root", "v1"]),
        ]
    );
    assert_eq!(
        pages[0].cont_cues,
        vec![ContCue {
            target_page: 4,
            direct_count: 1,
            side: CueSide::Investment,
            anchor_id: "root".to_owned(),
        }]
    );
    assert!(pages[1].cont_cues.is_empty());
    assert!(pages[2].cont_cues.is_empty());
    assert!(pages[3].cont_cues.is_empty());
    assert_eq!(
        pages[0].node_page_links,
        vec![NodePageLink {
            node_id: "__overflow__:root:s2".to_owned(),
            target_page: 2,
        }]
    );
    assert_eq!(
        pages[1].node_page_links,
        vec![NodePageLink {
            node_id: "__overflow__:root:s3".to_owned(),
            target_page: 3,
        }]
    );
    assert!(pages[2].node_page_links.is_empty());
    let overflow_text = |page: &PageSpec, id: &str| {
        page.boxes
            .iter()
            .find(|box_| box_.protocol_id == id)
            .and_then(|box_| box_.lines.last())
            .map(|line| line.text.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        overflow_text(&pages[0], "__overflow__:root:s2"),
        "详见附件2"
    );
    assert_eq!(
        overflow_text(&pages[1], "__overflow__:root:s3"),
        "详见附件3"
    );
}

#[test]
fn chain_cues_each_logical_attachment_after_final_numbering() {
    let pages = fake_pages(&chain(&["a1", "a2", "a3"], false), 2).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "a1"]),
            set_of(&["a1", "a2"]),
            set_of(&["a2", "a3"]),
        ]
    );
    assert_eq!(
        pages[0].cont_cues,
        vec![ContCue {
            target_page: 2,
            direct_count: 1,
            side: CueSide::Shareholder,
            anchor_id: "a1".to_owned(),
        }]
    );
    assert_eq!(
        pages[1].cont_cues,
        vec![ContCue {
            target_page: 3,
            direct_count: 1,
            side: CueSide::Shareholder,
            anchor_id: "a2".to_owned(),
        }]
    );
    assert!(pages[2].cont_cues.is_empty());
    assert_eq!(
        pages
            .iter()
            .map(|page| page.sub_title.as_str())
            .collect::<Vec<_>>(),
        ["各股东/合伙人 → 主体", "名-a1持有主体", "名-a2持有名-a1",]
    );
    assert_eq!(
        pages
            .iter()
            .map(|page| page.main_title.as_str())
            .collect::<Vec<_>>(),
        ["股权架构图: 主体", "附件2: 名-a1", "附件3: 名-a2",]
    );
}

#[test]
fn cut_subtitle_uses_display_ratio_rounded_to_two_places() {
    let mut a1 = Node::new("a1", "名-a1");
    a1.ratio = Some(10.0);
    let mut a2 = Node::new("a2", "名-a2");
    a2.ratio = Some(80.5556);
    let mut edge_a1 = Edge::new("a1", "root");
    edge_a1.ratio = Some(10.0);
    let mut edge_a2 = Edge::new("a2", "a1");
    edge_a2.ratio = Some(80.5556);
    let request = ChartRequest::new(
        "root",
        vec![Node::new("root", "主体"), a1, a2, Node::new("a3", "名-a3")],
        vec![edge_a1, edge_a2, Edge::new("a3", "a2")],
    );
    let pages = fake_pages(&request, 2).expect("pages");
    assert_eq!(
        pages
            .iter()
            .map(|page| page.sub_title.as_str())
            .collect::<Vec<_>>(),
        [
            "各股东/合伙人 → 主体",
            "名-a1持有主体 10.00%",
            "名-a2持有名-a1 80.56%",
        ]
    );
    assert_eq!(
        pages
            .iter()
            .map(|page| page.main_title.as_str())
            .collect::<Vec<_>>(),
        ["股权架构图: 主体", "附件2: 名-a1", "附件3: 名-a2",]
    );
}

#[test]
fn completed_appendix_has_no_unanchored_sibling_cue() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("a", "族甲"),
            Node::new("a1", "甲上"),
            Node::new("a2", "甲顶"),
            Node::new("b", "族乙"),
            Node::new("b1", "乙上"),
            Node::new("b2", "乙顶"),
        ],
        vec![
            Edge::new("a", "root"),
            Edge::new("a1", "a"),
            Edge::new("a2", "a1"),
            Edge::new("b", "root"),
            Edge::new("b1", "b"),
            Edge::new("b2", "b1"),
        ],
    );
    let pages = fake_pages(&request, 3).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "a", "b"]),
            set_of(&["a", "a1", "a2"]),
            set_of(&["b", "b1", "b2"]),
        ]
    );
    assert_eq!(
        pages[0].cont_cues,
        vec![
            ContCue {
                target_page: 2,
                direct_count: 1,
                side: CueSide::Shareholder,
                anchor_id: "a".to_owned(),
            },
            ContCue {
                target_page: 3,
                direct_count: 1,
                side: CueSide::Shareholder,
                anchor_id: "b".to_owned(),
            },
        ]
    );
    assert!(pages[1].cont_cues.is_empty());
    assert!(pages[2].cont_cues.is_empty());
}

#[test]
fn completed_investment_appendix_has_no_unanchored_sibling_cue() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("root", "主体"),
            Node::new("s1", "股东"),
            Node::new("v", "投甲"),
            Node::new("v1", "甲下"),
            Node::new("v2", "甲叶"),
            Node::new("w", "投乙"),
            Node::new("w1", "乙下"),
            Node::new("w2", "乙叶"),
        ],
        vec![
            Edge::new("s1", "root"),
            Edge::new("root", "v"),
            Edge::new("v", "v1"),
            Edge::new("v1", "v2"),
            Edge::new("root", "w"),
            Edge::new("w", "w1"),
            Edge::new("w1", "w2"),
        ],
    );
    let pages = fake_pages(&request, 3).expect("pages");
    assert_eq!(
        id_sets(&pages),
        vec![
            set_of(&["root", "s1"]),
            set_of(&["root", "v", "w"]),
            set_of(&["v", "v1", "v2"]),
            set_of(&["w", "w1", "w2"]),
        ]
    );
    assert_eq!(
        pages[1].cont_cues,
        vec![
            ContCue {
                target_page: 3,
                direct_count: 1,
                side: CueSide::Investment,
                anchor_id: "v".to_owned(),
            },
            ContCue {
                target_page: 4,
                direct_count: 1,
                side: CueSide::Investment,
                anchor_id: "w".to_owned(),
            },
        ]
    );
    assert!(pages[2].cont_cues.is_empty());
    assert!(pages[3].cont_cues.is_empty());
}

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
        .layout(boxes, &edges, &order, cols, 42.0, ROW_GAP_MM, 800.0)
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

#[test]
fn eight_and_fourteen_direct_holders_stay_within_the_overview_budget() {
    for count in [8_usize, 14] {
        let pages = pages_of(&chart(count, 0));
        assert!(!pages.is_empty());
        assert!(pages.len() <= 3, "{count} holders produced {}", pages.len());
        assert!(pages.iter().all(|page| page.layout.fits(page.geometry)));
        let landscape = pages
            .iter()
            .any(|page| page.geometry.page_width_mm > page.geometry.page_height_mm);
        assert!(
            pages.len() == 1 || landscape,
            "{count} holders stayed portrait across {} pages",
            pages.len()
        );
    }
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
            protocol_id: node.id.as_str(),
            lines: &display.get(NodeId::from_index(index)).expect("lines").lines,
            is_company: true,
        })
        .collect();
    let mut measurer = FakeMeasurer;
    let mut boxes = build_node_boxes(
        &inbox,
        &mut measurer,
        38.0,
        &request.subject_id,
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
    let pages = chain_pages(&deep, geometry, 20.0);
    assert!(pages.len() < 8, "deep chain pages {}", pages.len());
    assert!(pages.len() >= 2);
    let ids = id_sets(&pages);
    for pair in ids.windows(2) {
        assert!(pair[0].intersection(&pair[1]).next().is_some());
    }
}
