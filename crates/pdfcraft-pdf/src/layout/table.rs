//! Table column sizing, row placement, optional repeating headers.

use std::collections::VecDeque;
use std::num::NonZeroU16;
use std::ops::Range;

use crate::error::{
    CellSpanReason, ColumnWidthsReason, Error, ErrorKind, Fallible as Result, GeometryKind,
};
use crate::geom::{
    Color, Finite, Length, NonNegative, Offset, Paint, Path, PathBuilder, Pen, Point, Positive,
    Stroke,
};
use crate::layout::column_width::{self, ColumnWidth};
use crate::layout::frame::{self, Flowable, Fragment, FragmentCommand, Frame, LayoutCtx, Split};
use crate::layout::nesting::CellBody;

/// Rules for grid or three-line tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Rules {
    Grid(Pen),
    ThreeLine {
        top: Pen,
        header: Pen,
        bottom: Pen,
    },
    #[default]
    None,
}

impl Rules {
    /// Half the grid stroke, or zero. Cell geometry is shifted by this so strokes sit on the outer edge.
    pub(crate) fn border_inset(&self) -> Length<NonNegative> {
        match self {
            Self::Grid(pen) => Length::from_trusted_pt(pen.width().as_pt() / 2.0),
            Self::ThreeLine { .. } | Self::None => NonNegative::ZERO,
        }
    }

    /// Column width after subtracting the grid stroke. `None` when the stroke eats the frame.
    pub(crate) fn inner_content_width(
        &self,
        avail_width: Length<Positive>,
    ) -> Option<Length<Positive>> {
        match self {
            Self::Grid(pen) => {
                let border = pen.width().as_pt();
                if border > 0.0 {
                    let rem = avail_width.as_pt() - border;
                    (rem > 0.0).then(|| Length::from_trusted_pt(rem))
                } else {
                    Some(avail_width)
                }
            }
            Self::ThreeLine { .. } | Self::None => Some(avail_width),
        }
    }

    /// Height of the header, first body group, and this rule's outer strokes.
    pub(crate) fn opening_clearance(
        &self,
        header_h: Length<NonNegative>,
        first_row_h: Length<NonNegative>,
        has_header: bool,
    ) -> Length<NonNegative> {
        self.row_group_required_height(first_row_h, header_h, has_header)
    }

    /// Height a row group needs on a page, including a repeated header when `with_header`.
    pub(crate) fn row_group_required_height(
        &self,
        group_h: Length<NonNegative>,
        header_h: Length<NonNegative>,
        with_header: bool,
    ) -> Length<NonNegative> {
        match (self, with_header) {
            (Self::Grid(pen), true) => group_h + header_h + pen.width(),
            (Self::Grid(pen), false) => group_h + pen.width(),
            (
                Self::ThreeLine {
                    top,
                    header,
                    bottom,
                },
                true,
            ) => group_h + top.width() + bottom.width() + header_h + header.width(),
            (Self::ThreeLine { top, bottom, .. }, false) => group_h + top.width() + bottom.width(),
            (Self::None, true) => group_h + header_h,
            (Self::None, false) => group_h,
        }
    }

