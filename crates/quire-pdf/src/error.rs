//! Failures from constructing lengths, geometry, canvas ops, fonts, layout, and the PDF backend.
//!
//! An [`Error`] pairs what went wrong ([`ErrorKind`]) with where layout was when it
//! happened ([`Location`]). Failures inside `skrifa` or `krilla` are translated into
//! quire kinds; the dependency's own error, when it has one, is reachable through
//! [`std::error::Error::source`].

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

use crate::limits::LimitKind;

/// A construction, layout, or drawing failure.
///
/// Equality compares [`kind`](Self::kind) and [`at`](Self::at) only. The dependency
/// error behind [`source`](StdError::source) is not compared, so tests can match a
/// quire failure without rebuilding a `skrifa` or `krilla` value.
#[derive(Debug, Clone)]
pub struct Error {
    /// What went wrong.
    pub kind: ErrorKind,
    /// Where layout was when it went wrong. `None` when the failure is not tied to
    /// a section, page, band, or table cell.
    pub at: Option<Location>,
    source: Option<Arc<dyn StdError + Send + Sync>>,
}

impl Error {
    /// Keeps `source` as the dependency failure behind this error.
    pub(crate) fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Arc::new(source));
        self
    }

    /// Fills location fields this error does not know yet. An inner value wins.
    pub(crate) fn fill_missing(&mut self, at: &Location) {
        let loc = self.location();
        if loc.section.is_none() {
            loc.section = at.section;
        }
        if loc.page.is_none() {
            loc.page = at.page;
        }
        if loc.region.is_none() {
            loc.region = at.region;
        }
        if loc.cell.is_none() {
            loc.cell = at.cell;
        }
    }

    /// Records the zero-based section index unless an inner layer already did.
    pub(crate) fn in_section(mut self, index: usize) -> Self {
        self.fill_missing(&Location::section(index));
        self
    }

    /// Records the page at zero-based `index` unless an inner layer already did.
    ///
    /// A page number that does not fit `u32` stays unknown.
    pub(crate) fn on_page_index(mut self, index: usize) -> Self {
        self.fill_missing(&Location::page_index(index));
        self
    }

    /// Records the page region unless an inner layer already did.
    pub(crate) fn in_region(mut self, region: Region) -> Self {
        self.fill_missing(&Location::with_region(Location::default(), region));
        self
    }

    /// Records the table cell unless a nested table already did.
    pub(crate) fn in_cell(mut self, row: usize, col: usize) -> Self {
        self.fill_missing(&Location::at_cell(row, col));
        self
    }

    fn location(&mut self) -> &mut Location {
        self.at.get_or_insert_with(Location::default)
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            kind,
            at: None,
            source: None,
        }
    }
}

impl From<GeometryKind> for Error {
    fn from(kind: GeometryKind) -> Self {
        ErrorKind::InvalidGeometry(kind).into()
    }
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.at == other.at
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        match &self.at {
            Some(at) if *at != Location::default() => write!(f, " ({at})"),
            _ => Ok(()),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn StdError + 'static))
    }
}

