//! Page structure for charts the packer can keep on one sheet.

use std::collections::BTreeSet;

use crate::captions::CueSide;
use crate::display::{build_display, ratio_facts, LineKind};
use crate::ids::NodeId;
use crate::layout::{FamilyRowMetrics, LayoutEngine};
use crate::measure::{
    build_node_boxes, promote_to_page_subject, FontMetrics, InboxNode, Measurer, NodeBox,
    WrappedLine,
};
use crate::page::{ContCue, LayoutResult, NodePageLink, PageSpec, PlacedNode};
use crate::paging::{
    paginate_request as paginate, paginate_request_with as paginate_with, FallbackMeasurer,
};
use crate::units::PageGeometry;
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
    let mut font = FontMetrics::open(&hc_font()).expect("font");
    let pages = paginate_with(
        &request,
        &boxes,
        PageGeometry::default(),
        &FakeEngine { limit: 2 },
        &mut font,
    )
    .expect("pages");
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
    let mut again = FontMetrics::open(&hc_font()).expect("font");
    let expected = promote_to_page_subject(&source, 38.0, &mut again).expect("promote");
    assert_eq!(promoted.lines, expected.lines);
    let mut fake = FallbackMeasurer;
    let fallback = promote_to_page_subject(&source, 38.0, &mut fake).expect("fallback");
    assert_ne!(promoted.lines.len(), fallback.lines.len());
}

fn hc_font() -> FontBytes {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fonts/SourceHanSerifHC-ExtraLight.ttf");
    FontBytes::new(
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
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
