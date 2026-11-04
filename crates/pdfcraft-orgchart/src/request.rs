//! Chart request values. Optional fields stay optional through validation and folding.

use std::num::NonZeroUsize;

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

/// Paid-in capital shown on an investment node.
///
/// An amount that is blank after trimming is not drawn. A blank `currency` is
/// treated as absent. A currency with no amount cannot be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaidIn {
    /// Amount text. Not parsed.
    pub amount: String,
    /// Currency text. Not parsed. Blank means none.
    pub currency: Option<String>,
}

impl PaidIn {
    /// An amount with no currency.
    #[must_use]
    pub fn new(amount: impl Into<String>) -> Self {
        Self {
            amount: amount.into(),
            currency: None,
        }
    }
}

/// One party in the chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Caller id. Compared exactly, after a non-blank check.
    pub id: String,
    /// Display name. Must be non-blank to validate. [`Chart::title`](crate::Chart::title) copies it as stored.
    pub name: String,
    /// Legal representative. Blank handling belongs to display.
    pub legal_person: Option<String>,
    /// Ownership percent in `0..=100`, when the caller set one on the node.
    pub ratio: Option<f64>,
    /// Person or company, when the caller set one.
    pub node_type: Option<NodeType>,
    /// Paid-in capital, when the caller set one.
    pub paid_in: Option<PaidIn>,
    /// Location text. Not parsed.
    pub location: Option<String>,
    /// Approval date text. Not parsed.
    pub approval_date: Option<String>,
    /// This node already stands in for the holders of that node id folded by `top_n`.
    pub folded_holders_of: Option<String>,
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
            paid_in: None,
            location: None,
            approval_date: None,
            folded_holders_of: None,
        }
    }
}

/// A holding edge. `holder` holds `held`.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Holder node id.
    pub holder: String,
    /// Held node id.
    pub held: String,
    /// Ownership percent in `0..=100`, when the caller set one on the edge.
    pub ratio: Option<f64>,
}

impl Edge {
    /// An edge with no ratio.
    #[must_use]
    pub fn new(holder: impl Into<String>, held: impl Into<String>) -> Self {
        Self {
            holder: holder.into(),
            held: held.into(),
            ratio: None,
        }
    }
}

/// Reusable box width and page margin.
///
/// [`ChartStyle::default`] is 38 mm wide with an 8 mm margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartStyle {
    /// Node box width in mm. Legal range is
    /// [`MIN_NODE_WIDTH_MM`](Self::MIN_NODE_WIDTH_MM)..=[`MAX_NODE_WIDTH_MM`](Self::MAX_NODE_WIDTH_MM).
    pub node_width_mm: f64,
    /// Page margin in mm. Must be finite, greater than zero, and at most
    /// [`MAX_MARGIN_MM`](Self::MAX_MARGIN_MM).
    pub margin_mm: f64,
}

impl ChartStyle {
    /// Smallest accepted [`ChartStyle::node_width_mm`].
    pub const MIN_NODE_WIDTH_MM: f64 = 20.0;

    /// Largest accepted [`ChartStyle::node_width_mm`].
    pub const MAX_NODE_WIDTH_MM: f64 = 80.0;

    /// Largest accepted [`ChartStyle::margin_mm`]. The value must be greater than zero.
    pub const MAX_MARGIN_MM: f64 = 40.0;
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            node_width_mm: 38.0,
            margin_mm: 8.0,
        }
    }
}

/// One equity chart: a subject, its nodes, its holding edges, and layout policy.
///
/// There is no [`Default`]. [`ChartRequest::new`] fills `style`, `top_n`, and
/// `max_nodes`.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartRequest {
    /// Id of the company the chart is about.
    pub subject_id: String,
    /// Parties. Order is the tie-break order for a `top_n` fold.
    pub nodes: Vec<Node>,
    /// Holding edges. `holder` holds `held`.
    pub edges: Vec<Edge>,
    /// Box width and page margin.
    pub style: ChartStyle,
    /// Shareholder fold limit.
    ///
    /// `None` does not fold. `Some(n)` keeps at most `n` direct holders on each
    /// shareholder parent. `n` must be in `1..=max_nodes`.
    pub top_n: Option<NonZeroUsize>,
    /// Node cap, including nodes added by a fold. Must be in `1..=`[`MAX_NODES`](Self::MAX_NODES).
    pub max_nodes: usize,
}

impl ChartRequest {
    /// Hard cap for [`ChartRequest::max_nodes`], and the cap [`ChartRequest::new`] uses.
    pub const MAX_NODES: usize = 100;

    /// A request with [`ChartStyle::default`], no fold, and [`MAX_NODES`](Self::MAX_NODES).
    #[must_use]
    pub fn new(subject_id: impl Into<String>, nodes: Vec<Node>, edges: Vec<Edge>) -> Self {
        Self {
            subject_id: subject_id.into(),
            nodes,
            edges,
            style: ChartStyle::default(),
            top_n: None,
            max_nodes: Self::MAX_NODES,
        }
    }
}
