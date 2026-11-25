//! Orthogonal edge routing.

use crate::graph::NodeId;
use std::collections::{HashMap, HashSet};

use crate::error::{Error, ErrorCode};
use crate::page::{EdgeRef, LaidOutEdge, PlacedNode, PointMm};
use crate::units::{
    FiniteMm, BUS_ENDPOINT_CLEARANCE_MM, BUS_STAGGER_MM, BUS_Y_EPS_MM, NODE_SPACING_MM, ROW_GAP_MM,
    SHAREHOLDER_STUB_MIN_MM,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[expect(
    clippy::enum_variant_names,
    reason = "MultiIn/MultiOut/MultiOutEntry are the three bus roles in adaptive-layout spec §5.2"
)]
enum BusRole {
    MultiIn,
    MultiOut,
    MultiOutEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BusKey {
    role: BusRole,
    endpoint_id: NodeId,
    row: FiniteMm,
}

#[derive(Debug, Clone)]
struct BusSpan {
    key: BusKey,
    y_mm: f64,
    x0_mm: f64,
    x1_mm: f64,
}

impl PartialEq for BusSpan {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && FiniteMm::new(self.y_mm) == FiniteMm::new(other.y_mm)
            && FiniteMm::new(self.x0_mm) == FiniteMm::new(other.x0_mm)
            && FiniteMm::new(self.x1_mm) == FiniteMm::new(other.x1_mm)
    }
}

impl Eq for BusSpan {}

impl BusSpan {
    fn overlaps_x(&self, x0_mm: f64, x1_mm: f64) -> bool {
        let left = x0_mm.min(x1_mm);
        let right = x0_mm.max(x1_mm);
        self.x0_mm <= right && left <= self.x1_mm
    }

    fn near_y(&self, y_mm: f64) -> bool {
        (self.y_mm - y_mm).abs() < BUS_Y_EPS_MM
    }
}

#[derive(Default)]
struct OccupiedBuses {
    spans: Vec<BusSpan>,
}

impl OccupiedBuses {
    fn span_for(&self, key: &BusKey) -> Option<&BusSpan> {
        self.spans.iter().find(|span| &span.key == key)
    }

    fn conflicts(&self, role: BusRole, y_mm: f64, x0_mm: f64, x1_mm: f64) -> bool {
        self.spans
            .iter()
            .any(|span| span.key.role == role && span.near_y(y_mm) && span.overlaps_x(x0_mm, x1_mm))
    }

    fn with_span(&mut self, span: BusSpan) -> Result<(), Error> {
        for existing in &self.spans {
            if existing.key != span.key {
                continue;
            }
            if existing == &span {
                return Ok(());
            }
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                "bus span already published",
            ));
        }
        self.spans.push(span);
        Ok(())
    }
}

struct RouteCaches {
    drawn_trunk_to: HashSet<NodeId>,
    drawn_trunk_from: HashSet<NodeId>,
    occupied: OccupiedBuses,
    multi_in_spine_x: HashMap<NodeId, f64>,
    multi_in_row_bottom: HashMap<NodeId, HashMap<FiniteMm, f64>>,
    multi_in_bottom_row_y: HashMap<NodeId, f64>,
    multi_out_branch: HashMap<(NodeId, FiniteMm), (f64, f64)>,
    used_channel_x: Vec<f64>,
}

