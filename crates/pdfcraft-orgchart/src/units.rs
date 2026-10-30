//! Sizes shared by measuring, paging, and drawing.

pub(crate) const PAGE_WIDTH_MM: f64 = 210.0;
pub(crate) const PAGE_HEIGHT_MM: f64 = 297.0;
pub(crate) const NODE_WIDTH_MM: f64 = 38.0;
pub(crate) const SUBJECT_WIDTH_FACTOR: f64 = 2.55;
pub(crate) const SUBJECT_PADDING_Y_MM: f64 = 3.5;
pub(crate) const SUBJECT_NAME_FONT_PT: f64 = 12.0;
pub(crate) const MARGIN_MM: f64 = 8.0;
pub(crate) const HEADER_HEIGHT_MM: f64 = 18.0;
pub(crate) const FOOTER_HEIGHT_MM: f64 = 6.0;
pub(crate) const PADDING_X_MM: f64 = 2.0;
pub(crate) const PADDING_Y_MM: f64 = 1.5;
pub(crate) const NAME_FONT_PT: f64 = 10.0;
pub(crate) const DETAIL_FONT_PT: f64 = 9.0;
pub(crate) const RATIO_FONT_PT: f64 = 9.0;
pub(crate) const LINE_GAP_PT: f64 = 2.0;
pub(crate) const LAYER_SPACING_MM: f64 = 18.0;
pub(crate) const NODE_SPACING_MM: f64 = 6.0;
pub(crate) const ROW_GAP_MM: f64 = 8.0;
pub(crate) const FAMILY_GAP_MM: f64 = 12.0;
pub(crate) const FIT_EPSILON_MM: f64 = 0.5;
pub(crate) const BUS_STAGGER_MM: f64 = 2.0;
pub(crate) const BUS_Y_EPS_MM: f64 = 0.5;
pub(crate) const BUS_ENDPOINT_CLEARANCE_MM: f64 = 0.5;
pub(crate) const COMFORT_HEIGHT_RATIO: f64 = 0.85;
pub(crate) const ELASTIC_HEIGHT_RATIO: f64 = 0.95;
pub(crate) const NODE_CORNER_RADIUS_MM: f64 = 2.2;
pub(crate) const RATIO_LABEL_OFFSET_X_MM: f64 = 1.2;
pub(crate) const RATIO_LABEL_GAP_Y_MM: f64 = 0.4;
pub(crate) const SHAREHOLDER_STUB_MIN_MM: f64 = 6.5;
pub(crate) const EDGE_WIDTH_PT: f64 = 0.7;
/// Black stroke for node boxes and continuation boxes.
pub(crate) const INK_BYTE: u8 = 0;
/// Gray for edges, edge arrows, continuation leaders, and leader arrows.
///
/// An 8-bit channel of this gray is byte 51.
pub(crate) const EDGE_GRAY: f64 = 0.2;
pub(crate) const ARROW_SIZE_PT: f64 = 6.0;
pub(crate) const SUBJECT_BORDER_PT: f64 = 1.5;
pub(crate) const NORMAL_BORDER_PT: f64 = 0.8;
pub(crate) const MAIN_TITLE_PT: f64 = 16.0;
/// Gap from the page margin down to the main-title slot top.
pub(crate) const MAIN_TITLE_TOP_GAP_MM: f64 = 0.0;
/// Gap between the main-title line and the subtitle slot top.
pub(crate) const TITLE_LINE_GAP_MM: f64 = 1.5;
pub(crate) const SUB_TITLE_PT: f64 = 8.0;
pub(crate) const FOOTER_TEXT_PT: f64 = 9.0;
pub(crate) const CONT_CUE_TEXT_PT: f64 = 6.0;
pub(crate) const CONT_CUE_PAD_MM: f64 = 2.0;
pub(crate) const CONT_CUE_MIN_WIDTH_MM: f64 = 18.0;
pub(crate) const CONT_CUE_LINE_COUNT: f64 = 2.0;
pub(crate) const CONT_CUE_LINE_GAP_MM: f64 = 0.4;
pub(crate) const CONT_CUE_PAGE_DIGITS: f64 = 3.0;
pub(crate) const CONT_CUE_GAP_MM: f64 = 3.0;
pub(crate) const CONT_CUE_DASH_ON_PT: f64 = 4.0;
pub(crate) const CONT_CUE_DASH_OFF_PT: f64 = 3.0;
/// Continuation-box stroke width. Node boxes stay on their own widths.
pub(crate) const CONT_CUE_LINE_WIDTH_PT: f64 = 1.0;
/// Continuation-box corner radius. Node boxes stay at `NODE_CORNER_RADIUS_MM`.
pub(crate) const CONT_CUE_CORNER_MM: f64 = 1.0;
/// Leader arrow length as a fraction of `ARROW_SIZE_PT`. The drawn length is 4.5 pt.
pub(crate) const LEADER_ARROW_LENGTH_FACTOR: f64 = 0.75;
/// Leader arrow half-angle, degrees each side of the shaft. Edge arrows stay at 0.45.
pub(crate) const LEADER_ARROW_HALF_ANGLE_DEG: f64 = 30.0;
pub(crate) const PAGE_NUMBER_INSET_MM: f64 = 5.0;
pub(crate) const COMPANY_SHADE: f64 = 247.0 / 255.0;

