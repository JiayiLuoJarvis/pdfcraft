//! Shareholder `top_n` fold. Validates the original request, then rebuilds holders.

use std::collections::{HashMap, HashSet};

use super::arena::{EdgeId, Graph, NodeId};
use super::check::{shareholder_ids, validate};
use super::ratio::resolve_display_ratio;
use crate::error::{Error, ErrorCode};
use crate::request::{ChartRequest, Edge, Node, NodeType, RATIO_MAX, RATIO_MIN};

struct Fold {
    parent: NodeId,
    dropped: Vec<EdgeId>,
    total: f64,
}

/// Fold shareholder children so each parent keeps at most `options.top_n`.
///
/// `None` and `Some(0)` return a clone of `request` after it validates.
/// The rebuilt request is not validated. A fold can add a synthetic holder
/// while a dropped holder stays reachable, and that can pass this step and
/// fail a later node-cap check.
pub(crate) fn prepare_request(request: &ChartRequest) -> Result<ChartRequest, Error> {
    let mut graph = validate(request)?;
    let Some(top_n) = request.options.top_n.filter(|top_n| *top_n > 0) else {
        return Ok(request.clone());
    };
    fold_shareholders(&mut graph, top_n)
}

fn fold_shareholders(graph: &mut Graph, top_n: usize) -> Result<ChartRequest, Error> {
    let holders = shareholder_ids(graph);
    let mut parents = vec![graph.subject];
    for node in &graph.nodes {
        if holders.contains(&node.id) {
            parents.push(node.id);
        }
    }
    let mut folds = Vec::new();
    for parent in parents {
        if let Some(fold) = fold_parent(graph, parent, top_n, &holders)? {
            folds.push(fold);
        }
    }
    if folds.is_empty() {
        return Ok(graph.to_request());
    }
    Ok(rebuild(graph, &holders, &folds))
}

fn fold_parent(
    graph: &mut Graph,
    parent: NodeId,
    top_n: usize,
    holders: &HashSet<NodeId>,
) -> Result<Option<Fold>, Error> {
    let parent_protocol = graph.protocol_id(parent).to_owned();
    let synthetic = synthetic_id(&parent_protocol);
    let children: Vec<EdgeId> = graph
        .incoming_edges(parent)
        .iter()
        .copied()
        .filter(|&edge| {
            let from = graph.edge_from(edge);
            holders.contains(&from) && graph.protocol_id(from) != synthetic
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
                    graph.protocol_id(from)
                ),
            )
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
        ));
    }
    if graph.contains_protocol(&synthetic) {
        return Err(Error::new(
            ErrorCode::InvalidProtocol,
            format!("node id {synthetic} is reserved for the folded holders of {parent_protocol}"),
        ));
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

fn rebuild(graph: &Graph, holders: &HashSet<NodeId>, folds: &[Fold]) -> ChartRequest {
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

    let mut nodes: Vec<Node> = graph
        .nodes
        .iter()
        .filter(|node| !removed.contains(&node.id))
        .map(|node| Node {
            id: node.protocol_id.clone(),
            name: node.name.clone(),
            legal_person: node.legal_person.clone(),
            ratio: node.ratio,
            node_type: node.node_type,
            paid_in_amount: node.paid_in_amount.clone(),
            currency: node.currency.clone(),
            location: node.location.clone(),
            approval_date: node.approval_date.clone(),
        })
        .collect();
    for fold in &kept {
        nodes.push(Node {
            id: synthetic_id(graph.protocol_id(fold.parent)),
            name: other_shareholders_name(fold.dropped.len()),
            legal_person: None,
            ratio: Some(fold.total),
            node_type: Some(NodeType::Company),
            paid_in_amount: None,
            currency: None,
            location: None,
            approval_date: None,
        });
    }

    let mut edges: Vec<Edge> = graph
        .edges
        .iter()
        .filter(|edge| {
            !removed.contains(&edge.from)
                && !removed.contains(&edge.to)
                && !folded_edges.contains(&edge.id)
        })
        .map(|edge| Edge {
            from_id: graph.protocol_id(edge.from).to_owned(),
            to_id: graph.protocol_id(edge.to).to_owned(),
            ratio: edge.ratio,
        })
        .collect();
    for fold in &kept {
        edges.push(Edge {
            from_id: synthetic_id(graph.protocol_id(fold.parent)),
            to_id: graph.protocol_id(fold.parent).to_owned(),
            ratio: Some(fold.total),
        });
    }

    ChartRequest {
        subject_id: graph.subject_protocol_id.clone(),
        nodes,
        edges,
        options: graph.options.clone(),
    }
}

fn synthetic_id(parent_id: &str) -> String {
    format!("__other__:{parent_id}")
}

fn other_shareholders_name(count: usize) -> String {
    crate::captions::other_shareholders_name(count)
}

#[cfg(test)]
#[path = "fold_tests.rs"]
mod fold_tests;
