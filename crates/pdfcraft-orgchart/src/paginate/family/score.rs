//! Plan and overflow-run scores, and the page-utilisation helpers behind them.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use super::head::over_elastic;
use super::{
    AttachmentTask, Context, LayoutEvaluation, OverflowScore, PageDraft, PageOracle, PlanScore,
    SearchPage, SearchPlan, SPARSE_UTILIZATION, UTILIZATION_BUCKET,
};
use crate::error::Error;

impl PartialEq for PlanScore {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for PlanScore {}
impl PartialOrd for PlanScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for PlanScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.orientation_violations
            .cmp(&other.orientation_violations)
            .then(self.sparse_risk.cmp(&other.sparse_risk))
            .then(self.attachment_cuts.cmp(&other.attachment_cuts))
            .then(self.repeated_context.cmp(&other.repeated_context))
            .then(self.page_count.cmp(&other.page_count))
            .then(self.input_order.cmp(&other.input_order))
    }
}

impl PartialEq for OverflowScore {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for OverflowScore {}
impl PartialOrd for OverflowScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OverflowScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.page_count
            .cmp(&other.page_count)
            .then(self.degraded_pages.cmp(&other.degraded_pages))
            .then(self.sparse_flag.cmp(&other.sparse_flag))
            .then(self.sparse_util.total_cmp(&other.sparse_util))
            .then(self.front_retention.cmp(&other.front_retention))
            .then(self.utilization_spread.total_cmp(&other.utilization_spread))
            .then(self.input_order.cmp(&other.input_order))
    }
}

impl<O: PageOracle> Context<'_, O> {
    pub(super) fn score(&mut self, plan: &SearchPlan) -> Result<PlanScore, Error> {
        let mut input_order = Vec::new();
        for page in &plan.pages {
            input_order.push(
                self.sorted_nodes(&page.draft.node_ids)
                    .into_iter()
                    .map(|id| self.rank(id))
                    .collect(),
            );
        }
        Ok(PlanScore {
            orientation_violations: plan
                .pages
                .iter()
                .map(|page| usize::from(page.layout.orientation_violation))
                .sum(),
            sparse_risk: self.sparse_risk(plan)?,
            attachment_cuts: plan
                .pages
                .iter()
                .map(|page| page.attachment_cuts.len())
                .sum(),
            repeated_context: repeated_context(plan),
            page_count: plan.pages.len(),
            input_order,
        })
    }

    fn sparse_risk(&mut self, plan: &SearchPlan) -> Result<usize, Error> {
        let mut first_by_task: HashMap<AttachmentTask, SearchPage> = HashMap::new();
        for page in &plan.pages {
            first_by_task
                .entry(page.task.clone())
                .or_insert_with(|| page.clone());
        }
        let mut risk = 0;
        for source in &plan.pages {
            for cut in &source.attachment_cuts {
                let Some(attachment) = first_by_task.get(&cut.task) else {
                    continue;
                };
                let mut node_ids = source.draft.node_ids.clone();
                node_ids.extend(attachment.draft.node_ids.iter().copied());
                let mut stub_ids = source.draft.stub_ids.clone();
                stub_ids.remove(&cut.task.subject_id);
                stub_ids.extend(attachment.draft.stub_ids.iter().copied());
                let merged = self.evaluate(&PageDraft {
                    subject_id: source.draft.subject_id,
                    node_ids,
                    stub_ids,
                    overflow: source.draft.overflow.clone(),
                    kind: source.draft.kind,
                })?;
                if !merged.fits || over_elastic(&merged) {
                    continue;
                }
                let printable = attachment.layout.printable_height_mm;
                if printable <= 0.0 {
                    continue;
                }
                risk += sparse_buckets(attachment.layout.height_mm / printable);
            }
        }
        Ok(risk)
    }
}

pub(super) fn page_utilization(layout: &LayoutEvaluation) -> f64 {
    if layout.printable_height_mm <= 0.0 {
        0.0
    } else {
        layout.height_mm / layout.printable_height_mm
    }
}

fn sparse_buckets(utilization: f64) -> usize {
    let gap = SPARSE_UTILIZATION - utilization;
    if gap <= 0.0 {
        return 0;
    }
    let quot = gap / UTILIZATION_BUCKET;
    if !quot.is_finite() || quot <= 0.0 {
        return 0;
    }
    let whole = quot.trunc();
    let mut count = 0_usize;
    let mut cursor = 0.0;
    while cursor + 1.0 <= whole {
        cursor += 1.0;
        count += 1;
    }
    count
}

fn repeated_context(plan: &SearchPlan) -> usize {
    let mut seen = BTreeSet::new();
    let mut extra = 0;
    for page in &plan.pages {
        let repeats = page
            .draft
            .node_ids
            .iter()
            .filter(|id| seen.contains(*id))
            .count();
        let necessary = usize::from(seen.contains(&page.draft.subject_id));
        extra += repeats.saturating_sub(necessary);
        seen.extend(page.draft.node_ids.iter().copied());
    }
    extra
}

#[cfg(test)]
mod tests {
    use super::sparse_buckets;

    #[test]
    fn a_utilization_gap_of_0_35_counts_six_buckets() {
        assert_eq!(sparse_buckets(0.0), 6);
        assert_eq!(sparse_buckets(0.05), 5);
    }

    #[test]
    fn a_non_positive_utilization_gap_counts_zero_buckets() {
        assert_eq!(sparse_buckets(0.35), 0);
        assert_eq!(sparse_buckets(0.5), 0);
    }
}
