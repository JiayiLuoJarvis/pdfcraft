use std::num::NonZeroU32;

use crate::canvas::{pt_near, CanvasCommand, RecordedPage};
use crate::error::{Error, ErrorBag, ErrorKind, GeometryKind};
use crate::geom::{Color, NonNegative, Positive, Size};
use crate::layout::canvas::CanvasNode;
use crate::layout::column_width::ColumnWidth;
use crate::layout::columns::ColumnsNode;
use crate::layout::decorate::Decorations;
use crate::layout::flow::{AnchorNode, FlowNode, KeepTogetherNode};
use crate::layout::frame::{natural_height, Fragment, FragmentCommand, Frame, LayoutCtx};
use crate::layout::list::{
    first_baseline, ListItem, ListKind, ListNode, ListSpec, MarkerAffix, MarkerAlign, MarkerKind,
};
use crate::layout::style::{GlyphStyle, Style, TextAlign};
use crate::layout::table::{Cell, TableNode, TableSpec};
use crate::layout::test_util::{bits, fonts_and_style, metrics, page, render, runs, text, texts};

fn spec(
    kind: ListKind,
    marker_width: f32,
    style: &Style,
    items: Vec<ListItem<FlowNode>>,
) -> ListSpec<FlowNode> {
    ListSpec {
        kind,
        marker_width: Positive::pt(marker_width).unwrap(),
        body_indent: None,
        marker_style: GlyphStyle::plain(style),
        item_gap: NonNegative::ZERO,
        affix: MarkerAffix::DotSuffix,
        align: MarkerAlign::Start,
        items,
    }
}

fn ordered(style: MarkerKind, start: u32) -> ListKind {
    ListKind::Ordered {
        style,
        start: NonZeroU32::new(start).unwrap(),
    }
}

fn parens(mut built: ListSpec<FlowNode>) -> ListSpec<FlowNode> {
    built.affix = MarkerAffix::Parens;
    built
}

fn marker_right(page: &RecordedPage, text: &str) -> f32 {
    page.ops()
        .iter()
        .find_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } if run.text == text => {
                let width = run
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.advance_norm)
                    .sum::<f32>()
                    * run.font_size.as_pt();
                Some(run.origin.x().as_pt() + width)
            }
            CanvasCommand::DrawGlyphRun { .. }
            | CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::StrokePath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::AnnotateLink { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform => None,
        })
        .unwrap_or_else(|| panic!("missing glyph run {text}"))
}

fn item(s: &str, style: &Style) -> ListItem<FlowNode> {
    ListItem::new(vec![text(s, style)]).unwrap()
}

fn gap_list(
    kind: ListKind,
    gap: f32,
    style: &Style,
    items: Vec<ListItem<FlowNode>>,
) -> ListNode<FlowNode> {
    let mut built = spec(kind, 20.0, style, items);
    built.item_gap = NonNegative::pt(gap).unwrap();
    ListNode::new(built).unwrap()
}

fn y_of(drawn: &[(String, f32, f32)], label: &str) -> f32 {
    drawn.iter().find(|run| run.0 == label).unwrap().2
}

#[test]
fn hanging_indent_and_marker_on_first_fragment_only() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let list = ListNode::new(spec(
        ordered(MarkerKind::Decimal, 1),
        20.0,
        &style,
        vec![item("A\nB\nC", &style), item("D", &style)],
    ))
    .unwrap();
    let config = page(200.0, m.line_height.as_pt() * 2.0 + 1.0);
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        config,
        &Decorations::none(),
    );
    result.unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    let first_line = m.baseline.as_pt().to_bits();
    let second_line = (m.line_height.as_pt() + m.baseline.as_pt()).to_bits();
    assert_eq!(
        runs(&pages[0]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 0.0f32.to_bits(), first_line),
            ("A", 20.0f32.to_bits(), first_line),
            ("B", 20.0f32.to_bits(), second_line),
        ]
    );
    assert_eq!(
        runs(&pages[1]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("C", 20.0f32.to_bits(), first_line),
            ("2.", 0.0f32.to_bits(), second_line),
            ("D", 20.0f32.to_bits(), second_line),
        ]
    );
}

