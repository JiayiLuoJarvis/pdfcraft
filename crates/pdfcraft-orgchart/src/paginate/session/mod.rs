//! Split a validated chart into pages. Shareholders come before investments.

mod assemble;
mod fit;
mod titles;

use assemble::investment_id_refs;

use std::collections::{BTreeSet, HashMap, HashSet};

use super::cues::{direct_count, finalize_pages, PendingCue, PendingLink, UnnumberedPage};
use super::family::{
    search_family_pages, AttachmentTask, DirectFit, PageDraft, SearchPage, TaskKind,
};
use crate::captions::{
    both_cones_chart_title, equity_chart_title, investment_page_titles, whole_chart_subtitle,
    CueSide,
};
use crate::content::{self, RatioFact};
use crate::error::{Error, ErrorCode};
use crate::font::Measurer;
use crate::graph::{Graph, NodeId};
use crate::layout::{LayoutEngine, WrappedEngine};
use crate::page::{EdgeRef, LayoutResult, NodeBox, PageSpec};
#[cfg(test)]
use crate::request::ChartRequest;
use crate::units::{within, PageGeometry, NODE_WIDTH_MM};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Shareholder,
    Investment,
}

impl Side {
    fn cue_side(self) -> CueSide {
        match self {
            Self::Shareholder => CueSide::Shareholder,
            Self::Investment => CueSide::Investment,
        }
    }

    fn page_edge(self, parent_id: NodeId, child_id: NodeId) -> EdgeRef {
        match self {
            Self::Shareholder => EdgeRef::new(child_id, parent_id),
            Self::Investment => EdgeRef::new(parent_id, child_id),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PageKey {
    draft: PageDraft,
    side: Option<Side>,
    overview_cue: bool,
}

type GeometryCache = HashMap<(PageKey, (u64, u64, u64, u64, u64)), PackedLayout>;

#[derive(Clone)]
struct PackedLayout {
    boxes: HashMap<NodeId, NodeBox>,
    layout: LayoutResult,
    geometry: PageGeometry,
    physical_fits: bool,
    max_nondirect_rows: usize,
    orientation_violation: bool,
}

impl PackedLayout {
    fn fits(&self) -> bool {
        self.physical_fits && self.max_nondirect_rows <= 1
    }
}

struct SidePack {
    pages: Vec<UnnumberedPage>,
    cues: Vec<PendingCue>,
    links: Vec<PendingLink>,
    direct_ids: Vec<NodeId>,
}

struct Session<'a, E: LayoutEngine, M: Measurer> {
    graph: Graph,
    boxes: HashMap<NodeId, NodeBox>,
    geometry: PageGeometry,
    node_order: Vec<NodeId>,
    engine: &'a E,
    measurer: &'a mut M,
    subject_node: NodeId,
    subject_name: String,
    has_investments: bool,
    whole_main: String,
    whole_sub: String,
    node_width: f64,
    order_index: HashMap<NodeId, usize>,
    shareholder_continue: usize,
    investment_continue: usize,
    overflow_boxes: HashMap<NodeId, NodeBox>,
    chosen_cache: HashMap<PageKey, PackedLayout>,
    direct_cache: HashMap<PageKey, DirectFit>,
    geometry_cache: GeometryCache,
    ratio_facts: Vec<RatioFact>,
}

/// Pack `boxes` with the production engine and the caller's measurer.
///
/// # Errors
///
/// Returns [`Error`] when the request is illegal or no page plan fits.
pub(crate) fn paginate<M: Measurer>(
    graph: Graph,
    boxes: &[NodeBox],
    geometry: PageGeometry,
    measurer: &mut M,
) -> Result<Vec<PageSpec>, Error> {
    paginate_with(graph, boxes, geometry, &WrappedEngine, measurer)
}

/// Pack `boxes` with a caller-supplied engine and measurer.
///
/// # Errors
///
/// Returns [`Error`] when the request is illegal or no page plan fits.
pub(crate) fn paginate_with<E: LayoutEngine, M: Measurer>(
    graph: Graph,
    boxes: &[NodeBox],
    geometry: PageGeometry,
    engine: &E,
    measurer: &mut M,
) -> Result<Vec<PageSpec>, Error> {
    let mut box_map = HashMap::new();
    for box_ in boxes {
        box_map.insert(box_.id, box_.clone());
    }
    Session::new(graph, box_map, geometry, engine, measurer)?.run()
}

/// Test entry that validates `request` then paginates.
#[cfg(test)]
pub(crate) fn paginate_request<M: Measurer>(
    request: &ChartRequest,
    boxes: &[NodeBox],
    geometry: PageGeometry,
    measurer: &mut M,
) -> Result<Vec<PageSpec>, Error> {
    let graph = crate::graph::validate(request)?;
    paginate(graph, boxes, geometry, measurer)
}

/// Test entry with a caller-supplied engine.
#[cfg(test)]
pub(crate) fn paginate_request_with<E: LayoutEngine, M: Measurer>(
    request: &ChartRequest,
    boxes: &[NodeBox],
    geometry: PageGeometry,
    engine: &E,
    measurer: &mut M,
) -> Result<Vec<PageSpec>, Error> {
    let graph = crate::graph::validate(request)?;
    paginate_with(graph, boxes, geometry, engine, measurer)
}

impl<'a, E: LayoutEngine, M: Measurer> Session<'a, E, M> {
    fn new(
        mut graph: Graph,
        boxes: HashMap<NodeId, NodeBox>,
        geometry: PageGeometry,
        engine: &'a E,
        measurer: &'a mut M,
    ) -> Result<Self, Error> {
        let subject_node = graph.subject;
        let subject_name = graph.node_name(subject_node).to_owned();
        let has_shareholders = !graph.incoming_sources(subject_node).is_empty();
        let has_investments = !graph.outgoing_targets(subject_node).is_empty();
        // Whole-page chrome follows cone shape.
        let (whole_main, whole_sub) = match (has_shareholders, has_investments) {
            (false, true) => investment_page_titles(&subject_name, 1),
            (true, true) => (
                both_cones_chart_title(&subject_name),
                whole_chart_subtitle(&subject_name, true),
            ),
            (_, _) => (
                equity_chart_title(&subject_name),
                whole_chart_subtitle(&subject_name, has_investments),
            ),
        };
        let node_width = boxes
            .values()
            .find(|box_| !box_.is_subject)
            .map_or(NODE_WIDTH_MM, |box_| box_.width_mm);
        let node_order: Vec<NodeId> = graph.nodes.iter().map(|node| node.id).collect();
        let order_index = node_order
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, index))
            .collect();
        let ratio_facts = content::facts_of(&mut graph)?;
        Ok(Self {
            graph,
            boxes,
            geometry,
            node_order,
            engine,
            measurer,
            subject_node,
            subject_name,
            has_investments,
            whole_main,
            whole_sub,
            node_width,
            order_index,
            shareholder_continue: 0,
            investment_continue: 0,
            overflow_boxes: HashMap::new(),
            chosen_cache: HashMap::new(),
            direct_cache: HashMap::new(),
            geometry_cache: HashMap::new(),
            ratio_facts,
        })
    }

