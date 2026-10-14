//! Split a validated chart into pages. Shareholders come before investments.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::captions::{
    equity_chart_title, holds_relation, investment_page_titles, invests_relation,
    shareholder_page_titles, whole_chart_subtitle, CueSide,
};
use crate::cues::{
    absorb_cue_slots, direct_count, finalize_pages, CueAnchorSize, PendingCue, PendingLink,
    UnnumberedPage,
};
use crate::display::{self, RatioFact};
use crate::error::{Error, ErrorCode};
use crate::family::{
    search_family_pages, AttachmentTask, DirectFit, FamilyEdge, FamilyGraph, LayoutEvaluation,
    OverflowNode, PageDraft, PageOracle, SearchPage, TaskKind,
};
use crate::graph::Graph;
use crate::ids::NodeId;
use crate::layout::{LayoutEngine, WrappedEngine};
use crate::measure::{overflow_box, promote_to_page_subject, sized_cue_anchors, Measurer, NodeBox};
#[cfg(test)]
use crate::model::ChartRequest;
use crate::model::NodeType;
use crate::page::{EdgeRef, LayoutResult, PageSpec};
use crate::units::{within, PageGeometry, NODE_WIDTH_MM, ROW_GAP_MM};

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
    subject_protocol: String,
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
pub fn paginate<M: Measurer>(
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
pub fn paginate_with<E: LayoutEngine, M: Measurer>(
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
pub fn paginate_request<M: Measurer>(
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
pub fn paginate_request_with<E: LayoutEngine, M: Measurer>(
    request: &ChartRequest,
    boxes: &[NodeBox],
    geometry: PageGeometry,
    engine: &E,
    measurer: &mut M,
) -> Result<Vec<PageSpec>, Error> {
    let graph = crate::graph::validate(request)?;
    paginate_with(graph, boxes, geometry, engine, measurer)
}

/// Two millimetres per character. Paging tests inject this instead of a face.
#[cfg(test)]
#[derive(Debug, Default, Clone, Copy)]
pub struct FallbackMeasurer;

#[cfg(test)]
impl Measurer for FallbackMeasurer {
    fn string_width_mm(&mut self, text: &str, _size_pt: f64) -> Result<f64, Error> {
        Ok(crate::units::n64(text.chars().count()) * 2.0)
    }

    fn line_height_mm(&mut self, _size_pt: f64) -> f64 {
        5.0
    }

    fn ensure_glyphs(&mut self, _text: &str) -> Result<(), Error> {
        Ok(())
    }
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
        let subject_protocol = graph.subject_protocol_id.clone();
        let subject_name = graph.node_name(subject_node).to_owned();
        let has_investments = !graph.outgoing_targets(subject_node).is_empty();
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
        let ratio_facts = display::facts_of(&mut graph)?;
        Ok(Self {
            graph,
            boxes,
            geometry,
            node_order,
            engine,
            measurer,
            subject_protocol,
            subject_node,
            subject_name: subject_name.clone(),
            has_investments,
            whole_main: equity_chart_title(&subject_name),
            whole_sub: whole_chart_subtitle(&subject_name, has_investments),
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
                anchor_id: self.subject_protocol.clone(),
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
                    anchor_id: self.protocol(cut.task.subject_id).to_owned(),
                });
            }
            if let Some(overflow) = &page.overflow {
                let target = task_index(&first_by_task, &overflow.task, source_index)?;
                links.push(PendingLink {
                    source_index,
                    target_index: target,
                    node_id: overflow.node.protocol_id(|id| self.protocol(id).to_owned()),
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

    fn protocol(&self, id: NodeId) -> &str {
        self.graph.protocol_id(id)
    }

    fn family_graph(&self, direct_ids: &[NodeId], side: Side) -> FamilyGraph {
        let mut node_ids = HashSet::from([self.subject_node]);
        let mut stack: Vec<NodeId> = direct_ids.to_vec();
        while let Some(node_id) = stack.pop() {
            if !node_ids.insert(node_id) {
                continue;
            }
            let next = match side {
                Side::Shareholder => self.graph.incoming_sources(node_id),
                Side::Investment => self.graph.outgoing_targets(node_id),
            };
            stack.extend(next);
        }
        let mut edges = Vec::new();
        for edge in &self.graph.edges {
            let from_node = edge.from;
            let to_node = edge.to;
            if !node_ids.contains(&from_node) || !node_ids.contains(&to_node) {
                continue;
            }
            let (parent_id, child_id) = if side == Side::Shareholder {
                (to_node, from_node)
            } else {
                (from_node, to_node)
            };
            edges.push(FamilyEdge {
                id: edge.id.index(),
                parent_id,
                child_id,
            });
        }
        let person_ids = node_ids
            .iter()
            .copied()
            .filter(|id| {
                self.graph
                    .nodes
                    .get(id.index())
                    .is_some_and(|node| node.node_type == Some(NodeType::Person))
            })
            .collect();
        FamilyGraph {
            subject_id: self.subject_node,
            edges,
            node_order: self
                .node_order
                .iter()
                .copied()
                .filter(|id| node_ids.contains(id))
                .collect(),
            person_ids,
        }
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

    fn key(&self, draft: &PageDraft, side: Option<Side>, overview: bool) -> PageKey {
        PageKey {
            overview_cue: overview
                && side.is_some()
                && draft.kind == TaskKind::Primary
                && draft.subject_id == self.subject_node,
            draft: draft.clone(),
            side,
        }
    }

    fn standalone_key(&self, node_ids: BTreeSet<NodeId>) -> PageKey {
        PageKey {
            draft: PageDraft {
                subject_id: self.subject_node,
                node_ids,
                stub_ids: BTreeSet::new(),
                overflow: None,
                kind: TaskKind::Primary,
            },
            side: None,
            overview_cue: false,
        }
    }

    fn evaluate(&mut self, key: &PageKey) -> Result<LayoutEvaluation, Error> {
        let packed = self.layout_pack(key)?;
        Ok(Self::as_evaluation(&packed, packed.orientation_violation))
    }

    fn evaluate_direct(&mut self, key: &PageKey) -> Result<DirectFit, Error> {
        if let Some(cached) = self.direct_cache.get(key) {
            return Ok(cached.clone());
        }
        let chosen = self.choose_direct_layer(key)?;
        self.direct_cache.insert(key.clone(), chosen.clone());
        Ok(chosen)
    }

    fn layout_pack(&mut self, key: &PageKey) -> Result<PackedLayout, Error> {
        if let Some(cached) = self.chosen_cache.get(key) {
            return Ok(cached.clone());
        }
        let packed = self.choose_layout(key)?;
        self.chosen_cache.insert(key.clone(), packed.clone());
        Ok(packed)
    }

    fn choose_layout(&mut self, key: &PageKey) -> Result<PackedLayout, Error> {
        let (preferred, alternate) = self.orientation_pair(key)?;
        let preferred_pack = self.layout_at(key, preferred)?;
        if preferred_pack.fits() {
            return Ok(preferred_pack);
        }
        let mut alternate_pack = self.layout_at(key, alternate)?;
        if alternate_pack.fits() {
            alternate_pack.orientation_violation = true;
            return Ok(alternate_pack);
        }
        Ok(preferred_pack)
    }

    fn choose_direct_layer(&mut self, key: &PageKey) -> Result<DirectFit, Error> {
        let (preferred, alternate) = self.orientation_pair(key)?;
        let preferred_pack = self.layout_at(key, preferred)?;
        let alternate_pack = self.layout_at(key, alternate)?;
        for admit in [Admit::Comfort, Admit::Elastic] {
            for (pack, violation) in [(&preferred_pack, false), (&alternate_pack, true)] {
                if pack.fits() && admit.accepts(pack) {
                    return Ok(DirectFit::Comfortable(Self::as_evaluation(pack, violation)));
                }
            }
        }
        if preferred_pack.fits() {
            return Ok(DirectFit::PhysicalOnly(Self::as_evaluation(
                &preferred_pack,
                false,
            )));
        }
        if alternate_pack.fits() {
            return Ok(DirectFit::PhysicalOnly(Self::as_evaluation(
                &alternate_pack,
                true,
            )));
        }
        Ok(DirectFit::None)
    }

    fn whole_page_if_elastic(&mut self, key: &PageKey) -> Result<Option<PackedLayout>, Error> {
        let (preferred, alternate) = self.orientation_pair(key)?;
        let preferred_pack = self.layout_at(key, preferred)?;
        let alternate_pack = self.layout_at(key, alternate)?;
        for admit in [Admit::Comfort, Admit::Elastic] {
            for (mut pack, violation) in [
                (preferred_pack.clone(), false),
                (alternate_pack.clone(), true),
            ] {
                if !pack.fits() || !admit.accepts(&pack) {
                    continue;
                }
                if violation {
                    pack.orientation_violation = true;
                }
                return Ok(Some(pack));
            }
        }
        Ok(None)
    }

    fn orientation_pair(&mut self, key: &PageKey) -> Result<(PageGeometry, PageGeometry), Error> {
        let content = self.materialize(key)?;
        let portrait = self.geometry;
        let landscape = portrait.landscape();
        if has_fork(&content.edges) {
            Ok((landscape, portrait))
        } else {
            Ok((portrait, landscape))
        }
    }

    fn as_evaluation(packed: &PackedLayout, violation: bool) -> LayoutEvaluation {
        LayoutEvaluation {
            fits: packed.fits(),
            height_mm: packed.layout.height_mm,
            printable_height_mm: packed.geometry.printable_height_mm(),
            elastic_height_mm: packed.geometry.elastic_height_mm(),
            orientation_violation: violation,
            geometry: packed.fits().then_some(packed.geometry),
        }
    }

    fn layout_at(&mut self, key: &PageKey, geometry: PageGeometry) -> Result<PackedLayout, Error> {
        let cache_key = (key.clone(), geometry.cache_key());
        if let Some(cached) = self.geometry_cache.get(&cache_key) {
            return Ok(cached.clone());
        }
        let packed = self.pack_geometry(key, geometry)?;
        self.geometry_cache.insert(cache_key, packed.clone());
        Ok(packed)
    }

    fn pack_geometry(
        &mut self,
        key: &PageKey,
        geometry: PageGeometry,
    ) -> Result<PackedLayout, Error> {
        let content = self.materialize(key)?;
        let mut boxes = Vec::with_capacity(content.order.len());
        for id in &content.order {
            let Some(box_) = content.boxes.get(id) else {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    format!("page order id {} has no measured box", self.protocol(*id)),
                ));
            };
            boxes.push(box_.clone());
        }
        let cols = geometry.max_cols(self.node_width);
        let printable = geometry.printable_width_mm();
        let anchors = sized_cue_anchors(
            &content
                .anchors
                .iter()
                .map(|(id, side)| (id.as_str(), *side))
                .collect::<Vec<_>>(),
            &boxes,
            self.measurer,
        )?;
        let anchor_sizes: Vec<CueAnchorSize> = anchors
            .into_iter()
            .map(|anchor| CueAnchorSize {
                anchor_id: anchor.anchor_id,
                side: anchor.side,
                width_mm: anchor.width_mm,
                height_mm: anchor.height_mm,
            })
            .collect();
        let edges = content.edges.clone();
        let order = content.order.clone();
        let engine = self.engine;
        let node_width = self.node_width;
        let mut relayout = |row_gap: f64| {
            engine.layout(&boxes, &edges, &order, cols, node_width, row_gap, printable)
        };
        let placed = absorb_cue_slots(&anchor_sizes, &mut relayout, geometry)?;
        let metrics =
            engine.family_row_metrics(&boxes, &edges, &order, cols, node_width, printable)?;
        let (layout, physical_fits) = if let Some(layout) = placed {
            let fits = layout.fits(geometry);
            (layout, fits)
        } else {
            (relayout(ROW_GAP_MM)?, false)
        };
        Ok(PackedLayout {
            boxes: content.boxes,
            layout,
            geometry,
            physical_fits,
            max_nondirect_rows: metrics.max_nondirect_rows,
            orientation_violation: false,
        })
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
                anchors.push((self.protocol(*stub_id).to_owned(), side.cue_side()));
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
            anchors.push((
                self.protocol(draft.subject_id).to_owned(),
                CueSide::Investment,
            ));
        }
        Ok(PageContent {
            boxes,
            edges,
            order,
            anchors,
        })
    }

    fn boxes_for_page(
        &mut self,
        ids: &BTreeSet<NodeId>,
        layout_root_id: Option<NodeId>,
    ) -> Result<HashMap<NodeId, NodeBox>, Error> {
        let mut page_boxes = HashMap::new();
        for &id in ids {
            let Some(box_) = self.boxes.get(&id) else {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    format!("page draft node {} has no measured box", self.protocol(id)),
                ));
            };
            let stored = match layout_root_id {
                Some(root) if id == root => {
                    if id == self.subject_node && box_.is_subject {
                        box_.clone()
                    } else {
                        promote_to_page_subject(box_, self.node_width, self.measurer)?
                    }
                }
                Some(_) if box_.is_subject => NodeBox {
                    is_subject: false,
                    ..box_.clone()
                },
                None | Some(_) => box_.clone(),
            };
            page_boxes.insert(id, stored);
        }
        Ok(page_boxes)
    }

    fn layout_root(&self, ids: &BTreeSet<NodeId>, preferred: NodeId) -> Option<NodeId> {
        if ids.contains(&self.subject_node) {
            Some(self.subject_node)
        } else if ids.contains(&preferred) {
            Some(preferred)
        } else {
            None
        }
    }

    fn overflow_for(&mut self, node: &OverflowNode, side: Side) -> Result<NodeBox, Error> {
        if !self.boxes.contains_key(&node.subject_id) {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("overflow subject missing: {}", node.subject_id),
            ));
        }
        if !self.boxes.contains_key(&node.first_moved_id) {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!("overflow child missing: {}", node.first_moved_id),
            ));
        }
        let name = match side {
            Side::Shareholder => crate::captions::other_shareholders_name(node.moved_count),
            Side::Investment => crate::captions::other_investments_name(node.moved_count),
        };
        let id = self.graph.ensure_overflow(
            node.subject_id,
            node.first_moved_id,
            node.moved_count,
            name.clone(),
        );
        if let Some(cached) = self.overflow_boxes.get(&id) {
            return Ok(cached.clone());
        }
        let protocol = self.graph.protocol_id(id).to_owned();
        let box_ = overflow_box(id, &protocol, &name, self.measurer, self.node_width)?;
        self.overflow_boxes.insert(id, box_.clone());
        Ok(box_)
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

    fn next_shareholder_titles(&mut self) -> (String, String) {
        self.shareholder_continue += 1;
        shareholder_page_titles(&self.subject_name, self.shareholder_continue)
    }

    fn next_investment_titles(&mut self) -> (String, String) {
        self.investment_continue += 1;
        investment_page_titles(&self.subject_name, self.investment_continue)
    }

    fn cut_caption(&mut self, node_id: NodeId) -> Result<String, Error> {
        let name = self.node_name(node_id);
        let ratio = crate::graph::resolve_display_ratio(&mut self.graph, node_id)?;
        match self.graph.layout_parent_optional(node_id) {
            None => Ok(name),
            Some((from_id, to_id)) => {
                if from_id == node_id {
                    Ok(holds_relation(&name, &self.node_name(to_id), ratio))
                } else {
                    Ok(invests_relation(&self.node_name(from_id), &name, ratio))
                }
            }
        }
    }

    fn node_name(&self, id: NodeId) -> String {
        self.graph.node_name(id).to_owned()
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
    anchors: Vec<(String, CueSide)>,
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

fn investment_id_refs<'a>(ids: &[NodeId], boxes: &'a [crate::measure::NodeBox]) -> Vec<&'a str> {
    ids.iter()
        .filter_map(|id| {
            boxes
                .iter()
                .find(|box_| box_.id == *id)
                .map(|box_| box_.protocol_id.as_str())
        })
        .collect()
}

struct SideOracle<'s, 'a, E: LayoutEngine, M: Measurer> {
    session: &'s mut Session<'a, E, M>,
    side: Side,
    overview: bool,
}

impl<E: LayoutEngine, M: Measurer> PageOracle for SideOracle<'_, '_, E, M> {
    fn evaluate(&mut self, draft: &PageDraft) -> Result<LayoutEvaluation, Error> {
        let key = self.session.key(draft, Some(self.side), self.overview);
        self.session.evaluate(&key)
    }

    fn evaluate_direct(&mut self, draft: &PageDraft) -> Result<DirectFit, Error> {
        let key = self.session.key(draft, Some(self.side), self.overview);
        self.session.evaluate_direct(&key)
    }
}

#[cfg(test)]
#[path = "paging_tests.rs"]
mod paging_tests;
