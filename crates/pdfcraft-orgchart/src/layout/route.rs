//! Orthogonal edge routing.

use crate::ids::NodeId;
use std::collections::{HashMap, HashSet};

use crate::error::{Error, ErrorCode};
use crate::page::{EdgeRef, LaidOutEdge, PlacedNode, PointMm};
use crate::units::{
    BUS_ENDPOINT_CLEARANCE_MM, BUS_STAGGER_MM, BUS_Y_EPS_MM, NODE_SPACING_MM, ROW_GAP_MM,
    SHAREHOLDER_STUB_MIN_MM,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BusRole {
    MultiIn,
    MultiOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BusKey {
    role: BusRole,
    endpoint_id: NodeId,
}

#[derive(Debug, Clone, PartialEq)]
struct BusSpan {
    key: BusKey,
    y_mm: f64,
    x0_mm: f64,
    x1_mm: f64,
}

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
    multi_in_row_bottom: HashMap<NodeId, HashMap<u64, f64>>,
    multi_in_bottom_row_y: HashMap<NodeId, f64>,
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
    };
    for edge in page_edges {
        let (Some(src), Some(dst)) = (nodes.get(&edge.from_id), nodes.get(&edge.to_id)) else {
            continue;
        };
        let src_bottom = PointMm {
            x: src.x + src.width / 2.0,
            y: src.y + src.height,
        };
        let dst_top = PointMm {
            x: dst.x + dst.width / 2.0,
            y: dst.y,
        };
        let siblings_in: Vec<NodeId> = by_target
            .get(&edge.to_id)
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|id| nodes.contains_key(id))
                    .collect()
            })
            .unwrap_or_default();
        let siblings_out: Vec<NodeId> = by_source
            .get(&edge.from_id)
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|id| nodes.contains_key(id))
                    .collect()
            })
            .unwrap_or_default();
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
    let sources: Vec<&PlacedNode> = siblings_in.iter().map(|id| &nodes[id]).collect();
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
            let mut grouped: HashMap<u64, f64> = HashMap::new();
            for source in &sources {
                let key = source.y.to_bits();
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
    let row_bottoms = caches.multi_in_row_bottom[&edge.to_id].clone();
    let bottom_row_y = caches.multi_in_bottom_row_y[&edge.to_id];
    let multi_row = row_bottoms.len() > 1;
    let column_stack = sources
        .iter()
        .all(|node| (node.x - sources[0].x).abs() < 0.01);
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
        spine_x = Some(caches.multi_in_spine_x[&edge.to_id]);
    }
    let path_to_bus = if column_stack {
        let spine_x = spine_x.ok_or_else(|| {
            Error::new(
                ErrorCode::LayoutFailed,
                "column_stack requires a shared spine_x",
            )
        })?;
        route_side_to_spine(src, spine_x, bus_y, trunk_x)
    } else {
        let use_spine = multi_row && src.y < bottom_row_y - 0.01;
        let row_bottom = row_bottoms[&src.y.to_bits()];
        route_down_to_bus(
            src,
            src_bottom,
            bus_y,
            trunk_x,
            &node_list,
            &HashSet::from([edge.from_id, edge.to_id]),
            if use_spine { spine_x } else { None },
            row_bottom,
        )
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
    let children: Vec<&PlacedNode> = siblings_out.iter().map(|id| &nodes[id]).collect();
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
        },
        candidate,
        &centers,
        y_limit,
        &node_list,
        &exclude,
    )?;
    let bus_y = span.y_mm;
    let points = if caches.drawn_trunk_from.insert(edge.from_id) {
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
    };
    Ok(LaidOutEdge {
        from_id: edge.from_id,
        to_id: edge.to_id,
        points,
        draw_arrow: true,
    })
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

fn outward_points(
    src: &PlacedNode,
    src_bottom: PointMm,
    bus_y: f64,
    dst_top: PointMm,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
) -> Vec<PointMm> {
    let trunk_x = src_bottom.x;
    let mut down = if vertical_clear(trunk_x, src_bottom.y, bus_y, nodes, exclude_ids) {
        vec![
            src_bottom,
            PointMm {
                x: trunk_x,
                y: bus_y,
            },
        ]
    } else {
        let exit_y = (src_bottom.y + ROW_GAP_MM * 0.5).min(bus_y - 0.5);
        let side_x = side_channel_x(src, exit_y, bus_y, dst_top.x, nodes, exclude_ids);
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
    };
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

fn route_down_to_bus(
    src: &PlacedNode,
    src_bottom: PointMm,
    bus_y: f64,
    trunk_x: f64,
    nodes: &[PlacedNode],
    exclude_ids: &HashSet<NodeId>,
    spine_x: Option<f64>,
    row_max_bottom: f64,
) -> Vec<PointMm> {
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
    let left = min_x - NODE_SPACING_MM * 0.5;
    let right = max_x + NODE_SPACING_MM * 0.5;
    let left_clear = vertical_clear(left, top_y, bus_y, nodes, exclude_ids);
    let right_clear = vertical_clear(right, top_y, bus_y, nodes, exclude_ids);
    if left_clear && right_clear {
        if (left - trunk_x).abs() < (right - trunk_x).abs() - 0.01 {
            left
        } else {
            right
        }
    } else if left_clear {
        left
    } else {
        right
    }
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
    let left = src.x - NODE_SPACING_MM * 0.5;
    let right = src.x + src.width + NODE_SPACING_MM * 0.5;
    let left_clear = vertical_clear(left, from_y, bus_y, nodes, exclude_ids);
    let right_clear = vertical_clear(right, from_y, bus_y, nodes, exclude_ids);
    if left_clear && !right_clear {
        return left;
    }
    if right_clear && !left_clear {
        return right;
    }
    if (left - trunk_x).abs() <= (right - trunk_x).abs() {
        left
    } else {
        right
    }
}
