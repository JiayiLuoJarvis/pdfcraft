//! Layered placement. A page subject's direct row may wrap. Deeper rows stay on one line.

mod groups;
mod place;
mod ranks;
mod route;

use crate::ids::NodeId;
use std::collections::{BTreeMap, HashMap};

use groups::{collect_layer_groups, group_band_rows, use_family_columns};
use place::{
    dense_schedules, group_band_width_mm, measured_layer_width_mm, place_layers,
    resolve_row_schedules, LayerPlan, Spacing,
};
use ranks::prepare;
use route::route_edges;

use crate::error::Error;
use crate::measure::NodeBox;
use crate::page::{EdgeRef, LayoutResult};
type FamilyBands = Vec<Vec<Vec<NodeId>>>;

use crate::units::{within, FAMILY_GAP_MM, LAYER_SPACING_MM, NODE_SPACING_MM, ROW_GAP_MM};

/// Row counts a pager uses to decide whether a page may keep its deeper families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // `rows` is the unit these counts share
pub(crate) struct FamilyRowMetrics {
    /// Widest band, in rows, including the subject's own children.
    pub max_family_rows: usize,
    /// Rows beyond the first in every band.
    pub extra_rows: usize,
    /// Widest band below the subject's own children. That band must stay one row.
    pub max_nondirect_rows: usize,
}

/// Places boxes and reports how many rows each family uses.
pub trait LayoutEngine {
    /// Place `boxes` inside `printable_width_mm`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ErrorCode::LayoutFailed`] when a bus or a gap schedule is illegal.
    fn layout(
        &self,
        boxes: &[NodeBox],
        edges: &[EdgeRef],
        node_order: &[NodeId],
        cols: usize,
        node_width_mm: f64,
        row_gap_mm: f64,
        printable_width_mm: f64,
    ) -> Result<LayoutResult, Error>;

    /// Row counts for the same inputs, without placing coordinates.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ErrorCode::LayoutFailed`] when ranks cannot be assigned.
    fn family_row_metrics(
        &self,
        boxes: &[NodeBox],
        edges: &[EdgeRef],
        node_order: &[NodeId],
        cols: usize,
        node_width_mm: f64,
        printable_width_mm: f64,
    ) -> Result<FamilyRowMetrics, Error>;
}

/// Production packer. Forks use family columns. Chains stack.
#[derive(Debug, Default, Clone, Copy)]
pub struct WrappedEngine;

impl LayoutEngine for WrappedEngine {
    fn layout(
        &self,
        boxes: &[NodeBox],
        edges: &[EdgeRef],
        node_order: &[NodeId],
        cols: usize,
        node_width_mm: f64,
        row_gap_mm: f64,
        printable_width_mm: f64,
    ) -> Result<LayoutResult, Error> {
        let Some(prepared) = prepare(boxes, edges, node_order)? else {
            return Ok(LayoutResult::empty());
        };
        let box_by_id: HashMap<NodeId, NodeBox> =
            boxes.iter().map(|box_| (box_.id, box_.clone())).collect();
        let mut child_widths = HashMap::new();
        let spacing = spacing_of(node_width_mm, row_gap_mm, printable_width_mm);
        let plans = build_plans(&prepared, cols, &box_by_id, &mut child_widths, spacing)?;
        let laid_out = place_layers(&plans, &box_by_id, &child_widths, spacing)?;
        let edges = route_edges(&prepared.page_edges, &laid_out)?;
        LayoutResult::from_parts(laid_out.into_values().collect(), edges, Vec::new())
    }

    fn family_row_metrics(
        &self,
        boxes: &[NodeBox],
        edges: &[EdgeRef],
        node_order: &[NodeId],
        cols: usize,
        node_width_mm: f64,
        printable_width_mm: f64,
    ) -> Result<FamilyRowMetrics, Error> {
        let Some(prepared) = prepare(boxes, edges, node_order)? else {
            return Ok(FamilyRowMetrics {
                max_family_rows: 0,
                extra_rows: 0,
                max_nondirect_rows: 0,
            });
        };
        let box_by_id: HashMap<NodeId, NodeBox> =
            boxes.iter().map(|box_| (box_.id, box_.clone())).collect();
        let spacing = spacing_of(node_width_mm, ROW_GAP_MM, printable_width_mm);
        let plans = structure_plans(&prepared, cols, &box_by_id, spacing)?;
        let mut row_counts = Vec::new();
        let mut nondirect = Vec::new();
        for plan in &plans {
            for (group_index, rows) in plan.bands_per_group.iter().enumerate() {
                if rows.is_empty() {
                    continue;
                }
                row_counts.push(rows.len());
                if !is_subject_direct_group(&prepared, plan, group_index) {
                    nondirect.push(rows.len());
                }
            }
        }
        Ok(FamilyRowMetrics {
            max_family_rows: row_counts.iter().copied().max().unwrap_or(0),
            extra_rows: row_counts.iter().map(|count| count.saturating_sub(1)).sum(),
            max_nondirect_rows: nondirect.iter().copied().max().unwrap_or(0),
        })
    }
}

fn spacing_of(node_width_mm: f64, row_gap_mm: f64, printable_width_mm: f64) -> Spacing {
    Spacing {
        node_width_mm,
        node_spacing_mm: NODE_SPACING_MM,
        layer_spacing_mm: LAYER_SPACING_MM,
        row_gap_mm,
        family_gap_mm: FAMILY_GAP_MM,
        printable_width_mm,
    }
}

