//! Sizes the measurer reads. Page geometry arrives with paging.

pub(crate) const NODE_WIDTH_MM: f64 = 38.0;
pub(crate) const SUBJECT_WIDTH_FACTOR: f64 = 2.55;
pub(crate) const SUBJECT_PADDING_Y_MM: f64 = 3.5;
pub(crate) const SUBJECT_NAME_FONT_PT: f64 = 12.0;
pub(crate) const PADDING_X_MM: f64 = 2.0;
pub(crate) const PADDING_Y_MM: f64 = 1.5;
pub(crate) const NAME_FONT_PT: f64 = 10.0;
pub(crate) const DETAIL_FONT_PT: f64 = 9.0;
pub(crate) const LINE_GAP_PT: f64 = 2.0;
pub(crate) const LAYER_SPACING_MM: f64 = 18.0;
pub(crate) const CONT_CUE_TEXT_PT: f64 = 6.0;
pub(crate) const CONT_CUE_PAD_MM: f64 = 2.0;
pub(crate) const CONT_CUE_MIN_WIDTH_MM: f64 = 18.0;
pub(crate) const CONT_CUE_LINE_COUNT: f64 = 2.0;
pub(crate) const CONT_CUE_LINE_GAP_MM: f64 = 0.4;
pub(crate) const CONT_CUE_PAGE_DIGITS: f64 = 3.0;

pub(crate) fn pt_to_mm(pt: f64) -> f64 {
    pt * 25.4 / 72.0
}

/// Portrait limit for one node box. Half the printable height, after one layer gap.
pub(crate) fn node_height_limit_mm(printable_height_mm: f64) -> f64 {
    (printable_height_mm - LAYER_SPACING_MM) / 2.0
}
