//! Header, footer, and layer tests.
//!
//! These tests call `paginate` through `test_util`, so they live outside `decorate`.

use crate::canvas::CanvasCommand;
use crate::error::{Error, ErrorBag, ErrorKind, Region};
use crate::geom::{Color, Ink, NonNegative, Path, Pen, Point, Rect};
use crate::layout::column_width::{ColumnWidth, Ratio};
use crate::layout::columns::ColumnsNode;
use crate::layout::decorate::{
    BandContent, Decorations, LayerContent, LayerItem, LayerText, PageInfo,
};
use crate::layout::flow::FlowNode;
use crate::layout::style::{Style, TextAlign};
use crate::layout::table::{TableNode, TableSpec};
use crate::layout::test_util::{
    bits, ellipse_center_radii, fills, fonts_and_style, margins, metrics, page, render, runs, text,
    texts,
};

fn band(label: &'static str, style: &Style) -> BandContent<'static> {
    let style = style.clone();
    Box::new(move |_: &PageInfo, _: &mut ErrorBag| Ok(vec![text(label, &style)]))
}

fn numbered(style: &Style) -> BandContent<'static> {
    let style = style.clone();
    Box::new(move |page: &PageInfo, _: &mut ErrorBag| {
        let label = format!("Page {} / {}", page.number.get(), page.count.get());
        Ok(vec![text(&label, &style)])
    })
}

fn three_pages_with_blank(style: &Style) -> FlowNode {
    FlowNode::Stack(vec![
        text("A", style),
        FlowNode::PageBreak,
        FlowNode::PageBreak,
        text("B", style),
    ])
}

#[test]
fn bands_span_the_page_and_number_every_page_including_blank_ones() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let b = metrics(&fonts, &style).baseline.as_pt();
    let decorations = Decorations {
        header: Some(numbered(&style)),
        footer: Some(band("F", &style)),
        ..Decorations::none()
    };
    let root = three_pages_with_blank(&style);
    let (result, rec) = render(root, &mut fonts, margins(30.0, 20.0, 30.0), &decorations);
    result.unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 3);
    assert_eq!(
        runs(&pages[0]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("Page 1 / 3", 0.0f32.to_bits(), b.to_bits()),
            ("A", 20.0f32.to_bits(), (30.0 + b).to_bits()),
            ("F", 0.0f32.to_bits(), (370.0 + b).to_bits()),
        ]
    );
    assert_eq!(texts(&pages[1]), ["Page 2 / 3", "F"]);
    assert_eq!(texts(&pages[2]), ["Page 3 / 3", "B", "F"]);
}

#[test]
fn body_coordinates_are_identical_with_and_without_decorations() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let config = margins(30.0, 20.0, 30.0);
    let bare = render(
        three_pages_with_blank(&style),
        &mut fonts,
        config,
        &Decorations::none(),
    );
    let decorations = Decorations {
        header: Some(band("H", &style)),
        footer: Some(band("F", &style)),
        ..Decorations::none()
    };
    let dressed = render(
        three_pages_with_blank(&style),
        &mut fonts,
        config,
        &decorations,
    );
    bare.0.unwrap();
    dressed.0.unwrap();
    let body = |rec: &crate::canvas::RecordingCanvas| -> Vec<(String, u32, u32)> {
        rec.pages()
            .iter()
            .flat_map(runs)
            .filter(|r| r.0 != "H" && r.0 != "F")
            .map(|r| (r.0, r.1.to_bits(), r.2.to_bits()))
            .collect()
    };
    let b = metrics(&fonts, &style).baseline.as_pt();
    let expected = vec![
        ("A".to_owned(), 20.0f32.to_bits(), (30.0 + b).to_bits()),
        ("B".to_owned(), 20.0f32.to_bits(), (30.0 + b).to_bits()),
    ];
    assert_eq!(body(&bare.1), expected);
    assert_eq!(body(&dressed.1), expected);
}

