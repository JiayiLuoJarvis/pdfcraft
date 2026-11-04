use std::num::NonZeroU32;

use crate::canvas::{DecodedImage, ImageId};
use crate::error::{Error, ErrorBag, ErrorKind, GeometryKind, LimitKind};
use crate::fonts::FontStore;
use crate::geom::{NonNegative, Positive};
use crate::layout::column_width::ColumnWidth;
use crate::layout::columns::ColumnsNode;
use crate::layout::decorate::Decorations;
use crate::layout::flow::{check_limits, AnchorNode, FlowNode, SpacerNode};
use crate::layout::frame::{Frame, LayoutCtx, Split};
use crate::layout::image::ImageNode;
use crate::layout::inset::InsetNode;
use crate::layout::list::{
    ListItem, ListKind, ListNode, ListSpec, MarkerAffix, MarkerAlign, MarkerKind,
};
use crate::layout::style::{GlyphStyle, Style, TextAlign};
use crate::layout::table::{Cell, TableNode, TableSpec};
use crate::layout::test_util::{bits, fonts_and_style, metrics, page, render, runs, text};
use crate::limits::Limits;

fn inset(left: f32, right: f32, child: FlowNode) -> FlowNode {
    FlowNode::Inset(node(left, right, child))
}

fn node(left: f32, right: f32, child: FlowNode) -> InsetNode<FlowNode> {
    InsetNode {
        left: NonNegative::pt(left).unwrap(),
        right: NonNegative::pt(right).unwrap(),
        child: Box::new(child),
    }
}

fn frame(width: f32) -> Frame {
    Frame {
        width: Positive::pt(width).unwrap(),
        height: NonNegative::pt(400.0).unwrap(),
        full_height: Positive::pt(400.0).unwrap(),
    }
}

fn split_inset(
    built: InsetNode<FlowNode>,
    width: f32,
) -> Result<Split<InsetNode<FlowNode>>, Error> {
    let mut fonts = FontStore::new();
    let mut errors = ErrorBag::default();
    let mut ctx = LayoutCtx {
        fonts: &mut fonts,
        errors: &mut errors,
        natural: false,
    };
    built.split(&mut ctx, frame(width))
}

fn pixels(width: u32, height: u32) -> DecodedImage {
    DecodedImage {
        id: ImageId::from_raw(0),
        pixel_width: width,
        pixel_height: height,
    }
}

fn draw(node: FlowNode, fonts: &mut FontStore, width: f32, height: f32) -> Vec<(String, f32, f32)> {
    let (result, rec) = render(node, fonts, page(width, height), &Decorations::none());
    result.unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 1, "the sample fits on one page");
    runs(&pages[0])
}

fn ordered_list(style: &Style, blocks: Vec<FlowNode>) -> ListNode<FlowNode> {
    ListNode::new(ListSpec {
        kind: ListKind::Ordered {
            style: MarkerKind::Decimal,
            start: NonZeroU32::new(1).unwrap(),
        },
        marker_width: Positive::pt(20.0).unwrap(),
        body_indent: None,
        marker_style: GlyphStyle::plain(style),
        item_gap: NonNegative::ZERO,
        affix: MarkerAffix::DotSuffix,
        align: MarkerAlign::Start,
        items: vec![ListItem::new(blocks).unwrap()],
    })
    .unwrap()
}

fn ordered_item(style: &Style, blocks: Vec<FlowNode>) -> FlowNode {
    FlowNode::List(ordered_list(style, blocks))
}

