//! Data-only document specifications and their layout entry point.

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

#[cfg(not(feature = "backend-krilla"))]
use crate::canvas::RejectingImageRegistry;
use crate::canvas::{ImageFormat, ImageRegistry, RecordingCanvas};
use crate::document::{
    default_style, Cell, DocToken, Flow, FlowWriter, HLine, Image, ImageBox, List, ListBlock,
    Numbering, Paragraph, Rendered, Section, Spacer, TextProps, Track,
};
use crate::error::{Error, ErrorBag, ErrorKind, Errors, Location, Result};
use crate::fonts::{FontSource, FontStore};
use crate::geom::{Color, Ink, Length, Margins, NonNegative, PageSize, Pen, Point, Positive, Rect};
use crate::layout::flow::check_limits;
use crate::layout::frame::LayoutCtx;
use crate::layout::paginate::{layout_pages, paint_laid, SectionJob};
use crate::limits::Limits;

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
#[derive(Debug, Default)]
#[must_use]
pub struct Resources {
    /// Body font bytes. The first face is used by normal text.
    pub body_font: Vec<u8>,
    /// Optional face used by bold paragraph runs.
    pub bold_font: Option<Vec<u8>>,
    images: Vec<ImageResource>,
}

impl Resources {
    /// Creates resources with `body_font` as the normal text face.
    pub fn new(body_font: Vec<u8>) -> Self {
        Self {
            body_font,
            bold_font: None,
            images: Vec::new(),
        }
    }

    /// Stores an encoded image and returns its specification-local handle.
    pub fn add_image(&mut self, bytes: Vec<u8>, format: ImageFormat) -> ImageRef {
        let index = u32::try_from(self.images.len()).unwrap_or(u32::MAX);
        self.images.push(ImageResource { bytes, format });
        ImageRef(index)
    }
}

#[derive(Debug)]
struct ImageResource {
    bytes: Vec<u8>,
    format: ImageFormat,
}

/// A specification-local image handle returned by [`Resources::add_image`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct ImageRef(u32);

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

/// Lowers a data-only specification and records its laid-out pages.
///
/// # Errors
///
/// Returns resource, construction, layout, or drawing failures collected for the document.
pub fn layout_spec(spec: DocumentSpec) -> Result<Rendered> {
    let DocumentSpec {
        page_size,
        resources,
        limits,
        mut sections,
    } = spec;
    if sections.is_empty() {
        sections.push(SectionSpec::default());
    }

    let doc = DocToken::next();
    let mut errors = ErrorBag::default();
    let (mut fonts, images, image_handles) = lower_resources(resources, limits, doc, &mut errors);
    let style = default_style();
    let mut configs = Vec::with_capacity(sections.len());
    let mut roots = Vec::with_capacity(sections.len());
    let mut decorations = Vec::with_capacity(sections.len());

    for (index, spec) in sections.into_iter().enumerate() {
        let mark = errors.mark();
        let section = lower_section(spec, page_size, style.clone(), doc, &image_handles);
        let (config, root, decoration, section_errors) = section.lower(page_size, doc);
        errors.absorb(section_errors);
        errors.locate_since(mark, &Location::section(index));
        configs.push(config);
        roots.push(root);
        decorations.push(decoration);
    }

    if let Err(hard) = check_limits(&roots, &limits) {
        return Err(errors.fail(hard));
    }
    let jobs = configs
        .into_iter()
        .zip(roots)
        .zip(&decorations)
        .map(|((config, root), decoration)| SectionJob {
            config,
            root,
            decorations: decoration,
        })
        .collect();
    let laid = {
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        layout_pages(jobs, &mut ctx, limits.max_pages())
    };
    let laid = match laid {
        Ok(laid) => laid,
        Err(hard) => return Err(errors.fail(hard)),
    };
    let laid = errors.finish(laid)?;
    let mut recording = RecordingCanvas::new();
    paint_laid(&laid, &mut recording).map_err(Errors::from)?;
    Ok(Rendered {
        recording,
        fonts,
        images,
    })
}

