//! Display percents and the unique layout-parent edge.

use std::collections::{HashSet, VecDeque};

use super::arena::{EdgeId, Graph, NodeId};
use crate::error::{Error, ErrorCode};

pub(crate) fn unique_layout_parent_edge(
    graph: &mut Graph,
    node: NodeId,
) -> Result<Option<EdgeId>, Error> {
    if node == graph.subject {
        return Ok(None);
    }
    let to_subject: Vec<EdgeId> = graph.outgoing[node.index()]
        .iter()
        .copied()
        .filter(|&edge| graph.edge_to(edge) == graph.subject)
        .collect();
    if !to_subject.is_empty() {
        return single_edge(graph, &to_subject, node).map(Some);
    }
    let from_subject: Vec<EdgeId> = graph.incoming[node.index()]
        .iter()
        .copied()
        .filter(|&edge| graph.edge_from(edge) == graph.subject)
        .collect();
    if !from_subject.is_empty() {
        return single_edge(graph, &from_subject, node).map(Some);
    }
    let outgoing = graph.outgoing[node.index()].clone();
    let mut down = Vec::new();
    for edge in outgoing {
        let to = graph.edge_to(edge);
        if reaches(graph, to, graph.subject) {
            down.push(edge);
        }
    }
    if !down.is_empty() {
        return single_edge(graph, &down, node).map(Some);
    }
    let incoming = graph.incoming[node.index()].clone();
    let mut up = Vec::new();
    for edge in incoming {
        let from = graph.edge_from(edge);
        if from == graph.subject || reaches(graph, graph.subject, from) {
            up.push(edge);
        }
    }
    if !up.is_empty() {
        return single_edge(graph, &up, node).map(Some);
    }
    Ok(None)
}

fn single_edge(graph: &Graph, edges: &[EdgeId], node: NodeId) -> Result<EdgeId, Error> {
    if let [only] = edges {
        Ok(*only)
    } else {
        Err(ambiguous(graph, edges, node))
    }
}

fn ambiguous(graph: &Graph, edges: &[EdgeId], node: NodeId) -> Error {
    let ratios: Vec<Option<f64>> = edges
        .iter()
        .map(|&edge| graph.edges[edge.index()].ratio)
        .collect();
    Error::new(
        ErrorCode::InvalidProtocol,
        format!(
            "ambiguous parent edges for {} count={} ratios={ratios:?}",
            graph.protocol_id(node),
            edges.len()
        ),
    )
}

fn reaches(graph: &mut Graph, from: NodeId, target: NodeId) -> bool {
    if let Some(found) = graph.reach_cache.get(&(from, target)) {
        return *found;
    }
    if from == target {
        graph.reach_cache.insert((from, target), true);
        return true;
    }
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([from]);
    let mut found = false;
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current) {
            continue;
        }
        for &next in &graph.out_nodes[current.index()] {
            if next == target {
                found = true;
                break;
            }
            queue.push_back(next);
        }
        if found {
            break;
        }
    }
    graph.reach_cache.insert((from, target), found);
    found
}

/// Where a display percent came from. `parent` is set only when the value is an edge ratio.
pub(crate) struct RatioSource {
    pub(crate) value: f64,
    pub(crate) parent: Option<EdgeId>,
}

/// Display percent for `node`, plus the parent edge when the percent came from that edge.
///
/// A ratio stored on the node wins, and a diamond then does not need one parent.
/// The subject has no display ratio. Otherwise the unique layout parent edge
/// supplies the percent, or `None` when that edge has no ratio.
pub(crate) fn ratio_source(graph: &mut Graph, node: NodeId) -> Result<Option<RatioSource>, Error> {
    if let Some(ratio) = graph.nodes[node.index()].ratio {
        return Ok(Some(RatioSource {
            value: ratio,
            parent: None,
        }));
    }
    if node == graph.subject {
        return Ok(None);
    }
    let Some(parent) = unique_layout_parent_edge(graph, node)? else {
        return Ok(None);
    };
    let Some(value) = graph.edges[parent.index()].ratio else {
        return Ok(None);
    };
    Ok(Some(RatioSource {
        value,
        parent: Some(parent),
    }))
}

/// Display percent for `node`. See [`ratio_source`].
pub(crate) fn resolve_display_ratio(graph: &mut Graph, node: NodeId) -> Result<Option<f64>, Error> {
    Ok(ratio_source(graph, node)?.map(|source| source.value))
}