#[test]
fn inset_rejects_frame_too_narrow_even_when_empty() {
    let too_narrow = split_inset(node(30.0, 20.0, FlowNode::Stack(Vec::new())), 50.0).unwrap_err();
    assert_eq!(
        too_narrow.kind,
        ErrorKind::InvalidGeometry(GeometryKind::InsetWiderThanFrame {
            left: 30.0,
            right: 20.0,
            avail: 50.0,
        })
    );
    assert_eq!(
        too_narrow.to_string(),
        "invalid geometry: inset left 30 plus right 20 leaves no room in width 50"
    );

    let exact = split_inset(node(25.0, 25.0, FlowNode::Stack(Vec::new())), 50.0).unwrap_err();
    assert_eq!(
        exact.kind,
        ErrorKind::InvalidGeometry(GeometryKind::InsetWiderThanFrame {
            left: 25.0,
            right: 25.0,
            avail: 50.0,
        })
    );

    let Split::Fit { height, fragment } =
        split_inset(node(10.0, 10.0, FlowNode::Stack(Vec::new())), 100.0).unwrap()
    else {
        panic!("a positive remainder lays out the empty child");
    };
    assert_eq!(height, NonNegative::ZERO);
    assert!(fragment.commands.is_empty(), "an inset paints nothing");
    assert_eq!(fragment.first_baseline, None);
}

#[test]
fn inset_reports_the_image_when_the_narrowed_frame_is_still_too_narrow() {
    let image = ImageNode::new(
        pixels(10, 10),
        Some(Positive::pt(30.0).unwrap()),
        Some(Positive::pt(10.0).unwrap()),
        TextAlign::Start,
    )
    .unwrap();
    let err = split_inset(node(40.0, 40.0, FlowNode::Image(image)), 100.0).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::UnbreakableWiderThanFrame {
            text: None,
            width: 30.0,
            avail: 20.0,
        }
    );
}

#[test]
fn inset_shifts_lines_by_left_and_breaks_at_the_narrowed_width() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let words = "MM ".repeat(24);
    let bare_narrow = draw(text(words.trim_end(), &style), &mut fonts, 80.0, 800.0);
    let bare_wide = draw(text(words.trim_end(), &style), &mut fonts, 130.0, 800.0);
    let shifted = draw(
        inset(20.0, 30.0, text(words.trim_end(), &style)),
        &mut fonts,
        130.0,
        800.0,
    );
    assert!(
        bare_narrow.len() > bare_wide.len(),
        "80 pt must wrap more than 130 pt, narrow {} wide {}",
        bare_narrow.len(),
        bare_wide.len()
    );
    assert_eq!(bare_narrow.len(), shifted.len());
    for (plain, moved) in bare_narrow.iter().zip(&shifted) {
        assert_eq!(moved.0, plain.0);
        assert_eq!(moved.1.to_bits(), (plain.1 + 20.0).to_bits(), "{}", plain.0);
        assert_eq!(moved.2.to_bits(), plain.2.to_bits(), "{}", plain.0);
    }
}

#[test]
fn inset_keeps_the_shift_on_the_continuation_page() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let metrics = metrics(&fonts, &style);
    let height = metrics.line_height.as_pt() * 2.0 + 1.0;
    let (result, rec) = render(
        inset(20.0, 30.0, text("A\nB\nC", &style)),
        &mut fonts,
        page(200.0, height),
        &Decorations::none(),
    );
    result.unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2, "three lines on a two-line page");
    for run in runs(&pages[0]).iter().chain(runs(&pages[1]).iter()) {
        assert_eq!(run.1.to_bits(), 20.0f32.to_bits(), "{}", run.0);
    }
    assert_eq!(runs(&pages[1])[0].0, "C");
}

#[test]
fn zero_inset_matches_the_unwrapped_paragraph() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let bare = draw(text("A\nB", &style), &mut fonts, 200.0, 400.0);
    let wrapped = draw(
        inset(0.0, 0.0, text("A\nB", &style)),
        &mut fonts,
        200.0,
        400.0,
    );
    assert_eq!(
        bare.iter().map(bits).collect::<Vec<_>>(),
        wrapped.iter().map(bits).collect::<Vec<_>>()
    );
}

#[test]
fn inset_does_not_add_vertical_space_or_shift_the_next_block() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let bare = draw(
        FlowNode::Stack(vec![text("A", &style), text("B", &style)]),
        &mut fonts,
        200.0,
        400.0,
    );
    let wrapped = draw(
        FlowNode::Stack(vec![inset(20.0, 0.0, text("A", &style)), text("B", &style)]),
        &mut fonts,
        200.0,
        400.0,
    );
    assert_eq!(wrapped[0].0, "A");
    assert_eq!(wrapped[0].1.to_bits(), (bare[0].1 + 20.0).to_bits());
    assert_eq!(wrapped[0].2.to_bits(), bare[0].2.to_bits());
    assert_eq!(bits(&wrapped[1]), bits(&bare[1]));
}

