//! Public document construction surface (Rust DSL).
//!
//! [`document`] runs the caller's closure once to assemble the body flow.
//! Header, footer, behind, and front closures run again on every page after
//! pagination, so only they see page numbers.

use std::num::{NonZeroU16, NonZeroU32};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::canvas::{Canvas, DecodedImage, ImageFormat, LinkTarget, RecordedPage, RecordingCanvas};
use crate::error::{Error, Result};
use crate::fonts::{FontId, FontSource, FontStore};
use crate::geom::{
    Color, Ink, Length, Margins, NonNegative, PageSize, Path, Pen, Point, Positive, Rect, Size,
};
use crate::layout::columns::{default_column_gap, ColumnWidth, ColumnsNode, Ratio};
use crate::layout::decorate::{
    BandContent, Decorations, LayerContent, LayerItem, LayerText, PageInfo,
};
use crate::layout::flow::{FlowNode, KeepTogetherNode, LayoutCtx, SpacerNode};
use crate::layout::hline::HLineNode;
use crate::layout::image::ImageNode;
use crate::layout::list::{ListItem, ListKind, ListNode, ListSpec, MarkerKind};
use crate::layout::paginate::{paginate_sections, PaginateConfig, SectionJob};
use crate::layout::paragraph::{BlankWidth, Face, InlineBlank, InlineItem, TextNode, TextRun};
use crate::layout::style::{Style, TextAlign};
use crate::layout::table::{self, TableNode, TableSpec};

#[cfg(feature = "backend-krilla")]
use crate::backend::ImageTable;

/// Lays out a document whose default paper is `page_size`.
///
/// `build` runs once. Builder mistakes are deferred: the first one is
/// returned from here, before any page is drawn. A missing glyph anywhere,
/// including headers and layers, also fails the whole document.
///
/// Text uses the first font passed to [`Document::font`] at 12 pt with line
/// height 1.2, black, start-aligned. Margins default to zero.
pub fn document<'a>(
    page_size: PageSize,
    build: impl FnOnce(&mut Document<'a>) -> Result<()>,
) -> Result<Rendered> {
    let mut doc = Document::new(page_size);
    build(&mut doc)?;
    let mut recording = RecordingCanvas::new();
    let painted = doc.paint(&mut recording)?;
    Ok(Rendered {
        recording,
        fonts: painted.fonts,
        #[cfg(feature = "backend-krilla")]
        images: painted.images,
    })
}

type BandHook<'a> = Box<dyn Fn(&mut Band) + 'a>;
type LayerHook<'a> = Box<dyn Fn(&mut Layer) + 'a>;

/// Implicit lead section, or the explicit sections that replaced it.
enum Sections<'a> {
    Implicit(Section<'a>),
    Explicit {
        sections: Vec<Section<'a>>,
        /// Body pushes after the first `section` are rejected.
        closed: Flow,
    },
}

/// One run of pages with its own paper, margins, and page decorations.
pub struct Section<'a> {
    page_size: Option<PageSize>,
    margins: Margins,
    flow: Flow,
    header: Option<BandHook<'a>>,
    footer: Option<BandHook<'a>>,
    behind: Option<LayerHook<'a>>,
    front: Option<LayerHook<'a>>,
}

impl<'a> Section<'a> {
    fn new(style: Style, doc: DocToken) -> Self {
        Self {
            page_size: None,
            margins: Margins::default(),
            flow: Flow::new(style, doc),
            header: None,
            footer: None,
            behind: None,
            front: None,
        }
    }

    fn written(&self) -> bool {
        self.flow.written()
            || self.margins != Margins::default()
            || self.header.is_some()
            || self.footer.is_some()
            || self.behind.is_some()
            || self.front.is_some()
    }

    /// Sets this section's paper. Otherwise the document default is used.
    pub fn page_size(&mut self, page_size: PageSize) {
        self.page_size = Some(page_size);
    }

    /// Sets the four margins. Top and bottom are also the header and footer band heights.
    pub fn margins(&mut self, margins: Margins) {
        self.margins = margins;
    }

    /// Runs `f` on every page of this section to fill the header band.
    ///
    /// A later call replaces an earlier one.
    pub fn header(&mut self, f: impl Fn(&mut Band) + 'a) {
        self.header = Some(Box::new(f));
    }

    /// Runs `f` on every page of this section to fill the footer band.
    ///
    /// A later call replaces an earlier one.
    pub fn footer(&mut self, f: impl Fn(&mut Band) + 'a) {
        self.footer = Some(Box::new(f));
    }

    /// Runs `f` on every page of this section to draw under everything else.
    ///
    /// A later call replaces an earlier one.
    pub fn behind(&mut self, f: impl Fn(&mut Layer) + 'a) {
        self.behind = Some(Box::new(f));
    }

    /// Runs `f` on every page of this section to draw over everything else.
    ///
    /// A later call replaces an earlier one.
    pub fn front(&mut self, f: impl Fn(&mut Layer) + 'a) {
        self.front = Some(Box::new(f));
    }

    fn lower(
        self,
        default_page: PageSize,
        doc: DocToken,
    ) -> Result<(PaginateConfig, FlowNode, Decorations<'a>)> {
        let Self {
            page_size,
            margins,
            flow,
            header,
            footer,
            behind,
            front,
        } = self;
        let style = flow.style.clone();
        let config = PaginateConfig {
            page_size: page_size.unwrap_or(default_page).resolve(),
            margins,
        };
        let root = FlowNode::Stack(flow.into_nodes()?);
        let decorations = Decorations {
            header: header.map(|hook| band_content(hook, style.clone())),
            footer: footer.map(|hook| band_content(hook, style.clone())),
            behind: behind.map(|hook| layer_content(hook, style.clone(), doc)),
            front: front.map(|hook| layer_content(hook, style, doc)),
        };
        Ok((config, root, decorations))
    }
}

impl Deref for Section<'_> {
    type Target = Flow;

    fn deref(&self) -> &Flow {
        &self.flow
    }
}

impl DerefMut for Section<'_> {
    fn deref_mut(&mut self) -> &mut Flow {
        &mut self.flow
    }
}

/// Document settings and page-time closures. Body methods come from [`Flow`].
pub struct Document<'a> {
    page_size: PageSize,
    doc: DocToken,
    fonts: FontStore,
    body: Sections<'a>,
    /// First [`Error::ContentOutsideSection`], or the error from a section closure.
    error: Option<Error>,
    #[cfg(feature = "backend-krilla")]
    images: ImageTable,
}

impl<'a> Document<'a> {
    fn new(page_size: PageSize) -> Self {
        let doc = DocToken::next();
        Self {
            page_size,
            doc,
            fonts: FontStore::new(),
            body: Sections::Implicit(Section::new(default_style(), doc)),
            error: None,
            #[cfg(feature = "backend-krilla")]
            images: ImageTable::default(),
        }
    }

