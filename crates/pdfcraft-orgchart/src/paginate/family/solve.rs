//! Task solving: the memoised search over attachment tasks.

use std::collections::BTreeSet;

use super::head::over_elastic;
use super::{
    make_draft, AttachmentCut, AttachmentTask, ChildKind, Context, HeadPlan, PageDraft, PageOracle,
    PlacedChild, SearchPage, SearchPlan, TaskKind,
};
use crate::error::{Error, ErrorCode};
use crate::graph::NodeId;

impl<O: PageOracle> Context<'_, O> {
    pub(super) fn solve_task(
        &mut self,
        task: &AttachmentTask,
        seen_nodes: &BTreeSet<NodeId>,
    ) -> Result<SearchPlan, Error> {
        let key = (task.clone(), seen_nodes.clone());
        if let Some(&index) = self.memo.get(&key) {
            return Ok(self.plan_at(index));
        }
        let greedy = self.greedy_plan(task)?;
        if let Some(chain) = self.chain_path(task) {
            let best = self.solve_chain(task, &chain, seen_nodes, greedy)?;
            let index = self.store_plan(best);
            self.memo.insert(key, index);
            return Ok(self.plan_at(index));
        }
        let head = self.plan_head(task)?;
        let mut best = greedy;
        let mut best_score = self.score(&best)?;
        let pages = self.enumerate_inlines(task, &head)?;
        for page in pages {
            if self.budget_exhausted {
                break;
            }
            let violation = usize::from(page.layout.orientation_violation);
            if (violation, 0usize) > (best_score.orientation_violations, best_score.sparse_risk) {
                continue;
            }
            let candidate = self.expand_page(&page, seen_nodes)?;
            let candidate_score = self.score(&candidate)?;
            if candidate_score < best_score {
                best = candidate;
                best_score = candidate_score;
            }
        }
        let index = self.store_plan(best);
        self.memo.insert(key, index);
        Ok(self.plan_at(index))
    }

    fn expand_page(
        &mut self,
        page: &SearchPage,
        seen_nodes: &BTreeSet<NodeId>,
    ) -> Result<SearchPlan, Error> {
        let mut pages = vec![page.clone()];
        let mut branch_seen = seen_nodes.clone();
        branch_seen.extend(page.draft.node_ids.iter().copied());
        let mut follow_ups: Vec<AttachmentTask> = page
            .attachment_cuts
            .iter()
            .map(|cut| cut.task.clone())
            .collect();
        if let Some(overflow) = &page.overflow {
            follow_ups.insert(0, overflow.task.clone());
        }
        for follow_up in follow_ups {
            let subplan = self.solve_task(&follow_up, &branch_seen)?;
            for item in &subplan.pages {
                branch_seen.extend(item.draft.node_ids.iter().copied());
            }
            pages.extend(subplan.pages);
        }
        Ok(SearchPlan { pages })
    }

    pub(super) fn classify(
        &mut self,
        task: &AttachmentTask,
        child_id: NodeId,
    ) -> Result<ChildKind, Error> {
        let cone = self.reachable_edges(child_id, &task.edge_ids);
        if cone.is_empty() {
            if self.graph.person_ids.contains(&child_id) {
                return Ok(ChildKind::Person);
            }
            return Ok(ChildKind::LeafCompany);
        }
        let mut node_ids = BTreeSet::from([task.subject_id, child_id]);
        node_ids.extend(self.nodes_below(&cone));
        let draft = PageDraft {
            subject_id: task.subject_id,
            node_ids,
            stub_ids: BTreeSet::new(),
            overflow: None,
            kind: task.kind,
        };
        draft.check()?;
        let layout = self.evaluate(&draft)?;
        if !layout.fits || over_elastic(&layout) {
            Ok(ChildKind::LargeFamily)
        } else {
            Ok(ChildKind::SmallFamily)
        }
    }

    pub(super) fn page_for(
        &mut self,
        task: &AttachmentTask,
        head: &HeadPlan,
        inlined: &BTreeSet<NodeId>,
    ) -> Result<SearchPage, Error> {
        let mut node_ids = head.base_node_ids.clone();
        let mut cut_ids = Vec::new();
        for child in &head.placed {
            match child {
                PlacedChild::Leaf => {}
                PlacedChild::Inlined {
                    child_id,
                    node_ids: below,
                    ..
                } if inlined.contains(child_id) => {
                    node_ids.extend(below.iter().copied());
                }
                PlacedChild::Inlined { child_id, .. } | PlacedChild::Stub(child_id) => {
                    cut_ids.push(*child_id);
                }
            }
        }
        let cuts: Vec<AttachmentCut> = cut_ids
            .iter()
            .map(|child_id| AttachmentCut {
                task: AttachmentTask {
                    subject_id: *child_id,
                    edge_ids: self.reachable_edges(*child_id, &task.edge_ids),
                    kind: TaskKind::Primary,
                },
            })
            .collect();
        let overflow_node = head.overflow.as_ref().map(|cut| cut.node.clone());
        let draft = make_draft(task, node_ids, &cuts, overflow_node)?;
        let layout = if inlined.is_empty()
            && head.direct_layout.is_some()
            && head.direct_draft.as_ref() == Some(&draft)
        {
            head.direct_layout
                .clone()
                .ok_or_else(|| Error::new(ErrorCode::LayoutFailed, "direct layout missing"))?
        } else {
            self.evaluate(&draft)?
        };
        Ok(SearchPage {
            task: task.clone(),
            draft,
            attachment_cuts: cuts,
            overflow: head.overflow.clone(),
            layout,
        })
    }
}
