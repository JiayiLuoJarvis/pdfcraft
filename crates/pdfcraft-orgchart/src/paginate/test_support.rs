//! Shared fixtures for the pagination tests: fake measurer, fake engine, and request builders.

use std::collections::BTreeSet;

use crate::content::LineKind;
use crate::font::{FallbackMeasurer, Measurer};
use crate::graph::NodeId;
use crate::layout::{FamilyRowMetrics, LayoutEngine, LayoutInput};
use crate::page::{EdgeRef, LayoutResult, NodeBox, PageSpec, PlacedNode, WrappedLine};
use crate::paginate::paginate_request_with as paginate_with;
use crate::units::PageGeometry;
use crate::{ChartRequest, Edge, Error, Node};

pub(super) struct FakeMeasurer;

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

pub(super) struct FakeEngine {
    pub(super) limit: i32,
}

fn count_i32(len: usize) -> i32 {
    let mut count = 0_i32;
    for _ in 0..len {
        count = count.saturating_add(1);
    }
    count
}

impl LayoutEngine for FakeEngine {
    fn layout(&self, input: &LayoutInput<'_>) -> Result<LayoutResult, Error> {
        let boxes = input.boxes;
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
        _edges: &[EdgeRef],
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

pub(super) fn plain_box(index: usize, id: &str, name: &str, subject: bool) -> NodeBox {
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

pub(super) fn fake_pages(request: &ChartRequest, limit: i32) -> Result<Vec<PageSpec>, Error> {
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

pub(super) fn chain(ids: &[&str], investment: bool) -> ChartRequest {
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

pub(super) fn id_sets(pages: &[PageSpec]) -> Vec<BTreeSet<String>> {
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

pub(super) fn set_of(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}
