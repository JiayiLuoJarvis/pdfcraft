#![allow(clippy::wildcard_imports)]
use super::*;

/// A table or column track width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    /// A length in points. Validated when the table is assembled.
    Fixed(f32),
    /// A positive finite weight. Rejected at conversion with [`ErrorKind::ColumnWidths`].
    Ratio(f32),
    /// Remaining width after Fixed tracks. At most one Auto per set of tracks.
    Auto,
}

impl Track {
    fn into_width(self) -> Fallible<ColumnWidth> {
        match self {
            Self::Fixed(width) => Ok(ColumnWidth::Fixed(Positive::pt(width)?)),
            Self::Ratio(weight) => Ok(ColumnWidth::Ratio(Ratio::new(weight)?)),
            Self::Auto => Ok(ColumnWidth::Auto),
        }
    }
}

/// Grid-line stroke. Width zero draws nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub(crate) width: Length<NonNegative>,
    pub(crate) color: Color,
}

/// Rows being added to a table.
pub struct Table {
    pub(crate) parent: Flow,
    pub(crate) header: Vec<Vec<StoredCell>>,
    pub(crate) body: Vec<Vec<StoredCell>>,
    pub(crate) repeat_header: bool,
    pub(crate) tracks: Option<Vec<Track>>,
    pub(crate) padding: Length<NonNegative>,
    pub(crate) border: Border,
    pub(crate) errors: ErrorBag,
}

pub(crate) struct StoredCell {
    pub(crate) content: StoredContent,
    pub(crate) col_span: NonZeroU16,
    pub(crate) row_span: NonZeroU16,
    pub(crate) fill: Option<Color>,
}

pub(crate) enum StoredContent {
    Text(String),
    Paragraph(Paragraph),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
    List(Option<FlowNode>),
}

impl Table {
    /// Builds a table using `parent`'s style and document token.
    pub(crate) fn build_on(parent: &Flow, f: impl FnOnce(&mut Self)) -> Self {
        let mut table = Self {
            parent: parent.child(),
            header: Vec::new(),
            body: Vec::new(),
            repeat_header: true,
            tracks: None,
            padding: NonNegative::ZERO,
            border: Border {
                width: NonNegative::ZERO,
                color: Color::rgb(0, 0, 0),
            },
            errors: ErrorBag::default(),
        };
        f(&mut table);
        table
    }

    /// Builds a table on a detached flow that uses the default text style.
    ///
    /// Nested content keeps that build-time style. Prefer [`FlowWriter::table`]
    /// when writing into a live flow so nested content sees that flow's style.
    /// Use this when a helper returns a table for [`FlowWriter::add`].
    pub fn build(f: impl FnOnce(&mut Self)) -> Self {
        Self::build_on(&Flow::new(default_style(), DocToken::next()), f)
    }

    /// Adds a header row. Several calls make a multi-row header.
    pub fn header<'c, I>(&mut self, cells: I)
    where
        I: IntoIterator,
        I::Item: Into<Cell<'c>>,
    {
        let row = cells.into_iter().map(|c| self.store(c.into())).collect();
        self.header.push(row);
    }

    /// Adds a body row.
    pub fn row<'c, I>(&mut self, cells: I)
    where
        I: IntoIterator,
        I::Item: Into<Cell<'c>>,
    {
        let row = cells.into_iter().map(|c| self.store(c.into())).collect();
        self.body.push(row);
    }

    fn store(&mut self, cell: Cell<'_>) -> StoredCell {
        self.errors.absorb(cell.errors);
        let content = match cell.content {
            CellContent::Text(text) => StoredContent::Text(text),
            CellContent::Paragraph(paragraph) => StoredContent::Paragraph(paragraph),
            CellContent::Image(image, image_box) => StoredContent::Image(image, image_box),
            CellContent::HLine(line) => StoredContent::HLine(line),
            CellContent::Spacer(height) => StoredContent::Spacer(height),
            CellContent::List(DeferredList::Bullet(build)) => StoredContent::List(assemble_list(
                self.parent.child(),
                ListKind::Bullet,
                &mut self.errors,
                build,
            )),
            CellContent::List(DeferredList::Ordered(numbering, build)) => {
                StoredContent::List(assemble_list(
                    self.parent.child(),
                    ListKind::Ordered {
                        style: numbering_kind(numbering),
                        start: NonZeroU32::MIN,
                    },
                    &mut self.errors,
                    build,
                ))
            }
        };
        StoredCell {
            content,
            col_span: cell.col_span,
            row_span: cell.row_span,
            fill: cell.fill,
        }
    }