pub(crate) fn route_edges(
    page_edges: &[EdgeRef],
    nodes: &HashMap<NodeId, PlacedNode>,
) -> Result<Vec<LaidOutEdge>, Error> {
    if page_edges.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_target: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut by_source: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for edge in page_edges {
        by_target.entry(edge.to_id).or_default().push(edge.from_id);
        by_source.entry(edge.from_id).or_default().push(edge.to_id);
    }
    let mut result = Vec::new();
    let mut caches = RouteCaches {
        drawn_trunk_to: HashSet::new(),
        drawn_trunk_from: HashSet::new(),
        occupied: OccupiedBuses::default(),
        multi_in_spine_x: HashMap::new(),
        multi_in_row_bottom: HashMap::new(),
        multi_in_bottom_row_y: HashMap::new(),
        multi_out_branch: HashMap::new(),
        used_channel_x: Vec::new(),
    };
    for edge in page_edges {
        let Some(src) = nodes.get(&edge.from_id) else {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("edge source {} is not placed", edge.from_id),
            ));
        };
        let Some(dst) = nodes.get(&edge.to_id) else {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("edge target {} is not placed", edge.to_id),
            ));
        };
        let src_bottom = PointMm {
            x: src.x + src.width / 2.0,
            y: src.y + src.height,
        };
        let dst_top = PointMm {
            x: dst.x + dst.width / 2.0,
            y: dst.y,
        };
        let siblings_in: Vec<NodeId> = by_target.get(&edge.to_id).cloned().unwrap_or_default();
        let siblings_out: Vec<NodeId> = by_source.get(&edge.from_id).cloned().unwrap_or_default();
        let multi_in = siblings_in.len() > 1;
        let multi_out = siblings_out.len() > 1 && !multi_in;
        if multi_in {
            result.push(route_multi_in(
                edge,
                src,
                src_bottom,
                dst_top,
                &siblings_in,
                nodes,
                &mut caches,
            )?);
        } else if multi_out {
            result.push(route_multi_out(
                edge,
                src,
                src_bottom,
                dst_top,
                &siblings_out,
                nodes,
                &mut caches,
            )?);
        } else {
            result.push(route_single(edge, src, src_bottom, dst_top, nodes));
        }
    }
    Ok(result)
}

fn stagger_bus_y(
    candidate_y: f64,
    x0_mm: f64,
    x1_mm: f64,
    role: BusRole,
    occupied: &OccupiedBuses,
    y_limit: f64,
) -> f64 {
    let mut y = candidate_y.min(y_limit);
    while occupied.conflicts(role, y, x0_mm, x1_mm) {
        let next = y + BUS_STAGGER_MM;
        if next > y_limit {
            return y;
        }
        y = next;
    }
    y
}

fn commit_bus_y(
    occupied: &mut OccupiedBuses,
    key: BusKey,
    candidate_y: f64,
    centers: &[f64],
    y_limit: f64,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
) -> Result<BusSpan, Error> {
    if centers.is_empty() {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            "bus span has no centers",
        ));
    }
    if let Some(existing) = occupied.span_for(&key) {
        return Ok(existing.clone());
    }
    let x0 = centers.iter().copied().fold(f64::INFINITY, f64::min);
    let x1 = centers.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if x0 > x1 {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            "bus span x0 must be <= x1",
        ));
    }
    let mut y = stagger_bus_y(candidate_y, x0, x1, key.role, occupied, y_limit);
    y = drop_bus_below_boxes(y, x0, x1, y_limit, nodes, exclude_ids, ROW_GAP_MM * 0.5);
    let published = BusSpan {
        key,
        y_mm: y,
        x0_mm: x0,
        x1_mm: x1,
    };
    occupied.with_span(published.clone())?;
    Ok(published)
}

fn multi_in_candidate_y(max_src_bottom: f64, gap: f64, y_limit: f64) -> f64 {
    let stub = SHAREHOLDER_STUB_MIN_MM.max((ROW_GAP_MM * 0.7).min(1.5_f64.max(gap * 0.4)));
    let raw = max_src_bottom + stub;
    let candidate = raw.min(max_src_bottom + gap * 0.85).min(y_limit);
    let full_stub_floor =
        max_src_bottom + SHAREHOLDER_STUB_MIN_MM.min(BUS_ENDPOINT_CLEARANCE_MM.max(gap - 1.0));
    candidate.max(full_stub_floor)
}

