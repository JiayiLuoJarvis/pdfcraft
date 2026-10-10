//! Table column sizing, row placement, optional repeating headers.

#![cfg_attr(not(test), allow(dead_code))]

use std::collections::VecDeque;
use std::num::NonZeroU16;
use std::ops::Range;

use crate::error::{CellSpanReason, ColumnWidthsReason, Error, ErrorKind, GeometryKind, Result};
use crate::geom::{Color, Length, NonNegative, Paint, Path, Point, Positive, Stroke};
use crate::layout::columns::{resolve_column_widths, ColumnWidth};
use crate::layout::frame::{
    natural_height, Flowable, Fragment, FragmentCommand, Frame, LayoutCtx, Split,
};
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
    pub(crate) fn new(content: Vec<N>) -> Result<Self> {
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
    padding: f32,
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
    height: f32,
    content: Fragment,
    fill: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredTable {
    width: f32,
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
            padding: spec.padding.as_pt(),
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
}

fn measure<N: Flowable>(
    grid: GridTable<N>,
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
) -> Result<MeasuredTable> {
    let widths = resolve_column_widths(&grid.widths, avail.width)?;
    let mut xs = Vec::with_capacity(widths.len() + 1);
    let mut x = 0.0f32;
    for w in &widths {
        xs.push(x);
        x += w;
    }
    xs.push(x);

    let border = grid.border.width.as_pt();
    let inset = border / 2.0 + grid.padding;
    let inner_full = avail.full_height - 2.0 * inset;
    if inner_full <= 0.0 {
        return Err(ErrorKind::UnsplittableTallerThanFrame {
            height: 2.0 * inset,
            avail: avail.full_height,
        }
        .into());
    }

    let mut laid = Vec::with_capacity(grid.cells.len());
    let mut row_h = vec![0.0f32; grid.rows];
    for PlacedCell {
        span: cell,
        body,
        fill,
    } in grid.cells
    {
        let inner_w = xs[cell.col + cell.cols] - xs[cell.col] - 2.0 * inset;
        if inner_w <= 0.0 {
            return Err(Error::from(ErrorKind::ColumnWidths {
                reason: ColumnWidthsReason::ContentWidth,
            })
            .in_cell(cell.row, cell.col));
        }
        let frame = Frame {
            width: inner_w,
            height: inner_full,
            full_height: inner_full,
        };
        let (h, content) = natural_height(body.into_node(), ctx, frame)
            .map_err(|error| error.in_cell(cell.row, cell.col))?;
        let cell_h = h + 2.0 * inset;
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
    let row_y = row_offsets(row_h, &laid);
    let groups = row_groups(&laid, grid.header_rows, grid.rows);
    let fragments = GridPaint {
        col_x: &xs,
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
    let header_h = header.as_ref().map_or(0.0, |h| h.height);
    for (index, block) in fragments.iter().enumerate() {
        let with_header = index == 0 || grid.repeat_header;
        let need = block.height + if with_header { header_h } else { 0.0 };
        if need > avail.full_height {
            return Err(ErrorKind::UnsplittableTallerThanFrame {
                height: need,
                avail: avail.full_height,
            }
            .into());
        }
    }
    if header_h > avail.full_height {
        return Err(ErrorKind::UnsplittableTallerThanFrame {
            height: header_h,
            avail: avail.full_height,
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
                height: self.row_y[rows.end] - self.row_y[rows.start],
                spacing_after: 0.0,
                commands: Vec::new(),
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
            let commands = &mut fragments[group].commands;
            if let Some(color) = fill {
                commands.push(FragmentCommand::Fill {
                    path: rect_path(x, y, w, h)?,
                    paint: Paint::solid(color),
                });
            }
            commands.extend(
                content
                    .with_offset(x + self.content_inset, y + self.content_inset)?
                    .commands,
            );
            if let Some((stroke, paint)) = self.border {
                commands.push(FragmentCommand::Stroke {
                    path: rect_path(x, y, w, h)?,
                    paint,
                    stroke,
                });
            }
        }
        Ok(fragments)
    }
}

fn row_offsets(mut row_h: Vec<f32>, laid: &[LaidCell]) -> Vec<f32> {
    let mut spanning: Vec<_> = laid.iter().filter(|cell| cell.span.rows > 1).collect();
    spanning.sort_by_key(|cell| cell.span.rows);
    for cell in spanning {
        let rows = cell.span.row..cell.span.row + cell.span.rows;
        let have: f32 = row_h[rows.clone()].iter().sum();
        if cell.height > have {
            row_h[rows.end - 1] += cell.height - have;
        }
    }
    let mut row_y = Vec::with_capacity(row_h.len() + 1);
    let mut y = 0.0f32;
    for h in &row_h {
        row_y.push(y);
        y += h;
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
    if table.width > avail.width {
        return Err(GeometryKind::LaidOutWiderThanFrame {
            width: table.width,
            avail: avail.width,
        }
        .into());
    }
    let mut y = 0.0f32;
    let mut commands = Vec::new();
    if table.draw_header_next {
        if let Some(header) = &table.header {
            let first = table.body.front().map_or(0.0, |b| b.height);
            if header.height + first > avail.height {
                return Ok(Split::MoveToNextPage(TableNode(TableState::Measured(
                    table,
                ))));
            }
            commands.extend(header.commands.iter().cloned());
            y = header.height;
        }
    }
    let mut placed = 0usize;
    while let Some(block) = table.body.front() {
        if y + block.height > avail.height {
            break;
        }
        let block = table.body.pop_front().expect("front exists");
        let h = block.height;
        commands.extend(block.with_offset(0.0, y)?.commands);
        y += h;
        placed += 1;
    }
    let head = Fragment {
        height: y,
        spacing_after: 0.0,
        commands,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Margins, Size};
    use crate::layout::columns::{default_column_gap, ColumnsNode, Ratio};
    use crate::layout::decorate::Decorations;
    use crate::layout::flow::{FlowNode, KeepTogetherNode};
    use crate::layout::paginate::PaginateConfig;
    use crate::layout::style::Style;
    use crate::layout::test_util::{
        bits, fills, fonts_and_style, metrics, page, pt, render, runs, strokes, text, texts,
    };

    fn cell(s: &str, style: &Style) -> Cell<FlowNode> {
        Cell::new(vec![text(s, style)]).unwrap()
    }

    fn filled(s: &str, style: &Style, color: Color) -> Cell<FlowNode> {
        cell(s, style).with_fill(Some(color))
    }

    fn span(cols: u16, rows: u16) -> (NonZeroU16, NonZeroU16) {
        (
            NonZeroU16::new(cols).unwrap(),
            NonZeroU16::new(rows).unwrap(),
        )
    }

    fn halves() -> Vec<ColumnWidth> {
        vec![
            ColumnWidth::Ratio(Ratio::new(1.0).unwrap()),
            ColumnWidth::Ratio(Ratio::new(1.0).unwrap()),
        ]
    }

    fn numbered_rows(style: &Style, n: usize) -> Vec<Vec<Cell<FlowNode>>> {
        (0..n)
            .map(|i| vec![cell(&format!("r{i}"), style), cell("1", style)])
            .collect()
    }

    #[test]
    fn repeats_all_header_rows_on_continuation_page() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let mut spec = TableSpec::new(halves());
        let (c2, r1) = span(2, 1);
        spec.header = vec![
            vec![cell("H1", &style).with_span(c2, r1)],
            vec![cell("a", &style), cell("b", &style)],
        ];
        spec.body = numbered_rows(&style, 4);
        let table = TableNode::new(spec).unwrap();
        let config = page(200.0, m.line_height * 4.0 + 1.0);
        let (result, rec) = render(
            FlowNode::Table(table),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        result.unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(texts(&pages[0]), ["H1", "a", "b", "r0", "1", "r1", "1"]);
        assert_eq!(texts(&pages[1]), ["H1", "a", "b", "r2", "1", "r3", "1"]);
        let second = runs(&pages[1]);
        assert_eq!(
            bits(&second[0]),
            ("H1", 0.0f32.to_bits(), m.baseline.to_bits())
        );
        assert_eq!(
            second[3].2.to_bits(),
            (m.line_height * 2.0 + m.baseline).to_bits()
        );
    }

    #[test]
    fn repeat_header_off_continues_body_only() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let mut spec = TableSpec::new(halves());
        spec.header = vec![vec![cell("h", &style), cell("k", &style)]];
        spec.body = numbered_rows(&style, 3);
        spec.repeat_header = false;
        let table = TableNode::new(spec).unwrap();
        let config = page(200.0, m.line_height * 2.0 + 1.0);
        let (result, rec) = render(
            FlowNode::Table(table),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        result.unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(texts(&pages[0]), ["h", "k", "r0", "1"]);
        assert_eq!(texts(&pages[1]), ["r1", "1", "r2", "1"]);
    }

    #[test]
    fn row_span_group_moves_to_next_page_whole() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let (c1, r2) = span(1, 2);
        let mut spec = TableSpec::new(halves());
        spec.body = vec![
            vec![cell("r0", &style), cell("x", &style)],
            vec![cell("S", &style).with_span(c1, r2), cell("c", &style)],
            vec![cell("d", &style)],
        ];
        let table = TableNode::new(spec).unwrap();
        let config = page(200.0, m.line_height * 2.0 + 1.0);
        let (result, rec) = render(
            FlowNode::Table(table),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        result.unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(texts(&pages[0]), ["r0", "x"]);
        assert_eq!(
            runs(&pages[1]).iter().map(bits).collect::<Vec<_>>(),
            [
                ("S", 0.0f32.to_bits(), m.baseline.to_bits()),
                ("c", 100.0f32.to_bits(), m.baseline.to_bits()),
                (
                    "d",
                    100.0f32.to_bits(),
                    (m.line_height + m.baseline).to_bits()
                ),
            ]
        );
    }

    #[test]
    fn header_plus_group_taller_than_page_errors_with_no_output() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let mut spec = TableSpec::new(halves());
        spec.header = vec![vec![cell("h", &style), cell("k", &style)]];
        spec.body = vec![vec![cell("A\nB", &style), cell("1", &style)]];
        let table = TableNode::new(spec).unwrap();
        let config = page(200.0, m.line_height * 2.0 + 1.0);
        let (result, rec) = render(
            FlowNode::Table(table),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        let ErrorKind::UnsplittableTallerThanFrame { height, avail } = result.unwrap_err().kind
        else {
            panic!("expected UnsplittableTallerThanFrame");
        };
        assert!((height - m.line_height * 3.0).abs() < 1e-3, "{height}");
        assert_eq!(avail.to_bits(), (m.line_height * 2.0 + 1.0).to_bits());
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn borders_stroke_each_cell_centered_on_grid_lines() {
        let (mut fonts, style) = fonts_and_style();
        let m = metrics(&fonts, &style);
        let mut spec = TableSpec::new(vec![
            ColumnWidth::Fixed(Positive::pt(50.0).unwrap()),
            ColumnWidth::Auto,
        ]);
        spec.padding = pt(4.0);
        spec.border.width = pt(2.0);
        spec.body = vec![
            vec![cell("A", &style), cell("B", &style)],
            vec![cell("C", &style), cell("D", &style)],
        ];
        let table = TableNode::new(spec).unwrap();
        let config = PaginateConfig {
            page_size: Size::try_pt(220.0, 400.0).unwrap(),
            margins: Margins::new(pt(15.0), pt(10.0), pt(0.0), pt(10.0)),
        };
        let (result, rec) = render(
            FlowNode::Table(table),
            &mut fonts,
            config,
            &Decorations::none(),
        );
        result.unwrap();
        let page = &rec.pages()[0];
        let row = m.line_height + 10.0;
        let rect = |x0: f32, x1: f32, y0: f32, y1: f32| {
            vec![
                Some((x0, y0)),
                Some((x1, y0)),
                Some((x1, y1)),
                Some((x0, y1)),
                None,
            ]
        };
        assert_eq!(
            strokes(page),
            [
                rect(10.0, 60.0, 0.0 + 15.0, row + 15.0),
                rect(60.0, 210.0, 0.0 + 15.0, row + 15.0),
                rect(10.0, 60.0, row + 15.0, row + row + 15.0),
                rect(60.0, 210.0, row + 15.0, row + row + 15.0),
            ]
        );
        let baseline = (15.0 + 5.0 + m.baseline).to_bits();
        assert_eq!(
            runs(page).iter().take(2).map(bits).collect::<Vec<_>>(),
            [
                ("A", 15.0f32.to_bits(), baseline),
                ("B", 65.0f32.to_bits(), baseline)
            ]
        );
    }

    #[test]
    fn zero_border_draws_no_strokes() {
        let (mut fonts, style) = fonts_and_style();
        let mut spec = TableSpec::new(halves());
        spec.body = vec![vec![cell("A", &style), cell("B", &style)]];
        let table = TableNode::new(spec).unwrap();
        let (result, rec) = render(
            FlowNode::Table(table),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        result.unwrap();
        assert!(strokes(&rec.pages()[0]).is_empty());
        assert_eq!(texts(&rec.pages()[0]), ["A", "B"]);
    }

    #[test]
    fn cell_rejects_columns_page_break_keep_together_and_tables() {
        let (_, style) = fonts_and_style();
        let columns = ColumnsNode::new(
            default_column_gap(),
            vec![
                (ColumnWidth::Auto, vec![text("A", &style)]),
                (ColumnWidth::Fixed(Positive::pt(10.0).unwrap()), Vec::new()),
            ],
        )
        .unwrap();
        let mut inner = TableSpec::new(vec![ColumnWidth::Auto]);
        inner.body = vec![vec![cell("A", &style)]];
        let rejected = [
            (FlowNode::Columns(columns), "columns"),
            (FlowNode::PageBreak, "page break"),
            (
                FlowNode::KeepTogether(KeepTogetherNode::Open(Vec::new())),
                "keep together",
            ),
            (FlowNode::Table(TableNode::new(inner).unwrap()), "table"),
            (FlowNode::Stack(vec![FlowNode::PageBreak]), "page break"),
        ];
        for (node, name) in rejected {
            assert_eq!(
                Cell::new(vec![node]).unwrap_err(),
                Error::from(ErrorKind::InvalidCellContent {
                    container: "table cell",
                    node: name
                })
            );
        }
    }

    #[test]
    fn cell_fill_paints_behind_content_and_border() {
        let (mut fonts, style) = fonts_and_style();
        let navy = Color::rgb(0x19, 0x2d, 0x74);
        let mut spec = TableSpec::new(vec![ColumnWidth::Fixed(Positive::pt(40.0).unwrap())]);
        spec.border = Border {
            width: NonNegative::pt(1.0).unwrap(),
            color: Color::rgb(0, 0, 0),
        };
        spec.header = vec![vec![filled("H", &style, navy)]];
        spec.body = vec![vec![cell("b", &style)]];
        let table = TableNode::new(spec).unwrap();
        let (result, rec) = render(
            FlowNode::Table(table),
            &mut fonts,
            page(40.0, 200.0),
            &Decorations::none(),
        );
        result.unwrap();
        let page = &rec.pages()[0];
        assert_eq!(fills(page).len(), 1);
        assert_eq!(strokes(page).len(), 2);
        assert_eq!(texts(page), ["H", "b"]);
        let ops = page.ops();
        let fill_at = ops
            .iter()
            .position(|op| matches!(op, crate::canvas::CanvasCommand::FillPath { .. }))
            .unwrap();
        let first_glyphs = ops
            .iter()
            .position(|op| matches!(op, crate::canvas::CanvasCommand::DrawGlyphRun { .. }))
            .unwrap();
        let first_stroke = ops
            .iter()
            .position(|op| matches!(op, crate::canvas::CanvasCommand::StrokePath { .. }))
            .unwrap();
        assert!(fill_at < first_glyphs);
        assert!(first_glyphs < first_stroke);
    }

    #[test]
    fn cell_allows_hline() {
        let line = crate::layout::hline::HLineNode::new(
            None,
            crate::layout::style::TextAlign::Center,
            Positive::pt(1.0).unwrap(),
            Color::rgb(0, 0, 0),
            NonNegative::ZERO,
            NonNegative::ZERO,
        );
        Cell::new(vec![FlowNode::HLine(line)]).unwrap();
    }

    #[test]
    fn cell_allows_spacer() {
        Cell::new(vec![FlowNode::Spacer(crate::layout::flow::SpacerNode {
            height: pt(8.0),
        })])
        .unwrap();
    }

    #[test]
    fn overlapping_spans_are_rejected() {
        let (_, style) = fonts_and_style();
        let (c1, r2) = span(1, 2);
        let mut spec = TableSpec::new(halves());
        spec.body = vec![
            vec![cell("S", &style).with_span(c1, r2), cell("b", &style)],
            vec![cell("c", &style), cell("d", &style)],
        ];
        assert_eq!(
            TableNode::new(spec).unwrap_err(),
            span_error(CellSpanReason::OutOfGrid, 1, 2)
        );

        let (c2, r1) = span(2, 1);
        let mut cross = TableSpec::new(halves());
        cross.body = vec![
            vec![cell("a", &style), cell("B", &style).with_span(c1, r2)],
            vec![cell("C", &style).with_span(c2, r1)],
        ];
        assert_eq!(
            TableNode::new(cross).unwrap_err(),
            span_error(CellSpanReason::Overlap, 1, 1)
        );
    }

    fn span_error(reason: CellSpanReason, row: usize, col: usize) -> Error {
        Error::from(ErrorKind::InvalidCellSpan(reason)).in_cell(row, col)
    }

    #[test]
    fn spans_past_the_grid_or_out_of_the_header_are_rejected() {
        let (_, style) = fonts_and_style();
        let (c3, r1) = span(3, 1);
        let mut wide = TableSpec::new(halves());
        wide.body = vec![vec![cell("W", &style).with_span(c3, r1)]];
        assert_eq!(
            TableNode::new(wide).unwrap_err(),
            span_error(CellSpanReason::OutOfGrid, 0, 0)
        );

        let (c1, r2) = span(1, 2);
        let mut leak = TableSpec::new(halves());
        leak.header = vec![vec![cell("H", &style).with_span(c1, r2), cell("k", &style)]];
        leak.body = vec![vec![cell("b", &style)]];
        assert_eq!(
            TableNode::new(leak).unwrap_err(),
            span_error(CellSpanReason::CrossesHeader, 0, 0)
        );

        let mut gap = TableSpec::new(halves());
        gap.body = vec![vec![cell("only", &style)]];
        assert_eq!(
            TableNode::new(gap).unwrap_err(),
            span_error(CellSpanReason::EmptySlot, 0, 1)
        );
    }
}
