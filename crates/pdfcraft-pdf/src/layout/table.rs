//! Table column sizing, row placement, optional repeating headers.

use std::collections::VecDeque;
use std::num::NonZeroU16;
use std::ops::Range;

use crate::error::{
    CellSpanReason, ColumnWidthsReason, Error, ErrorKind, Fallible as Result, GeometryKind,
};
use crate::geom::{Color, Finite, Length, NonNegative, Paint, Path, Point, Positive, Stroke};
use crate::layout::column_width::{self, ColumnWidth};
use crate::layout::frame::{self, Flowable, Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::nesting::CellBody;

/// Grid-line stroke centered on cell edges. Width zero draws nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Border {
    pub(crate) width: Length<NonNegative>,
    pub(crate) color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableSpec<N> {
    pub(crate) widths: Vec<ColumnWidth>,
    pub(crate) header: Vec<Vec<Cell<N>>>,
    pub(crate) body: Vec<Vec<Cell<N>>>,
    pub(crate) repeat_header: bool,
    pub(crate) padding: Length<NonNegative>,
    pub(crate) border: Border,
}

impl<N> TableSpec<N> {
    pub(crate) fn new(widths: Vec<ColumnWidth>) -> Self {
        Self {
            widths,
            header: Vec::new(),
            body: Vec::new(),
            repeat_header: true,
            padding: NonNegative::ZERO,
            border: Border {
                width: NonNegative::ZERO,
                color: Color::rgb(0, 0, 0),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cell<N> {
    body: CellBody<N>,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
    fill: Option<Color>,
}

impl<N: Flowable> Cell<N> {
    pub(crate) fn new(content: impl IntoIterator<Item = N>) -> Result<Self> {
        Ok(Self {
            body: CellBody::new(content)?,
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
            fill: None,
        })
    }

    pub(crate) fn with_span(self, cols: NonZeroU16, rows: NonZeroU16) -> Self {
        Self {
            col_span: cols,
            row_span: rows,
            ..self
        }
    }

    pub(crate) fn with_fill(self, fill: Option<Color>) -> Self {
        Self { fill, ..self }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableNode<N>(TableState<N>);

#[derive(Debug, Clone, PartialEq)]
enum TableState<N> {
    Open(GridTable<N>),
    Measured(MeasuredTable),
}

#[derive(Debug, Clone, PartialEq)]
struct GridTable<N> {
    widths: Vec<ColumnWidth>,
    padding: Length<NonNegative>,
    border: Border,
    header_rows: usize,
    rows: usize,
    cells: Vec<PlacedCell<N>>,
    repeat_header: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    row: usize,
    col: usize,
    cols: usize,
    rows: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct PlacedCell<N> {
    span: Span,
    body: CellBody<N>,
    fill: Option<Color>,
}

struct LaidCell {
    span: Span,
    height: Length<NonNegative>,
    content: Fragment,
    fill: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredTable {
    width: Length<Positive>,
    header: Option<Fragment>,
    body: VecDeque<Fragment>,
    repeat_header: bool,
    draw_header_next: bool,
}

impl<N: Flowable> TableNode<N> {
    pub(crate) fn new(spec: TableSpec<N>) -> Result<Self> {
        let cols = spec.widths.len();
        if cols == 0 {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::ColumnCount,
            }));
        }
        let header_rows = spec.header.len();
        let rows = header_rows + spec.body.len();
        let mut taken = vec![vec![false; cols]; rows];
        let mut cells = Vec::new();
        for (row, row_cells) in spec.header.into_iter().chain(spec.body).enumerate() {
            let row_limit = if row < header_rows { header_rows } else { rows };
            let mut col = 0;
            for cell in row_cells {
                while col < cols && taken[row][col] {
                    col += 1;
                }
                let col_span = usize::from(cell.col_span.get());
                let row_span = usize::from(cell.row_span.get());
                if col + col_span > cols || row + row_span > row_limit {
                    let reason = if col + col_span <= cols && row + row_span <= rows {
                        CellSpanReason::CrossesHeader
                    } else {
                        CellSpanReason::OutOfGrid
                    };
                    return Err(Error::from(ErrorKind::InvalidCellSpan(reason)).in_cell(row, col));
                }
                for (r, slots) in taken[row..row + row_span].iter_mut().enumerate() {
                    for (c, slot) in slots[col..col + col_span].iter_mut().enumerate() {
                        if *slot {
                            return Err(Error::from(ErrorKind::InvalidCellSpan(
                                CellSpanReason::Overlap,
                            ))
                            .in_cell(row + r, col + c));
                        }
                        *slot = true;
                    }
                }
                cells.push(PlacedCell {
                    span: Span {
                        row,
                        col,
                        cols: col_span,
                        rows: row_span,
                    },
                    body: cell.body,
                    fill: cell.fill,
                });
                col += col_span;
            }
        }
        for (row, slots) in taken.iter().enumerate() {
            if let Some(col) = slots.iter().position(|slot| !slot) {
                return Err(
                    Error::from(ErrorKind::InvalidCellSpan(CellSpanReason::EmptySlot))
                        .in_cell(row, col),
                );
            }
        }
        Ok(Self(TableState::Open(GridTable {
            widths: spec.widths,
            padding: spec.padding,
            border: spec.border,
            header_rows,
            rows,
            cells,
            repeat_header: spec.repeat_header,
        })))
    }

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>> {
        let table = match self.0 {
            TableState::Open(grid) => measure(grid, ctx, avail)?,
            TableState::Measured(table) => table,
        };
        split_measured(table, avail)
    }

    pub(crate) fn open_cell_count(&self) -> usize {
        match &self.0 {
            TableState::Open(grid) => grid.cells.len(),
            TableState::Measured(_) => 0,
        }
    }

    pub(crate) fn append_open_children<'a>(&'a self, out: &mut Vec<&'a N>) {
        if let TableState::Open(grid) = &self.0 {
            for cell in &grid.cells {
                out.extend(cell.body.nodes());
            }
        }
    }
}

fn column_edges(widths: &[Length<Positive>]) -> Vec<Length<NonNegative>> {
    let mut xs = Vec::with_capacity(widths.len() + 1);
    let mut x = NonNegative::ZERO;
    for width in widths {
        xs.push(x);
        x += *width;
    }
    xs.push(x);
    xs
}

fn measure<N: Flowable>(
    grid: GridTable<N>,
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
) -> Result<MeasuredTable> {
    let widths = column_width::resolve_column_widths(&grid.widths, avail.width)?;
    let xs = column_edges(&widths);

    let border = grid.border.width.as_pt();
    let inset = border / 2.0 + grid.padding.as_pt();
    let inner_full_pt = avail.full_height.as_pt() - 2.0 * inset;
    if inner_full_pt <= 0.0 {
        return Err(ErrorKind::UnsplittableTallerThanFrame {
            height: 2.0 * inset,
            avail: avail.full_height.as_pt(),
        }
        .into());
    }
    let inner_full = Length::<Positive>::from_trusted_pt(inner_full_pt);

    let mut laid = Vec::with_capacity(grid.cells.len());
    let mut row_h = vec![NonNegative::ZERO; grid.rows];
    for PlacedCell {
        span: cell,
        body,
        fill,
    } in grid.cells
    {
        let inner_w = xs[cell.col + cell.cols].as_pt() - xs[cell.col].as_pt() - 2.0 * inset;
        if inner_w <= 0.0 {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::ContentWidth,
            })
            .in_cell(cell.row, cell.col));
        }
        let frame = Frame {
            width: Length::from_trusted_pt(inner_w),
            height: inner_full.into(),
            full_height: inner_full,
        };
        let (h, content) = ctx.located(
            &crate::error::Location::at_cell(cell.row, cell.col),
            |ctx| frame::natural_height(body.into_node(), ctx, frame),
        )?;
        let cell_h = Length::<NonNegative>::from_trusted_pt(h.as_pt() + 2.0 * inset);
        if cell.rows == 1 {
            row_h[cell.row] = row_h[cell.row].max(cell_h);
        }
        laid.push(LaidCell {
            span: cell,
            height: cell_h,
            content,
            fill,
        });
    }
    let col_x: Vec<f32> = xs.iter().map(|value| value.as_pt()).collect();
    let row_y = row_offsets(row_h, &laid);
    let groups = row_groups(&laid, grid.header_rows, grid.rows);
    let fragments = GridPaint {
        col_x: &col_x,
        row_y: &row_y,
        groups: &groups,
        content_inset: inset,
        border: Positive::pt(border)
            .ok()
            .map(|width| (Stroke::new(width), Paint::solid(grid.border.color))),
    }
    .paint(laid)?;

    let mut fragments = VecDeque::from(fragments);
    let header = if grid.header_rows > 0 {
        fragments.pop_front()
    } else {
        None
    };
    let header_h = header
        .as_ref()
        .map_or(NonNegative::ZERO, |header| header.height);
    for (index, block) in fragments.iter().enumerate() {
        let with_header = index == 0 || grid.repeat_header;
        let need = block.height
            + if with_header {
                header_h
            } else {
                NonNegative::ZERO
            };
        if need.as_pt() > avail.full_height.as_pt() {
            return Err(ErrorKind::UnsplittableTallerThanFrame {
                height: need.as_pt(),
                avail: avail.full_height.as_pt(),
            }
            .into());
        }
    }
    if header_h.as_pt() > avail.full_height.as_pt() {
        return Err(ErrorKind::UnsplittableTallerThanFrame {
            height: header_h.as_pt(),
            avail: avail.full_height.as_pt(),
        }
        .into());
    }
    Ok(MeasuredTable {
        width: avail.width,
        header,
        body: fragments,
        repeat_header: grid.repeat_header,
        draw_header_next: true,
    })
}

/// Shared grid geometry used while turning measured cells into draw commands.
struct GridPaint<'a> {
    col_x: &'a [f32],
    row_y: &'a [f32],
    groups: &'a [Range<usize>],
    content_inset: f32,
    border: Option<(Stroke, Paint)>,
}

impl GridPaint<'_> {
    fn paint(self, laid: Vec<LaidCell>) -> Result<Vec<Fragment>> {
        let mut fragments: Vec<Fragment> = self
            .groups
            .iter()
            .map(|rows| Fragment {
                height: Length::from_trusted_pt(self.row_y[rows.end] - self.row_y[rows.start]),
                spacing_after: NonNegative::ZERO,
                commands: Vec::new(),
                anchors: Vec::new(),
            })
            .collect();
        for LaidCell {
            span: cell,
            content,
            fill,
            ..
        } in laid
        {
            let group = self
                .groups
                .iter()
                .position(|rows| rows.contains(&cell.row))
                .expect("every row belongs to a group");
            let x = self.col_x[cell.col];
            let y = self.row_y[cell.row] - self.row_y[self.groups[group].start];
            let w = self.col_x[cell.col + cell.cols] - x;
            let h = self.row_y[cell.row + cell.rows] - self.row_y[cell.row];
            let fragment = &mut fragments[group];
            if let Some(color) = fill {
                fragment.commands.push(FragmentCommand::Fill {
                    path: rect_path(x, y, w, h)?,
                    paint: Paint::solid(color),
                });
            }
            let offset = content.with_offset(
                Length::from_trusted_pt(x + self.content_inset),
                Length::from_trusted_pt(y + self.content_inset),
            )?;
            fragment.commands.extend(offset.commands);
            fragment.anchors.extend(offset.anchors);
            if let Some((stroke, paint)) = self.border {
                fragment.commands.push(FragmentCommand::Stroke {
                    path: rect_path(x, y, w, h)?,
                    paint,
                    stroke,
                });
            }
        }
        Ok(fragments)
    }
}

