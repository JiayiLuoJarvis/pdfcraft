//! The arena: copyable ids, node and edge records, and the [`Graph`] that owns them.
//!
//! Caller string ids stay on [`ChartRequest`](crate::request::ChartRequest),
//! in [`NodeOrigin::Caller`], and in error messages. Layout, paging, and drawing
//! index nodes with [`NodeId`].

use std::collections::HashMap;
use std::fmt;

use super::ratio::unique_layout_parent_edge;
use crate::error::{Error, ErrorSubject};
use crate::request::{ChartStyle, NodeType};

/// One party, a fold stand-in, or a synthetic overflow box.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct NodeId(usize);

impl NodeId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(crate) fn index(self) -> usize {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One holding edge in the validated graph.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct EdgeId(usize);

impl EdgeId {
    pub(crate) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(crate) fn index(self) -> usize {
        self.0
    }
}

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How a node entered the arena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeOrigin {
    /// From the request. `index` is the position in `ChartRequest::nodes`.
    /// `stands_in_for` marks a caller-declared stand-in for the folded holders of that node.
    Caller {
        index: usize,
        id: String,
        stands_in_for: Option<NodeId>,
    },
    /// `top_n` stand-in for the holders folded under `parent`.
    FoldedHolders { parent: NodeId, count: usize },
    /// Paging stand-in for direct children moved off one page.
    Overflow {
        subject: NodeId,
        first_moved: NodeId,
        moved_count: usize,
    },
}

impl NodeOrigin {
    /// The node already stands in for the folded holders of `parent`.
    pub(crate) fn stands_in_for(&self, parent: NodeId) -> bool {
        match self {
            Self::Caller { stands_in_for, .. } => *stands_in_for == Some(parent),
            Self::FoldedHolders { parent: own, .. } => *own == parent,
            Self::Overflow { .. } => false,
        }
    }

    pub(crate) fn is_overflow(&self) -> bool {
        matches!(self, Self::Overflow { .. })
    }

    /// Caller-authored node, when this origin is one.
    pub(crate) fn error_subject(&self) -> Option<ErrorSubject> {
        match self {
            Self::Caller { index, id, .. } => Some(ErrorSubject::Node {
                index: *index,
                id: id.clone(),
            }),
            Self::FoldedHolders { .. } | Self::Overflow { .. } => None,
        }
    }
}

impl fmt::Display for NodeOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Caller { id, .. } => f.write_str(id),
            Self::FoldedHolders { parent, count } => {
                write!(f, "{count} folded holders of node #{parent}")
            }
            Self::Overflow { subject, .. } => write!(f, "overflow box of node #{subject}"),
        }
    }
}

/// One node in the arena.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeRec {
    pub(crate) id: NodeId,
    pub(crate) origin: NodeOrigin,
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
    /// Position in `ChartRequest::edges`; `None` for an edge added by folding.
    pub(crate) caller_index: Option<usize>,
}

/// A request that passed [`validate`](super::validate), owned as an arena.
///
/// Nodes and edges are addressed by [`NodeId`] / [`EdgeId`]. `outgoing` and
/// `incoming` are indexed by [`NodeId::index`].
#[derive(Debug, Clone)]
pub(crate) struct Graph {
    pub(crate) nodes: Vec<NodeRec>,
    pub(crate) edges: Vec<EdgeRec>,
    pub(super) outgoing: Vec<Vec<EdgeId>>,
    pub(super) incoming: Vec<Vec<EdgeId>>,
    pub(super) out_nodes: Vec<Vec<NodeId>>,
    pub(crate) subject: NodeId,
    pub(crate) subject_protocol_id: String,
    pub(crate) style: ChartStyle,
    pub(crate) max_nodes: usize,
    /// Reachability answers for layout-parent selection. Same lifetime as the graph.
    pub(super) reach_cache: HashMap<(NodeId, NodeId), bool>,
    /// Overflow boxes already appended for `(subject, first_moved)`.
    pub(super) overflow_ids: HashMap<(NodeId, NodeId), NodeId>,
}

