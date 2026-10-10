use crate::error::{ColumnWidthsReason, Error, ErrorKind, GeometryKind, Result};
use crate::geom::{NonNegative, Positive};
use crate::layout::columns::{
    default_column_gap, resolve_column_widths, ColumnWidth, ColumnsNode, Ratio,
};
use crate::layout::decorate::Decorations;
use crate::layout::flow::{FlowNode, KeepTogetherNode};
use crate::layout::paragraph::ParagraphNode;
use crate::layout::test_util::{bits, fonts_and_style, metrics, page, render, runs, text, texts};

fn fixed(w: f32) -> ColumnWidth {
    ColumnWidth::Fixed(Positive::pt(w).unwrap())
}

fn ratio(w: f32) -> ColumnWidth {
    ColumnWidth::Ratio(Ratio::new(w).unwrap())
}

fn reason(result: Result<impl std::fmt::Debug>) -> ColumnWidthsReason {
    match result {
        Err(Error {
            kind: ErrorKind::ColumnWidths { reason },
            ..
        }) => reason,
        other => panic!("expected ColumnWidths, got {other:?}"),
    }
}

#[test]
fn auto_takes_what_fixed_leaves() {
    let widths = resolve_column_widths(&[fixed(80.0), ColumnWidth::Auto], 188.0).unwrap();
    assert_eq!(widths, [80.0, 108.0]);
}

#[test]
fn ratios_split_the_rest_by_weight() {
    let widths = resolve_column_widths(&[ratio(2.0), ratio(1.0)], 90.0).unwrap();
    assert_eq!(widths, [60.0, 30.0]);
}

#[test]
fn auto_beside_ratio_is_rejected() {
    let specs = [fixed(10.0), ratio(1.0), ColumnWidth::Auto];
    assert_eq!(
        reason(resolve_column_widths(&specs, 110.0)),
        ColumnWidthsReason::AutoWithRatio
    );
}

#[test]
fn fixed_only_leaves_slack_on_the_right() {
    assert_eq!(
        resolve_column_widths(&[fixed(50.0)], 200.0).unwrap(),
        [50.0]
    );
}

#[test]
fn column_widths_more_than_one_auto() {
    let specs = [ColumnWidth::Auto, ColumnWidth::Auto];
    assert_eq!(
        reason(resolve_column_widths(&specs, 100.0)),
        ColumnWidthsReason::MoreThanOneAuto
    );
}

#[test]
fn column_widths_fixed_sum_exceeds() {
    let specs = [fixed(150.0), fixed(60.0)];
    assert_eq!(
        reason(resolve_column_widths(&specs, 200.0)),
        ColumnWidthsReason::FixedSumExceeds
    );
}

#[test]
fn column_widths_no_remaining_width() {
    let specs = [fixed(100.0), ColumnWidth::Auto];
    assert_eq!(
        reason(resolve_column_widths(&specs, 100.0)),
        ColumnWidthsReason::NoRemainingWidth
    );
}

#[test]
fn column_widths_bad_ratio() {
    assert_eq!(
        reason(Ratio::new(0.0)),
        ColumnWidthsReason::NonPositiveRatio
    );
    assert_eq!(
        reason(Ratio::new(f32::NAN)),
        ColumnWidthsReason::NonFiniteRatio
    );
}

#[test]
fn column_widths_column_count() {
    let (_, style) = fonts_and_style();
    let one = ColumnsNode::new(
        default_column_gap(),
        vec![(fixed(10.0), vec![text("A", &style)])],
    );
    assert_eq!(reason(one), ColumnWidthsReason::ColumnCount);
}

