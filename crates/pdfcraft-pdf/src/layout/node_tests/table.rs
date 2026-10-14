use std::num::NonZeroU16;

use crate::error::{CellSpanReason, Error, ErrorKind};
use crate::geom::{Color, Margins, NonNegative, Positive, Size};
use crate::layout::column_width::{ColumnWidth, Ratio};
use crate::layout::columns::{default_column_gap, ColumnsNode};
use crate::layout::decorate::Decorations;
use crate::layout::flow::{FlowNode, KeepTogetherNode, SpacerNode};
use crate::layout::paginate::PaginateConfig;
use crate::layout::style::Style;
use crate::layout::table::{Border, Cell, TableNode, TableSpec};
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
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let mut spec = TableSpec::new(halves());
    let (c2, r1) = span(2, 1);
    spec.header = vec![
        vec![cell("H1", &style).with_span(c2, r1)],
        vec![cell("a", &style), cell("b", &style)],
    ];
    spec.body = numbered_rows(&style, 4);
    let table = TableNode::new(spec).unwrap();
    let config = page(200.0, m.line_height.as_pt() * 4.0 + 1.0);
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
        ("H1", 0.0f32.to_bits(), m.baseline.as_pt().to_bits())
    );
    assert_eq!(
        second[3].2.to_bits(),
        (m.line_height.as_pt() * 2.0 + m.baseline.as_pt()).to_bits()
    );
}

#[test]
fn repeat_header_off_continues_body_only() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let mut spec = TableSpec::new(halves());
    spec.header = vec![vec![cell("h", &style), cell("k", &style)]];
    spec.body = numbered_rows(&style, 3);
    spec.repeat_header = false;
    let table = TableNode::new(spec).unwrap();
    let config = page(200.0, m.line_height.as_pt() * 2.0 + 1.0);
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
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let (c1, r2) = span(1, 2);
    let mut spec = TableSpec::new(halves());
    spec.body = vec![
        vec![cell("r0", &style), cell("x", &style)],
        vec![cell("S", &style).with_span(c1, r2), cell("c", &style)],
        vec![cell("d", &style)],
    ];
    let table = TableNode::new(spec).unwrap();
    let config = page(200.0, m.line_height.as_pt() * 2.0 + 1.0);
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
            ("S", 0.0f32.to_bits(), m.baseline.as_pt().to_bits()),
            ("c", 100.0f32.to_bits(), m.baseline.as_pt().to_bits()),
            (
                "d",
                100.0f32.to_bits(),
                (m.line_height.as_pt() + m.baseline.as_pt()).to_bits()
            ),
        ]
    );
}

#[test]
fn header_plus_group_taller_than_page_errors_with_no_output() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let mut spec = TableSpec::new(halves());
    spec.header = vec![vec![cell("h", &style), cell("k", &style)]];
    spec.body = vec![vec![cell("A\nB", &style), cell("1", &style)]];
    let table = TableNode::new(spec).unwrap();
    let config = page(200.0, m.line_height.as_pt() * 2.0 + 1.0);
    let (result, rec) = render(
        FlowNode::Table(table),
        &mut fonts,
        config,
        &Decorations::none(),
    );
    let ErrorKind::UnsplittableTallerThanFrame { height, avail } = result.unwrap_err().kind else {
        panic!("expected UnsplittableTallerThanFrame");
    };
    assert!(
        (height - m.line_height.as_pt() * 3.0).abs() < 1e-3,
        "{height}"
    );
    assert_eq!(
        avail.to_bits(),
        (m.line_height.as_pt() * 2.0 + 1.0).to_bits()
    );
    assert!(rec.commands().is_empty());
}

#[test]
fn borders_stroke_each_cell_centered_on_grid_lines() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
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
    let row = m.line_height.as_pt() + 10.0;
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
    let baseline = (15.0 + 5.0 + m.baseline.as_pt()).to_bits();
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
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
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
fn cell_height_includes_spacing_between_paragraphs() {
    let Some((mut fonts, mut style)) = fonts_and_style() else {
        return;
    };
    let line_h = metrics(&fonts, &style).line_height.as_pt();
    let spacing = line_h + 1.0;
    style.paragraph_spacing = NonNegative::pt(spacing).unwrap();
    let mut spec = TableSpec::new(vec![ColumnWidth::Fixed(Positive::pt(200.0).unwrap())]);
    spec.body = vec![vec![
        Cell::new(vec![text("A", &style), text("B", &style)]).unwrap()
    ]];
    let table = TableNode::new(spec).unwrap();
    let (result, rec) = render(
        FlowNode::Table(table),
        &mut fonts,
        page(200.0, line_h * 2.0),
        &Decorations::none(),
    );
    let ErrorKind::UnsplittableTallerThanFrame { height, avail } = result.unwrap_err().kind else {
        panic!("expected UnsplittableTallerThanFrame");
    };
    assert_eq!(height.to_bits(), (line_h + spacing + line_h).to_bits());
    assert_eq!(avail.to_bits(), (line_h * 2.0).to_bits());
    assert!(rec.commands().is_empty());
}

#[test]
fn cell_rejects_columns_page_break_keep_together_and_tables() {
    let Some((_, style)) = fonts_and_style() else {
        return;
    };
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
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
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
    Cell::new(vec![FlowNode::Spacer(SpacerNode { height: pt(8.0) })]).unwrap();
}

#[test]
fn overlapping_spans_are_rejected() {
    let Some((_, style)) = fonts_and_style() else {
        return;
    };
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
    let Some((_, style)) = fonts_and_style() else {
        return;
    };
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