/// What went wrong.
///
/// Length variants carry `value`, the argument of `pt` or `mm`, in the unit
/// the caller passed. When a finite millimeter converts to a non-finite point
/// value, or a positive millimeter underflows to a point value that is not
/// positive, `value` is that point value.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// `value` is NaN or infinite.
    NonFiniteLength {
        /// The rejected number.
        value: f32,
    },
    /// `value` is finite and below zero.
    NegativeLength {
        /// The rejected number.
        value: f32,
    },
    /// `value` is finite and not above zero.
    NonPositiveLength {
        /// The rejected number.
        value: f32,
    },
    /// A link URI was empty.
    EmptyLink,
    /// A `Canvas` call used an illegal page lifecycle state.
    InvalidCanvasState {
        /// The method that failed.
        operation: &'static str,
        /// The state that blocked it.
        state: &'static str,
    },
    /// Geometry that brands allow but layout or the backend cannot use.
    InvalidGeometry(GeometryKind),
    /// Font bytes could not be parsed, were empty, or are otherwise unusable.
    ///
    /// When a parser rejected the bytes, its error is the [`source`](StdError::source).
    /// A file over the byte cap is [`ErrorKind::LimitExceeded`] with [`LimitKind::FontBytes`].
    Font {
        /// Collection index passed to the parser.
        index: u32,
        /// What quire was doing when the font failed.
        message: String,
    },
    /// `scalar` has no glyph in `font`. The document is not produced.
    MissingGlyph {
        /// Raw id of the font that was queried, as [`FontId::as_u32`](crate::FontId::as_u32) returns.
        font: u32,
        /// Unmapped Unicode scalar.
        scalar: char,
    },
    /// `id` is not registered in the store this canvas uses.
    UnknownFont {
        /// Raw identifier that was not found.
        id: u32,
    },
    /// Bold text was requested but [`Resources::bold_font`](crate::spec::Resources::bold_font) was not set.
    BoldFaceRequired,
    /// skrifa and krilla read different units per em from the same bytes.
    FontMetricsMismatch {
        /// Units per em from skrifa.
        skrifa_upem: f32,
        /// Units per em from krilla.
        krilla_upem: f32,
    },
    /// Content that cannot break or shrink is wider than the available frame width.
    UnbreakableWiderThanFrame {
        /// Short text of the unbreakable run. `None` for an image or a rule.
        text: Option<String>,
        /// Width from the current line start through the overflowing atom.
        width: f32,
        /// Available width that was exceeded.
        avail: f32,
    },
    /// A fill-to-end blank is followed by another inline run.
    RunAfterFillBlank,
    /// A node that cannot split is taller than the full body frame.
    UnsplittableTallerThanFrame {
        /// Measured height of the content that has to stay in one frame. Content that
        /// split anyway is summed over fresh full frames until a frame placed nothing.
        height: f32,
        /// Full frame height that was exceeded.
        avail: f32,
    },
    /// Column or table track widths cannot be resolved inside the frame.
    ColumnWidths {
        /// Which rule failed.
        reason: ColumnWidthsReason,
    },
    /// `node` is not allowed inside `container`.
    InvalidCellContent {
        /// Container that rejected the node, such as `"table cell"`.
        container: &'static str,
        /// Rejected node kind, such as `"columns"`.
        node: &'static str,
    },
    /// Table spans do not tile the grid. [`Location`]'s `cell` field names the offending slot.
    InvalidCellSpan(CellSpanReason),
    /// Header or footer content is taller than its margin band.
    DecorationTallerThanMargin {
        /// Measured decoration height. Content taller than the page is summed over
        /// page-high frames.
        height: f32,
        /// Band height, the matching page margin.
        margin: f32,
    },
    /// Header or footer content is wider than the page.
    DecorationWiderThanFrame {
        /// Measured decoration width.
        width: f32,
        /// Page width available to the band.
        avail: f32,
    },
    /// Image bytes were empty, had a zero side, could not be decoded, or this build has no decoder.
    ///
    /// A byte, pixel, or edge cap is [`ErrorKind::LimitExceeded`].
    Image {
        /// Decoder text, or why the bytes were rejected before decode.
        message: String,
    },
    /// A [`Limits`](crate::Limits) cap was exceeded, or a setter rejected the requested cap.
    ///
    /// `value` is the measured size, or the rejected setting. `max` is the cap.
    /// A setter given 0 reports `value: 0` and `max: 0`.
    LimitExceeded {
        /// Which cap failed.
        kind: LimitKind,
        /// Measured size, or the rejected setting.
        value: u64,
        /// Cap that rejected `value`. `0` when a setter rejected zero.
        max: u64,
    },
    /// `id` is not registered in this document's image table.
    UnknownImage {
        /// Raw identifier that was not found.
        id: u32,
    },
    /// Body, margins, or page hooks were written both on the document and in sections.
    ContentOutsideSection,
    /// A single-line run contains a forced line break.
    UnexpectedLineBreakInSingleLine {
        /// Short normalized text that contains the break. CRLF is one `\n`.
        text: String,
    },
    /// The PDF backend failed.
    ///
    /// When the PDF writer returned an error, it is the [`source`](StdError::source).
    Backend {
        /// What quire asked the backend to do.
        message: String,
    },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteLength { value } => write!(f, "{value} is not a finite length"),
            Self::NegativeLength { value } => write!(f, "{value} is a negative length"),
            Self::NonPositiveLength { value } => write!(f, "{value} is not a positive length"),
            Self::EmptyLink => f.write_str("link URI is empty"),
            Self::InvalidCanvasState { operation, state } => {
                write!(f, "{operation} is invalid while {state}")
            }
            Self::InvalidGeometry(kind) => write!(f, "invalid geometry: {kind}"),
            Self::Font { index, message } => write!(f, "font index {index}: {message}"),
            Self::MissingGlyph { font, scalar } => write!(
                f,
                "font {font} has no glyph for {scalar:?} (U+{:04X})",
                u32::from(*scalar)
            ),
            Self::UnknownFont { id } => write!(f, "font {id} is not registered"),
            Self::BoldFaceRequired => f.write_str("bold text needs a registered bold font"),
            Self::FontMetricsMismatch {
                skrifa_upem,
                krilla_upem,
            } => write!(
                f,
                "font units per em differ: skrifa {skrifa_upem}, krilla {krilla_upem}"
            ),
            Self::UnbreakableWiderThanFrame { text, width, avail } => match text {
                Some(text) => write!(
                    f,
                    "unbreakable content {text:?} width {width} exceeds available width {avail}"
                ),
                None => write!(
                    f,
                    "unbreakable content width {width} exceeds available width {avail}"
                ),
            },
            Self::RunAfterFillBlank => f.write_str("a fill-to-end blank must be the last run"),
            Self::UnsplittableTallerThanFrame { height, avail } => write!(
                f,
                "unsplittable content height {height} exceeds frame height {avail}"
            ),
            Self::ColumnWidths { reason } => write!(f, "column widths cannot resolve: {reason}"),
            Self::InvalidCellContent { container, node } => {
                write!(f, "{node} is not allowed in a {container}")
            }
            Self::InvalidCellSpan(reason) => write!(f, "table cell span is invalid: {reason}"),
            Self::DecorationTallerThanMargin { height, margin } => {
                write!(f, "decoration height {height} exceeds margin band {margin}")
            }
            Self::DecorationWiderThanFrame { width, avail } => {
                write!(f, "decoration width {width} exceeds page width {avail}")
            }
            Self::Image { message } => write!(f, "image: {message}"),
            Self::LimitExceeded { kind, value, max } => {
                if *value == 0 {
                    write!(f, "{kind} limit must be at least 1")
                } else {
                    write!(f, "{kind} {value} exceeds maximum {max}")
                }
            }
            Self::UnknownImage { id } => write!(f, "image {id} is not registered"),
            Self::ContentOutsideSection => {
                f.write_str("body or decorations written both on the document and in sections")
            }
            Self::UnexpectedLineBreakInSingleLine { text } => {
                write!(f, "single-line text {text:?} contains a forced line break")
            }
            Self::Backend { message } => write!(f, "PDF backend failed: {message}"),
        }
    }
}

