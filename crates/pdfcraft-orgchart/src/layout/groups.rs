//! Sibling groups and row bands.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::graph::NodeId;
use crate::page::EdgeRef;
use crate::units::{NODE_SPACING_MM, ROW_SPAN_EPS_MM};

use super::BoxById;

type RankGroups = BTreeMap<i32, Vec<Vec<NodeId>>>;
type RankParents = BTreeMap<i32, Vec<NodeId>>;
type OrderKey = (bool, Option<usize>);
type SiblingBucketKey = (OrderKey, NodeId);

pub(crate) fn use_family_columns(
    subject_rank: Option<i32>,
    layer_rank: i32,
    group_count: usize,
) -> bool {
    subject_rank.is_some_and(|rank| layer_rank != rank && group_count >= 2)
}

fn order_key(order_index: &HashMap<NodeId, usize>, id: NodeId) -> OrderKey {
    let index = order_index.get(&id).copied();
    (index.is_none(), index)
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
    let mut buckets: BTreeMap<SiblingBucketKey, Vec<NodeId>> = BTreeMap::new();
    for node_id in layer_ids {
        let parent = layout_parent(*node_id, edges, ranks, order_index, subject_rank);
        let key = (order_key(order_index, parent), parent);
        buckets.entry(key).or_default().push(*node_id);
    }
    buckets
        .into_values()
        .map(|mut group| {
            group.sort_by_key(|id| order_key(order_index, *id));
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
        layer_ids.sort_by_key(|id| *super::map_get(order_index, id, "layer node is in node_order"));
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
    box_by_id: &BoxById<'_>,
    max_row_width_mm: f64,
) -> Vec<Vec<NodeId>> {
    if group_ids.is_empty() {
        return Vec::new();
    }
    let mut ordered = group_ids.to_vec();
    ordered.sort_by_key(|id| *super::map_get(order_index, id, "group node is in node_order"));
    let group_rank = *super::map_get(ranks, &group_ids[0], "group node has a hop rank");
    let single_fits = row_span_mm(&ordered, box_by_id) <= max_row_width_mm + ROW_SPAN_EPS_MM;
    if subject_rank == Some(group_rank) || single_fits {
        return vec![ordered];
    }
    let Some(subject_rank) = subject_rank else {
        return balanced_rows_by_width(&ordered, max_row_width_mm, box_by_id);
    };
    let (outer, leaf) = partition_outer_leaf(group_ids, edges, ranks, subject_rank, order_index);
    let (first, second) = if group_rank < subject_rank {
        (outer, leaf)
    } else {
        (leaf, outer)
    };
    if first.is_empty() {
        return balanced_rows_by_width(&second, max_row_width_mm, box_by_id);
    }
    if second.is_empty() {
        return balanced_rows_by_width(&first, max_row_width_mm, box_by_id);
    }
    let joined: Vec<NodeId> = first
        .iter()
        .copied()
        .chain(second.iter().copied())
        .collect();
    let split_sum = min_rows(&first, max_row_width_mm, box_by_id)
        + min_rows(&second, max_row_width_mm, box_by_id);
    if split_sum == min_rows(&joined, max_row_width_mm, box_by_id) {
        let mut rows = balanced_rows_by_width(&first, max_row_width_mm, box_by_id);
        rows.extend(balanced_rows_by_width(&second, max_row_width_mm, box_by_id));
        rows
    } else {
        balanced_rows_by_width(&joined, max_row_width_mm, box_by_id)
    }
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
    outer.sort_by_key(|id| order_key(order_index, *id));
    let outer_set: HashSet<NodeId> = outer.iter().copied().collect();
    let mut leaf: Vec<NodeId> = group_ids
        .iter()
        .copied()
        .filter(|id| !outer_set.contains(id))
        .collect();
    leaf.sort_by_key(|id| order_key(order_index, *id));
    (outer, leaf)
}

fn is_outer(
    node_id: NodeId,
    edges: &[EdgeRef],
    ranks: &HashMap<NodeId, i32>,
    subject_rank: i32,
    page: &HashSet<NodeId>,
) -> bool {
    let my_rank = *super::map_get(ranks, &node_id, "page node has a hop rank");
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
    let my_rank = *super::map_get(ranks, &node_id, "page node has a hop rank");
    let order_of = |id: &NodeId| order_key(order_index, *id);
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
                    (
                        *super::map_get(ranks, left, "down-neighbor has a hop rank"),
                        order_of(left),
                        *left,
                    )
                        .cmp(&(
                            *super::map_get(ranks, right, "down-neighbor has a hop rank"),
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
                    (
                        std::cmp::Reverse(*super::map_get(
                            ranks,
                            left,
                            "up-neighbor has a hop rank",
                        )),
                        order_of(left),
                        *left,
                    )
                        .cmp(&(
                            std::cmp::Reverse(*super::map_get(
                                ranks,
                                right,
                                "up-neighbor has a hop rank",
                            )),
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
        (
            std::cmp::Reverse(*super::map_get(ranks, left, "up-neighbor has a hop rank")),
            order_of(left),
            *left,
        )
            .cmp(&(
                std::cmp::Reverse(*super::map_get(ranks, right, "up-neighbor has a hop rank")),
                order_of(right),
                *right,
            ))
    });
    ups[0]
}

fn box_width_mm(node_id: NodeId, box_by_id: &BoxById<'_>) -> f64 {
    super::map_get(box_by_id, &node_id, "group node has a measured box").width_mm
}

pub(crate) fn row_span_mm(ids: &[NodeId], box_by_id: &BoxById<'_>) -> f64 {
    if ids.is_empty() {
        return 0.0;
    }
    let mut span = 0.0;
    for (index, id) in ids.iter().enumerate() {
        span += box_width_mm(*id, box_by_id);
        if index + 1 < ids.len() {
            span += NODE_SPACING_MM;
        }
    }
    span
}

fn row_fits(ids: &[NodeId], max_row_width_mm: f64, box_by_id: &BoxById<'_>) -> bool {
    ids.len() <= 1 || row_span_mm(ids, box_by_id) <= max_row_width_mm + ROW_SPAN_EPS_MM
}

fn min_rows(ids: &[NodeId], max_row_width_mm: f64, box_by_id: &BoxById<'_>) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let mut start = 0;
    let mut rows = 0;
    while start < ids.len() {
        let mut end = start + 1;
        while end <= ids.len() && row_fits(&ids[start..end], max_row_width_mm, box_by_id) {
            end += 1;
        }
        start = (end - 1).max(start + 1);
        rows += 1;
    }
    rows
}

fn balanced_rows_by_width(
    items: &[NodeId],
    max_row_width_mm: f64,
    box_by_id: &BoxById<'_>,
) -> Vec<Vec<NodeId>> {
    split_into_rows(
        items,
        min_rows(items, max_row_width_mm, box_by_id).max(1),
        max_row_width_mm,
        box_by_id,
    )
}

/// Split `items` into exactly `row_count` width-balanced rows.
pub(crate) fn split_into_rows(
    items: &[NodeId],
    row_count: usize,
    max_row_width_mm: f64,
    box_by_id: &BoxById<'_>,
) -> Vec<Vec<NodeId>> {
    if items.is_empty() {
        return Vec::new();
    }
    let row_count = row_count.clamp(1, items.len());
    if row_count == 1 {
        return vec![items.to_vec()];
    }
    if row_count == items.len() {
        return items.iter().copied().map(|id| vec![id]).collect();
    }
    let count = items.len();
    let mut prefix = vec![0.0; count + 1];
    for (index, id) in items.iter().enumerate() {
        prefix[index + 1] = prefix[index] + box_width_mm(*id, box_by_id);
    }
    let span = |start: usize, end: usize| {
        let widths = prefix[end] - prefix[start];
        widths + NODE_SPACING_MM * crate::units::n64(end.saturating_sub(start + 1))
    };
    let fits = |start: usize, end: usize| {
        end > start && (end == start + 1 || span(start, end) <= max_row_width_mm + ROW_SPAN_EPS_MM)
    };
    let total_span =
        prefix[count] + NODE_SPACING_MM * crate::units::n64(count.saturating_sub(row_count));
    let mean = total_span / crate::units::n64(row_count);
    let mut best: Vec<Vec<Option<RowSplit>>> = vec![vec![None; count + 1]; row_count + 1];
    best[0][0] = Some(RowSplit {
        cost: 0.0,
        counts: Vec::new(),
        widths: Vec::new(),
        start: 0,
    });
    for rows in 1..=row_count {
        for end in 1..=count {
            for start in 0..end {
                if !fits(start, end) {
                    continue;
                }
                let Some(prev) = best[rows - 1][start].as_ref() else {
                    continue;
                };
                let width = span(start, end);
                let mut counts = prev.counts.clone();
                counts.push(end - start);
                let mut widths = prev.widths.clone();
                widths.push(width);
                let candidate = RowSplit {
                    cost: prev.cost + (width - mean).powi(2),
                    counts,
                    widths,
                    start,
                };
                let take = match &best[rows][end] {
                    None => true,
                    Some(current) => candidate.better_than(current),
                };
                if take {
                    best[rows][end] = Some(candidate);
                }
            }
        }
    }
    reconstruct_width_rows(items, &best, count, row_count)
}

#[derive(Clone)]
struct RowSplit {
    cost: f64,
    counts: Vec<usize>,
    widths: Vec<f64>,
    start: usize,
}

impl RowSplit {
    fn better_than(&self, other: &Self) -> bool {
        if (self.cost - other.cost).abs() > ROW_SPAN_EPS_MM {
            return self.cost < other.cost;
        }
        if self.counts != other.counts {
            return self.counts > other.counts;
        }
        for (left, right) in self.widths.iter().zip(&other.widths) {
            if (left - right).abs() > ROW_SPAN_EPS_MM {
                return left > right;
            }
        }
        false
    }
}

fn reconstruct_width_rows(
    items: &[NodeId],
    best: &[Vec<Option<RowSplit>>],
    count: usize,
    row_count: usize,
) -> Vec<Vec<NodeId>> {
    let mut starts = vec![0; row_count];
    let mut end = count;
    for rows in (1..=row_count).rev() {
        let start = best[rows][end].as_ref().map_or(0, |split| split.start);
        starts[rows - 1] = start;
        end = start;
    }
    let mut rows = Vec::with_capacity(row_count);
    for index in 0..row_count {
        let start = starts[index];
        let row_end = if index + 1 == row_count {
            count
        } else {
            starts[index + 1]
        };
        let start = start.min(items.len());
        let row_end = row_end.clamp(start, items.len());
        rows.push(items[start..row_end].to_vec());
    }
    rows
}
