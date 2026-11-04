//! Single-path chains: split a chain at each node and keep the best plan.

use std::collections::BTreeSet;

use super::{
    make_draft, AttachmentCut, AttachmentTask, Context, PageOracle, SearchPage, SearchPlan,
    TaskKind,
};
use crate::error::Error;
use crate::graph::NodeId;

impl<O: PageOracle> Context<'_, O> {
    pub(super) fn chain_path(&self, task: &AttachmentTask) -> Option<(Vec<NodeId>, Vec<usize>)> {
        let mut node_ids = vec![task.subject_id];
        let mut edge_ids = Vec::new();
        let mut remaining = task.edge_ids.clone();
        let mut current = task.subject_id;
        while !remaining.is_empty() {
            let outgoing: Vec<usize> = self
                .outgoing
                .get(&current)
                .into_iter()
                .flatten()
                .copied()
                .filter(|edge_id| remaining.contains(edge_id))
                .collect();
            if outgoing.len() != 1 {
                return None;
            }
            let edge_id = outgoing[0];
            let edge = &self.edge_by_id[&edge_id];
            edge_ids.push(edge_id);
            node_ids.push(edge.child_id);
            remaining.remove(&edge_id);
            current.clone_from(&edge.child_id);
        }
        Some((node_ids, edge_ids))
    }

    pub(super) fn solve_chain(
        &mut self,
        task: &AttachmentTask,
        chain: &(Vec<NodeId>, Vec<usize>),
        seen_nodes: &BTreeSet<NodeId>,
        baseline: SearchPlan,
    ) -> Result<SearchPlan, Error> {
        let (node_ids, edge_ids) = chain;
        let mut best = baseline;
        let mut best_score = self.score(&best)?;
        for end_index in 1..node_ids.len() {
            if !self.consume_expansion() {
                break;
            }
            let page_nodes: BTreeSet<NodeId> = node_ids[..=end_index].iter().copied().collect();
            let cuts = if end_index == node_ids.len() - 1 {
                Vec::new()
            } else {
                vec![AttachmentCut {
                    task: AttachmentTask {
                        subject_id: node_ids[end_index],
                        edge_ids: edge_ids[end_index..].iter().copied().collect(),
                        kind: TaskKind::Primary,
                    },
                }]
            };
            let draft = make_draft(task, page_nodes.clone(), &cuts, None)?;
            let layout = self.evaluate(&draft)?;
            if !layout.fits {
                break;
            }
            let page = SearchPage {
                task: task.clone(),
                draft,
                attachment_cuts: cuts.clone(),
                overflow: None,
                layout,
            };
            let candidate = if cuts.is_empty() {
                SearchPlan { pages: vec![page] }
            } else {
                let subplan = self.solve_task(&cuts[0].task, &{
                    let mut seen = seen_nodes.clone();
                    seen.extend(page_nodes);
                    seen
                })?;
                let mut pages = vec![page];
                pages.extend(subplan.pages);
                SearchPlan { pages }
            };
            let candidate_score = self.score(&candidate)?;
            if candidate_score < best_score {
                best = candidate;
                best_score = candidate_score;
            }
        }
        Ok(best)
    }
}
