//! Hop ranks from the page subject.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, ErrorCode};
use crate::graph::NodeId;
use crate::page::{EdgeRef, NodeBox};

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
    let ordered = if node_order.is_empty() {
        boxes.iter().map(|box_| box_.id).collect()
    } else {
        let mut seen = HashSet::new();
        let mut ordered = Vec::new();
        for id in node_order {
            if !box_ids.contains(id) {
                continue;
            }
            if seen.insert(*id) {
                ordered.push(*id);
            }
        }
        for box_ in boxes {
            if !seen.contains(&box_.id) {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    format!("box {} is not listed in node_order", box_.id),
                ));
            }
        }
        ordered
    };
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
    let subject_rank =
        subject_id.map(|id| *super::map_get(&ranks, &id, "page subject is in the hop rank map"));
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
        .map(|id| {
            let hop = *super::map_get(&hops, &id, "ordered node has a hop");
            (
                id,
                *super::map_get(&remap, &hop, "hop value has a remapped rank"),
            )
        })
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
            return hops_from_subject(ordered_ids, &id_set, &incoming, &outgoing, subject_id);
        }
    }
    Ok(hops_by_indegree(ordered_ids, edges, &id_set, &outgoing))
}

fn hops_from_subject(
    ordered_ids: &[NodeId],
    id_set: &HashSet<NodeId>,
    incoming: &HashMap<NodeId, Vec<NodeId>>,
    outgoing: &HashMap<NodeId, Vec<NodeId>>,
    subject_id: NodeId,
) -> Result<HashMap<NodeId, i32>, Error> {
    let mut pending: HashMap<NodeId, Option<i32>> =
        ordered_ids.iter().copied().map(|id| (id, None)).collect();
    pending.insert(subject_id, Some(0));
    let mut queue = VecDeque::from([subject_id]);
    while let Some(node_id) = queue.pop_front() {
        let base = super::map_get(&pending, &node_id, "queued node is in the hop map").ok_or_else(
            || {
                Error::new(
                    ErrorCode::LayoutFailed,
                    format!("hop rank missing for {node_id}"),
                )
            },
        )?;
        if let Some(preds) = incoming.get(&node_id) {
            for pred in preds {
                if !id_set.contains(pred) {
                    continue;
                }
                let cand = base - 1;
                let current = *super::map_get(&pending, pred, "predecessor is in the hop map");
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
                let current = *super::map_get(&pending, succ, "successor is in the hop map");
                if current.is_none_or(|value| cand > value) {
                    pending.insert(*succ, Some(cand));
                    queue.push_back(*succ);
                }
            }
        }
    }
    // A node the subject walk never reaches keeps hop 0.
    Ok(ordered_ids
        .iter()
        .copied()
        .map(|id| {
            (
                id,
                super::map_get(&pending, &id, "ordered node is in the hop map").unwrap_or(0),
            )
        })
        .collect())
}

fn hops_by_indegree(
    ordered_ids: &[NodeId],
    edges: &[EdgeRef],
    id_set: &HashSet<NodeId>,
    outgoing: &HashMap<NodeId, Vec<NodeId>>,
) -> HashMap<NodeId, i32> {
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
        .filter(|id| *super::map_get(&indegree, id, "ordered node has an indegree") == 0)
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
                let hop_next = *super::map_get(&hop, &node_id, "queued node has a hop") + 1;
                if hop_next > *super::map_get(&hop, next, "successor has a hop") {
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
    ordered_ids
        .iter()
        .copied()
        .map(|id| (id, *super::map_get(&hop, &id, "ordered node has a hop")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::prepare;
    use crate::error::ErrorCode;
    use crate::graph::{NodeId, NodeOrigin};
    use crate::page::NodeBox;

    fn box_of(index: usize, subject: bool) -> NodeBox {
        NodeBox {
            id: NodeId::from_index(index),
            origin: NodeOrigin::Caller {
                index,
                id: format!("n{index}"),
                stands_in_for: None,
            },
            width_mm: 38.0,
            height_mm: 10.0,
            lines: Vec::new(),
            is_subject: subject,
            is_company: false,
        }
    }

    #[test]
    fn extra_node_order_ids_are_ignored() {
        let boxes = [box_of(0, true)];
        let prepared = prepare(&boxes, &[], &[NodeId::from_index(0), NodeId::from_index(9)])
            .expect("ok")
            .expect("some");
        assert_eq!(prepared.ordered_ids, vec![NodeId::from_index(0)]);
    }

    #[test]
    fn box_missing_from_node_order_is_layout_failed() {
        let boxes = [box_of(0, true), box_of(1, false)];
        match prepare(&boxes, &[], &[NodeId::from_index(0)]) {
            Err(error) => assert_eq!(error.code(), ErrorCode::LayoutFailed),
            Ok(_) => panic!("expected LAYOUT_FAILED"),
        }
    }

    #[test]
    fn node_order_is_deduped() {
        let boxes = [box_of(0, true), box_of(1, false)];
        let prepared = prepare(
            &boxes,
            &[],
            &[
                NodeId::from_index(0),
                NodeId::from_index(1),
                NodeId::from_index(0),
            ],
        )
        .expect("ok")
        .expect("some");
        assert_eq!(
            prepared.ordered_ids,
            vec![NodeId::from_index(0), NodeId::from_index(1)]
        );
    }

    #[test]
    fn empty_node_order_uses_all_boxes() {
        let boxes = [box_of(0, true), box_of(1, false)];
        let prepared = prepare(&boxes, &[], &[]).expect("ok").expect("some");
        assert_eq!(
            prepared.ordered_ids,
            vec![NodeId::from_index(0), NodeId::from_index(1)]
        );
    }
}