#[test]
fn nested_list_indents_by_its_own_marker_width() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let inner = ListNode::new(spec(
        ListKind::Bullet,
        16.0,
        &style,
        vec![item("in", &style)],
    ))
    .unwrap();
    let outer = ListNode::new(spec(
        ListKind::Bullet,
        20.0,
        &style,
        vec![ListItem::new(vec![text("out", &style), FlowNode::List(inner)]).unwrap()],
    ))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(outer),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    let xs: Vec<(String, f32)> = runs(&rec.pages()[0])
        .into_iter()
        .map(|r| (r.0, r.1))
        .collect();
    assert_eq!(
        xs,
        [
            ("\u{2022}".to_owned(), 0.0),
            ("out".to_owned(), 20.0),
            ("\u{2022}".to_owned(), 20.0),
            ("in".to_owned(), 36.0),
        ]
    );
}

#[test]
fn body_indent_moves_the_body_column() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let mut s = spec(ListKind::Bullet, 14.0, &style, vec![item("A", &style)]);
    s.body_indent = Some(Positive::pt(30.0).unwrap());
    let list = ListNode::new(s).unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    let xs: Vec<f32> = runs(&rec.pages()[0]).iter().map(|r| r.1).collect();
    assert_eq!(xs, [0.0, 30.0]);
}

#[test]
fn upper_roman_custom_marker_does_not_consume_the_counter() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let list = ListNode::new(parens(spec(
        ordered(MarkerKind::UpperRoman, 4),
        30.0,
        &style,
        vec![
            item("a", &style),
            ListItem::custom("(x)", vec![text("b", &style)]).unwrap(),
            item("c", &style),
        ],
    )))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(
        texts(&rec.pages()[0]),
        ["(IV)", "a", "(x)", "b", "(V)", "c"]
    );
}

#[test]
fn decimal_custom_marker_between_autos_does_not_consume() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let list = ListNode::new(parens(spec(
        ordered(MarkerKind::Decimal, 1),
        20.0,
        &style,
        vec![
            item("a", &style),
            ListItem::custom("(x)", vec![text("b", &style)]).unwrap(),
            item("c", &style),
        ],
    )))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(texts(&rec.pages()[0]), ["(1)", "a", "(x)", "b", "(2)", "c"]);
}

#[test]
fn leading_custom_marker_does_not_consume_the_first_number() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let list = ListNode::new(parens(spec(
        ordered(MarkerKind::Decimal, 1),
        20.0,
        &style,
        vec![
            ListItem::custom("(x)", vec![text("a", &style)]).unwrap(),
            item("b", &style),
            item("c", &style),
        ],
    )))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(texts(&rec.pages()[0]), ["(x)", "a", "(1)", "b", "(2)", "c"]);
}

