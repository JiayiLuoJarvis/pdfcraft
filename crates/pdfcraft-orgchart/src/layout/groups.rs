//! Sibling groups and row bands.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ids::NodeId;
use crate::page::EdgeRef;

const MISSING_ORDER: usize = usize::MAX;

type RankGroups = BTreeMap<i32, Vec<Vec<NodeId>>>;
type RankParents = BTreeMap<i32, Vec<NodeId>>;

pub(crate) fn use_family_columns(
    subject_rank: Option<i32>,
    layer_rank: i32,
    group_count: usize,
) -> bool {
    subject_rank.is_some_and(|rank| layer_rank != rank && group_count >= 2)
}

pub(crate) fn sibling_groups(
    layer_ids: &[NodeId],
    edges: &[EdgeRef],
    ranks: &HashMap<NodeId, i32>,
    order_index: &HashMap<NodeId, usize>,
    subject_rank: Option<i32>,
) -> Vec<Vec<NodeId>> {
    if layer_ids.is_empty() {
        return Vec::new();
    }
    let mut buckets: BTreeMap<(usize, NodeId), Vec<NodeId>> = BTreeMap::new();
    for node_id in layer_ids {
        let parent = layout_parent(*node_id, edges, ranks, order_index, subject_rank);
        let key = (
            order_index.get(&parent).copied().unwrap_or(MISSING_ORDER),
            parent,
        );
        buckets.entry(key).or_default().push(*node_id);
    }
    buckets
        .into_values()
        .map(|mut group| {
            group.sort_by_key(|id| order_index.get(id).copied().unwrap_or(MISSING_ORDER));
            group
        })
        .collect()
}

pub(crate) fn collect_layer_groups(
    by_rank: &BTreeMap<i32, Vec<NodeId>>,
    edges: &[EdgeRef],
    ranks: &HashMap<NodeId, i32>,
    order_index: &HashMap<NodeId, usize>,
    subject_rank: Option<i32>,
) -> (RankGroups, RankParents) {
    let mut groups_by_rank = BTreeMap::new();
    let mut parents_by_rank = BTreeMap::new();
    for (&layer_rank, ids) in by_rank {
        let mut layer_ids = ids.clone();
        layer_ids.sort_by_key(|id| order_index[id]);
        let groups = sibling_groups(&layer_ids, edges, ranks, order_index, subject_rank);
        let parents = groups
            .iter()
            .map(|group| layout_parent(group[0], edges, ranks, order_index, subject_rank))
            .collect();
        groups_by_rank.insert(layer_rank, groups);
        parents_by_rank.insert(layer_rank, parents);
    }
    (groups_by_rank, parents_by_rank)
}

pub(crate) fn group_band_rows(
    group_ids: &[NodeId],
    edges: &[EdgeRef],
    ranks: &HashMap<NodeId, i32>,
    subject_rank: Option<i32>,
    order_index: &HashMap<NodeId, usize>,
    cols: usize,
) -> Vec<Vec<NodeId>> {
    if group_ids.is_empty() {
        return Vec::new();
    }
    let safe_cols = cols.max(1);
    let mut ordered = group_ids.to_vec();
    ordered.sort_by_key(|id| order_index[id]);
    let group_rank = ranks[&group_ids[0]];
    let Some(subject_rank) = subject_rank else {
        return balanced_rows(&ordered, safe_cols);
    };
    if subject_rank == group_rank || group_ids.len() <= safe_cols {
        return balanced_rows(&ordered, safe_cols);
    }
    let (outer, leaf) = partition_outer_leaf(group_ids, edges, ranks, subject_rank, order_index);
    let bands = if group_rank < subject_rank {
        [outer, leaf]
    } else {
        [leaf, outer]
    };
    let banded = bands.into_iter().flatten().collect::<Vec<_>>();
    balanced_rows(&banded, safe_cols)
}