#[test]
fn header_with_zero_margin_errors_before_any_page_starts() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let line = metrics(&fonts, &style).line_height.as_pt();
    let decorations = Decorations {
        header: Some(band("H", &style)),
        footer: None,
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        page(200.0, 400.0),
        &decorations,
    );
    assert_eq!(
        result,
        Err(Error::from(ErrorKind::DecorationTallerThanMargin {
            height: line,
            margin: 0.0
        })
        .in_region(Region::Header)
        .on_page_index(0)
        .in_section(0))
    );
    assert!(rec.commands().is_empty());
}

#[test]
fn header_taller_than_the_page_reports_its_full_height() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let line = metrics(&fonts, &style).line_height.as_pt();
    let body = text("A", &style);
    let lines = vec!["H"; 40].join("\n");
    let decorations = Decorations {
        header: Some(Box::new(move |_: &PageInfo, _: &mut ErrorBag| {
            Ok(vec![text(&lines, &style)])
        })),
        ..Decorations::none()
    };
    let (result, _) = render(body, &mut fonts, margins(30.0, 0.0, 0.0), &decorations);
    let ErrorKind::DecorationTallerThanMargin { height, margin } = result.unwrap_err().kind else {
        panic!("expected DecorationTallerThanMargin");
    };
    assert!(height > 400.0);
    assert!((height - line * 40.0).abs() < 1e-3, "{height}");
    assert_eq!(margin.to_bits(), 30.0f32.to_bits());
}

#[test]
fn footer_taller_than_its_band_errors() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let line = metrics(&fonts, &style).line_height.as_pt();
    let decorations = Decorations {
        header: None,
        footer: Some(band("A\nB", &style)),
        ..Decorations::none()
    };
    let config = margins(0.0, 0.0, line + 1.0);
    let (result, rec) = render(text("A", &style), &mut fonts, config, &decorations);
    assert_eq!(
        result,
        Err(Error::from(ErrorKind::DecorationTallerThanMargin {
            height: line * 2.0,
            margin: line + 1.0
        })
        .in_region(Region::Footer)
        .on_page_index(0)
        .in_section(0))
    );
    assert!(rec.commands().is_empty());
}

#[test]
fn header_wider_than_page_errors() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let decorations = Decorations {
        header: Some(band("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", &style)),
        footer: None,
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        margins(40.0, 0.0, 0.0),
        &decorations,
    );
    assert!(
        matches!(result.map_err(|error| error.kind), Err(ErrorKind::DecorationWiderThanFrame { avail, .. }) if avail.to_bits() == 200.0f32.to_bits())
    );
    assert!(rec.commands().is_empty());
}

#[test]
fn band_rejects_images() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let image = crate::layout::image::ImageNode::new(
        crate::canvas::DecodedImage {
            id: crate::canvas::ImageId::from_raw(0),
            pixel_width: 1,
            pixel_height: 1,
        },
        Some(crate::geom::Positive::pt(10.0).unwrap()),
        Some(crate::geom::Positive::pt(10.0).unwrap()),
        TextAlign::Start,
    )
    .unwrap();
    let decorations = Decorations {
        header: Some(Box::new(move |_: &PageInfo, _: &mut ErrorBag| {
            Ok(vec![FlowNode::Image(image.clone())])
        })),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        margins(40.0, 0.0, 0.0),
        &decorations,
    );
    assert_eq!(
        result,
        Err(Error::from(ErrorKind::InvalidCellContent {
            container: "header/footer band",
            node: "image"
        })
        .in_region(Region::Header)
        .on_page_index(0)
        .in_section(0))
    );
    assert!(rec.commands().is_empty());
}

#[test]
fn band_rejects_tables() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let decorations = Decorations {
        header: Some(Box::new(|_: &PageInfo, _: &mut ErrorBag| {
            Ok(vec![FlowNode::Table(TableNode::new(TableSpec::new(
                vec![ColumnWidth::Auto],
            ))?)])
        })),
        footer: None,
        ..Decorations::none()
    };
    let (result, _) = render(
        text("A", &style),
        &mut fonts,
        margins(40.0, 0.0, 0.0),
        &decorations,
    );
    assert_eq!(
        result,
        Err(Error::from(ErrorKind::InvalidCellContent {
            container: "header/footer band",
            node: "table"
        })
        .in_region(Region::Header)
        .on_page_index(0)
        .in_section(0))
    );
}

