//! The arena: copyable ids, node and edge records, and the [`Graph`] that owns them.
//!
//! Protocol strings stay on [`ChartRequest`](crate::request::ChartRequest) and in
//! error messages. Layout, paging, and drawing index nodes with [`NodeId`].

use std::collections::HashMap;

use super::ratio::unique_layout_parent_edge;
use crate::error::Error;
use crate::request::{ChartOptions, ChartRequest, NodeType};

/// One party or a synthetic overflow box.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct NodeId(u32);

impl NodeId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).unwrap_or(u32::MAX))
    }

    pub(crate) fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One holding edge in the validated graph.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct EdgeId(u32);

impl EdgeId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).unwrap_or(u32::MAX))
    }

    pub(crate) fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

impl std::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How a node entered the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeOrigin {
    /// From the request, including a caller-written `__other__:` id.
    Input,
    /// `top_n` fold stand-in. Protocol id is still `__other__:{parent}`.
    FoldedHolders { parent: NodeId },
    /// Paging overflow box. Protocol id is still `__overflow__:{subject}:{first}`.
    Overflow {
        subject: NodeId,
        first_moved: NodeId,
        moved_count: usize,
    },
}

/// One node in the arena.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeRec {
    pub(crate) id: NodeId,
    pub(crate) origin: NodeOrigin,
    pub(crate) protocol_id: String,
    pub(crate) name: String,
    pub(crate) legal_person: Option<String>,
    pub(crate) ratio: Option<f64>,
    pub(crate) node_type: Option<NodeType>,
    pub(crate) paid_in_amount: Option<String>,
    pub(crate) currency: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) approval_date: Option<String>,
}

/// One edge in the arena.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EdgeRec {
    pub(crate) id: EdgeId,
    pub(crate) from: NodeId,
    pub(crate) to: NodeId,
    pub(crate) ratio: Option<f64>,
}

/// User-supplied node ids must not use this prefix. Layout assigns these ids later.
pub(crate) const OVERFLOW_ID_PREFIX: &str = "__overflow__:";

/// Id of the stand-in for direct children moved off one page.
///
/// `first_moved_id` is the earliest moved child in input order, so one subject
/// can overflow more than once.
#[must_use]
pub(crate) fn overflow_node_id(subject_id: &str, first_moved_id: &str) -> String {
    format!("{OVERFLOW_ID_PREFIX}{subject_id}:{first_moved_id}")
}

/// A request that passed [`validate`], owned as an arena.
///
/// Nodes and edges are addressed by [`NodeId`] / [`EdgeId`]. `outgoing` and
/// `incoming` are indexed by [`NodeId::index`].
#[derive(Debug, Clone)]
pub(crate) struct Graph {
    pub(crate) nodes: Vec<NodeRec>,
    pub(crate) edges: Vec<EdgeRec>,
    pub(super) by_protocol_id: HashMap<String, NodeId>,
    pub(super) outgoing: Vec<Vec<EdgeId>>,
    pub(super) incoming: Vec<Vec<EdgeId>>,
    pub(super) out_nodes: Vec<Vec<NodeId>>,
    pub(crate) subject: NodeId,
    pub(crate) subject_protocol_id: String,
    pub(crate) options: ChartOptions,
    /// Reachability answers for layout-parent selection. Same lifetime as the graph.
    pub(super) reach_cache: HashMap<(NodeId, NodeId), bool>,
    /// Overflow boxes already appended for `(subject, first_moved)`.
    pub(super) overflow_ids: HashMap<(NodeId, NodeId), NodeId>,
}

impl Graph {
    pub(crate) fn protocol_id(&self, id: NodeId) -> &str {
        &self.nodes[id.index()].protocol_id
    }

    pub(crate) fn node_name(&self, id: NodeId) -> &str {
        &self.nodes[id.index()].name
    }

    pub(crate) fn contains_protocol(&self, protocol: &str) -> bool {
        self.by_protocol_id.contains_key(protocol)
    }

