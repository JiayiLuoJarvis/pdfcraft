//! Copyable ids assigned after a chart is validated.
//!
//! Protocol strings stay on [`ChartRequest`](crate::model::ChartRequest) and in
//! error messages. Layout, paging, and drawing index nodes with [`NodeId`].

use crate::model::NodeType;

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