    /// Height of the header band and its strokes, with no body row.
    pub(crate) fn header_clearance(
        &self,
        header_h: Length<NonNegative>,
        has_header: bool,
    ) -> Length<NonNegative> {
        self.row_group_required_height(NonNegative::ZERO, header_h, has_header)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableSpec<N> {
    pub(crate) widths: Vec<ColumnWidth>,
    pub(crate) header: Vec<Vec<Cell<N>>>,
    pub(crate) body: Vec<Vec<Cell<N>>>,
    pub(crate) repeat_header: bool,
    pub(crate) padding: Length<NonNegative>,
    pub(crate) rules: Rules,
}

impl<N> TableSpec<N> {
    pub(crate) fn new(widths: Vec<ColumnWidth>) -> Self {
        Self {
            widths,
            header: Vec::new(),
            body: Vec::new(),
            repeat_header: true,
            padding: NonNegative::ZERO,
            rules: Rules::default(),
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

/// Placed span of one cell in the table grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellSpan {
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) cols: usize,
    pub(crate) rows: usize,
}

/// Validated grid geometry for table construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GridPlacement {
    pub(crate) cols: usize,
    pub(crate) header_rows: usize,
    pub(crate) rows: usize,
    pub(crate) spans: Vec<CellSpan>,
}

/// Calculates grid placement, verifies spanning constraints, and returns each cell's [`CellSpan`].
pub(crate) fn place_table_grid(
    cols: usize,
    header: impl IntoIterator<Item = impl IntoIterator<Item = (NonZeroU16, NonZeroU16)>>,
    body: impl IntoIterator<Item = impl IntoIterator<Item = (NonZeroU16, NonZeroU16)>>,
) -> Result<GridPlacement> {
    if cols == 0 {
        return Err(Error::from(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::ColumnCount,
        }));
    }
    let header_rows_vec: Vec<Vec<(NonZeroU16, NonZeroU16)>> = header
        .into_iter()
        .map(|row| row.into_iter().collect())
        .collect();
    let body_rows_vec: Vec<Vec<(NonZeroU16, NonZeroU16)>> = body
        .into_iter()
        .map(|row| row.into_iter().collect())
        .collect();

    let header_rows = header_rows_vec.len();
    let rows = header_rows + body_rows_vec.len();
    let mut taken = vec![vec![false; cols]; rows];
    let mut spans = Vec::new();

    for (row, row_cells) in header_rows_vec.into_iter().chain(body_rows_vec).enumerate() {
        let row_limit = if row < header_rows { header_rows } else { rows };
        let mut col = 0;
        for (col_span_nz, row_span_nz) in row_cells {
            while col < cols
                && taken
                    .get(row)
                    .and_then(|r| r.get(col))
                    .copied()
                    .unwrap_or(false)
            {
                col += 1;
            }
            let col_span = usize::from(col_span_nz.get());
            let row_span = usize::from(row_span_nz.get());
            if col + col_span > cols || row + row_span > row_limit {
                let reason = if col + col_span <= cols && row + row_span <= rows {
                    CellSpanReason::CrossesHeader
                } else {
                    CellSpanReason::OutOfGrid
                };
                return Err(Error::from(ErrorKind::InvalidCellSpan(reason)).in_cell(row, col));
            }
            if let Some(row_slice) = taken.get_mut(row..row + row_span) {
                for (r, slots) in row_slice.iter_mut().enumerate() {
                    if let Some(col_slice) = slots.get_mut(col..col + col_span) {
                        for (c, slot) in col_slice.iter_mut().enumerate() {
                            if *slot {
                                return Err(Error::from(ErrorKind::InvalidCellSpan(
                                    CellSpanReason::Overlap,
                                ))
                                .in_cell(row + r, col + c));
                            }
                            *slot = true;
                        }
                    }
                }
            }
            spans.push(CellSpan {
                row,
                col,
                cols: col_span,
                rows: row_span,
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

    Ok(GridPlacement {
        cols,
        header_rows,
        rows,
        spans,
    })
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
    rules: Rules,
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
struct RowGroup {
    rows: Range<usize>,
    height: Length<NonNegative>,
    commands: Vec<FragmentCommand>,
    anchors: Vec<frame::AnchorMark>,
    first_baseline: Option<Length<Finite>>,
}

impl RowGroup {
    fn with_offset(&self, dx: Length<Finite>, dy: Length<Finite>) -> Result<Self> {
        let mut cloned = self.clone();
        let offset = Offset::new(dx, dy);
        for command in &mut cloned.commands {
            frame::translate_command(command, offset)?;
        }
        for mark in &mut cloned.anchors {
            mark.relative = Point::try_pt(
                mark.relative.x().as_pt() + dx.as_pt(),
                mark.relative.y().as_pt() + dy.as_pt(),
            )?;
        }
        cloned.first_baseline = cloned
            .first_baseline
            .map(|baseline| Finite::pt(baseline.as_pt() + dy.as_pt()))
            .transpose()?;
        Ok(cloned)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredTable {
    width: Length<Positive>,
    header: Option<RowGroup>,
    body: VecDeque<RowGroup>,
    repeat_header: bool,
    draw_header_next: bool,
    col_x: Vec<f32>,
    row_y: Vec<f32>,
    cols: usize,
    cell_spans: Vec<Span>,
    rules: Rules,
}

impl MeasuredTable {
    pub(crate) fn opening_height(&self) -> Length<NonNegative> {
        let first = self.body.front().map_or(NonNegative::ZERO, |b| b.height);
        let header_h = self.header.as_ref().map_or(NonNegative::ZERO, |h| h.height);
        self.rules
            .opening_clearance(header_h, first, self.header.is_some())
    }
}

impl<N: Flowable> TableNode<N> {
    #[cfg_attr(not(test), expect(dead_code, reason = "used in layout unit tests"))]
    pub(crate) fn new(spec: TableSpec<N>) -> Result<Self> {
        let cols = spec.widths.len();
        let placement = place_table_grid(
            cols,
            spec.header
                .iter()
                .map(|row| row.iter().map(|c| (c.col_span, c.row_span))),
            spec.body
                .iter()
                .map(|row| row.iter().map(|c| (c.col_span, c.row_span))),
        )?;
        Ok(Self::with_placement(spec, placement))
    }

    pub(crate) fn with_placement(spec: TableSpec<N>, placement: GridPlacement) -> Self {
        let mut spans = placement.spans.into_iter();
        let mut cells = Vec::new();
        for cell in spec.header.into_iter().chain(spec.body).flatten() {
            let span = spans.next().unwrap_or(CellSpan {
                row: 0,
                col: 0,
                cols: 1,
                rows: 1,
            });
            cells.push(PlacedCell {
                span: Span {
                    row: span.row,
                    col: span.col,
                    cols: span.cols,
                    rows: span.rows,
                },
                body: cell.body,
                fill: cell.fill,
            });
        }
        Self(TableState::Open(GridTable {
            widths: spec.widths,
            padding: spec.padding,
            rules: spec.rules,
            header_rows: placement.header_rows,
            rows: placement.rows,
            cells,
            repeat_header: spec.repeat_header,
        }))
    }

    pub(crate) fn opening_height(
        &mut self,
        ctx: &mut LayoutCtx<'_>,
        avail: Frame,
    ) -> Result<Length<NonNegative>> {
        match &mut self.0 {
            TableState::Open(grid) => {
                let taken = std::mem::replace(
                    grid,
                    GridTable {
                        widths: Vec::new(),
                        padding: NonNegative::ZERO,
                        rules: Rules::None,
                        header_rows: 0,
                        rows: 0,
                        cells: Vec::new(),
                        repeat_header: false,
                    },
                );
                let measured = measure(taken, ctx, avail)?;
                let height = measured.opening_height();
                self.0 = TableState::Measured(measured);
                Ok(height)
            }
            TableState::Measured(table) => Ok(table.opening_height()),
        }
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

fn validate_table_groups_fit(
    body_groups: &VecDeque<RowGroup>,
    header: Option<&RowGroup>,
    repeat_header: bool,
    rules: Rules,
    full_height: Length<Positive>,
) -> Result<()> {
    let header_h = header.map_or(NonNegative::ZERO, |hdr| hdr.height);
    let has_header = header.is_some();
    for (index, block) in body_groups.iter().enumerate() {
        let with_header = index == 0 || repeat_header;
        let need = rules.row_group_required_height(block.height, header_h, with_header);
        if need.as_pt() > full_height.as_pt() {
            return Err(ErrorKind::UnsplittableTallerThanFrame {
                height: need.as_pt(),
                avail: full_height.as_pt(),
            }
            .into());
        }
    }
    let header_need = rules.header_clearance(header_h, has_header);
    if header_need.as_pt() > full_height.as_pt() {
        return Err(ErrorKind::UnsplittableTallerThanFrame {
            height: header_need.as_pt(),
            avail: full_height.as_pt(),
        }
        .into());
    }
    Ok(())
}

fn measure<N: Flowable>(
    grid: GridTable<N>,
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
) -> Result<MeasuredTable> {
    let Some(avail_w) = grid.rules.inner_content_width(avail.width) else {
        return Err(Error::from(ErrorKind::ColumnWidths {
            reason: ColumnWidthsReason::ContentWidth,
        }));
    };
    let half_w = grid.rules.border_inset().as_pt();
    let inset = half_w + grid.padding.as_pt();
    let widths = column_width::resolve_column_widths(&grid.widths, avail_w)?;
    let xs = column_edges(&widths);

    let inner_full_pt = avail.full_height.as_pt() - 2.0 * inset;
    if inner_full_pt <= 0.0 {
        return Err(ErrorKind::UnsplittableTallerThanFrame {
            height: 2.0 * inset,
            avail: avail.full_height.as_pt(),
        }
        .into());
    }
    let inner_full = Length::<Positive>::from_trusted_pt(inner_full_pt);

    let (laid, row_h) = lay_cells(grid.cells, grid.rows, &xs, inset, inner_full, ctx)?;
    let cell_spans: Vec<Span> = laid.iter().map(|c| c.span).collect();
    let col_x: Vec<f32> = xs.iter().map(|value| value.as_pt() + half_w).collect();
    let row_y = row_offsets(row_h, &laid);
    let groups = row_groups(&laid, grid.header_rows, grid.rows);

    let groups_measured = GridPaint {
        col_x: &col_x,
        row_y: &row_y,
        groups: &groups,
        content_inset: inset,
        half_w,
    }
    .paint(laid)?;

    let mut body_groups = VecDeque::from(groups_measured);
    let header = if grid.header_rows > 0 {
        body_groups.pop_front()
    } else {
        None
    };
    validate_table_groups_fit(
        &body_groups,
        header.as_ref(),
        grid.repeat_header,
        grid.rules,
        avail.full_height,
    )?;
    Ok(MeasuredTable {
        width: avail.width,
        header,
        body: body_groups,
        repeat_header: grid.repeat_header,
        draw_header_next: true,
        col_x,
        row_y,
        cols: grid.widths.len(),
        cell_spans,
        rules: grid.rules,
    })
}

fn lay_cells<N: Flowable>(
    cells: Vec<PlacedCell<N>>,
    rows: usize,
    xs: &[Length<NonNegative>],
    inset: f32,
    inner_full: Length<Positive>,
    ctx: &mut LayoutCtx<'_>,
) -> Result<(Vec<LaidCell>, Vec<Length<NonNegative>>)> {
    let mut laid = Vec::with_capacity(cells.len());
    let mut row_h = vec![NonNegative::ZERO; rows];
    for PlacedCell {
        span: cell,
        body,
        fill,
    } in cells
    {
        let x_start = xs.get(cell.col).copied().unwrap_or(NonNegative::ZERO);
        let x_end = xs.get(cell.col + cell.cols).copied().unwrap_or(x_start);
        let inner_w = x_end.as_pt() - x_start.as_pt() - 2.0 * inset;
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
        let (cell_content_h, content) = ctx.located(
            &crate::error::Location::at_cell(cell.row, cell.col),
            |ctx| frame::natural_height(body.into_node(), ctx, frame),
        )?;
        let cell_h = Length::<NonNegative>::from_trusted_pt(cell_content_h.as_pt() + 2.0 * inset);
        if cell.rows == 1 {
            if let Some(entry) = row_h.get_mut(cell.row) {
                *entry = (*entry).max(cell_h);
            }
        }
        laid.push(LaidCell {
            span: cell,
            height: cell_h,
            content,
            fill,
        });
    }
    Ok((laid, row_h))
}

/// Shared grid geometry used while turning measured cells into draw commands.
struct GridPaint<'a> {
    col_x: &'a [f32],
    row_y: &'a [f32],
    groups: &'a [Range<usize>],
    content_inset: f32,
    half_w: f32,
}

impl GridPaint<'_> {
    fn paint(self, laid: Vec<LaidCell>) -> Result<Vec<RowGroup>> {
        let half_w = self.half_w;
        let mut groups: Vec<RowGroup> = self
            .groups
            .iter()
            .map(|rows| {
                let y_start = self.row_y.get(rows.start).copied().unwrap_or(0.0);
                let y_end = self.row_y.get(rows.end).copied().unwrap_or(y_start);
                let height_pt = y_end - y_start;
                RowGroup {
                    rows: rows.clone(),
                    height: Length::from_trusted_pt(height_pt),
                    commands: Vec::new(),
                    anchors: Vec::new(),
                    first_baseline: None,
                }
            })
            .collect();
        for LaidCell {
            span: cell,
            content,
            fill,
            ..
        } in laid
        {
            #[expect(
                clippy::expect_used,
                reason = "row_groups covers every row index before paint"
            )]
            let group_idx = self
                .groups
                .iter()
                .position(|rows| rows.contains(&cell.row))
                .expect("every row belongs to a group");
            let group_start_row = self.groups.get(group_idx).map_or(0, |r| r.start);
            let group_y0 = self.row_y.get(group_start_row).copied().unwrap_or(0.0);

            let x = self.col_x.get(cell.col).copied().unwrap_or(0.0);
            let x_end = self.col_x.get(cell.col + cell.cols).copied().unwrap_or(x);
            let y_cell = self.row_y.get(cell.row).copied().unwrap_or(0.0);
            let y_cell_end = self
                .row_y
                .get(cell.row + cell.rows)
                .copied()
                .unwrap_or(y_cell);

            let y = y_cell - group_y0 + half_w;
            let w = x_end - x;
            let h = y_cell_end - y_cell;

            let Some(group) = groups.get_mut(group_idx) else {
                continue;
            };
            if let Some(color) = fill {
                group.commands.push(FragmentCommand::Fill {
                    path: rect_path(x, y, w, h)?,
                    paint: Paint::solid(color),
                });
            }
            let offset = content.with_offset(
                Length::from_trusted_pt(x + self.content_inset),
                Length::from_trusted_pt(y + self.content_inset),
            )?;
            if cell.row == group_start_row {
                group.first_baseline =
                    frame::earlier_baseline(group.first_baseline, offset.first_baseline);
            }
            group.commands.extend(offset.commands);
            group.anchors.extend(offset.anchors);
        }

        Ok(groups)
    }
}

fn row_offsets(mut row_h: Vec<Length<NonNegative>>, laid: &[LaidCell]) -> Vec<f32> {
    let mut spanning: Vec<_> = laid.iter().filter(|cell| cell.span.rows > 1).collect();
    spanning.sort_by_key(|cell| cell.span.rows);
    for cell in spanning {
        let rows = cell.span.row..cell.span.row + cell.span.rows;
        let have: f32 = row_h
            .get(rows.clone())
            .unwrap_or(&[])
            .iter()
            .map(|height| height.as_pt())
            .sum();
        if cell.height.as_pt() > have {
            if let Some(last_row_idx) = rows.end.checked_sub(1) {
                if let Some(last_h) = row_h.get_mut(last_row_idx) {
                    let grown = last_h.as_pt() + cell.height.as_pt() - have;
                    *last_h = Length::from_trusted_pt(grown);
                }
            }
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
        if let Some(entry) = reach.get_mut(span.row) {
            *entry = (*entry).max(span.row + span.rows - 1);
        }
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
            if let Some(&r) = reach.get(row) {
                end = end.max(r);
            }
            row += 1;
        }
        groups.push(start..end + 1);
        start = end + 1;
    }
    groups
}

struct PlacedGroupInfo<'a> {
    rows: &'a Range<usize>,
    page_y0: f32,
}

struct PlacedRow {
    row_idx: usize,
    y_top: f32,
    y_bottom: f32,
}

fn split_measured<N>(table: MeasuredTable, avail: Frame) -> Result<Split<TableNode<N>>> {
    if table.width.as_pt() > avail.width.as_pt() {
        return Err(GeometryKind::LaidOutWiderThanFrame {
            width: table.width.as_pt(),
            avail: avail.width.as_pt(),
        }
        .into());
    }
    match table.rules {
        Rules::Grid(pen) => split_measured_grid(table, pen, avail),
        Rules::ThreeLine {
            top,
            header,
            bottom,
        } => split_measured_three_line_or_none(table, Some((top, header, bottom)), avail),
        Rules::None => split_measured_three_line_or_none(table, None, avail),
    }
}

fn append_three_line_strokes(
    commands: &mut Vec<FragmentCommand>,
    rules: (Pen, Pen, Pen),
    width: Length<Positive>,
    placed_header: bool,
    header_height: f32,
    y_body_end: f32,
) -> Result<()> {
    let (top, header_pen, bottom) = rules;
    let x0 = 0.0;
    let x1 = width.as_pt();
    let w_top = top.width().as_pt();
    let w_hdr = header_pen.width().as_pt();
    let w_bot = bottom.width().as_pt();

    // 1. Top line
    let y_top = w_top / 2.0;
    commands.push(FragmentCommand::Stroke {
        path: Path::begin(Point::try_pt(x0, y_top)?)
            .line_to(Point::try_pt(x1, y_top)?)
            .finish(),
        paint: Paint::solid(top.color()),
        stroke: Stroke::new(top.width()),
    });

    // 2. Header line
    if placed_header {
        let y_hdr = w_top + header_height + w_hdr / 2.0;
        commands.push(FragmentCommand::Stroke {
            path: Path::begin(Point::try_pt(x0, y_hdr)?)
                .line_to(Point::try_pt(x1, y_hdr)?)
                .finish(),
            paint: Paint::solid(header_pen.color()),
            stroke: Stroke::new(header_pen.width()),
        });
    }

    // 3. Bottom line
    let y_bot_center = y_body_end + w_bot / 2.0;
    commands.push(FragmentCommand::Stroke {
        path: Path::begin(Point::try_pt(x0, y_bot_center)?)
            .line_to(Point::try_pt(x1, y_bot_center)?)
            .finish(),
        paint: Paint::solid(bottom.color()),
        stroke: Stroke::new(bottom.width()),
    });
    Ok(())
}

fn split_measured_three_line_or_none<N>(
    mut table: MeasuredTable,
    rules: Option<(Pen, Pen, Pen)>,
    avail: Frame,
) -> Result<Split<TableNode<N>>> {
    let w_top = rules.map_or(0.0, |(top, _, _)| top.width().as_pt());
    let w_hdr = rules.map_or(0.0, |(_, hdr, _)| hdr.width().as_pt());
    let w_bot = rules.map_or(0.0, |(_, _, bot)| bot.width().as_pt());

    let mut y = w_top;
    let mut commands = Vec::new();
    let mut anchors = Vec::new();
    let mut first_baseline = None;
    let mut baseline_chosen = false;
    let mut placed_header = false;

    let has_header = table.draw_header_next && table.header.is_some();
    let first = table
        .body
        .front()
        .map_or(NonNegative::ZERO, |block| block.height);
    let header_h = table
        .header
        .as_ref()
        .map_or(NonNegative::ZERO, |header| header.height);
    if table
        .rules
        .opening_clearance(header_h, first, has_header)
        .as_pt()
        > avail.height.as_pt()
    {
        return Ok(Split::MoveToNextPage(TableNode(TableState::Measured(
            table,
        ))));
    }
    if has_header {
        if let Some(header) = &table.header {
            let offset = header.with_offset(Finite::ZERO, Finite::pt(y)?)?;
            commands.extend(offset.commands);
            anchors.extend(offset.anchors);
            first_baseline = offset.first_baseline;
            baseline_chosen = true;
            y += header.height.as_pt() + w_hdr;
            placed_header = true;
        }
    }

    let mut placed_body = Vec::new();
    while let Some(block) = table.body.pop_front() {
        let total_needed = y + block.height.as_pt() + w_bot;
        if total_needed > avail.height.as_pt() {
            table.body.push_front(block);
            break;
        }
        let h = block.height.as_pt();
        let offset = block.with_offset(Finite::ZERO, Finite::pt(y)?)?;
        if !baseline_chosen {
            first_baseline = offset.first_baseline;
            baseline_chosen = true;
        }
        commands.extend(offset.commands);
        anchors.extend(offset.anchors);
        y += h;
        placed_body.push(block);
    }
    if placed_body.is_empty() {
        return Ok(Split::MoveToNextPage(TableNode(TableState::Measured(
            table,
        ))));
    }

    if let Some(three_pens) = rules {
        let hdr_h = table.header.as_ref().map_or(0.0, |h| h.height.as_pt());
        append_three_line_strokes(
            &mut commands,
            three_pens,
            avail.width,
            placed_header,
            hdr_h,
            y,
        )?;
    }

    let total_head_height = Length::from_trusted_pt(y + w_bot);
    let head = Fragment {
        height: total_head_height,
        spacing_after: NonNegative::ZERO,
        commands,
        anchors,
        first_baseline,
    };
    if table.body.is_empty() {
        return Ok(Split::Fit {
            height: total_head_height,
            fragment: head,
        });
    }
    table.draw_header_next = table.repeat_header && table.header.is_some();
    Ok(Split::Partial {
        head_height: total_head_height,
        head,
        tail: TableNode(TableState::Measured(table)),
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "table grid border line segment layout algorithm"
)]
fn split_measured_grid<N>(
    mut table: MeasuredTable,
    pen: Pen,
    avail: Frame,
) -> Result<Split<TableNode<N>>> {
    let border_width = pen.width();
    let stroke = Stroke::new(pen.width());
    let paint = Paint::solid(pen.color());
    let mut y = NonNegative::ZERO;
    let mut commands = Vec::new();
    let mut anchors = Vec::new();
    let mut first_baseline = None;
    let mut baseline_chosen = false;
    let mut placed_header = false;
    if table.draw_header_next {
        if let Some(header) = &table.header {
            let first = table
                .body
                .front()
                .map_or(NonNegative::ZERO, |block| block.height);
            let needed = table.rules.opening_clearance(header.height, first, true);
            if needed.as_pt() > avail.height.as_pt() {
                return Ok(Split::MoveToNextPage(TableNode(TableState::Measured(
                    table,
                ))));
            }
            let offset = header.with_offset(Finite::ZERO, Finite::ZERO)?;
            commands.extend(offset.commands);
            anchors.extend(offset.anchors);
            first_baseline = offset.first_baseline;
            baseline_chosen = true;
            y = header.height;
            placed_header = true;
        }
    }
    let mut placed_body = Vec::new();
    while let Some(block) = table.body.pop_front() {
        let total_needed = y + block.height + border_width;
        if total_needed.as_pt() > avail.height.as_pt() {
            table.body.push_front(block);
            break;
        }
        let h = block.height;
        let offset = block.with_offset(Finite::ZERO, y.into())?;
        if !baseline_chosen {
            first_baseline = offset.first_baseline;
            baseline_chosen = true;
        }
        commands.extend(offset.commands);
        anchors.extend(offset.anchors);
        y += h;
        placed_body.push(block);
    }
    if placed_body.is_empty() {
        return Ok(Split::MoveToNextPage(TableNode(TableState::Measured(
            table,
        ))));
    }

    let half_w = table.rules.border_inset().as_pt();
    let rows_total_height = y.as_pt();

    let mut groups_info = Vec::new();
    if placed_header {
        if let Some(header) = &table.header {
            groups_info.push(PlacedGroupInfo {
                rows: &header.rows,
                page_y0: 0.0,
            });
        }
    }
    let mut cur_y = if placed_header {
        table.header.as_ref().map_or(0.0, |h| h.height.as_pt())
    } else {
        0.0
    };
    for block in &placed_body {
        groups_info.push(PlacedGroupInfo {
            rows: &block.rows,
            page_y0: cur_y,
        });
        cur_y += block.height.as_pt();
    }

    let mut placed_rows = Vec::new();
    for g in &groups_info {
        let y0_group = table.row_y.get(g.rows.start).copied().unwrap_or(0.0);
        for r in g.rows.clone() {
            let yr = table.row_y.get(r).copied().unwrap_or(y0_group);
            let yr_next = table.row_y.get(r + 1).copied().unwrap_or(yr);
            let y_top = g.page_y0 + (yr - y0_group) + half_w;
            let y_bottom = g.page_y0 + (yr_next - y0_group) + half_w;
            placed_rows.push(PlacedRow {
                row_idx: r,
                y_top,
                y_bottom,
            });
        }
    }

    let mut segments: Vec<(Point, Point)> = Vec::new();
    let x0 = table.col_x.first().copied().unwrap_or(0.0);
    let x_end = table.col_x.get(table.cols).copied().unwrap_or(x0);

    // 1. Horizontal lines
    // 1a. Top line at y = half_w across columns 0..cols
    segments.push((Point::try_pt(x0, half_w)?, Point::try_pt(x_end, half_w)?));

    // 1b. Internal row boundaries between placed rows on this page
    if placed_rows.len() > 1 {
        for i in 0..placed_rows.len() - 1 {
            let Some(row_above) = placed_rows.get(i) else {
                continue;
            };
            let Some(row_below) = placed_rows.get(i + 1) else {
                continue;
            };
            let r_above = row_above.row_idx;
            let r_below = row_below.row_idx;
            let y_b = row_above.y_bottom;

            let mut c = 0;
            while c < table.cols {
                let is_drawn = !table.cell_spans.iter().any(|span| {
                    span.row <= r_above
                        && r_below < span.row + span.rows
                        && span.col <= c
                        && c < span.col + span.cols
                });
                if is_drawn {
                    let c_start = c;
                    while c < table.cols {
                        let next_drawn = !table.cell_spans.iter().any(|span| {
                            span.row <= r_above
                                && r_below < span.row + span.rows
                                && span.col <= c
                                && c < span.col + span.cols
                        });
                        if !next_drawn {
                            break;
                        }
                        c += 1;
                    }
                    let sx0 = table.col_x.get(c_start).copied().unwrap_or(0.0);
                    let sx1 = table.col_x.get(c).copied().unwrap_or(sx0);
                    segments.push((Point::try_pt(sx0, y_b)?, Point::try_pt(sx1, y_b)?));
                } else {
                    c += 1;
                }
            }
        }
    }

    // 1c. Bottom line at y = rows_total_height + half_w across columns 0..cols
    let y_bot = rows_total_height + half_w;
    segments.push((Point::try_pt(x0, y_bot)?, Point::try_pt(x_end, y_bot)?));

    // 2. Vertical lines
    for c in 0..=table.cols {
        let x = table.col_x.get(c).copied().unwrap_or(0.0);
        let mut i = 0;
        while i < placed_rows.len() {
            let Some(curr_row) = placed_rows.get(i) else {
                break;
            };
            let is_drawn = if c == 0 || c == table.cols {
                true
            } else {
                let r = curr_row.row_idx;
                !table.cell_spans.iter().any(|span| {
                    span.col < c
                        && span.col + span.cols > c
                        && span.row <= r
                        && r < span.row + span.rows
                })
            };
            if is_drawn {
                let y_start = curr_row.y_top;
                let mut y_finish = curr_row.y_bottom;
                i += 1;
                while i < placed_rows.len() {
                    let Some(next_row) = placed_rows.get(i) else {
                        break;
                    };
                    let next_drawn = if c == 0 || c == table.cols {
                        true
                    } else {
                        let r = next_row.row_idx;
                        !table.cell_spans.iter().any(|span| {
                            span.col < c
                                && span.col + span.cols > c
                                && span.row <= r
                                && r < span.row + span.rows
                        })
                    };
                    if !next_drawn || (next_row.y_top - y_finish).abs() > 0.01 {
                        break;
                    }
                    y_finish = next_row.y_bottom;
                    i += 1;
                }
                segments.push((Point::try_pt(x, y_start)?, Point::try_pt(x, y_finish)?));
            } else {
                i += 1;
            }
        }
    }

    if !segments.is_empty() {
        let mut builder: Option<PathBuilder> = None;
        for (p1, p2) in segments {
            match builder {
                Some(b) => builder = Some(b.move_to(p1).line_to(p2)),
                None => builder = Some(Path::begin(p1).line_to(p2)),
            }
        }
        if let Some(path) = builder.map(PathBuilder::finish) {
            commands.push(FragmentCommand::Stroke {
                path,
                paint,
                stroke,
            });
        }
    }

    let total_head_height = y + border_width;
    let head = Fragment {
        height: total_head_height,
        spacing_after: NonNegative::ZERO,
        commands,
        anchors,
        first_baseline,
    };
    if table.body.is_empty() {
        return Ok(Split::Fit {
            height: total_head_height,
            fragment: head,
        });
    }
    table.draw_header_next = table.repeat_header && table.header.is_some();
    Ok(Split::Partial {
        head_height: total_head_height,
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
