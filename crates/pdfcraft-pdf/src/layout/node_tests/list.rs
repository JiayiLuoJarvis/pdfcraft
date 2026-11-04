use std::num::{NonZeroU16, NonZeroU32};

use crate::canvas::{pt_near, CanvasCommand, RecordedPage};
use crate::error::{Error, ErrorBag, ErrorKind, GeometryKind};
use crate::fonts::FontStore;
use crate::geom::{Color, Finite, NonNegative, Point, Positive, Size};
use crate::layout::canvas::{CanvasMarkNode, CanvasNode};
use crate::layout::column_width::ColumnWidth;
use crate::layout::columns::ColumnsNode;
use crate::layout::decorate::Decorations;
use crate::layout::flow::{AnchorNode, FlowNode, KeepTogetherNode, SpacerNode};
use crate::layout::frame::{
    natural_height, Flowable, Fragment, FragmentCommand, Frame, LayoutCtx, Split,
};
use crate::layout::hline::HLineNode;
use crate::layout::list::{
    ListItem, ListKind, ListNode, ListSpec, MarkerAffix, MarkerAlign, MarkerKind,
};
use crate::layout::paragraph::{BlankWidth, InlineBlank, InlineItem, TextNode};
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
            children: vec![
                FlowNode::Spacer(SpacerNode {
                    height: NonNegative::ZERO,
                }),
                FlowNode::PageBreak,
            ],
            landed: false,
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
    let line = HLineNode::new(
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

fn one_item(marker: &Style, blocks: Vec<FlowNode>) -> FlowNode {
    FlowNode::List(
        ListNode::new(spec(
            ordered(MarkerKind::Decimal, 1),
            20.0,
            marker,
            vec![ListItem::new(blocks).unwrap()],
        ))
        .unwrap(),
    )
}

fn page_runs(node: FlowNode, fonts: &mut FontStore) -> Vec<(String, f32, f32)> {
    let (result, rec) = render(node, fonts, page(280.0, 500.0), &Decorations::none());
    result.unwrap();
    assert_eq!(rec.pages().len(), 1, "the sample fits on one page");
    runs(&rec.pages()[0])
}

fn split_frame(
    node: FlowNode,
    fonts: &mut FontStore,
    width: f32,
    height: f32,
    full: f32,
) -> Split<FlowNode> {
    let mut errors = ErrorBag::default();
    let mut ctx = LayoutCtx {
        natural: false,
        fonts,
        errors: &mut errors,
    };
    let split = node
        .split(
            &mut ctx,
            Frame {
                width: Positive::pt(width).unwrap(),
                height: NonNegative::pt(height).unwrap(),
                full_height: Positive::pt(full).unwrap(),
            },
        )
        .unwrap();
    assert!(errors.is_empty());
    split
}

fn fit_fragment(node: FlowNode, fonts: &mut FontStore) -> Fragment {
    let Split::Fit { fragment, .. } = split_frame(node, fonts, 240.0, 400.0, 400.0) else {
        panic!("node fits");
    };
    fragment
}

fn sized(style: &Style, points: f32) -> Style {
    let mut style = style.clone();
    style.font_size = Positive::pt(points).unwrap();
    style
}

fn baseline_bits(fragment: &Fragment) -> u32 {
    fragment
        .first_baseline
        .expect("text baseline")
        .as_pt()
        .to_bits()
}

fn has_glyphs(fragment: &Fragment, label: &str) -> bool {
    fragment.commands.iter().any(|command| match command {
        FragmentCommand::Glyphs(run) => run.text == label,
        FragmentCommand::Fill { .. }
        | FragmentCommand::Stroke { .. }
        | FragmentCommand::Image { .. }
        | FragmentCommand::Link { .. }
        | FragmentCommand::PushTransform { .. }
        | FragmentCommand::PopTransform => false,
    })
}

fn spacer(height: f32) -> FlowNode {
    FlowNode::Spacer(SpacerNode {
        height: NonNegative::pt(height).unwrap(),
    })
}

fn rule() -> FlowNode {
    FlowNode::HLine(HLineNode::new(
        None,
        TextAlign::Start,
        Positive::pt(2.0).unwrap(),
        Color::rgb(0, 0, 0),
        NonNegative::ZERO,
        NonNegative::ZERO,
    ))
}

fn canvas_text(style: &Style) -> FlowNode {
    let side = Positive::pt(40.0).unwrap();
    FlowNode::Canvas(
        CanvasNode::new(
            Size::new(side, side),
            TextAlign::Start,
            [CanvasMarkNode::Text {
                text: "C".into(),
                at: Point::try_pt(0.0, 30.0).unwrap(),
                style: GlyphStyle::plain(style),
            }],
        )
        .unwrap(),
    )
}

fn text_table(style: &Style, padding: f32, border: f32, header: bool) -> FlowNode {
    let mut spec = TableSpec::new(vec![ColumnWidth::Fixed(Positive::pt(80.0).unwrap())]);
    spec.padding = NonNegative::pt(padding).unwrap();
    spec.border.width = NonNegative::pt(border).unwrap();
    let cell = |label: &str| Cell::new(vec![text(label, style)]).unwrap();
    if header {
        spec.header = vec![vec![cell("H")]];
    }
    spec.body = vec![vec![cell("B")]];
    FlowNode::Table(TableNode::new(spec).unwrap())
}

#[test]
fn marker_matches_the_paragraph_first_line() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let drawn = page_runs(one_item(&style, vec![text("A\nB", &style)]), &mut fonts);
    let base = m.baseline.as_pt().to_bits();
    let second = (m.line_height.as_pt() + m.baseline.as_pt()).to_bits();
    assert_eq!(y_of(&drawn, "1.").to_bits(), base);
    assert_eq!(y_of(&drawn, "A").to_bits(), base);
    assert_eq!(y_of(&drawn, "B").to_bits(), second);
}

