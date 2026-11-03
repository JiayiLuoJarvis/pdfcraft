//! Data-only document specifications and their layout entry point.
//!
//! # Overview
//!
//! | Type | Role |
//! | --- | --- |
//! | [`DocumentSpec`] | Page default, [`Resources`], [`crate::Limits`], [`DocumentInfo`], sections |
//! | [`SectionSpec`] | Margins, bands, layers, flow [`Block`] list |
//! | [`Resources`] | [`FontRegistry`] and [`Resources::add_image`] → [`ImageRef`] |
//! | [`Block`] | Paragraphs, tables, columns, lists, images, rules, … |
//! | [`BandFn`] / [`BandSpec`] / [`TextRun`] | Header/footer; fixed or per page |
//! | [`LayerMark`] / [`PageWhen`] | Page-anchored marks (`behind` / `front`) |
//! | [`layout_spec`] | Lower + lay out → [`Rendered`] |
//!
//! Callers assemble these values with ordinary Rust control flow. There is no
//! template language and no public `Flowable` trait.
//!
//! See the crate README and `docs/guide.md` for end-to-end usage.

mod depth;
mod latin;
mod lower;
mod token;
mod types;

#[cfg(test)]
mod tests;

#[doc(inline)]
pub use crate::canvas::DocumentInfo;
#[doc(inline)]
pub use crate::fonts::{FontFace, FontFamily, FontRegistry};
#[doc(inline)]
pub use lower::layout_spec;
#[doc(inline)]
pub use types::{Numbering, Ratio, Rendered, Track};

use std::fmt;
use std::num::{NonZeroU16, NonZeroU32};

use crate::canvas::ImageFormat;
use crate::error::Error;
use crate::geom::{
    Color, Finite, Ink, Length, Margins, NonNegative, PageSize, Path, Pen, Point, Positive, Rect,
    Size,
};
use crate::limits::Limits;

use self::token::DocToken;

/// A complete data-only document description.
#[derive(Debug)]
#[must_use]
pub struct DocumentSpec<K> {
    /// Default paper for sections that do not override it.
    pub page_size: PageSize,
    /// Fonts and images used by the document.
    pub resources: Resources<K>,
    /// Resource and document-size caps.
    pub limits: Limits,
    /// PDF document information.
    ///
    /// Non-empty `title` and `author` become the PDF title and author. Empty
    /// strings are omitted. The creator is `pdfcraft`. Producer is not set.
    pub info: DocumentInfo,
    /// Sections in document order. An empty list produces one empty default section.
    pub sections: Vec<SectionSpec<K>>,
}

impl<K> DocumentSpec<K> {
    /// Starts a document with one default section.
    pub fn new(page_size: PageSize, resources: Resources<K>) -> Self {
        Self {
            page_size,
            resources,
            limits: Limits::default(),
            info: DocumentInfo::default(),
            sections: vec![SectionSpec::default()],
        }
    }
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DocumentSpec<()>>();
};

/// Fonts and encoded images owned by a specification.
#[derive(Debug)]
#[must_use]
pub struct Resources<K> {
    fonts: FontRegistry<K>,
    token: DocToken,
    images: Vec<ImageResource>,
}

impl<K> Default for Resources<K> {
    fn default() -> Self {
        Self {
            fonts: FontRegistry::new(),
            token: DocToken::next(),
            images: Vec::new(),
        }
    }
}

impl<K> Resources<K> {
    /// Creates resources that draw text with `fonts`.
    ///
    /// An empty registry is valid when the document has no text.
    pub fn with_fonts(fonts: FontRegistry<K>) -> Self {
        Self {
            fonts,
            ..Self::default()
        }
    }

    /// Stores an encoded image and returns its specification-local handle.
    ///
    /// The bytes are not decoded here. A malformed image, an unsupported format,
    /// or a [`Limits`] cap is reported by [`layout_spec`] as
    /// [`ErrorKind::Image`](crate::ErrorKind::Image) or
    /// [`ErrorKind::LimitExceeded`](crate::ErrorKind::LimitExceeded).
    pub fn add_image(&mut self, bytes: Vec<u8>, format: ImageFormat) -> ImageRef {
        let index = u32::try_from(self.images.len()).unwrap_or(u32::MAX);
        self.images.push(ImageResource { bytes, format });
        ImageRef {
            resources: self.token,
            index,
        }
    }
}