    /// Whether header rows repeat at the top of each page the table continues on. Default on.
    pub fn repeat_header(&mut self, on: bool) {
        self.repeat_header = on;
    }

    /// Sets per-column widths. Length must match the first row's span sum.
    pub fn tracks(&mut self, tracks: impl IntoIterator<Item = Track>) {
        self.tracks = Some(tracks.into_iter().collect());
    }

    /// Inner padding on every side of each cell. Default zero.
    pub fn cell_padding(&mut self, padding: impl IntoNonNegative) {
        match padding.into_non_negative() {
            Ok(padding) => self.padding = padding,
            Err(error) => self.note(error),
        }
    }

    /// Grid-line stroke. Width zero draws nothing. Default black, width zero.
    pub fn border(&mut self, width: impl IntoNonNegative, color: Color) {
        match width.into_non_negative() {
            Ok(width) => self.border = Border { width, color },
            Err(error) => self.note(error),
        }
    }

    pub(crate) fn note(&mut self, error: Error) {
        self.errors.record(error);
    }

    pub(crate) fn lower(self, into: &mut ErrorBag) -> Option<FlowNode> {
        let failed = !self.errors.is_empty();
        into.absorb(self.errors);
        let flow = &self.parent;
        let first = self.header.first().or(self.body.first());
        let cols: usize = first.map_or(0, |row| {
            row.iter()
                .map(|cell| usize::from(cell.col_span.get()))
                .sum()
        });
        let header = lower_stored_rows(flow, into, self.header);
        let body = lower_stored_rows(flow, into, self.body);
        let mut blocked = failed || header.is_none() || body.is_none();
        let widths = match self.tracks {
            Some(tracks) => table_widths(tracks, cols, &mut blocked, into),
            None => Some(vec![
                ColumnWidth::Ratio(
                    Ratio::new(1.0).expect("1 is positive")
                );
                cols
            ]),
        };
        if let Some(widths) = widths.as_ref() {
            if let Err(error) = reject_auto_with_ratio(widths) {
                into.record(error);
                blocked = true;
            }
        }
        if blocked {
            return None;
        }
        let (Some(header), Some(body), Some(widths)) = (header, body, widths) else {
            return None;
        };
        let spec = TableSpec {
            header,
            body,
            repeat_header: self.repeat_header,
            padding: self.padding,
            border: layout_table::Border {
                width: self.border.width,
                color: self.border.color,
            },
            ..TableSpec::new(widths)
        };
        match TableNode::new(spec) {
            Ok(node) => Some(FlowNode::Table(node)),
            Err(error) => {
                into.record(error);
                None
            }
        }
    }
}

fn table_widths(
    tracks: Vec<Track>,
    cols: usize,
    blocked: &mut bool,
    into: &mut ErrorBag,
) -> Option<Vec<ColumnWidth>> {
    if tracks.len() == cols {
        let mut widths = Vec::new();
        for track in tracks {
            match track.into_width() {
                Ok(width) => widths.push(width),
                Err(error) => {
                    into.record(error);
                    *blocked = true;
                }
            }
        }
        if *blocked {
            None
        } else {
            Some(widths)
        }
    } else {
        into.record(Error::from(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::ColumnCount,
        }));
        *blocked = true;
        None
    }
}

fn lower_stored_rows(
    flow: &Flow,
    into: &mut ErrorBag,
    rows: Vec<Vec<StoredCell>>,
) -> Option<Vec<Vec<layout_table::Cell<FlowNode>>>> {
    let mut out = Vec::with_capacity(rows.len());
    let mut ok = true;
    for row in rows {
        let mut stored = Vec::with_capacity(row.len());
        for cell in row {
            let span_col = cell.col_span;
            let span_row = cell.row_span;
            let fill = cell.fill;
            let content = match cell.content {
                StoredContent::Text(text) => Some(vec![flow.text(text)]),
                StoredContent::Paragraph(paragraph) => {
                    lower_paragraph(paragraph, &flow.style, into).map(|node| vec![node])
                }
                StoredContent::List(node) => node.map(|node| vec![node]),
                StoredContent::Image(image, image_box) => image_box
                    .lower(&image, flow.doc, into)
                    .map(|node| vec![node]),
                StoredContent::HLine(line) => line.lower(into).map(|node| vec![node]),
                StoredContent::Spacer(height) => {
                    Some(vec![FlowNode::Spacer(SpacerNode { height })])
                }
            };
            match content {
                Some(content) => match layout_table::Cell::new(content) {
                    Ok(built) => {
                        stored.push(built.with_span(span_col, span_row).with_fill(fill));
                    }
                    Err(error) => {
                        into.record(error);
                        ok = false;
                    }
                },
                None => ok = false,
            }
        }
        out.push(stored);
    }
    ok.then_some(out)
}

