//! Split a validated chart into pages. Shareholders come before investments.

use std::cell::RefCell;
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
    OverflowNode, PageDraft, SearchPage, TaskKind,
};
use crate::graph::{self, ValidatedGraph};
use crate::layout::{LayoutEngine, WrappedEngine};
use crate::measure::{overflow_box, promote_to_page_subject, sized_cue_anchors, Measurer, NodeBox};
use crate::model::{ChartRequest, NodeType};
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

    fn page_edge(self, parent_id: &str, child_id: &str) -> EdgeRef {
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

struct PackedLayout {
    boxes: HashMap<String, NodeBox>,
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

    fn clone_pack(&self) -> Self {
        Self {
            boxes: self.boxes.clone(),
            layout: self.layout.clone(),
            geometry: self.geometry,
            physical_fits: self.physical_fits,
            max_nondirect_rows: self.max_nondirect_rows,
            orientation_violation: self.orientation_violation,
        }
    }
}

struct SidePack {
    pages: Vec<UnnumberedPage>,
    cues: Vec<PendingCue>,
    links: Vec<PendingLink>,
    direct_ids: Vec<String>,
}

struct Session<'a, E: LayoutEngine, M: Measurer> {
    graph: ValidatedGraph<'a>,
    boxes: HashMap<String, NodeBox>,
    geometry: PageGeometry,
    node_order: Vec<String>,
    engine: &'a E,
    measurer: &'a M,
    all_edges: Vec<(String, String)>,
    subject_id: String,
    subject_name: String,
    has_investments: bool,
    whole_main: String,
    whole_sub: String,
    node_width: f64,
    order_index: HashMap<String, usize>,
    shareholder_continue: RefCell<usize>,
    investment_continue: RefCell<usize>,
    overflow_boxes: RefCell<HashMap<String, NodeBox>>,
    chosen_cache: RefCell<HashMap<PageKey, PackedLayout>>,
    direct_cache: RefCell<HashMap<PageKey, DirectFit>>,
    geometry_cache: RefCell<GeometryCache>,
    ratio_facts: Vec<RatioFact>,
}

/// Pack `boxes` with the production engine and a width of two millimetres per character.
///
/// # Errors
///
/// Returns [`Error`] when the request is illegal or no page plan fits.
pub fn paginate(
    request: &ChartRequest,
    boxes: &[NodeBox],
    geometry: PageGeometry,
) -> Result<Vec<PageSpec>, Error> {
    paginate_with(request, boxes, geometry, &WrappedEngine, &FallbackMeasurer)
}

/// Pack `boxes` with a caller-supplied engine and measurer.
///
/// # Errors
///
/// Returns [`Error`] when the request is illegal or no page plan fits.
pub fn paginate_with<E: LayoutEngine, M: Measurer>(
    request: &ChartRequest,
    boxes: &[NodeBox],
    geometry: PageGeometry,
    engine: &E,
    measurer: &M,
) -> Result<Vec<PageSpec>, Error> {
    let graph = graph::validate(request)?;
    let mut box_map = HashMap::new();
    for box_ in boxes {
        box_map.insert(box_.id.clone(), box_.clone());
    }
    Session::new(graph, box_map, geometry, engine, measurer)?.run()
}

/// Two millimetres per character. Used when a page search has no face open.
#[derive(Debug, Default, Clone, Copy)]
pub struct FallbackMeasurer;

impl Measurer for FallbackMeasurer {
    fn string_width_mm(&self, text: &str, _size_pt: f64) -> Result<f64, Error> {
        Ok(crate::units::n64(text.chars().count()) * 2.0)
    }

    fn line_height_mm(&self, _size_pt: f64) -> f64 {
        5.0
    }

    fn ensure_glyphs(&self, _text: &str) -> Result<(), Error> {
        Ok(())
    }
}