    /// Decodes `bytes` once and returns a handle for [`Flow::image`], cells, and [`Layer::image_at`].
    ///
    /// Fails now on oversized, unparsable, or empty images. Without `backend-krilla` every call fails.
    pub fn load_image(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<Image> {
        #[cfg(feature = "backend-krilla")]
        {
            let decoded = self.images.register(bytes, format)?;
            Ok(Image {
                decoded,
                doc: self.doc,
            })
        }
        #[cfg(not(feature = "backend-krilla"))]
        {
            let _ = (self.doc, bytes, format);
            Err(Error::Image {
                message: "decoding needs feature `backend-krilla`".to_owned(),
            })
        }
    }

    /// Registers a font from its file bytes. The first font is the one text uses.
    pub fn font(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.fonts
            .register(FontSource {
                bytes: Arc::new(bytes),
                index: 0,
            })
            .map(drop)
    }

    /// Registers the face used by [`Paragraph::bold`].
    ///
    /// A body font must be registered first. A later call replaces the bold binding.
    pub fn bold_font(&mut self, bytes: Vec<u8>) -> Result<()> {
        if self.fonts.face(FontId::from_raw(0)).is_err() {
            return Err(Error::Font {
                index: 0,
                message: "body font is not registered".to_owned(),
            });
        }
        let id = self.fonts.register(FontSource {
            bytes: Arc::new(bytes),
            index: 0,
        })?;
        self.fonts.set_bold(id);
        Ok(())
    }

    /// Sets the four page margins. Top and bottom are also the header and footer band heights.
    pub fn margins(&mut self, margins: Margins) {
        if let Sections::Implicit(section) = &mut self.body {
            section.margins(margins);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to fill the header band. A later call replaces an earlier one.
    pub fn header(&mut self, f: impl Fn(&mut Band) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.header(f);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to fill the footer band. A later call replaces an earlier one.
    pub fn footer(&mut self, f: impl Fn(&mut Band) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.footer(f);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to draw under everything else. A later call replaces an earlier one.
    pub fn behind(&mut self, f: impl Fn(&mut Layer) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.behind(f);
        } else {
            self.fail_outside();
        }
    }

    /// Runs `f` on every page to draw over everything else. A later call replaces an earlier one.
    pub fn front(&mut self, f: impl Fn(&mut Layer) + 'a) {
        if let Sections::Implicit(section) = &mut self.body {
            section.front(f);
        } else {
            self.fail_outside();
        }
    }

    /// Appends a section that starts on a new page and keeps its own margins and hooks.
    ///
    /// Registering a font or an image does not count as writing the document.
    /// Fails with [`Error::ContentOutsideSection`] when the implicit section
    /// already has body content, non-default margins, or a page hook, and when
    /// body content, margins, or hooks are written on the document after this.
    pub fn section(&mut self, f: impl FnOnce(&mut Section<'a>) -> Result<()>) -> Result<()> {
        if let Some(error) = self.error.clone() {
            return Err(error);
        }
        let lead_written = matches!(&self.body, Sections::Implicit(section) if section.written());
        if lead_written {
            let error = Error::ContentOutsideSection;
            self.error = Some(error.clone());
            return Err(error);
        }
        let style = match &self.body {
            Sections::Implicit(section) => section.flow.style.clone(),
            Sections::Explicit { closed, .. } => closed.style.clone(),
        };
        let mut section = Section::new(style, self.doc);
        if let Err(error) = f(&mut section) {
            self.error = Some(error.clone());
            return Err(error);
        }
        if matches!(self.body, Sections::Implicit(_)) {
            let style = section.flow.style.clone();
            let doc = self.doc;
            self.body = Sections::Explicit {
                sections: vec![section],
                closed: Flow::rejecting(style, doc),
            };
        } else if let Sections::Explicit { sections, .. } = &mut self.body {
            sections.push(section);
        }
        Ok(())
    }

    fn fail_outside(&mut self) {
        if self.error.is_none() {
            self.error = Some(Error::ContentOutsideSection);
        }
    }

    fn paint(self, canvas: &mut dyn Canvas) -> Result<Painted> {
        let Self {
            page_size,
            doc,
            mut fonts,
            body,
            error,
            #[cfg(feature = "backend-krilla")]
            images,
        } = self;
        if let Some(error) = error {
            return Err(error);
        }
        let sections = match body {
            Sections::Implicit(section) => vec![section],
            Sections::Explicit { sections, closed } => {
                closed.into_nodes()?;
                sections
            }
        };
        let mut configs = Vec::with_capacity(sections.len());
        let mut roots = Vec::with_capacity(sections.len());
        let mut decorations = Vec::with_capacity(sections.len());
        for section in sections {
            let (config, root, deco) = section.lower(page_size, doc)?;
            configs.push(config);
            roots.push(root);
            decorations.push(deco);
        }
        let jobs = configs
            .into_iter()
            .zip(roots)
            .zip(&decorations)
            .map(|((config, root), decorations)| SectionJob {
                config,
                root,
                decorations,
            })
            .collect();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
        paginate_sections(jobs, &mut ctx, canvas)?;
        Ok(Painted {
            fonts,
            #[cfg(feature = "backend-krilla")]
            images,
        })
    }
}

struct Painted {
    fonts: FontStore,
    #[cfg(feature = "backend-krilla")]
    images: ImageTable,
}

/// Identifies one [`Document`].
///
/// Image ids are dense indexes. A handle moved into another document would paint
/// that document's image at the same index, so each document gets its own token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DocToken(u32);

impl DocToken {
    fn next() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// An image registered with one document. Copy it into page closures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Image {
    decoded: DecodedImage,
    doc: DocToken,
}

impl Image {
    /// Width in pixels.
    #[must_use]
    pub const fn pixel_width(self) -> u32 {
        self.decoded.pixel_width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn pixel_height(self) -> u32 {
        self.decoded.pixel_height
    }
}

/// Display size and placement of a flow or cell image.
///
/// Both sides stretch the image. One side derives the other from the pixel aspect.
/// Neither side uses 1 px = 1 pt. The default placement is the start of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct ImageBox {
    width: Option<Length<Positive>>,
    height: Option<Length<Positive>>,
    align: TextAlign,
}

impl Default for ImageBox {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            align: TextAlign::Start,
        }
    }
}

impl ImageBox {
    /// Sets the display width. The height follows the pixel aspect when unset.
    pub fn width(&mut self, width: Length<Positive>) {
        self.width = Some(width);
    }

    /// Sets the display height. The width follows the pixel aspect when unset.
    pub fn height(&mut self, height: Length<Positive>) {
        self.height = Some(height);
    }

    /// Centers the image in the current frame when it is narrower than that frame.
    pub fn center(&mut self) {
        self.align = TextAlign::Center;
    }

    /// Places the image at the end of the current frame when it is narrower than that frame.
    pub fn end(&mut self) {
        self.align = TextAlign::End;
    }

    fn lower(self, image: &Image, doc: DocToken) -> Result<FlowNode> {
        if image.doc != doc {
            return Err(Error::UnknownImage {
                id: image.decoded.id.as_u32(),
            });
        }
        ImageNode::new(image.decoded, self.width, self.height, self.align).map(FlowNode::Image)
    }
}

/// A solid horizontal line in the flow, cell, or list item it is added to.
///
/// Defaults: full width of the current frame, centered when shorter, 1 pt thick,
/// black, no space above or below. The line never splits; it moves to the next
/// page whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct HLine {
    width: Option<Length<Positive>>,
    align: TextAlign,
    thickness: Length<Positive>,
    color: Color,
    space_before: Length<NonNegative>,
    space_after: Length<NonNegative>,
}

impl Default for HLine {
    fn default() -> Self {
        Self {
            width: None,
            align: TextAlign::Center,
            thickness: Positive::pt(1.0).expect("1 pt is positive"),
            color: Color::rgb(0, 0, 0),
            space_before: NonNegative::ZERO,
            space_after: NonNegative::ZERO,
        }
    }
}

impl HLine {
    /// Sets the line length. Longer than the frame is an error when laid out.
    pub fn width(&mut self, width: Length<Positive>) {
        self.width = Some(width);
    }

    /// Line thickness.
    pub fn thickness(&mut self, thickness: Length<Positive>) {
        self.thickness = thickness;
    }

    /// Line color, including alpha.
    pub fn color(&mut self, color: Color) {
        self.color = color;
    }

    /// Blank space above the line. Counts toward the line's height.
    pub fn space_before(&mut self, space: Length<NonNegative>) {
        self.space_before = space;
    }

    /// Blank space below the line. Counts toward the line's height.
    pub fn space_after(&mut self, space: Length<NonNegative>) {
        self.space_after = space;
    }

    /// Places a shorter line at the start of the frame.
    pub fn start(&mut self) {
        self.align = TextAlign::Start;
    }

    /// Places a shorter line at the end of the frame.
    pub fn end(&mut self) {
        self.align = TextAlign::End;
    }

    fn lower(self) -> FlowNode {
        FlowNode::HLine(HLineNode::new(
            self.width,
            self.align,
            self.thickness,
            self.color,
            self.space_before,
            self.space_after,
        ))
    }
}

impl Deref for Document<'_> {
    type Target = Flow;

    fn deref(&self) -> &Flow {
        match &self.body {
            Sections::Implicit(section) => &section.flow,
            Sections::Explicit { closed, .. } => closed,
        }
    }
}

impl DerefMut for Document<'_> {
    fn deref_mut(&mut self) -> &mut Flow {
        match &mut self.body {
            Sections::Implicit(section) => &mut section.flow,
            Sections::Explicit { closed, .. } => closed,
        }
    }
}