/// Why geometry was rejected. Point values are in pt.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum GeometryKind {
    /// A line width for text breaking is not finite and positive.
    LineWidth {
        /// The rejected width.
        width: f32,
    },
    /// The first-line indent leaves no positive width for the first line.
    FirstLineIndent {
        /// The indent.
        indent: f32,
        /// The paragraph width it was taken from.
        width: f32,
    },
    /// An inline advance, such as a blank's width, is negative or not finite.
    AdvanceWidth {
        /// The rejected advance.
        width: f32,
    },
    /// A line height factor is not finite and positive.
    LineHeightFactor {
        /// The rejected factor.
        factor: f32,
    },
    /// Margins leave no positive body width or height on the page.
    BodyFrame {
        /// Page width minus left and right margins.
        width: f32,
        /// Page height minus top and bottom margins.
        height: f32,
    },
    /// A layout frame has a non-finite or non-positive extent.
    LayoutFrame {
        /// Frame width.
        width: f32,
        /// Height left in the frame.
        height: f32,
        /// Height of a fresh frame.
        full_height: f32,
    },
    /// Content laid out at one width was offered a narrower frame.
    LaidOutWiderThanFrame {
        /// Width the content was laid out at.
        width: f32,
        /// Width of the frame offered later.
        avail: f32,
    },
    /// A fresh page placed nothing and the content did not report why.
    StalledOnFreshPage {
        /// Body height of the fresh page.
        height: f32,
    },
    /// A page break reached a nested frame, such as a column or a band.
    NestedPageBreak,
    /// A section has no pages, or the page count does not fit `u32`.
    PageCount {
        /// The page count, or the page index that overflowed.
        pages: usize,
    },
    /// Section page spans do not tile the page list in order.
    SectionSpans,
    /// A straight segment starts and ends at the same point.
    ZeroLengthSegment,
    /// A path has no drawable segment, such as a lone move-to.
    DegeneratePath,
    /// A rotation angle is NaN or infinite.
    NonFiniteRotation {
        /// The rejected angle in degrees.
        degrees: f32,
    },
    /// An ordered list number has no label. Letters stop at Z and roman numerals at 3999.
    MarkerOutOfRange {
        /// The item number.
        number: u64,
    },
    /// A custom list marker is empty.
    EmptyListMarker,
    /// A nested list indent is narrower than the marker column.
    ListIndent {
        /// The nest indent.
        indent: f32,
        /// The marker column width.
        marker_width: f32,
    },
    /// The list indent leaves no positive width for item bodies.
    ListIndentWiderThanFrame {
        /// The list indent.
        indent: f32,
        /// The frame width.
        avail: f32,
    },
}