fn lower_resources(
    resources: Resources,
    limits: Limits,
    doc: DocToken,
    errors: &mut ErrorBag,
) -> (FontStore, Box<dyn ImageRegistry>, Vec<Image>) {
    let mut fonts = FontStore::new();
    fonts.set_max_bytes(limits.max_font_bytes());
    match fonts.register(FontSource {
        bytes: Arc::new(resources.body_font),
        index: 0,
    }) {
        Ok(_) => {}
        Err(error) => errors.record(error),
    }
    if let Some(bytes) = resources.bold_font {
        match fonts.register(FontSource {
            bytes: Arc::new(bytes),
            index: 0,
        }) {
            Ok(id) => fonts.set_bold(id),
            Err(error) => errors.record(error),
        }
    }

    let mut registry = image_registry();
    registry.set_limits(&limits);
    let mut handles = Vec::with_capacity(resources.images.len());
    for image in resources.images {
        match registry.register(image.bytes, image.format) {
            Ok(decoded) => handles.push(Image { decoded, doc }),
            Err(error) => errors.record(error),
        }
    }
    (fonts, registry, handles)
}

#[cfg(feature = "backend-krilla")]
fn image_registry() -> Box<dyn ImageRegistry> {
    Box::new(crate::backend::ImageTable::default())
}

#[cfg(not(feature = "backend-krilla"))]
fn image_registry() -> Box<dyn ImageRegistry> {
    Box::new(RejectingImageRegistry)
}

fn lower_section(
    spec: SectionSpec,
    default_page: PageSize,
    style: crate::layout::style::Style,
    doc: DocToken,
    images: &[Image],
) -> Section<'static> {
    let SectionSpec {
        page_size,
        margins,
        header,
        footer,
        behind,
        front,
        body,
    } = spec;
    let mut section = Section::new(style, doc);
    section.page_size(page_size.unwrap_or(default_page));
    section.margins(margins);
    if let Some(spec) = header {
        section.header(move |band| apply_band(&spec, band));
    }
    if let Some(spec) = footer {
        section.footer(move |band| apply_band(&spec, band));
    }
    if !behind.is_empty() {
        let handles = images.to_vec();
        section.behind(move |layer| apply_layer(&behind, &handles, layer));
    }
    if !front.is_empty() {
        let handles = images.to_vec();
        section.front(move |layer| apply_layer(&front, &handles, layer));
    }
    lower_blocks(body, &mut section.flow, images);
    section
}

fn lower_blocks(blocks: Vec<Block>, flow: &mut Flow, images: &[Image]) {
    for block in blocks {
        match block {
            Block::Paragraph(spec) => flow.paragraph(move |paragraph| {
                apply_paragraph(spec, paragraph);
            }),
            Block::Spacer(height) => flow.add(Spacer::new(height)),
            Block::PageBreak => flow.page_break(),
            Block::KeepTogether(children) => {
                flow.keep_together(|nested| lower_blocks(children, nested, images));
            }
            Block::Styled { style, children } => {
                flow.stack(
                    |props| apply_text_style(style, props),
                    |nested| lower_blocks(children, nested, images),
                );
            }
            Block::Columns(spec) => lower_columns(spec, flow, images),
            Block::Table(spec) => lower_table(spec, flow, images),
            Block::List(spec) => lower_list(spec, flow, images),
            Block::Image(spec) => match image_for(spec.image, images) {
                Ok(image) => flow.image(image, |box_| apply_image_box(spec, box_)),
                Err(error) => flow.push_err(error),
            },
            Block::HLine(spec) => flow.hline(|line| apply_hline(spec, line)),
        }
    }
}

fn apply_paragraph(spec: ParagraphSpec, paragraph: &mut Paragraph) {
    paragraph.first_line_indent(spec.first_line_indent);
    apply_alignment(spec.align, paragraph);
    if let Some(color) = spec.color {
        paragraph.color(color);
    }
    if let Some(size) = spec.font_size {
        paragraph.font_size(size);
    }
    apply_inline_runs(spec.runs, paragraph);
}

fn apply_inline_runs(runs: Vec<InlineRun>, paragraph: &mut Paragraph) {
    for run in runs {
        match run {
            InlineRun::Text(text) => paragraph.text(text),
            InlineRun::Bold(text) => paragraph.bold(text),
            InlineRun::Link { uri, runs } => {
                paragraph.link(uri, move |linked| apply_inline_runs(runs, linked));
            }
            InlineRun::Blank(spec) => {
                let mut blank = match spec.width {
                    BlankWidth::Fixed(width) => paragraph.blank(width),
                    BlankWidth::Em(ems) => paragraph.blank_em(ems),
                    BlankWidth::ToEndOfLine => paragraph.blank_to_end(),
                };
                if let Some(thickness) = spec.thickness {
                    blank.thickness(thickness);
                }
                if let Some(color) = spec.color {
                    blank.color(color);
                }
                if let Some(offset) = spec.baseline_offset {
                    blank.baseline_offset(offset);
                }
            }
        }
    }
}