#[test]
fn marker_top_aligns_when_the_item_starts_with_an_image() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let drawn = page_runs(
        one_item(&style, vec![image_node(), text("A", &style)]),
        &mut fonts,
    );
    assert_eq!(y_of(&drawn, "1.").to_bits(), m.baseline.as_pt().to_bits());
    assert_eq!(
        y_of(&drawn, "A").to_bits(),
        (10.0 + m.baseline.as_pt()).to_bits()
    );
}

#[test]
fn marker_top_aligns_when_the_item_starts_with_a_spacer() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let drawn = page_runs(
        one_item(&style, vec![spacer(10.0), text("A", &style)]),
        &mut fonts,
    );
    assert_eq!(y_of(&drawn, "1.").to_bits(), m.baseline.as_pt().to_bits());
    assert_eq!(
        y_of(&drawn, "A").to_bits(),
        (10.0 + m.baseline.as_pt()).to_bits()
    );
}

#[test]
fn marker_top_aligns_when_the_item_starts_with_a_rule_or_canvas() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let ruled = page_runs(
        one_item(&style, vec![rule(), text("A", &style)]),
        &mut fonts,
    );
    assert_eq!(y_of(&ruled, "1.").to_bits(), m.baseline.as_pt().to_bits());
    assert_eq!(
        y_of(&ruled, "A").to_bits(),
        (2.0 + m.baseline.as_pt()).to_bits()
    );
    let painted = page_runs(
        one_item(&style, vec![canvas_text(&style), text("A", &style)]),
        &mut fonts,
    );
    assert_eq!(y_of(&painted, "1.").to_bits(), m.baseline.as_pt().to_bits());
    assert_eq!(y_of(&painted, "C").to_bits(), 30.0f32.to_bits());
    assert_eq!(
        y_of(&painted, "A").to_bits(),
        (40.0 + m.baseline.as_pt()).to_bits()
    );
}

#[test]
fn zero_height_spacer_and_empty_keep_together_are_skipped() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let cases = [
        vec![spacer(0.0), text("A", &style)],
        vec![
            FlowNode::KeepTogether(KeepTogetherNode::Open(Vec::new())),
            text("A", &style),
        ],
    ];
    for blocks in cases {
        let drawn = page_runs(one_item(&style, blocks), &mut fonts);
        assert_eq!(y_of(&drawn, "1.").to_bits(), m.baseline.as_pt().to_bits());
        assert_eq!(y_of(&drawn, "A").to_bits(), m.baseline.as_pt().to_bits());
    }
}

#[test]
fn marker_matches_the_table_first_row_with_padding() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let inset = 5.0;
    let drawn = page_runs(
        one_item(&style, vec![text_table(&style, 4.0, 2.0, false)]),
        &mut fonts,
    );
    let base = (inset + m.baseline.as_pt()).to_bits();
    assert_eq!(y_of(&drawn, "1.").to_bits(), base);
    assert_eq!(y_of(&drawn, "B").to_bits(), base);
}