fn is_subject_direct_group(
    prepared: &ranks::Prepared,
    plan: &LayerPlan,
    group_index: usize,
) -> bool {
    let (Some(subject_rank), Some(subject_id)) = (prepared.subject_rank, prepared.subject_id)
    else {
        return false;
    };
    if (plan.rank - subject_rank).abs() != 1 {
        return false;
    }
    plan.group_parents
        .get(group_index)
        .is_some_and(|parent| *parent == subject_id)
}

fn build_plans(
    prepared: &ranks::Prepared,
    cols: usize,
    box_by_id: &HashMap<NodeId, NodeBox>,
    child_widths: &mut HashMap<NodeId, f64>,
    spacing: Spacing,
) -> Result<Vec<LayerPlan>, Error> {
    let mut plans = structure_plans(prepared, cols, box_by_id, spacing)?;
    fixup_shallow_gaps(
        &mut plans,
        child_widths,
        prepared.subject_rank,
        box_by_id,
        spacing,
    )?;
    Ok(plans)
}

fn structure_plans(
    prepared: &ranks::Prepared,
    cols: usize,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<Vec<LayerPlan>, Error> {
    let safe_cols = cols.max(1);
    let mut by_rank: BTreeMap<i32, Vec<NodeId>> = BTreeMap::new();
    for node_id in &prepared.ordered_ids {
        by_rank
            .entry(prepared.ranks[node_id])
            .or_default()
            .push(*node_id);
    }
    let (groups_by_rank, parents_by_rank) = collect_layer_groups(
        &by_rank,
        &prepared.page_edges,
        &prepared.ranks,
        &prepared.order_index,
        prepared.subject_rank,
    );
    let mut plans = Vec::new();
    for layer_rank in by_rank.keys().copied() {
        let groups = &groups_by_rank[&layer_rank];
        let family = use_family_columns(prepared.subject_rank, layer_rank, groups.len());
        let (bands, layer_width) = if family {
            family_bands_within_width(groups, prepared, safe_cols, box_by_id, spacing)?
        } else {
            let bands = bands_for(groups, prepared, safe_cols);
            let schedules = dense_schedules(&bands);
            let width = measured_layer_width_mm(&bands, &schedules, false, box_by_id, spacing)?;
            (bands, width)
        };
        let schedules = dense_schedules(&bands);
        plans.push(LayerPlan {
            rank: layer_rank,
            bands_per_group: bands,
            family_columns: family,
            layer_width_mm: layer_width,
            row_schedules: schedules,
            group_parents: parents_by_rank[&layer_rank].clone(),
        });
    }
    Ok(plans)
}

fn bands_for(
    groups: &[Vec<NodeId>],
    prepared: &ranks::Prepared,
    cols: usize,
) -> Vec<Vec<Vec<NodeId>>> {
    groups
        .iter()
        .map(|group| {
            group_band_rows(
                group,
                &prepared.page_edges,
                &prepared.ranks,
                prepared.subject_rank,
                &prepared.order_index,
                cols,
            )
        })
        .collect()
}

fn family_bands_within_width(
    groups: &[Vec<NodeId>],
    prepared: &ranks::Prepared,
    safe_cols: usize,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<(FamilyBands, f64), Error> {
    let mut best_bands = Vec::new();
    let mut best_width = 0.0;
    for cols_try in (1..=safe_cols).rev() {
        let bands = bands_for(groups, prepared, cols_try);
        let schedules = dense_schedules(&bands);
        let layer_width = measured_layer_width_mm(&bands, &schedules, true, box_by_id, spacing)?;
        best_bands = bands;
        best_width = layer_width;
        if within(layer_width, spacing.printable_width_mm) {
            return Ok((best_bands, best_width));
        }
    }
    Ok((best_bands, best_width))
}

fn leaf_to_root_key(plan: &LayerPlan, subject_rank: Option<i32>) -> (i32, i32) {
    match subject_rank {
        None => (-plan.rank, 0),
        Some(subject_rank) => (-(plan.rank - subject_rank).abs(), plan.rank),
    }
}

fn fixup_shallow_gaps(
    plans: &mut [LayerPlan],
    child_widths: &mut HashMap<NodeId, f64>,
    subject_rank: Option<i32>,
    box_by_id: &HashMap<NodeId, NodeBox>,
    spacing: Spacing,
) -> Result<(), Error> {
    let mut order: Vec<usize> = (0..plans.len()).collect();
    order.sort_by_key(|index| leaf_to_root_key(&plans[*index], subject_rank));
    for index in order {
        let schedules = resolve_row_schedules(
            &plans[index].bands_per_group,
            plans[index].family_columns,
            child_widths,
            box_by_id,
            spacing,
        )?;
        let width = measured_layer_width_mm(
            &plans[index].bands_per_group,
            &schedules,
            plans[index].family_columns,
            box_by_id,
            spacing,
        )?;
        let family = plans[index].family_columns;
        let parents = plans[index].group_parents.clone();
        let bands = plans[index].bands_per_group.clone();
        plans[index].row_schedules.clone_from(&schedules);
        plans[index].layer_width_mm = width;
        if family {
            for ((parent_id, band), band_schedules) in
                parents.iter().zip(bands.iter()).zip(schedules.iter())
            {
                let width = group_band_width_mm(band, band_schedules, box_by_id, spacing)?;
                child_widths
                    .entry(*parent_id)
                    .and_modify(|previous| *previous = previous.max(width))
                    .or_insert(width);
            }
        }
    }
    Ok(())
}