    fn run(mut self) -> Result<Vec<PageSpec>, Error> {
        let shareholders = self.sorted(self.graph.incoming_sources(self.subject_node));
        let investments = self.sorted(self.graph.outgoing_targets(self.subject_node));
        let all_ids: BTreeSet<NodeId> = self.boxes.keys().copied().collect();
        let whole_key = self.standalone_key(all_ids);
        if let Some(whole) = self.whole_page_if_elastic(&whole_key)? {
            return Ok(vec![self.to_page(&whole)?]);
        }
        let shareholder = self.pack_side(&shareholders, Side::Shareholder)?;
        let investment = self.pack_side(&investments, Side::Investment)?;
        let shareholder_nonempty = !shareholder.pages.is_empty();
        let mut pages = shareholder.pages;
        let offset = pages.len();
        pages.extend(investment.pages);
        if pages.is_empty() {
            let only_key = self.standalone_key(BTreeSet::from([self.subject_node]));
            let only = self.layout_pack(&only_key)?;
            return Ok(vec![self.to_page(&only)?]);
        }
        let mut cues = shareholder.cues;
        for cue in investment.cues {
            cues.push(PendingCue {
                source_index: cue.source_index + offset,
                target_index: cue.target_index + offset,
                ..cue
            });
        }
        let mut links = shareholder.links;
        for link in investment.links {
            links.push(PendingLink {
                source_index: link.source_index + offset,
                target_index: link.target_index + offset,
                ..link
            });
        }
        if shareholder_nonempty && offset < pages.len() {
            let refs = investment_id_refs(&investment.direct_ids, &pages[offset].boxes);
            let count = direct_count(&pages[offset].boxes, &refs).max(1);
            cues.push(PendingCue {
                source_index: 0,
                target_index: offset,
                direct_count: count,
                side: CueSide::Investment,
                anchor_id: self.subject_node,
            });
        }
        finalize_pages(&pages, &cues, &links, self.measurer)
    }