impl fmt::Display for GeometryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineWidth { width } => write!(f, "line width {width} is not positive"),
            Self::FirstLineIndent { indent, width } => write!(
                f,
                "first-line indent {indent} leaves no room in width {width}"
            ),
            Self::AdvanceWidth { width } => write!(f, "advance {width} is negative or not finite"),
            Self::LineHeightFactor { factor } => {
                write!(f, "line height factor {factor} is not positive")
            }
            Self::BodyFrame { width, height } => write!(
                f,
                "margins leave a {width} x {height} body, which is not positive"
            ),
            Self::LayoutFrame {
                width,
                height,
                full_height,
            } => write!(
                f,
                "layout frame width {width}, height {height}, full height {full_height} is invalid"
            ),
            Self::LaidOutWiderThanFrame { width, avail } => write!(
                f,
                "content laid out at width {width} does not fit width {avail}"
            ),
            Self::StalledOnFreshPage { height } => {
                write!(
                    f,
                    "nothing was placed on a fresh page of body height {height}"
                )
            }
            Self::NestedPageBreak => f.write_str("a page break is not allowed in a nested frame"),
            Self::PageCount { pages } => {
                write!(f, "page count {pages} is zero or does not fit u32")
            }
            Self::SectionSpans => f.write_str("section page spans do not tile the pages"),
            Self::ZeroLengthSegment => f.write_str("segment starts and ends at the same point"),
            Self::DegeneratePath => f.write_str("path has no drawable segment"),
            Self::NonFiniteRotation { degrees } => {
                write!(f, "rotation {degrees} degrees is not finite")
            }
            Self::MarkerOutOfRange { number } => write!(f, "list number {number} has no label"),
            Self::EmptyListMarker => f.write_str("custom list marker is empty"),
            Self::ListIndent {
                indent,
                marker_width,
            } => write!(
                f,
                "list indent {indent} is narrower than marker width {marker_width}"
            ),
            Self::ListIndentWiderThanFrame { indent, avail } => {
                write!(f, "list indent {indent} leaves no room in width {avail}")
            }
        }
    }
}

/// Which column or track width rule failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ColumnWidthsReason {
    /// A ratio weight is NaN or infinite.
    NonFiniteRatio,
    /// A ratio weight is zero or negative.
    NonPositiveRatio,
    /// Too few columns, or the track count does not match the grid.
    ColumnCount,
    /// More than one Auto track.
    MoreThanOneAuto,
    /// Auto and Ratio tracks are mixed.
    AutoWithRatio,
    /// Fixed tracks are wider than the frame.
    FixedSumExceeds,
    /// Ratio or Auto tracks are left with no positive width.
    NoRemainingWidth,
    /// Gaps leave no positive width for the columns.
    Gap,
    /// Cell padding and border leave no positive content width.
    ContentWidth,
}