#[expect(
    clippy::too_many_lines,
    reason = "multi-in bus routing stays in one function"
)]
fn route_multi_in(
    edge: &EdgeRef,
    src: &PlacedNode,
    src_bottom: PointMm,
    dst_top: PointMm,
    siblings_in: &[NodeId],
    nodes: &HashMap<NodeId, PlacedNode>,
    caches: &mut RouteCaches,
) -> Result<LaidOutEdge, Error> {
    let sources: Vec<&PlacedNode> = siblings_in
        .iter()
        .map(|id| super::map_get(nodes, id, "multi-in sibling is placed"))
        .collect();
    let max_src_bottom = sources
        .iter()
        .map(|node| node.y + node.height)
        .fold(f64::NEG_INFINITY, f64::max);
    let gap = (dst_top.y - max_src_bottom).max(0.0);
    let y_limit = dst_top.y - BUS_ENDPOINT_CLEARANCE_MM;
    let candidate = multi_in_candidate_y(max_src_bottom, gap, y_limit);
    let mut centers: Vec<f64> = sources
        .iter()
        .map(|node| node.x + node.width / 2.0)
        .collect();
    centers.push(dst_top.x);
    let mut exclude: HashSet<NodeId> = siblings_in.iter().copied().collect();
    exclude.insert(edge.to_id);
    let node_list: Vec<PlacedNode> = nodes.values().cloned().collect();
    let span = commit_bus_y(
        &mut caches.occupied,
        BusKey {
            role: BusRole::MultiIn,
            endpoint_id: edge.to_id,
            row: FiniteMm::ZERO,
        },
        candidate,
        &centers,
        y_limit,
        &node_list,
        &exclude,
    )?;
    let bus_y = span.y_mm;
    let trunk_x = dst_top.x;
    caches
        .multi_in_row_bottom
        .entry(edge.to_id)
        .or_insert_with(|| {
            let mut grouped: HashMap<FiniteMm, f64> = HashMap::new();
            for source in &sources {
                let key = FiniteMm::or_zero(source.y);
                let bottom = source.y + source.height;
                grouped
                    .entry(key)
                    .and_modify(|value| *value = value.max(bottom))
                    .or_insert(bottom);
            }
            grouped
        });
    caches
        .multi_in_bottom_row_y
        .entry(edge.to_id)
        .or_insert_with(|| {
            sources
                .iter()
                .map(|node| node.y)
                .fold(f64::NEG_INFINITY, f64::max)
        });
    let row_bottoms = super::map_get(
        &caches.multi_in_row_bottom,
        &edge.to_id,
        "multi-in row bottoms were cached for this target",
    )
    .clone();
    let bottom_row_y = *super::map_get(
        &caches.multi_in_bottom_row_y,
        &edge.to_id,
        "multi-in bottom row was cached for this target",
    );
    let multi_row = row_bottoms.len() > 1;
    let first_center_x = sources[0].x + sources[0].width / 2.0;
    let column_stack = sources.len() >= 2
        && row_bottoms.len() == sources.len()
        && sources
            .iter()
            .all(|node| (node.x + node.width / 2.0 - first_center_x).abs() < 0.01);
    let mut spine_x = None;
    if multi_row || column_stack {
        caches
            .multi_in_spine_x
            .entry(edge.to_id)
            .or_insert_with(|| {
                let top_y = if column_stack {
                    sources
                        .iter()
                        .map(|node| node.y + node.height / 2.0)
                        .fold(f64::INFINITY, f64::min)
                } else {
                    sources
                        .iter()
                        .map(|node| node.y + node.height)
                        .fold(f64::INFINITY, f64::min)
                };
                group_spine_x(&sources, top_y, bus_y, trunk_x, &node_list, &exclude)
            });
        spine_x = Some(*super::map_get(
            &caches.multi_in_spine_x,
            &edge.to_id,
            "multi-in spine x was cached for this target",
        ));
    }
    let path_to_bus = if column_stack {
        let shared_spine = *super::map_get(
            &caches.multi_in_spine_x,
            &edge.to_id,
            "column-stack spine x was cached for this target",
        );
        route_side_to_spine(src, shared_spine, bus_y, trunk_x)
    } else {
        let use_spine = multi_row && src.y < bottom_row_y - 0.01;
        let row_bottom = *super::map_get(
            &row_bottoms,
            &FiniteMm::or_zero(src.y),
            "source row has a cached bottom",
        );
        route_down_to_bus(&DownBus {
            src,
            src_bottom,
            bus_y,
            trunk_x,
            nodes: &node_list,
            exclude_ids: &HashSet::from([edge.from_id, edge.to_id]),
            spine_x: if use_spine { spine_x } else { None },
            row_max_bottom: row_bottom,
        })
    };
    let draw_arrow = caches.drawn_trunk_to.insert(edge.to_id);
    let mut points = path_to_bus;
    if draw_arrow {
        points.push(dst_top);
    }
    Ok(LaidOutEdge {
        from_id: edge.from_id,
        to_id: edge.to_id,
        points,
        draw_arrow,
    })
}

