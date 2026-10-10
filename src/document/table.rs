#![allow(clippy::wildcard_imports)]
use super::*;

/// A table or column track width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    /// A length in points. Validated when the table is assembled.
    Fixed(f32),
    /// A positive finite weight. Rejected at conversion with [`Error::ColumnWidths`].
    Ratio(f32),
    /// Remaining width after Fixed tracks. At most one Auto per set of tracks.
    Auto,
}

impl Track {
    fn into_width(self) -> Result<ColumnWidth> {
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
    pub(crate) error: Option<Error>,
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
    List(Result<FlowNode>),
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
            error: None,
        };
        f(&mut table);
        table
    }

    /// Builds a table against a detached parent style.
    ///
    /// Prefer [`FlowWriter::table`] when writing into a live flow so nested
    /// content sees that flow's style. Use this when a helper returns a table
    /// for [`FlowWriter::add`].
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
        if let Some(error) = cell.error {
            self.note(error);
        }
        let content = match cell.content {
            CellContent::Text(text) => StoredContent::Text(text),
            CellContent::Paragraph(paragraph) => StoredContent::Paragraph(paragraph),
            CellContent::Image(image, image_box) => StoredContent::Image(image, image_box),
            CellContent::HLine(line) => StoredContent::HLine(line),
            CellContent::Spacer(height) => StoredContent::Spacer(height),
            CellContent::List(DeferredList::Bullet(build)) => {
                StoredContent::List(assemble_list(self.parent.child(), ListKind::Bullet, build))
            }
            CellContent::List(DeferredList::Ordered(numbering, build)) => {
                StoredContent::List(assemble_list(
                    self.parent.child(),
                    ListKind::Ordered {
                        style: numbering_kind(numbering),
                        start: NonZeroU32::MIN,
                    },
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
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    pub(crate) fn lower(self) -> Result<FlowNode> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let flow = &self.parent;
        let first = self.header.first().or(self.body.first());
        let cols: usize = first.map_or(0, |row| {
            row.iter()
                .map(|cell| usize::from(cell.col_span.get()))
                .sum()
        });
        let even = ColumnWidth::Ratio(Ratio::new(1.0)?);
        let widths = match self.tracks {
            Some(tracks) => {
                if tracks.len() != cols {
                    return Err(Error::ColumnWidths {
                        reason: "column-count",
                    });
                }
                tracks
                    .into_iter()
                    .map(Track::into_width)
                    .collect::<Result<Vec<_>>>()?
            }
            None => vec![even; cols],
        };
        reject_auto_with_ratio(&widths)?;
        let lower_rows = |rows: Vec<Vec<StoredCell>>| -> Result<Vec<Vec<layout_table::Cell>>> {
            rows.into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|cell| {
                            let content = match cell.content {
                                StoredContent::Text(text) => vec![flow.text(text)],
                                StoredContent::Paragraph(paragraph) => {
                                    vec![lower_paragraph(paragraph, &flow.style)?]
                                }
                                StoredContent::List(node) => vec![node?],
                                StoredContent::Image(image, image_box) => {
                                    vec![image_box.lower(&image, flow.doc)?]
                                }
                                StoredContent::HLine(line) => vec![line.lower()?],
                                StoredContent::Spacer(height) => {
                                    vec![FlowNode::Spacer(SpacerNode { height })]
                                }
                            };
                            Ok(layout_table::Cell::new(content)?
                                .with_span(cell.col_span, cell.row_span)
                                .with_fill(cell.fill))
                        })
                        .collect()
                })
                .collect()
        };
        let spec = TableSpec {
            header: lower_rows(self.header)?,
            body: lower_rows(self.body)?,
            repeat_header: self.repeat_header,
            padding: self.padding,
            border: layout_table::Border {
                width: self.border.width,
                color: self.border.color,
            },
            ..TableSpec::new(widths)
        };
        Ok(FlowNode::Table(TableNode::new(spec)?))
    }
}

/// One table cell.
#[derive(Debug)]
#[must_use]
pub struct Cell<'a> {
    pub(crate) content: CellContent<'a>,
    pub(crate) col_span: NonZeroU16,
    pub(crate) row_span: NonZeroU16,
    pub(crate) fill: Option<Color>,
    pub(crate) error: Option<Error>,
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
            error: None,
        }
    }

    /// A cell spanning one column and one row.
    pub fn new(text: impl Into<String>) -> Self {
        Self::bare(CellContent::Text(text.into()))
    }

    /// A one-by-one cell holding one paragraph. The closure runs immediately.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`].
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
    /// [`Error::UnsplittableTallerThanFrame`].
    pub fn ul(f: impl FnOnce(&mut List) + 'a) -> Self {
        Self::bare(CellContent::List(DeferredList::Bullet(Box::new(f))))
    }

    /// A one-by-one cell holding a numbered list starting at 1.
    ///
    /// The closure runs when the cell is written into a row. It may borrow
    /// locals that live until that `row` or `header` call returns.
    ///
    /// Content taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`].
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
    /// [`Error::UnsplittableTallerThanFrame`]. Unlike [`Flow::spacer`], the blank
    /// does not split across pages inside the cell.
    pub fn spacer(height: impl IntoNonNegative) -> Self {
        match height.into_non_negative() {
            Ok(height) => Self::bare(CellContent::Spacer(height)),
            Err(error) => Self {
                error: Some(error),
                ..Self::bare(CellContent::Spacer(NonNegative::ZERO))
            },
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