impl fmt::Display for ColumnWidthsReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NonFiniteRatio => "a ratio is not finite",
            Self::NonPositiveRatio => "a ratio is not positive",
            Self::ColumnCount => "column count does not match",
            Self::MoreThanOneAuto => "more than one auto track",
            Self::AutoWithRatio => "auto and ratio tracks are mixed",
            Self::FixedSumExceeds => "fixed tracks exceed the frame width",
            Self::NoRemainingWidth => "no width remains for flexible tracks",
            Self::Gap => "gaps leave no width for columns",
            Self::ContentWidth => "padding and border leave no content width",
        })
    }
}

/// How table spans fail to tile the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CellSpanReason {
    /// A span runs past the last column or the last row.
    OutOfGrid,
    /// A header span runs into the body rows.
    CrossesHeader,
    /// Two spans claim the same slot.
    Overlap,
    /// No cell covers this slot.
    EmptySlot,
}

impl fmt::Display for CellSpanReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutOfGrid => "span leaves the grid",
            Self::CrossesHeader => "header span runs into the body",
            Self::Overlap => "spans overlap",
            Self::EmptySlot => "slot is not covered",
        })
    }
}

/// Where layout was when an error happened. Unknown parts are `None`.
///
/// An inner layer records first and outer layers do not overwrite it, so
/// the `cell` field names the innermost table cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Location {
    /// Zero-based section index, in the order sections were added. A document
    /// without explicit sections has one section, index 0.
    pub section: Option<usize>,
    /// One-based page number across the whole document, as page numbers print.
    pub page: Option<u32>,
    /// Part of the page being laid out or painted.
    pub region: Option<Region>,
    /// Innermost table cell.
    pub cell: Option<CellIndex>,
}

impl Location {
    pub(crate) fn section(index: usize) -> Self {
        Self {
            section: Some(index),
            ..Self::default()
        }
    }

    pub(crate) fn page_index(index: usize) -> Self {
        Self {
            page: index.checked_add(1).and_then(|n| u32::try_from(n).ok()),
            ..Self::default()
        }
    }

    pub(crate) fn at_cell(row: usize, col: usize) -> Self {
        Self {
            cell: Some(CellIndex { row, col }),
            ..Self::default()
        }
    }

    pub(crate) fn with_region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(section) = self.section {
            parts.push(format!("section {section}"));
        }
        if let Some(page) = self.page {
            parts.push(format!("page {page}"));
        }
        if let Some(region) = self.region {
            parts.push(region.to_string());
        }
        if let Some(CellIndex { row, col }) = self.cell {
            parts.push(format!("cell row {row} column {col}"));
        }
        f.write_str(&parts.join(", "))
    }
}

/// Part of a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Region {
    /// The body frame inside the margins.
    Body,
    /// The top margin band.
    Header,
    /// The bottom margin band.
    Footer,
    /// The layer drawn under everything else.
    Behind,
    /// The layer drawn over everything else.
    Front,
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Body => "body",
            Self::Header => "header",
            Self::Footer => "footer",
            Self::Behind => "behind layer",
            Self::Front => "front layer",
        })
    }
}

/// Zero-based table grid slot. Rows count header rows first, then body rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellIndex {
    /// Grid row.
    pub row: usize,
    /// Grid column.
    pub col: usize,
}

/// One failure from a call that can only fail one way.
///
/// Document assembly and [`crate::layout_spec`] return [`Result`], which is a list.
pub(crate) type Fallible<T> = std::result::Result<T, Error>;

const MAX_ERRORS: usize = 64;

/// One or more failures from one document build, in discovery order.
///
/// Never empty. At most 64 items are kept. [`truncated`](Self::truncated) reports
/// that more were found. Equality compares each item by [`Error::kind`] and
/// [`Error::at`] only.
#[derive(Debug, Clone)]
pub struct Errors {
    items: Vec<Error>,
    truncated: bool,
}

impl PartialEq for Errors {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl Errors {
    /// The first failure found.
    #[must_use]
    pub fn first(&self) -> &Error {
        self.items.first().expect("Errors keeps at least one item")
    }