fn two_columns(left: Vec<FlowNode>, right: Vec<FlowNode>) -> FlowNode {
    let even = ColumnWidth::Ratio(Ratio::new(1.0).unwrap());
    FlowNode::Columns(
        ColumnsNode::new(NonNegative::ZERO, vec![(even, left), (even, right)]).unwrap(),
    )
}

#[test]
fn band_columns_share_one_line() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let b = metrics(&fonts, &style).baseline.as_pt();
    let header_style = style.clone();
    let decorations = Decorations {
        header: Some(Box::new(move |_: &PageInfo, _: &mut ErrorBag| {
            Ok(vec![two_columns(
                vec![text("L", &header_style)],
                vec![text("R", &header_style)],
            )])
        })),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        margins(40.0, 0.0, 0.0),
        &decorations,
    );
    result.unwrap();
    assert_eq!(
        runs(&rec.pages()[0]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("L", 0.0f32.to_bits(), b.to_bits()),
            ("R", 100.0f32.to_bits(), b.to_bits()),
            ("A", 0.0f32.to_bits(), (40.0 + b).to_bits()),
        ]
    );
}

#[test]
fn band_columns_still_reject_a_nested_table() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let header_style = style.clone();
    let decorations = Decorations {
        header: Some(Box::new(move |_: &PageInfo, _: &mut ErrorBag| {
            let table = TableNode::new(TableSpec::new(vec![ColumnWidth::Auto]))?;
            Ok(vec![two_columns(
                vec![text("L", &header_style)],
                vec![FlowNode::Table(table)],
            )])
        })),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        margins(40.0, 0.0, 0.0),
        &decorations,
    );
    assert_eq!(
        result,
        Err(Error::from(ErrorKind::InvalidCellContent {
            container: "header/footer band",
            node: "table"
        })
        .in_region(Region::Header)
        .on_page_index(0)
        .in_section(0))
    );
    assert!(rec.commands().is_empty());
}

fn layer(label: &'static str, x: f32, y: f32, style: &Style) -> LayerContent<'static> {
    let style = style.clone();
    Box::new(move |_: &PageInfo, _: &mut ErrorBag| {
        Ok(vec![LayerItem::Text(LayerText {
            text: label.to_owned(),
            at: Point::try_pt(x, y).unwrap(),
            style: style.clone(),
        })])
    })
}

#[test]
fn layers_paint_behind_header_body_footer_front() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let b = metrics(&fonts, &style).baseline.as_pt();
    let decorations = Decorations {
        header: Some(band("H", &style)),
        footer: Some(band("F", &style)),
        behind: Some(layer("W", 50.0, 120.0, &style)),
        front: Some(layer("S", 150.0, 300.0, &style)),
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        margins(30.0, 20.0, 30.0),
        &decorations,
    );
    result.unwrap();
    assert_eq!(
        runs(&rec.pages()[0]).iter().map(bits).collect::<Vec<_>>(),
        [
            ("W", 50.0f32.to_bits(), (120.0 + b).to_bits()),
            ("H", 0.0f32.to_bits(), b.to_bits()),
            ("A", 20.0f32.to_bits(), (30.0 + b).to_bits()),
            ("F", 0.0f32.to_bits(), (370.0 + b).to_bits()),
            ("S", 150.0f32.to_bits(), (300.0 + b).to_bits()),
        ]
    );
}

#[test]
fn layer_ignores_alignment_and_page_bounds() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let b = metrics(&fonts, &style).baseline.as_pt();
    let centered = Style {
        align: TextAlign::Center,
        ..style.clone()
    };
    let decorations = Decorations {
        front: Some(layer("S", 190.0, 395.0, &centered)),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        page(200.0, 400.0),
        &decorations,
    );
    result.unwrap();
    assert_eq!(
        runs(&rec.pages()[0])[1..]
            .iter()
            .map(bits)
            .collect::<Vec<_>>(),
        [("S", 190.0f32.to_bits(), (395.0 + b).to_bits())]
    );
}