const MARKER_LABELS: &[(MarkerKind, u64, MarkerAffix, &str)] = &[
    (MarkerKind::Decimal, 1, MarkerAffix::DotSuffix, "1."),
    (
        MarkerKind::Decimal,
        1,
        MarkerAffix::IdeographicComma,
        "1\u{3001}",
    ),
    (MarkerKind::Decimal, 1, MarkerAffix::ClosingParen, "1)"),
    (MarkerKind::Decimal, 1, MarkerAffix::Parens, "(1)"),
    (MarkerKind::Decimal, 1, MarkerAffix::Bare, "1"),
    (MarkerKind::Decimal, 12, MarkerAffix::DotSuffix, "12."),
    (
        MarkerKind::Decimal,
        12,
        MarkerAffix::IdeographicComma,
        "12\u{3001}",
    ),
    (MarkerKind::Decimal, 12, MarkerAffix::ClosingParen, "12)"),
    (MarkerKind::Decimal, 12, MarkerAffix::Parens, "(12)"),
    (MarkerKind::Decimal, 12, MarkerAffix::Bare, "12"),
    (MarkerKind::UpperAlpha, 3, MarkerAffix::DotSuffix, "C."),
    (
        MarkerKind::UpperAlpha,
        1,
        MarkerAffix::IdeographicComma,
        "A\u{3001}",
    ),
    (MarkerKind::UpperAlpha, 26, MarkerAffix::DotSuffix, "Z."),
    (
        MarkerKind::UpperAlpha,
        26,
        MarkerAffix::IdeographicComma,
        "Z\u{3001}",
    ),
    (MarkerKind::UpperAlpha, 26, MarkerAffix::ClosingParen, "Z)"),
    (MarkerKind::UpperAlpha, 26, MarkerAffix::Parens, "(Z)"),
    (MarkerKind::UpperAlpha, 26, MarkerAffix::Bare, "Z"),
    (MarkerKind::LowerAlpha, 1, MarkerAffix::ClosingParen, "a)"),
    (MarkerKind::LowerAlpha, 26, MarkerAffix::DotSuffix, "z."),
    (
        MarkerKind::LowerAlpha,
        26,
        MarkerAffix::IdeographicComma,
        "z\u{3001}",
    ),
    (MarkerKind::LowerAlpha, 26, MarkerAffix::ClosingParen, "z)"),
    (MarkerKind::LowerAlpha, 26, MarkerAffix::Parens, "(z)"),
    (MarkerKind::LowerAlpha, 26, MarkerAffix::Bare, "z"),
    (
        MarkerKind::UpperRoman,
        1994,
        MarkerAffix::DotSuffix,
        "MCMXCIV.",
    ),
    (
        MarkerKind::UpperRoman,
        3999,
        MarkerAffix::DotSuffix,
        "MMMCMXCIX.",
    ),
    (
        MarkerKind::UpperRoman,
        3999,
        MarkerAffix::IdeographicComma,
        "MMMCMXCIX\u{3001}",
    ),
    (
        MarkerKind::UpperRoman,
        3999,
        MarkerAffix::ClosingParen,
        "MMMCMXCIX)",
    ),
    (
        MarkerKind::UpperRoman,
        3999,
        MarkerAffix::Parens,
        "(MMMCMXCIX)",
    ),
    (MarkerKind::UpperRoman, 3999, MarkerAffix::Bare, "MMMCMXCIX"),
    (MarkerKind::LowerRoman, 4, MarkerAffix::Parens, "(iv)"),
    (
        MarkerKind::LowerRoman,
        3999,
        MarkerAffix::DotSuffix,
        "mmmcmxcix.",
    ),
    (
        MarkerKind::LowerRoman,
        3999,
        MarkerAffix::IdeographicComma,
        "mmmcmxcix\u{3001}",
    ),
    (
        MarkerKind::LowerRoman,
        3999,
        MarkerAffix::ClosingParen,
        "mmmcmxcix)",
    ),
    (
        MarkerKind::LowerRoman,
        3999,
        MarkerAffix::Parens,
        "(mmmcmxcix)",
    ),
    (MarkerKind::LowerRoman, 3999, MarkerAffix::Bare, "mmmcmxcix"),
];

#[test]
fn labels_for_each_marker_kind_and_affix() {
    for &(kind, number, affix, expected) in MARKER_LABELS {
        assert_eq!(kind.label(number, affix).unwrap(), expected);
    }
}

#[test]
fn bullet_ignores_marker_affix() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let mut built = spec(ListKind::Bullet, 20.0, &style, vec![item("A", &style)]);
    built.affix = MarkerAffix::Parens;
    let list = ListNode::new(built).unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(texts(&rec.pages()[0]), ["\u{2022}", "A"]);
}

