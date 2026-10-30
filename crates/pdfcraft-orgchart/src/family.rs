//! Family-boundary page search. It chooses which nodes share a page. Placement is separate.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, VecDeque};

use crate::error::{Error, ErrorCode};
use crate::graph::overflow_node_id;
use crate::ids::NodeId;
use crate::units::{PageGeometry, FIT_EPSILON_MM};

const MAX_SEARCH_EXPANSIONS: usize = 8192;
const SPARSE_UTILIZATION: f64 = 0.35;
const UTILIZATION_BUCKET: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum TaskKind {
    Primary,
    Continuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildKind {
    Person,
    LeafCompany,
    SmallFamily,
    LargeFamily,
}

impl ChildKind {
    fn eviction_rank(self) -> u8 {
        match self {
            Self::Person => 0,
            Self::LeafCompany => 1,
            Self::SmallFamily => 2,
            Self::LargeFamily => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FamilyEdge {
    pub(crate) id: usize,
    pub(crate) parent_id: NodeId,
    pub(crate) child_id: NodeId,
}

#[derive(Debug, Clone)]
pub(crate) struct FamilyGraph {
    pub(crate) subject_id: NodeId,
    pub(crate) edges: Vec<FamilyEdge>,
    pub(crate) node_order: Vec<NodeId>,
    pub(crate) person_ids: BTreeSet<NodeId>,
}

#[derive(Debug, Clone)]
pub(crate) struct LayoutEvaluation {
    pub(crate) fits: bool,
    pub(crate) height_mm: f64,
    pub(crate) printable_height_mm: f64,
    pub(crate) elastic_height_mm: f64,
    pub(crate) orientation_violation: bool,
    pub(crate) geometry: Option<PageGeometry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AttachmentTask {
    pub(crate) subject_id: NodeId,
    pub(crate) edge_ids: BTreeSet<usize>,
    pub(crate) kind: TaskKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct OverflowNode {
    pub(crate) subject_id: NodeId,
    pub(crate) first_moved_id: NodeId,
    pub(crate) moved_count: usize,
}

impl OverflowNode {
    pub(crate) fn protocol_id(&self, protocol: impl Fn(NodeId) -> String) -> String {
        overflow_node_id(&protocol(self.subject_id), &protocol(self.first_moved_id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct OverflowCut {
    pub(crate) node: OverflowNode,
    pub(crate) task: AttachmentTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PageDraft {
    pub(crate) subject_id: NodeId,
    pub(crate) node_ids: BTreeSet<NodeId>,
    pub(crate) stub_ids: BTreeSet<NodeId>,
    pub(crate) overflow: Option<OverflowNode>,
    pub(crate) kind: TaskKind,
}

impl PageDraft {
    pub(crate) fn check(&self) -> Result<(), Error> {
        if !self.node_ids.contains(&self.subject_id) {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("page draft subject {} is not on the page", self.subject_id),
            ));
        }
        if !self.stub_ids.is_subset(&self.node_ids) {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!(
                    "page draft stubs {:?} are not all on the page",
                    self.stub_ids
                ),
            ));
        }
        if let Some(overflow) = &self.overflow {
            if overflow.subject_id != self.subject_id {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    format!(
                        "page draft overflow subject {} does not match {}",
                        overflow.subject_id, self.subject_id
                    ),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) enum DirectFit {
    Comfortable(LayoutEvaluation),
    PhysicalOnly(LayoutEvaluation),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AttachmentCut {
    pub(crate) task: AttachmentTask,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchPage {
    pub(crate) task: AttachmentTask,
    pub(crate) draft: PageDraft,
    pub(crate) attachment_cuts: Vec<AttachmentCut>,
    pub(crate) overflow: Option<OverflowCut>,
    pub(crate) layout: LayoutEvaluation,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchPlan {
    pub(crate) pages: Vec<SearchPage>,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchResult {
    pub(crate) plan: SearchPlan,
}

#[derive(Clone)]
enum PlacedChild {
    Stub(NodeId),
    Leaf,
    Inlined {
        child_id: NodeId,
        edge_ids: BTreeSet<usize>,
        node_ids: BTreeSet<NodeId>,
    },
}

#[derive(Clone)]
struct HeadPlan {
    base_node_ids: BTreeSet<NodeId>,
    placed: Vec<PlacedChild>,
    units: Vec<(NodeId, BTreeSet<usize>, BTreeSet<NodeId>)>,
    overflow: Option<OverflowCut>,
    direct_layout: Option<LayoutEvaluation>,
    direct_degraded: bool,
    direct_draft: Option<PageDraft>,
}

#[derive(Clone)]
struct PlanScore {
    orientation_violations: usize,
    sparse_risk: usize,
    attachment_cuts: usize,
    repeated_context: usize,
    page_count: usize,
    input_order: Vec<Vec<usize>>,
}

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

#[derive(Clone)]
struct OverflowScore {
    page_count: usize,
    degraded_pages: usize,
    sparse_flag: usize,
    sparse_util: f64,
    front_retention: Vec<i32>,
    utilization_spread: f64,
    input_order: Vec<usize>,
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

struct ChainPage {
    retained_count: usize,
    layout: LayoutEvaluation,
    degraded: bool,
    node_ids: BTreeSet<NodeId>,
}

struct MeasuredCut {
    retained: Vec<NodeId>,
    overflow: Option<OverflowCut>,
    draft: PageDraft,
    fit: DirectFit,
}

pub(crate) trait PageOracle {
    fn evaluate(&mut self, draft: &PageDraft) -> Result<LayoutEvaluation, Error>;
    fn evaluate_direct(&mut self, draft: &PageDraft) -> Result<DirectFit, Error>;
}

struct Context<'a, O: PageOracle> {
    graph: &'a FamilyGraph,
    oracle: &'a mut O,
    max_expansions: usize,
    expansions: usize,
    budget_exhausted: bool,
    edge_by_id: HashMap<usize, FamilyEdge>,
    outgoing: HashMap<NodeId, Vec<usize>>,
    order_index: HashMap<NodeId, usize>,
    layout_cache: HashMap<PageDraft, LayoutEvaluation>,
    direct_cache: HashMap<PageDraft, DirectFit>,
    /// Search plans live here. Memo maps store indices into this arena.
    plans: Vec<SearchPlan>,
    memo: HashMap<(AttachmentTask, BTreeSet<NodeId>), usize>,
    head_memo: HashMap<AttachmentTask, HeadPlan>,
    greedy_memo: HashMap<AttachmentTask, usize>,
    run_memo: HashMap<AttachmentTask, Vec<ChainPage>>,
}

pub(crate) fn search_family_pages(
    graph: &FamilyGraph,
    oracle: &mut impl PageOracle,
) -> Result<SearchResult, Error> {
    let mut context = Context::new(graph, oracle, MAX_SEARCH_EXPANSIONS);
    let root = AttachmentTask {
        subject_id: graph.subject_id,
        edge_ids: graph.edges.iter().map(|edge| edge.id).collect(),
        kind: TaskKind::Primary,
    };
    let plan = context.solve_task(&root, &BTreeSet::new())?;
    Ok(SearchResult { plan })
}

impl<'a, O: PageOracle> Context<'a, O> {
    fn new(graph: &'a FamilyGraph, oracle: &'a mut O, max_expansions: usize) -> Self {
        let mut outgoing: HashMap<NodeId, Vec<usize>> = HashMap::new();
        let mut edge_by_id = HashMap::new();
        for edge in &graph.edges {
            outgoing.entry(edge.parent_id).or_default().push(edge.id);
            edge_by_id.insert(edge.id, edge.clone());
        }
        let order_index = graph
            .node_order
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, index))
            .collect();
        Self {
            graph,
            oracle,
            max_expansions: max_expansions.max(1),
            expansions: 0,
            budget_exhausted: false,
            edge_by_id,
            outgoing,
            order_index,
            layout_cache: HashMap::new(),
            direct_cache: HashMap::new(),
            plans: Vec::new(),
            memo: HashMap::new(),
            head_memo: HashMap::new(),
            greedy_memo: HashMap::new(),
            run_memo: HashMap::new(),
        }
    }

    fn store_plan(&mut self, plan: SearchPlan) -> usize {
        let index = self.plans.len();
        self.plans.push(plan);
        index
    }

    fn plan_at(&self, index: usize) -> SearchPlan {
        self.plans[index].clone()
    }

    fn evaluate(&mut self, draft: &PageDraft) -> Result<LayoutEvaluation, Error> {
        if let Some(cached) = self.layout_cache.get(draft) {
            return Ok(cached.clone());
        }
        let evaluated = self.oracle.evaluate(draft)?;
        self.layout_cache.insert(draft.clone(), evaluated.clone());
        Ok(evaluated)
    }

    fn evaluate_direct(&mut self, draft: &PageDraft) -> Result<DirectFit, Error> {
        if let Some(cached) = self.direct_cache.get(draft) {
            return Ok(cached.clone());
        }
        let evaluated = self.oracle.evaluate_direct(draft)?;
        self.direct_cache.insert(draft.clone(), evaluated.clone());
        Ok(evaluated)
    }

    fn consume_expansion(&mut self) -> bool {
        if self.expansions >= self.max_expansions {
            self.budget_exhausted = true;
            return false;
        }
        self.expansions += 1;
        true
    }

    fn rank(&self, node_id: NodeId) -> usize {
        self.order_index
            .get(&node_id)
            .copied()
            .unwrap_or(self.order_index.len())
    }

    fn sorted_nodes(&self, node_ids: &BTreeSet<NodeId>) -> Vec<NodeId> {
        let mut ids: Vec<NodeId> = node_ids.iter().copied().collect();
        ids.sort_by(|left, right| {
            self.rank(*left)
                .cmp(&self.rank(*right))
                .then(left.cmp(right))
        });
        ids
    }

    fn direct_children(&self, parent_id: NodeId, allowed: &BTreeSet<usize>) -> Vec<NodeId> {
        let mut children = Vec::new();
        if let Some(edges) = self.outgoing.get(&parent_id) {
            for edge_id in edges {
                if allowed.contains(edge_id) {
                    children.push(self.edge_by_id[edge_id].child_id);
                }
            }
        }
        children.sort_by(|left, right| {
            self.rank(*left)
                .cmp(&self.rank(*right))
                .then(left.cmp(right))
        });
        children.dedup();
        children
    }

    fn direct_edges(&self, parent_id: NodeId, allowed: &BTreeSet<usize>) -> BTreeSet<usize> {
        self.outgoing
            .get(&parent_id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|edge_id| allowed.contains(edge_id))
            .collect()
    }

    fn reachable_edges(&self, subject_id: NodeId, allowed: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut found = BTreeSet::new();
        let mut stack = vec![subject_id];
        let mut seen = BTreeSet::new();
        while let Some(parent_id) = stack.pop() {
            if !seen.insert(parent_id) {
                continue;
            }
            if let Some(edges) = self.outgoing.get(&parent_id) {
                for edge_id in edges {
                    if !allowed.contains(edge_id) {
                        continue;
                    }
                    found.insert(*edge_id);
                    stack.push(self.edge_by_id[edge_id].child_id);
                }
            }
        }
        found
    }

    fn nodes_below(&self, edge_ids: &BTreeSet<usize>) -> BTreeSet<NodeId> {
        edge_ids
            .iter()
            .map(|edge_id| self.edge_by_id[edge_id].child_id)
            .collect()
    }

    fn solve_task(
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

    fn greedy_plan(&mut self, task: &AttachmentTask) -> Result<SearchPlan, Error> {
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

    fn plan_head(&mut self, task: &AttachmentTask) -> Result<HeadPlan, Error> {
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
            layout,
            degraded,
            best.draft.clone(),
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
        direct_layout: LayoutEvaluation,
        direct_degraded: bool,
        direct_draft: PageDraft,
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
            direct_layout: Some(direct_layout),
            direct_degraded,
            direct_draft: Some(direct_draft),
        }
    }

    fn classify(&mut self, task: &AttachmentTask, child_id: NodeId) -> Result<ChildKind, Error> {
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

    fn page_for(
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

    fn enumerate_inlines(
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

    fn chain_path(&self, task: &AttachmentTask) -> Option<(Vec<NodeId>, Vec<usize>)> {
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

    fn solve_chain(
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

    fn score(&mut self, plan: &SearchPlan) -> Result<PlanScore, Error> {
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

fn make_draft(
    task: &AttachmentTask,
    node_ids: BTreeSet<NodeId>,
    cuts: &[AttachmentCut],
    overflow: Option<OverflowNode>,
) -> Result<PageDraft, Error> {
    let draft = PageDraft {
        subject_id: task.subject_id,
        node_ids,
        stub_ids: cuts.iter().map(|cut| cut.task.subject_id).collect(),
        overflow,
        kind: task.kind,
    };
    draft.check()?;
    Ok(draft)
}

fn over_elastic(layout: &LayoutEvaluation) -> bool {
    layout.height_mm > layout.elastic_height_mm + FIT_EPSILON_MM
}

fn page_utilization(layout: &LayoutEvaluation) -> f64 {
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

impl Clone for ChainPage {
    fn clone(&self) -> Self {
        Self {
            retained_count: self.retained_count,
            layout: self.layout.clone(),
            degraded: self.degraded,
            node_ids: self.node_ids.clone(),
        }
    }
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
