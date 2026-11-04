//! Measured boxes, placed results, and the finished page model.

mod boxes;
mod model;

pub(crate) use crate::content::EdgeRef;
#[cfg(test)]
pub(crate) use boxes::WrappedLine;
pub(crate) use boxes::{
    build_node_boxes, overflow_box, promote_to_page_subject, sized_cue_anchors, InboxNode, NodeBox,
};
pub(crate) use model::{
    ContCue, CueSlot, LaidOutEdge, LayoutResult, NodePageLink, PageSpec, PlacedNode, PointMm,
    RatioHug,
};