    #[cfg(test)]
    pub(crate) fn id_of(&self, protocol: &str) -> Option<NodeId> {
        self.by_protocol_id.get(protocol).copied()
    }

    pub(crate) fn incoming_edges(&self, node: NodeId) -> &[EdgeId] {
        &self.incoming[node.index()]
    }

    pub(crate) fn edge_from(&self, edge: EdgeId) -> NodeId {
        self.edges[edge.index()].from
    }

    pub(crate) fn edge_to(&self, edge: EdgeId) -> NodeId {
        self.edges[edge.index()].to
    }

    /// Holders of `id` (`from` on an incoming edge), in edge order.
    pub(crate) fn incoming_sources(&self, id: NodeId) -> Vec<NodeId> {
        self.incoming[id.index()]
            .iter()
            .map(|&edge| self.edge_from(edge))
            .collect()
    }

    /// Nodes `id` holds (`to` on an outgoing edge), in edge order.
    pub(crate) fn outgoing_targets(&self, id: NodeId) -> Vec<NodeId> {
        self.outgoing[id.index()]
            .iter()
            .map(|&edge| self.edge_to(edge))
            .collect()
    }

    /// Endpoints of the unique layout-parent edge, or `None` when there is no parent.
    ///
    /// An ambiguous parent is [`ErrorCode::InvalidProtocol`].
    pub(crate) fn layout_parent_ends(
        &mut self,
        id: NodeId,
    ) -> Result<Option<(NodeId, NodeId)>, Error> {
        let Some(edge) = unique_layout_parent_edge(self, id)? else {
            return Ok(None);
        };
        Ok(Some((self.edge_from(edge), self.edge_to(edge))))
    }

    /// Unique layout parent when one exists. No parent or an ambiguous parent is `None`.
    pub(crate) fn layout_parent_optional(&mut self, id: NodeId) -> Option<(NodeId, NodeId)> {
        self.layout_parent_ends(id).unwrap_or_default()
    }

    /// Append an overflow node, or reuse the id for the same subject and first moved child.
    pub(crate) fn ensure_overflow(
        &mut self,
        subject: NodeId,
        first_moved: NodeId,
        moved_count: usize,
        name: String,
    ) -> NodeId {
        if let Some(id) = self.overflow_ids.get(&(subject, first_moved)) {
            return *id;
        }
        let protocol = overflow_node_id(self.protocol_id(subject), self.protocol_id(first_moved));
        let id = NodeId::from_index(self.nodes.len());
        self.nodes.push(NodeRec {
            id,
            origin: NodeOrigin::Overflow {
                subject,
                first_moved,
                moved_count,
            },
            protocol_id: protocol.clone(),
            name,
            legal_person: None,
            ratio: None,
            node_type: None,
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        });
        self.by_protocol_id.insert(protocol, id);
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        self.out_nodes.push(Vec::new());
        self.overflow_ids.insert((subject, first_moved), id);
        id
    }

    /// Rebuild a [`ChartRequest`] for the prepare boundary (fold output).
    pub(crate) fn to_request(&self) -> ChartRequest {
        ChartRequest {
            subject_id: self.subject_protocol_id.clone(),
            nodes: self
                .nodes
                .iter()
                .filter(|node| !matches!(node.origin, NodeOrigin::Overflow { .. }))
                .map(|node| crate::request::Node {
                    id: node.protocol_id.clone(),
                    name: node.name.clone(),
                    legal_person: node.legal_person.clone(),
                    ratio: node.ratio,
                    node_type: node.node_type,
                    paid_in_amount: node.paid_in_amount.clone(),
                    currency: node.currency.clone(),
                    location: node.location.clone(),
                    approval_date: node.approval_date.clone(),
                })
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|edge| crate::request::Edge {
                    from_id: self.protocol_id(edge.from).to_owned(),
                    to_id: self.protocol_id(edge.to).to_owned(),
                    ratio: edge.ratio,
                })
                .collect(),
            options: self.options.clone(),
        }
    }
}