#[test]
fn marker_align_end_shares_the_marker_column_right_edge() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let marker_width = 36.0;
    let mut built = spec(
        ordered(MarkerKind::Decimal, 9),
        marker_width,
        &style,
        vec![item("a", &style), item("b", &style)],
    );
    built.align = MarkerAlign::End;
    let list = ListNode::new(built).unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    let page = &rec.pages()[0];
    assert!(
        pt_near(marker_right(page, "9."), marker_width),
        "9. right edge"
    );
    assert!(
        pt_near(marker_right(page, "10."), marker_width),
        "10. right edge"
    );
    let xs: Vec<f32> = runs(page).iter().map(|run| run.1).collect();
    assert_eq!(xs[1].to_bits(), marker_width.to_bits());
    assert_eq!(xs[3].to_bits(), marker_width.to_bits());
}

#[test]
fn marker_align_start_places_the_marker_at_x_zero() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let mut built = spec(
        ordered(MarkerKind::Decimal, 9),
        36.0,
        &style,
        vec![item("a", &style), item("b", &style)],
    );
    built.align = MarkerAlign::Start;
    let list = ListNode::new(built).unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    let xs: Vec<f32> = runs(&rec.pages()[0]).iter().map(|run| run.1).collect();
    assert_eq!(xs[0].to_bits(), 0.0f32.to_bits());
    assert_eq!(xs[2].to_bits(), 0.0f32.to_bits());
}

#[test]
fn labels_past_z_or_3999_are_errors() {
    assert_eq!(
        MarkerKind::UpperAlpha.label(27, MarkerAffix::Parens),
        Err(GeometryKind::MarkerOutOfRange { number: 27 }.into())
    );
    assert_eq!(
        MarkerKind::LowerRoman.label(4000, MarkerAffix::DotSuffix),
        Err(GeometryKind::MarkerOutOfRange { number: 4000 }.into())
    );
    let Some((_, style)) = fonts_and_style() else {
        return;
    };
    let items = (0..2).map(|_| item("a", &style)).collect();
    let err = ListNode::new(spec(
        ordered(MarkerKind::LowerAlpha, 26),
        20.0,
        &style,
        items,
    ));
    assert_eq!(
        err.unwrap_err(),
        GeometryKind::MarkerOutOfRange { number: 27 }.into()
    );
    let sandwiched = ListNode::new(spec(
        ordered(MarkerKind::LowerAlpha, 26),
        20.0,
        &style,
        vec![
            item("a", &style),
            ListItem::custom("(x)", vec![text("b", &style)]).unwrap(),
            item("c", &style),
        ],
    ));
    assert_eq!(
        sandwiched.unwrap_err(),
        GeometryKind::MarkerOutOfRange { number: 27 }.into()
    );
}

#[test]
fn custom_marker_wider_than_marker_width_errors() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let list = ListNode::new(spec(
        ListKind::Bullet,
        10.0,
        &style,
        vec![ListItem::custom("(1000)", vec![text("A", &style)]).unwrap()],
    ))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    assert!(
        matches!(result.map_err(|error| error.kind), Err(ErrorKind::UnbreakableWiderThanFrame { avail, .. }) if avail.to_bits() == 10.0f32.to_bits())
    );
    assert!(rec.commands().is_empty());
}

#[test]
fn empty_custom_marker_is_rejected() {
    let Some((_, style)) = fonts_and_style() else {
        return;
    };
    let err = ListItem::custom("", vec![text("A", &style)]).unwrap_err();
    assert_eq!(err, GeometryKind::EmptyListMarker.into());
}

#[test]
fn image_command_has_no_baseline() {
    let fragment = Fragment {
        height: NonNegative::pt(10.0).unwrap(),
        spacing_after: NonNegative::ZERO,
        commands: vec![FragmentCommand::Image {
            image: crate::canvas::DecodedImage {
                id: crate::canvas::ImageId::from_raw(3),
                pixel_width: 1,
                pixel_height: 1,
            },
            rect: crate::geom::Rect::try_pt(0.0, 4.0, 10.0, 10.0).unwrap(),
        }],
        anchors: Vec::new(),
    };
    assert_eq!(first_baseline(&fragment), None);
}

