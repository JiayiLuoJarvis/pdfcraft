//! Sizes shared by measuring, paging, and drawing.

pub(crate) const PAGE_WIDTH_MM: f64 = 210.0;
pub(crate) const PAGE_HEIGHT_MM: f64 = 297.0;
#[cfg(test)]
pub(crate) const NODE_WIDTH_MM: f64 = 38.0;
pub(crate) const SUBJECT_WIDTH_FACTOR: f64 = 3.0;
/// Cap for page-subject boxes. Independent of [`crate::request::NodeWidth::MAX_FIXED_MM`].
pub(crate) const SUBJECT_MAX_WIDTH_MM: f64 = 120.0;
/// Adaptive non-subject width tiers, mm. Adaptive-layout spec §4.1.
pub(crate) const ADAPTIVE_TIERS_MM: [f64; 3] = [32.0, 48.0, 60.0];
/// Ordinary width used to size an Adaptive page-subject box, mm. Adaptive-layout spec §4.1.
pub(crate) const ADAPTIVE_SUBJECT_BASE_WIDTH_MM: f64 = 38.0;
/// Continuation-cue width never exceeds this, mm. Adaptive-layout spec §4.3.
pub(crate) const CONT_CUE_ANCHOR_MATCH_MAX_MM: f64 = 38.0;
/// Name size after a three-line wrap at [`NAME_FONT_PT`]. Adaptive-layout spec §4.2.
pub(crate) const NAME_FALLBACK_FONT_PT: f64 = 9.0;
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
/// Gap between detail / legal-person lines, pt. Adaptive-layout spec §3.2.
pub(crate) const DETAIL_LINE_GAP_PT: f64 = 1.2;
/// Float tolerance for width-budget row spans. Adaptive-layout spec §5.1.
pub(crate) const ROW_SPAN_EPS_MM: f64 = 1e-9;
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

/// Finite millimetre quantity that can be a map key.
///
/// Construction rejects non-finite values. Equality and hash use the bit
/// pattern, matching prior `f64::to_bits` keys without exposing them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FiniteMm(u64);

impl FiniteMm {
    /// Canonical finite zero. Same bits as `0.0_f64`.
    pub(crate) const ZERO: Self = Self(0);

    /// `None` when `value` is NaN or infinite.
    #[must_use]
    pub(crate) fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value.to_bits()))
    }

    /// Finite zero when `value` is not finite.
    #[must_use]
    pub(crate) fn or_zero(value: f64) -> Self {
        Self::new(value).unwrap_or(Self::ZERO)
    }
}

/// Portrait versus landscape of an A4-derived page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PageOrientation {
    /// Width is at most height.
    Portrait,
    /// Width is greater than height.
    Landscape,
}

/// Geometry cache identity: orientation plus margin. Header and footer come
/// from the page constructors and are not part of the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GeometryKey {
    /// Portrait or landscape of this page.
    orientation: PageOrientation,
    /// Page margin in millimetres.
    margin: FiniteMm,
}

/// A4 page. Landscape swaps the page edges and keeps the margin and bands.
#[derive(Debug, Clone, Copy, PartialEq)]
#[expect(
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

    /// Portrait when width is at most height.
    #[must_use]
    pub(crate) fn orientation(self) -> PageOrientation {
        if self.page_width_mm <= self.page_height_mm {
            PageOrientation::Portrait
        } else {
            PageOrientation::Landscape
        }
    }

    /// Orientation and margin. Equal keys share a layout cache entry.
    #[must_use]
    pub(crate) fn cache_key(self) -> GeometryKey {
        GeometryKey {
            orientation: self.orientation(),
            margin: FiniteMm::or_zero(self.margin_mm),
        }
    }
}

impl Default for PageGeometry {
    fn default() -> Self {
        Self::portrait(MARGIN_MM)
    }
}

#[cfg(test)]
mod tests {
    use super::{PageGeometry, PageOrientation, HEADER_HEIGHT_MM, MARGIN_MM};

    #[test]
    fn cache_key_is_orientation_and_margin() {
        let portrait = PageGeometry::portrait(MARGIN_MM);
        assert_eq!(portrait.cache_key(), PageGeometry::default().cache_key());
        assert_eq!(portrait.orientation(), PageOrientation::Portrait);
        let landscape = portrait.landscape();
        assert_eq!(landscape.orientation(), PageOrientation::Landscape);
        assert_ne!(portrait.cache_key(), landscape.cache_key());
        let wider_margin = PageGeometry::portrait(10.0);
        assert_ne!(portrait.cache_key(), wider_margin.cache_key());
        let different_header = PageGeometry {
            header_height_mm: HEADER_HEIGHT_MM + 1.0,
            ..portrait
        };
        assert_eq!(portrait.cache_key(), different_header.cache_key());
    }
}