#[derive(Debug)]
struct ImageResource {
    bytes: Vec<u8>,
    format: ImageFormat,
}

/// A specification-local image handle returned by [`Resources::add_image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct ImageRef {
    resources: DocToken,
    index: u32,
}

/// One section with its own page setup, decorations, and body.
#[derive(Debug)]
#[must_use]
pub struct SectionSpec<K> {
    /// Paper override. `None` uses [`DocumentSpec::page_size`].
    pub page_size: Option<PageSize>,
    /// Page margins and header/footer band heights.
    pub margins: Margins,
    /// Header for this section. `None` is not called.
    pub header: Option<BandFn<K>>,
    /// Footer for this section. `None` is not called.
    pub footer: Option<BandFn<K>>,
    /// Marks painted below the header and body.
    pub behind: Vec<LayerMark<K>>,
    /// Marks painted above the footer and body.
    pub front: Vec<LayerMark<K>>,
    /// Flow blocks in document order.
    pub body: Vec<Block<K>>,
}

impl<K> Default for SectionSpec<K> {
    fn default() -> Self {
        Self {
            page_size: None,
            margins: Margins::default(),
            header: None,
            footer: None,
            behind: Vec::new(),
            front: Vec::new(),
            body: Vec::new(),
        }
    }
}

/// Outline bookmark contributed by a [`Block::Anchor`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct OutlineEntry {
    /// Non-empty bookmark title (byte-non-empty; spaces allowed).
    pub title: String,
    /// Outline depth. First outline item in the document must be `1`.
    /// Each following item may be at most one deeper than the previous (`level <= prev + 1`),
    /// and may jump up by any number of levels (e.g. `3` → `1`).
    pub level: u8,
}

/// A flow block in a section, column, list item, or keep-together group.
#[derive(Debug)]
#[must_use]
pub enum Block<K> {
    /// A wrapping paragraph.
    Paragraph(ParagraphSpec<K>),
    /// Vertical blank space.
    Spacer(Length<NonNegative>),
    /// Starts following content on a new page.
    PageBreak,
    /// Keeps all children on one page.
    KeepTogether(Vec<Self>),
    /// Applies a parent run style to its children.
    ///
    /// Paragraph runs inherit every field. Table cell text and list markers
    /// inherit only size and color. Bands, layers, and canvas text do not see it.
    Styled {
        /// Style patch inherited by descendants.
        style: RunStyle,
        /// Styled blocks.
        children: Vec<Self>,
    },
    /// Marks a destination for in-document links and optional outline entries.
    Anchor {
        /// Document-unique, non-empty id (byte-non-empty; spaces allowed).
        id: String,
        /// When `Some`, contributes one outline item in document order.
        outline: Option<OutlineEntry>,
        /// Wrapped flow content. Destination is the first placed fragment of this child.
        child: Box<Self>,
    },
    /// Side-by-side columns.
    Columns(ColumnsSpec<K>),
    /// A table.
    Table(TableSpec<K>),
    /// A bulleted or numbered list.
    List(ListSpec<K>),
    /// A flow image.
    Image(ImageSpec),
    /// A fixed-size vector block. Marks occupy `size` and do not split.
    Canvas(CanvasSpec<K>),
    /// A horizontal rule.
    HLine(HLineSpec),
}

impl<K> Block<K> {
    /// A paragraph containing one text run in `font`.
    pub fn paragraph(font: K, text: impl Into<String>) -> Self {
        Self::Paragraph(ParagraphSpec::text(font, text))
    }
}