    fn pack_side(&mut self, direct_ids: &[NodeId], side: Side) -> Result<SidePack, Error> {
        if direct_ids.is_empty() {
            return Ok(SidePack {
                pages: Vec::new(),
                cues: Vec::new(),
                links: Vec::new(),
                direct_ids: Vec::new(),
            });
        }
        let family = self.family_graph(direct_ids, side);
        let overview = side == Side::Shareholder && self.has_investments;
        let result = {
            let mut oracle = SideOracle {
                session: self,
                side,
                overview,
            };
            search_family_pages(&family, &mut oracle)?
        };
        if result.plan.pages.is_empty() {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                "family search returned no pages",
            ));
        }
        let mut pages = Vec::new();
        for page in &result.plan.pages {
            pages.push(self.search_page_to_spec(page, side, overview)?);
        }
        let mut first_by_task = HashMap::new();
        for (index, page) in result.plan.pages.iter().enumerate().rev() {
            first_by_task.entry(task_key(&page.task)).or_insert(index);
        }
        let mut cues = Vec::new();
        let mut links = Vec::new();
        for (source_index, page) in result.plan.pages.iter().enumerate() {
            for cut in &page.attachment_cuts {
                let target = task_index(&first_by_task, &cut.task, source_index)?;
                cues.push(PendingCue {
                    source_index,
                    target_index: target,
                    direct_count: 1,
                    side: side.cue_side(),
                    anchor_id: cut.task.subject_id,
                });
            }
            if let Some(overflow) = &page.overflow {
                let target = task_index(&first_by_task, &overflow.task, source_index)?;
                links.push(PendingLink {
                    source_index,
                    target_index: target,
                    node_id: self.overflow_for(&overflow.node, side)?.id,
                });
            }
        }
        Ok(SidePack {
            pages,
            cues,
            links,
            direct_ids: direct_ids.to_vec(),
        })
    }

    fn search_page_to_spec(
        &mut self,
        page: &SearchPage,
        side: Side,
        overview: bool,
    ) -> Result<UnnumberedPage, Error> {
        let geometry = page.layout.geometry.ok_or_else(|| {
            Error::new(
                ErrorCode::LayoutFailed,
                format!(
                    "search page missing geometry subject={}",
                    page.draft.subject_id
                ),
            )
        })?;
        let key = self.key(&page.draft, Some(side), overview);
        let packed = self.layout_at(&key, geometry)?;
        if page.draft.subject_id == self.subject_node {
            let (main, sub) = match side {
                Side::Shareholder => self.next_shareholder_titles(),
                Side::Investment => self.next_investment_titles(),
            };
            self.unnumbered(&packed, main, sub, None)
        } else {
            let subject = page.draft.subject_id;
            let sub = self.cut_caption(subject)?;
            let name = self.node_name(subject);
            self.unnumbered(&packed, self.whole_main.clone(), sub, Some(name))
        }
    }

    fn materialize(&mut self, key: &PageKey) -> Result<PageContent, Error> {
        let draft = &key.draft;
        let root = self.layout_root(&draft.node_ids, draft.subject_id);
        let mut boxes = self.boxes_for_page(&draft.node_ids, root)?;
        let mut edges: Vec<EdgeRef> = self
            .graph
            .edges
            .iter()
            .filter_map(|edge| {
                if draft.node_ids.contains(&edge.from) && draft.node_ids.contains(&edge.to) {
                    Some(EdgeRef::new(edge.from, edge.to))
                } else {
                    None
                }
            })
            .collect();
        let mut order: Vec<NodeId> = self
            .node_order
            .iter()
            .copied()
            .filter(|id| draft.node_ids.contains(id))
            .collect();
        let mut anchors = Vec::new();
        if let Some(side) = key.side {
            for stub_id in &draft.stub_ids {
                anchors.push((*stub_id, side.cue_side()));
            }
            if let Some(overflow) = &draft.overflow {
                let box_ = self.overflow_for(overflow, side)?;
                let id = box_.id;
                boxes.insert(id, box_);
                edges.push(side.page_edge(draft.subject_id, id));
                order.push(id);
            }
        }
        if key.overview_cue {
            anchors.push((draft.subject_id, CueSide::Investment));
        }
        Ok(PageContent {
            boxes,
            edges,
            order,
            anchors,
        })
    }

    fn unnumbered(
        &self,
        packed: &PackedLayout,
        main_title: String,
        sub_title: String,
        penetration_name: Option<String>,
    ) -> Result<UnnumberedPage, Error> {
        if !packed.fits() {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!(
                    "page does not fit sub_title={sub_title} width_mm={} height_mm={}",
                    packed.layout.width_mm, packed.layout.height_mm
                ),
            ));
        }
        let boxes: Vec<NodeBox> = packed.boxes.values().cloned().collect();
        let ids: HashSet<NodeId> = packed.boxes.keys().copied().collect();
        let ratio_facts = self
            .ratio_facts
            .iter()
            .filter(|fact| ids.contains(&fact.node_id))
            .cloned()
            .collect();
        Ok(UnnumberedPage {
            main_title,
            sub_title,
            boxes,
            layout: packed.layout.clone(),
            geometry: packed.geometry,
            penetration_name,
            ratio_facts,
        })
    }

    fn to_page(&self, packed: &PackedLayout) -> Result<PageSpec, Error> {
        let page = self.unnumbered(
            packed,
            self.whole_main.clone(),
            self.whole_sub.clone(),
            None,
        )?;
        let spec = PageSpec {
            main_title: page.main_title,
            sub_title: page.sub_title,
            boxes: page.boxes,
            layout: page.layout,
            geometry: page.geometry,
            cont_cues: Vec::new(),
            node_page_links: Vec::new(),
            ratio_facts: page.ratio_facts,
        };
        spec.check()?;
        Ok(spec)
    }

    fn sorted(&self, ids: Vec<NodeId>) -> Vec<NodeId> {
        let mut owned = ids;
        owned.sort_by_key(|id| self.order_index.get(id).copied().unwrap_or(usize::MAX));
        owned.dedup();
        owned
    }
}

