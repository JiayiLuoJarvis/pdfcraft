//! Hop ranks from the page subject.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, ErrorCode};
use crate::ids::NodeId;
use crate::measure::NodeBox;
use crate::page::EdgeRef;

pub(crate) struct Prepared {
    pub(crate) ordered_ids: Vec<NodeId>,
    pub(crate) page_edges: Vec<EdgeRef>,
    pub(crate) ranks: HashMap<NodeId, i32>,
    pub(crate) order_index: HashMap<NodeId, usize>,
    pub(crate) subject_rank: Option<i32>,
    pub(crate) subject_id: Option<NodeId>,
}

pub(crate) fn prepare(
    boxes: &[NodeBox],
    edges: &[EdgeRef],
    node_order: &[NodeId],
) -> Result<Option<Prepared>, Error> {
    let box_ids: HashSet<NodeId> = boxes.iter().map(|box_| box_.id).collect();
    let mut ordered: Vec<NodeId> = node_order
        .iter()
        .copied()
        .filter(|id| box_ids.contains(id))
        .collect();
    if ordered.is_empty() {
        ordered = boxes.iter().map(|box_| box_.id).collect();
    }
    if ordered.is_empty() {
        return Ok(None);
    }
    let id_set: HashSet<NodeId> = ordered.iter().copied().collect();
    let page_edges = edges
        .iter()
        .filter(|edge| id_set.contains(&edge.from_id) && id_set.contains(&edge.to_id))
        .cloned()
        .collect::<Vec<_>>();
    let subject_id = boxes.iter().find_map(|box_| {
        if box_.is_subject && id_set.contains(&box_.id) {
            Some(box_.id)
        } else {
            None
        }
    });
    let ranks = assign_hop_ranks(&ordered, &page_edges, subject_id)?;
    let order_index = ordered
        .iter()
        .copied()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect();
    let subject_rank = subject_id.map(|id| ranks[&id]);
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
    ordered_ids: &[NodeId],
    edges: &[EdgeRef],
    subject_id: Option<NodeId>,
) -> Result<HashMap<NodeId, i32>, Error> {
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
        .copied()
        .map(|id| (id, remap[&hops[&id]]))
        .collect())
}

fn compute_hops(
    ordered_ids: &[NodeId],
    edges: &[EdgeRef],
    subject_id: Option<NodeId>,
) -> Result<HashMap<NodeId, i32>, Error> {
    let id_set: HashSet<NodeId> = ordered_ids.iter().copied().collect();
    let mut incoming: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut outgoing: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for edge in edges {
        incoming.entry(edge.to_id).or_default().push(edge.from_id);
        outgoing.entry(edge.from_id).or_default().push(edge.to_id);
    }
    if let Some(subject_id) = subject_id {
        if id_set.contains(&subject_id) {
            let mut pending: HashMap<NodeId, Option<i32>> =
                ordered_ids.iter().copied().map(|id| (id, None)).collect();
            pending.insert(subject_id, Some(0));
            let mut queue = VecDeque::from([subject_id]);
            while let Some(node_id) = queue.pop_front() {
                let base = pending[&node_id].ok_or_else(|| {
                    Error::new(
                        ErrorCode::LayoutFailed,
                        format!("hop rank missing for {node_id}"),
                    )
                })?;
                if let Some(preds) = incoming.get(&node_id) {
                    for pred in preds {
                        if !id_set.contains(pred) {
                            continue;
                        }
                        let cand = base - 1;
                        let current = pending[pred];
                        if current.is_none_or(|value| cand < value) {
                            pending.insert(*pred, Some(cand));
                            queue.push_back(*pred);
                        }
                    }
                }
                if let Some(succs) = outgoing.get(&node_id) {
                    for succ in succs {
                        if !id_set.contains(succ) {
                            continue;
                        }
                        let cand = base + 1;
                        let current = pending[succ];
                        if current.is_none_or(|value| cand > value) {
                            pending.insert(*succ, Some(cand));
                            queue.push_back(*succ);
                        }
                    }
                }
            }
            // A node the subject walk never reaches keeps hop 0.
            return Ok(ordered_ids
                .iter()
                .copied()
                .map(|id| (id, pending[&id].unwrap_or(0)))
                .collect());
        }
    }
    let mut hop: HashMap<NodeId, i32> = ordered_ids.iter().copied().map(|id| (id, 0)).collect();
    let mut indegree: HashMap<NodeId, i32> =
        ordered_ids.iter().copied().map(|id| (id, 0)).collect();
    for edge in edges {
        if id_set.contains(&edge.from_id) && id_set.contains(&edge.to_id) {
            if let Some(degree) = indegree.get_mut(&edge.to_id) {
                *degree += 1;
            }
        }
    }
    let mut queue: VecDeque<NodeId> = ordered_ids
        .iter()
        .copied()
        .filter(|id| indegree[id] == 0)
        .collect();
    let mut seen = HashSet::new();
    while let Some(node_id) = queue.pop_front() {
        if !seen.insert(node_id) {
            continue;
        }
        if let Some(nexts) = outgoing.get(&node_id) {
            for next in nexts {
                if !id_set.contains(next) {
                    continue;
                }
                let hop_next = hop[&node_id] + 1;
                if hop_next > hop[next] {
                    hop.insert(*next, hop_next);
                }
                let Some(degree) = indegree.get_mut(next) else {
                    continue;
                };
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(*next);
                }
            }
        }
    }
    Ok(ordered_ids
        .iter()
        .copied()
        .map(|id| (id, hop[&id]))
        .collect())
}