fn default_style() -> Style {
    Style {
        font: FontId::from_raw(0),
        font_size: Positive::pt(12.0).expect("12 pt is positive"),
        line_height_factor: 1.2,
        color: Color::rgb(0, 0, 0),
        paragraph_spacing: NonNegative::ZERO,
        first_line_indent: NonNegative::ZERO,
        align: TextAlign::Start,
    }
}

/// Vertical content: the body, a column, a keep-together group, or a list item.
pub struct Flow {
    style: Style,
    doc: DocToken,
    nodes: Vec<FlowNode>,
    error: Option<Error>,
    reject_pushes: bool,
}

impl Flow {
    fn new(style: Style, doc: DocToken) -> Self {
        Self {
            style,
            doc,
            nodes: Vec::new(),
            error: None,
            reject_pushes: false,
        }
    }

    fn rejecting(style: Style, doc: DocToken) -> Self {
        Self {
            reject_pushes: true,
            ..Self::new(style, doc)
        }
    }

    fn written(&self) -> bool {
        !self.nodes.is_empty() || self.error.is_some()
    }

    fn child(&self) -> Self {
        Self::new(self.style.clone(), self.doc)
    }

    fn push(&mut self, node: Result<FlowNode>) {
        if self.error.is_some() {
            return;
        }
        if self.reject_pushes {
            self.error = Some(Error::ContentOutsideSection);
            return;
        }
        match node {
            Ok(node) => self.nodes.push(node),
            Err(error) => self.error = Some(error),
        }
    }

    fn nested(&mut self, f: impl FnOnce(&mut Flow)) -> Result<Vec<FlowNode>> {
        let mut child = self.child();
        f(&mut child);
        child.into_nodes()
    }

    fn into_nodes(self) -> Result<Vec<FlowNode>> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.nodes),
        }
    }

    fn text(&self, text: String) -> FlowNode {
        FlowNode::Text(TextNode::new(text, self.style.clone()))
    }

    /// Appends an image that moves to the next page whole instead of splitting.
    ///
    /// Wider than the frame, or taller than the page body, is an error. The image never shrinks.
    pub fn image(&mut self, image: &Image, f: impl FnOnce(&mut ImageBox)) {
        let mut box_ = ImageBox::default();
        f(&mut box_);
        self.push(box_.lower(image, self.doc));
    }

    /// Appends a paragraph. It wraps at the width of wherever it lands.
    pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
        let mut paragraph = Paragraph::default();
        f(&mut paragraph);
        self.push(lower_paragraph(paragraph, &self.style));
    }

    /// Appends equal-width columns side by side, 12 pt apart. Needs at least two.
    pub fn columns(&mut self, f: impl FnOnce(&mut Columns)) {
        let mut columns = Columns {
            parent: self.child(),
            cols: Vec::new(),
        };
        f(&mut columns);
        let node = columns.lower();
        self.push(node);
    }

    /// Appends a borderless table with equal-width columns.
    ///
    /// The column count is the span sum of the first row.
    pub fn table(&mut self, f: impl FnOnce(&mut Table)) {
        let mut table = Table {
            header: Vec::new(),
            body: Vec::new(),
            repeat_header: true,
        };
        f(&mut table);
        let node = table.lower(self);
        self.push(node);
    }

    /// Appends content that moves to the next page as one piece instead of splitting.
    pub fn keep_together(&mut self, f: impl FnOnce(&mut Flow)) {
        let node = self
            .nested(f)
            .map(|nodes| FlowNode::KeepTogether(KeepTogetherNode::Open(nodes)));
        self.push(node);
    }

    /// Appends vertical space. The part that does not fit continues on the next page.
    pub fn spacer(&mut self, height: Length<NonNegative>) {
        self.push(Ok(FlowNode::Spacer(SpacerNode { height })));
    }

    /// Appends a horizontal line that moves to the next page whole instead of splitting.
    ///
    /// Longer than the frame, or taller than the page body, is an error. The line never shrinks.
    pub fn hline(&mut self, f: impl FnOnce(&mut HLine)) {
        let mut line = HLine::default();
        f(&mut line);
        self.push(Ok(line.lower()));
    }

    /// Starts the next content on a new page. Only allowed in the body.
    pub fn page_break(&mut self) {
        self.push(Ok(FlowNode::PageBreak));
    }

    /// Appends a bulleted list.
    pub fn ul(&mut self, f: impl FnOnce(&mut List)) {
        self.list(ListKind::Bullet, f);
    }

    /// Appends a numbered list starting at 1.
    pub fn ol(&mut self, numbering: Numbering, f: impl FnOnce(&mut List)) {
        self.list(
            ListKind::Ordered {
                style: numbering_kind(numbering),
                start: NonZeroU32::MIN,
            },
            f,
        );
    }

    fn list(&mut self, kind: ListKind, f: impl FnOnce(&mut List)) {
        self.push(assemble_list(self.child(), kind, f));
    }
}

fn lower_paragraph(paragraph: Paragraph, parent: &Style) -> Result<FlowNode> {
    if let Some(error) = paragraph.error {
        return Err(error);
    }
    let mut style = parent.clone();
    style.first_line_indent = paragraph.first_line_indent;
    Ok(FlowNode::Text(TextNode::from_items(paragraph.items, style)))
}

fn numbering_kind(numbering: Numbering) -> MarkerKind {
    match numbering {
        Numbering::Decimal => MarkerKind::Decimal,
        Numbering::UpperAlpha => MarkerKind::UpperAlpha,
        Numbering::LowerAlpha => MarkerKind::LowerAlpha,
        Numbering::UpperRoman => MarkerKind::UpperRoman,
        Numbering::LowerRoman => MarkerKind::LowerRoman,
    }
}

fn assemble_list(parent: Flow, kind: ListKind, f: impl FnOnce(&mut List)) -> Result<FlowNode> {
    let mut list = List {
        parent,
        items: Vec::new(),
    };
    f(&mut list);
    list.lower(kind)
}