fn row_offsets(mut row_h: Vec<Length<NonNegative>>, laid: &[LaidCell]) -> Vec<f32> {
    let mut spanning: Vec<_> = laid.iter().filter(|cell| cell.span.rows > 1).collect();
    spanning.sort_by_key(|cell| cell.span.rows);
    for cell in spanning {
        let rows = cell.span.row..cell.span.row + cell.span.rows;
        let have: f32 = row_h[rows.clone()]
            .iter()
            .map(|height| height.as_pt())
            .sum();
        if cell.height.as_pt() > have {
            let grown = row_h[rows.end - 1].as_pt() + cell.height.as_pt() - have;
            row_h[rows.end - 1] = Length::from_trusted_pt(grown);
        }
    }
    let mut row_y = Vec::with_capacity(row_h.len() + 1);
    let mut y = 0.0f32;
    for h in &row_h {
        row_y.push(y);
        y += h.as_pt();
    }
    row_y.push(y);
    row_y
}

fn row_groups(laid: &[LaidCell], header_rows: usize, rows: usize) -> Vec<Range<usize>> {
    let mut reach: Vec<usize> = (0..rows).collect();
    for LaidCell { span, .. } in laid {
        reach[span.row] = reach[span.row].max(span.row + span.rows - 1);
    }
    let mut groups = Vec::new();
    if header_rows > 0 {
        groups.push(0..header_rows);
    }
    let mut start = header_rows;
    while start < rows {
        let mut end = start;
        let mut row = start;
        while row <= end {
            end = end.max(reach[row]);
            row += 1;
        }
        groups.push(start..end + 1);
        start = end + 1;
    }
    groups
}

