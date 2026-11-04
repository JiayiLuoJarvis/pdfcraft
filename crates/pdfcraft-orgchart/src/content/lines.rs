//! In-box lines and the ratio label that sits outside the box.
//!
//! A ratio never enters the box. Company gray fill is decided here and stored
//! on the box when measuring runs.

use std::collections::HashSet;

use crate::error::{Error, ErrorCode};
use crate::graph::{self, ratio_source, Graph, NodeId, NodeRec};
#[cfg(test)]
use crate::request::ChartRequest;
#[cfg(test)]
use crate::request::Node;
use crate::request::NodeType;

/// Which cone a node belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DisplayRole {
    /// The chart subject.
    Subject,
    /// A node reached by walking holders upward.
    Shareholder,
    /// A node reached by walking investments downward.
    Investment,
}

/// What an in-box line is for. Measuring uses this to pick a font size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LineKind {
    /// The party name.
    Name,
    /// A legal representative, with no prefix.
    LegalPerson,
    /// Paid-in amount, location, or approval date.
    Detail,
}

/// One drawn line inside a node box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogicalLine {
    /// Name, legal person, or detail.
    pub kind: LineKind,
    /// Text after blank fields are dropped. The name is kept as stored.
    pub text: String,
}

/// Lines and role for every node, indexed by [`NodeId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeDisplays {
    nodes: Vec<NodeContent>,
}

/// In-box content for one node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeContent {
    /// Cone role.
    pub role: DisplayRole,
    /// Lines in draw order. Empty optional fields are absent.
    pub lines: Vec<LogicalLine>,
}

impl NodeDisplays {
    /// Content for `id`.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn get(&self, id: NodeId) -> Option<&NodeContent> {
        self.nodes.get(id.index())
    }

    /// Content parallel to graph nodes.
    #[must_use]
    pub(crate) fn nodes(&self) -> &[NodeContent] {
        &self.nodes
    }
}

/// Which side of a peripheral node the ratio label sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LabelHug {
    /// Shareholder. The label sits under the box.
    Bottom,
    /// Investment. The label sits above the box.
    Top,
}

/// The holding edge a ratio was copied from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParentEdge {
    /// Holder id.
    pub from_id: String,
    /// Held id.
    pub to_id: String,
}

/// Graph-time ratio for one non-subject node. No coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RatioFact {
    /// Node id.
    pub node_id: NodeId,
    /// Protocol id for page filtering and PDF text keys.
    pub protocol_id: String,
    /// Percent in `0..=100`.
    pub ratio: f64,
    /// `xx.xx%`.
    pub text: String,
    /// Which side of the box the label hugs.
    pub hug: LabelHug,
    /// Parent edge when the percent came from that edge.
    ///
    /// A ratio stored on the node leaves this `None`.
    pub preferred_parent: Option<ParentEdge>,
}

/// Build in-box lines for every node.
///
/// # Errors
///
/// Returns [`Error`] when `request` breaks the chart protocol.
#[cfg(test)]
pub(crate) fn build_display(request: &ChartRequest) -> Result<NodeDisplays, Error> {
    let graph = graph::validate(request)?;
    Ok(display_of(&graph))
}

pub(crate) fn display_of(graph: &Graph) -> NodeDisplays {
    let shareholders = graph::shareholder_ids(graph);
    let investments = graph::investment_ids(graph);
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let role = role_of(node.id, graph.subject, &shareholders, &investments);
        nodes.push(NodeContent {
            role,
            lines: inbox_lines_rec(node, role),
        });
    }
    NodeDisplays { nodes }
}

fn role_of(
    id: NodeId,
    subject: NodeId,
    shareholders: &HashSet<NodeId>,
    investments: &HashSet<NodeId>,
) -> DisplayRole {
    if id == subject {
        DisplayRole::Subject
    } else if investments.contains(&id) {
        DisplayRole::Investment
    } else if shareholders.contains(&id) {
        DisplayRole::Shareholder
    } else {
        // `validate` already rejects a node outside both cones.
        DisplayRole::Shareholder
    }
}

fn inbox_lines_rec(node: &NodeRec, role: DisplayRole) -> Vec<LogicalLine> {
    inbox_fields(
        &node.name,
        node.legal_person.as_deref(),
        node.paid_in_amount.as_deref(),
        node.currency.as_deref(),
        node.location.as_deref(),
        node.approval_date.as_deref(),
        role,
    )
}

