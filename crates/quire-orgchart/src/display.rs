//! In-box lines and the ratio label that sits outside the box.
//!
//! A ratio never enters the box. Company gray fill is decided here and stored
//! on the box when measuring runs.

use std::collections::{HashMap, HashSet};

use crate::error::{Error, ErrorCode};
use crate::graph::{self, ratio_source, ValidatedGraph};
use crate::model::{ChartRequest, Node, NodeType};

/// Which cone a node belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayRole {
    /// The chart subject.
    Subject,
    /// A node reached by walking holders upward.
    Shareholder,
    /// A node reached by walking investments downward.
    Investment,
}

impl DisplayRole {
    /// `SUBJECT`, `SHAREHOLDER`, or `INVESTMENT`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subject => "SUBJECT",
            Self::Shareholder => "SHAREHOLDER",
            Self::Investment => "INVESTMENT",
        }
    }
}

/// What an in-box line is for. Measuring uses this to pick a font size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineKind {
    /// The party name.
    Name,
    /// A legal representative, with no prefix.
    LegalPerson,
    /// Paid-in amount, location, or approval date.
    Detail,
}

impl LineKind {
    /// `NAME`, `LEGAL_PERSON`, or `DETAIL`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "NAME",
            Self::LegalPerson => "LEGAL_PERSON",
            Self::Detail => "DETAIL",
        }
    }
}

/// One drawn line inside a node box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalLine {
    /// Name, legal person, or detail.
    pub kind: LineKind,
    /// Text after blank fields are dropped. The name is kept as stored.
    pub text: String,
}

/// Lines and role for every node, in request order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDisplays {
    nodes: Vec<NodeContent>,
    index_of: HashMap<String, usize>,
}

/// In-box content for one node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeContent {
    /// Cone role.
    pub role: DisplayRole,
    /// Lines in draw order. Empty optional fields are absent.
    pub lines: Vec<LogicalLine>,
}

impl NodeDisplays {
    /// Content for `id`, or `None` when the id is not in the request.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&NodeContent> {
        self.index_of.get(id).map(|&index| &self.nodes[index])
    }

    /// Content parallel to `request.nodes`.
    #[must_use]
    pub fn nodes(&self) -> &[NodeContent] {
        &self.nodes
    }
}

/// Which side of a peripheral node the ratio label sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabelHug {
    /// Shareholder. The label sits under the box.
    Bottom,
    /// Investment. The label sits above the box.
    Top,
}

impl LabelHug {
    /// `BOTTOM` or `TOP`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "BOTTOM",
            Self::Top => "TOP",
        }
    }
}

/// The holding edge a ratio was copied from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentEdge {
    /// Holder id.
    pub from_id: String,
    /// Held id.
    pub to_id: String,
}

/// Graph-time ratio for one non-subject node. No coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct RatioFact {
    /// Node id.
    pub node_id: String,
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
pub fn build_display(request: &ChartRequest) -> Result<NodeDisplays, Error> {
    let graph = graph::validate(request)?;
    Ok(display_of(&graph))
}

pub(crate) fn display_of(graph: &ValidatedGraph<'_>) -> NodeDisplays {
    let shareholders = graph::shareholder_ids(graph);
    let investments = graph::investment_ids(graph);
    let mut nodes = Vec::with_capacity(graph.request.nodes.len());
    let mut index_of = HashMap::with_capacity(graph.request.nodes.len());
    for (index, node) in graph.request.nodes.iter().enumerate() {
        let role = role_of(index, graph.subject, &shareholders, &investments);
        index_of.insert(node.id.clone(), index);
        nodes.push(NodeContent {
            role,
            lines: inbox_lines(node, role),
        });
    }
    NodeDisplays { nodes, index_of }
}

fn role_of(
    index: usize,
    subject: usize,
    shareholders: &HashSet<usize>,
    investments: &HashSet<usize>,
) -> DisplayRole {
    if index == subject {
        DisplayRole::Subject
    } else if investments.contains(&index) {
        DisplayRole::Investment
    } else if shareholders.contains(&index) {
        DisplayRole::Shareholder
    } else {
        // `validate` already rejects a node outside both cones.
        DisplayRole::Shareholder
    }
}

