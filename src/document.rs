//! Public document construction surface (Rust DSL).
//!
//! [`document`] runs the caller's closure once to assemble the body flow.
//! Header, footer, behind, and front closures run again on every page after
//! pagination, so only they see page numbers.

use std::num::{NonZeroU16, NonZeroU32};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::canvas::{Canvas, RecordedPage, RecordingCanvas};
use crate::error::{Error, Result};
use crate::fonts::{FontId, FontSource, FontStore};
use crate::geom::{Color, Length, Margins, NonNegative, Point, Positive, Size};
use crate::layout::columns::{default_column_gap, ColumnWidth, ColumnsNode, Ratio};
use crate::layout::decorate::{BandContent, Decorations, LayerContent, LayerText, PageInfo};
use crate::layout::flow::{FlowNode, KeepTogetherNode, LayoutCtx, SpacerNode};
use crate::layout::list::{ListItem, ListKind, ListNode, ListSpec, MarkerKind};
use crate::layout::paginate::{paginate_root, PaginateConfig};
use crate::layout::paragraph::TextNode;
use crate::layout::style::{Style, TextAlign};
use crate::layout::table::{self, TableNode, TableSpec};

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
    let fonts = doc.paint(&mut recording)?;
    Ok(Rendered { recording, fonts })
}

type BandHook<'a> = Box<dyn Fn(&mut Band) + 'a>;
type LayerHook<'a> = Box<dyn Fn(&mut Layer) + 'a>;

/// Document settings and page-time closures. Body methods come from [`Flow`].
pub struct Document<'a> {
    page_size: Size,
    fonts: FontStore,
    margins: Margins,
    body: Flow,
    header: Option<BandHook<'a>>,
    footer: Option<BandHook<'a>>,
    behind: Option<LayerHook<'a>>,
    front: Option<LayerHook<'a>>,
}

impl<'a> Document<'a> {
    fn new(page_size: Size) -> Self {
        Self {
            page_size,
            fonts: FontStore::new(),
            margins: Margins::default(),
            body: Flow::new(default_style()),
            header: None,
            footer: None,
            behind: None,
            front: None,
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

    fn paint(self, canvas: &mut dyn Canvas) -> Result<FontStore> {
        let Self {
            page_size,
            mut fonts,
            margins,
            body,
            header,
            footer,
            behind,
            front,
        } = self;
        let style = body.style.clone();
        let root = FlowNode::Stack(body.into_nodes()?);
        let decorations = Decorations {
            header: header.map(|hook| band_content(hook, style.clone())),
            footer: footer.map(|hook| band_content(hook, style.clone())),
            behind: behind.map(|hook| layer_content(hook, style.clone())),
            front: front.map(|hook| layer_content(hook, style.clone())),
        };
        let config = PaginateConfig { page_size, margins };
        let mut ctx = LayoutCtx { fonts: &mut fonts };
        paginate_root(root, &mut ctx, config, &decorations, canvas)?;
        Ok(fonts)
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
    nodes: Vec<FlowNode>,
    error: Option<Error>,
}

impl Flow {
    fn new(style: Style) -> Self {
        Self {
            style,
            nodes: Vec::new(),
            error: None,
        }
    }

    fn child(&self) -> Self {
        Self::new(self.style.clone())
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
                            Ok(table::Cell::new(vec![flow.text(cell.text)])?
                                .with_span(cell.col_span, cell.row_span))
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

/// One table cell of text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Cell {
    text: String,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
}

impl Cell {
    /// A cell spanning one column and one row.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
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
    texts: Vec<(String, Point)>,
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
        self.texts.push((s.as_ref().to_owned(), at));
    }
}

fn layer_content(hook: LayerHook<'_>, style: Style) -> LayerContent<'_> {
    Box::new(move |info: &PageInfo| {
        let mut layer = Layer {
            page: Page::from(info),
            texts: Vec::new(),
        };
        hook(&mut layer);
        layer
            .texts
            .into_iter()
            .map(|(text, at)| LayerText {
                text,
                at,
                style: style.clone(),
            })
            .collect()
    })
}

/// A laid-out document.
#[must_use]
pub struct Rendered {
    recording: RecordingCanvas,
    #[cfg_attr(not(feature = "backend-krilla"), allow(dead_code))]
    fonts: FontStore,
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
        let mut pdf = crate::backend::KrillaCanvas::new_with_fonts(Arc::new(self.fonts));
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
