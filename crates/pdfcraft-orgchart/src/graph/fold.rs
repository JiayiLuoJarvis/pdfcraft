//! Shareholder `top_n` fold. Builds a new arena from a validated one.

use std::collections::{HashMap, HashSet};

use super::arena::{EdgeId, EdgeRec, Graph, NodeId, NodeOrigin, NodeRec};
use super::check::{shareholder_ids, validate};
use super::ratio::resolve_display_ratio;
use crate::error::{Error, ErrorCode};
use crate::request::{ChartRequest, NodeType, RATIO_MAX, RATIO_MIN};

struct Fold {
    parent: NodeId,
    dropped: Vec<EdgeId>,
    total: f64,
}

/// Validate `request`, then fold shareholder children so each parent keeps at most
/// `top_n`. `None` returns the validated graph unchanged.
///
/// The folded arena is not checked again. A fold can add a synthetic holder
/// while a dropped holder stays reachable, and that can pass this step and
/// fail [`recheck`](super::recheck).
pub(crate) fn prepare_graph(request: &ChartRequest) -> Result<Graph, Error> {
    let graph = validate(request)?;
    match request.top_n {
        Some(top_n) => fold(graph, top_n.get()),
        None => Ok(graph),
    }
}

/// Fold shareholder children of every parent down to `top_n`, into a new arena.
///
/// Surviving nodes keep their order, then one [`NodeOrigin::FoldedHolders`] per
/// kept fold. Surviving edges keep their order, then one edge per kept fold.
pub(crate) fn fold(mut graph: Graph, top_n: usize) -> Result<Graph, Error> {
    let holders = shareholder_ids(&graph);
    let mut parents = vec![graph.subject];
    for node in &graph.nodes {
        if holders.contains(&node.id) {
            parents.push(node.id);
        }
    }
    let mut folds = Vec::new();
    for parent in parents {
        if let Some(fold) = fold_parent(&mut graph, parent, top_n, &holders)? {
            folds.push(fold);
        }
    }
    if folds.is_empty() {
        return Ok(graph);
    }
    folded_arena(&graph, &holders, &folds)
}

fn fold_parent(
    graph: &mut Graph,
    parent: NodeId,
    top_n: usize,
    holders: &HashSet<NodeId>,
) -> Result<Option<Fold>, Error> {
    let parent_protocol = graph.label(parent);
    let children: Vec<EdgeId> = graph
        .incoming_edges(parent)
        .iter()
        .copied()
        .filter(|&edge| {
            let from = graph.edge_from(edge);
            holders.contains(&from) && !graph.origin(from).stands_in_for(parent)
        })
        .collect();
    let mut ratios = HashMap::with_capacity(children.len());
    for &edge in &children {
        let from = graph.edge_from(edge);
        let ratio = resolve_display_ratio(graph, from)?.ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidProtocol,
                format!(
                    "top_n requires a display ratio for {} under {parent_protocol}",
                    graph.label(from)
                ),
            )
            .with_optional_subject(graph.node_subject(from))
        })?;
        ratios.insert(from, ratio);
    }
    if children.len() <= top_n {
        return Ok(None);
    }
    let mut ranked = children;
    ranked.sort_by(|&left, &right| {
        let left_from = graph.edge_from(left);
        let right_from = graph.edge_from(right);
        ratios[&right_from]
            .total_cmp(&ratios[&left_from])
            .then(left_from.cmp(&right_from))
    });
    let dropped = ranked.split_off(top_n);
    let dropped_ratios: Vec<f64> = dropped
        .iter()
        .map(|&edge| ratios[&graph.edge_from(edge)])
        .collect();
    let total = exact_sum(&dropped_ratios);
    if !total.is_finite() || !(RATIO_MIN..=RATIO_MAX).contains(&total) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "folded ratio sum={total:?} under {parent_protocol} must be in {RATIO_MIN}..{RATIO_MAX}"
            ),
        )
        .with_optional_subject(graph.node_subject(parent)));
    }
    if let Some(existing) = graph
        .nodes
        .iter()
        .find(|node| node.origin.stands_in_for(parent))
    {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!(
                "node {} already stands in for the folded holders of {parent_protocol}",
                graph.label(existing.id)
            ),
        )
        .with_optional_subject(graph.node_subject(existing.id)));
    }
    Ok(Some(Fold {
        parent,
        dropped,
        total,
    }))
}