#[test]
fn list_address_shifts_and_leaves_the_marker_on_the_first_paragraph() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let metrics = metrics(&fonts, &style);
    let drawn = draw(
        ordered_item(
            &style,
            vec![
                text("Ship", &style),
                inset(20.0, 0.0, text("Street", &style)),
            ],
        ),
        &mut fonts,
        280.0,
        400.0,
    );
    assert_eq!(
        drawn.iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 0.0f32.to_bits(), metrics.baseline.as_pt().to_bits()),
            (
                "Ship",
                20.0f32.to_bits(),
                metrics.baseline.as_pt().to_bits()
            ),
            (
                "Street",
                40.0f32.to_bits(),
                (metrics.line_height.as_pt() + metrics.baseline.as_pt()).to_bits()
            ),
        ]
    );
}

#[test]
fn inset_around_a_list_shifts_the_marker_and_the_body() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let metrics = metrics(&fonts, &style);
    let drawn = draw(
        inset(
            12.0,
            0.0,
            ordered_item(&style, vec![text("Subsidiary", &style)]),
        ),
        &mut fonts,
        280.0,
        400.0,
    );
    assert_eq!(
        drawn.iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 12.0f32.to_bits(), metrics.baseline.as_pt().to_bits()),
            (
                "Subsidiary",
                32.0f32.to_bits(),
                metrics.baseline.as_pt().to_bits()
            ),
        ]
    );
}

#[test]
fn inset_forwards_first_baseline_unchanged() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let metrics = metrics(&fonts, &style);
    let drawn = draw(
        ordered_item(&style, vec![inset(20.0, 0.0, text("Line", &style))]),
        &mut fonts,
        280.0,
        400.0,
    );
    assert_eq!(
        drawn.iter().map(bits).collect::<Vec<_>>(),
        [
            ("1.", 0.0f32.to_bits(), metrics.baseline.as_pt().to_bits()),
            (
                "Line",
                40.0f32.to_bits(),
                metrics.baseline.as_pt().to_bits()
            ),
        ]
    );
}

#[test]
fn cell_inset_adds_left_after_padding() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let metrics = metrics(&fonts, &style);
    let mut spec = TableSpec::new(vec![ColumnWidth::Auto]);
    spec.padding = NonNegative::pt(6.0).unwrap();
    spec.body = vec![vec![
        Cell::new(vec![inset(8.0, 0.0, text("Net", &style))]).unwrap()
    ]];
    let drawn = draw(
        FlowNode::Table(TableNode::new(spec).unwrap()),
        &mut fonts,
        200.0,
        200.0,
    );
    assert_eq!(
        drawn.iter().map(bits).collect::<Vec<_>>(),
        [(
            "Net",
            14.0f32.to_bits(),
            (6.0 + metrics.baseline.as_pt()).to_bits()
        )]
    );
}

