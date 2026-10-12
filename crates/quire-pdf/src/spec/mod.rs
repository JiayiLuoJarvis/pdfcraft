//! Data-only document specifications and their layout entry point.
//!
//! # Overview
//!
//! | Type | Role |
//! | --- | --- |
//! | [`DocumentSpec`] | Page default, [`Resources`], [`crate::Limits`], sections |
//! | [`SectionSpec`] | Margins, bands, layers, flow [`Block`] list |
//! | [`Resources`] | Body/bold font bytes; [`Resources::add_image`] → [`ImageRef`] |
//! | [`Block`] | Paragraphs, tables, columns, lists, images, rules, … |
//! | [`BandSpec`] / [`TextRun`] | Header/footer; page numbers resolved at paint time |
//! | [`LayerMark`] / [`PageWhen`] | Page-anchored marks (`behind` / `front`) |
//! | [`layout_spec`] | Lower + lay out → [`Rendered`] |
//!
//! Callers assemble these values with ordinary Rust control flow. There is no
//! template language and no public `Flowable` trait.
//!
//! See the crate README and `docs/guide.md` for end-to-end usage.

mod lower;
mod token;
mod types;

#[cfg(test)]
mod tests;

pub use lower::layout_spec;
pub use types::{Numbering, Ratio, Rendered, Track};

use std::num::{NonZeroU16, NonZeroU32};

use crate::canvas::ImageFormat;
use crate::geom::{Color, Ink, Length, Margins, NonNegative, PageSize, Pen, Point, Positive, Rect};
use crate::limits::Limits;

use self::token::DocToken;

/// A complete data-only document description.
#[derive(Debug)]
#[must_use]
pub struct DocumentSpec {
    /// Default paper for sections that do not override it.
    pub page_size: PageSize,
    /// Fonts and images used by the document.
    pub resources: Resources,
    /// Resource and document-size caps.
    pub limits: Limits,
    /// Sections in document order. An empty list produces one empty default section.
    pub sections: Vec<SectionSpec>,
}

impl DocumentSpec {
    /// Starts a document with one default section.
    pub fn new(page_size: PageSize, resources: Resources) -> Self {
        Self {
            page_size,
            resources,
            limits: Limits::default(),
            sections: vec![SectionSpec::default()],
        }
    }
}

/// Fonts and encoded images owned by a specification.
#[derive(Debug)]
#[must_use]
pub struct Resources {
    /// Body font bytes. The first face is used by normal text.
    pub body_font: Vec<u8>,
    /// Optional face used by bold paragraph runs.
    pub bold_font: Option<Vec<u8>>,
    token: DocToken,
    images: Vec<ImageResource>,
}

impl Default for Resources {
    fn default() -> Self {
        Self {
            body_font: Vec::new(),
            bold_font: None,
            token: DocToken::next(),
            images: Vec::new(),
        }
    }
}

impl Resources {
    /// Creates resources with `body_font` as the normal text face.
    pub fn new(body_font: Vec<u8>) -> Self {
        Self {
            body_font,
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
#[derive(Debug, Default)]
#[must_use]
pub struct SectionSpec {
    /// Paper override. `None` uses [`DocumentSpec::page_size`].
    pub page_size: Option<PageSize>,
    /// Page margins and header/footer band heights.
    pub margins: Margins,
    /// Header rendered on every page in this section.
    pub header: Option<BandSpec>,
    /// Footer rendered on every page in this section.
    pub footer: Option<BandSpec>,
    /// Marks painted below the header and body.
    pub behind: Vec<LayerMark>,
    /// Marks painted above the footer and body.
    pub front: Vec<LayerMark>,
    /// Flow blocks in document order.
    pub body: Vec<Block>,
}

/// A flow block in a section, column, list item, or keep-together group.
#[derive(Debug)]
#[must_use]
pub enum Block {
    /// A wrapping paragraph.
    Paragraph(ParagraphSpec),
    /// Vertical blank space.
    Spacer(Length<NonNegative>),
    /// Starts following content on a new page.
    PageBreak,
    /// Keeps all children on one page.
    KeepTogether(Vec<Self>),
    /// Applies a parent text style to its children.
    Styled {
        /// Style patch inherited by descendants.
        style: TextStyle,
        /// Styled blocks.
        children: Vec<Self>,
    },
    /// Side-by-side columns.
    Columns(ColumnsSpec),
    /// A table.
    Table(TableSpec),
    /// A bulleted or numbered list.
    List(ListSpec),
    /// A flow image.
    Image(ImageSpec),
    /// A horizontal rule.
    HLine(HLineSpec),
}

impl Block {
    /// A paragraph containing one normal-face text run.
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::Paragraph(ParagraphSpec::text(text))
    }
}

/// Parent text properties used by [`Block::Styled`] and layer text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[must_use]
pub struct TextStyle {
    /// Font-size override.
    pub font_size: Option<Length<Positive>>,
    /// Text-color override.
    pub color: Option<Color>,
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
pub struct ParagraphSpec {
    /// Inline content in order.
    pub runs: Vec<InlineRun>,
    /// First-line inset.
    pub first_line_indent: Length<NonNegative>,
    /// Line alignment.
    pub align: Alignment,
    /// Paragraph color override.
    pub color: Option<Color>,
    /// Paragraph font-size override.
    pub font_size: Option<Length<Positive>>,
}

impl Default for ParagraphSpec {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            first_line_indent: NonNegative::ZERO,
            align: Alignment::Start,
            color: None,
            font_size: None,
        }
    }
}

impl ParagraphSpec {
    /// Creates a paragraph containing one normal-face text run.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            runs: vec![InlineRun::Text(text.into())],
            ..Self::default()
        }
    }
}