#[test]
fn marker_matches_the_table_header_row() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let inset = 5.0;
    let drawn = page_runs(
        one_item(&style, vec![text_table(&style, 4.0, 2.0, true)]),
        &mut fonts,
    );
    let header = (inset + m.baseline.as_pt()).to_bits();
    let body = (m.line_height.as_pt() + 3.0 * inset + m.baseline.as_pt()).to_bits();
    assert_eq!(y_of(&drawn, "1.").to_bits(), header);
    assert_eq!(y_of(&drawn, "H").to_bits(), header);
    assert_eq!(y_of(&drawn, "B").to_bits(), body);
}

#[test]
fn marker_ignores_text_that_starts_on_a_later_table_row() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let mut spec = TableSpec::new(vec![
        ColumnWidth::Fixed(Positive::pt(40.0).unwrap()),
        ColumnWidth::Fixed(Positive::pt(40.0).unwrap()),
    ]);
    spec.body = vec![
        vec![
            Cell::new(vec![image_node()])
                .unwrap()
                .with_span(NonZeroU16::MIN, NonZeroU16::new(2).unwrap()),
            Cell::new(vec![spacer(20.0)]).unwrap(),
        ],
        vec![Cell::new(vec![text("B", &style)]).unwrap()],
    ];
    let drawn = page_runs(
        one_item(&style, vec![FlowNode::Table(TableNode::new(spec).unwrap())]),
        &mut fonts,
    );
    assert_eq!(y_of(&drawn, "1.").to_bits(), m.baseline.as_pt().to_bits());
    assert_eq!(
        y_of(&drawn, "B").to_bits(),
        (20.0 + m.baseline.as_pt()).to_bits()
    );
}

#[test]
fn marker_matches_the_higher_column_baseline() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let large = sized(&style, 24.0);
    let small = sized(&style, 18.0);
    let marker = metrics(&fonts, &style).baseline.as_pt();
    let high = metrics(&fonts, &small).baseline.as_pt();
    let low = metrics(&fonts, &large).baseline.as_pt();
    assert_ne!(high.to_bits(), marker.to_bits());
    assert_ne!(high.to_bits(), low.to_bits());
    let drawn = page_runs(
        one_item(
            &style,
            vec![two_columns(
                vec![text("L", &large)],
                vec![text("S", &small)],
            )],
        ),
        &mut fonts,
    );
    assert_eq!(y_of(&drawn, "1.").to_bits(), high.to_bits());
    assert_eq!(y_of(&drawn, "S").to_bits(), high.to_bits());
    assert_eq!(y_of(&drawn, "L").to_bits(), low.to_bits());
}

#[test]
fn outer_marker_matches_the_nested_list_marker() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let inner = ListNode::new(spec(
        ordered(MarkerKind::Decimal, 1),
        16.0,
        &style,
        vec![ListItem::new(vec![image_node(), text("in", &style)]).unwrap()],
    ))
    .unwrap();
    let drawn = page_runs(one_item(&style, vec![FlowNode::List(inner)]), &mut fonts);
    let base = m.baseline.as_pt().to_bits();
    assert_eq!(
        drawn.iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 0.0f32.to_bits(), base),
            ("1.", 20.0f32.to_bits(), base),
            (
                "in",
                36.0f32.to_bits(),
                (10.0 + m.baseline.as_pt()).to_bits()
            ),
        ]
    );
}

#[test]
fn marker_matches_anchor_and_keep_together_paragraphs() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let cases = [
        FlowNode::Anchor(AnchorNode {
            id: "a".into(),
            children: vec![text("A", &style)],
            landed: false,
        }),
        FlowNode::KeepTogether(KeepTogetherNode::Open(vec![text("A", &style)])),
    ];
    for block in cases {
        let drawn = page_runs(one_item(&style, vec![block]), &mut fonts);
        assert_eq!(y_of(&drawn, "1.").to_bits(), m.baseline.as_pt().to_bits());
        assert_eq!(y_of(&drawn, "A").to_bits(), m.baseline.as_pt().to_bits());
    }
}

