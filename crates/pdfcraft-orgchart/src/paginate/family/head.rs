//! The first page of a task: which direct children stay, which move to a continuation.

use std::collections::{BTreeSet, HashMap, VecDeque};

use super::score::page_utilization;
use super::{
    make_draft, AttachmentCut, AttachmentTask, ChainPage, ChildKind, Context, DirectFit, HeadPlan,
    LayoutEvaluation, OverflowCut, OverflowNode, OverflowScore, PageDraft, PageOracle, PlacedChild,
    TaskKind, SPARSE_UTILIZATION,
};
use crate::error::{Error, ErrorCode};
use crate::graph::NodeId;
use crate::units::FIT_EPSILON_MM;

struct DirectChoice {
    layout: LayoutEvaluation,
    degraded: bool,
    draft: PageDraft,
}

struct MeasuredCut {
    retained: Vec<NodeId>,
    overflow: Option<OverflowCut>,
    draft: PageDraft,
    fit: DirectFit,
}

impl<O: PageOracle> Context<'_, O> {
    pub(super) fn plan_head(&mut self, task: &AttachmentTask) -> Result<HeadPlan, Error> {
        if let Some(cached) = self.head_memo.get(task) {
            return Ok(cached.clone());
        }
        let head = self.compute_head(task)?;
        self.head_memo.insert(task.clone(), head.clone());
        Ok(head)
    }

    fn compute_head(&mut self, task: &AttachmentTask) -> Result<HeadPlan, Error> {
        let children = self.direct_children(task.subject_id, &task.edge_ids);
        if children.is_empty() {
            let draft = make_draft(task, BTreeSet::from([task.subject_id]), &[], None)?;
            if !self.evaluate(&draft)?.fits {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    format!("page subject does not fit: {}", task.subject_id),
                ));
            }
            return Ok(HeadPlan {
                base_node_ids: draft.node_ids.clone(),
                placed: Vec::new(),
                units: Vec::new(),
                overflow: None,
                direct_layout: None,
                direct_degraded: false,
                direct_draft: None,
            });
        }
        let mut kinds = HashMap::new();
        for child_id in &children {
            kinds.insert(*child_id, self.classify(task, *child_id)?);
        }
        let measured = self.measure_direct_cuts(task, &children, &kinds)?;
        let legal = legal_direct_cuts(&measured);
        if legal.is_empty() {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("direct family cannot fit: {}", task.subject_id),
            ));
        }
        let mut best_index = legal[0];
        let best_run = self.run_for_cut(&measured[best_index])?;
        let mut best_score = self.direct_run_score(&best_run);
        for index in legal.iter().skip(1).copied() {
            let run = self.run_for_cut(&measured[index])?;
            let score = self.direct_run_score(&run);
            if score < best_score {
                best_index = index;
                best_score = score;
            }
        }
        let best = &measured[best_index];
        let (layout, degraded) = accepted_layout(&best.fit)?;
        Ok(self.build_head(
            task,
            &best.retained,
            &kinds,
            best.overflow.clone(),
            DirectChoice {
                layout,
                degraded,
                draft: best.draft.clone(),
            },
        ))
    }

    fn measure_direct_cuts(
        &mut self,
        task: &AttachmentTask,
        children: &[NodeId],
        kinds: &HashMap<NodeId, ChildKind>,
    ) -> Result<Vec<MeasuredCut>, Error> {
        let mut eviction = children.to_vec();
        eviction.sort_by(|left, right| {
            kinds[left]
                .eviction_rank()
                .cmp(&kinds[right].eviction_rank())
                .then(self.rank(*right).cmp(&self.rank(*left)))
                .then(left.cmp(right))
        });
        let mut kept: BTreeSet<NodeId> = children.iter().copied().collect();
        let mut measured = Vec::new();
        let mut victims: VecDeque<Option<NodeId>> = VecDeque::new();
        victims.push_back(None);
        victims.extend(eviction.into_iter().map(Some));
        for victim in victims {
            if let Some(victim) = victim {
                if kept.len() <= 1 {
                    break;
                }
                kept.remove(&victim);
            }
            let moved: Vec<NodeId> = children
                .iter()
                .filter(|id| !kept.contains(*id))
                .copied()
                .collect();
            let overflow = self.overflow_cut(task, &moved);
            let retained: Vec<NodeId> = children
                .iter()
                .filter(|id| kept.contains(*id))
                .copied()
                .collect();
            let draft = head_draft(self, task, &retained, overflow.as_ref())?;
            let fit = self.evaluate_direct(&draft)?;
            measured.push(MeasuredCut {
                retained,
                overflow,
                draft,
                fit,
            });
        }
        Ok(measured)
    }

    fn overflow_cut(&self, task: &AttachmentTask, moved: &[NodeId]) -> Option<OverflowCut> {
        if moved.is_empty() {
            return None;
        }
        let mut edge_ids = BTreeSet::new();
        for child_id in moved {
            edge_ids.extend(self.reachable_edges(*child_id, &task.edge_ids));
        }
        let moved_set: BTreeSet<NodeId> = moved.iter().copied().collect();
        for edge_id in self.direct_edges(task.subject_id, &task.edge_ids) {
            if moved_set.contains(&self.edge_by_id[&edge_id].child_id) {
                edge_ids.insert(edge_id);
            }
        }
        let node = OverflowNode {
            subject_id: task.subject_id,
            first_moved_id: moved[0],
            moved_count: moved.len(),
        };
        Some(OverflowCut {
            node,
            task: AttachmentTask {
                subject_id: task.subject_id,
                edge_ids,
                kind: TaskKind::Continuation,
            },
        })
    }

    fn run_for_cut(&mut self, cut: &MeasuredCut) -> Result<Vec<ChainPage>, Error> {
        let (layout, degraded) = accepted_layout(&cut.fit)?;
        let head = ChainPage {
            retained_count: cut.draft.node_ids.len().saturating_sub(1),
            layout,
            degraded,
            node_ids: cut.draft.node_ids.clone(),
        };
        let Some(overflow) = &cut.overflow else {
            return Ok(vec![head]);
        };
        let rest = self.chosen_direct_run(&overflow.task)?;
        let mut pages = vec![head];
        pages.extend(rest);
        Ok(pages)
    }

    fn chosen_direct_run(&mut self, task: &AttachmentTask) -> Result<Vec<ChainPage>, Error> {
        if let Some(cached) = self.run_memo.get(task) {
            return Ok(cached.clone());
        }
        let head = self.plan_head(task)?;
        let (Some(layout), Some(draft)) = (head.direct_layout.clone(), head.direct_draft.clone())
        else {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("direct family cannot fit: {}", task.subject_id),
            ));
        };
        let page = ChainPage {
            retained_count: draft.node_ids.len().saturating_sub(1),
            layout,
            degraded: head.direct_degraded,
            node_ids: draft.node_ids,
        };
        let run = if let Some(overflow) = &head.overflow {
            let mut pages = vec![page];
            pages.extend(self.chosen_direct_run(&overflow.task)?);
            pages
        } else {
            vec![page]
        };
        self.run_memo.insert(task.clone(), run.clone());
        Ok(run)
    }

    fn direct_run_score(&self, run: &[ChainPage]) -> OverflowScore {
        let (sparse_flag, sparse_util) = if run.len() < 2 {
            (0, 0.0)
        } else {
            let continuation = &run[1].layout;
            let utilization = if continuation.printable_height_mm > 0.0 {
                continuation.height_mm / continuation.printable_height_mm
            } else {
                0.0
            };
            if utilization < SPARSE_UTILIZATION {
                (1, -utilization)
            } else {
                (0, 0.0)
            }
        };
        let utilizations: Vec<f64> = run
            .iter()
            .map(|page| page_utilization(&page.layout))
            .collect();
        let spread = utilizations
            .iter()
            .copied()
            .fold(None, |acc: Option<(f64, f64)>, value| {
                Some(match acc {
                    None => (value, value),
                    Some((lo, hi)) => (lo.min(value), hi.max(value)),
                })
            })
            .map_or(0.0, |(lo, hi)| hi - lo);
        let mut input_order = Vec::new();
        for page in run {
            for node_id in self.sorted_nodes(&page.node_ids) {
                input_order.push(self.rank(node_id));
            }
        }
        OverflowScore {
            page_count: run.len(),
            degraded_pages: run.iter().filter(|page| page.degraded).count(),
            sparse_flag,
            sparse_util,
            front_retention: run
                .iter()
                .map(|page| -i32::try_from(page.retained_count).unwrap_or(i32::MAX))
                .collect(),
            utilization_spread: spread,
            input_order,
        }
    }

    fn build_head(
        &self,
        task: &AttachmentTask,
        retained: &[NodeId],
        kinds: &HashMap<NodeId, ChildKind>,
        overflow: Option<OverflowCut>,
        direct: DirectChoice,
    ) -> HeadPlan {
        let mut placed = Vec::new();
        for child_id in retained {
            let cone = self.reachable_edges(*child_id, &task.edge_ids);
            if cone.is_empty() {
                placed.push(PlacedChild::Leaf);
            } else if kinds[child_id] == ChildKind::LargeFamily {
                placed.push(PlacedChild::Stub(*child_id));
            } else {
                placed.push(PlacedChild::Inlined {
                    child_id: *child_id,
                    edge_ids: cone.clone(),
                    node_ids: self.nodes_below(&cone),
                });
            }
        }
        let mut units = Vec::new();
        for child in &placed {
            if let PlacedChild::Inlined {
                child_id,
                edge_ids,
                node_ids,
            } = child
            {
                units.push((*child_id, edge_ids.clone(), node_ids.clone()));
            }
        }
        units.sort_by(|left, right| {
            left.1
                .len()
                .cmp(&right.1.len())
                .then(self.rank(left.0).cmp(&self.rank(right.0)))
        });
        let mut base_node_ids = BTreeSet::from([task.subject_id]);
        base_node_ids.extend(retained.iter().copied());
        HeadPlan {
            base_node_ids,
            placed,
            units,
            overflow,
            direct_layout: Some(direct.layout),
            direct_degraded: direct.degraded,
            direct_draft: Some(direct.draft),
        }
    }
}

