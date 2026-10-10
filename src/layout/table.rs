//! Table column sizing, row placement, optional repeating headers.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::num::NonZeroU16;
use std::ops::Range;

use crate::error::{Error, Result};
use crate::geom::{Color, Length, NonNegative, Paint, Path, Point, Positive, Stroke};
use crate::layout::columns::{resolve_column_widths, ColumnWidth};
use crate::layout::flow::{
    natural_height, FlowNode, Fragment, FragmentCommand, Frame, LayoutCtx, Split,
};
use crate::layout::lower::CellBody;

/// Grid-line stroke centered on cell edges. Width zero draws nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Border {
    pub(crate) width: Length<NonNegative>,
    pub(crate) color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableSpec {
    pub(crate) widths: Vec<ColumnWidth>,
    pub(crate) header: Vec<Vec<Cell>>,
    pub(crate) body: Vec<Vec<Cell>>,
    pub(crate) repeat_header: bool,
    pub(crate) padding: Length<NonNegative>,
    pub(crate) border: Border,
}

impl TableSpec {
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
pub(crate) struct Cell {
    body: CellBody,
    col_span: NonZeroU16,
    row_span: NonZeroU16,
}

impl Cell {
    pub(crate) fn new(content: Vec<FlowNode>) -> Result<Self> {
        Ok(Self {
            body: CellBody::new(content)?,
            col_span: NonZeroU16::MIN,
            row_span: NonZeroU16::MIN,
        })
    }