/// Inline text properties. `None` inherits the enclosing value.
///
/// Body runs resolve in this order, later `Some` values winning: the document
/// seed (12 pt, black, regular, no lines, no tracking), each enclosing
/// [`Block::Styled`] from outside in, [`ParagraphSpec::font_size`] and
/// [`ParagraphSpec::color`], then the run's own style. `Some(false)` turns off
/// a flag an outer style turned on.
///
/// Header, footer, layer, and canvas text start from the seed and apply only
/// their own style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[must_use]
pub struct RunStyle {
    /// Use the family's bold face. Missing it fails with [`crate::ErrorKind::BoldFaceRequired`].
    pub bold: Option<bool>,
    /// Use the family's italic face. Missing it fails with
    /// [`crate::ErrorKind::ItalicFaceRequired`]. Bold and italic together need
    /// [`FontFamily::bold_italic`], or fail with
    /// [`crate::ErrorKind::BoldItalicFaceRequired`].
    pub italic: Option<bool>,
    /// Draw a 1 pt underline. The position comes from the regular face's `post` table
    /// and scales with the run's font size.
    pub underline: Option<bool>,
    /// Draw a 1 pt strikethrough. The position comes from the regular face's `OS/2`
    /// table and scales with the run's font size.
    pub strikethrough: Option<bool>,
    /// Extra space after each grapheme cluster that is followed by more text on
    /// the same line. May be negative. Counts toward line breaking.
    pub tracking: Option<Length<Finite>>,
    /// Text and decoration color.
    pub color: Option<Color>,
    /// Font size. Decoration position scales with it. Thickness stays 1 pt.
    pub font_size: Option<Length<Positive>>,
}

/// Horizontal alignment within the current frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[must_use]
pub enum Alignment {
    /// Align to the start edge.
    #[default]
    Start,
    /// Center in the frame.
    Center,
    /// Align to the end edge.
    End,
    /// Two-sided alignment. Paragraph lines distribute remainder; Image / `HLine` /
    /// Canvas treat this as [`Self::Start`].
    Justify,
}

/// A paragraph and its inline runs.
#[derive(Debug)]
#[must_use]
pub struct ParagraphSpec<K> {
    /// Inline content in order.
    pub runs: Vec<InlineRun<K>>,
    /// First-line inset.
    pub first_line_indent: Length<NonNegative>,
    /// Line alignment.
    pub align: Alignment,
    /// Paragraph color override, applied over [`Block::Styled`] and under each run.
    pub color: Option<Color>,
    /// Paragraph font-size override, applied over [`Block::Styled`] and under each run.
    ///
    /// Paragraph size. Em blanks use it. A line of only blanks uses the first text
    /// run's resolved face at this size. A paragraph with no text runs uses
    /// `size * line-height factor` and does not read a font. A line with glyphs
    /// scales each face by that face's own size.
    pub font_size: Option<Length<Positive>>,
    /// Line-height factor override. `None` keeps the document seed (1.2).
    pub line_height_factor: Option<f32>,
}

impl<K> Default for ParagraphSpec<K> {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            first_line_indent: NonNegative::ZERO,
            align: Alignment::Start,
            color: None,
            font_size: None,
            line_height_factor: None,
        }
    }
}

impl<K> ParagraphSpec<K> {
    /// Creates a paragraph containing one text run in `font`.
    pub fn text(font: K, text: impl Into<String>) -> Self {
        Self {
            runs: vec![InlineRun::Text {
                text: text.into(),
                font,
                style: RunStyle::default(),
            }],
            ..Self::default()
        }
    }
}

/// One paragraph run.
#[derive(Debug)]
#[must_use]
pub enum InlineRun<K> {
    /// Text in a face of the keyed family, chosen by the resolved bold and italic flags.
    ///
    /// An empty string is skipped and needs no face.
    Text {
        /// Run text.
        text: String,
        /// Key in the document [`FontRegistry`].
        font: K,
        /// Patch over the inherited style.
        style: RunStyle,
    },
    /// Runs linked to a URI or in-document anchor.
    Link {
        /// `Uri` or `Anchor` at the call site. Layout may replace `Anchor` with `Xyz` before paint.
        target: crate::canvas::LinkTarget,
        /// Linked runs.
        runs: Vec<Self>,
    },
    /// A fixed, em-based, or fill-to-end blank.
    Blank(BlankSpec),
}