    /// Kept failures, in discovery order.
    #[must_use]
    pub fn as_slice(&self) -> &[Error] {
        &self.items
    }

    /// Same as [`as_slice`](Self::as_slice)`.iter()`.
    pub fn iter(&self) -> std::slice::Iter<'_, Error> {
        self.items.iter()
    }

    /// More failures were found than kept.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

impl From<Error> for Errors {
    fn from(error: Error) -> Self {
        Self {
            items: vec![error],
            truncated: false,
        }
    }
}

impl<'a> IntoIterator for &'a Errors {
    type Item = &'a Error;
    type IntoIter = std::slice::Iter<'a, Error>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl IntoIterator for Errors {
    type Item = Error;
    type IntoIter = std::vec::IntoIter<Error>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.items.len() == 1 && !self.truncated {
            return write!(f, "{}", self.items[0]);
        }
        for (index, error) in self.items.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{}. {error}", index + 1)?;
        }
        if self.truncated {
            write!(f, "\nfurther errors omitted")?;
        }
        Ok(())
    }
}

impl StdError for Errors {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self.items.as_slice() {
            [only] => StdError::source(only),
            _ => None,
        }
    }
}

/// Result of running a document or writing its PDF.
pub type Result<T> = std::result::Result<T, Errors>;

/// Append-only accumulator. Cap, order, and truncation live here.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ErrorBag {
    items: Vec<Error>,
    seen: u64,
    unknown_fonts: Vec<u32>,
}

/// Position in a bag, for "anything since here?" and locating those items.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mark {
    index: usize,
    seen: u64,
}

/// Soft font failures. Layout records these and keeps scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontFault {
    /// `scalar` has no glyph in `font`.
    MissingGlyph {
        /// Raw font id.
        font: u32,
        /// Unmapped scalar.
        scalar: char,
    },
    /// Bold was requested and no bold face is registered.
    BoldFaceRequired,
    /// `id` is not in the font store.
    UnknownFont {
        /// Raw font id.
        id: u32,
    },
}

impl From<FontFault> for Error {
    fn from(fault: FontFault) -> Self {
        match fault {
            FontFault::MissingGlyph { font, scalar } => {
                ErrorKind::MissingGlyph { font, scalar }.into()
            }
            FontFault::BoldFaceRequired => ErrorKind::BoldFaceRequired.into(),
            FontFault::UnknownFont { id } => ErrorKind::UnknownFont { id }.into(),
        }
    }
}

impl ErrorBag {
    /// Appends `error` unless this id was already recorded as [`ErrorKind::UnknownFont`].
    ///
    /// A full bag counts the error and drops it.
    pub(crate) fn record(&mut self, error: impl Into<Error>) {
        let error = error.into();
        if let ErrorKind::UnknownFont { id } = error.kind {
            if self.unknown_fonts.contains(&id) {
                return;
            }
            self.unknown_fonts.push(id);
        }
        self.seen = self.seen.saturating_add(1);
        if self.items.len() < MAX_ERRORS {
            self.items.push(error);
        }
    }

    /// Appends `other` after this bag. A repeated [`ErrorKind::UnknownFont`] id is kept once.
    pub(crate) fn absorb(&mut self, other: Self) {
        let dropped = other.seen.saturating_sub(other.items.len() as u64);
        for error in other.items {
            self.record(error);
        }
        self.seen = self.seen.saturating_add(dropped);
    }

    pub(crate) fn mark(&self) -> Mark {
        Mark {
            index: self.items.len(),
            seen: self.seen,
        }
    }

    /// Nothing was recorded since `mark`. A repeated unknown-font id does not count.
    pub(crate) fn clean_since(&self, mark: Mark) -> bool {
        self.seen == mark.seen
    }

