//! Public document construction surface (Rust DSL).
//!
//! [`document`] runs the caller's closure once to assemble the body flow.
//! Header, footer, behind, and front closures run again on every page after
//! pagination, so only they see page numbers.

use std::num::{NonZeroU16, NonZeroU32};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::canvas::{Canvas, DecodedImage, ImageFormat, RecordedPage, RecordingCanvas};
use crate::error::{Error, Result};
use crate::fonts::{FontId, FontSource, FontStore};
use crate::geom::{Color, Length, Margins, NonNegative, Point, Positive, Rect, Size};
use crate::layout::columns::{default_column_gap, ColumnWidth, ColumnsNode, Ratio};
use crate::layout::decorate::{
    BandContent, Decorations, LayerContent, LayerItem, LayerText, PageInfo,
};
use crate::layout::flow::{FlowNode, KeepTogetherNode, LayoutCtx, SpacerNode};
use crate::layout::image::ImageNode;
use crate::layout::list::{ListItem, ListKind, ListNode, ListSpec, MarkerKind};
use crate::layout::paginate::{paginate_root, PaginateConfig};
use crate::layout::paragraph::TextNode;
use crate::layout::style::{Style, TextAlign};
use crate::layout::table::{self, TableNode, TableSpec};

#[cfg(feature = "backend-krilla")]
use crate::backend::ImageTable;

/// Lays out a document of `page_size` pages.
///
/// `build` runs once. Builder mistakes are deferred: the first one is
/// returned from here, before any page is drawn. A missing glyph anywhere,
/// including headers and layers, also fails the whole document.
///
/// Text uses the first font passed to [`Document::font`] at 12 pt with line
/// height 1.2, black, start-aligned. Margins default to zero.
pub fn document<'a>(
    page_size: Size,
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

/// Document settings and page-time closures. Body methods come from [`Flow`].
pub struct Document<'a> {
    page_size: Size,
    doc: DocToken,
    fonts: FontStore,
    margins: Margins,
    body: Flow,
    header: Option<BandHook<'a>>,
    footer: Option<BandHook<'a>>,
    behind: Option<LayerHook<'a>>,
    front: Option<LayerHook<'a>>,
    #[cfg(feature = "backend-krilla")]
    images: ImageTable,
}