/// One table cell.
#[derive(Debug)]
#[must_use]
pub struct Cell<'a> {
    pub(crate) content: CellContent<'a>,
    pub(crate) col_span: NonZeroU16,
    pub(crate) row_span: NonZeroU16,
    pub(crate) fill: Option<Color>,
    pub(crate) errors: ErrorBag,
}

#[derive(Debug)]
pub(crate) enum CellContent<'a> {
    Text(String),
    Paragraph(Paragraph),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
    List(DeferredList<'a>),
}

pub(crate) enum DeferredList<'a> {
    Bullet(Box<dyn FnOnce(&mut List) + 'a>),
    Ordered(Numbering, Box<dyn FnOnce(&mut List) + 'a>),
}

impl std::fmt::Debug for DeferredList<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeferredList::Bullet(_) => f.debug_tuple("Bullet").finish(),
            DeferredList::Ordered(numbering, _) => {
                f.debug_tuple("Ordered").field(numbering).finish()
            }
        }
    }
}

impl<'a> Cell<'a> {
    fn bare(content: CellContent<'a>) -> Self {
        Self {
            content,
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
            fill: None,
            errors: ErrorBag::default(),
        }
    }

    /// A cell spanning one column and one row.
    pub fn new(text: impl Into<String>) -> Self {
        Self::bare(CellContent::Text(text.into()))
    }

    /// A one-by-one cell holding one paragraph. The closure runs immediately.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`].
    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self {
        let mut paragraph = Paragraph::default();
        f(&mut paragraph);
        Self::bare(CellContent::Paragraph(paragraph))
    }

    /// A one-by-one cell holding a bulleted list.
    ///
    /// The closure runs when the cell is written into a row. It may borrow
    /// locals that live until that `row` or `header` call returns.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`].
    pub fn ul(f: impl FnOnce(&mut List) + 'a) -> Self {
        Self::bare(CellContent::List(DeferredList::Bullet(Box::new(f))))
    }

    /// A one-by-one cell holding a numbered list starting at 1.
    ///
    /// The closure runs when the cell is written into a row. It may borrow
    /// locals that live until that `row` or `header` call returns.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`].
    pub fn ol(numbering: Numbering, f: impl FnOnce(&mut List) + 'a) -> Self {
        Self::bare(CellContent::List(DeferredList::Ordered(
            numbering,
            Box::new(f),
        )))
    }

    /// A one-by-one cell holding `image`.
    pub fn image(image: &Image, f: impl FnOnce(&mut ImageBox)) -> Self {
        let mut image_box = ImageBox::default();
        f(&mut image_box);
        Self::bare(CellContent::Image(*image, image_box))
    }

    /// A one-by-one cell holding a horizontal line as wide as the cell content.
    pub fn hline(f: impl FnOnce(&mut HLine)) -> Self {
        let mut line = HLine::default();
        f(&mut line);
        Self::bare(CellContent::HLine(line))
    }

    /// A one-by-one cell holding vertical space that paints nothing.
    ///
    /// Height may be zero. Taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`]. Unlike [`Flow::spacer`], the blank
    /// does not split across pages inside the cell.
    pub fn spacer(height: impl IntoNonNegative) -> Self {
        match height.into_non_negative() {
            Ok(height) => Self::bare(CellContent::Spacer(height)),
            Err(error) => {
                let mut cell = Self::bare(CellContent::Spacer(NonNegative::ZERO));
                cell.errors.record(error);
                cell
            }
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

    /// Solid background behind the cell content. Drawn under text and under the grid stroke.
    pub fn fill(self, color: Color) -> Self {
        Self {
            fill: Some(color),
            ..self
        }
    }
}

impl From<&str> for Cell<'_> {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Cell<'_> {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&String> for Cell<'_> {
    fn from(text: &String) -> Self {
        Self::new(text.as_str())
    }
}
