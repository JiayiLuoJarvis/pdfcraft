//! Protocol checks for one chart: ids, edges, cycles, and the two cones.

use std::collections::{HashMap, HashSet};

use super::arena::{EdgeId, EdgeRec, Graph, NodeId, NodeOrigin, NodeRec};
use super::ratio::unique_layout_parent_edge;
use crate::error::{Error, ErrorCode, ErrorSubject};
use crate::request::{ChartRequest, ChartStyle, PaidIn, RATIO_MAX, RATIO_MIN};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unvisited,
    Visiting,
    Visited,
}

/// Check `request` and copy it into an arena. Folding is a separate step.
pub(crate) fn validate(request: &ChartRequest) -> Result<Graph, Error> {
    if request.subject_id.trim().is_empty() {
        return Err(
            Error::new(ErrorCode::InvalidProtocol, "subject_id is blank")
                .with_subject(ErrorSubject::Field("subject_id")),
        );
    }
    request_bounds(request)?;
    let max_nodes = request.max_nodes;
    if request.nodes.len() > max_nodes {
        return Err(Error::new(
            ErrorCode::TooManyNodes,
            format!(
                "nodes={} exceeds max_nodes={max_nodes}",
                request.nodes.len()
            ),
        )
        .with_subject(ErrorSubject::Field("nodes")));
    }
    let (mut nodes, by_protocol_id) = index_nodes(request)?;
    bind_folded_holders(&mut nodes, &request.nodes, &by_protocol_id)?;
    let Some(&subject) = by_protocol_id.get(request.subject_id.as_str()) else {
        return Err(Error::new(
            ErrorCode::SubjectNotFound,
            format!("subject_id={} not in nodes", request.subject_id),
        )
        .with_subject(ErrorSubject::Field("subject_id")));
    };
    let edges = check_edges(request, &by_protocol_id)?;
    let mut graph = Graph::from_records(
        nodes,
        edges,
        subject,
        request.subject_id.clone(),
        request.style,
        request.max_nodes,
    );
    check_structure(&mut graph)?;
    Ok(graph)
}

/// Rerun the protocol rules that a fold can break: the node cap, cycles, the
/// two cones, a declared stand-in edge, and a unique layout parent.
pub(crate) fn recheck(graph: &mut Graph) -> Result<(), Error> {
    let max_nodes = graph.max_nodes;
    if graph.nodes.len() > max_nodes {
        return Err(Error::new(
            ErrorCode::TooManyNodes,
            format!("nodes={} exceeds max_nodes={max_nodes}", graph.nodes.len()),
        )
        .with_subject(ErrorSubject::Field("nodes")));
    }
    check_structure(graph)
}

fn check_structure(graph: &mut Graph) -> Result<(), Error> {
    detect_cycle(graph)?;
    require_dual_cone(graph)?;
    require_stand_in_edges(graph)?;
    reject_ambiguous_ratios(graph)
}

fn request_bounds(request: &ChartRequest) -> Result<(), Error> {
    let max_nodes = request.max_nodes;
    if !(1..=ChartRequest::MAX_NODES).contains(&max_nodes) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "max_nodes={max_nodes} exceeds limit {}",
                ChartRequest::MAX_NODES
            ),
        )
        .with_subject(ErrorSubject::Field("max_nodes")));
    }
    if let Some(top_n) = request.top_n {
        let top_n = top_n.get();
        if top_n > max_nodes {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("top_n={top_n} must be in 0..={max_nodes}"),
            )
            .with_subject(ErrorSubject::Field("top_n")));
        }
    }
    let ChartStyle {
        margin_mm,
        node_width_mm,
    } = request.style;
    if !(margin_mm.is_finite() && margin_mm > 0.0 && margin_mm <= ChartStyle::MAX_MARGIN_MM) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "margin_mm={margin_mm:?} must be in (0, {}]",
                ChartStyle::MAX_MARGIN_MM
            ),
        )
        .with_subject(ErrorSubject::Field("style.margin_mm")));
    }
    let in_range = node_width_mm.is_finite()
        && (ChartStyle::MIN_NODE_WIDTH_MM..=ChartStyle::MAX_NODE_WIDTH_MM).contains(&node_width_mm);
    if !in_range {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "node_width_mm={node_width_mm:?} must be in [{}, {}]",
                ChartStyle::MIN_NODE_WIDTH_MM,
                ChartStyle::MAX_NODE_WIDTH_MM
            ),
        )
        .with_subject(ErrorSubject::Field("style.node_width_mm")));
    }
    Ok(())
}

