//! Choosing a layout for a page key: orientation, comfort, and the geometry cache.

use std::collections::BTreeSet;

use super::{has_fork, Admit, PackedLayout, PageKey, Session, Side, SideOracle};
use crate::error::{Error, ErrorCode};
use crate::font::Measurer;
use crate::graph::NodeId;
use crate::layout::{LayoutEngine, LayoutInput};
use crate::page::sized_cue_anchors;
use crate::paginate::cues::{absorb_cue_slots, CueAnchorSize};
use crate::paginate::family::{DirectFit, LayoutEvaluation, PageDraft, PageOracle, TaskKind};
use crate::units::{PageGeometry, ROW_GAP_MM};

impl<E: LayoutEngine, M: Measurer> Session<'_, E, M> {
    pub(super) fn key(&self, draft: &PageDraft, side: Option<Side>, overview: bool) -> PageKey {
        PageKey {
            overview_cue: overview
                && side.is_some()
                && draft.kind == TaskKind::Primary
                && draft.subject_id == self.subject_node,
            draft: draft.clone(),
            side,
        }
    }

    pub(super) fn standalone_key(&self, node_ids: BTreeSet<NodeId>) -> PageKey {
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

    pub(super) fn layout_pack(&mut self, key: &PageKey) -> Result<PackedLayout, Error> {
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

    pub(super) fn whole_page_if_elastic(
        &mut self,
        key: &PageKey,
    ) -> Result<Option<PackedLayout>, Error> {
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

    pub(super) fn layout_at(
        &mut self,
        key: &PageKey,
        geometry: PageGeometry,
    ) -> Result<PackedLayout, Error> {
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
                    format!(
                        "page order id {} has no measured box",
                        self.graph.label(*id)
                    ),
                ));
            };
            boxes.push(box_.clone());
        }
        let cols = geometry.max_cols(self.node_width);
        let printable = geometry.printable_width_mm();
        let anchors = sized_cue_anchors(&content.anchors, &boxes, self.measurer)?;
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
            engine.layout(&LayoutInput {
                boxes: &boxes,
                edges: &edges,
                node_order: &order,
                cols,
                node_width_mm: node_width,
                row_gap_mm: row_gap,
                printable_width_mm: printable,
            })
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

    pub(super) fn layout_root(&self, ids: &BTreeSet<NodeId>, preferred: NodeId) -> Option<NodeId> {
        if ids.contains(&self.subject_node) {
            Some(self.subject_node)
        } else if ids.contains(&preferred) {
            Some(preferred)
        } else {
            None
        }
    }
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
