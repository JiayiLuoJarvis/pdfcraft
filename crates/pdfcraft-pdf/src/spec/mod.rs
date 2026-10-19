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
//! | [`BandSpec`] / [`TextRun`] | Header/footer; page numbers resolved at paint time |
//! | [`LayerMark`] / [`PageWhen`] | Page-anchored marks (`behind` / `front`) |
//! | [`layout_spec`] | Lower + lay out → [`Rendered`] |
//!
//! Callers assemble these values with ordinary Rust control flow. There is no
//! template language and no public `Flowable` trait.
//!
//! See the crate README and `docs/guide.md` for end-to-end usage.

mod latin;
mod lower;
mod token;
mod types;

#[cfg(test)]
mod tests;

pub use crate::canvas::DocumentInfo;
pub use crate::fonts::{FontFace, FontFamily, FontRegistry};
pub use lower::layout_spec;
pub use types::{Numbering, Ratio, Rendered, Track};

use std::num::{NonZeroU16, NonZeroU32};

use crate::canvas::ImageFormat;
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
    /// Header rendered on every page in this section.
    pub header: Option<BandSpec<K>>,
    /// Footer rendered on every page in this section.
    pub footer: Option<BandSpec<K>>,
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
    /// Runs linked to a URI.
    Link {
        /// Non-empty URI.
        uri: String,
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
    pub header: Vec<Vec<CellSpec<K>>>,
    /// Body rows.
    pub body: Vec<Vec<CellSpec<K>>>,
    /// Whether header rows repeat after a page break.
    pub repeat_header: bool,
    /// Optional per-column tracks.
    pub tracks: Option<Vec<Track>>,
    /// Padding on every side of every cell.
    pub cell_padding: Length<NonNegative>,
    /// Grid-line width. Zero omits the border.
    pub border_width: Length<NonNegative>,
    /// Grid-line color.
    pub border_color: Color,
}

impl<K> Default for TableSpec<K> {
    fn default() -> Self {
        Self {
            header: Vec::new(),
            body: Vec::new(),
            repeat_header: true,
            tracks: None,
            cell_padding: NonNegative::ZERO,
            border_width: NonNegative::ZERO,
            border_color: Color::rgb(0, 0, 0),
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
}

/// Bulleted or numbered list settings.
#[derive(Debug)]
#[must_use]
pub struct ListSpec<K> {
    /// Marker kind.
    pub kind: ListKind,
    /// First number for an ordered list.
    pub start: NonZeroU32,
    /// Hanging marker-column width.
    pub marker_width: Length<Positive>,
    /// Extra nested-body inset. `None` uses `marker_width`.
    pub nest_indent: Option<Length<Positive>>,
    /// List items in order.
    pub items: Vec<ListItemSpec<K>>,
}

impl<K> Default for ListSpec<K> {
    fn default() -> Self {
        Self {
            kind: ListKind::Bullet,
            start: NonZeroU32::MIN,
            marker_width: Positive::pt(18.0).expect("18 pt is positive"),
            nest_indent: None,
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
    /// Generated and custom markers use the first registered face. They take
    /// size and color from [`Block::Styled`] and nothing else.
    pub marker: Option<String>,
    /// Item blocks.
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
            thickness: Positive::pt(1.0).expect("1 pt is positive"),
            color: Color::rgb(0, 0, 0),
            space_before: NonNegative::ZERO,
            space_after: NonNegative::ZERO,
        }
    }
}

/// Header or footer slots.
#[derive(Debug)]
#[must_use]
pub struct BandSpec<K> {
    /// Horizontal inset rule.
    pub inset: BandInset,
    /// Start-aligned slot.
    pub left: Option<BandSlot<K>>,
    /// End-aligned slot.
    pub right: Option<BandSlot<K>>,
}

impl<K> Default for BandSpec<K> {
    fn default() -> Self {
        Self {
            inset: BandInset::default(),
            left: None,
            right: None,
        }
    }
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
#[derive(Debug)]
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
