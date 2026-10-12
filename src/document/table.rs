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
    pieces: Vec<Piece<Option<FlowNode>>>,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
    fill: Option<Color>,
}

/// One cell child. `L` is [`ListBlock`] on a [`Cell`] and the resolved list
/// node once the cell is stored on a row.
#[derive(Debug)]
enum Piece<L> {
    Text(String),
    Paragraph(Paragraph),
    Line(HLine),
    Image(PlacedImage),
    Spacer(Spacer),
    List(L),
}

impl<L> Piece<L> {
    fn map_list<M>(self, f: impl FnOnce(L) -> M) -> Piece<M> {
        match self {
            Piece::Text(text) => Piece::Text(text),
            Piece::Paragraph(paragraph) => Piece::Paragraph(paragraph),
            Piece::Line(line) => Piece::Line(line),
            Piece::Image(image) => Piece::Image(image),
            Piece::Spacer(spacer) => Piece::Spacer(spacer),
            Piece::List(list) => Piece::List(f(list)),
        }
    }
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
        let parent = &self.parent;
        let errors = &mut self.errors;
        StoredCell {
            pieces: cell
                .pieces
                .into_iter()
                .map(|piece| piece.map_list(|block| block.build_on(parent).lower(errors)))
                .collect(),
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
            match lower_pieces(flow, into, cell.pieces) {
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

/// Lowers every piece through the same [`IntoFlowItem`] path the body uses,
/// so sibling errors are still recorded after one piece fails.
fn lower_pieces(
    parent: &Flow,
    into: &mut ErrorBag,
    pieces: Vec<Piece<Option<FlowNode>>>,
) -> Option<Vec<FlowNode>> {
    let mut flow = parent.child();
    let mut ok = true;
    for piece in pieces {
        match piece {
            Piece::Text(text) => {
                let node = flow.text(text);
                flow.push(node);
            }
            Piece::Paragraph(paragraph) => paragraph.add_to(&mut flow),
            Piece::Line(line) => line.add_to(&mut flow),
            Piece::Image(image) => image.add_to(&mut flow),
            Piece::Spacer(spacer) => spacer.add_to(&mut flow),
            Piece::List(Some(node)) => flow.push(node),
            Piece::List(None) => ok = false,
        }
    }
    let (nodes, errors) = flow.into_parts();
    ok &= errors.is_empty();
    into.absorb(errors);
    ok.then_some(nodes)
}

mod seal {
    pub trait Sealed {}
}

/// Value [`Cell::add`] accepts.
///
/// Sealed. Implementors are [`Paragraph`], [`HLine`], [`PlacedImage`],
/// [`Spacer`], and [`ListBlock`]. [`Table`] and [`Columns`] implement
/// [`IntoFlowItem`] only, so they cannot go into a cell:
///
/// ```compile_fail
/// use quire_pdf::{Cell, Table};
/// let _ = Cell::empty().add(Table::build(|t| t.row(["a"])));
/// ```
///
/// Adding stores the value. Paragraphs, lines, images, and spacers lower with
/// the table's parent style and document token. A [`ListBlock`] runs when the
/// cell is written into a row or header, on that same parent.
pub trait IntoCellItem<'a>: seal::Sealed {
    /// Appends `self` to `cell`.
    fn add_to_cell(self, cell: &mut Cell<'a>);
}

impl seal::Sealed for Paragraph {}
impl IntoCellItem<'_> for Paragraph {
    fn add_to_cell(self, cell: &mut Cell<'_>) {
        cell.pieces.push(Piece::Paragraph(self));
    }
}

impl seal::Sealed for HLine {}
impl IntoCellItem<'_> for HLine {
    fn add_to_cell(self, cell: &mut Cell<'_>) {
        cell.pieces.push(Piece::Line(self));
    }
}

impl seal::Sealed for PlacedImage {}
impl IntoCellItem<'_> for PlacedImage {
    fn add_to_cell(self, cell: &mut Cell<'_>) {
        cell.pieces.push(Piece::Image(self));
    }
}

impl seal::Sealed for Spacer {}
impl IntoCellItem<'_> for Spacer {
    fn add_to_cell(self, cell: &mut Cell<'_>) {
        match self.into_result() {
            Ok(height) => cell.pieces.push(Piece::Spacer(Spacer::new(height))),
            Err(error) => cell.errors.record(error),
        }
    }
}

impl seal::Sealed for ListBlock<'_> {}
impl<'a> IntoCellItem<'a> for ListBlock<'a> {
    fn add_to_cell(self, cell: &mut Cell<'a>) {
        cell.pieces.push(Piece::List(self));
    }
}

/// One table cell.
///
/// Children stack top to bottom in the order they were added. A cell with no
/// children is a zero-height slot.
#[derive(Debug)]
#[must_use]
pub struct Cell<'a> {
    pieces: Vec<Piece<ListBlock<'a>>>,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
    fill: Option<Color>,
    errors: ErrorBag,
}

impl<'a> Cell<'a> {
    /// A one-by-one cell with no children.
    pub fn empty() -> Self {
        Self {
            pieces: Vec::new(),
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
            fill: None,
            errors: ErrorBag::default(),
        }
    }

    /// Appends one child below the earlier ones.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`].
    // Same verb as `FlowWriter::add`; a cell is not an arithmetic operand.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, item: impl IntoCellItem<'a>) -> Self {
        item.add_to_cell(&mut self);
        self
    }

    /// A one-by-one cell holding `text` in the table's text style.
    pub fn new(text: impl Into<String>) -> Self {
        let mut cell = Self::empty();
        cell.pieces.push(Piece::Text(text.into()));
        cell
    }

    /// A one-by-one cell holding one paragraph. The closure runs immediately.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`].
    pub fn paragraph(f: impl FnOnce(&mut Paragraph)) -> Self {
        Self::empty().add(Paragraph::build(f))
    }

    /// A one-by-one cell holding a bulleted list.
    ///
    /// The closure runs when the cell is written into a row. It may borrow
    /// locals that live until that `row` or `header` call returns.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`].
    pub fn ul(f: impl FnOnce(&mut List) + 'a) -> Self {
        Self::empty().add(ListBlock::bullet(f))
    }

    /// A one-by-one cell holding a numbered list starting at 1.
    ///
    /// The closure runs when the cell is written into a row. It may borrow
    /// locals that live until that `row` or `header` call returns.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`].
    pub fn ol(numbering: Numbering, f: impl FnOnce(&mut List) + 'a) -> Self {
        Self::empty().add(ListBlock::numbered(numbering, f))
    }

    /// A one-by-one cell holding `image`.
    pub fn image(image: &Image, f: impl FnOnce(&mut ImageBox)) -> Self {
        Self::empty().add(PlacedImage::build(image, f))
    }

    /// A one-by-one cell holding a horizontal line as wide as the cell content.
    pub fn hline(f: impl FnOnce(&mut HLine)) -> Self {
        Self::empty().add(HLine::build(f))
    }

    /// A one-by-one cell holding vertical space that paints nothing.
    ///
    /// Height may be zero. Taller than the cell's inner full frame is
    /// [`ErrorKind::UnsplittableTallerThanFrame`]. Unlike [`Flow::spacer`], the blank
    /// does not split across pages inside the cell.
    pub fn spacer(height: impl IntoNonNegative) -> Self {
        Self::empty().add(Spacer::new(height))
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