fn index_nodes(request: &ChartRequest) -> Result<(Vec<NodeRec>, HashMap<String, NodeId>), Error> {
    let mut by_protocol_id = HashMap::with_capacity(request.nodes.len());
    let mut nodes = Vec::with_capacity(request.nodes.len());
    for (index, node) in request.nodes.iter().enumerate() {
        if node.id.trim().is_empty() || node.name.trim().is_empty() {
            return Err(
                Error::new(ErrorCode::InvalidProtocol, "node id/name blank").with_subject(
                    ErrorSubject::Node {
                        index,
                        id: node.id.clone(),
                    },
                ),
            );
        }
        if by_protocol_id.contains_key(node.id.as_str()) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("duplicate node id={}", node.id),
            )
            .with_subject(ErrorSubject::Node {
                index,
                id: node.id.clone(),
            }));
        }
        let id = NodeId::from_index(index);
        by_protocol_id.insert(node.id.clone(), id);
        let (paid_in_amount, currency) = paid_in_fields(node.paid_in.as_ref());
        nodes.push(NodeRec {
            id,
            origin: NodeOrigin::Caller {
                index,
                id: node.id.clone(),
                stands_in_for: None,
            },
            name: node.name.clone(),
            legal_person: node.legal_person.clone(),
            ratio: node.ratio,
            node_type: node.node_type,
            paid_in_amount,
            currency,
            location: node.location.clone(),
            approval_date: node.approval_date.clone(),
        });
    }
    Ok((nodes, by_protocol_id))
}

fn paid_in_fields(paid_in: Option<&PaidIn>) -> (Option<String>, Option<String>) {
    match paid_in {
        Some(paid) => (Some(paid.amount.clone()), paid.currency.clone()),
        None => (None, None),
    }
}

/// `folded_holders_of` must name one existing node, and only one caller may claim it.
fn bind_folded_holders(
    nodes: &mut [NodeRec],
    request_nodes: &[crate::request::Node],
    by_protocol_id: &HashMap<String, NodeId>,
) -> Result<(), Error> {
    let mut claimed = HashSet::new();
    for (index, (node, request_node)) in nodes.iter_mut().zip(request_nodes).enumerate() {
        let Some(parent_key) = request_node.folded_holders_of.as_deref() else {
            continue;
        };
        let subject = ErrorSubject::Node {
            index,
            id: request_node.id.clone(),
        };
        let Some(&parent) = by_protocol_id.get(parent_key) else {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "node {} folded_holders_of={parent_key} is not a node id",
                    request_node.id
                ),
            )
            .with_subject(subject));
        };
        if !claimed.insert(parent) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "node {} folded_holders_of={parent_key} duplicates another stand-in",
                    request_node.id
                ),
            )
            .with_subject(subject));
        }
        if let NodeOrigin::Caller { stands_in_for, .. } = &mut node.origin {
            *stands_in_for = Some(parent);
        }
    }
    Ok(())
}

fn check_edges(
    request: &ChartRequest,
    by_protocol_id: &HashMap<String, NodeId>,
) -> Result<Vec<EdgeRec>, Error> {
    let mut seen = HashSet::with_capacity(request.edges.len());
    let mut edges = Vec::with_capacity(request.edges.len());
    for (index, edge) in request.edges.iter().enumerate() {
        let (Some(&from), Some(&to)) = (
            by_protocol_id.get(edge.holder.as_str()),
            by_protocol_id.get(edge.held.as_str()),
        ) else {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("dangling edge {}->{}", edge.holder, edge.held),
            )
            .with_subject(ErrorSubject::Edge {
                index,
                holder: edge.holder.clone(),
                held: edge.held.clone(),
            }));
        };
        if from == to {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("self-loop on {}", edge.holder),
            )
            .with_subject(ErrorSubject::Edge {
                index,
                holder: edge.holder.clone(),
                held: edge.held.clone(),
            }));
        }
        if !seen.insert((from, to)) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("duplicate edge {}->{}", edge.holder, edge.held),
            )
            .with_subject(ErrorSubject::Edge {
                index,
                holder: edge.holder.clone(),
                held: edge.held.clone(),
            }));
        }
        require_ratio(
            edge.ratio,
            &format!("edge {}->{}", edge.holder, edge.held),
            ErrorSubject::Edge {
                index,
                holder: edge.holder.clone(),
                held: edge.held.clone(),
            },
        )?;
        edges.push(EdgeRec {
            id: EdgeId::from_index(index),
            from,
            to,
            ratio: edge.ratio,
            caller_index: Some(index),
        });
    }
    for (index, node) in request.nodes.iter().enumerate() {
        require_ratio(
            node.ratio,
            &format!("node {}", node.id),
            ErrorSubject::Node {
                index,
                id: node.id.clone(),
            },
        )?;
    }
    Ok(edges)
}

