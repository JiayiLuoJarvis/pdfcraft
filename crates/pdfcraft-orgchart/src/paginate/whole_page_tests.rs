//! Charts the packer keeps on one sheet, and the overview budget for wide holder sets.

use super::test_support::{caller, fake_pages, id_sets, node_id, set_of, FakeMeasurer};
use crate::content::{build_display, ratio_facts};
use crate::graph::NodeId;
use crate::page::{build_node_boxes, InboxNode, PageSpec};
use crate::paginate::paginate_request as paginate;
use crate::units::{PageGeometry, SHAREHOLDER_STUB_MIN_MM};
use crate::{ChartRequest, Edge, Node};

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

fn pages_of(request: &ChartRequest) -> Vec<PageSpec> {
    drop(crate::graph::validate(request).expect("graph"));
    let display = build_display(request).expect("display");
    drop(ratio_facts(request).expect("facts"));
    let inbox: Vec<InboxNode<'_>> = request
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let id = NodeId::from_index(index);
            InboxNode {
                id,
                origin: caller(index, &node.id),
                lines: &display.get(id).expect("lines").lines,
                is_company: crate::content::company_fill(node),
            }
        })
        .collect();
    let mut measurer = FakeMeasurer;
    let subject = node_id(request, &request.subject_id);
    let boxes = build_node_boxes(&inbox, &mut measurer, 38.0, subject, 257.0).expect("boxes");
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
    let request = chart(2, 0);
    let root = node_id(&request, "root");
    let pages = pages_of(&request);
    assert_eq!(pages.len(), 1);
    let subject = pages[0]
        .layout
        .nodes
        .iter()
        .find(|node| node.id == root)
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
        .filter(|node| node.id != root)
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