fn apply_alignment(align: Alignment, paragraph: &mut Paragraph) {
    match align {
        Alignment::Start => {}
        Alignment::Center => paragraph.center(),
        Alignment::End => paragraph.end(),
    }
}

fn apply_text_style(style: TextStyle, props: &mut TextProps) {
    if let Some(size) = style.font_size {
        props.font_size(size);
    }
    if let Some(color) = style.color {
        props.color(color);
    }
}

fn lower_columns(spec: ColumnsSpec, flow: &mut Flow, images: &[Image]) {
    flow.columns(|columns| {
        columns.gap(spec.gap);
        for column in spec.columns {
            let blocks = column.blocks;
            match column.width {
                Track::Fixed(width) => {
                    columns.col_fixed(width, |child| lower_blocks(blocks, child, images));
                }
                Track::Ratio(ratio) => {
                    columns.col_ratio(ratio.get(), |child| lower_blocks(blocks, child, images));
                }
                Track::Auto => {
                    columns.col_auto(|child| lower_blocks(blocks, child, images));
                }
            }
        }
    });
}

fn lower_table(spec: TableSpec, flow: &mut Flow, images: &[Image]) {
    flow.table(|table| {
        table.repeat_header(spec.repeat_header);
        if let Some(tracks) = spec.tracks {
            table.tracks(tracks);
        }
        table.cell_padding(spec.cell_padding);
        table.border(spec.border_width, spec.border_color);
        for row in spec.header {
            table.header(row.into_iter().map(|cell| lower_cell(cell, images)));
        }
        for row in spec.body {
            table.row(row.into_iter().map(|cell| lower_cell(cell, images)));
        }
    });
}

fn lower_cell(spec: CellSpec, images: &[Image]) -> Cell<'_> {
    let mut cell = Cell::empty();
    for block in spec.content {
        cell = match block {
            CellBlock::Text(text) => cell.add(Paragraph::build(|p| p.text(text))),
            CellBlock::Paragraph(spec) => cell.add(Paragraph::build(|paragraph| {
                apply_paragraph(spec, paragraph);
            })),
            CellBlock::HLine(spec) => cell.add(HLine::build(|line| apply_hline(spec, line))),
            CellBlock::Image(spec) => match image_for(spec.image, images) {
                Ok(image) => cell.add(crate::document::PlacedImage::build(image, |box_| {
                    apply_image_box(spec, box_);
                })),
                Err(_) => cell,
            },
            CellBlock::Spacer(height) => cell.add(Spacer::new(height)),
            CellBlock::List(spec) => cell.add(list_block(spec, images)),
        };
    }
    let mut cell = cell.col_span(spec.col_span).row_span(spec.row_span);
    if let Some(fill) = spec.fill {
        cell = cell.fill(fill);
    }
    cell
}

fn lower_list(spec: ListSpec, flow: &mut Flow, images: &[Image]) {
    match spec.kind {
        ListKind::Bullet => flow.ul(|list| apply_list(spec, list, images)),
        ListKind::Ordered(numbering) => {
            flow.ol(numbering, |list| apply_list(spec, list, images));
        }
    }
}

fn list_block(spec: ListSpec, images: &[Image]) -> ListBlock<'_> {
    match spec.kind {
        ListKind::Bullet => ListBlock::bullet(move |list| apply_list(spec, list, images)),
        ListKind::Ordered(numbering) => {
            ListBlock::numbered(numbering, move |list| apply_list(spec, list, images))
        }
    }
}

fn apply_list(spec: ListSpec, list: &mut List, images: &[Image]) {
    list.start(spec.start);
    list.marker_width(spec.marker_width);
    if let Some(indent) = spec.nest_indent {
        list.nest_indent(indent);
    }
    for item in spec.items {
        match item.marker {
            Some(marker) => {
                list.item_marked(marker, |flow| lower_blocks(item.blocks, flow, images));
            }
            None => list.item(|flow| lower_blocks(item.blocks, flow, images)),
        }
    }
}