fn require_stand_in_edges(graph: &Graph) -> Result<(), Error> {
    for node in &graph.nodes {
        let NodeOrigin::Caller {
            stands_in_for: Some(parent),
            id,
            index,
        } = &node.origin
        else {
            continue;
        };
        let linked = graph
            .incoming_edges(*parent)
            .iter()
            .any(|&edge| graph.edge_from(edge) == node.id);
        if !linked {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "node {id} declares folded holders of {} but has no direct edge to it",
                    graph.label(*parent)
                ),
            )
            .with_subject(ErrorSubject::Node {
                index: *index,
                id: id.clone(),
            }));
        }
    }
    Ok(())
}

fn require_ratio(value: Option<f64>, label: &str, subject: ErrorSubject) -> Result<(), Error> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_finite() && (RATIO_MIN..=RATIO_MAX).contains(&value) {
        return Ok(());
    }
    Err(Error::new(
        ErrorCode::InvalidProtocol,
        format!("{label} ratio={value:?} must be finite and in {RATIO_MIN}..{RATIO_MAX}"),
    )
    .with_subject(subject))
}

fn detect_cycle(graph: &Graph) -> Result<(), Error> {
    let out_nodes = &graph.out_nodes;
    let mut state = vec![Visit::Unvisited; out_nodes.len()];
    for start in 0..out_nodes.len() {
        if state[start] != Visit::Unvisited {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state[start] = Visit::Visiting;
        while let Some(&(node, mut child_index)) = stack.last() {
            if child_index < out_nodes[node].len() {
                let target = out_nodes[node][child_index].index();
                child_index += 1;
                if let Some(top) = stack.last_mut() {
                    top.1 = child_index;
                }
                match state[target] {
                    Visit::Visiting => {
                        let from_id = NodeId::from_index(node);
                        let to_id = NodeId::from_index(target);
                        return Err(Error::new(
                            ErrorCode::CycleDetected,
                            format!("cycle at {}->{}", graph.label(from_id), graph.label(to_id)),
                        )
                        .with_optional_subject(graph.subject_between(from_id, to_id)));
                    }
                    Visit::Unvisited => {
                        state[target] = Visit::Visiting;
                        stack.push((target, 0));
                    }
                    Visit::Visited => {}
                }
            } else {
                stack.pop();
                state[node] = Visit::Visited;
            }
        }
    }
    Ok(())
}

/// Shareholder cone, excluding the subject.
pub(crate) fn shareholder_ids(graph: &Graph) -> HashSet<NodeId> {
    walk_cone(graph, Cone::Shareholder)
}

/// Investment cone, excluding the subject.
pub(crate) fn investment_ids(graph: &Graph) -> HashSet<NodeId> {
    walk_cone(graph, Cone::Investment)
}

#[derive(Clone, Copy)]
enum Cone {
    Shareholder,
    Investment,
}

fn walk_cone(graph: &Graph, cone: Cone) -> HashSet<NodeId> {
    let mut into = HashSet::new();
    let mut stack = vec![graph.subject];
    let mut visited = HashSet::from([graph.subject]);
    while let Some(current) = stack.pop() {
        let nexts: Vec<NodeId> = match cone {
            Cone::Shareholder => graph.incoming_sources(current),
            Cone::Investment => graph.out_nodes[current.index()].clone(),
        };
        for next in nexts {
            if visited.insert(next) {
                into.insert(next);
                stack.push(next);
            }
        }
    }
    into
}

fn require_dual_cone(graph: &Graph) -> Result<(), Error> {
    let shareholders = shareholder_ids(graph);
    let investments = investment_ids(graph);
    let mut orphans = Vec::new();
    for node in &graph.nodes {
        if node.id != graph.subject
            && !shareholders.contains(&node.id)
            && !investments.contains(&node.id)
        {
            orphans.push(node.id);
        }
    }
    if !orphans.is_empty() {
        let mut labels: Vec<String> = orphans.iter().map(|id| graph.label(*id)).collect();
        labels.sort_unstable();
        let subject = orphans
            .iter()
            .filter_map(|id| graph.node_subject(*id))
            .min_by_key(|subject| match subject {
                ErrorSubject::Node { index, .. } => *index,
                ErrorSubject::Edge { .. } | ErrorSubject::Field(_) | ErrorSubject::Character(_) => {
                    usize::MAX
                }
            });
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "nodes not connected to subject={}: {}",
                graph.subject_protocol_id,
                labels.join(",")
            ),
        )
        .with_optional_subject(subject));
    }
    for edge in &graph.edges {
        let from = edge.from;
        let to = edge.to;
        let up =
            shareholders.contains(&from) && (shareholders.contains(&to) || to == graph.subject);
        let down =
            investments.contains(&to) && (from == graph.subject || investments.contains(&from));
        // A back edge into the shareholder cone is already a cycle.
        // What remains is a shareholder that also holds an investment.
        if !up && !down {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "edge crosses shareholder/investment cones: {}->{}",
                    graph.label(from),
                    graph.label(to)
                ),
            )
            .with_optional_subject(graph.edge_subject(edge)));
        }
    }
    Ok(())
}