fn route_multi_out(
    edge: &EdgeRef,
    src: &PlacedNode,
    src_bottom: PointMm,
    dst_top: PointMm,
    siblings_out: &[NodeId],
    nodes: &HashMap<NodeId, PlacedNode>,
    caches: &mut RouteCaches,
) -> Result<LaidOutEdge, Error> {
    let children: Vec<&PlacedNode> = siblings_out
        .iter()
        .map(|id| super::map_get(nodes, id, "multi-out sibling is placed"))
        .collect();
    let min_dst_top = children
        .iter()
        .map(|node| node.y)
        .fold(f64::INFINITY, f64::min);
    let gap = (min_dst_top - src_bottom.y).max(0.0);
    let mut exclude: HashSet<NodeId> = siblings_out.iter().copied().collect();
    exclude.insert(edge.from_id);
    let y_limit = min_dst_top - BUS_ENDPOINT_CLEARANCE_MM;
    let raw = src_bottom.y + (ROW_GAP_MM * 0.5).min(1.5_f64.max(gap * 0.35));
    let candidate = raw
        .min(src_bottom.y + gap * 0.85)
        .min(y_limit)
        .max(src_bottom.y + BUS_ENDPOINT_CLEARANCE_MM);
    let mut centers = vec![src_bottom.x];
    centers.extend(children.iter().map(|node| node.x + node.width / 2.0));
    let node_list: Vec<PlacedNode> = nodes.values().cloned().collect();
    let span = commit_bus_y(
        &mut caches.occupied,
        BusKey {
            role: BusRole::MultiOut,
            endpoint_id: edge.from_id,
            row: FiniteMm::ZERO,
        },
        candidate,
        &centers,
        y_limit,
        &node_list,
        &exclude,
    )?;
    let bus_y = span.y_mm;
    let dst = super::map_get(nodes, &edge.to_id, "multi-out dest is placed");
    let child_exclude = HashSet::from([edge.from_id, edge.to_id]);
    let points = if vertical_clear(dst_top.x, bus_y, dst_top.y, &node_list, &child_exclude) {
        if caches.drawn_trunk_from.insert(edge.from_id) {
            outward_points(src, src_bottom, bus_y, dst_top, &node_list, &exclude)
        } else {
            vec![
                PointMm {
                    x: src_bottom.x,
                    y: bus_y,
                },
                PointMm {
                    x: dst_top.x,
                    y: bus_y,
                },
                dst_top,
            ]
        }
    } else {
        multi_out_branch_points(
            edge,
            &OutBranch {
                src,
                src_bottom,
                dst,
                dst_top,
                bus_y,
                children: &children,
            },
            &node_list,
            &exclude,
            caches,
        )?
    };
    Ok(LaidOutEdge {
        from_id: edge.from_id,
        to_id: edge.to_id,
        points,
        draw_arrow: true,
    })
}

#[derive(Clone, Copy)]
struct OutBranch<'a> {
    src: &'a PlacedNode,
    src_bottom: PointMm,
    dst: &'a PlacedNode,
    dst_top: PointMm,
    bus_y: f64,
    children: &'a [&'a PlacedNode],
}