/// Width and stroke properties for an inline blank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct BlankSpec {
    /// Blank-width rule.
    pub width: BlankWidth,
    /// Stroke thickness. `None` keeps the paragraph default.
    pub thickness: Option<Length<Positive>>,
    /// Stroke color. `None` inherits the paragraph color.
    pub color: Option<Color>,
    /// Distance below the baseline. `None` keeps the paragraph default.
    pub baseline_offset: Option<Length<NonNegative>>,
}

/// Width rule for an inline blank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum BlankWidth {
    /// Fixed width.
    Fixed(Length<NonNegative>),
    /// A multiple of the paragraph font size.
    Em(u16),
    /// Remaining width on the visual line.
    ToEndOfLine,
}

/// A column and its flow content.
#[derive(Debug)]
#[must_use]
pub struct ColumnSpec<K> {
    /// Fixed, ratio, or auto width.
    pub width: Track,
    /// Blocks in the column.
    pub blocks: Vec<Block<K>>,
}

/// Side-by-side columns.
#[derive(Debug)]
#[must_use]
pub struct ColumnsSpec<K> {
    /// Gap between adjacent columns.
    pub gap: Length<NonNegative>,
    /// Columns from left to right.
    pub columns: Vec<ColumnSpec<K>>,
}

/// Table rows and presentation.
#[derive(Debug)]
#[must_use]
pub struct TableSpec<K> {
    /// Header rows, repeated when [`Self::repeat_header`] is true.
    pub header: Vec<TableRow<K>>,
    /// Body rows.
    pub body: Vec<TableRow<K>>,
    /// Whether header rows repeat after a page break.
    pub repeat_header: bool,
    /// Optional per-column tracks.
    pub tracks: Option<Vec<Track>>,
    /// Padding on every side of every cell.
    pub cell_padding: Length<NonNegative>,
    /// Grid-line border. Zero width omits the border.
    pub border: TableBorder,
}

impl<K> Default for TableSpec<K> {
    fn default() -> Self {
        Self {
            header: Vec::new(),
            body: Vec::new(),
            repeat_header: true,
            tracks: None,
            cell_padding: NonNegative::ZERO,
            border: TableBorder::default(),
        }
    }
}

/// One table row.
#[derive(Debug, Default)]
#[must_use]
pub struct TableRow<K> {
    /// Cells in the row, from left to right.
    pub cells: Vec<CellSpec<K>>,
}

/// Grid-line stroke centered on cell edges. Width zero draws nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
#[must_use]
pub struct TableBorder {
    /// Grid-line width. Zero omits the border.
    pub width: Length<NonNegative>,
    /// Grid-line color.
    pub color: Color,
}

impl Default for TableBorder {
    fn default() -> Self {
        Self {
            width: NonNegative::ZERO,
            color: Color::rgb(0, 0, 0),
        }
    }
}

/// One table cell.
#[derive(Debug)]
#[must_use]
pub struct CellSpec<K> {
    /// Allowed cell children in vertical order.
    pub content: Vec<CellBlock<K>>,
    /// Horizontal span.
    pub col_span: NonZeroU16,
    /// Vertical span.
    pub row_span: NonZeroU16,
    /// Optional background fill.
    pub fill: Option<Color>,
}

impl<K> Default for CellSpec<K> {
    fn default() -> Self {
        Self {
            content: Vec::new(),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
            fill: None,
        }
    }
}