impl<'a> Document<'a> {
    fn new(page_size: Size) -> Self {
        let doc = DocToken::next();
        Self {
            page_size,
            doc,
            fonts: FontStore::new(),
            margins: Margins::default(),
            body: Flow::new(default_style(), doc),
            header: None,
            footer: None,
            behind: None,
            front: None,
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

    /// Sets the four page margins. Top and bottom are also the header and footer band heights.
    pub fn margins(&mut self, margins: Margins) {
        self.margins = margins;
    }

    /// Runs `f` on every page to fill the header band. A later call replaces an earlier one.
    pub fn header(&mut self, f: impl Fn(&mut Band) + 'a) {
        self.header = Some(Box::new(f));
    }

    /// Runs `f` on every page to fill the footer band. A later call replaces an earlier one.
    pub fn footer(&mut self, f: impl Fn(&mut Band) + 'a) {
        self.footer = Some(Box::new(f));
    }

    /// Runs `f` on every page to draw under everything else. A later call replaces an earlier one.
    pub fn behind(&mut self, f: impl Fn(&mut Layer) + 'a) {
        self.behind = Some(Box::new(f));
    }

    /// Runs `f` on every page to draw over everything else. A later call replaces an earlier one.
    pub fn front(&mut self, f: impl Fn(&mut Layer) + 'a) {
        self.front = Some(Box::new(f));
    }

    fn paint(self, canvas: &mut dyn Canvas) -> Result<Painted> {
        let Self {
            page_size,
            doc,
            mut fonts,
            margins,
            body,
            header,
            footer,
            behind,
            front,
            #[cfg(feature = "backend-krilla")]
            images,
        } = self;
        let style = body.style.clone();
        let root = FlowNode::Stack(body.into_nodes()?);
        let decorations = Decorations {
            header: header.map(|hook| band_content(hook, style.clone())),
            footer: footer.map(|hook| band_content(hook, style.clone())),
            behind: behind.map(|hook| layer_content(hook, style.clone(), doc)),
            front: front.map(|hook| layer_content(hook, style.clone(), doc)),
        };
        let config = PaginateConfig { page_size, margins };
        let mut ctx = LayoutCtx { fonts: &mut fonts };
        paginate_root(root, &mut ctx, config, &decorations, canvas)?;
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

impl Deref for Document<'_> {
    type Target = Flow;

    fn deref(&self) -> &Flow {
        &self.body
    }
}

impl DerefMut for Document<'_> {
    fn deref_mut(&mut self) -> &mut Flow {
        &mut self.body
    }
}

fn default_style() -> Style {
    Style {
        font: FontId::from_raw(0),
        font_size: Positive::pt(12.0).expect("12 pt is positive"),
        line_height_factor: 1.2,
        color: Color::rgb(0, 0, 0),
        paragraph_spacing: NonNegative::ZERO,
        align: TextAlign::Start,
    }
}

/// Vertical content: the body, a column, a keep-together group, or a list item.
pub struct Flow {
    style: Style,
    doc: DocToken,
    nodes: Vec<FlowNode>,
    error: Option<Error>,
}

impl Flow {
    fn new(style: Style, doc: DocToken) -> Self {
        Self {
            style,
            doc,
            nodes: Vec::new(),
            error: None,
        }
    }

    fn child(&self) -> Self {
        Self::new(self.style.clone(), self.doc)
    }

    fn push(&mut self, node: Result<FlowNode>) {
        if self.error.is_some() {
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
        let node = self.text(paragraph.text);
        self.push(Ok(node));
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
        let style = match numbering {
            Numbering::Decimal => MarkerKind::Decimal,
            Numbering::UpperAlpha => MarkerKind::UpperAlpha,
            Numbering::LowerAlpha => MarkerKind::LowerAlpha,
            Numbering::UpperRoman => MarkerKind::UpperRoman,
            Numbering::LowerRoman => MarkerKind::LowerRoman,
        };
        self.list(
            ListKind::Ordered {
                style,
                start: NonZeroU32::MIN,
            },
            f,
        );
    }

    fn list(&mut self, kind: ListKind, f: impl FnOnce(&mut List)) {
        let mut list = List {
            parent: self.child(),
            items: Vec::new(),
        };
        f(&mut list);
        let node = list.lower(kind);
        self.push(node);
    }
}

/// Text of one paragraph.
#[derive(Default)]
pub struct Paragraph {
    text: String,
}

impl Paragraph {
    /// Appends `s` to the paragraph.
    pub fn text(&mut self, s: impl AsRef<str>) {
        self.text.push_str(s.as_ref());
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
                                CellContent::Image(image, image_box) => {
                                    vec![image_box.lower(&image, flow.doc)?]
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Cell {
    content: CellContent,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
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
/// The document is one implicit section, so section counters equal document totals.
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
            section_number: info.number.get(),
            section_count: info.count.get(),
            size: info.size,
            margins: info.margins,
        }
    }
}

/// One page's header or footer band.
///
/// The band is as tall as its margin. Slots inset by the left and right
/// margins so they line up with the body. Content that does not fit is an
/// error.
pub struct Band {
    page: Page,
    left: Option<String>,
    right: Option<String>,
}

impl Band {
    /// The page being drawn.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page
    }

    /// Sets the start-aligned text.
    pub fn left(&mut self, s: impl AsRef<str>) {
        self.left = Some(s.as_ref().to_owned());
    }

    /// Sets the end-aligned text.
    pub fn right(&mut self, s: impl AsRef<str>) {
        self.right = Some(s.as_ref().to_owned());
    }

    fn lower(self, style: &Style) -> Result<Vec<FlowNode>> {
        let slot = |text: String, align: TextAlign| {
            let style = Style {
                align,
                ..style.clone()
            };
            vec![FlowNode::Text(TextNode::new(text, style))]
        };
        let even = ColumnWidth::Ratio(Ratio::new(1.0)?);
        let slots: Vec<_> = [(self.left, TextAlign::Start), (self.right, TextAlign::End)]
            .into_iter()
            .filter_map(|(text, align)| text.map(|text| (even, slot(text, align))))
            .collect();
        if slots.is_empty() {
            return Ok(Vec::new());
        }
        let inset = |margin: Length<NonNegative>| -> Result<Option<(ColumnWidth, Vec<FlowNode>)>> {
            if margin.as_pt() == 0.0 {
                return Ok(None);
            }
            Ok(Some((
                ColumnWidth::Fixed(Positive::pt(margin.as_pt())?),
                Vec::new(),
            )))
        };
        let margins = self.page.margins;
        let mut tracks: Vec<_> = inset(margins.left())?.into_iter().collect();
        tracks.extend(slots);
        tracks.extend(inset(margins.right())?);
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
    use crate::geom::A4;

    fn doc_with_font<'a>() -> Document<'a> {
        let mut doc = Document::new(A4);
        doc.font(fixture_bytes().to_vec()).unwrap();

        doc
    }

    #[test]
    fn paragraph_enters_the_flow_unwrapped() {
        let mut doc = doc_with_font();
        doc.paragraph(|p| p.text("A B"));
        assert!(matches!(
            doc.body.nodes.as_slice(),
            [FlowNode::Text(text)] if *text == TextNode::new("A B", default_style())
        ));
    }

    fn assert_fails_with_empty_canvas(doc: Document<'_>, expected: &Error) {
        let mut rec = RecordingCanvas::new();
        let result = doc.paint(&mut rec);
        assert_eq!(result.err().as_ref(), Some(expected));
        assert!(rec.commands().is_empty());
    }

    const MISSING: char = '\u{6f22}';

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
}