fn multi_out_branch_points(
    edge: &EdgeRef,
    branch: &OutBranch<'_>,
    node_list: &[PlacedNode],
    exclude: &HashSet<NodeId>,
    caches: &mut RouteCaches,
) -> Result<Vec<PointMm>, Error> {
    let row_key = FiniteMm::or_zero(branch.dst.y);
    let branch_key = (edge.from_id, row_key);
    cache_multi_out_branch(edge, branch, node_list, exclude, caches)?;
    let (channel_x, entry_y) = *super::map_get(
        &caches.multi_out_branch,
        &branch_key,
        "multi-out branch was cached for this parent row",
    );
    let OutBranch {
        src,
        src_bottom,
        dst_top,
        bus_y,
        ..
    } = *branch;
    let mut points = if caches.drawn_trunk_from.insert(edge.from_id) {
        src_to_bus(src, src_bottom, bus_y, node_list, exclude)
    } else {
        vec![PointMm {
            x: src_bottom.x,
            y: bus_y,
        }]
    };
    points.extend([
        PointMm {
            x: channel_x,
            y: bus_y,
        },
        PointMm {
            x: channel_x,
            y: entry_y,
        },
        PointMm {
            x: dst_top.x,
            y: entry_y,
        },
        dst_top,
    ]);
    Ok(points)
}

fn cache_multi_out_branch(
    edge: &EdgeRef,
    branch: &OutBranch<'_>,
    node_list: &[PlacedNode],
    exclude: &HashSet<NodeId>,
    caches: &mut RouteCaches,
) -> Result<(), Error> {
    let row_key = FiniteMm::or_zero(branch.dst.y);
    let branch_key = (edge.from_id, row_key);
    if caches.multi_out_branch.contains_key(&branch_key) {
        return Ok(());
    }
    let row_mates: Vec<&PlacedNode> = branch
        .children
        .iter()
        .copied()
        .filter(|child| FiniteMm::or_zero(child.y) == row_key)
        .collect();
    let prev_row_bottom = branch
        .children
        .iter()
        .filter(|child| child.y < branch.dst.y - 0.01)
        .map(|child| child.y + child.height)
        .fold(f64::NEG_INFINITY, f64::max);
    let prev_row_bottom = if prev_row_bottom.is_finite() {
        prev_row_bottom
    } else {
        branch.src.y + branch.src.height
    };
    let gap = (branch.dst.y - prev_row_bottom).max(0.0);
    let raw = prev_row_bottom + (gap * 0.35).clamp(1.5, ROW_GAP_MM * 0.5);
    let y_limit = branch.dst.y - BUS_ENDPOINT_CLEARANCE_MM;
    let candidate = raw.clamp(prev_row_bottom + BUS_ENDPOINT_CLEARANCE_MM, y_limit);
    let mut centers: Vec<f64> = row_mates
        .iter()
        .map(|child| child.x + child.width / 2.0)
        .collect();
    if centers.is_empty() {
        centers.push(branch.dst_top.x);
    }
    let row_center = centers.iter().copied().sum::<f64>() / crate::units::n64(centers.len());
    let span = commit_bus_y(
        &mut caches.occupied,
        BusKey {
            role: BusRole::MultiOutEntry,
            endpoint_id: edge.from_id,
            row: row_key,
        },
        candidate,
        &centers,
        y_limit,
        node_list,
        exclude,
    )?;
    let entry_y = span.y_mm;
    let min_x = row_mates
        .iter()
        .map(|child| child.x)
        .fold(branch.dst.x, f64::min);
    let max_x = row_mates
        .iter()
        .map(|child| child.x + child.width)
        .fold(branch.dst.x + branch.dst.width, f64::max);
    let mut channel_x = nearest_clear_x(&ChannelSearch {
        y_lo: branch.bus_y.min(entry_y),
        y_hi: branch.bus_y.max(entry_y),
        trunk_x: row_center,
        first_left: min_x - NODE_SPACING_MM * 0.5,
        first_right: max_x + NODE_SPACING_MM * 0.5,
        nodes: node_list,
        exclude_ids: exclude,
    });
    if caches
        .used_channel_x
        .iter()
        .any(|used| (used - channel_x).abs() < 0.01)
    {
        channel_x += BUS_STAGGER_MM;
    }
    caches.used_channel_x.push(channel_x);
    caches
        .multi_out_branch
        .insert(branch_key, (channel_x, entry_y));
    Ok(())
}