fn split_measured<N>(mut table: MeasuredTable, avail: Frame) -> Result<Split<TableNode<N>>> {
    if table.width.as_pt() > avail.width.as_pt() {
        return Err(GeometryKind::LaidOutWiderThanFrame {
            width: table.width.as_pt(),
            avail: avail.width.as_pt(),
        }
        .into());
    }
    let mut y = NonNegative::ZERO;
    let mut commands = Vec::new();
    let mut anchors = Vec::new();
    if table.draw_header_next {
        if let Some(header) = &table.header {
            let first = table
                .body
                .front()
                .map_or(NonNegative::ZERO, |block| block.height);
            if (header.height + first).as_pt() > avail.height.as_pt() {
                return Ok(Split::MoveToNextPage(TableNode(TableState::Measured(
                    table,
                ))));
            }
            commands.extend(header.commands.iter().cloned());
            anchors.extend(header.anchors.iter().cloned());
            y = header.height;
        }
    }
    let mut placed = 0usize;
    while let Some(block) = table.body.front() {
        if (y + block.height).as_pt() > avail.height.as_pt() {
            break;
        }
        let block = table.body.pop_front().expect("front exists");
        let h = block.height;
        let offset = block.with_offset(Finite::ZERO, y.into())?;
        commands.extend(offset.commands);
        anchors.extend(offset.anchors);
        y += h;
        placed += 1;
    }
    let head = Fragment {
        height: y,
        spacing_after: NonNegative::ZERO,
        commands,
        anchors,
    };
    if table.body.is_empty() {
        return Ok(Split::Fit {
            height: y,
            fragment: head,
        });
    }
    if placed == 0 {
        return Ok(Split::MoveToNextPage(TableNode(TableState::Measured(
            table,
        ))));
    }
    table.draw_header_next = table.repeat_header && table.header.is_some();
    Ok(Split::Partial {
        head_height: y,
        head,
        tail: TableNode(TableState::Measured(table)),
    })
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Result<Path> {
    Ok(Path::begin(Point::try_pt(x, y)?)
        .line_to(Point::try_pt(x + w, y)?)
        .line_to(Point::try_pt(x + w, y + h)?)
        .line_to(Point::try_pt(x, y + h)?)
        .close())
}