    pub(crate) fn with_span(self, cols: NonZeroU16, rows: NonZeroU16) -> Self {
        Self {
            col_span: cols,
            row_span: rows,
            ..self
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableNode(TableState);

#[derive(Debug, Clone, PartialEq)]
enum TableState {
    Open(GridTable),
    Measured(MeasuredTable),
}

#[derive(Debug, Clone, PartialEq)]
struct GridTable {
    widths: Vec<ColumnWidth>,
    padding: f32,
    border: Border,
    header_rows: usize,
    rows: usize,
    cells: Vec<PlacedCell>,
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
struct PlacedCell {
    span: Span,
    body: CellBody,
}

struct LaidCell {
    span: Span,
    height: f32,
    content: Fragment,
}

#[derive(Debug, Clone, PartialEq)]
struct MeasuredTable {
    width: f32,
    header: Option<Fragment>,
    body: VecDeque<Fragment>,
    repeat_header: bool,
    draw_header_next: bool,
}

impl TableNode {
    pub(crate) fn new(spec: TableSpec) -> Result<Self> {
        let cols = spec.widths.len();
        if cols == 0 {
            return Err(Error::ColumnWidths {
                reason: "column-count",
            });
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
                    return Err(Error::InvalidCellSpan);
                }
                for slots in &mut taken[row..row + row_span] {
                    for slot in &mut slots[col..col + col_span] {
                        if *slot {
                            return Err(Error::InvalidCellSpan);
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
                });
                col += col_span;
            }
        }
        if taken.iter().flatten().any(|slot| !slot) {
            return Err(Error::InvalidCellSpan);
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

    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> {
        let table = match self.0 {
            TableState::Open(grid) => measure(grid, ctx, avail)?,
            TableState::Measured(table) => table,
        };
        split_measured(table, avail)
    }
}

fn measure(grid: GridTable, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<MeasuredTable> {
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
        return Err(Error::UnsplittableTallerThanFrame);
    }

    let mut laid = Vec::with_capacity(grid.cells.len());
    let mut row_h = vec![0.0f32; grid.rows];
    for PlacedCell { span: cell, body } in grid.cells {
        let inner_w = xs[cell.col + cell.cols] - xs[cell.col] - 2.0 * inset;
        if inner_w <= 0.0 {
            return Err(Error::ColumnWidths {
                reason: "content-width",
            });
        }
        let frame = Frame {
            width: inner_w,
            height: inner_full,
            full_height: inner_full,
        };
        let (h, content) = natural_height(body.into_node(), ctx, frame)?;
        let cell_h = h + 2.0 * inset;
        if cell.rows == 1 {
            row_h[cell.row] = row_h[cell.row].max(cell_h);
        }
        laid.push(LaidCell {
            span: cell,
            height: cell_h,
            content,
        });
    }
    let row_y = row_offsets(row_h, &laid);
    let groups = row_groups(&laid, grid.header_rows, grid.rows);

    let stroke = Positive::pt(border).ok().map(Stroke::new);
    let paint = Paint::solid(grid.border.color);
    let mut fragments: Vec<Fragment> = groups
        .iter()
        .map(|rows| Fragment {
            height: row_y[rows.end] - row_y[rows.start],
            spacing_after: 0.0,
            commands: Vec::new(),
        })
        .collect();
    for LaidCell {
        span: cell,
        content,
        ..
    } in laid
    {
        let group = groups
            .iter()
            .position(|rows| rows.contains(&cell.row))
            .expect("every row belongs to a group");
        let x = xs[cell.col];
        let y = row_y[cell.row] - row_y[groups[group].start];
        let commands = &mut fragments[group].commands;
        commands.extend(content.with_offset(x + inset, y + inset)?.commands);
        if let Some(stroke) = stroke {
            let w = xs[cell.col + cell.cols] - x;
            let h = row_y[cell.row + cell.rows] - row_y[cell.row];
            commands.push(FragmentCommand::Stroke {
                path: rect_path(x, y, w, h)?,
                paint,
                stroke,
            });
        }
    }

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
            return Err(Error::UnsplittableTallerThanFrame);
        }
    }
    if header_h > avail.full_height {
        return Err(Error::UnsplittableTallerThanFrame);
    }
    Ok(MeasuredTable {
        width: avail.width,
        header,
        body: fragments,
        repeat_header: grid.repeat_header,
        draw_header_next: true,
    })
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

fn split_measured(mut table: MeasuredTable, avail: Frame) -> Result<Split> {
    if table.width > avail.width {
        return Err(Error::InvalidGeometry);
    }
    let mut y = 0.0f32;
    let mut commands = Vec::new();
    if table.draw_header_next {
        if let Some(header) = &table.header {
            let first = table.body.front().map_or(0.0, |b| b.height);
            if header.height + first > avail.height {
                return Ok(Split::MoveToNextPage(FlowNode::Table(TableNode(
                    TableState::Measured(table),
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
        return Ok(Split::MoveToNextPage(FlowNode::Table(TableNode(
            TableState::Measured(table),
        ))));
    }
    table.draw_header_next = table.repeat_header && table.header.is_some();
    Ok(Split::Partial {
        head_height: y,
        head,
        tail: FlowNode::Table(TableNode(TableState::Measured(table))),
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
    use crate::layout::flow::KeepTogetherNode;
    use crate::layout::paginate::PaginateConfig;
    use crate::layout::style::Style;
    use crate::layout::test_util::{
        bits, fonts_and_style, metrics, page, pt, render, runs, strokes, text, texts,
    };

    fn cell(s: &str, style: &Style) -> Cell {
        Cell::new(vec![text(s, style)]).unwrap()
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

    fn numbered_rows(style: &Style, n: usize) -> Vec<Vec<Cell>> {
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
        assert!(matches!(result, Err(Error::UnsplittableTallerThanFrame)));
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
                Error::InvalidCellContent {
                    container: "table cell",
                    node: name
                }
            );
        }
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
        assert_eq!(TableNode::new(spec).unwrap_err(), Error::InvalidCellSpan);
    }

    #[test]
    fn spans_past_the_grid_or_out_of_the_header_are_rejected() {
        let (_, style) = fonts_and_style();
        let (c3, r1) = span(3, 1);
        let mut wide = TableSpec::new(halves());
        wide.body = vec![vec![cell("W", &style).with_span(c3, r1)]];
        assert_eq!(TableNode::new(wide).unwrap_err(), Error::InvalidCellSpan);

        let (c1, r2) = span(1, 2);
        let mut leak = TableSpec::new(halves());
        leak.header = vec![vec![cell("H", &style).with_span(c1, r2), cell("k", &style)]];
        leak.body = vec![vec![cell("b", &style)]];
        assert_eq!(TableNode::new(leak).unwrap_err(), Error::InvalidCellSpan);

        let mut gap = TableSpec::new(halves());
        gap.body = vec![vec![cell("only", &style)]];
        assert_eq!(TableNode::new(gap).unwrap_err(), Error::InvalidCellSpan);
    }
}