fn route_single(
    edge: &EdgeRef,
    src: &PlacedNode,
    src_bottom: PointMm,
    dst_top: PointMm,
    nodes: &HashMap<NodeId, PlacedNode>,
) -> LaidOutEdge {
    let gap = dst_top.y - src_bottom.y;
    let y_limit = dst_top.y - 0.5;
    let exclude = HashSet::from([edge.from_id, edge.to_id]);
    let node_list: Vec<PlacedNode> = nodes.values().cloned().collect();
    let points = if (src_bottom.x - dst_top.x).abs() < 0.01 {
        if vertical_clear(src_bottom.x, src_bottom.y, dst_top.y, &node_list, &exclude) {
            vec![src_bottom, dst_top]
        } else {
            outward_points(src, src_bottom, y_limit, dst_top, &node_list, &exclude)
        }
    } else {
        let mut mid_y = if gap > 2.0 {
            src_bottom.y + gap * 0.65
        } else {
            f64::midpoint(src_bottom.y, dst_top.y)
        };
        mid_y = mid_y.clamp(src_bottom.y + 0.5, y_limit);
        mid_y = drop_bus_below_boxes(
            mid_y,
            src_bottom.x,
            dst_top.x,
            y_limit,
            &node_list,
            &exclude,
            ROW_GAP_MM * 0.5,
        );
        outward_points(src, src_bottom, mid_y, dst_top, &node_list, &exclude)
    };
    LaidOutEdge {
        from_id: edge.from_id,
        to_id: edge.to_id,
        points,
        draw_arrow: true,
    }
}

fn row_max_bottom(src: &PlacedNode, nodes: &[PlacedNode]) -> f64 {
    nodes
        .iter()
        .filter(|node| (node.y - src.y).abs() < 0.01)
        .map(|node| node.y + node.height)
        .fold(src.y + src.height, f64::max)
}

fn src_to_bus(
    src: &PlacedNode,
    src_bottom: PointMm,
    bus_y: f64,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
) -> Vec<PointMm> {
    let trunk_x = src_bottom.x;
    if vertical_clear(trunk_x, src_bottom.y, bus_y, nodes, exclude_ids) {
        vec![
            src_bottom,
            PointMm {
                x: trunk_x,
                y: bus_y,
            },
        ]
    } else {
        let bottom = row_max_bottom(src, nodes);
        let exit_y = (bottom + ROW_GAP_MM * 0.5).min(bus_y - 0.5);
        let side_x = side_channel_x(src, exit_y, bus_y, trunk_x, nodes, exclude_ids);
        vec![
            src_bottom,
            PointMm {
                x: trunk_x,
                y: exit_y,
            },
            PointMm {
                x: side_x,
                y: exit_y,
            },
            PointMm {
                x: side_x,
                y: bus_y,
            },
            PointMm {
                x: trunk_x,
                y: bus_y,
            },
        ]
    }
}

fn outward_points(
    src: &PlacedNode,
    src_bottom: PointMm,
    bus_y: f64,
    dst_top: PointMm,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
) -> Vec<PointMm> {
    let mut down = src_to_bus(src, src_bottom, bus_y, nodes, exclude_ids);
    down.push(PointMm {
        x: dst_top.x,
        y: bus_y,
    });
    down.push(dst_top);
    down
}

fn drop_bus_below_boxes(
    bus_y: f64,
    x1: f64,
    x2: f64,
    y_limit: f64,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
    clearance: f64,
) -> f64 {
    let pending: Vec<&PlacedNode> = nodes
        .iter()
        .filter(|node| !exclude_ids.contains(&node.id))
        .collect();
    let mut y = bus_y;
    for _ in 0..=pending.len() {
        let blocked: Vec<&&PlacedNode> = pending
            .iter()
            .filter(|node| horizontal_hits_box(y, x1, x2, node))
            .collect();
        if blocked.is_empty() {
            break;
        }
        let cleared = blocked
            .iter()
            .map(|node| node.y + node.height)
            .fold(f64::NEG_INFINITY, f64::max)
            + clearance;
        if cleared <= y {
            break;
        }
        y = cleared.min(y_limit);
        if y >= y_limit {
            break;
        }
    }
    y
}

