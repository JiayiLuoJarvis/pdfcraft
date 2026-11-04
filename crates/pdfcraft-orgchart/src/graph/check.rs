//! Protocol checks for one chart: ids, edges, cycles, and the two cones.

use std::collections::{HashMap, HashSet};

use super::arena::{EdgeId, EdgeRec, Graph, NodeId, NodeOrigin, NodeRec, OVERFLOW_ID_PREFIX};
use super::ratio::unique_layout_parent_edge;
use crate::error::{Error, ErrorCode};
use crate::request::{
    ChartOptions, ChartRequest, MAX_MARGIN_MM, MAX_NODES, MAX_NODE_WIDTH_MM, MIN_NODE_WIDTH_MM,
    RATIO_MAX, RATIO_MIN,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    Unvisited,
    Visiting,
    Visited,
}

/// Check `request` and copy it into an arena. Folding is a separate step.
pub(crate) fn validate(request: &ChartRequest) -> Result<Graph, Error> {
    if request.subject_id.trim().is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            "subject_id is blank",
        ));
    }
    let max_nodes = option_bounds(&request.options)?;
    if request.nodes.len() > max_nodes {
        return Err(Error::new(
            ErrorCode::TooManyNodes,
            format!(
                "nodes={} exceeds max_nodes={max_nodes}",
                request.nodes.len()
            ),
        ));
    }
    let (nodes, by_protocol_id) = index_nodes(request)?;
    let Some(&subject) = by_protocol_id.get(request.subject_id.as_str()) else {
        return Err(Error::new(
            ErrorCode::SubjectNotFound,
            format!("subject_id={} not in nodes", request.subject_id),
        ));
    };
    let edges = check_edges(request, &by_protocol_id)?;
    let node_count = nodes.len();
    let mut outgoing = vec![Vec::new(); node_count];
    let mut incoming = vec![Vec::new(); node_count];
    let mut out_nodes = vec![Vec::new(); node_count];
    for edge in &edges {
        outgoing[edge.from.index()].push(edge.id);
        incoming[edge.to.index()].push(edge.id);
        out_nodes[edge.from.index()].push(edge.to);
    }
    detect_cycle(&out_nodes, &nodes)?;
    let mut graph = Graph {
        nodes,
        edges,
        by_protocol_id,
        outgoing,
        incoming,
        out_nodes,
        subject,
        subject_protocol_id: request.subject_id.clone(),
        options: request.options.clone(),
        reach_cache: HashMap::new(),
        overflow_ids: HashMap::new(),
    };
    require_dual_cone(&graph)?;
    reject_ambiguous_ratios(&mut graph)?;
    Ok(graph)
}

fn option_bounds(options: &ChartOptions) -> Result<usize, Error> {
    if let Some(max_nodes) = options.max_nodes {
        if !(1..=MAX_NODES).contains(&max_nodes) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("max_nodes={max_nodes} exceeds limit {MAX_NODES}"),
            ));
        }
    }
    let max_nodes = options.max_nodes.unwrap_or(MAX_NODES);
    if let Some(top_n) = options.top_n {
        if top_n > max_nodes {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("top_n={top_n} must be in 0..{max_nodes}"),
            ));
        }
    }
    if let Some(margin_mm) = options.margin_mm {
        if !(margin_mm.is_finite() && margin_mm > 0.0 && margin_mm <= MAX_MARGIN_MM) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("margin_mm={margin_mm:?} must be in (0, {MAX_MARGIN_MM}]"),
            ));
        }
    }
    if let Some(node_width_mm) = options.node_width_mm {
        let in_range = node_width_mm.is_finite()
            && (MIN_NODE_WIDTH_MM..=MAX_NODE_WIDTH_MM).contains(&node_width_mm);
        if !in_range {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "node_width_mm={node_width_mm:?} must be in [{MIN_NODE_WIDTH_MM}, {MAX_NODE_WIDTH_MM}]"
                ),
            ));
        }
    }
    Ok(max_nodes)
}