/// Text and paragraph-level settings for one paragraph.
#[derive(Debug)]
pub struct Paragraph {
    items: Vec<InlineItem>,
    first_line_indent: Length<NonNegative>,
    active_link: Option<LinkTarget>,
    error: Option<Error>,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            first_line_indent: NonNegative::ZERO,
            active_link: None,
            error: None,
        }
    }
}

impl Paragraph {
    /// Appends `s` to the paragraph.
    pub fn text(&mut self, s: impl AsRef<str>) {
        self.append(Face::Body, s.as_ref());
    }

    /// Appends `s` using the face registered by [`Document::bold_font`].
    pub fn bold(&mut self, s: impl AsRef<str>) {
        self.append(Face::Bold, s.as_ref());
    }

    /// Appends a fixed-width blank. Stroke settings keep their defaults when the result is ignored.
    pub fn blank(&mut self, width: Length<NonNegative>) -> Blank<'_> {
        self.push_blank(BlankWidth::Fixed(width))
    }

    /// Appends a blank `ems` times the paragraph font size wide.
    pub fn blank_em(&mut self, ems: u16) -> Blank<'_> {
        self.push_blank(BlankWidth::Em(ems))
    }

    /// Appends a blank that fills the rest of its visual line.
    ///
    /// A later non-empty run fails assembly with [`Error::RunAfterFillBlank`].
    pub fn blank_to_end(&mut self) -> Blank<'_> {
        self.push_blank(BlankWidth::ToEndOfLine)
    }

    /// Sets the horizontal inset of the first visual line.
    pub fn first_line_indent(&mut self, indent: Length<NonNegative>) {
        self.first_line_indent = indent;
    }

    /// Appends the text and bold calls inside `f` as one link to `uri`.
    ///
    /// `f` returns `()` because it runs inside [`Flow::paragraph`], which cannot
    /// use `?`. An empty `uri` is stored as [`Error::EmptyLink`] and `f` does not run.
    pub fn link(&mut self, uri: impl AsRef<str>, f: impl FnOnce(&mut Paragraph)) {
        let target = match LinkTarget::uri(uri.as_ref()) {
            Ok(target) => target,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        let previous = self.active_link.replace(target);
        f(self);
        self.active_link = previous;
    }

    fn append(&mut self, face: Face, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.sealed() {
            self.note(Error::RunAfterFillBlank);
            return;
        }
        if let Some(InlineItem::Text(last)) = self.items.last_mut() {
            if last.face == face && last.link == self.active_link {
                last.text.push_str(text);
                return;
            }
        }
        self.items.push(InlineItem::Text(
            TextRun::new(text, face).with_link(self.active_link.clone()),
        ));
    }

    fn push_blank(&mut self, width: BlankWidth) -> Blank<'_> {
        if self.sealed() {
            self.note(Error::RunAfterFillBlank);
            return Blank {
                paragraph: self,
                index: usize::MAX,
            };
        }
        self.items.push(InlineItem::Blank(InlineBlank::new(width)));
        let index = self.items.len() - 1;
        Blank {
            paragraph: self,
            index,
        }
    }

    fn sealed(&self) -> bool {
        matches!(
            self.items.last(),
            Some(InlineItem::Blank(blank)) if matches!(blank.width, BlankWidth::ToEndOfLine)
        )
    }

    fn note(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }
}

/// Stroke settings for the blank just appended to a [`Paragraph`].
pub struct Blank<'a> {
    paragraph: &'a mut Paragraph,
    index: usize,
}

impl Blank<'_> {
    /// Sets the stroke thickness. The default is 1 pt.
    pub fn thickness(&mut self, thickness: Length<Positive>) -> &mut Self {
        if let Some(blank) = self.blank_mut() {
            blank.thickness = thickness;
        }
        self
    }

    /// Sets the stroke color. The default is the paragraph text color.
    pub fn color(&mut self, color: Color) -> &mut Self {
        if let Some(blank) = self.blank_mut() {
            blank.color = Some(color);
        }
        self
    }

    /// Sets the distance from the baseline down to the stroke. The default is 1 pt.
    pub fn baseline_offset(&mut self, offset: Length<NonNegative>) -> &mut Self {
        if let Some(blank) = self.blank_mut() {
            blank.baseline_offset = offset;
        }
        self
    }

    fn blank_mut(&mut self) -> Option<&mut InlineBlank> {
        match self.paragraph.items.get_mut(self.index) {
            Some(InlineItem::Blank(blank)) => Some(blank),
            _ => None,
        }
    }
}

/// Columns being added to a row.
pub struct Columns {
    parent: Flow,
    cols: Vec<Result<Vec<FlowNode>>>,
}

impl Columns {
    /// Adds a column to the right of the previous ones.
    pub fn col(&mut self, f: impl FnOnce(&mut Flow)) {
        let col = self.parent.nested(f);
        self.cols.push(col);
    }

    fn lower(self) -> Result<FlowNode> {
        let even = ColumnWidth::Ratio(Ratio::new(1.0)?);
        let cols = self
            .cols
            .into_iter()
            .map(|col| col.map(|nodes| (even, nodes)))
            .collect::<Result<_>>()?;
        Ok(FlowNode::Columns(ColumnsNode::new(
            default_column_gap(),
            cols,
        )?))
    }
}

/// Rows being added to a table.
pub struct Table {
    header: Vec<Vec<Cell>>,
    body: Vec<Vec<Cell>>,
    repeat_header: bool,
}

impl Table {
    /// Adds a header row. Several calls make a multi-row header.
    pub fn header<I>(&mut self, cells: I)
    where
        I: IntoIterator,
        I::Item: Into<Cell>,
    {
        self.header
            .push(cells.into_iter().map(Into::into).collect());
    }

    /// Adds a body row.
    pub fn row<I>(&mut self, cells: I)
    where
        I: IntoIterator,
        I::Item: Into<Cell>,
    {
        self.body.push(cells.into_iter().map(Into::into).collect());
    }

    /// Whether header rows repeat at the top of each page the table continues on. Default on.
    pub fn repeat_header(&mut self, on: bool) {
        self.repeat_header = on;
    }

    fn lower(self, flow: &Flow) -> Result<FlowNode> {
        let first = self.header.first().or(self.body.first());
        let cols: usize = first.map_or(0, |row| {
            row.iter()
                .map(|cell| usize::from(cell.col_span.get()))
                .sum()
        });
        let even = ColumnWidth::Ratio(Ratio::new(1.0)?);
        let lower_rows = |rows: Vec<Vec<Cell>>| -> Result<Vec<Vec<table::Cell>>> {
            rows.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|cell| {
                            let content = match cell.content {
                                CellContent::Text(text) => vec![flow.text(text)],
                                CellContent::Paragraph(paragraph) => {
                                    vec![lower_paragraph(paragraph, &flow.style)?]
                                }
                                CellContent::List(DeferredList::Bullet(build)) => {
                                    vec![assemble_list(flow.child(), ListKind::Bullet, build)?]
                                }
                                CellContent::List(DeferredList::Ordered(numbering, build)) => {
                                    vec![assemble_list(
                                        flow.child(),
                                        ListKind::Ordered {
                                            style: numbering_kind(numbering),
                                            start: NonZeroU32::MIN,
                                        },
                                        build,
                                    )?]
                                }
                                CellContent::Image(image, image_box) => {
                                    vec![image_box.lower(&image, flow.doc)?]
                                }
                                CellContent::HLine(line) => vec![line.lower()],
                                CellContent::Spacer(height) => {
                                    vec![FlowNode::Spacer(SpacerNode { height })]
                                }
                            };
                            Ok(table::Cell::new(content)?.with_span(cell.col_span, cell.row_span))
                        })
                        .collect()
                })
                .collect()
        };
        let spec = TableSpec {
            header: lower_rows(self.header)?,
            body: lower_rows(self.body)?,
            repeat_header: self.repeat_header,
            ..TableSpec::new(vec![even; cols])
        };
        Ok(FlowNode::Table(TableNode::new(spec)?))
    }
}