/// One paragraph run.
#[derive(Debug)]
#[must_use]
pub enum InlineRun {
    /// Normal-face text.
    Text(String),
    /// Bold-face text.
    Bold(String),
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
pub struct ColumnSpec {
    /// Fixed, ratio, or auto width.
    pub width: Track,
    /// Blocks in the column.
    pub blocks: Vec<Block>,
}

/// Side-by-side columns.
#[derive(Debug)]
#[must_use]
pub struct ColumnsSpec {
    /// Gap between adjacent columns.
    pub gap: Length<NonNegative>,
    /// Columns from left to right.
    pub columns: Vec<ColumnSpec>,
}

/// Table rows and presentation.
#[derive(Debug)]
#[must_use]
pub struct TableSpec {
    /// Header rows, repeated when [`Self::repeat_header`] is true.
    pub header: Vec<Vec<CellSpec>>,
    /// Body rows.
    pub body: Vec<Vec<CellSpec>>,
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

impl Default for TableSpec {
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
pub struct CellSpec {
    /// Allowed cell children in vertical order.
    pub content: Vec<CellBlock>,
    /// Horizontal span.
    pub col_span: NonZeroU16,
    /// Vertical span.
    pub row_span: NonZeroU16,
    /// Optional background fill.
    pub fill: Option<Color>,
}

impl Default for CellSpec {
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
pub enum CellBlock {
    /// Plain text in the table's inherited style.
    Text(String),
    /// A paragraph.
    Paragraph(ParagraphSpec),
    /// A horizontal rule.
    HLine(HLineSpec),
    /// A flow image.
    Image(ImageSpec),
    /// Vertical blank space.
    Spacer(Length<NonNegative>),
    /// A nested list.
    List(ListSpec),
}

/// Bulleted or numbered list settings.
#[derive(Debug)]
#[must_use]
pub struct ListSpec {
    /// Marker kind.
    pub kind: ListKind,
    /// First number for an ordered list.
    pub start: NonZeroU32,
    /// Hanging marker-column width.
    pub marker_width: Length<Positive>,
    /// Extra nested-body inset. `None` uses `marker_width`.
    pub nest_indent: Option<Length<Positive>>,
    /// List items in order.
    pub items: Vec<ListItemSpec>,
}

impl Default for ListSpec {
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
pub struct ListItemSpec {
    /// Custom marker. `None` uses the list's generated marker.
    pub marker: Option<String>,
    /// Item blocks.
    pub blocks: Vec<Block>,
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
#[derive(Debug, Default)]
#[must_use]
pub struct BandSpec {
    /// Horizontal inset rule.
    pub inset: BandInset,
    /// Start-aligned slot.
    pub left: Option<BandSlot>,
    /// End-aligned slot.
    pub right: Option<BandSlot>,
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
pub enum BandSlot {
    /// Unlinked text.
    Text(Vec<TextRun>),
    /// Text linked to a URI assembled from page-time runs.
    Link {
        /// URI runs.
        uri: Vec<TextRun>,
        /// Visible text runs.
        text: Vec<TextRun>,
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
pub enum LayerMark {
    /// One unwrapped text line.
    Text {
        /// Page selection.
        when: PageWhen,
        /// Page-time text runs.
        text: Vec<TextRun>,
        /// Line-box top-left in page coordinates.
        at: Point,
        /// Text style.
        style: TextStyle,
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
        /// Shapes in paint order.
        shapes: Vec<LayerShape>,
    },
}

/// One shape allowed in a rotated layer group.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum LayerShape {
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
}