/// A child allowed inside a table cell.
#[derive(Debug)]
#[must_use]
pub enum CellBlock<K> {
    /// Plain text in `font`.
    Text {
        /// Cell text.
        text: String,
        /// Key in the document [`FontRegistry`].
        font: K,
    },
    /// A paragraph.
    Paragraph(ParagraphSpec<K>),
    /// A horizontal rule.
    HLine(HLineSpec),
    /// A flow image.
    Image(ImageSpec),
    /// A fixed-size vector block.
    Canvas(CanvasSpec<K>),
    /// Vertical blank space.
    Spacer(Length<NonNegative>),
    /// A nested list.
    List(ListSpec<K>),
    /// Marks a destination inside a table cell.
    Anchor {
        /// Document-unique, non-empty id.
        id: String,
        /// Optional outline entry.
        outline: Option<OutlineEntry>,
        /// Wrapped cell content.
        child: Box<Self>,
    },
}

/// Suffix or parentheses on a generated ordered-list number.
///
/// Bullets and custom markers ignore this. Each variant combines with every
/// [`Numbering`] value. Letters still stop at Z and roman numerals at 3999.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub enum MarkerFormat {
    /// `1.`, `C.`, `iv.`
    #[default]
    DotSuffix,
    /// `1、`, `A、`. The comma is U+3001.
    IdeographicComma,
    /// `1)`, `a)`.
    ClosingParen,
    /// `(1)`, `(iv)`.
    Parens,
    /// `1`, `z`.
    Bare,
}

/// Where a list marker sits in the marker column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub enum MarkerAlign {
    /// The left edge of the marker column.
    #[default]
    Start,
    /// The right edge of the marker column, so `9.` and `10.` share that edge.
    End,
}

/// Bulleted or numbered list settings.
///
/// On an item's first fragment, the marker baseline matches the first text
/// baseline of that item's first block. A paragraph contributes its first
/// line. A table contributes the first physical row. That row is the header
/// when the table has one, and a cell that starts there counts even when it
/// spans later rows. Columns contribute the topmost first line. A nested list
/// contributes its first marker line. An image, a canvas, a horizontal rule,
/// or a spacer taller than zero has no text baseline, so the marker's line
/// box sits on the item top. A zero-height spacer, an empty stack, and an
/// empty keep-together group are skipped. [`Block::Styled`] passes the rule
/// through to its children. A continuation fragment draws no marker.
#[derive(Debug)]
#[must_use]
pub struct ListSpec<K> {
    /// Marker kind.
    pub kind: ListKind,
    /// First number for an ordered list.
    pub start: NonZeroU32,
    /// Hanging marker-column width.
    pub marker_width: Length<Positive>,
    /// Body column left edge, measured from the list's left edge.
    ///
    /// `None` uses `marker_width`. The value must not be less than `marker_width`.
    pub body_indent: Option<Length<Positive>>,
    /// Space between adjacent items. The default is zero.
    ///
    /// The value is added to the previous item's trailing spacing. Nothing is
    /// added before the first item or after the last item. When the gap does not
    /// fit in the remaining height, it is dropped. It is not carried to the next
    /// page.
    pub item_gap: Length<NonNegative>,
    /// Suffix or parentheses for generated numbers. Bullets ignore this.
    pub marker_format: MarkerFormat,
    /// Face for bullets, generated numbers, and custom markers.
    ///
    /// `None` uses the first registered face. `Some` uses that family's regular
    /// face. Size and color still follow [`Block::Styled`]. Bold and italic do not.
    pub marker_font: Option<K>,
    /// Marker position in the marker column.
    pub marker_align: MarkerAlign,
    /// List items in order.
    pub items: Vec<ListItemSpec<K>>,
}

impl<K> Default for ListSpec<K> {
    fn default() -> Self {
        Self {
            kind: ListKind::Bullet,
            start: NonZeroU32::MIN,
            marker_width: Length::from_trusted_pt(18.0),
            body_indent: None,
            item_gap: NonNegative::ZERO,
            marker_format: MarkerFormat::default(),
            marker_font: None,
            marker_align: MarkerAlign::default(),
            items: Vec::new(),
        }
    }
}

/// A list marker rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum ListKind {
    /// Bullet markers.
    Bullet,
    /// Generated numbering.
    Ordered(Numbering),
}

