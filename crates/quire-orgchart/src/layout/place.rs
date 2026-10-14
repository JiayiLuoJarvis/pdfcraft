//! Place nodes on layer coordinates.

use crate::ids::NodeId;
use std::collections::{HashMap, HashSet};

use crate::error::{Error, ErrorCode};
use crate::measure::NodeBox;
use crate::page::PlacedNode;
use crate::units::{FAMILY_GAP_MM, FIT_EPSILON_MM, NODE_SPACING_MM};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiblingGap {
    NodeSpacing,
    FamilyGap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowGapSchedule {
    pub(crate) gaps: Vec<SiblingGap>,
    pub(crate) degraded: bool,
}

impl RowGapSchedule {
    pub(crate) fn all_node(gap_count: usize, degraded: bool) -> Self {
        Self {
            gaps: vec![SiblingGap::NodeSpacing; gap_count],
            degraded,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LayerPlan {
    pub(crate) rank: i32,
    pub(crate) bands_per_group: Vec<Vec<Vec<NodeId>>>,
    pub(crate) family_columns: bool,
    pub(crate) layer_width_mm: f64,
    pub(crate) row_schedules: Vec<Vec<RowGapSchedule>>,
    pub(crate) group_parents: Vec<NodeId>,
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_field_names)]
pub(crate) struct Spacing {
    pub(crate) node_width_mm: f64,
    pub(crate) node_spacing_mm: f64,
    pub(crate) layer_spacing_mm: f64,
    pub(crate) row_gap_mm: f64,
    pub(crate) family_gap_mm: f64,
    pub(crate) printable_width_mm: f64,
}

struct ParentSlot {
    #[allow(dead_code)]
    node_id: NodeId,
    offset_x_mm: f64,
    slot_width_mm: f64,
}

struct SlottedRow {
    slots: Vec<ParentSlot>,
    width_mm: f64,
}

struct PlanSlotGeometry {
    rows_per_group: Vec<Vec<SlottedRow>>,
    layer_width_mm: f64,
}

enum ChildFamilyFit {
    Absent,
    Shallow,
    Deep,
}

pub(crate) fn place_layers(
    plans: &[LayerPlan],
    box_by_id: &HashMap<NodeId, NodeBox>,
    child_family_width_by_parent: &HashMap<NodeId, f64>,
    spacing: Spacing,
) -> Result<HashMap<NodeId, PlacedNode>, Error> {
    let pitch = spacing.node_width_mm + spacing.node_spacing_mm;
    let slot_geometries: Vec<Option<PlanSlotGeometry>> = plans
        .iter()
        .map(|plan| parent_slot_geometry(plan, child_family_width_by_parent, box_by_id, spacing))
        .collect();
    let effective_widths: Vec<f64> = plans
        .iter()
        .zip(&slot_geometries)
        .map(|(plan, geometry)| {
            geometry
                .as_ref()
                .map_or(plan.layer_width_mm, |item| item.layer_width_mm)
        })
        .collect();
    let mut content_width = spacing.node_width_mm;
    for width in effective_widths
        .iter()
        .copied()
        .chain(box_by_id.values().map(|box_| box_.width_mm))
    {
        content_width = content_width.max(width);
    }
    let xs = assign_xs(
        plans,
        &slot_geometries,
        content_width,
        pitch,
        box_by_id,
        spacing,
    )?;
    Ok(assign_ys(plans, &xs, box_by_id, spacing))
}

pub(crate) fn measured_layer_width_mm(
    bands_per_group: &[Vec<Vec<NodeId>>],
    schedules_per_group: &[Vec<RowGapSchedule>],
    family_columns: bool,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<f64, Error> {
    if family_columns {
        let mut total = 0.0;
        let mut count = 0usize;
        for (band, schedules) in bands_per_group.iter().zip(schedules_per_group) {
            total += group_band_width_mm(band, schedules, box_by_id, spacing)?;
            count += 1;
        }
        return Ok(total + spacing.family_gap_mm * crate::units::n64(count.saturating_sub(1)));
    }
    let mut width = 0.0_f64;
    for (band, schedules) in bands_per_group.iter().zip(schedules_per_group) {
        for (row, schedule) in band.iter().zip(schedules) {
            width = width.max(row_span_mm(row, box_by_id, schedule, spacing)?);
        }
    }
    Ok(width)
}

pub(crate) fn group_band_width_mm(
    band: &[Vec<NodeId>],
    schedules: &[RowGapSchedule],
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<f64, Error> {
    if band.is_empty() {
        return Ok(spacing.node_width_mm);
    }
    let mut width = 0.0_f64;
    for (row, schedule) in band.iter().zip(schedules) {
        width = width.max(row_span_mm(row, box_by_id, schedule, spacing)?);
    }
    Ok(width)
}

pub(crate) fn resolve_row_schedules(
    bands_per_group: &[Vec<Vec<NodeId>>],
    family_columns: bool,
    child_family_width_by_parent: &HashMap<NodeId, f64>,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<Vec<Vec<RowGapSchedule>>, Error> {
    let fit_of = |node_id: NodeId| -> ChildFamilyFit {
        let Some(width) = child_family_width_by_parent.get(&node_id) else {
            return ChildFamilyFit::Absent;
        };
        let box_width = box_width_mm(node_id, box_by_id, spacing.node_width_mm);
        if *width <= box_width + FIT_EPSILON_MM {
            ChildFamilyFit::Shallow
        } else {
            ChildFamilyFit::Deep
        }
    };
    let adaptive_row = |row_ids: &[NodeId]| -> RowGapSchedule {
        let mut gaps = Vec::new();
        for pair in row_ids.windows(2) {
            let both_shallow = matches!(fit_of(pair[0]), ChildFamilyFit::Shallow)
                && matches!(fit_of(pair[1]), ChildFamilyFit::Shallow);
            gaps.push(if both_shallow {
                SiblingGap::FamilyGap
            } else {
                SiblingGap::NodeSpacing
            });
        }
        RowGapSchedule {
            gaps,
            degraded: false,
        }
    };
    let mut chosen: Vec<Vec<RowGapSchedule>> = bands_per_group
        .iter()
        .map(|band| {
            band.iter()
                .map(|row| RowGapSchedule::all_node(row.len().saturating_sub(1), false))
                .collect()
        })
        .collect();
    for (group_index, band) in bands_per_group.iter().enumerate() {
        for (row_index, _row) in band.iter().enumerate() {
            let schedule = adaptive_row(&band[row_index]);
            if schedule
                .gaps
                .iter()
                .all(|gap| *gap == SiblingGap::NodeSpacing)
            {
                continue;
            }
            chosen[group_index][row_index] = schedule.clone();
            let width = measured_layer_width_mm(
                bands_per_group,
                &chosen,
                family_columns,
                box_by_id,
                spacing,
            )?;
            if width > spacing.printable_width_mm + FIT_EPSILON_MM {
                chosen[group_index][row_index] =
                    RowGapSchedule::all_node(schedule.gaps.len(), true);
            }
        }
    }
    Ok(chosen)
}

pub(crate) fn dense_schedules(bands: &[Vec<Vec<NodeId>>]) -> Vec<Vec<RowGapSchedule>> {
    bands
        .iter()
        .map(|band| {
            band.iter()
                .map(|row| RowGapSchedule::all_node(row.len().saturating_sub(1), false))
                .collect()
        })
        .collect()
}

fn row_span_mm(
    row_ids: &[NodeId],
    box_by_id: &HashMap<NodeId, NodeBox>,
    schedule: &RowGapSchedule,
    spacing: Spacing,
) -> Result<f64, Error> {
    if schedule.gaps.len() != row_ids.len().saturating_sub(1) {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            "one gap per adjacent pair",
        ));
    }
    let mut total = 0.0;
    for (index, node_id) in row_ids.iter().enumerate() {
        total += box_width_mm(*node_id, box_by_id, spacing.node_width_mm);
        if index + 1 < row_ids.len() {
            total += sibling_gap_mm(schedule.gaps[index], spacing);
        }
    }
    Ok(total)
}

fn sibling_gap_mm(gap: SiblingGap, spacing: Spacing) -> f64 {
    match gap {
        SiblingGap::FamilyGap => spacing.family_gap_mm,
        SiblingGap::NodeSpacing => spacing.node_spacing_mm,
    }
}

fn box_width_mm(node_id: NodeId, box_by_id: &HashMap<NodeId, NodeBox>, node_width_mm: f64) -> f64 {
    box_by_id
        .get(&node_id)
        .map_or(node_width_mm, |box_| box_.width_mm)
}

fn node_ids_in_plan(plan: &LayerPlan) -> HashSet<NodeId> {
    plan.bands_per_group
        .iter()
        .flat_map(|band| band.iter().flat_map(|row| row.iter().copied()))
        .collect()
}

fn parent_center_mm(
    parent_id: NodeId,
    xs: &HashMap<NodeId, f64>,
    box_by_id: &HashMap<NodeId, NodeBox>,
    node_width_mm: f64,
) -> f64 {
    let width = box_width_mm(parent_id, box_by_id, node_width_mm);
    xs[&parent_id] + width / 2.0
}

fn parents_ready(plan: &LayerPlan, xs: &HashMap<NodeId, f64>) -> bool {
    let plan_ids = node_ids_in_plan(plan);
    plan.group_parents
        .iter()
        .all(|parent| xs.contains_key(parent) || plan_ids.contains(parent))
}

fn assign_xs(
    plans: &[LayerPlan],
    slot_geometries: &[Option<PlanSlotGeometry>],
    content_width: f64,
    pitch: f64,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<HashMap<NodeId, f64>, Error> {
    let _ = pitch;
    let mut xs = HashMap::new();
    let mut remaining: HashSet<usize> = (0..plans.len()).collect();
    while !remaining.is_empty() {
        let ready: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|index| parents_ready(&plans[*index], &xs))
            .collect();
        if ready.is_empty() {
            for index in remaining {
                place_plan_fallback(
                    &plans[index],
                    slot_geometries[index].as_ref(),
                    &mut xs,
                    content_width,
                    box_by_id,
                    spacing,
                )?;
            }
            break;
        }
        for index in ready {
            place_plan_xs(
                &plans[index],
                slot_geometries[index].as_ref(),
                &mut xs,
                content_width,
                box_by_id,
                spacing,
            )?;
            remaining.remove(&index);
        }
    }
    Ok(xs)
}

fn place_plan_xs(
    plan: &LayerPlan,
    geometry: Option<&PlanSlotGeometry>,
    xs: &mut HashMap<NodeId, f64>,
    content_width: f64,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<(), Error> {
    if plan.family_columns {
        place_family_xs(plan, xs, content_width, box_by_id, spacing)?;
        return Ok(());
    }
    if let Some(geometry) = geometry {
        place_slotted_plan_xs(plan, geometry, xs, content_width, box_by_id, spacing);
        return Ok(());
    }
    if try_align_single_group(plan, xs, box_by_id, spacing)? {
        return Ok(());
    }
    place_plan_layer_centered(plan, xs, content_width, box_by_id, spacing)
}

fn place_plan_fallback(
    plan: &LayerPlan,
    geometry: Option<&PlanSlotGeometry>,
    xs: &mut HashMap<NodeId, f64>,
    content_width: f64,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<(), Error> {
    if let Some(geometry) = geometry {
        place_slotted_plan_xs(plan, geometry, xs, content_width, box_by_id, spacing);
        Ok(())
    } else {
        place_plan_layer_centered(plan, xs, content_width, box_by_id, spacing)
    }
}

fn try_align_single_group(
    plan: &LayerPlan,
    xs: &mut HashMap<NodeId, f64>,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<bool, Error> {
    if plan.bands_per_group.len() != 1 || plan.group_parents.len() != 1 {
        return Ok(false);
    }
    let parent_id = &plan.group_parents[0];
    let plan_ids = node_ids_in_plan(plan);
    if !xs.contains_key(parent_id) || plan_ids.contains(parent_id) {
        return Ok(false);
    }
    let parent_center = parent_center_mm(*parent_id, xs, box_by_id, spacing.node_width_mm);
    for (row_ids, schedule) in plan.bands_per_group[0].iter().zip(&plan.row_schedules[0]) {
        let width = row_span_mm(row_ids, box_by_id, schedule, spacing)?;
        write_row_xs(
            xs,
            row_ids,
            schedule,
            parent_center - width / 2.0,
            box_by_id,
            spacing,
        )?;
    }
    Ok(true)
}

fn write_row_xs(
    xs: &mut HashMap<NodeId, f64>,
    row_ids: &[NodeId],
    schedule: &RowGapSchedule,
    start_x: f64,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<(), Error> {
    if schedule.gaps.len() != row_ids.len().saturating_sub(1) {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            "one gap per adjacent pair",
        ));
    }
    let mut x = start_x;
    for (index, node_id) in row_ids.iter().enumerate() {
        xs.insert(*node_id, x);
        x += box_width_mm(*node_id, box_by_id, spacing.node_width_mm);
        if index + 1 < row_ids.len() {
            x += sibling_gap_mm(schedule.gaps[index], spacing);
        }
    }
    Ok(())
}

fn parent_slot_geometry(
    plan: &LayerPlan,
    child_family_width_by_parent: &HashMap<NodeId, f64>,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Option<PlanSlotGeometry> {
    if plan.family_columns {
        return None;
    }
    let plan_ids = node_ids_in_plan(plan);
    if !plan_ids
        .iter()
        .any(|id| child_family_width_by_parent.contains_key(id))
    {
        return None;
    }
    let mut rows_per_group = Vec::new();
    let mut layer_width = 0.0_f64;
    for group in &plan.bands_per_group {
        let mut slotted_rows = Vec::new();
        for row_ids in group {
            let slotted = slotted_row(row_ids, child_family_width_by_parent, box_by_id, spacing);
            layer_width = layer_width.max(slotted.width_mm);
            slotted_rows.push(slotted);
        }
        rows_per_group.push(slotted_rows);
    }
    if layer_width > spacing.printable_width_mm + FIT_EPSILON_MM {
        return None;
    }
    Some(PlanSlotGeometry {
        rows_per_group,
        layer_width_mm: layer_width,
    })
}

fn slotted_row(
    row_ids: &[NodeId],
    child_family_width_by_parent: &HashMap<NodeId, f64>,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> SlottedRow {
    let mut slots = Vec::new();
    let mut cursor = 0.0;
    for (index, node_id) in row_ids.iter().enumerate() {
        let box_width = box_width_mm(*node_id, box_by_id, spacing.node_width_mm);
        let slot_width = child_family_width_by_parent
            .get(node_id)
            .map_or(box_width, |child| child.max(box_width));
        slots.push(ParentSlot {
            node_id: *node_id,
            offset_x_mm: cursor,
            slot_width_mm: slot_width,
        });
        cursor += slot_width;
        if index + 1 < row_ids.len() {
            cursor += spacing.family_gap_mm;
        }
    }
    SlottedRow {
        slots,
        width_mm: cursor,
    }
}

fn place_slotted_plan_xs(
    plan: &LayerPlan,
    geometry: &PlanSlotGeometry,
    xs: &mut HashMap<NodeId, f64>,
    content_width: f64,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) {
    let mut anchor = content_width / 2.0;
    if plan.bands_per_group.len() == 1 && plan.group_parents.len() == 1 {
        let parent_id = &plan.group_parents[0];
        let plan_ids = node_ids_in_plan(plan);
        if xs.contains_key(parent_id) && !plan_ids.contains(parent_id) {
            anchor = parent_center_mm(*parent_id, xs, box_by_id, spacing.node_width_mm);
        }
    }
    for (group_rows, slotted_rows) in plan.bands_per_group.iter().zip(&geometry.rows_per_group) {
        for (row_ids, slotted) in group_rows.iter().zip(slotted_rows) {
            let row_start = anchor - slotted.width_mm / 2.0;
            for (node_id, slot) in row_ids.iter().zip(&slotted.slots) {
                let box_width = box_width_mm(*node_id, box_by_id, spacing.node_width_mm);
                xs.insert(
                    *node_id,
                    row_start + slot.offset_x_mm + (slot.slot_width_mm - box_width) / 2.0,
                );
            }
        }
    }
}

fn place_plan_layer_centered(
    plan: &LayerPlan,
    xs: &mut HashMap<NodeId, f64>,
    content_width: f64,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<(), Error> {
    let layer_offset_x = (content_width - plan.layer_width_mm) / 2.0;
    for (rows, schedules) in plan.bands_per_group.iter().zip(&plan.row_schedules) {
        for (row_ids, schedule) in rows.iter().zip(schedules) {
            let row_width = row_span_mm(row_ids, box_by_id, schedule, spacing)?;
            let start = layer_offset_x + (plan.layer_width_mm - row_width) / 2.0;
            write_row_xs(xs, row_ids, schedule, start, box_by_id, spacing)?;
        }
    }
    Ok(())
}

fn family_widths(
    plan: &LayerPlan,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<Vec<f64>, Error> {
    plan.bands_per_group
        .iter()
        .zip(&plan.row_schedules)
        .map(|(rows, schedules)| group_band_width_mm(rows, schedules, box_by_id, spacing))
        .collect()
}

fn parent_xs_overlap(parent_xs: &[f64], parent_widths: &[f64]) -> bool {
    let mut ordered: Vec<(f64, f64)> = parent_xs
        .iter()
        .copied()
        .zip(parent_widths.iter().copied())
        .collect();
    ordered.sort_by(|left, right| left.0.total_cmp(&right.0));
    ordered
        .windows(2)
        .any(|pair| pair[1].0 - pair[0].0 < pair[0].1 - 0.5)
}

fn soft_family_starts(centers: &[f64], widths: &[f64], gap_mm: f64) -> Vec<f64> {
    let mut starts = Vec::new();
    let mut cursor = f64::NEG_INFINITY;
    for (center, width) in centers.iter().zip(widths) {
        let ideal = center - width / 2.0;
        let start = ideal.max(cursor);
        starts.push(start);
        cursor = start + width + gap_mm;
    }
    starts
}

fn place_family_xs(
    plan: &LayerPlan,
    xs: &mut HashMap<NodeId, f64>,
    content_width: f64,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<(), Error> {
    let layer_offset_x = (content_width - plan.layer_width_mm) / 2.0;
    let family_widths = family_widths(plan, box_by_id, spacing)?;
    let ready: Vec<usize> = (0..plan.bands_per_group.len())
        .filter(|index| {
            plan.group_parents
                .get(*index)
                .is_some_and(|parent| xs.contains_key(parent))
        })
        .collect();
    let align = ready.len() == plan.bands_per_group.len()
        && !parent_xs_overlap(
            &ready
                .iter()
                .map(|index| xs[&plan.group_parents[*index]])
                .collect::<Vec<_>>(),
            &ready
                .iter()
                .map(|index| {
                    box_width_mm(plan.group_parents[*index], box_by_id, spacing.node_width_mm)
                })
                .collect::<Vec<_>>(),
        );
    if align {
        let mut order: Vec<usize> = (0..ready.len()).collect();
        let centers: Vec<f64> = ready
            .iter()
            .map(|index| {
                parent_center_mm(
                    plan.group_parents[*index],
                    xs,
                    box_by_id,
                    spacing.node_width_mm,
                )
            })
            .collect();
        order.sort_by(|&left, &right| centers[left].total_cmp(&centers[right]));
        let ordered_centers: Vec<f64> = order.iter().map(|index| centers[*index]).collect();
        let ordered_widths: Vec<f64> = order.iter().map(|index| family_widths[*index]).collect();
        let ordered_starts =
            soft_family_starts(&ordered_centers, &ordered_widths, spacing.family_gap_mm);
        let mut starts = vec![0.0; order.len()];
        for (slot, col_index) in order.iter().enumerate() {
            starts[*col_index] = ordered_starts[slot];
        }
        for (col_index, (rows, schedules)) in plan
            .bands_per_group
            .iter()
            .zip(&plan.row_schedules)
            .enumerate()
        {
            for (row_ids, schedule) in rows.iter().zip(schedules) {
                let row_width = row_span_mm(row_ids, box_by_id, schedule, spacing)?;
                write_row_xs(
                    xs,
                    row_ids,
                    schedule,
                    starts[col_index] + (family_widths[col_index] - row_width) / 2.0,
                    box_by_id,
                    spacing,
                )?;
            }
        }
        return Ok(());
    }
    let mut cursor = layer_offset_x;
    for (col_index, (rows, schedules)) in plan
        .bands_per_group
        .iter()
        .zip(&plan.row_schedules)
        .enumerate()
    {
        let family_width = family_widths[col_index];
        for (row_ids, schedule) in rows.iter().zip(schedules) {
            let row_width = row_span_mm(row_ids, box_by_id, schedule, spacing)?;
            write_row_xs(
                xs,
                row_ids,
                schedule,
                cursor + (family_width - row_width) / 2.0,
                box_by_id,
                spacing,
            )?;
        }
        cursor += family_width + spacing.family_gap_mm;
    }
    Ok(())
}

fn assign_ys(
    plans: &[LayerPlan],
    xs: &HashMap<NodeId, f64>,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> HashMap<NodeId, PlacedNode> {
    let mut laid_out = HashMap::new();
    let mut y = 0.0;
    for (layer_index, plan) in plans.iter().enumerate() {
        if layer_index > 0 {
            y += spacing.layer_spacing_mm;
        }
        if plan.family_columns {
            let mut layer_bottom = y;
            for rows in &plan.bands_per_group {
                let mut y_col = y;
                for (row_index, row_ids) in rows.iter().enumerate() {
                    if row_index > 0 {
                        y_col += spacing.row_gap_mm;
                    }
                    let row_height = row_ids
                        .iter()
                        .map(|id| box_by_id[id].height_mm)
                        .fold(0.0, f64::max);
                    for node_id in row_ids {
                        let box_ = &box_by_id[node_id];
                        laid_out.insert(
                            *node_id,
                            PlacedNode {
                                id: *node_id,
                                protocol_id: box_.protocol_id.clone(),
                                x: xs[node_id],
                                y: y_col,
                                width: box_.width_mm,
                                height: box_.height_mm,
                            },
                        );
                    }
                    y_col += row_height;
                }
                layer_bottom = layer_bottom.max(y_col);
            }
            y = layer_bottom;
        } else {
            let mut first_group = true;
            for rows in &plan.bands_per_group {
                if !first_group {
                    y += spacing.family_gap_mm;
                }
                first_group = false;
                for (row_index, row_ids) in rows.iter().enumerate() {
                    if row_index > 0 {
                        y += spacing.row_gap_mm;
                    }
                    let row_height = row_ids
                        .iter()
                        .map(|id| box_by_id[id].height_mm)
                        .fold(0.0, f64::max);
                    for node_id in row_ids {
                        let box_ = &box_by_id[node_id];
                        laid_out.insert(
                            *node_id,
                            PlacedNode {
                                id: *node_id,
                                protocol_id: box_.protocol_id.clone(),
                                x: xs[node_id],
                                y,
                                width: box_.width_mm,
                                height: box_.height_mm,
                            },
                        );
                    }
                    y += row_height;
                }
            }
        }
    }
    let _ = (NODE_SPACING_MM, FAMILY_GAP_MM);
    laid_out
}