fn horizontal_hits_box(y: f64, x1: f64, x2: f64, node: &PlacedNode) -> bool {
    let left = node.x + 0.5;
    let right = node.x + node.width - 0.5;
    let top = node.y + 0.5;
    let bottom = node.y + node.height - 0.5;
    if y <= top || y >= bottom {
        return false;
    }
    x1.max(x2) > left && x1.min(x2) < right
}

#[derive(Clone, Copy)]
struct DownBus<'a> {
    src: &'a PlacedNode,
    src_bottom: PointMm,
    bus_y: f64,
    trunk_x: f64,
    nodes: &'a [PlacedNode],
    exclude_ids: &'a HashSet<NodeId>,
    spine_x: Option<f64>,
    row_max_bottom: f64,
}

fn route_down_to_bus(route: &DownBus<'_>) -> Vec<PointMm> {
    let DownBus {
        src,
        src_bottom,
        bus_y,
        trunk_x,
        nodes,
        exclude_ids,
        spine_x,
        row_max_bottom,
    } = *route;
    let src_x = src_bottom.x;
    if let Some(spine_x) = spine_x {
        let exit_y = shareholder_exit_y(row_max_bottom, bus_y);
        return vec![
            src_bottom,
            PointMm {
                x: src_x,
                y: exit_y,
            },
            PointMm {
                x: spine_x,
                y: exit_y,
            },
            PointMm {
                x: spine_x,
                y: bus_y,
            },
            PointMm {
                x: trunk_x,
                y: bus_y,
            },
        ];
    }
    if vertical_clear(src_x, src_bottom.y, bus_y, nodes, exclude_ids) {
        return vec![
            src_bottom,
            PointMm { x: src_x, y: bus_y },
            PointMm {
                x: trunk_x,
                y: bus_y,
            },
        ];
    }
    let exit_y = shareholder_exit_y(row_max_bottom, bus_y);
    let side_x = side_channel_x(src, exit_y, bus_y, trunk_x, nodes, exclude_ids);
    vec![
        src_bottom,
        PointMm {
            x: src_x,
            y: exit_y,
        },
        PointMm {
            x: side_x,
            y: exit_y,
        },
        PointMm {
            x: side_x,
            y: bus_y,
        },
        PointMm {
            x: trunk_x,
            y: bus_y,
        },
    ]
}

fn shareholder_exit_y(row_max_bottom: f64, bus_y: f64) -> f64 {
    (row_max_bottom + SHAREHOLDER_STUB_MIN_MM).min(bus_y - 0.5)
}

fn route_side_to_spine(src: &PlacedNode, spine_x: f64, bus_y: f64, trunk_x: f64) -> Vec<PointMm> {
    let cy = src.y + src.height / 2.0;
    let side_x = if spine_x >= src.x + src.width {
        src.x + src.width
    } else {
        src.x
    };
    vec![
        PointMm { x: side_x, y: cy },
        PointMm { x: spine_x, y: cy },
        PointMm {
            x: spine_x,
            y: bus_y,
        },
        PointMm {
            x: trunk_x,
            y: bus_y,
        },
    ]
}