/// One list item.
#[derive(Debug)]
#[must_use]
pub struct ListItemSpec<K> {
    /// Custom marker. `None` uses the list's generated marker.
    ///
    /// Bullets, generated numbers, and custom markers use [`ListSpec::marker_font`]
    /// when it is set, and otherwise the first registered face. They take size
    /// and color from [`Block::Styled`] and stay on the regular face.
    pub marker: Option<String>,
    /// Item blocks.
    ///
    /// Any block except [`Block::PageBreak`], at any depth inside the item, is
    /// allowed. A list inside a table cell also keeps the cell's rules for every
    /// item: no tables, columns, or keep-together groups.
    pub blocks: Vec<Block<K>>,
}

/// A vector block that occupies `size` in the flow.
///
/// The origin is the block's top-left. X increases to the right and y increases
/// downward. Marks are painted in order. Ink outside `size` is still painted.
/// The block does not split.
#[derive(Debug)]
#[must_use]
pub struct CanvasSpec<K> {
    /// Occupied width and height. Both are strictly positive.
    pub size: Size,
    /// Horizontal placement in the current frame.
    pub align: Alignment,
    /// Drawing commands. Later marks paint over earlier ones.
    pub marks: Vec<CanvasMark<K>>,
}

/// One drawing command inside a [`CanvasSpec`].
#[derive(Debug)]
#[must_use]
pub enum CanvasMark<K> {
    /// A straight stroke from `from` to `to`.
    Line {
        /// Start, block coordinates.
        from: Point,
        /// End, block coordinates.
        to: Point,
        /// Stroke color and width.
        pen: Pen,
    },
    /// An axis-aligned rectangle.
    Rect {
        /// Box. Width and height are positive.
        rect: Rect,
        /// Fill, stroke, or both.
        ink: Ink,
    },
    /// A filled path. The path is not stroked.
    Fill {
        /// Path in block coordinates.
        path: Path,
        /// Fill color.
        color: Color,
    },
    /// One unwrapped line. `at` is the baseline left, not the line-box top.
    Text {
        /// Text. An empty string is rejected at layout.
        text: String,
        /// Key in the document [`FontRegistry`].
        font: K,
        /// Baseline left, block coordinates, y down.
        at: Point,
        /// Patch over the document seed (12 pt, black), not an enclosing [`Block::Styled`].
        style: RunStyle,
    },
}

/// A flow image and its display box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct ImageSpec {
    /// Image resource.
    pub image: ImageRef,
    /// Display width. `None` derives it or uses pixel width.
    pub width: Option<Length<Positive>>,
    /// Display height. `None` derives it or uses pixel height.
    pub height: Option<Length<Positive>>,
    /// Horizontal placement.
    pub align: Alignment,
}

/// A horizontal rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct HLineSpec {
    /// Rule width. `None` fills the frame.
    pub width: Option<Length<Positive>>,
    /// Horizontal placement for a shorter rule.
    pub align: Alignment,
    /// Stroke thickness.
    pub thickness: Length<Positive>,
    /// Stroke color.
    pub color: Color,
    /// Blank space above the rule.
    pub space_before: Length<NonNegative>,
    /// Blank space below the rule.
    pub space_after: Length<NonNegative>,
}

impl Default for HLineSpec {
    fn default() -> Self {
        Self {
            width: None,
            align: Alignment::Center,
            thickness: Length::from_trusted_pt(1.0),
            color: Color::rgb(0, 0, 0),
            space_before: NonNegative::ZERO,
            space_after: NonNegative::ZERO,
        }
    }
}

/// One page passed to a header or footer function.
///
/// The library builds this after the body is paginated. Callers only read it.
/// Margins stay on the section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub struct PageBandQuery {
    /// One-based page number across the whole document.
    pub number: NonZeroU32,
    /// Page count of the whole document.
    pub count: NonZeroU32,
    /// One-based page number inside the current section.
    pub section_number: NonZeroU32,
    /// Page count of the current section.
    pub section_count: NonZeroU32,
    /// Final page width and height.
    ///
    /// A [`PageSize::Paper`] value includes orientation.
    /// [`PageSize::Custom`] is already final.
    pub size: Size,
}