fn image_node() -> FlowNode {
    FlowNode::Image(
        crate::layout::image::ImageNode::new(
            crate::canvas::DecodedImage {
                id: crate::canvas::ImageId::from_raw(0),
                pixel_width: 1,
                pixel_height: 1,
            },
            Some(Positive::pt(10.0).unwrap()),
            Some(Positive::pt(10.0).unwrap()),
            TextAlign::Start,
        )
        .unwrap(),
    )
}

fn canvas_node() -> FlowNode {
    let side = Positive::pt(10.0).unwrap();
    FlowNode::Canvas(CanvasNode::new(Size::new(side, side), TextAlign::Start, Vec::new()).unwrap())
}

fn one_cell_table(s: &str, style: &Style) -> FlowNode {
    let mut table = TableSpec::new(vec![ColumnWidth::Auto]);
    table.body = vec![vec![Cell::new(vec![text(s, style)]).unwrap()]];
    FlowNode::Table(TableNode::new(table).unwrap())
}

fn two_columns(left: Vec<FlowNode>, right: Vec<FlowNode>) -> FlowNode {
    FlowNode::Columns(
        ColumnsNode::new(
            NonNegative::pt(10.0).unwrap(),
            vec![
                (ColumnWidth::Auto, left),
                (ColumnWidth::Fixed(Positive::pt(80.0).unwrap()), right),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn item_holds_tables_columns_images_canvases_groups_and_nested_lists() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let innermost = ListNode::new(spec(
        ordered(MarkerKind::LowerAlpha, 1),
        16.0,
        &style,
        vec![item("deep", &style)],
    ))
    .unwrap();
    let bullets = ListNode::new(spec(
        ListKind::Bullet,
        16.0,
        &style,
        vec![ListItem::new(vec![text("mid", &style), FlowNode::List(innermost)]).unwrap()],
    ))
    .unwrap();
    let content = vec![
        one_cell_table("cell", &style),
        two_columns(vec![text("left", &style)], vec![text("right", &style)]),
        image_node(),
        canvas_node(),
        FlowNode::KeepTogether(KeepTogetherNode::Open(vec![text("kept", &style)])),
        FlowNode::List(bullets),
    ];
    let list = ListNode::new(spec(
        ordered(MarkerKind::Decimal, 1),
        20.0,
        &style,
        vec![ListItem::new(content).unwrap()],
    ))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(300.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(
        texts(&rec.pages()[0]),
        ["1.", "cell", "left", "right", "kept", "\u{2022}", "mid", "a.", "deep"]
    );
}

#[test]
fn item_rejects_page_breaks_inside_groups_stacks_and_anchors() {
    let wrapped = [
        FlowNode::PageBreak,
        FlowNode::Stack(vec![FlowNode::PageBreak]),
        FlowNode::KeepTogether(KeepTogetherNode::Open(vec![FlowNode::PageBreak])),
        FlowNode::Anchor(AnchorNode {
            id: "a".to_owned(),
            child: Box::new(FlowNode::PageBreak),
            marked: false,
        }),
    ];
    for node in wrapped {
        assert_eq!(
            ListItem::new(vec![node]).unwrap_err(),
            Error::from(ErrorKind::InvalidCellContent {
                container: "list item",
                node: "page break"
            })
        );
    }
}

#[test]
fn column_may_hold_a_list_whose_item_holds_a_table() {
    let Some((_, style)) = fonts_and_style() else {
        return;
    };
    let list = ListNode::new(spec(
        ListKind::Bullet,
        20.0,
        &style,
        vec![ListItem::new(vec![one_cell_table("cell", &style)]).unwrap()],
    ))
    .unwrap();
    ColumnsNode::new(
        NonNegative::ZERO,
        vec![
            (ColumnWidth::Auto, vec![FlowNode::List(list)]),
            (
                ColumnWidth::Fixed(Positive::pt(80.0).unwrap()),
                vec![text("x", &style)],
            ),
        ],
    )
    .unwrap();
}

#[test]
fn item_allows_hline() {
    let line = crate::layout::hline::HLineNode::new(
        None,
        TextAlign::Center,
        Positive::pt(1.0).unwrap(),
        Color::rgb(0, 0, 0),
        NonNegative::ZERO,
        NonNegative::ZERO,
    );
    ListItem::new(vec![FlowNode::HLine(line)]).unwrap();
}

#[test]
fn item_gap_shifts_the_next_item_by_the_line_height_plus_the_gap() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let list = gap_list(
        ordered(MarkerKind::Decimal, 1),
        6.0,
        &style,
        vec![item("A", &style), item("B", &style)],
    );
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    let first = m.baseline.as_pt();
    let second = m.line_height.as_pt() + 6.0 + first;
    assert_eq!(
        runs(&rec.pages()[0]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 0.0f32.to_bits(), first.to_bits()),
            ("A", 20.0f32.to_bits(), first.to_bits()),
            ("2.", 0.0f32.to_bits(), second.to_bits()),
            ("B", 20.0f32.to_bits(), second.to_bits()),
        ]
    );
}

#[test]
fn item_gap_is_only_between_items() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let h = m.line_height.as_pt();
    let base = m.baseline.as_pt();
    let gap = 6.0;

    {
        let one = FlowNode::Stack(vec![
            FlowNode::List(gap_list(
                ListKind::Bullet,
                gap,
                &style,
                vec![item("A", &style)],
            )),
            text("TAIL", &style),
        ]);
        let (result, rec) = render(one, &mut fonts, page(200.0, 400.0), &Decorations::none());
        result.unwrap();
        let drawn = runs(&rec.pages()[0]);
        assert_eq!(y_of(&drawn, "A").to_bits(), base.to_bits());
        assert_eq!(y_of(&drawn, "TAIL").to_bits(), (h + base).to_bits());
    }

    let three = FlowNode::Stack(vec![
        FlowNode::List(gap_list(
            ListKind::Bullet,
            gap,
            &style,
            vec![item("A", &style), item("B", &style), item("C", &style)],
        )),
        text("TAIL", &style),
    ]);
    let (three_result, three_rec) =
        render(three, &mut fonts, page(200.0, 400.0), &Decorations::none());
    three_result.unwrap();
    let three_drawn = runs(&three_rec.pages()[0]);
    let b = h + gap + base;
    let c = h + gap + h + gap + base;
    let tail = h + gap + h + gap + h + base;
    assert_eq!(y_of(&three_drawn, "A").to_bits(), base.to_bits());
    assert_eq!(y_of(&three_drawn, "B").to_bits(), b.to_bits());
    assert_eq!(y_of(&three_drawn, "C").to_bits(), c.to_bits());
    assert_eq!(y_of(&three_drawn, "TAIL").to_bits(), tail.to_bits());
}

#[test]
fn item_gap_is_dropped_when_the_page_cannot_hold_it() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let h = m.line_height.as_pt();
    let base = m.baseline.as_pt();
    let list = gap_list(
        ordered(MarkerKind::Decimal, 1),
        6.0,
        &style,
        vec![item("A", &style), item("B", &style)],
    );
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, h),
        &Decorations::none(),
    );
    result.unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(
        runs(&pages[0]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 0.0f32.to_bits(), base.to_bits()),
            ("A", 20.0f32.to_bits(), base.to_bits()),
        ]
    );
    assert_eq!(
        runs(&pages[1]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("2.", 0.0f32.to_bits(), base.to_bits()),
            ("B", 20.0f32.to_bits(), base.to_bits()),
        ]
    );
}