/// Finite sum matching `CPython` `math.fsum`, including the half-even correction.
fn exact_sum(values: &[f64]) -> f64 {
    let mut partials: Vec<f64> = Vec::new();
    for &value in values {
        let mut x = value;
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if x.abs() < y.abs() {
                std::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let yr = hi - x;
            let lo = y - yr;
            if nonzero(lo) {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        partials.truncate(i);
        if nonzero(x) {
            partials.push(x);
        }
    }
    if partials.is_empty() {
        return 0.0;
    }
    let mut n = partials.len() - 1;
    let mut hi = partials[n];
    let mut lo = 0.0;
    while n > 0 {
        let x = hi;
        n -= 1;
        let y = partials[n];
        hi = x + y;
        let yr = hi - x;
        lo = y - yr;
        if nonzero(lo) {
            break;
        }
    }
    if n > 0 {
        let next = partials[n - 1];
        if (lo < 0.0 && next < 0.0) || (lo > 0.0 && next > 0.0) {
            let y = lo * 2.0;
            let x = hi + y;
            let yr = x - hi;
            if same_float(y, yr) {
                hi = x;
            }
        }
    }
    hi
}

fn nonzero(value: f64) -> bool {
    value.to_bits() != 0.0_f64.to_bits() && value.to_bits() != (-0.0_f64).to_bits()
}

fn same_float(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits()
        || (left.to_bits() == 0.0_f64.to_bits() && right.to_bits() == (-0.0_f64).to_bits())
        || (left.to_bits() == (-0.0_f64).to_bits() && right.to_bits() == 0.0_f64.to_bits())
}

fn folded_arena(graph: &Graph, holders: &HashSet<NodeId>, folds: &[Fold]) -> Result<Graph, Error> {
    let folded_edges: HashSet<EdgeId> = folds
        .iter()
        .flat_map(|fold| fold.dropped.iter().copied())
        .collect();
    let mut alive = HashSet::new();
    let mut stack = vec![graph.subject];
    while let Some(current) = stack.pop() {
        for &edge in graph.incoming_edges(current) {
            let from = graph.edge_from(edge);
            if holders.contains(&from) && !folded_edges.contains(&edge) && alive.insert(from) {
                stack.push(from);
            }
        }
    }
    let removed: HashSet<NodeId> = holders.difference(&alive).copied().collect();
    let kept: Vec<&Fold> = folds
        .iter()
        .filter(|fold| !removed.contains(&fold.parent))
        .collect();

    let (mut nodes, new_id) = carried_nodes(graph, &removed)?;
    let remap = |old: NodeId| remap(graph, &new_id, old);
    let mut stand_ins = Vec::with_capacity(kept.len());
    for fold in &kept {
        let id = NodeId::from_index(nodes.len());
        let parent = remap(fold.parent)?;
        nodes.push(NodeRec {
            id,
            origin: NodeOrigin::FoldedHolders {
                parent,
                count: fold.dropped.len(),
            },
            name: other_shareholders_name(fold.dropped.len()),
            legal_person: None,
            ratio: Some(fold.total),
            node_type: Some(NodeType::Company),
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        });
        stand_ins.push((id, parent, fold.total));
    }

    let mut edges = Vec::with_capacity(graph.edges.len());
    for edge in &graph.edges {
        if removed.contains(&edge.from)
            || removed.contains(&edge.to)
            || folded_edges.contains(&edge.id)
        {
            continue;
        }
        edges.push(EdgeRec {
            id: EdgeId::from_index(edges.len()),
            from: remap(edge.from)?,
            to: remap(edge.to)?,
            ratio: edge.ratio,
            caller_index: edge.caller_index,
        });
    }
    for (from, to, total) in stand_ins {
        edges.push(EdgeRec {
            id: EdgeId::from_index(edges.len()),
            from,
            to,
            ratio: Some(total),
            caller_index: None,
        });
    }

    Ok(Graph::from_records(
        nodes,
        edges,
        remap(graph.subject)?,
        graph.subject_protocol_id.clone(),
        graph.style,
        graph.max_nodes,
    ))
}

/// Surviving nodes in arena order with fresh ids, and the old-to-new id map.
/// Overflow nodes are dropped.
fn carried_nodes(
    graph: &Graph,
    removed: &HashSet<NodeId>,
) -> Result<(Vec<NodeRec>, HashMap<NodeId, NodeId>), Error> {
    let surviving: Vec<&NodeRec> = graph
        .nodes
        .iter()
        .filter(|node| !removed.contains(&node.id) && !node.origin.is_overflow())
        .collect();
    let new_id: HashMap<NodeId, NodeId> = surviving
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id, NodeId::from_index(index)))
        .collect();
    let mut nodes = Vec::with_capacity(surviving.len());
    for node in surviving {
        let origin = match &node.origin {
            NodeOrigin::Caller {
                index,
                id,
                stands_in_for,
            } => NodeOrigin::Caller {
                index: *index,
                id: id.clone(),
                stands_in_for: stands_in_for.and_then(|parent| new_id.get(&parent).copied()),
            },
            NodeOrigin::FoldedHolders { parent, count } => NodeOrigin::FoldedHolders {
                parent: remap(graph, &new_id, *parent)?,
                count: *count,
            },
            NodeOrigin::Overflow { .. } => continue,
        };
        nodes.push(NodeRec {
            id: NodeId::from_index(nodes.len()),
            origin,
            ..node.clone()
        });
    }
    Ok((nodes, new_id))
}

fn remap(graph: &Graph, new_id: &HashMap<NodeId, NodeId>, old: NodeId) -> Result<NodeId, Error> {
    new_id.get(&old).copied().ok_or_else(|| {
        Error::new(
            ErrorCode::LayoutFailed,
            format!(
                "fold dropped {} but kept a node that refers to it",
                graph.label(old)
            ),
        )
        .with_optional_subject(graph.node_subject(old))
    })
}

fn other_shareholders_name(count: usize) -> String {
    crate::captions::other_shareholders_name(count)
}

#[cfg(test)]
#[path = "fold_tests.rs"]
mod fold_tests;
