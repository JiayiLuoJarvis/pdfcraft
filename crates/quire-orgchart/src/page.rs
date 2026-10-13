//! Placed nodes, cue slots, and one finished page.

use crate::captions::CueSide;
use crate::display::RatioFact;
use crate::error::{Error, ErrorCode};
use crate::graph::OVERFLOW_ID_PREFIX;
use crate::measure::NodeBox;
use crate::units::{within, PageGeometry};

/// One holding edge on a page, in holder → held order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EdgeRef {
    /// Holder.
    pub from_id: String,
    /// Held company.
    pub to_id: String,
}

impl EdgeRef {
    #[must_use]
    pub fn new(from_id: impl Into<String>, to_id: impl Into<String>) -> Self {
        Self {
            from_id: from_id.into(),
            to_id: to_id.into(),
        }
    }
}

/// A point in layout space. Origin is the chart's top-left. `y` grows downward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointMm {
    /// Millimetres from the chart origin.
    pub x: f64,
    /// Millimetres from the chart origin.
    pub y: f64,
}

/// A node after packing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaidOutNode {
    /// Node id.
    pub id: &'static str,
    /// Left edge, mm.
    pub x: f64,
    /// Top edge, mm.
    pub y: f64,
    /// Width, mm.
    pub width: f64,
    /// Height, mm.
    pub height: f64,
}

/// Owned placed node. Layout stores ids that outlive the function, so the id is owned.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedNode {
    /// Node id.
    pub id: String,
    /// Left edge, mm.
    pub x: f64,
    /// Top edge, mm.
    pub y: f64,
    /// Width, mm.
    pub width: f64,
    /// Height, mm.
    pub height: f64,
}

/// Orthogonal edge polyline. The last point is the arrow tip when `draw_arrow` is set.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutEdge {
    /// Holder.
    pub from_id: String,
    /// Held company.
    pub to_id: String,
    /// Polyline in layout space.
    pub points: Vec<PointMm>,
    /// Draw an arrow on the last segment.
    pub draw_arrow: bool,
}

/// Placed continuation-cue box.
#[derive(Debug, Clone, PartialEq)]
pub struct CueSlot {
    /// Node the cue points at.
    pub anchor_id: String,
    /// Which cone the cue belongs to.
    pub side: CueSide,
    /// Left edge, mm.
    pub x: f64,
    /// Top edge, mm.
    pub y: f64,
    /// Width, mm.
    pub width: f64,
    /// Height, mm.
    pub height: f64,
    /// A dashed leader is drawn when the cue sits beside the anchor.
    pub needs_leader: bool,
}

/// Packed chart in layout space, before the page offset.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutResult {
    /// Placed nodes.
    pub nodes: Vec<PlacedNode>,
    /// Routed edges.
    pub edges: Vec<LaidOutEdge>,
    /// Left of the bounding box.
    pub min_x: f64,
    /// Top of the bounding box.
    pub min_y: f64,
    /// Bounding width, mm.
    pub width_mm: f64,
    /// Bounding height, mm.
    pub height_mm: f64,
    /// Continuation-cue boxes included in the bounds.
    pub slots: Vec<CueSlot>,
}

impl LayoutResult {
    /// Empty chart.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            min_x: 0.0,
            min_y: 0.0,
            width_mm: 0.0,
            height_mm: 0.0,
            slots: Vec::new(),
        }
    }

    /// Bounds cover nodes, edge points, and cue slots.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorCode::LayoutFailed`] when two slots share an anchor and a side.
    pub fn from_parts(
        nodes: Vec<PlacedNode>,
        edges: Vec<LaidOutEdge>,
        slots: Vec<CueSlot>,
    ) -> Result<Self, Error> {
        let mut seen = Vec::new();
        for slot in &slots {
            let key = (slot.anchor_id.as_str(), slot.side);
            if seen.contains(&key) {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    "duplicate cue slot identity",
                ));
            }
            seen.push(key);
        }
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut include = |x: f64, y: f64| {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        };
        for node in &nodes {
            include(node.x, node.y);
            include(node.x + node.width, node.y + node.height);
        }
        for edge in &edges {
            for point in &edge.points {
                include(point.x, point.y);
            }
        }
        for slot in &slots {
            include(slot.x, slot.y);
            include(slot.x + slot.width, slot.y + slot.height);
        }
        if !min_x.is_finite() {
            return Ok(Self::empty());
        }
        Ok(Self {
            nodes,
            edges,
            min_x,
            min_y,
            width_mm: max_x - min_x,
            height_mm: max_y - min_y,
            slots,
        })
    }

    /// The bounding box sits inside the printable area, within half a millimetre.
    #[must_use]
    pub fn fits(&self, geometry: PageGeometry) -> bool {
        within(self.width_mm, geometry.printable_width_mm())
            && within(self.height_mm, geometry.printable_height_mm())
    }
}