fn partition_outer_leaf(
    group_ids: &[NodeId],
    edges: &[EdgeRef],
    ranks: &HashMap<NodeId, i32>,
    subject_rank: i32,
    order_index: &HashMap<NodeId, usize>,
) -> (Vec<NodeId>, Vec<NodeId>) {
    let page: HashSet<NodeId> = ranks.keys().copied().collect();
    let mut outer: Vec<NodeId> = group_ids
        .iter()
        .copied()
        .filter(|id| is_outer(*id, edges, ranks, subject_rank, &page))
        .collect();
    outer.sort_by_key(|id| order_index.get(id).copied().unwrap_or(MISSING_ORDER));
    let outer_set: HashSet<NodeId> = outer.iter().copied().collect();
    let mut leaf: Vec<NodeId> = group_ids
        .iter()
        .copied()
        .filter(|id| !outer_set.contains(id))
        .collect();
    leaf.sort_by_key(|id| order_index.get(id).copied().unwrap_or(MISSING_ORDER));
    (outer, leaf)
}

fn is_outer(
    node_id: NodeId,
    edges: &[EdgeRef],
    ranks: &HashMap<NodeId, i32>,
    subject_rank: i32,
    page: &HashSet<NodeId>,
) -> bool {
    let my_rank = ranks[&node_id];
    if my_rank < subject_rank {
        return edges.iter().any(|edge| {
            edge.to_id == node_id
                && page.contains(&edge.from_id)
                && ranks.get(&edge.from_id).is_some_and(|rank| *rank < my_rank)
        });
    }
    if my_rank > subject_rank {
        return edges.iter().any(|edge| {
            edge.from_id == node_id
                && page.contains(&edge.to_id)
                && ranks.get(&edge.to_id).is_some_and(|rank| *rank > my_rank)
        });
    }
    false
}

fn layout_parent(
    node_id: NodeId,
    edges: &[EdgeRef],
    ranks: &HashMap<NodeId, i32>,
    order_index: &HashMap<NodeId, usize>,
    subject_rank: Option<i32>,
) -> NodeId {
    let my_rank = ranks[&node_id];
    let order_of = |id: &NodeId| order_index.get(id).copied().unwrap_or(MISSING_ORDER);
    if let Some(subject_rank) = subject_rank {
        if my_rank < subject_rank {
            let mut downs: Vec<NodeId> = edges
                .iter()
                .filter(|edge| {
                    edge.from_id == node_id
                        && ranks.get(&edge.to_id).is_some_and(|rank| *rank > my_rank)
                })
                .map(|edge| edge.to_id)
                .collect();
            if !downs.is_empty() {
                downs.sort_by(|left, right| {
                    (ranks[left], order_of(left), *left).cmp(&(
                        ranks[right],
                        order_of(right),
                        *right,
                    ))
                });
                return downs[0];
            }
        } else if my_rank > subject_rank {
            let mut ups: Vec<NodeId> = edges
                .iter()
                .filter(|edge| {
                    edge.to_id == node_id
                        && ranks.get(&edge.from_id).is_some_and(|rank| *rank < my_rank)
                })
                .map(|edge| edge.from_id)
                .collect();
            if !ups.is_empty() {
                ups.sort_by(|left, right| {
                    (std::cmp::Reverse(ranks[left]), order_of(left), *left).cmp(&(
                        std::cmp::Reverse(ranks[right]),
                        order_of(right),
                        *right,
                    ))
                });
                return ups[0];
            }
        }
        return node_id;
    }
    let mut ups: Vec<NodeId> = edges
        .iter()
        .filter(|edge| {
            edge.to_id == node_id && ranks.get(&edge.from_id).is_some_and(|rank| *rank < my_rank)
        })
        .map(|edge| edge.from_id)
        .collect();
    if ups.is_empty() {
        return node_id;
    }
    ups.sort_by(|left, right| {
        (std::cmp::Reverse(ranks[left]), order_of(left), *left).cmp(&(
            std::cmp::Reverse(ranks[right]),
            order_of(right),
            *right,
        ))
    });
    ups[0]
}

fn balanced_rows(items: &[NodeId], cols: usize) -> Vec<Vec<NodeId>> {
    if items.is_empty() {
        return Vec::new();
    }
    let safe_cols = cols.max(1);
    let row_count = items.len().div_ceil(safe_cols);
    let base_size = items.len() / row_count;
    let larger_rows = items.len() % row_count;
    let mut rows = Vec::new();
    let mut offset = 0;
    for row_index in 0..row_count {
        let row_size = base_size + usize::from(row_index < larger_rows);
        rows.push(items[offset..offset + row_size].to_vec());
        offset += row_size;
    }
    rows
}