    /// Fills unknown location fields on items kept since `mark`.
    pub(crate) fn locate_since(&mut self, mark: Mark, at: &Location) {
        for error in self.items.iter_mut().skip(mark.index) {
            error.fill_missing(at);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.clean_since(Mark { index: 0, seen: 0 })
    }

    pub(crate) fn as_slice(&self) -> &[Error] {
        &self.items
    }

    /// `Ok(value)` when nothing was recorded.
    pub(crate) fn finish<T>(self, value: T) -> Result<T> {
        if self.is_empty() {
            debug_assert!(self.as_slice().is_empty());
            Ok(value)
        } else {
            Err(self.into_errors())
        }
    }

    /// Keeps `hard` as the stop reason.
    ///
    /// When the bag is already full, the last kept item is replaced and
    /// [`Errors::truncated`] stays set so the dropped item is still reported.
    pub(crate) fn fail(mut self, hard: Error) -> Errors {
        if self.items.len() >= MAX_ERRORS {
            let last = self.items.len() - 1;
            self.note_unknown_font(&hard);
            self.items[last] = hard;
            if self.seen <= self.items.len() as u64 {
                self.seen = self.items.len() as u64 + 1;
            }
        } else {
            self.record(hard);
        }
        self.into_errors()
    }

    fn note_unknown_font(&mut self, error: &Error) {
        if let ErrorKind::UnknownFont { id } = error.kind {
            if !self.unknown_fonts.contains(&id) {
                self.unknown_fonts.push(id);
            }
        }
    }

    fn into_errors(self) -> Errors {
        Errors {
            truncated: self.seen > self.items.len() as u64,
            items: self.items,
        }
    }
}

impl From<Errors> for ErrorBag {
    fn from(errors: Errors) -> Self {
        let mut bag = Self::default();
        let truncated = errors.truncated;
        for error in errors.items {
            bag.record(error);
        }
        if truncated && bag.seen <= bag.items.len() as u64 {
            bag.seen = bag.items.len() as u64 + 1;
        }
        bag
    }
}

/// At most 24 scalars, then `...` when the source is longer.
pub(crate) fn text_snippet(text: &str) -> String {
    const MAX_CHARS: usize = 24;
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index == MAX_CHARS {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_the_missing_scalar_and_the_text_fragment() {
        assert_eq!(
            Error::from(ErrorKind::MissingGlyph {
                font: 0,
                scalar: '中',
            })
            .to_string(),
            "font 0 has no glyph for '中' (U+4E2D)"
        );
        assert_eq!(
            Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: Some("hello".to_owned()),
                width: 5.0,
                avail: 4.0,
            })
            .to_string(),
            "unbreakable content \"hello\" width 5 exceeds available width 4"
        );
        assert_eq!(
            Error::from(ErrorKind::UnexpectedLineBreakInSingleLine {
                text: "A\nB".to_owned(),
            })
            .to_string(),
            "single-line text \"A\\nB\" contains a forced line break"
        );
        assert_eq!(text_snippet(&"字".repeat(30)).chars().count(), 27);
    }

    #[test]
    fn display_appends_the_known_location() {
        let error = Error::from(ErrorKind::DecorationTallerThanMargin {
            height: 30.0,
            margin: 20.0,
        })
        .in_region(Region::Header)
        .on_page_index(2)
        .in_section(1);
        assert_eq!(
            error.to_string(),
            "decoration height 30 exceeds margin band 20 (section 1, page 3, header)"
        );
        assert_eq!(
            Error::from(GeometryKind::ZeroLengthSegment).to_string(),
            "invalid geometry: segment starts and ends at the same point"
        );
    }

    #[test]
    fn inner_location_wins_over_outer() {
        let error = Error::from(ErrorKind::EmptyLink)
            .in_cell(1, 2)
            .in_cell(0, 0)
            .on_page_index(0)
            .on_page_index(4);
        assert_eq!(
            error.at,
            Some(Location {
                section: None,
                page: Some(1),
                region: None,
                cell: Some(CellIndex { row: 1, col: 2 }),
            })
        );
    }

    #[test]
    fn equality_ignores_the_source() {
        let plain = Error::from(ErrorKind::Backend {
            message: "m".to_owned(),
        });
        let chained = plain.clone().with_source(std::fmt::Error);
        assert_eq!(plain, chained);
        assert!(plain.source().is_none());
        assert!(chained.source().is_some());
    }

    fn kind(n: u32) -> Error {
        ErrorKind::UnknownImage { id: n }.into()
    }

