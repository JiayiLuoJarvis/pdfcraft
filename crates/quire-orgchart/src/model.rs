//! Chart request values. Optional fields stay optional through validation and folding.

use crate::error::{Error, ErrorCode};

/// Hard cap for [`ChartOptions::max_nodes`], and the cap used when that field is `None`.
pub const MAX_NODES: usize = 100;

/// Smallest accepted [`ChartOptions::node_width_mm`].
pub const MIN_NODE_WIDTH_MM: f64 = 20.0;

/// Largest accepted [`ChartOptions::node_width_mm`].
pub const MAX_NODE_WIDTH_MM: f64 = 80.0;

/// Largest accepted [`ChartOptions::margin_mm`]. The value must be greater than zero.
pub const MAX_MARGIN_MM: f64 = 40.0;

pub(crate) const RATIO_MIN: f64 = 0.0;
pub(crate) const RATIO_MAX: f64 = 100.0;

/// Kind of party on a node.
///
/// [`NodeType::as_str`] is the external token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeType {
    /// A natural person. External token `PERSON`.
    Person,
    /// A company. External token `COMPANY`.
    Company,
}

impl NodeType {
    /// External token: `PERSON` or `COMPANY`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Company => "COMPANY",
        }
    }
}

/// One party in the chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Caller id. Compared exactly, after a non-blank check.
    pub id: String,
    /// Display name. Must be non-blank to validate. [`document_title`] reads it as stored.
    pub name: String,
    /// Legal representative. Blank handling belongs to display.
    pub legal_person: Option<String>,
    /// Ownership percent in `0..=100`, when the caller set one on the node.
    pub ratio: Option<f64>,
    /// Person or company, when the caller set one.
    pub node_type: Option<NodeType>,
    /// Paid-in amount text. Not parsed.
    pub paid_in_amount: Option<String>,
    /// Currency text. Not parsed.
    pub currency: Option<String>,
    /// Location text. Not parsed.
    pub location: Option<String>,
    /// Approval date text. Not parsed.
    pub approval_date: Option<String>,
}

impl Node {
    /// A node with only `id` and `name` set. Other fields are `None`.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            legal_person: None,
            ratio: None,
            node_type: None,
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        }
    }
}

/// A holding edge. `from_id` holds `to_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Holder node id.
    pub from_id: String,
    /// Held node id.
    pub to_id: String,
    /// Ownership percent in `0..=100`, when the caller set one on the edge.
    pub ratio: Option<f64>,
}

impl Edge {
    /// An edge with no ratio.
    #[must_use]
    pub fn new(from_id: impl Into<String>, to_id: impl Into<String>) -> Self {
        Self {
            from_id: from_id.into(),
            to_id: to_id.into(),
            ratio: None,
        }
    }
}

/// Chart-wide options.
///
/// `None` means the caller did not set the field. Validation reads a set value
/// and otherwise uses [`MAX_NODES`] for the node cap. Width and margin stay
/// unset here.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartOptions {
    /// Node box width in mm, when set.
    pub node_width_mm: Option<f64>,
    /// Page margin in mm, when set.
    pub margin_mm: Option<f64>,
    /// Caller title for PDF document info.
    ///
    /// `None` asks [`document_title`] to build `{subject name}股权架构图`.
    /// `Some("")` is kept.
    pub title: Option<String>,
    /// Node cap. `None` uses [`MAX_NODES`]. A set value must be in `1..=MAX_NODES`.
    pub max_nodes: Option<usize>,
    /// Shareholder fold limit.
    ///
    /// `None` and `Some(0)` do not fold. `Some(n)` for `n >= 1` keeps at most `n`
    /// direct holders on each shareholder parent.
    pub top_n: Option<usize>,
}

/// One equity chart: a subject, its nodes, its holding edges, and options.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartRequest {
    /// Id of the company the chart is about.
    pub subject_id: String,
    /// Parties. Order is the tie-break order for a `top_n` fold.
    pub nodes: Vec<Node>,
    /// Holding edges. `from_id` holds `to_id`.
    pub edges: Vec<Edge>,
    /// Options. Unset fields stay `None` through folding.
    pub options: ChartOptions,
}

impl ChartRequest {
    /// A request with default options.
    #[must_use]
    pub fn new(subject_id: impl Into<String>, nodes: Vec<Node>, edges: Vec<Edge>) -> Self {
        Self {
            subject_id: subject_id.into(),
            nodes,
            edges,
            options: ChartOptions::default(),
        }
    }
}

/// Title for the caller to store as PDF document info.
///
/// Looks up `request.subject_id` in `request.nodes` and does no other check.
/// The first node with that id wins. A missing id is [`ErrorCode::SubjectNotFound`].
///
/// `options.title == None` becomes `{name}股权架构图`. `Some("")` stays empty.
/// The name is copied as stored.
pub fn document_title(request: &ChartRequest) -> Result<String, Error> {
    let Some(node) = request
        .nodes
        .iter()
        .find(|node| node.id == request.subject_id)
    else {
        return Err(Error::new(
            ErrorCode::SubjectNotFound,
            format!("subject_id={} not in nodes", request.subject_id),
        ));
    };
    Ok(match &request.options.title {
        Some(title) => title.clone(),
        None => format!("{}股权架构图", node.name),
    })
}