fn apply_image_box(spec: ImageSpec, box_: &mut ImageBox) {
    if let Some(width) = spec.width {
        box_.width(width);
    }
    if let Some(height) = spec.height {
        box_.height(height);
    }
    match spec.align {
        Alignment::Start => {}
        Alignment::Center => box_.center(),
        Alignment::End => box_.end(),
    }
}

fn apply_hline(spec: HLineSpec, line: &mut HLine) {
    if let Some(width) = spec.width {
        line.width(width);
    }
    line.thickness(spec.thickness);
    line.color(spec.color);
    line.space_before(spec.space_before);
    line.space_after(spec.space_after);
    match spec.align {
        Alignment::Start => line.start(),
        Alignment::Center => {}
        Alignment::End => line.end(),
    }
}

fn apply_band(spec: &BandSpec, band: &mut crate::document::Band) {
    match spec.inset {
        BandInset::Fixed { left, right } => band.inset(left, right),
        BandInset::PageMargins => {
            let margins = band.page().margins;
            band.inset(margins.left(), margins.right());
        }
    }
    let page = band.page();
    if let Some(slot) = &spec.left {
        apply_band_slot(slot, &page, true, band);
    }
    if let Some(slot) = &spec.right {
        apply_band_slot(slot, &page, false, band);
    }
}

fn apply_band_slot(
    slot: &BandSlot,
    page: &crate::document::Page,
    left: bool,
    band: &mut crate::document::Band,
) {
    match slot {
        BandSlot::Text(runs) => {
            let text = render_text(runs, page);
            if left {
                band.left(text);
            } else {
                band.right(text);
            }
        }
        BandSlot::Link { uri, text } => {
            let uri = render_text(uri, page);
            let text = render_text(text, page);
            if left {
                band.left_link(uri, text);
            } else {
                band.right_link(uri, text);
            }
        }
    }
}

fn render_text(runs: &[TextRun], page: &crate::document::Page) -> String {
    let mut text = String::new();
    for run in runs {
        match run {
            TextRun::Literal(value) => text.push_str(value),
            TextRun::PageNumber => text.push_str(&page.number.to_string()),
            TextRun::PageCount => text.push_str(&page.count.to_string()),
            TextRun::SectionNumber => text.push_str(&page.section_number.to_string()),
            TextRun::SectionCount => text.push_str(&page.section_count.to_string()),
        }
    }
    text
}

fn apply_layer(marks: &[LayerMark], images: &[Image], layer: &mut crate::document::Layer) {
    let page = layer.page();
    for mark in marks {
        let when = match mark {
            LayerMark::Text { when, .. }
            | LayerMark::Image { when, .. }
            | LayerMark::Line { when, .. }
            | LayerMark::Rect { when, .. }
            | LayerMark::Ellipse { when, .. }
            | LayerMark::Rotated { when, .. } => *when,
        };
        if !when.matches(&page) {
            continue;
        }
        match mark {
            LayerMark::Text {
                text, at, style, ..
            } => {
                let mut text_at = layer.text_at(render_text(text, &page), *at);
                if let Some(size) = style.font_size {
                    text_at.font_size(size);
                }
                if let Some(color) = style.color {
                    text_at.color(color);
                }
            }
            LayerMark::Image { image, rect, .. } => {
                if let Ok(image) = image_for(*image, images) {
                    layer.image_at(image, *rect);
                }
            }
            LayerMark::Line { from, to, pen, .. } => layer.line(*from, *to, *pen),
            LayerMark::Rect { rect, ink, .. } => layer.rect(*rect, *ink),
            LayerMark::Ellipse { rect, ink, .. } => layer.ellipse(*rect, *ink),
            LayerMark::Rotated {
                degrees,
                pivot,
                shapes,
                ..
            } => layer.rotated(*degrees, *pivot, |group| {
                for shape in shapes {
                    match shape {
                        LayerShape::Line { from, to, pen } => group.line(*from, *to, *pen),
                        LayerShape::Rect { rect, ink } => group.rect(*rect, *ink),
                        LayerShape::Ellipse { rect, ink } => group.ellipse(*rect, *ink),
                    }
                }
            }),
        }
    }
}

impl PageWhen {
    fn matches(self, page: &crate::document::Page) -> bool {
        match self {
            Self::All => true,
            Self::Last => page.is_last(),
        }
    }
}

fn image_for(image: ImageRef, images: &[Image]) -> std::result::Result<&Image, Error> {
    images
        .get(image.0 as usize)
        .ok_or_else(|| Error::from(ErrorKind::UnknownImage { id: image.0 }))
}