/// One table cell.
#[derive(Debug)]
#[must_use]
pub struct Cell {
    content: CellContent,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
}

#[derive(Debug)]
enum CellContent {
    Text(String),
    Paragraph(Paragraph),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
    List(DeferredList),
}

enum DeferredList {
    Bullet(Box<dyn FnOnce(&mut List) + 'static>),
    Ordered(Numbering, Box<dyn FnOnce(&mut List) + 'static>),
}

impl std::fmt::Debug for DeferredList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeferredList::Bullet(_) => f.debug_tuple("Bullet").finish(),
            DeferredList::Ordered(numbering, _) => {
                f.debug_tuple("Ordered").field(numbering).finish()
            }
        }
    }
}

impl Cell {
    /// A cell spanning one column and one row.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            content: CellContent::Text(text.into()),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding one paragraph. The closure runs immediately.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`].
    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self {
        let mut paragraph = Paragraph::default();
        f(&mut paragraph);
        Self {
            content: CellContent::Paragraph(paragraph),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding a bulleted list.
    ///
    /// The closure is `'static` and runs when the table is lowered, after the
    /// table closure returns. Move owned data in. A local declared inside the
    /// table closure cannot be borrowed.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`].
    pub fn ul(f: impl FnOnce(&mut List) + 'static) -> Self {
        Self {
            content: CellContent::List(DeferredList::Bullet(Box::new(f))),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding a numbered list starting at 1.
    ///
    /// The closure is `'static` and runs when the table is lowered, after the
    /// table closure returns. Move owned data in. A local declared inside the
    /// table closure cannot be borrowed.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`].
    pub fn ol(numbering: Numbering, f: impl FnOnce(&mut List) + 'static) -> Self {
        Self {
            content: CellContent::List(DeferredList::Ordered(numbering, Box::new(f))),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding `image`.
    pub fn image(image: &Image, f: impl FnOnce(&mut ImageBox)) -> Self {
        let mut image_box = ImageBox::default();
        f(&mut image_box);
        Self {
            content: CellContent::Image(*image, image_box),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding a horizontal line as wide as the cell content.
    pub fn hline(f: impl FnOnce(&mut HLine)) -> Self {
        let mut line = HLine::default();
        f(&mut line);
        Self {
            content: CellContent::HLine(line),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// A one-by-one cell holding vertical space that paints nothing.
    ///
    /// Height may be zero. Taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`]. Unlike [`Flow::spacer`], the blank
    /// does not split across pages inside the cell.
    pub fn spacer(height: Length<NonNegative>) -> Self {
        Self {
            content: CellContent::Spacer(height),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        }
    }

    /// Spans `n` columns to the right.
    pub fn col_span(self, n: NonZeroU16) -> Self {
        Self {
            col_span: n,
            ..self
        }
    }

    /// Spans `n` rows down. A header cell cannot span into the body.
    pub fn row_span(self, n: NonZeroU16) -> Self {
        Self {
            row_span: n,
            ..self
        }
    }
}

impl From<&str> for Cell {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Cell {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&String> for Cell {
    fn from(text: &String) -> Self {
        Self::new(text.as_str())
    }
}

/// Marker numbering of an ordered list. Each label ends with `.`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Numbering {
    /// 1. 2. 3.
    Decimal,
    /// A. B. C., up to Z.
    UpperAlpha,
    /// a. b. c., up to z.
    LowerAlpha,
    /// I. II. III., up to 3999.
    UpperRoman,
    /// i. ii. iii., up to 3999.
    LowerRoman,
}

/// Items being added to a list. Markers hang in an 18 pt column.
pub struct List {
    parent: Flow,
    items: Vec<Result<ListItem>>,
}

impl List {
    /// Adds an item with the list's own marker.
    pub fn item(&mut self, f: impl FnOnce(&mut Flow)) {
        let item = self.parent.nested(f).and_then(ListItem::new);
        self.items.push(item);
    }

    /// Adds an item with `marker` in place of the list's marker. An empty marker is an error.
    pub fn item_marked(&mut self, marker: impl Into<String>, f: impl FnOnce(&mut Flow)) {
        let marker = marker.into();
        let item = self
            .parent
            .nested(f)
            .and_then(|nodes| ListItem::custom(marker, nodes));
        self.items.push(item);
    }

    fn lower(self, kind: ListKind) -> Result<FlowNode> {
        let spec = ListSpec {
            kind,
            marker_width: Positive::pt(18.0)?,
            nest_indent: None,
            marker_style: self.parent.style,
            items: self.items.into_iter().collect::<Result<_>>()?,
        };
        Ok(FlowNode::List(ListNode::new(spec)?))
    }
}

/// Page facts a page-time closure can read.
///
/// Section counters are filled after pagination. With one section they match
/// `number` and `count`, because that section's span covers the document.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Page {
    /// One-based page number in the document.
    pub number: u32,
    /// Pages in the document, blank ones included.
    pub count: u32,
    /// One-based page number in the section.
    pub section_number: u32,
    /// Pages in the section.
    pub section_count: u32,
    /// Paper size.
    pub size: Size,
    /// The four margins of this page.
    pub margins: Margins,
}

impl Page {
    /// Whether this is the document's last page.
    #[must_use]
    pub fn is_last(&self) -> bool {
        self.number == self.count
    }
}

impl From<&PageInfo> for Page {
    fn from(info: &PageInfo) -> Self {
        Self {
            number: info.number.get(),
            count: info.count.get(),
            section_number: info.section_number.get(),
            section_count: info.section_count.get(),
            size: info.size,
            margins: info.margins,
        }
    }
}

struct BandSlot {
    text: String,
    link: Option<LinkTarget>,
}

/// One page's header or footer band.
///
/// The band is as tall as its margin and spans the full page width. Call
/// [`Band::inset`] when slots should line up with the body. Content that does
/// not fit is an error.
pub struct Band {
    page: Page,
    left: Option<BandSlot>,
    right: Option<BandSlot>,
    inset_left: Length<NonNegative>,
    inset_right: Length<NonNegative>,
    error: Option<Error>,
}

impl Band {
    /// The page being drawn.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page
    }

    /// Shrinks the slot area inward from the page edges.
    ///
    /// Defaults are zero. Side margins do not apply automatically; pass
    /// [`Page::margins`] when the slots should align with the body.
    pub fn inset(&mut self, left: Length<NonNegative>, right: Length<NonNegative>) {
        self.inset_left = left;
        self.inset_right = right;
    }

    /// Sets the start-aligned text.
    pub fn left(&mut self, s: impl AsRef<str>) {
        self.left = Some(BandSlot {
            text: s.as_ref().to_owned(),
            link: None,
        });
    }

    /// Sets the end-aligned text.
    pub fn right(&mut self, s: impl AsRef<str>) {
        self.right = Some(BandSlot {
            text: s.as_ref().to_owned(),
            link: None,
        });
    }

    /// Sets the start-aligned slot to `text` linked to `uri`.
    ///
    /// An empty `uri` is stored as [`Error::EmptyLink`] and leaves the slot unchanged.
    pub fn left_link(&mut self, uri: impl AsRef<str>, text: impl AsRef<str>) {
        self.set_link(true, uri.as_ref(), text.as_ref());
    }

    /// Sets the end-aligned slot to `text` linked to `uri`.
    ///
    /// An empty `uri` is stored as [`Error::EmptyLink`] and leaves the slot unchanged.
    pub fn right_link(&mut self, uri: impl AsRef<str>, text: impl AsRef<str>) {
        self.set_link(false, uri.as_ref(), text.as_ref());
    }

    fn set_link(&mut self, left: bool, uri: &str, text: &str) {
        let target = match LinkTarget::uri(uri) {
            Ok(target) => target,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        let slot = Some(BandSlot {
            text: text.to_owned(),
            link: Some(target),
        });
        if left {
            self.left = slot;
        } else {
            self.right = slot;
        }
    }

    fn note(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    fn lower(self, style: &Style) -> Result<Vec<FlowNode>> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let column = |slot: BandSlot, align: TextAlign| {
            let style = Style {
                align,
                ..style.clone()
            };
            let node = if slot.text.is_empty() {
                TextNode::from_items(Vec::new(), style)
            } else {
                TextNode::from_runs(
                    vec![TextRun::new(slot.text, Face::Body).with_link(slot.link)],
                    style,
                )
            };
            vec![FlowNode::Text(node)]
        };
        let even = ColumnWidth::Ratio(Ratio::new(1.0)?);
        let slots: Vec<_> = [(self.left, TextAlign::Start), (self.right, TextAlign::End)]
            .into_iter()
            .filter_map(|(text, align)| text.map(|text| (even, column(text, align))))
            .collect();
        if slots.is_empty() {
            return Ok(Vec::new());
        }
        let spacer = |pad: Length<NonNegative>| -> Result<Option<(ColumnWidth, Vec<FlowNode>)>> {
            if pad.as_pt() == 0.0 {
                return Ok(None);
            }
            Ok(Some((
                ColumnWidth::Fixed(Positive::pt(pad.as_pt())?),
                Vec::new(),
            )))
        };
        let mut tracks: Vec<_> = spacer(self.inset_left)?.into_iter().collect();
        tracks.extend(slots);
        tracks.extend(spacer(self.inset_right)?);
        if tracks.len() == 1 {
            return Ok(tracks.remove(0).1);
        }
        Ok(vec![FlowNode::Columns(ColumnsNode::new(
            NonNegative::ZERO,
            tracks,
        )?)])
    }
}

fn band_content(hook: BandHook<'_>, style: Style) -> BandContent<'_> {
    Box::new(move |info: &PageInfo| {
        let mut band = Band {
            page: Page::from(info),
            left: None,
            right: None,
            inset_left: NonNegative::ZERO,
            inset_right: NonNegative::ZERO,
            error: None,
        };
        hook(&mut band);
        band.lower(&style)
    })
}

/// One page's behind or front layer. Coordinates are page coordinates, not inset by margins.
pub struct Layer {
    page: Page,
    style: Style,
    doc: DocToken,
    items: Vec<LayerItem>,
    error: Option<Error>,
}

impl Layer {
    /// The page being drawn.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page
    }

    /// Whether this is the document's last page.
    #[must_use]
    pub fn is_last(&self) -> bool {
        self.page.is_last()
    }

    /// Draws `s` on one line with the top-left of its line box at `at`. It is not wrapped or clipped.
    pub fn text_at(&mut self, s: impl AsRef<str>, at: Point) {
        self.items.push(LayerItem::Text(LayerText {
            text: s.as_ref().to_owned(),
            at,
            style: self.style.clone(),
        }));
    }

    /// Draws `image` stretched into `rect`, in page coordinates.
    ///
    /// The rectangle is not clipped and is not inset by margins.
    pub fn image_at(&mut self, image: &Image, rect: Rect) {
        if image.doc != self.doc {
            if self.error.is_none() {
                self.error = Some(Error::UnknownImage {
                    id: image.decoded.id.as_u32(),
                });
            }
            return;
        }
        self.items.push(LayerItem::Image {
            image: image.decoded,
            rect,
        });
    }

    /// Straight segment in page coordinates. `from == to` fails the document with
    /// [`Error::InvalidGeometry`] before any page is drawn.
    pub fn line(&mut self, from: Point, to: Point, pen: Pen) {
        self.push_shape(straight_path(from, to), Ink::Stroke(pen), None);
    }

    /// Rectangle in page coordinates. Not inset by margins and not clipped.
    pub fn rect(&mut self, rect: Rect, ink: Ink) {
        self.push_shape(Path::rect(rect), ink, None);
    }

    /// Ellipse inscribed in `rect`, in page coordinates.
    pub fn ellipse(&mut self, rect: Rect, ink: Ink) {
        self.push_shape(Path::ellipse(rect), ink, None);
    }

    /// Draws the shapes added in `draw` rotated by `degrees` about `about`.
    ///
    /// Positive is clockwise on the page. A non-finite angle fails the document with
    /// [`Error::InvalidGeometry`] and does not run `draw`. Shapes keep their order relative
    /// to the layer's other items.
    pub fn rotated(&mut self, degrees: f32, about: Point, draw: impl FnOnce(&mut Shapes<'_>)) {
        if !degrees.is_finite() {
            if self.error.is_none() {
                self.error = Some(Error::InvalidGeometry);
            }
            return;
        }
        let mut shapes = Shapes {
            layer: self,
            degrees,
            about,
        };
        draw(&mut shapes);
    }

    fn push_shape(&mut self, path: Result<Path>, ink: Ink, turn: Option<(f32, Point)>) {
        let path = path.and_then(|path| match turn {
            Some((degrees, about)) if !is_zero_angle(degrees) => path.rotated(degrees, about),
            _ => Ok(path),
        });
        match path {
            Ok(path) => self.items.push(LayerItem::Shape { path, ink }),
            Err(error) => {
                if self.error.is_none() {
                    self.error = Some(error);
                }
            }
        }
    }
}

/// Path-only view of a [`Layer`] inside [`Layer::rotated`].
pub struct Shapes<'l> {
    layer: &'l mut Layer,
    degrees: f32,
    about: Point,
}

impl Shapes<'_> {
    /// Straight segment. See [`Layer::line`].
    pub fn line(&mut self, from: Point, to: Point, pen: Pen) {
        self.layer.push_shape(
            straight_path(from, to),
            Ink::Stroke(pen),
            Some((self.degrees, self.about)),
        );
    }

    /// Rectangle. See [`Layer::rect`].
    pub fn rect(&mut self, rect: Rect, ink: Ink) {
        self.layer
            .push_shape(Path::rect(rect), ink, Some((self.degrees, self.about)));
    }

    /// Ellipse inscribed in `rect`. See [`Layer::ellipse`].
    pub fn ellipse(&mut self, rect: Rect, ink: Ink) {
        self.layer
            .push_shape(Path::ellipse(rect), ink, Some((self.degrees, self.about)));
    }
}

fn straight_path(from: Point, to: Point) -> Result<Path> {
    if from == to {
        Err(Error::InvalidGeometry)
    } else {
        Ok(Path::line(from, to))
    }
}

fn is_zero_angle(degrees: f32) -> bool {
    degrees.to_bits() == 0 || degrees.to_bits() == (-0.0f32).to_bits()
}

fn layer_content(hook: LayerHook<'_>, style: Style, doc: DocToken) -> LayerContent<'_> {
    Box::new(move |info: &PageInfo| {
        let mut layer = Layer {
            page: Page::from(info),
            style: style.clone(),
            doc,
            items: Vec::new(),
            error: None,
        };
        hook(&mut layer);
        match layer.error {
            Some(error) => Err(error),
            None => Ok(layer.items),
        }
    })
}

/// A laid-out document.
#[must_use]
pub struct Rendered {
    recording: RecordingCanvas,
    #[cfg_attr(not(feature = "backend-krilla"), allow(dead_code))]
    fonts: FontStore,
    #[cfg(feature = "backend-krilla")]
    images: ImageTable,
}

impl Rendered {
    /// Every page's drawing commands, in paint order.
    #[must_use]
    pub fn pages(&self) -> Vec<RecordedPage> {
        self.recording.pages()
    }

    /// Writes the pages as PDF bytes.
    #[cfg(feature = "backend-krilla")]
    pub fn to_pdf(self) -> Result<Vec<u8>> {
        let mut pdf =
            crate::backend::KrillaCanvas::new_with_resources(Arc::new(self.fonts), self.images);
        crate::canvas::replay(&self.recording, &mut pdf)?;
        pdf.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::fixture_bytes;
    use crate::geom::PageSize;

    fn doc_with_font<'a>() -> Document<'a> {
        let mut doc = Document::new(PageSize::A4);
        doc.font(fixture_bytes().to_vec()).unwrap();

        doc
    }

    #[test]
    fn paragraph_enters_the_flow_unwrapped() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| p.text("A B"));
        let Sections::Implicit(section) = &doc.body else {
            panic!("paragraph stays on the implicit section");
        };
        assert!(matches!(
            section.flow.nodes.as_slice(),
            [FlowNode::Text(text)] if *text == TextNode::new("A B", default_style())
        ));
    }

    #[test]
    fn paragraph_coalesces_adjacent_faces_and_ignores_empty_appends() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| {
            p.text("A");
            p.text("");
            p.text("B");
            p.bold("C");
            p.bold("D");
            p.text("E");
        });
        let Sections::Implicit(section) = &doc.body else {
            panic!("paragraph stays on the implicit section");
        };
        let expected = TextNode::from_runs(
            vec![
                TextRun::new("AB", Face::Body),
                TextRun::new("CD", Face::Bold),
                TextRun::new("E", Face::Body),
            ],
            default_style(),
        );
        assert!(matches!(
            section.flow.nodes.as_slice(),
            [FlowNode::Text(text)] if *text == expected
        ));
    }

    fn flow_text(doc: &Document<'_>) -> TextNode {
        let Sections::Implicit(section) = &doc.body else {
            panic!("paragraph stays on the implicit section");
        };
        match section.flow.nodes.as_slice() {
            [FlowNode::Text(text)] => text.clone(),
            other => panic!("expected one text node, got {other:?}"),
        }
    }

    #[test]
    fn blank_breaks_text_coalescing() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| {
            p.text("A");
            p.blank(NonNegative::pt(10.0).unwrap())
                .thickness(Positive::pt(0.5).unwrap())
                .color(Color::rgb(1, 2, 3))
                .baseline_offset(NonNegative::pt(2.0).unwrap());
            p.text("B");
        });
        let mut blank = InlineBlank::new(BlankWidth::Fixed(NonNegative::pt(10.0).unwrap()));
        blank.thickness = Positive::pt(0.5).unwrap();
        blank.color = Some(Color::rgb(1, 2, 3));
        blank.baseline_offset = NonNegative::pt(2.0).unwrap();
        let expected = TextNode::from_items(
            vec![
                InlineItem::Text(TextRun::new("A", Face::Body)),
                InlineItem::Blank(blank),
                InlineItem::Text(TextRun::new("B", Face::Body)),
            ],
            default_style(),
        );
        assert_eq!(flow_text(&doc), expected);
    }

    #[test]
    fn fill_blank_coalesces_text_before_it_and_rejects_a_later_run() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| {
            p.text("A");
            p.text("B");
            p.blank_to_end();
            p.text("");
            p.bold("");
        });
        let expected = TextNode::from_items(
            vec![
                InlineItem::Text(TextRun::new("AB", Face::Body)),
                InlineItem::Blank(InlineBlank::new(BlankWidth::ToEndOfLine)),
            ],
            default_style(),
        );
        assert_eq!(flow_text(&doc), expected);

        for build in [
            |p: &mut Paragraph| {
                p.blank_to_end();
                p.text("x");
            },
            |p: &mut Paragraph| {
                p.blank_to_end();
                p.bold("x");
            },
            |p: &mut Paragraph| {
                p.blank_to_end();
                p.blank(NonNegative::ZERO);
            },
            |p: &mut Paragraph| {
                p.blank_to_end();
                p.blank_em(1);
            },
            |p: &mut Paragraph| {
                p.blank_to_end();
                p.blank_to_end();
            },
        ] {
            let mut doc = doc_with_font();
            doc.paragraph(build);
            let Sections::Implicit(section) = &doc.body else {
                panic!("paragraph stays on the implicit section");
            };
            assert_eq!(section.flow.error, Some(Error::RunAfterFillBlank));
            assert!(section.flow.nodes.is_empty());
        }
    }

    #[test]
    fn run_after_fill_blank_paints_nothing() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| {
            p.text("A");
            p.blank_to_end();
            p.text("x");
        });
        assert_fails_with_empty_canvas(doc, &Error::RunAfterFillBlank);
    }

    #[test]
    fn fixed_blank_wider_than_the_indented_line_fails_before_any_page() {
        let mut doc = Document::new(PageSize::custom(Size::try_pt(100.0, 200.0).unwrap()));
        doc.font(fixture_bytes().to_vec()).unwrap();
        doc.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(40.0).unwrap());
            p.blank(NonNegative::pt(80.0).unwrap());
        });
        let mut rec = RecordingCanvas::new();
        let Err(error) = doc.paint(&mut rec) else {
            panic!("expected the blank to be wider than the indented line");
        };
        let Error::UnbreakableWiderThanFrame { width, avail } = error else {
            panic!("{error:?}");
        };
        assert!(crate::canvas::pt_near(width, 80.0), "{width}");
        assert!(crate::canvas::pt_near(avail, 60.0), "{avail}");
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn bold_font_requires_body_and_rebinds_to_the_latest_face() {
        let mut doc = Document::new(PageSize::A4);
        assert_eq!(
            doc.bold_font(fixture_bytes().to_vec()).unwrap_err(),
            Error::Font {
                index: 0,
                message: "body font is not registered".to_owned(),
            }
        );
        doc.font(fixture_bytes().to_vec()).unwrap();
        doc.bold_font(fixture_bytes().to_vec()).unwrap();
        assert_eq!(doc.fonts.bold().unwrap(), FontId::from_raw(1));
        doc.bold_font(fixture_bytes().to_vec()).unwrap();
        assert_eq!(doc.fonts.bold().unwrap(), FontId::from_raw(2));
    }

    fn assert_fails_with_empty_canvas(doc: Document<'_>, expected: &Error) {
        let mut rec = RecordingCanvas::new();
        let result = doc.paint(&mut rec);
        assert_eq!(result.err().as_ref(), Some(expected));
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn link_coalesces_on_face_and_uri_and_restores_the_outer_target() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| {
            p.link("https://a", |p| {
                p.text("A");
                p.text("B");
                p.bold("C");
                p.link("https://b", |p| p.text("D"));
                p.text("E");
            });
            p.text("F");
        });
        let a = Some(LinkTarget::uri("https://a").unwrap());
        let b = Some(LinkTarget::uri("https://b").unwrap());
        let expected = TextNode::from_runs(
            vec![
                TextRun::new("AB", Face::Body).with_link(a.clone()),
                TextRun::new("C", Face::Bold).with_link(a),
                TextRun::new("D", Face::Body).with_link(b),
                TextRun::new("E", Face::Body)
                    .with_link(Some(LinkTarget::uri("https://a").unwrap())),
                TextRun::new("F", Face::Body),
            ],
            default_style(),
        );
        assert_eq!(flow_text(&doc), expected);
    }

    #[test]
    fn blank_inside_a_link_stays_unlinked() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| {
            p.link("u", |p| {
                p.text("A");
                p.blank(NonNegative::pt(10.0).unwrap());
                p.text("B");
            });
        });
        let link = Some(LinkTarget::uri("u").unwrap());
        let expected = TextNode::from_items(
            vec![
                InlineItem::Text(TextRun::new("A", Face::Body).with_link(link.clone())),
                InlineItem::Blank(InlineBlank::new(BlankWidth::Fixed(
                    NonNegative::pt(10.0).unwrap(),
                ))),
                InlineItem::Text(TextRun::new("B", Face::Body).with_link(link)),
            ],
            default_style(),
        );
        assert_eq!(flow_text(&doc), expected);
    }

    #[test]
    fn empty_uri_does_not_run_the_link_closure() {
        let mut doc = doc_with_font();
        let mut ran = false;
        doc.paragraph(|p| {
            p.text("A");
            p.link("", |_| ran = true);
        });
        assert!(!ran);
        assert_fails_with_empty_canvas(doc, &Error::EmptyLink);
    }

    #[test]
    fn empty_band_uri_paints_nothing() {
        let mut doc = doc_with_font();
        doc.margins(Margins::uniform(NonNegative::pt(36.0).unwrap()));
        doc.header(|h| {
            h.left_link("", "Home");
            h.right_link("https://example.com", "Later");
        });
        doc.paragraph(|p| p.text("A"));
        assert_fails_with_empty_canvas(doc, &Error::EmptyLink);
    }

    const MISSING: char = '\u{6f22}';

    #[test]
    fn bold_and_indent_failures_happen_before_any_page() {
        let mut missing_bold = doc_with_font();
        missing_bold.paragraph(|p| p.bold("A"));
        assert_fails_with_empty_canvas(missing_bold, &Error::UnknownFont { id: 1 });

        let mut missing_glyph = doc_with_font();
        missing_glyph.bold_font(fixture_bytes().to_vec()).unwrap();
        missing_glyph.paragraph(|p| p.bold(MISSING.to_string()));
        assert_fails_with_empty_canvas(
            missing_glyph,
            &Error::MissingGlyph {
                font: FontId::from_raw(1),
                scalar: MISSING,
            },
        );

        let mut invalid_indent = Document::new(PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()));
        invalid_indent.font(fixture_bytes().to_vec()).unwrap();
        invalid_indent.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(24.0).unwrap());
            p.text("A");
        });
        assert_fails_with_empty_canvas(invalid_indent, &Error::InvalidGeometry);

        let mut overwide = Document::new(PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()));
        overwide.font(fixture_bytes().to_vec()).unwrap();
        overwide.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(12.0).unwrap());
            p.text("AA");
        });
        let mut rec = RecordingCanvas::new();
        let error = overwide.paint(&mut rec).err().unwrap();
        assert!(
            matches!(
                error,
                Error::UnbreakableWiderThanFrame { avail, .. }
                    if crate::canvas::pt_near(avail, 12.0)
            ),
            "{error:?}"
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn missing_glyph_in_header_fails_before_any_page() {
        let mut doc = doc_with_font();
        doc.margins(Margins::uniform(NonNegative::pt(36.0).unwrap()));
        doc.header(|h| h.left(MISSING.to_string()));
        doc.paragraph(|p| p.text("A"));
        assert_fails_with_empty_canvas(
            doc,
            &Error::MissingGlyph {
                font: FontId::from_raw(0),
                scalar: MISSING,
            },
        );
    }

    #[test]
    fn degenerate_layer_line_fails_before_any_page() {
        let mut doc = Document::new(PageSize::custom(Size::try_pt(200.0, 100.0).unwrap()));
        let point = Point::try_pt(4.0, 5.0).unwrap();
        let pen = Pen::new(Color::rgb(0, 0, 0), Positive::pt(1.0).unwrap());
        doc.behind(|layer| {
            layer.line(point, point, pen);
            layer.rect(
                Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap(),
                Ink::Fill(Color::rgb(0, 0, 0)),
            );
        });
        assert_fails_with_empty_canvas(doc, &Error::InvalidGeometry);
    }

    #[test]
    fn non_finite_rotation_does_not_draw() {
        use std::cell::Cell;

        let ran = Cell::new(false);
        let mut doc = Document::new(PageSize::custom(Size::try_pt(200.0, 100.0).unwrap()));
        doc.behind(|layer| {
            layer.rotated(f32::NAN, Point::ZERO, |_| ran.set(true));
        });
        assert_fails_with_empty_canvas(doc, &Error::InvalidGeometry);
        assert!(!ran.get());
    }

    #[test]
    fn missing_glyph_in_front_fails_before_any_page() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| p.text("A"));
        doc.front(|page| {
            if page.is_last() {
                page.text_at(MISSING.to_string(), Point::try_pt(10.0, 10.0).unwrap());
            }
        });
        assert_fails_with_empty_canvas(
            doc,
            &Error::MissingGlyph {
                font: FontId::from_raw(0),
                scalar: MISSING,
            },
        );
    }

    #[test]
    fn header_without_margin_fails_before_any_page() {
        let mut doc = doc_with_font();
        doc.header(|h| h.left("H"));
        doc.paragraph(|p| p.text("A"));
        let mut rec = RecordingCanvas::new();
        let result = doc.paint(&mut rec);
        assert!(matches!(
            result,
            Err(Error::DecorationTallerThanMargin { margin, .. }) if margin == 0.0
        ));
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn keep_together_taller_than_the_page_fails_before_any_page() {
        let mut doc = doc_with_font();
        doc.keep_together(|g| {
            for _ in 0..100 {
                g.paragraph(|p| p.text("line"));
            }
        });
        assert_fails_with_empty_canvas(doc, &Error::UnsplittableTallerThanFrame);
    }

    #[test]
    fn section_after_paragraph_fails_before_any_page() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| p.text("A"));
        let err = doc
            .section(|section| {
                section.paragraph(|p| p.text("B"));
                Ok(())
            })
            .unwrap_err();
        assert_eq!(err, Error::ContentOutsideSection);
        assert_eq!(
            err.to_string(),
            "body or decorations written both on the document and in sections"
        );
        assert_fails_with_empty_canvas(doc, &Error::ContentOutsideSection);
    }

    #[test]
    fn paragraph_after_section_fails_before_any_page() {
        let mut doc = doc_with_font();
        doc.section(|section| {
            section.paragraph(|p| p.text("A"));
            Ok(())
        })
        .unwrap();
        doc.paragraph(|p| p.text("B"));
        assert_fails_with_empty_canvas(doc, &Error::ContentOutsideSection);
    }

    #[test]
    fn margins_after_section_fail_before_any_page() {
        let mut doc = doc_with_font();
        doc.section(|section| {
            section.paragraph(|p| p.text("A"));
            Ok(())
        })
        .unwrap();
        doc.margins(Margins::uniform(NonNegative::pt(12.0).unwrap()));
        assert_fails_with_empty_canvas(doc, &Error::ContentOutsideSection);
    }
}