fn reject_ambiguous_ratios(graph: &mut Graph) -> Result<(), Error> {
    for index in 0..graph.nodes.len() {
        let id = NodeId::from_index(index);
        if graph.nodes[index].ratio.is_some() || id == graph.subject {
            continue;
        }
        unique_layout_parent_edge(graph, id)?;
    }
    Ok(())
}

#[cfg(test)]
mod subject_tests {
    use super::super::arena::{EdgeId, EdgeRec, Graph, NodeId, NodeOrigin, NodeRec};
    use super::require_dual_cone;
    use crate::error::{ErrorCode, ErrorSubject};
    use crate::request::ChartStyle;

    fn caller(index: usize, id: &str) -> NodeRec {
        NodeRec {
            id: NodeId::from_index(index),
            origin: NodeOrigin::Caller {
                index,
                id: id.to_owned(),
                stands_in_for: None,
            },
            name: id.to_owned(),
            legal_person: None,
            ratio: None,
            node_type: None,
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        }
    }

    fn folded(index: usize, parent: usize) -> NodeRec {
        NodeRec {
            id: NodeId::from_index(index),
            origin: NodeOrigin::FoldedHolders {
                parent: NodeId::from_index(parent),
                count: 2,
            },
            name: "其他股东(2家)".to_owned(),
            legal_person: None,
            ratio: None,
            node_type: None,
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        }
    }

    fn edge(index: usize, from: usize, to: usize, caller_index: Option<usize>) -> EdgeRec {
        EdgeRec {
            id: EdgeId::from_index(index),
            from: NodeId::from_index(from),
            to: NodeId::from_index(to),
            ratio: None,
            caller_index,
        }
    }

    fn graph(nodes: Vec<NodeRec>, edges: Vec<EdgeRec>) -> Graph {
        Graph::from_records(
            nodes,
            edges,
            NodeId::from_index(0),
            "root".to_owned(),
            ChartStyle::default(),
            100,
        )
    }

    #[test]
    fn a_synthetic_cross_cone_edge_names_the_caller_end() {
        let error = require_dual_cone(&graph(
            vec![caller(0, "root"), caller(1, "invest"), folded(2, 0)],
            vec![
                edge(0, 2, 0, None),
                edge(1, 0, 1, Some(0)),
                edge(2, 2, 1, None),
            ],
        ))
        .expect_err("cross");
        assert_eq!(error.code(), ErrorCode::InvalidProtocol);
        assert_eq!(
            error.subject(),
            Some(&ErrorSubject::Node {
                index: 1,
                id: "invest".to_owned(),
            })
        );
    }

    #[test]
    fn a_folded_orphan_has_no_subject() {
        let error = require_dual_cone(&graph(vec![caller(0, "root"), folded(1, 0)], vec![]))
            .expect_err("orphan");
        assert_eq!(error.code(), ErrorCode::InvalidProtocol);
        assert_eq!(error.subject(), None);
        assert!(error.message().contains("folded holders"));
    }
}