#[test]
fn marker_follows_a_larger_paragraph_baseline() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let big = sized(&style, 24.0);
    let marker = metrics(&fonts, &style).baseline.as_pt();
    let line = metrics(&fonts, &big).baseline.as_pt();
    assert_ne!(marker.to_bits(), line.to_bits());
    let drawn = page_runs(one_item(&style, vec![text("A", &big)]), &mut fonts);
    assert_eq!(y_of(&drawn, "1.").to_bits(), line.to_bits());
    assert_eq!(y_of(&drawn, "A").to_bits(), line.to_bits());
}

#[test]
fn paragraph_blank_keeps_its_metric_baseline_without_glyphs() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let mut fontless = style.clone();
    fontless.font = None;
    let fragment = fit_fragment(
        FlowNode::Text(TextNode::from_items(
            vec![InlineItem::Blank(InlineBlank::new(BlankWidth::Fixed(
                NonNegative::pt(12.0).unwrap(),
            )))],
            fontless,
        )),
        &mut fonts,
    );
    assert_eq!(
        baseline_bits(&fragment),
        style.font_size.as_pt().to_bits(),
        "a blank line stores the fontless baseline"
    );
    assert!(
        !fragment
            .commands
            .iter()
            .any(|command| matches!(command, FragmentCommand::Glyphs(_))),
        "the blank line has no glyph run to scan"
    );
    let text = fit_fragment(text("A\nB", &style), &mut fonts);
    assert_eq!(baseline_bits(&text), m.baseline.as_pt().to_bits());
    assert!(has_glyphs(&text, "A"), "the paragraph draws A");
}

#[test]
fn with_offset_shifts_first_baseline() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let fragment = fit_fragment(text("A", &style), &mut fonts);
    let shifted = fragment
        .with_offset(Finite::pt(3.0).unwrap(), Finite::pt(7.0).unwrap())
        .unwrap();
    assert_eq!(
        shifted.first_baseline.unwrap().as_pt().to_bits(),
        (m.baseline.as_pt() + 7.0).to_bits()
    );
    let image = fit_fragment(image_node(), &mut fonts);
    assert!(image.first_baseline.is_none());
    assert!(image
        .with_offset(Finite::ZERO, Finite::pt(4.0).unwrap())
        .unwrap()
        .first_baseline
        .is_none());
}

#[test]
fn stack_stops_at_the_first_effective_child() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let image_then_text = fit_fragment(
        FlowNode::Stack(vec![image_node(), text("A", &style)]),
        &mut fonts,
    );
    assert!(image_then_text.first_baseline.is_none());
    assert!(has_glyphs(&image_then_text, "A"));
    let spacer_then_text = fit_fragment(
        FlowNode::Stack(vec![spacer(10.0), text("A", &style)]),
        &mut fonts,
    );
    assert!(spacer_then_text.first_baseline.is_none());
    let skipped = fit_fragment(
        FlowNode::Stack(vec![spacer(0.0), text("A", &style)]),
        &mut fonts,
    );
    assert_eq!(baseline_bits(&skipped), m.baseline.as_pt().to_bits());
}