#[test]
fn dropped_item_gap_packs_the_next_item_on_the_same_page() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let h = m.line_height.as_pt();
    let base = m.baseline.as_pt();
    let list = gap_list(
        ordered(MarkerKind::Decimal, 1),
        h + 1.0,
        &style,
        vec![item("A", &style), item("B", &style)],
    );
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, h + h),
        &Decorations::none(),
    );
    result.unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 1);
    let second = h + base;
    assert_eq!(
        runs(&pages[0]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 0.0f32.to_bits(), base.to_bits()),
            ("A", 20.0f32.to_bits(), base.to_bits()),
            ("2.", 0.0f32.to_bits(), second.to_bits()),
            ("B", 20.0f32.to_bits(), second.to_bits()),
        ]
    );
}

#[test]
fn natural_height_counts_item_gaps_that_do_not_fit() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let h = m.line_height.as_pt();
    let gap = 6.0;
    let list = gap_list(
        ListKind::Bullet,
        gap,
        &style,
        vec![item("A", &style), item("B", &style), item("C", &style)],
    );
    let node = FlowNode::KeepTogether(KeepTogetherNode::Open(vec![FlowNode::List(list)]));
    let mut errors = ErrorBag::default();
    let mut ctx = LayoutCtx {
        natural: false,
        fonts: &mut fonts,
        errors: &mut errors,
    };
    let full = Frame {
        width: Positive::pt(200.0).unwrap(),
        height: NonNegative::pt(h).unwrap(),
        full_height: Positive::pt(h).unwrap(),
    };
    let err = natural_height(node, &mut ctx, full).unwrap_err();
    assert_eq!(
        err,
        Error::from(ErrorKind::UnsplittableTallerThanFrame {
            height: h + gap + h + gap + h,
            avail: h,
        })
    );
}