#[test]
fn missing_glyph_in_a_layer_fails_before_any_page_starts() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let decorations = Decorations {
        behind: Some(layer("\u{FFFF}", 0.0, 0.0, &style)),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        page(200.0, 400.0),
        &decorations,
    );
    assert!(matches!(
        result.map_err(|error| error.kind),
        Err(ErrorKind::MissingGlyph {
            scalar: '\u{FFFF}',
            ..
        })
    ));
    assert!(rec.commands().is_empty());
}

#[test]
fn path_behind_paints_before_header_glyphs() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let path = Path::line(
        Point::try_pt(0.0, 0.0).unwrap(),
        Point::try_pt(40.0, 10.0).unwrap(),
    );
    let decorations = Decorations {
        header: Some(band("H", &style)),
        behind: Some(Box::new(move |_: &PageInfo, _: &mut ErrorBag| {
            Ok(vec![LayerItem::Shape {
                path: path.clone(),
                ink: Ink::Stroke(Pen::new(Color::rgb(1, 2, 3), 1.5).unwrap()),
            }])
        })),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        margins(30.0, 20.0, 30.0),
        &decorations,
    );
    result.unwrap();
    let pages = rec.pages();
    let ops = pages[0].ops();
    assert!(matches!(ops[0], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[1], CanvasCommand::DrawGlyphRun { .. }));
    assert_eq!(texts(&pages[0])[0], "H");
}

#[test]
fn fill_stroke_paints_fill_then_stroke() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let rect = Rect::try_pt(10.0, 20.0, 40.0, 20.0).unwrap();
    let path = Path::ellipse(rect).unwrap();
    let decorations = Decorations {
        behind: Some(Box::new(move |_: &PageInfo, _: &mut ErrorBag| {
            Ok(vec![LayerItem::Shape {
                path: path.clone(),
                ink: Ink::FillStroke(
                    Color::rgba(9, 8, 7, 40),
                    Pen::new(Color::rgb(1, 2, 3), 2.0).unwrap(),
                ),
            }])
        })),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        page(200.0, 400.0),
        &decorations,
    );
    result.unwrap();
    let pages = rec.pages();
    let ops = pages[0].ops();
    let CanvasCommand::FillPath {
        path: filled,
        paint,
    } = &ops[0]
    else {
        panic!("expected fill first, got {:?}", ops[0]);
    };
    let CanvasCommand::StrokePath {
        path: stroked,
        paint: stroke_paint,
        stroke,
    } = &ops[1]
    else {
        panic!("expected stroke second, got {:?}", ops[1]);
    };
    assert_eq!(filled, stroked);
    assert_eq!(paint.color(), Color::rgba(9, 8, 7, 40));
    assert_eq!(stroke_paint.color(), Color::rgb(1, 2, 3));
    assert_eq!(stroke.width().as_pt().to_bits(), 2.0f32.to_bits());
    assert_eq!(ellipse_center_radii(filled), Some((30.0, 30.0, 20.0, 10.0)));
    assert_eq!(fills(&pages[0]).len(), 1);
}

#[test]
fn band_error_returns_before_any_page_starts() {
    let Some((mut fonts, style)) = fonts_and_style() else {
        return;
    };
    let decorations = Decorations {
        header: Some(Box::new(|_: &PageInfo, _: &mut ErrorBag| {
            Err(Error::from(ErrorKind::EmptyLink))
        })),
        ..Decorations::none()
    };
    let (result, rec) = render(
        text("A", &style),
        &mut fonts,
        page(200.0, 400.0),
        &decorations,
    );
    assert_eq!(
        result,
        Err(Error::from(ErrorKind::EmptyLink)
            .in_region(Region::Header)
            .on_page_index(0)
            .in_section(0))
    );
    assert!(rec.commands().is_empty());
}