type BandCallback<K> =
    Box<dyn FnMut(&PageBandQuery) -> Result<Option<BandSpec<K>>, Error> + Send + Sync>;

enum BandFnInner<K> {
    Fixed(BandSpec<K>),
    Dynamic(BandCallback<K>),
}

/// Header or footer for one section.
///
/// [`Self::fixed`] uses one [`BandSpec`] on every page. [`Self::from_fn`] is
/// called once per page after the body is paginated. `None` on
/// [`SectionSpec::header`] or [`SectionSpec::footer`] means that side does not
/// exist and the function is not called.
///
/// `Ok(None)` skips painting that side on that page. The margin stays, and the
/// body does not move.
#[must_use]
pub struct BandFn<K> {
    inner: BandFnInner<K>,
}

impl<K> fmt::Debug for BandFn<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BandFn")
    }
}

impl<K> BandFn<K> {
    /// Uses `spec` on every page of the section.
    ///
    /// Font keys are checked once while [`layout_spec`] assembles the section,
    /// not once per page.
    ///
    /// # Errors
    ///
    /// This function does not return [`Result`]. A missing
    /// font is reported by [`layout_spec`] for the section, with no page number.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub fn fixed(spec: BandSpec<K>) -> Self {
        Self {
            inner: BandFnInner::Fixed(spec),
        }
    }

    /// Calls `f` once per page, header before footer, in page order.
    ///
    /// `Ok(None)` paints nothing on that side for that page. `Ok(Some(spec))`
    /// resolves fonts for that page. `Err` stops later pages. The body is
    /// paginated first, so a body failure never calls `f`.
    ///
    /// # Errors
    ///
    /// `f` may return [`Error::band_fn`](crate::Error::band_fn) or any other
    /// [`Error`]. [`layout_spec`] keeps a page `f` already stored on the error
    /// and fills the section, page, and header or footer region that are still
    /// empty.
    ///
    /// # Panics
    ///
    /// A panic inside `f` aborts [`layout_spec`]. The library does not catch it.
    pub fn from_fn<F>(f: F) -> Self
    where
        K: 'static,
        F: FnMut(&PageBandQuery) -> Result<Option<BandSpec<K>>, Error> + Send + Sync + 'static,
    {
        Self {
            inner: BandFnInner::Dynamic(Box::new(f)),
        }
    }
}

/// Header or footer content.
#[derive(Debug, Clone)]
#[must_use]
pub struct BandSpec<K> {
    /// Horizontal inset rule.
    pub inset: BandInset,
    /// Start-aligned slot.
    pub left: Option<BandSlot<K>>,
    /// End-aligned slot.
    pub right: Option<BandSlot<K>>,
    /// Absolute marks in band coordinates, painted before the slots.
    ///
    /// The origin is the band top-left. A box outside the band fails layout.
    /// The library does not clip the mark and does not grow the margin.
    pub marks: Vec<BandMark<K>>,
}

impl<K> Default for BandSpec<K> {
    fn default() -> Self {
        Self {
            inset: BandInset::default(),
            left: None,
            right: None,
            marks: Vec::new(),
        }
    }
}

/// One absolute mark inside a header or footer band.
///
/// Coordinates are the band top-left. There is no page filter on the mark.
/// Choose the page in [`BandFn::from_fn`].
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub enum BandMark<K> {
    /// One unrotated shape, text line, or image.
    Shape(LayerShape<K>),
    /// Shapes rotated as one group.
    Rotated {
        /// Clockwise angle in degrees.
        degrees: f32,
        /// Rotation pivot in band coordinates.
        pivot: Point,
        /// Shapes in paint order.
        shapes: Vec<LayerShape<K>>,
    },
}

/// Horizontal inset for a band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum BandInset {
    /// Explicit left and right insets.
    Fixed {
        /// Left inset.
        left: Length<NonNegative>,
        /// Right inset.
        right: Length<NonNegative>,
    },
    /// Use the current page's left and right margins.
    PageMargins,
}

