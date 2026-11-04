//! Assembling one page: measured boxes, the overflow stand-in, and the family subgraph.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::{Session, Side};
use crate::error::{Error, ErrorCode};
use crate::font::Measurer;
use crate::graph::NodeId;
use crate::layout::LayoutEngine;
use crate::page::{overflow_box, promote_to_page_subject, NodeBox};
use crate::paginate::family::{FamilyEdge, FamilyGraph, OverflowNode};
use crate::request::NodeType;

impl<E: LayoutEngine, M: Measurer> Session<'_, E, M> {
    pub(super) fn boxes_for_page(
        &mut self,
        ids: &BTreeSet<NodeId>,
        layout_root_id: Option<NodeId>,
    ) -> Result<HashMap<NodeId, NodeBox>, Error> {
        let mut page_boxes = HashMap::new();
        for &id in ids {
            let Some(box_) = self.boxes.get(&id) else {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    format!(
                        "page draft node {} has no measured box",
                        self.graph.label(id)
                    ),
                ));
            };
            let stored = match layout_root_id {
                Some(root) if id == root => {
                    if id == self.subject_node && box_.is_subject {
                        box_.clone()
                    } else {
                        promote_to_page_subject(box_, self.node_width, self.measurer)?
                    }
                }
                Some(_) if box_.is_subject => NodeBox {
                    is_subject: false,
                    ..box_.clone()
                },
                None | Some(_) => box_.clone(),
            };
            page_boxes.insert(id, stored);
        }
        Ok(page_boxes)
    }

    pub(super) fn overflow_for(
        &mut self,
        node: &OverflowNode,
        side: Side,
    ) -> Result<NodeBox, Error> {
        if !self.boxes.contains_key(&node.subject_id) {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("overflow subject missing: {}", node.subject_id),
            ));
        }
        if !self.boxes.contains_key(&node.first_moved_id) {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("overflow child missing: {}", node.first_moved_id),
            ));
        }
        let name = match side {
            Side::Shareholder => crate::captions::other_shareholders_name(node.moved_count),
            Side::Investment => crate::captions::other_investments_name(node.moved_count),
        };
        let id = self.graph.ensure_overflow(
            node.subject_id,
            node.first_moved_id,
            node.moved_count,
            name.clone(),
        );
        if let Some(cached) = self.overflow_boxes.get(&id) {
            return Ok(cached.clone());
        }
        let origin = self.graph.origin(id).clone();
        let box_ = overflow_box(id, origin, &name, self.measurer, self.node_width)?;
        self.overflow_boxes.insert(id, box_.clone());
        Ok(box_)
    }

    pub(super) fn family_graph(&self, direct_ids: &[NodeId], side: Side) -> FamilyGraph {
        let mut node_ids = HashSet::from([self.subject_node]);
        let mut stack: Vec<NodeId> = direct_ids.to_vec();
        while let Some(node_id) = stack.pop() {
            if !node_ids.insert(node_id) {
                continue;
            }
            let next = match side {
                Side::Shareholder => self.graph.incoming_sources(node_id),
                Side::Investment => self.graph.outgoing_targets(node_id),
            };
            stack.extend(next);
        }
        let mut edges = Vec::new();
        for edge in &self.graph.edges {
            let from_node = edge.from;
            let to_node = edge.to;
            if !node_ids.contains(&from_node) || !node_ids.contains(&to_node) {
                continue;
            }
            let (parent_id, child_id) = if side == Side::Shareholder {
                (to_node, from_node)
            } else {
                (from_node, to_node)
            };
            edges.push(FamilyEdge {
                id: edge.id.index(),
                parent_id,
                child_id,
            });
        }
        let person_ids = node_ids
            .iter()
            .copied()
            .filter(|id| {
                self.graph
                    .nodes
                    .get(id.index())
                    .is_some_and(|node| node.node_type == Some(NodeType::Person))
            })
            .collect();
        FamilyGraph {
            subject_id: self.subject_node,
            edges,
            node_order: self
                .node_order
                .iter()
                .copied()
                .filter(|id| node_ids.contains(id))
                .collect(),
            person_ids,
        }
    }
}

pub(super) fn investment_id_refs(ids: &[NodeId], boxes: &[NodeBox]) -> Vec<NodeId> {
    ids.iter()
        .copied()
        .filter(|id| boxes.iter().any(|box_| box_.id == *id))
        .collect()
}