fn inbox_fields(
    name: &str,
    legal_person: Option<&str>,
    paid_in_amount: Option<&str>,
    currency: Option<&str>,
    location: Option<&str>,
    approval_date: Option<&str>,
    role: DisplayRole,
) -> Vec<LogicalLine> {
    let mut lines = vec![LogicalLine {
        kind: LineKind::Name,
        text: name.to_owned(),
    }];
    match role {
        DisplayRole::Subject | DisplayRole::Shareholder => {
            if let Some(text) = shown(legal_person) {
                lines.push(LogicalLine {
                    kind: LineKind::LegalPerson,
                    text,
                });
            }
        }
        DisplayRole::Investment => {
            if let Some(text) = paid_in_text(paid_in_amount, currency) {
                lines.push(LogicalLine {
                    kind: LineKind::Detail,
                    text,
                });
            }
            if let Some(text) = shown(location) {
                lines.push(LogicalLine {
                    kind: LineKind::Detail,
                    text,
                });
            }
            if let Some(text) = shown(approval_date) {
                lines.push(LogicalLine {
                    kind: LineKind::Detail,
                    text,
                });
            }
        }
    }
    lines
}

/// Gray fill when the node is a company, or when type is unset and a legal person is present.
///
/// [`NodeType::Person`] is always white, even when `legal_person` is set.
#[cfg(test)]
#[must_use]
pub(crate) fn company_fill(node: &Node) -> bool {
    company_fill_fields(node.node_type, node.legal_person.as_deref())
}

/// Gray fill for an arena node. Same rules as [`company_fill`].
pub(crate) fn company_fill_rec(node: &NodeRec) -> bool {
    company_fill_fields(node.node_type, node.legal_person.as_deref())
}

fn company_fill_fields(node_type: Option<NodeType>, legal_person: Option<&str>) -> bool {
    match node_type {
        Some(NodeType::Company) => true,
        Some(NodeType::Person) => false,
        None => shown(legal_person).is_some(),
    }
}

fn paid_in_text(paid_in_amount: Option<&str>, currency: Option<&str>) -> Option<String> {
    let amount = shown(paid_in_amount)?;
    let text = match shown(currency) {
        Some(currency) => format!("实缴 {amount} {currency}"),
        None => format!("实缴 {amount}"),
    };
    Some(text)
}

fn shown(value: Option<&str>) -> Option<String> {
    let text = value?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}

/// `xx.xx%`, two digits after the decimal point.
#[must_use]
pub(crate) fn format_display_ratio(ratio: f64) -> String {
    format!("{ratio:.2}%")
}

/// One fact per non-subject node that has a resolved ratio, including folded nodes.
///
/// # Errors
///
/// Returns [`Error`] when `request` fails validation, or when a parent edge is ambiguous.
#[cfg(test)]
pub(crate) fn ratio_facts(request: &ChartRequest) -> Result<Vec<RatioFact>, Error> {
    let mut graph = graph::validate(request)?;
    facts_of(&mut graph)
}

pub(crate) fn facts_of(graph: &mut Graph) -> Result<Vec<RatioFact>, Error> {
    let shareholders = graph::shareholder_ids(graph);
    let investments = graph::investment_ids(graph);
    let mut facts = Vec::new();
    let count = graph.nodes.len();
    for index in 0..count {
        let id = NodeId::from_index(index);
        let role = role_of(id, graph.subject, &shareholders, &investments);
        if role == DisplayRole::Subject {
            continue;
        }
        let Some(source) = ratio_source(graph, id)? else {
            continue;
        };
        let preferred_parent = match source.parent {
            Some(edge) => {
                let from = graph.edge_from(edge);
                let to = graph.edge_to(edge);
                Some(ParentEdge {
                    from_id: graph.protocol_id(from).to_owned(),
                    to_id: graph.protocol_id(to).to_owned(),
                })
            }
            None => None,
        };
        facts.push(RatioFact {
            node_id: id,
            protocol_id: graph.protocol_id(id).to_owned(),
            ratio: source.value,
            text: format_display_ratio(source.value),
            hug: hug_for(role)?,
            preferred_parent,
        });
    }
    Ok(facts)
}

fn hug_for(role: DisplayRole) -> Result<LabelHug, Error> {
    match role {
        DisplayRole::Investment => Ok(LabelHug::Top),
        DisplayRole::Shareholder => Ok(LabelHug::Bottom),
        DisplayRole::Subject => Err(Error::new(
            ErrorCode::InvalidProtocol,
            "role SUBJECT has no ratio hug",
        )),
    }
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod lines_tests;