struct PageContent {
    boxes: HashMap<NodeId, NodeBox>,
    edges: Vec<EdgeRef>,
    order: Vec<NodeId>,
    anchors: Vec<(NodeId, CueSide)>,
}

#[derive(Clone, Copy)]
enum Admit {
    Comfort,
    Elastic,
}

impl Admit {
    fn accepts(self, packed: &PackedLayout) -> bool {
        let limit = match self {
            Self::Comfort => packed.geometry.comfort_height_mm(),
            Self::Elastic => packed.geometry.elastic_height_mm(),
        };
        within(packed.layout.height_mm, limit)
    }
}

fn has_fork(edges: &[EdgeRef]) -> bool {
    let mut degree: HashMap<(NodeId, bool), usize> = HashMap::new();
    for edge in edges {
        for (node_id, outward) in [(edge.from_id, true), (edge.to_id, false)] {
            let count = degree.entry((node_id, outward)).or_insert(0);
            *count += 1;
            if *count >= 2 {
                return true;
            }
        }
    }
    false
}

fn task_key(task: &AttachmentTask) -> (NodeId, BTreeSet<usize>, TaskKind) {
    (task.subject_id, task.edge_ids.clone(), task.kind)
}

fn task_index(
    first_by_task: &HashMap<(NodeId, BTreeSet<usize>, TaskKind), usize>,
    task: &AttachmentTask,
    source_index: usize,
) -> Result<usize, Error> {
    let target = *first_by_task.get(&task_key(task)).ok_or_else(|| {
        Error::new(
            ErrorCode::LayoutFailed,
            format!("attachment target missing for {}", task.subject_id),
        )
    })?;
    if target <= source_index {
        return Err(Error::new(
            ErrorCode::LayoutFailed,
            format!("attachment target missing for {}", task.subject_id),
        ));
    }
    Ok(target)
}

struct SideOracle<'s, 'a, E: LayoutEngine, M: Measurer> {
    session: &'s mut Session<'a, E, M>,
    side: Side,
    overview: bool,
}