impl Default for BandInset {
    fn default() -> Self {
        Self::Fixed {
            left: NonNegative::ZERO,
            right: NonNegative::ZERO,
        }
    }
}

/// Text or linked text in one band slot.
#[derive(Debug, Clone)]
#[must_use]
pub enum BandSlot<K> {
    /// Unlinked text.
    Text {
        /// Page-time runs.
        runs: Vec<TextRun>,
        /// Key in the document [`FontRegistry`].
        font: K,
        /// Patch over the document seed.
        style: RunStyle,
    },
    /// Text linked to a URI assembled from page-time runs.
    Link {
        /// URI runs.
        uri: Vec<TextRun>,
        /// Visible text runs.
        text: Vec<TextRun>,
        /// Key in the document [`FontRegistry`].
        font: K,
        /// Patch over the document seed.
        style: RunStyle,
    },
}

/// One page-time text fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum TextRun {
    /// Literal text.
    Literal(String),
    /// One-based document page number.
    PageNumber,
    /// Document page count.
    PageCount,
    /// One-based page number within the section.
    SectionNumber,
    /// Page count within the section.
    SectionCount,
}

/// Pages on which a layer mark appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum PageWhen {
    /// Every page in the section.
    All,
    /// Only the final page of the whole document.
    Last,
}

/// One absolute page mark.
#[derive(Debug)]
#[must_use]
pub enum LayerMark<K> {
    /// One unwrapped text line.
    Text {
        /// Page selection.
        when: PageWhen,
        /// Page-time text runs.
        text: Vec<TextRun>,
        /// Key in the document [`FontRegistry`].
        font: K,
        /// Line-box top-left in page coordinates.
        at: Point,
        /// Patch over the document seed, not an enclosing [`Block::Styled`].
        style: RunStyle,
    },
    /// An image stretched into a page-coordinate rectangle.
    Image {
        /// Page selection.
        when: PageWhen,
        /// Image resource.
        image: ImageRef,
        /// Destination rectangle.
        rect: Rect,
    },
    /// A straight segment.
    Line {
        /// Page selection.
        when: PageWhen,
        /// Start point.
        from: Point,
        /// End point.
        to: Point,
        /// Stroke.
        pen: Pen,
    },
    /// A rectangle.
    Rect {
        /// Page selection.
        when: PageWhen,
        /// Geometry.
        rect: Rect,
        /// Fill and/or stroke.
        ink: Ink,
    },
    /// An ellipse.
    Ellipse {
        /// Page selection.
        when: PageWhen,
        /// Bounding rectangle.
        rect: Rect,
        /// Fill and/or stroke.
        ink: Ink,
    },
    /// Shapes rotated as one group.
    Rotated {
        /// Page selection.
        when: PageWhen,
        /// Clockwise angle in degrees.
        degrees: f32,
        /// Rotation pivot.
        pivot: Point,
        /// Shapes in paint order. May mix path, text, and image.
        shapes: Vec<LayerShape<K>>,
    },
}

/// One shape allowed in a rotated layer group.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum LayerShape<K> {
    /// A straight segment.
    Line {
        /// Start point.
        from: Point,
        /// End point.
        to: Point,
        /// Stroke.
        pen: Pen,
    },
    /// A rectangle.
    Rect {
        /// Geometry.
        rect: Rect,
        /// Fill and/or stroke.
        ink: Ink,
    },
    /// An ellipse.
    Ellipse {
        /// Bounding rectangle.
        rect: Rect,
        /// Fill and/or stroke.
        ink: Ink,
    },
    /// One unwrapped text line. Coordinates are pre-rotation page space.
    Text {
        /// Page-time text runs.
        text: Vec<TextRun>,
        /// Key in the document [`FontRegistry`].
        font: K,
        /// Line-box top-left before rotation.
        at: Point,
        /// Patch over the document seed.
        style: RunStyle,
    },
    /// An image stretched into a pre-rotation page rectangle.
    Image {
        /// Image resource.
        image: ImageRef,
        /// Destination rectangle before rotation.
        rect: Rect,
    },
}
