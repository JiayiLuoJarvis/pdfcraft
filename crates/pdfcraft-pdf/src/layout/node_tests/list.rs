use std::num::NonZeroU32;

use crate::error::{Error, ErrorKind, GeometryKind};
use crate::geom::{Color, NonNegative, Positive};
use crate::layout::column_width::ColumnWidth;
use crate::layout::decorate::Decorations;
use crate::layout::flow::FlowNode;
use crate::layout::frame::{Fragment, FragmentCommand};
use crate::layout::list::{first_baseline, ListItem, ListKind, ListNode, ListSpec, MarkerKind};
use crate::layout::style::{GlyphStyle, Style};
use crate::layout::table::{TableNode, TableSpec};
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
        nest_indent: None,
        marker_style: GlyphStyle::plain(style),
        items,
    }
}

fn ordered(style: MarkerKind, start: u32) -> ListKind {
    ListKind::Ordered {
        style,
        start: NonZeroU32::new(start).unwrap(),
    }
}

fn item(s: &str, style: &Style) -> ListItem<FlowNode> {
    ListItem::new(vec![text(s, style)]).unwrap()
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
fn nest_indent_moves_the_body_column() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let mut s = spec(ListKind::Bullet, 14.0, &style, vec![item("A", &style)]);
    s.nest_indent = Some(Positive::pt(30.0).unwrap());
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
    let list = ListNode::new(spec(
        ordered(MarkerKind::UpperRoman, 4),
        30.0,
        &style,
        vec![
            item("a", &style),
            ListItem::custom("(x)", vec![text("b", &style)]).unwrap(),
            item("c", &style),
        ],
    ))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(texts(&rec.pages()[0]), ["IV.", "a", "(x)", "b", "V.", "c"]);
}

#[test]
fn decimal_custom_marker_between_autos_does_not_consume() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let list = ListNode::new(spec(
        ordered(MarkerKind::Decimal, 1),
        20.0,
        &style,
        vec![
            item("a", &style),
            ListItem::custom("(x)", vec![text("b", &style)]).unwrap(),
            item("c", &style),
        ],
    ))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(texts(&rec.pages()[0]), ["1.", "a", "(x)", "b", "2.", "c"]);
}

#[test]
fn leading_custom_marker_does_not_consume_the_first_number() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let list = ListNode::new(spec(
        ordered(MarkerKind::Decimal, 1),
        20.0,
        &style,
        vec![
            ListItem::custom("(x)", vec![text("a", &style)]).unwrap(),
            item("b", &style),
            item("c", &style),
        ],
    ))
    .unwrap();
    let (result, rec) = render(
        FlowNode::List(list),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    result.unwrap();
    assert_eq!(texts(&rec.pages()[0]), ["(x)", "a", "1.", "b", "2.", "c"]);
}

#[test]
fn labels_for_each_marker_kind() {
    assert_eq!(MarkerKind::Decimal.label(12).unwrap(), "12.");
    assert_eq!(MarkerKind::UpperAlpha.label(3).unwrap(), "C.");
    assert_eq!(MarkerKind::LowerAlpha.label(26).unwrap(), "z.");
    assert_eq!(MarkerKind::UpperRoman.label(1994).unwrap(), "MCMXCIV.");
    assert_eq!(MarkerKind::LowerRoman.label(3999).unwrap(), "mmmcmxcix.");
}

#[test]
fn labels_past_z_or_3999_are_errors() {
    assert_eq!(
        MarkerKind::UpperAlpha.label(27),
        Err(GeometryKind::MarkerOutOfRange { number: 27 }.into())
    );
    assert_eq!(
        MarkerKind::LowerRoman.label(4000),
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

#[test]
fn item_rejects_table_page_break_and_image() {
    let table = TableNode::new(TableSpec::new(vec![ColumnWidth::Auto])).unwrap();
    let image = FlowNode::Image(
        crate::layout::image::ImageNode::new(
            crate::canvas::DecodedImage {
                id: crate::canvas::ImageId::from_raw(0),
                pixel_width: 1,
                pixel_height: 1,
            },
            Some(Positive::pt(10.0).unwrap()),
            Some(Positive::pt(10.0).unwrap()),
            crate::layout::style::TextAlign::Start,
        )
        .unwrap(),
    );
    for (node, name) in [
        (FlowNode::Table(table), "table"),
        (FlowNode::PageBreak, "page break"),
        (image, "image"),
    ] {
        assert_eq!(
            ListItem::new(vec![node]).unwrap_err(),
            Error::from(ErrorKind::InvalidCellContent {
                container: "list item",
                node: name
            })
        );
    }
}

#[test]
fn item_allows_hline() {
    let line = crate::layout::hline::HLineNode::new(
        None,
        crate::layout::style::TextAlign::Center,
        Positive::pt(1.0).unwrap(),
        Color::rgb(0, 0, 0),
        NonNegative::ZERO,
        NonNegative::ZERO,
    );
    ListItem::new(vec![FlowNode::HLine(line)]).unwrap();
}