impl Graph {
    /// Index `nodes` and `edges` into an arena. Runs no protocol checks.
    pub(super) fn from_records(
        nodes: Vec<NodeRec>,
        edges: Vec<EdgeRec>,
        subject: NodeId,
        subject_protocol_id: String,
        style: ChartStyle,
        max_nodes: usize,
    ) -> Self {
        let node_count = nodes.len();
        let mut outgoing = vec![Vec::new(); node_count];
        let mut incoming = vec![Vec::new(); node_count];
        let mut out_nodes = vec![Vec::new(); node_count];
        for edge in &edges {
            outgoing[edge.from.index()].push(edge.id);
            incoming[edge.to.index()].push(edge.id);
            out_nodes[edge.from.index()].push(edge.to);
        }
        Self {
            nodes,
            edges,
            outgoing,
            incoming,
            out_nodes,
            subject,
            subject_protocol_id,
            style,
            max_nodes,
            reach_cache: HashMap::new(),
            overflow_ids: HashMap::new(),
        }
    }

    pub(crate) fn origin(&self, id: NodeId) -> &NodeOrigin {
        &self.nodes[id.index()].origin
    }

    /// Caller node, when `id` was written on the request.
    pub(crate) fn node_subject(&self, id: NodeId) -> Option<ErrorSubject> {
        self.origin(id).error_subject()
    }

    /// Caller edge, or the caller-authored end of a synthetic edge.
    pub(crate) fn edge_subject(&self, edge: &EdgeRec) -> Option<ErrorSubject> {
        if let Some(index) = edge.caller_index {
            return Some(ErrorSubject::Edge {
                index,
                holder: self.label(edge.from),
                held: self.label(edge.to),
            });
        }
        self.node_subject(edge.from)
            .or_else(|| self.node_subject(edge.to))
    }

    /// Subject for the edge from `from` to `to`, when that edge exists.
    pub(crate) fn subject_between(&self, from: NodeId, to: NodeId) -> Option<ErrorSubject> {
        self.edges
            .iter()
            .find(|edge| edge.from == from && edge.to == to)
            .and_then(|edge| self.edge_subject(edge))
    }

    /// Text for error messages: the caller id, or a description of a synthetic node.
    pub(crate) fn label(&self, id: NodeId) -> String {
        match self.origin(id) {
            NodeOrigin::Caller { id, .. } => id.clone(),
            NodeOrigin::FoldedHolders { parent, count } => {
                format!("{count} folded holders of {}", self.label(*parent))
            }
            NodeOrigin::Overflow {
                subject,
                first_moved,
                ..
            } => format!(
                "overflow of {} from {}",
                self.label(*subject),
                self.label(*first_moved)
            ),
        }
    }

    pub(crate) fn node_name(&self, id: NodeId) -> &str {
        &self.nodes[id.index()].name
    }

    #[cfg(test)]
    pub(crate) fn id_of(&self, protocol: &str) -> Option<NodeId> {
        self.nodes.iter().find_map(|node| match &node.origin {
            NodeOrigin::Caller { id, .. } if id == protocol => Some(node.id),
            NodeOrigin::Caller { .. }
            | NodeOrigin::FoldedHolders { .. }
            | NodeOrigin::Overflow { .. } => None,
        })
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
    /// An ambiguous parent is [`ErrorCode::InvalidProtocol`](crate::error::ErrorCode::InvalidProtocol).
    pub(crate) fn layout_parent_ends(
        &mut self,
        id: NodeId,
    ) -> Result<Option<(NodeId, NodeId)>, Error> {
        let Some(edge) = unique_layout_parent_edge(self, id)? else {
            return Ok(None);
        };
        Ok(Some((self.edge_from(edge), self.edge_to(edge))))
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
        let id = NodeId::from_index(self.nodes.len());
        self.nodes.push(NodeRec {
            id,
            origin: NodeOrigin::Overflow {
                subject,
                first_moved,
                moved_count,
            },
            name,
            legal_person: None,
            ratio: None,
            node_type: None,
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        });
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        self.out_nodes.push(Vec::new());
        self.overflow_ids.insert((subject, first_moved), id);
        id
    }
}