fn index_nodes(request: &ChartRequest) -> Result<(Vec<NodeRec>, HashMap<String, NodeId>), Error> {
    let mut by_protocol_id = HashMap::with_capacity(request.nodes.len());
    let mut nodes = Vec::with_capacity(request.nodes.len());
    for (index, node) in request.nodes.iter().enumerate() {
        if node.id.trim().is_empty() || node.name.trim().is_empty() {
            return Err(Error::new(ErrorCode::InvalidProtocol, "node id/name blank"));
        }
        if by_protocol_id.contains_key(node.id.as_str()) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("duplicate node id={}", node.id),
            ));
        }
        if node.id.starts_with(OVERFLOW_ID_PREFIX) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "node id uses reserved prefix {OVERFLOW_ID_PREFIX}: {}",
                    node.id
                ),
            ));
        }
        let id = NodeId::from_index(index);
        let origin = folded_origin(&node.id, &by_protocol_id).unwrap_or(NodeOrigin::Input);
        by_protocol_id.insert(node.id.clone(), id);
        nodes.push(NodeRec {
            id,
            origin,
            protocol_id: node.id.clone(),
            name: node.name.clone(),
            legal_person: node.legal_person.clone(),
            ratio: node.ratio,
            node_type: node.node_type,
            paid_in_amount: node.paid_in_amount.clone(),
            currency: node.currency.clone(),
            location: node.location.clone(),
            approval_date: node.approval_date.clone(),
        });
    }
    Ok((nodes, by_protocol_id))
}

/// When a later node is `__other__:{parent}` and that parent is already indexed,
/// treat it as a fold stand-in. Caller-written `__other__:` ids whose parent is
/// missing stay [`NodeOrigin::Input`].
fn folded_origin(
    protocol_id: &str,
    by_protocol_id: &HashMap<String, NodeId>,
) -> Option<NodeOrigin> {
    let parent_protocol = protocol_id.strip_prefix("__other__:")?;
    let parent = *by_protocol_id.get(parent_protocol)?;
    Some(NodeOrigin::FoldedHolders { parent })
}

fn check_edges(
    request: &ChartRequest,
    by_protocol_id: &HashMap<String, NodeId>,
) -> Result<Vec<EdgeRec>, Error> {
    let mut seen = HashSet::with_capacity(request.edges.len());
    let mut edges = Vec::with_capacity(request.edges.len());
    for (index, edge) in request.edges.iter().enumerate() {
        let (Some(&from), Some(&to)) = (
            by_protocol_id.get(edge.from_id.as_str()),
            by_protocol_id.get(edge.to_id.as_str()),
        ) else {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("dangling edge {}->{}", edge.from_id, edge.to_id),
            ));
        };
        if from == to {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("self-loop on {}", edge.from_id),
            ));
        }
        if !seen.insert((from, to)) {
            return Err(Error::new(
                ErrorCode::InvalidProtocol,
                format!("duplicate edge {}->{}", edge.from_id, edge.to_id),
            ));
        }
        require_ratio(
            edge.ratio,
            &format!("edge {}->{}", edge.from_id, edge.to_id),
        )?;
        edges.push(EdgeRec {
            id: EdgeId::from_index(index),
            from,
            to,
            ratio: edge.ratio,
        });
    }
    for node in &request.nodes {
        require_ratio(node.ratio, &format!("node {}", node.id))?;
    }
    Ok(edges)
}

fn require_ratio(value: Option<f64>, label: &str) -> Result<(), Error> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_finite() && (RATIO_MIN..=RATIO_MAX).contains(&value) {
        return Ok(());
    }
    Err(Error::new(
        ErrorCode::InvalidProtocol,
        format!("{label} ratio={value:?} must be finite and in {RATIO_MIN}..{RATIO_MAX}"),
    ))
}

fn detect_cycle(out_nodes: &[Vec<NodeId>], nodes: &[NodeRec]) -> Result<(), Error> {
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
                        return Err(Error::new(
                            ErrorCode::CycleDetected,
                            format!(
                                "cycle at {}->{}",
                                nodes[node].protocol_id, nodes[target].protocol_id
                            ),
                        ));
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
            orphans.push(node.protocol_id.as_str());
        }
    }
    if !orphans.is_empty() {
        orphans.sort_unstable();
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "nodes not connected to subject={}: {}",
                graph.subject_protocol_id,
                orphans.join(",")
            ),
        ));
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
                    graph.protocol_id(from),
                    graph.protocol_id(to)
                ),
            ));
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