#[test]
fn column_widths_gap_eats_the_frame() {
    let (mut fonts, style) = fonts_and_style();
    let cols = ColumnsNode::new(
        NonNegative::pt(300.0).unwrap(),
        vec![
            (ColumnWidth::Auto, vec![text("A", &style)]),
            (fixed(10.0), vec![text("B", &style)]),
        ],
    )
    .unwrap();
    let (result, rec) = render(
        FlowNode::Columns(cols),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    assert_eq!(reason(result), ColumnWidthsReason::Gap);
    assert!(rec.commands().is_empty());
}

#[test]
fn columns_place_each_column_at_its_x() {
    let (mut fonts, style) = fonts_and_style();
    let b = metrics(&fonts, &style).baseline;
    let cols = ColumnsNode::new(
        default_column_gap(),
        vec![
            (fixed(80.0), vec![text("A", &style)]),
            (ColumnWidth::Auto, vec![text("X", &style)]),
        ],
    )
    .unwrap();
    let (result, rec) = render(
        FlowNode::Columns(cols),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    let runs = runs(&rec.pages()[0]);
    assert_eq!(
        runs.iter().map(bits).collect::<Vec<_>>(),
        [
            ("A", 0.0f32.to_bits(), b.to_bits()),
            ("X", 92.0f32.to_bits(), b.to_bits())
        ]
    );
}

#[test]
fn text_wraps_at_column_width_not_body_width() {
    let (mut fonts, style) = fonts_and_style();
    let m = metrics(&fonts, &style);
    let words = "AAAA AAAA AAAA";
    let cols = ColumnsNode::new(
        default_column_gap(),
        vec![
            (fixed(60.0), vec![text(words, &style)]),
            (ColumnWidth::Auto, Vec::new()),
        ],
    )
    .unwrap();
    let root = FlowNode::Stack(vec![text(words, &style), FlowNode::Columns(cols)]);
    let (result, rec) = render(root, &mut fonts, page(200.0, 400.0), &Decorations::none());
    result.unwrap();
    let ys: Vec<u32> = runs(&rec.pages()[0])
        .iter()
        .map(|r| r.2.to_bits())
        .collect();
    let line = |n: f32| (m.line_height * n + m.baseline).to_bits();
    assert_eq!(ys, [line(0.0), line(1.0), line(2.0), line(3.0)]);
}

#[test]
fn columns_split_across_pages_per_column() {
    let (mut fonts, style) = fonts_and_style();
    let m = metrics(&fonts, &style);
    let cols = ColumnsNode::new(
        default_column_gap(),
        vec![
            (ratio(1.0), vec![text("A\nB\nC", &style)]),
            (ratio(1.0), vec![text("X", &style)]),
        ],
    )
    .unwrap();
    let config = page(200.0, m.line_height * 2.0 + 1.0);
    let (result, rec) = render(
        FlowNode::Columns(cols),
        &mut fonts,
        config,
        &Decorations::none(),
    );
    result.unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(texts(&pages[0]), ["A", "B", "X"]);
    assert_eq!(
        runs(&pages[1]).iter().map(bits).collect::<Vec<_>>(),
        [("C", 0.0f32.to_bits(), m.baseline.to_bits())]
    );
}

#[test]
fn columns_spacing_after_is_the_largest_column_spacing() {
    let (mut fonts, style) = fonts_and_style();
    let m = metrics(&fonts, &style);
    let mut wide = style.clone();
    wide.paragraph_spacing = NonNegative::pt(8.0).unwrap();
    let mut narrow = style.clone();
    narrow.paragraph_spacing = NonNegative::pt(3.0).unwrap();
    let cols = ColumnsNode::new(
        default_column_gap(),
        vec![
            (ratio(1.0), vec![text("A", &narrow)]),
            (ratio(1.0), vec![text("B", &wide)]),
        ],
    )
    .unwrap();
    let root = FlowNode::Stack(vec![FlowNode::Columns(cols), text("Z", &style)]);
    let (result, rec) = render(root, &mut fonts, page(200.0, 400.0), &Decorations::none());
    result.unwrap();
    let z = runs(&rec.pages()[0]).pop().unwrap();
    assert_eq!(z.2.to_bits(), (m.line_height + 8.0 + m.baseline).to_bits());
}

#[test]
fn stuck_column_on_full_page_errors_with_no_output() {
    let (mut fonts, style) = fonts_and_style();
    let m = metrics(&fonts, &style);
    let cols = ColumnsNode::new(
        default_column_gap(),
        vec![
            (
                ratio(1.0),
                vec![FlowNode::KeepTogether(KeepTogetherNode::Open(vec![text(
                    "A\nB\nC", &style,
                )]))],
            ),
            (ratio(1.0), vec![text("X", &style)]),
        ],
    )
    .unwrap();
    let config = page(200.0, m.line_height * 2.0 + 1.0);
    let (result, rec) = render(
        FlowNode::Columns(cols),
        &mut fonts,
        config,
        &Decorations::none(),
    );
    assert!(matches!(
        result.unwrap_err().kind,
        ErrorKind::UnsplittableTallerThanFrame { height, avail }
            if height > avail && avail.to_bits() == (m.line_height * 2.0 + 1.0).to_bits()
    ));
    assert!(rec.commands().is_empty());
}

#[test]
fn pre_laid_paragraph_wider_than_column_errors() {
    let (mut fonts, style) = fonts_and_style();
    let wide = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
    let cols = ColumnsNode::new(
        default_column_gap(),
        vec![
            (fixed(80.0), vec![FlowNode::Paragraph(wide)]),
            (ColumnWidth::Auto, Vec::new()),
        ],
    )
    .unwrap();
    let (result, _) = render(
        FlowNode::Columns(cols),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    assert_eq!(
        result.unwrap_err().kind,
        ErrorKind::InvalidGeometry(GeometryKind::LaidOutWiderThanFrame {
            width: 200.0,
            avail: 80.0,
        })
    );
}