fn legal_direct_cuts(measured: &[MeasuredCut]) -> Vec<usize> {
    let comfortable: Vec<usize> = measured
        .iter()
        .enumerate()
        .filter(|(_, cut)| matches!(cut.fit, DirectFit::Comfortable(_)))
        .map(|(index, _)| index)
        .collect();
    if !comfortable.is_empty() {
        return comfortable;
    }
    measured
        .iter()
        .enumerate()
        .filter(|(_, cut)| matches!(cut.fit, DirectFit::PhysicalOnly(_)))
        .map(|(index, _)| index)
        .collect()
}

fn accepted_layout(fit: &DirectFit) -> Result<(LayoutEvaluation, bool), Error> {
    match fit {
        DirectFit::Comfortable(layout) => Ok((layout.clone(), false)),
        DirectFit::PhysicalOnly(layout) => Ok((layout.clone(), true)),
        DirectFit::None => Err(Error::new(
            ErrorCode::LayoutFailed,
            "direct layer has no physical fit",
        )),
    }
}

fn head_draft<O: PageOracle>(
    context: &Context<'_, O>,
    task: &AttachmentTask,
    retained: &[NodeId],
    overflow: Option<&OverflowCut>,
) -> Result<PageDraft, Error> {
    let cuts: Vec<AttachmentCut> = retained
        .iter()
        .filter_map(|child_id| {
            let cone = context.reachable_edges(*child_id, &task.edge_ids);
            if cone.is_empty() {
                None
            } else {
                Some(AttachmentCut {
                    task: AttachmentTask {
                        subject_id: *child_id,
                        edge_ids: cone,
                        kind: TaskKind::Primary,
                    },
                })
            }
        })
        .collect();
    let mut node_ids = BTreeSet::from([task.subject_id]);
    node_ids.extend(retained.iter().copied());
    make_draft(task, node_ids, &cuts, overflow.map(|cut| cut.node.clone()))
}

pub(super) fn over_elastic(layout: &LayoutEvaluation) -> bool {
    layout.height_mm > layout.elastic_height_mm + FIT_EPSILON_MM
}