fn group_spine_x(
    sources: &[&PlacedNode],
    top_y: f64,
    bus_y: f64,
    trunk_x: f64,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
) -> f64 {
    let min_x = sources
        .iter()
        .map(|node| node.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = sources
        .iter()
        .map(|node| node.x + node.width)
        .fold(f64::NEG_INFINITY, f64::max);
    nearest_clear_x(&ChannelSearch {
        y_lo: top_y.min(bus_y),
        y_hi: top_y.max(bus_y),
        trunk_x,
        first_left: min_x - NODE_SPACING_MM * 0.5,
        first_right: max_x + NODE_SPACING_MM * 0.5,
        nodes,
        exclude_ids,
    })
}

fn vertical_clear(
    x: f64,
    y1: f64,
    y2: f64,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
) -> bool {
    let lo = y1.min(y2);
    let hi = y1.max(y2);
    nodes.iter().all(|node| {
        exclude_ids.contains(&node.id)
            || !(x > node.x + 0.5
                && x < node.x + node.width - 0.5
                && hi > node.y + 0.5
                && lo < node.y + node.height - 0.5)
    })
}

fn side_channel_x(
    src: &PlacedNode,
    from_y: f64,
    bus_y: f64,
    trunk_x: f64,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
) -> f64 {
    nearest_clear_x(&ChannelSearch {
        y_lo: from_y.min(bus_y),
        y_hi: from_y.max(bus_y),
        trunk_x,
        first_left: src.x - NODE_SPACING_MM * 0.5,
        first_right: src.x + src.width + NODE_SPACING_MM * 0.5,
        nodes,
        exclude_ids,
    })
}

struct ChannelSearch<'a> {
    y_lo: f64,
    y_hi: f64,
    trunk_x: f64,
    first_left: f64,
    first_right: f64,
    nodes: &'a [PlacedNode],
    exclude_ids: &'a HashSet<NodeId>,
}

fn nearest_clear_x(search: &ChannelSearch<'_>) -> f64 {
    let ChannelSearch {
        y_lo,
        y_hi,
        trunk_x,
        first_left,
        first_right,
        nodes,
        exclude_ids,
    } = *search;
    let left_clear = vertical_clear(first_left, y_lo, y_hi, nodes, exclude_ids);
    let right_clear = vertical_clear(first_right, y_lo, y_hi, nodes, exclude_ids);
    if left_clear || right_clear {
        return closest_x(
            trunk_x,
            left_clear.then_some(first_left),
            right_clear.then_some(first_right),
        );
    }
    gap_channel_x(search).unwrap_or(first_right)
}

fn closest_x(trunk_x: f64, left: Option<f64>, right: Option<f64>) -> f64 {
    match (left, right) {
        (Some(left), Some(right)) => {
            if (left - trunk_x).abs() <= (right - trunk_x).abs() {
                left
            } else {
                right
            }
        }
        (Some(x), None) | (None, Some(x)) => x,
        (None, None) => trunk_x,
    }
}

fn gap_channel_x(search: &ChannelSearch<'_>) -> Option<f64> {
    let ChannelSearch {
        y_lo,
        y_hi,
        trunk_x,
        nodes,
        exclude_ids,
        ..
    } = *search;
    let mut intervals: Vec<(f64, f64)> = nodes
        .iter()
        .filter(|node| {
            !exclude_ids.contains(&node.id)
                && y_hi > node.y + 0.5
                && y_lo < node.y + node.height - 0.5
        })
        .map(|node| (node.x - 0.5, node.x + node.width + 0.5))
        .collect();
    if intervals.is_empty() {
        return None;
    }
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut merged = vec![intervals[0]];
    for (lo, hi) in intervals.into_iter().skip(1) {
        let last = merged.last_mut()?;
        if lo <= last.1 {
            last.1 = last.1.max(hi);
        } else {
            merged.push((lo, hi));
        }
    }
    let min_left = merged.first()?.0;
    let max_right = merged.last()?.1;
    let mut candidates = vec![
        min_left - NODE_SPACING_MM * 0.5,
        max_right + NODE_SPACING_MM * 0.5,
    ];
    for pair in merged.windows(2) {
        candidates.push(f64::midpoint(pair[0].1, pair[1].0));
    }
    let mut best: Option<f64> = None;
    for x in candidates {
        if !vertical_clear(x, y_lo, y_hi, nodes, exclude_ids) {
            continue;
        }
        let take = best.is_none_or(|current| (x - trunk_x).abs() < (current - trunk_x).abs());
        if take {
            best = Some(x);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::route_edges;
    use crate::error::ErrorCode;
    use crate::graph::NodeId;
    use crate::page::EdgeRef;
    use std::collections::HashMap;

    #[test]
    fn missing_endpoint_is_layout_failed() {
        let edges = [EdgeRef::new(NodeId::from_index(0), NodeId::from_index(1))];
        let error = route_edges(&edges, &HashMap::new()).expect_err("missing");
        assert_eq!(error.code(), ErrorCode::LayoutFailed);
    }
}
