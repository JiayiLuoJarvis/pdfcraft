//! Family-boundary page search. It chooses which nodes share a page. Placement is separate.

mod chain;
mod head;
mod inline;
mod score;
mod solve;

use std::collections::{BTreeSet, HashMap};

use crate::error::{Error, ErrorCode};
use crate::graph::{overflow_node_id, NodeId};
use crate::units::PageGeometry;

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

struct ChainPage {
    retained_count: usize,
    layout: LayoutEvaluation,
    degraded: bool,
    node_ids: BTreeSet<NodeId>,
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