#[test]
fn columns_table_anchor_and_list_report_first_baseline() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let small = sized(&style, 18.0);
    let large = sized(&style, 24.0);
    let columns = fit_fragment(
        two_columns(vec![text("L", &large)], vec![text("S", &small)]),
        &mut fonts,
    );
    assert_eq!(
        baseline_bits(&columns),
        metrics(&fonts, &small).baseline.as_pt().to_bits()
    );

    let padded = fit_fragment(text_table(&style, 4.0, 2.0, false), &mut fonts);
    assert_eq!(baseline_bits(&padded), (5.0 + m.baseline.as_pt()).to_bits());
    let headed = fit_fragment(text_table(&style, 4.0, 2.0, true), &mut fonts);
    assert_eq!(baseline_bits(&headed), (5.0 + m.baseline.as_pt()).to_bits());

    let mut image_row = TableSpec::new(vec![ColumnWidth::Fixed(Positive::pt(40.0).unwrap())]);
    image_row.body = vec![
        vec![Cell::new(vec![image_node()]).unwrap()],
        vec![Cell::new(vec![text("B", &style)]).unwrap()],
    ];
    let image_first = fit_fragment(
        FlowNode::Table(TableNode::new(image_row).unwrap()),
        &mut fonts,
    );
    assert!(image_first.first_baseline.is_none());
    assert!(has_glyphs(&image_first, "B"));

    let mut spanned = TableSpec::new(vec![
        ColumnWidth::Fixed(Positive::pt(40.0).unwrap()),
        ColumnWidth::Fixed(Positive::pt(40.0).unwrap()),
    ]);
    spanned.body = vec![
        vec![
            Cell::new(vec![image_node()])
                .unwrap()
                .with_span(NonZeroU16::MIN, NonZeroU16::new(2).unwrap()),
            Cell::new(vec![spacer(20.0)]).unwrap(),
        ],
        vec![Cell::new(vec![text("B", &style)]).unwrap()],
    ];
    let span = fit_fragment(
        FlowNode::Table(TableNode::new(spanned).unwrap()),
        &mut fonts,
    );
    assert!(
        span.first_baseline.is_none(),
        "a later row does not supply the table baseline"
    );
    assert!(has_glyphs(&span, "B"), "the later row is still drawn");

    let anchor = fit_fragment(
        FlowNode::Anchor(AnchorNode {
            id: "a".into(),
            children: vec![text("A", &style)],
            landed: false,
        }),
        &mut fonts,
    );
    assert_eq!(baseline_bits(&anchor), m.baseline.as_pt().to_bits());
    let kept = fit_fragment(
        FlowNode::KeepTogether(KeepTogetherNode::Open(vec![text("A", &style)])),
        &mut fonts,
    );
    assert_eq!(baseline_bits(&kept), m.baseline.as_pt().to_bits());

    for node in [image_node(), canvas_text(&style), rule(), spacer(10.0)] {
        assert!(fit_fragment(node, &mut fonts).first_baseline.is_none());
    }
    assert!(has_glyphs(
        &fit_fragment(canvas_text(&style), &mut fonts),
        "C"
    ));

    let listed = fit_fragment(one_item(&style, vec![text("A", &style)]), &mut fonts);
    assert_eq!(baseline_bits(&listed), m.baseline.as_pt().to_bits());
    let topped = fit_fragment(
        one_item(&style, vec![image_node(), text("A", &style)]),
        &mut fonts,
    );
    assert_eq!(baseline_bits(&topped), m.baseline.as_pt().to_bits());
}

#[test]
fn continuation_fragment_uses_the_body_baseline() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let line = m.line_height.as_pt();
    let Split::Partial { head, tail, .. } = split_frame(
        one_item(&style, vec![text("A\nB", &style)]),
        &mut fonts,
        240.0,
        line,
        400.0,
    ) else {
        panic!("the item splits after the first line");
    };
    assert_eq!(baseline_bits(&head), m.baseline.as_pt().to_bits());
    assert!(has_glyphs(&head, "1."));
    let Split::Fit { fragment, .. } = split_frame(tail, &mut fonts, 240.0, 400.0, 400.0) else {
        panic!("the continuation fits");
    };
    assert_eq!(baseline_bits(&fragment), m.baseline.as_pt().to_bits());
    assert!(!has_glyphs(&fragment, "1."));
    assert!(has_glyphs(&fragment, "B"));
}

#[test]
fn repeated_header_keeps_the_header_baseline() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let m = metrics(&fonts, &style);
    let line = m.line_height.as_pt();
    let mut spec = TableSpec::new(vec![ColumnWidth::Fixed(Positive::pt(80.0).unwrap())]);
    spec.header = vec![vec![Cell::new(vec![text("H", &style)]).unwrap()]];
    spec.body = vec![
        vec![Cell::new(vec![text("B", &style)]).unwrap()],
        vec![Cell::new(vec![text("C", &style)]).unwrap()],
    ];
    let Split::Partial { head, tail, .. } = split_frame(
        FlowNode::Table(TableNode::new(spec).unwrap()),
        &mut fonts,
        200.0,
        line * 2.0 + 1.0,
        400.0,
    ) else {
        panic!("the table splits after the header and the first body row");
    };
    assert_eq!(baseline_bits(&head), m.baseline.as_pt().to_bits());
    let Split::Fit { fragment, .. } = split_frame(tail, &mut fonts, 200.0, line * 2.0 + 1.0, 400.0)
    else {
        panic!("the repeated header and the last row fit");
    };
    assert_eq!(baseline_bits(&fragment), m.baseline.as_pt().to_bits());
    assert!(has_glyphs(&fragment, "H"));
    assert!(has_glyphs(&fragment, "C"));
}