/// Navigation cue. The drawn box is the matching [`CueSlot`].
#[derive(Debug, Clone, PartialEq)]
pub struct ContCue {
    /// 1-based PDF page number.
    pub target_page: usize,
    /// Direct children represented by the cue.
    pub direct_count: usize,
    /// Cone.
    pub side: CueSide,
    /// Anchor node. Overflow ids are rejected.
    pub anchor_id: String,
}

/// Clickable overflow box. `target_page` is 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePageLink {
    /// Overflow node id.
    pub node_id: String,
    /// 1-based PDF page number.
    pub target_page: usize,
}

/// One finished page. Render draws `main_title` and `sub_title` as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSpec {
    /// Centered header title.
    pub main_title: String,
    /// Centered header subtitle.
    pub sub_title: String,
    /// Boxes on this page, including a synthetic overflow box.
    pub boxes: Vec<NodeBox>,
    /// Packed geometry.
    pub layout: LayoutResult,
    /// Paper and margin.
    pub geometry: PageGeometry,
    /// Continuation cues. Each one matches a slot.
    pub cont_cues: Vec<ContCue>,
    /// One link per overflow box.
    pub node_page_links: Vec<NodePageLink>,
    /// Ratio labels whose nodes are on this page.
    pub ratio_facts: Vec<RatioFact>,
}

impl PageSpec {
    pub(crate) fn check(&self) -> Result<(), Error> {
        let overflow_ids: Vec<&str> = self
            .boxes
            .iter()
            .filter(|box_| box_.id.starts_with(OVERFLOW_ID_PREFIX))
            .map(|box_| box_.id.as_str())
            .collect();
        let link_ids: Vec<&str> = self
            .node_page_links
            .iter()
            .map(|link| link.node_id.as_str())
            .collect();
        if link_ids.len() != overflow_ids.len()
            || link_ids
                .iter()
                .any(|id| link_ids.iter().filter(|other| other == &id).count() != 1)
            || overflow_ids.iter().any(|id| !link_ids.contains(id))
        {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                "each overflow box needs exactly one node page link",
            ));
        }
        for cue in &self.cont_cues {
            if cue.target_page < 2 || cue.direct_count < 1 {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    "continuation cue requires target_page >= 2 and direct_count >= 1",
                ));
            }
            if cue.anchor_id.starts_with(OVERFLOW_ID_PREFIX) {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    format!(
                        "continuation cue cannot anchor an overflow id: {}",
                        cue.anchor_id
                    ),
                ));
            }
        }
        for link in &self.node_page_links {
            if link.target_page < 2 {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    "node page link requires target_page >= 2",
                ));
            }
        }
        if self.cont_cues.len() != self.layout.slots.len() {
            return Err(Error::new(
                ErrorCode::LayoutFailed,
                "continuation cues and cue slots must match",
            ));
        }
        for cue in &self.cont_cues {
            let found = self
                .layout
                .slots
                .iter()
                .any(|slot| slot.anchor_id == cue.anchor_id && slot.side == cue.side);
            if !found {
                return Err(Error::new(
                    ErrorCode::LayoutFailed,
                    "continuation cues and cue slots must match",
                ));
            }
        }
        Ok(())
    }
}