#[test]
fn item_gap_applies_to_bullets_and_numbers() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let second = m.line_height.as_pt() + 6.0 + m.baseline.as_pt();
    for kind in [ListKind::Bullet, ordered(MarkerKind::Decimal, 1)] {
        let list = gap_list(
            kind,
            6.0,
            &style,
            vec![item("A", &style), item("B", &style)],
        );
        let (result, rec) = render(
            FlowNode::List(list),
            &mut fonts,
            page(200.0, 400.0),
            &Decorations::none(),
        );
        result.unwrap();
        let drawn = runs(&rec.pages()[0]);
        assert_eq!(y_of(&drawn, "A").to_bits(), m.baseline.as_pt().to_bits());
        assert_eq!(y_of(&drawn, "B").to_bits(), second.to_bits());
    }
}

#[test]
fn nested_list_uses_its_own_item_gap() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let h = m.line_height.as_pt();
    let base = m.baseline.as_pt();
    let inner_gap = 6.0;
    let outer_gap = 20.0;
    let mut inner = spec(
        ListKind::Bullet,
        16.0,
        &style,
        vec![item("a", &style), item("b", &style)],
    );
    inner.item_gap = NonNegative::pt(inner_gap).unwrap();
    let inner = ListNode::new(inner).unwrap();
    let outer = gap_list(
        ordered(MarkerKind::Decimal, 1),
        outer_gap,
        &style,
        vec![
            ListItem::new(vec![text("out", &style), FlowNode::List(inner)]).unwrap(),
            item("tail", &style),
        ],
    );
    let (result, rec) = render(
        FlowNode::List(outer),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    let drawn = runs(&rec.pages()[0]);
    assert_eq!(
        texts(&rec.pages()[0]),
        ["1.", "out", "\u{2022}", "a", "\u{2022}", "b", "2.", "tail"]
    );
    assert_eq!(y_of(&drawn, "out").to_bits(), base.to_bits());
    assert_eq!(y_of(&drawn, "a").to_bits(), (h + base).to_bits());
    assert_eq!(
        y_of(&drawn, "b").to_bits(),
        (h + h + inner_gap + base).to_bits()
    );
    assert_eq!(
        y_of(&drawn, "tail").to_bits(),
        (h + h + inner_gap + h + outer_gap + base).to_bits()
    );
}