/// Lines for one node. Ratio text is not included.
#[must_use]
pub fn inbox_lines(node: &Node, role: DisplayRole) -> Vec<LogicalLine> {
    let mut lines = vec![LogicalLine {
        kind: LineKind::Name,
        text: node.name.clone(),
    }];
    match role {
        DisplayRole::Subject | DisplayRole::Shareholder => {
            if let Some(text) = shown(node.legal_person.as_deref()) {
                lines.push(LogicalLine {
                    kind: LineKind::LegalPerson,
                    text,
                });
            }
        }
        DisplayRole::Investment => {
            if let Some(text) = paid_in_text(node) {
                lines.push(LogicalLine {
                    kind: LineKind::Detail,
                    text,
                });
            }
            if let Some(text) = shown(node.location.as_deref()) {
                lines.push(LogicalLine {
                    kind: LineKind::Detail,
                    text,
                });
            }
            if let Some(text) = shown(node.approval_date.as_deref()) {
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
#[must_use]
pub fn company_fill(node: &Node) -> bool {
    match node.node_type {
        Some(NodeType::Company) => true,
        Some(NodeType::Person) => false,
        None => shown(node.legal_person.as_deref()).is_some(),
    }
}

fn paid_in_text(node: &Node) -> Option<String> {
    let amount = shown(node.paid_in_amount.as_deref())?;
    let text = match shown(node.currency.as_deref()) {
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
pub fn format_display_ratio(ratio: f64) -> String {
    format!("{ratio:.2}%")
}

/// One fact per non-subject node that has a resolved ratio, including folded nodes.
///
/// # Errors
///
/// Returns [`Error`] when `request` fails validation, or when a parent edge is ambiguous.
pub fn ratio_facts(request: &ChartRequest) -> Result<Vec<RatioFact>, Error> {
    let graph = graph::validate(request)?;
    facts_of(&graph)
}

pub(crate) fn facts_of(graph: &ValidatedGraph<'_>) -> Result<Vec<RatioFact>, Error> {
    let shareholders = graph::shareholder_ids(graph);
    let investments = graph::investment_ids(graph);
    let mut cache = HashMap::new();
    let mut facts = Vec::new();
    for index in 0..graph.request.nodes.len() {
        let role = role_of(index, graph.subject, &shareholders, &investments);
        if role == DisplayRole::Subject {
            continue;
        }
        let Some(source) = ratio_source(graph, index, &mut cache)? else {
            continue;
        };
        facts.push(RatioFact {
            node_id: graph.node_id(index).to_owned(),
            ratio: source.value,
            text: format_display_ratio(source.value),
            hug: hug_for(role)?,
            preferred_parent: source.parent.map(|edge| ParentEdge {
                from_id: graph.request.edges[edge].from_id.clone(),
                to_id: graph.request.edges[edge].to_id.clone(),
            }),
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
#[allow(clippy::float_cmp)]
mod tests {
    use super::{build_display, company_fill, ratio_facts, LabelHug, LineKind};
    use crate::model::{ChartRequest, Edge, Node, NodeType};

    fn edge(from: &str, to: &str, ratio: f64) -> Edge {
        Edge {
            ratio: Some(ratio),
            ..Edge::new(from, to)
        }
    }

    #[test]
    fn cones_keep_their_fields_and_leave_ratio_out_of_the_box() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node {
                    legal_person: Some("李四".to_string()),
                    ratio: Some(10.0),
                    paid_in_amount: Some("1万".to_string()),
                    currency: Some("USD".to_string()),
                    location: Some("上海".to_string()),
                    approval_date: Some("2020-01-01".to_string()),
                    ..Node::new("root", "主体")
                },
                Node {
                    legal_person: Some("  王五  ".to_string()),
                    ratio: Some(40.0),
                    paid_in_amount: Some("9万".to_string()),
                    currency: Some("HKD".to_string()),
                    location: Some("北京".to_string()),
                    approval_date: Some("2019-01-01".to_string()),
                    ..Node::new("s", "股东甲")
                },
                Node {
                    legal_person: Some("赵六".to_string()),
                    paid_in_amount: Some("100万".to_string()),
                    currency: Some("CNY".to_string()),
                    location: Some("  深圳  ".to_string()),
                    approval_date: Some("2024-06-01".to_string()),
                    ..Node::new("c", "子公司")
                },
            ],
            vec![edge("s", "root", 40.0), edge("root", "c", 51.2)],
        );
        let display = build_display(&request).expect("display");
        let pairs = |id: &str| {
            display
                .get(id)
                .expect(id)
                .lines
                .iter()
                .map(|line| (line.kind, line.text.as_str()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pairs("root"),
            vec![(LineKind::Name, "主体"), (LineKind::LegalPerson, "李四"),]
        );
        assert_eq!(
            pairs("s"),
            vec![(LineKind::Name, "股东甲"), (LineKind::LegalPerson, "王五"),]
        );
        assert_eq!(
            pairs("c"),
            vec![
                (LineKind::Name, "子公司"),
                (LineKind::Detail, "实缴 100万 CNY"),
                (LineKind::Detail, "深圳"),
                (LineKind::Detail, "2024-06-01"),
            ]
        );
        for content in display.nodes() {
            for line in &content.lines {
                assert!(!line.text.contains("法定代表人"));
                assert!(!line.text.contains('%'));
            }
        }
        assert!(display
            .get("s")
            .expect("s")
            .lines
            .iter()
            .all(|line| !line.text.contains("实缴")));
        assert!(display
            .get("c")
            .expect("c")
            .lines
            .iter()
            .all(|line| !line.text.contains("赵六")));
        let facts = ratio_facts(&request).expect("facts");
        let holder = facts.iter().find(|fact| fact.node_id == "s").expect("s");
        assert_eq!(holder.text, "40.00%");
        assert_eq!(holder.ratio, 40.0);
        assert_eq!(holder.hug, LabelHug::Bottom);
        assert!(holder.preferred_parent.is_none());
        let child = facts.iter().find(|fact| fact.node_id == "c").expect("c");
        assert_eq!(child.text, "51.20%");
        assert_eq!(child.hug, LabelHug::Top);
        let parent = child.preferred_parent.as_ref().expect("edge");
        assert_eq!(parent.from_id, "root");
        assert_eq!(parent.to_id, "c");
        assert!(facts.iter().all(|fact| fact.node_id != "root"));
    }

    #[test]
    fn currency_without_amount_adds_no_detail_line() {
        let request = ChartRequest::new(
            "root",
            vec![
                Node::new("root", "主体"),
                Node {
                    currency: Some("CNY".to_string()),
                    legal_person: Some("不应出现".to_string()),
                    ..Node::new("c1", "只有币种")
                },
                Node {
                    paid_in_amount: Some("   ".to_string()),
                    location: Some("  ".to_string()),
                    ..Node::new("c2", "空白实缴")
                },
            ],
            vec![edge("root", "c1", 10.0), edge("root", "c2", 0.0)],
        );
        let display = build_display(&request).expect("display");
        assert_eq!(
            display
                .get("c1")
                .expect("c1")
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["只有币种"]
        );
        assert_eq!(
            display
                .get("c2")
                .expect("c2")
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            vec!["空白实缴"]
        );
        let facts = ratio_facts(&request).expect("facts");
        assert_eq!(
            facts
                .iter()
                .find(|fact| fact.node_id == "c2")
                .expect("c2")
                .text,
            "0.00%"
        );
        assert!(facts.iter().all(|fact| fact.node_id != "root"));
    }

    #[test]
    fn person_is_white_and_company_is_gray() {
        assert!(!company_fill(&Node {
            node_type: Some(NodeType::Person),
            legal_person: Some("张三".to_string()),
            ..Node::new("p", "张三")
        }));
        assert!(company_fill(&Node {
            node_type: Some(NodeType::Company),
            ..Node::new("c", "公司")
        }));
        assert!(company_fill(&Node {
            legal_person: Some("李四".to_string()),
            ..Node::new("u", "未标")
        }));
        assert!(!company_fill(&Node {
            legal_person: Some("  ".to_string()),
            ..Node::new("b", "空白")
        }));
    }
}
