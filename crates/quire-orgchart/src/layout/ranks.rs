//! Hop ranks from the page subject.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, ErrorCode};
use crate::measure::NodeBox;
use crate::page::EdgeRef;

pub(crate) struct Prepared {
    pub(crate) ordered_ids: Vec<String>,
    pub(crate) page_edges: Vec<EdgeRef>,
    pub(crate) ranks: HashMap<String, i32>,
    pub(crate) order_index: HashMap<String, usize>,
    pub(crate) subject_rank: Option<i32>,
    pub(crate) subject_id: Option<String>,
}

pub(crate) fn prepare(
    boxes: &[NodeBox],
    edges: &[EdgeRef],
    node_order: &[String],
) -> Result<Option<Prepared>, Error> {
    let box_ids: HashSet<&str> = boxes.iter().map(|box_| box_.id.as_str()).collect();
    let mut ordered: Vec<String> = node_order
        .iter()
        .filter(|id| box_ids.contains(id.as_str()))
        .cloned()
        .collect();
    if ordered.is_empty() {
        ordered = boxes.iter().map(|box_| box_.id.clone()).collect();
    }
    if ordered.is_empty() {
        return Ok(None);
    }
    let id_set: HashSet<&str> = ordered.iter().map(String::as_str).collect();
    let page_edges = edges
        .iter()
        .filter(|edge| {
            id_set.contains(edge.from_id.as_str()) && id_set.contains(edge.to_id.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let subject_id = boxes.iter().find_map(|box_| {
        if box_.is_subject && id_set.contains(box_.id.as_str()) {
            Some(box_.id.clone())
        } else {
            None
        }
    });
    let ranks = assign_hop_ranks(&ordered, &page_edges, subject_id.as_deref())?;
    let order_index = ordered
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), index))
        .collect();
    let subject_rank = subject_id.as_ref().map(|id| ranks[id]);
    Ok(Some(Prepared {
        ordered_ids: ordered,
        page_edges,
        ranks,
        order_index,
        subject_rank,
        subject_id,
    }))
}

fn assign_hop_ranks(
    ordered_ids: &[String],
    edges: &[EdgeRef],
    subject_id: Option<&str>,
) -> Result<HashMap<String, i32>, Error> {
    if ordered_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let hops = compute_hops(ordered_ids, edges, subject_id)?;
    let mut values: Vec<i32> = hops.values().copied().collect();
    values.sort_unstable();
    values.dedup();
    let remap: HashMap<i32, i32> = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| (value, i32::try_from(index).unwrap_or(i32::MAX)))
        .collect();
    Ok(ordered_ids
        .iter()
        .map(|id| (id.clone(), remap[&hops[id]]))
        .collect())
}

#[allow(clippy::too_many_lines)]
fn compute_hops(
    ordered_ids: &[String],
    edges: &[EdgeRef],
    subject_id: Option<&str>,
) -> Result<HashMap<String, i32>, Error> {
    let id_set: HashSet<&str> = ordered_ids.iter().map(String::as_str).collect();
    let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        incoming
            .entry(edge.to_id.as_str())
            .or_default()
            .push(edge.from_id.as_str());
        outgoing
            .entry(edge.from_id.as_str())
            .or_default()
            .push(edge.to_id.as_str());
    }
    if let Some(subject_id) = subject_id {
        if id_set.contains(subject_id) {
            let mut pending: HashMap<&str, Option<i32>> =
                ordered_ids.iter().map(|id| (id.as_str(), None)).collect();
            pending.insert(subject_id, Some(0));
            let mut queue = VecDeque::from([subject_id]);
            while let Some(node_id) = queue.pop_front() {
                let base = pending[node_id].ok_or_else(|| {
                    Error::new(
                        ErrorCode::LayoutFailed,
                        format!("hop rank missing for {node_id}"),
                    )
                })?;
                if let Some(preds) = incoming.get(node_id) {
                    for pred in preds {
                        if !id_set.contains(pred) {
                            continue;
                        }
                        let cand = base - 1;
                        let current = pending[pred];
                        if current.is_none_or(|value| cand < value) {
                            pending.insert(pred, Some(cand));
                            queue.push_back(pred);
                        }
                    }
                }
                if let Some(succs) = outgoing.get(node_id) {
                    for succ in succs {
                        if !id_set.contains(succ) {
                            continue;
                        }
                        let cand = base + 1;
                        let current = pending[succ];
                        if current.is_none_or(|value| cand > value) {
                            pending.insert(succ, Some(cand));
                            queue.push_back(succ);
                        }
                    }
                }
            }
            return Ok(ordered_ids
                .iter()
                .map(|id| (id.clone(), pending[id.as_str()].unwrap_or(0)))
                .collect());
        }
    }
    let mut hop: HashMap<&str, i32> = ordered_ids.iter().map(|id| (id.as_str(), 0)).collect();
    let mut indegree: HashMap<&str, i32> = ordered_ids.iter().map(|id| (id.as_str(), 0)).collect();
    for edge in edges {
        if id_set.contains(edge.from_id.as_str()) && id_set.contains(edge.to_id.as_str()) {
            if let Some(degree) = indegree.get_mut(edge.to_id.as_str()) {
                *degree += 1;
            }
        }
    }
    let mut queue: VecDeque<&str> = ordered_ids
        .iter()
        .map(String::as_str)
        .filter(|id| indegree[id] == 0)
        .collect();
    let mut seen = HashSet::new();
    while let Some(node_id) = queue.pop_front() {
        if !seen.insert(node_id) {
            continue;
        }
        if let Some(nexts) = outgoing.get(node_id) {
            for next in nexts {
                if !id_set.contains(next) {
                    continue;
                }
                let hop_next = hop[node_id] + 1;
                if hop_next > hop[next] {
                    hop.insert(next, hop_next);
                }
                let Some(degree) = indegree.get_mut(next) else {
                    continue;
                };
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(next);
                }
            }
        }
    }
    Ok(ordered_ids
        .iter()
        .map(|id| (id.clone(), hop[id.as_str()]))
        .collect())
}