impl<'a, E: LayoutEngine, M: Measurer> Session<'a, E, M> {
    fn new(
        graph: ValidatedGraph<'a>,
        boxes: HashMap<String, NodeBox>,
        geometry: PageGeometry,
        engine: &'a E,
        measurer: &'a M,
    ) -> Result<Self, Error> {
        let subject_id = graph.request.subject_id.clone();
        let subject_name = graph.request.nodes[graph.subject].name.clone();
        let has_investments = !graph.outgoing_targets(&subject_id).is_empty();
        let node_width = boxes
            .values()
            .find(|box_| !box_.is_subject)
            .map_or(NODE_WIDTH_MM, |box_| box_.width_mm);
        let node_order = graph
            .request
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect();
        let order_index = graph
            .request
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.clone(), index))
            .collect();
        let all_edges = graph
            .request
            .edges
            .iter()
            .map(|edge| (edge.from_id.clone(), edge.to_id.clone()))
            .collect();
        let ratio_facts = display::facts_of(&graph)?;
        Ok(Self {
            graph,
            boxes,
            geometry,
            node_order,
            engine,
            measurer,
            all_edges,
            subject_id,
            subject_name: subject_name.clone(),
            has_investments,
            whole_main: equity_chart_title(&subject_name),
            whole_sub: whole_chart_subtitle(&subject_name, has_investments),
            node_width,
            order_index,
            shareholder_continue: RefCell::new(0),
            investment_continue: RefCell::new(0),
            overflow_boxes: RefCell::new(HashMap::new()),
            chosen_cache: RefCell::new(HashMap::new()),
            direct_cache: RefCell::new(HashMap::new()),
            geometry_cache: RefCell::new(HashMap::new()),
            ratio_facts,
        })
    }

    fn run(self) -> Result<Vec<PageSpec>, Error> {
        let shareholders = self.sorted(self.graph.incoming_sources(&self.subject_id));
        let investments = self.sorted(self.graph.outgoing_targets(&self.subject_id));
        let all_ids: BTreeSet<String> = self.boxes.keys().cloned().collect();
        if let Some(whole) = self.whole_page_if_elastic(&self.standalone_key(all_ids))? {
            return Ok(vec![self.to_page(&whole)?]);
        }
        let shareholder = self.pack_side(&shareholders, Side::Shareholder)?;
        let investment = self.pack_side(&investments, Side::Investment)?;
        let shareholder_nonempty = !shareholder.pages.is_empty();
        let mut pages = shareholder.pages;
        let offset = pages.len();
        pages.extend(investment.pages);
        if pages.is_empty() {
            let only =
                self.layout_pack(&self.standalone_key(BTreeSet::from([self.subject_id.clone()])))?;
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
            let refs = investment_id_refs(&investment.direct_ids);
            let count = direct_count(&pages[offset].boxes, &refs).max(1);
            cues.push(PendingCue {
                source_index: 0,
                target_index: offset,
                direct_count: count,
                side: CueSide::Investment,
                anchor_id: self.subject_id.clone(),
            });
        }
        finalize_pages(&pages, &cues, &links, self.measurer)
    }

    fn pack_side(&self, direct_ids: &[String], side: Side) -> Result<SidePack, Error> {
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
        let result = search_family_pages(
            &family,
            &mut |draft| self.evaluate(&self.key(draft, Some(side), overview)),
            &mut |draft| self.evaluate_direct(&self.key(draft, Some(side), overview)),
        )?;
        if result.plan.pages.is_empty() {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                format!(
                    "family search returned no pages after {} expansions (budget_exhausted={})",
                    result.expansions, result.budget_exhausted
                ),
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
                    anchor_id: cut.task.subject_id.clone(),
                });
            }
            if let Some(overflow) = &page.overflow {
                let target = task_index(&first_by_task, &overflow.task, source_index)?;
                links.push(PendingLink {
                    source_index,
                    target_index: target,
                    node_id: overflow.node.id(),
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

    fn family_graph(&self, direct_ids: &[String], side: Side) -> FamilyGraph {
        let mut node_ids = HashSet::from([self.subject_id.clone()]);
        let mut stack = direct_ids.to_vec();
        while let Some(node_id) = stack.pop() {
            if !node_ids.insert(node_id.clone()) {
                continue;
            }
            let next = match side {
                Side::Shareholder => self.graph.incoming_sources(&node_id),
                Side::Investment => self.graph.outgoing_targets(&node_id),
            };
            stack.extend(next.into_iter().map(str::to_owned));
        }
        let mut edges = Vec::new();
        for (edge_index, (from_id, to_id)) in self.all_edges.iter().enumerate() {
            if !node_ids.contains(from_id) || !node_ids.contains(to_id) {
                continue;
            }
            let (parent_id, child_id) = if side == Side::Shareholder {
                (to_id.clone(), from_id.clone())
            } else {
                (from_id.clone(), to_id.clone())
            };
            edges.push(FamilyEdge {
                id: edge_index,
                parent_id,
                child_id,
            });
        }
        let person_ids = node_ids
            .iter()
            .filter(|id| {
                self.graph.index(id).is_some_and(|index| {
                    self.graph.request.nodes[index].node_type == Some(NodeType::Person)
                })
            })
            .cloned()
            .collect();
        FamilyGraph {
            subject_id: self.subject_id.clone(),
            edges,
            node_order: self
                .node_order
                .iter()
                .filter(|id| node_ids.contains(*id))
                .cloned()
                .collect(),
            person_ids,
        }
    }

    fn search_page_to_spec(
        &self,
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
        let packed = self.layout_at(&self.key(&page.draft, Some(side), overview), geometry)?;
        if page.draft.subject_id == self.subject_id {
            let (main, sub) = match side {
                Side::Shareholder => self.next_shareholder_titles(),
                Side::Investment => self.next_investment_titles(),
            };
            self.unnumbered(&packed, main, sub, None)
        } else {
            let sub = self.cut_caption(&page.draft.subject_id)?;
            let name = self.node_name(&page.draft.subject_id);
            self.unnumbered(&packed, self.whole_main.clone(), sub, Some(name))
        }
    }

    fn key(&self, draft: &PageDraft, side: Option<Side>, overview: bool) -> PageKey {
        PageKey {
            overview_cue: overview
                && side.is_some()
                && draft.kind == TaskKind::Primary
                && draft.subject_id == self.subject_id,
            draft: draft.clone(),
            side,
        }
    }

    fn standalone_key(&self, node_ids: BTreeSet<String>) -> PageKey {
        PageKey {
            draft: PageDraft {
                subject_id: self.subject_id.clone(),
                node_ids,
                stub_ids: BTreeSet::new(),
                overflow: None,
                kind: TaskKind::Primary,
            },
            side: None,
            overview_cue: false,
        }
    }

    fn evaluate(&self, key: &PageKey) -> Result<LayoutEvaluation, Error> {
        let packed = self.layout_pack(key)?;
        Ok(Self::as_evaluation(&packed, packed.orientation_violation))
    }

    fn evaluate_direct(&self, key: &PageKey) -> Result<DirectFit, Error> {
        if let Some(cached) = self.direct_cache.borrow().get(key) {
            return Ok(cached.clone());
        }
        let chosen = self.choose_direct_layer(key)?;
        self.direct_cache
            .borrow_mut()
            .insert(key.clone(), chosen.clone());
        Ok(chosen)
    }

    fn layout_pack(&self, key: &PageKey) -> Result<PackedLayout, Error> {
        if let Some(cached) = self.chosen_cache.borrow().get(key) {
            return Ok(cached.clone_pack());
        }
        let packed = self.choose_layout(key)?;
        self.chosen_cache
            .borrow_mut()
            .insert(key.clone(), packed.clone_pack());
        Ok(packed)
    }

    fn choose_layout(&self, key: &PageKey) -> Result<PackedLayout, Error> {
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

    fn choose_direct_layer(&self, key: &PageKey) -> Result<DirectFit, Error> {
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

    fn whole_page_if_elastic(&self, key: &PageKey) -> Result<Option<PackedLayout>, Error> {
        let (preferred, alternate) = self.orientation_pair(key)?;
        let preferred_pack = self.layout_at(key, preferred)?;
        let alternate_pack = self.layout_at(key, alternate)?;
        for admit in [Admit::Comfort, Admit::Elastic] {
            for (mut pack, violation) in [
                (preferred_pack.clone_pack(), false),
                (alternate_pack.clone_pack(), true),
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

    fn orientation_pair(&self, key: &PageKey) -> Result<(PageGeometry, PageGeometry), Error> {
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

    fn layout_at(&self, key: &PageKey, geometry: PageGeometry) -> Result<PackedLayout, Error> {
        let cache_key = (key.clone(), geometry.cache_key());
        if let Some(cached) = self.geometry_cache.borrow().get(&cache_key) {
            return Ok(cached.clone_pack());
        }
        let packed = self.pack_geometry(key, geometry)?;
        self.geometry_cache
            .borrow_mut()
            .insert(cache_key, packed.clone_pack());
        Ok(packed)
    }

    fn pack_geometry(&self, key: &PageKey, geometry: PageGeometry) -> Result<PackedLayout, Error> {
        let content = self.materialize(key)?;
        let boxes: Vec<NodeBox> = content
            .order
            .iter()
            .filter_map(|id| content.boxes.get(id).cloned())
            .collect();
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

    fn materialize(&self, key: &PageKey) -> Result<PageContent, Error> {
        let draft = &key.draft;
        let root = self.layout_root(&draft.node_ids, &draft.subject_id);
        let mut boxes = self.boxes_for_page(&draft.node_ids, root.as_deref())?;
        let mut edges: Vec<EdgeRef> = self
            .all_edges
            .iter()
            .filter(|(from_id, to_id)| {
                draft.node_ids.contains(from_id) && draft.node_ids.contains(to_id)
            })
            .map(|(from_id, to_id)| EdgeRef::new(from_id, to_id))
            .collect();
        let mut order: Vec<String> = self
            .node_order
            .iter()
            .filter(|id| draft.node_ids.contains(*id))
            .cloned()
            .collect();
        let mut anchors = Vec::new();
        if let Some(side) = key.side {
            for stub_id in &draft.stub_ids {
                anchors.push((stub_id.clone(), side.cue_side()));
            }
            if let Some(overflow) = &draft.overflow {
                let box_ = self.overflow_for(overflow, side)?;
                let id = box_.id.clone();
                boxes.insert(id.clone(), box_);
                edges.push(side.page_edge(&draft.subject_id, &id));
                order.push(id);
            }
        }
        if key.overview_cue {
            anchors.push((draft.subject_id.clone(), CueSide::Investment));
        }
        Ok(PageContent {
            boxes,
            edges,
            order,
            anchors,
        })
    }

    fn boxes_for_page(
        &self,
        ids: &BTreeSet<String>,
        layout_root_id: Option<&str>,
    ) -> Result<HashMap<String, NodeBox>, Error> {
        let mut page_boxes = HashMap::new();
        for node_id in ids {
            let Some(box_) = self.boxes.get(node_id) else {
                continue;
            };
            let stored = match layout_root_id {
                Some(root) if node_id == root => {
                    if node_id == &self.subject_id && box_.is_subject {
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
            page_boxes.insert(node_id.clone(), stored);
        }
        Ok(page_boxes)
    }

    fn layout_root(&self, ids: &BTreeSet<String>, preferred: &str) -> Option<String> {
        if ids.contains(&self.subject_id) {
            Some(self.subject_id.clone())
        } else if ids.contains(preferred) {
            Some(preferred.to_owned())
        } else {
            None
        }
    }

    fn overflow_for(&self, node: &OverflowNode, side: Side) -> Result<NodeBox, Error> {
        let id = node.id();
        if let Some(cached) = self.overflow_boxes.borrow().get(&id) {
            return Ok(cached.clone());
        }
        let name = match side {
            Side::Shareholder => crate::captions::other_shareholders_name(node.moved_count),
            Side::Investment => crate::captions::other_investments_name(node.moved_count),
        };
        let box_ = overflow_box(&id, &name, self.measurer, self.node_width)?;
        self.overflow_boxes.borrow_mut().insert(id, box_.clone());
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
        let ids: HashSet<&str> = boxes.iter().map(|box_| box_.id.as_str()).collect();
        let ratio_facts = self
            .ratio_facts
            .iter()
            .filter(|fact| ids.contains(fact.node_id.as_str()))
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

    fn next_shareholder_titles(&self) -> (String, String) {
        let mut counter = self.shareholder_continue.borrow_mut();
        *counter += 1;
        shareholder_page_titles(&self.subject_name, *counter)
    }

    fn next_investment_titles(&self) -> (String, String) {
        let mut counter = self.investment_continue.borrow_mut();
        *counter += 1;
        investment_page_titles(&self.subject_name, *counter)
    }

    fn cut_caption(&self, node_id: &str) -> Result<String, Error> {
        let name = self.node_name(node_id);
        let ratio = self
            .graph
            .index(node_id)
            .map(|index| {
                let mut cache = HashMap::new();
                graph::resolve_display_ratio(&self.graph, index, &mut cache)
            })
            .transpose()?
            .flatten();
        match self.graph.layout_parent_ends(node_id) {
            Err(error) if error.code == ErrorCode::InvalidProtocol => Ok(name),
            Err(error) => Err(error),
            Ok(None) => Ok(name),
            Ok(Some((from_id, to_id))) => {
                if from_id == node_id {
                    Ok(holds_relation(&name, &self.node_name(to_id), ratio))
                } else {
                    Ok(invests_relation(&self.node_name(from_id), &name, ratio))
                }
            }
        }
    }

    fn node_name(&self, id: &str) -> String {
        self.graph
            .index(id)
            .map(|index| self.graph.request.nodes[index].name.clone())
            .unwrap_or_default()
    }

    fn sorted(&self, ids: Vec<&str>) -> Vec<String> {
        let mut owned: Vec<String> = ids.into_iter().map(str::to_owned).collect();
        owned.sort_by_key(|id| self.order_index.get(id).copied().unwrap_or(usize::MAX));
        owned.dedup();
        owned
    }
}

struct PageContent {
    boxes: HashMap<String, NodeBox>,
    edges: Vec<EdgeRef>,
    order: Vec<String>,
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
    let mut degree: HashMap<(String, bool), usize> = HashMap::new();
    for edge in edges {
        for (node_id, outward) in [(&edge.from_id, true), (&edge.to_id, false)] {
            let count = degree.entry((node_id.clone(), outward)).or_insert(0);
            *count += 1;
            if *count >= 2 {
                return true;
            }
        }
    }
    false
}

fn task_key(task: &AttachmentTask) -> (String, BTreeSet<usize>, TaskKind) {
    (task.subject_id.clone(), task.edge_ids.clone(), task.kind)
}

fn task_index(
    first_by_task: &HashMap<(String, BTreeSet<usize>, TaskKind), usize>,
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

fn investment_id_refs(ids: &[String]) -> Vec<&str> {
    ids.iter().map(String::as_str).collect()
}