    #[test]
    fn bag_keeps_sixty_four_and_marks_truncation() {
        let mut bag = ErrorBag::default();
        for n in 0..65 {
            bag.record(kind(n));
        }
        let errors = bag.finish(()).unwrap_err();
        assert_eq!(errors.as_slice().len(), 64);
        assert!(errors.truncated());
        assert_eq!(errors.first(), &kind(0));
        assert_eq!(errors.as_slice()[63], kind(63));
        assert!(errors.as_slice().iter().all(|error| error != &kind(64)));
    }

    #[test]
    fn absorb_keeps_discovery_order() {
        let mut first = ErrorBag::default();
        first.record(kind(1));
        let mut second = ErrorBag::default();
        second.record(kind(2));
        second.record(kind(3));
        first.absorb(second);
        let errors = first.finish(()).unwrap_err();
        assert_eq!(errors.as_slice(), &[kind(1), kind(2), kind(3)]);
        assert!(!errors.truncated());
    }

    #[test]
    fn equality_ignores_truncation() {
        let mut truncated_bag = ErrorBag::default();
        for n in 0..65 {
            truncated_bag.record(kind(n));
        }
        let truncated = truncated_bag.finish(()).unwrap_err();
        assert!(truncated.truncated());
        let mut same = ErrorBag::default();
        for error in truncated.as_slice() {
            same.record(error.clone());
        }
        let kept = same.finish(()).unwrap_err();
        assert!(!kept.truncated());
        assert_eq!(kept, truncated);
    }

    #[test]
    fn from_error_is_one_item() {
        let error = Error::from(ErrorKind::EmptyLink);
        let errors = Errors::from(error.clone());
        assert_eq!(errors.as_slice(), std::slice::from_ref(&error));
        assert!(!errors.truncated());
        assert_eq!(errors.to_string(), error.to_string());
    }

    #[test]
    fn display_numbers_more_than_one_item() {
        let mut bag = ErrorBag::default();
        bag.record(ErrorKind::EmptyLink);
        bag.record(ErrorKind::BoldFaceRequired);
        let errors = bag.finish(()).unwrap_err();
        assert_eq!(
            errors.to_string(),
            "1. link URI is empty\n2. bold text needs a registered bold font"
        );
    }

    #[test]
    fn fail_on_a_full_bag_replaces_the_last_item() {
        let mut bag = ErrorBag::default();
        for n in 0..64 {
            bag.record(kind(n));
        }
        let errors = bag.fail(Error::from(ErrorKind::EmptyLink));
        assert_eq!(errors.as_slice().len(), 64);
        assert!(errors.truncated());
        assert_eq!(errors.as_slice()[63], Error::from(ErrorKind::EmptyLink));
        assert_eq!(errors.first(), &kind(0));
    }

    #[test]
    fn unknown_font_is_recorded_once_per_id() {
        let mut bag = ErrorBag::default();
        bag.record(FontFault::UnknownFont { id: 0 });
        bag.record(ErrorKind::UnknownFont { id: 0 });
        bag.record(FontFault::UnknownFont { id: 1 });
        bag.record(FontFault::MissingGlyph {
            font: 0,
            scalar: 'A',
        });
        bag.record(FontFault::MissingGlyph {
            font: 0,
            scalar: 'A',
        });
        let errors = bag.finish(()).unwrap_err();
        assert_eq!(
            errors.as_slice(),
            &[
                Error::from(ErrorKind::UnknownFont { id: 0 }),
                Error::from(ErrorKind::UnknownFont { id: 1 }),
                Error::from(ErrorKind::MissingGlyph {
                    font: 0,
                    scalar: 'A'
                }),
                Error::from(ErrorKind::MissingGlyph {
                    font: 0,
                    scalar: 'A'
                }),
            ]
        );
        assert!(!errors.truncated());
    }

    #[test]
    fn one_item_delegates_source_and_many_do_not() {
        let chained = Error::from(ErrorKind::Backend {
            message: "m".to_owned(),
        })
        .with_source(std::fmt::Error);
        let one = Errors::from(chained);
        assert!(one.source().is_some());
        let mut bag = ErrorBag::default();
        bag.record(ErrorKind::EmptyLink);
        bag.record(ErrorKind::BoldFaceRequired);
        let many = bag.finish(()).unwrap_err();
        assert!(many.source().is_none());
    }
}