#[test]
fn inset_keeps_the_container_that_forbids_the_child() {
    let break_in_column = ColumnsNode::new(
        NonNegative::ZERO,
        vec![
            (
                ColumnWidth::Auto,
                vec![inset(0.0, 0.0, FlowNode::PageBreak)],
            ),
            (
                ColumnWidth::Fixed(Positive::pt(10.0).unwrap()),
                vec![FlowNode::Spacer(SpacerNode {
                    height: NonNegative::pt(1.0).unwrap(),
                })],
            ),
        ],
    )
    .unwrap_err();
    assert_eq!(
        break_in_column,
        Error::from(ErrorKind::InvalidCellContent {
            container: "column",
            node: "page break",
        })
    );

    let break_in_item = ListItem::new(vec![inset(0.0, 0.0, FlowNode::PageBreak)]).unwrap_err();
    assert_eq!(
        break_in_item,
        Error::from(ErrorKind::InvalidCellContent {
            container: "list item",
            node: "page break",
        })
    );

    let mut table = TableSpec::new(vec![ColumnWidth::Auto]);
    table.body = vec![vec![Cell::new(vec![FlowNode::Spacer(SpacerNode {
        height: NonNegative::pt(4.0).unwrap(),
    })])
    .unwrap()]];
    ColumnsNode::new(
        NonNegative::ZERO,
        vec![
            (
                ColumnWidth::Auto,
                vec![inset(
                    0.0,
                    0.0,
                    FlowNode::Table(TableNode::new(table).unwrap()),
                )],
            ),
            (
                ColumnWidth::Fixed(Positive::pt(20.0).unwrap()),
                vec![FlowNode::Spacer(SpacerNode {
                    height: NonNegative::pt(1.0).unwrap(),
                })],
            ),
        ],
    )
    .expect("a column may hold an inset around a table");

    let columns = ColumnsNode::new(
        NonNegative::ZERO,
        vec![
            (
                ColumnWidth::Auto,
                vec![FlowNode::Spacer(SpacerNode {
                    height: NonNegative::pt(1.0).unwrap(),
                })],
            ),
            (
                ColumnWidth::Fixed(Positive::pt(10.0).unwrap()),
                vec![FlowNode::Spacer(SpacerNode {
                    height: NonNegative::pt(1.0).unwrap(),
                })],
            ),
        ],
    )
    .unwrap();
    let Some((_fonts, style)) = fonts_and_style() else {
        return;
    };
    let list = ordered_list(&style, vec![FlowNode::Columns(columns)]);
    let denied = Cell::new(vec![inset(0.0, 0.0, FlowNode::List(list))]).unwrap_err();
    assert_eq!(
        denied,
        Error::from(ErrorKind::InvalidCellContent {
            container: "table cell",
            node: "columns",
        })
    );
}

#[test]
fn inset_page_break_in_body_is_nested_page_break() {
    let mut fonts = FontStore::new();
    let (result, _) = render(
        inset(0.0, 0.0, FlowNode::PageBreak),
        &mut fonts,
        page(200.0, 400.0),
        &Decorations::none(),
    );
    assert_eq!(
        result.unwrap_err().kind,
        ErrorKind::InvalidGeometry(GeometryKind::NestedPageBreak)
    );
}

#[test]
fn inset_shifts_anchor_x() {
    let anchor = FlowNode::Anchor(AnchorNode {
        id: "here".into(),
        children: vec![FlowNode::Spacer(SpacerNode {
            height: NonNegative::pt(8.0).unwrap(),
        })],
        landed: false,
    });
    let Split::Fit { fragment, .. } = split_inset(node(15.0, 0.0, anchor), 100.0).unwrap() else {
        panic!("the spacer fits");
    };
    assert_eq!(fragment.anchors.len(), 1);
    assert_eq!(fragment.anchors[0].id, "here");
    assert_eq!(
        fragment.anchors[0].relative.x().as_pt().to_bits(),
        15.0f32.to_bits()
    );
    assert_eq!(
        fragment.anchors[0].relative.y().as_pt().to_bits(),
        0.0f32.to_bits()
    );
    assert_eq!(fragment.first_baseline, None);
}

#[test]
fn census_counts_inset_as_one_block() {
    let spacer = FlowNode::Spacer(SpacerNode {
        height: NonNegative::pt(1.0).unwrap(),
    });
    let mut limits = Limits::default();
    limits.set_max_blocks(1).unwrap();
    check_limits(std::slice::from_ref(&spacer), &limits).unwrap();

    let empty = inset(0.0, 0.0, FlowNode::Stack(Vec::new()));
    check_limits(std::slice::from_ref(&empty), &limits).unwrap();

    let wrapped = inset(0.0, 0.0, spacer);
    let err = check_limits(std::slice::from_ref(&wrapped), &limits).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::Blocks,
            value: 2,
            max: 1,
        }
    );
}
