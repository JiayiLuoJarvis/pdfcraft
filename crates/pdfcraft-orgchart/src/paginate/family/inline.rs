//! Inline-subset enumeration for one task: the greedy plan and the full walk.

use std::collections::BTreeSet;

use super::head::over_elastic;
use super::{AttachmentTask, Context, HeadPlan, PageOracle, SearchPage, SearchPlan};
use crate::error::Error;

impl<O: PageOracle> Context<'_, O> {
    pub(super) fn greedy_plan(&mut self, task: &AttachmentTask) -> Result<SearchPlan, Error> {
        if let Some(&index) = self.greedy_memo.get(task) {
            return Ok(self.plan_at(index));
        }
        let head = self.plan_head(task)?;
        let mut inlined = BTreeSet::new();
        for (child_id, _, _) in &head.units {
            let mut candidate = inlined.clone();
            candidate.insert(*child_id);
            let layout = self.page_for(task, &head, &candidate)?.layout;
            if layout.fits && !over_elastic(&layout) {
                inlined = candidate;
            }
        }
        let page = self.page_for(task, &head, &inlined)?;
        let mut pages = vec![page.clone()];
        let mut follow_ups: Vec<AttachmentTask> = page
            .attachment_cuts
            .iter()
            .map(|cut| cut.task.clone())
            .collect();
        if let Some(overflow) = &page.overflow {
            follow_ups.insert(0, overflow.task.clone());
        }
        for follow_up in follow_ups {
            pages.extend(self.greedy_plan(&follow_up)?.pages);
        }
        let index = self.store_plan(SearchPlan { pages });
        self.greedy_memo.insert(task.clone(), index);
        Ok(self.plan_at(index))
    }

    pub(super) fn enumerate_inlines(
        &mut self,
        task: &AttachmentTask,
        head: &HeadPlan,
    ) -> Result<Vec<SearchPage>, Error> {
        let mut pages = Vec::new();
        let mut stack = vec![BTreeSet::new()];
        let mut visited = BTreeSet::new();
        while let Some(state) = stack.pop() {
            if !visited.insert(state.clone()) {
                continue;
            }
            if !self.consume_expansion() {
                break;
            }
            pages.push(self.page_for(task, head, &state)?);
            let mut next_states = Vec::new();
            for (child_id, _, _) in &head.units {
                if state.contains(child_id) {
                    continue;
                }
                let mut expanded = state.clone();
                expanded.insert(*child_id);
                if visited.contains(&expanded) {
                    continue;
                }
                let layout = self.page_for(task, head, &expanded)?.layout;
                if !layout.fits || over_elastic(&layout) {
                    continue;
                }
                next_states.push(expanded);
            }
            stack.extend(next_states.into_iter().rev());
        }
        Ok(pages)
    }
}