pub(crate) fn pt_to_mm(pt: f64) -> f64 {
    pt * 25.4 / 72.0
}

pub(crate) fn mm_to_pt(mm: f64) -> f64 {
    mm * 72.0 / 25.4
}

/// `count` as millimetre arithmetic. Counts above `u32` saturate.
#[must_use]
pub(crate) fn n64(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// `value` is inside `limit`, with the fit epsilon.
#[must_use]
pub(crate) fn within(value: f64, limit: f64) -> bool {
    value <= limit + FIT_EPSILON_MM
}

/// Portrait limit for one node box. Half the printable height, after one layer gap.
pub(crate) fn node_height_limit_mm(printable_height_mm: f64) -> f64 {
    (printable_height_mm - LAYER_SPACING_MM) / 2.0
}

/// A4 page. Landscape swaps the page edges and keeps the margin and bands.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "mm keeps page lengths distinct from points"
)]
pub(crate) struct PageGeometry {
    /// Page width in mm.
    pub page_width_mm: f64,
    /// Page height in mm.
    pub page_height_mm: f64,
    /// Margin on every edge, in mm.
    pub margin_mm: f64,
    /// Header band height in mm.
    pub header_height_mm: f64,
    /// Footer band height in mm.
    pub footer_height_mm: f64,
}

impl PageGeometry {
    /// Portrait A4 with `margin_mm`.
    pub(crate) fn portrait(margin_mm: f64) -> Self {
        Self {
            page_width_mm: PAGE_WIDTH_MM,
            page_height_mm: PAGE_HEIGHT_MM,
            margin_mm,
            header_height_mm: HEADER_HEIGHT_MM,
            footer_height_mm: FOOTER_HEIGHT_MM,
        }
    }

    /// Swap width and height.
    #[must_use]
    pub(crate) fn landscape(self) -> Self {
        Self {
            page_width_mm: self.page_height_mm,
            page_height_mm: self.page_width_mm,
            ..self
        }
    }

    /// Width inside the side margins.
    #[must_use]
    pub(crate) fn printable_width_mm(self) -> f64 {
        self.page_width_mm - 2.0 * self.margin_mm
    }

    /// Height inside the margins, header, and footer.
    #[must_use]
    pub(crate) fn printable_height_mm(self) -> f64 {
        self.page_height_mm - 2.0 * self.margin_mm - self.header_height_mm - self.footer_height_mm
    }

    /// Comfort band.
    #[must_use]
    pub(crate) fn comfort_height_mm(self) -> f64 {
        self.printable_height_mm() * COMFORT_HEIGHT_RATIO
    }

    /// Elastic band.
    #[must_use]
    pub(crate) fn elastic_height_mm(self) -> f64 {
        self.printable_height_mm() * ELASTIC_HEIGHT_RATIO
    }

    /// Bit pattern of the page size. Equal geometries share a layout cache entry.
    #[must_use]
    pub(crate) fn cache_key(self) -> (u64, u64, u64, u64, u64) {
        (
            self.page_width_mm.to_bits(),
            self.page_height_mm.to_bits(),
            self.margin_mm.to_bits(),
            self.header_height_mm.to_bits(),
            self.footer_height_mm.to_bits(),
        )
    }

    /// How many ordinary boxes fit on one row.
    #[must_use]
    pub(crate) fn max_cols(self, node_width_mm: f64) -> usize {
        let pitch = node_width_mm + NODE_SPACING_MM;
        if pitch <= 0.0 {
            return 1;
        }
        let raw = (self.printable_width_mm() + NODE_SPACING_MM) / pitch;
        count_units(raw).max(1)
    }
}

fn count_units(value: f64) -> usize {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let whole = value.trunc();
    let mut count = 0_usize;
    let mut cursor = 0.0;
    while cursor + 1.0 <= whole && count < 64 {
        cursor += 1.0;
        count += 1;
    }
    count
}

impl Default for PageGeometry {
    fn default() -> Self {
        Self::portrait(MARGIN_MM)
    }
}

#[cfg(test)]
mod tests {
    use super::{PageGeometry, NODE_SPACING_MM, NODE_WIDTH_MM};

    #[test]
    fn default_portrait_fits_four_columns() {
        assert_eq!(PageGeometry::default().max_cols(NODE_WIDTH_MM), 4);
    }

    #[test]
    fn default_landscape_fits_six_columns() {
        assert_eq!(
            PageGeometry::default().landscape().max_cols(NODE_WIDTH_MM),
            6
        );
    }

    #[test]
    fn a_raw_count_above_64_stays_at_64() {
        let geometry = PageGeometry {
            page_width_mm: 20_000.0,
            page_height_mm: 297.0,
            margin_mm: 0.0,
            header_height_mm: 0.0,
            footer_height_mm: 0.0,
        };
        assert_eq!(geometry.max_cols(1.0), 64);
    }

    #[test]
    fn a_non_positive_pitch_is_one_column() {
        assert_eq!(PageGeometry::default().max_cols(-NODE_SPACING_MM), 1);
        assert_eq!(PageGeometry::default().max_cols(-NODE_SPACING_MM - 1.0), 1);
    }
}
