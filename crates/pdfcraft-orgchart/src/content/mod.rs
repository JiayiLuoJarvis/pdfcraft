//! What a node shows: in-box lines, company shading, and ratio text.

mod lines;

#[cfg(test)]
pub(crate) use lines::{build_display, company_fill, ratio_facts, ParentEdge};
pub(crate) use lines::{
    company_fill_rec, display_of, facts_of, LabelHug, LineKind, LogicalLine, RatioFact,
};
