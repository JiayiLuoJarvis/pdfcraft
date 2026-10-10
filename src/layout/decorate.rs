//! Header and footer drawing inside margin bands (does not shrink the body frame).

#![cfg_attr(not(test), allow(dead_code))]

use std::num::NonZeroU32;

use crate::error::{Error, Result};
use crate::geom::{Ink, Margins, Paint, Path, Point, Size, Stroke};
use crate::layout::flow::{
    FlowNode, Fragment, FragmentCommand, Frame, LayoutCtx, PlacedFragment, Split,
};
use crate::layout::lower::{check_content, Container};
use crate::layout::paragraph::single_line_run;
use crate::layout::style::{Style, TextAlign};

/// One-based page number across the whole document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageNumber(NonZeroU32);

impl PageNumber {
    pub(crate) fn get(self) -> u32 {
        self.0.get()
    }
}

/// Total pages in the document, blank body pages included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageCount(NonZeroU32);

impl PageCount {
    pub(crate) fn get(self) -> u32 {
        self.0.get()
    }
}

/// Inputs for a header or footer band closure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PageInfo {
    pub(crate) number: PageNumber,
    pub(crate) count: PageCount,
    pub(crate) section_number: PageNumber,
    pub(crate) section_count: PageCount,
    pub(crate) size: Size,
    pub(crate) margins: Margins,
}

pub(crate) type BandContent<'a> = Box<dyn Fn(&PageInfo) -> Result<Vec<FlowNode>> + 'a>;

/// Unwrapped line for behind/front. `at` is the line-box top-left; alignment is ignored.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayerText {
    pub(crate) text: String,
    pub(crate) at: Point,
    pub(crate) style: Style,
}

/// One behind or front primitive, painted in the order the closure added it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LayerItem {
    Text(LayerText),
    /// Page-coordinate rect. Not inset by margins and not checked against the page.
    Image {
        image: crate::canvas::DecodedImage,
        rect: crate::geom::Rect,
    },
    /// Final page-coordinate geometry. Ellipse arcs and rotation are already baked in.
    Shape {
        path: Path,
        ink: Ink,
    },
}

pub(crate) type LayerContent<'a> = Box<dyn Fn(&PageInfo) -> Result<Vec<LayerItem>> + 'a>;

#[derive(Default)]
pub(crate) struct Decorations<'a> {
    pub(crate) header: Option<BandContent<'a>>,
    pub(crate) footer: Option<BandContent<'a>>,
    pub(crate) behind: Option<LayerContent<'a>>,
    pub(crate) front: Option<LayerContent<'a>>,
}

impl Decorations<'_> {
    pub(crate) fn none() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PageBands {
    pub(crate) behind: Option<PlacedFragment>,
    pub(crate) header: Option<PlacedFragment>,
    pub(crate) footer: Option<PlacedFragment>,
    pub(crate) front: Option<PlacedFragment>,
}

/// Measures bands and layers before paint. Failure leaves the canvas untouched.
///
/// `spans` tile `sizes` in order. Each span supplies that section's margins,
/// decorations, and section page counters. `number` and `count` stay document-wide.
pub(crate) fn measure_bands(
    sizes: &[Size],
    spans: &[crate::layout::paginate::SectionSpan<'_>],
    ctx: &mut LayoutCtx<'_>,
) -> Result<Vec<PageBands>> {
    let count = page_count(sizes.len())?;
    let mut covered = 0usize;
    for span in spans {
        if span.start != covered {
            return Err(Error::InvalidGeometry);
        }
        covered = covered
            .checked_add(usize::try_from(span.len.get()).map_err(|_| Error::InvalidGeometry)?)
            .ok_or(Error::InvalidGeometry)?;
    }
    if covered != sizes.len() {
        return Err(Error::InvalidGeometry);
    }
    let mut bands = Vec::with_capacity(sizes.len());
    for span in spans {
        let section_count = PageCount(span.len);
        let margins = span.margins;
        let decorations = span.decorations;
        for offset in 0..span.len.get() {
            let index = span
                .start
                .checked_add(offset as usize)
                .ok_or(Error::InvalidGeometry)?;
            let size = sizes.get(index).copied().ok_or(Error::InvalidGeometry)?;
            let page = PageInfo {
                number: PageNumber(page_index(index)?),
                count,
                section_number: PageNumber(page_index(offset as usize)?),
                section_count,
                size,
                margins,
            };
            let top = margins.top().as_pt();
            let bottom = margins.bottom().as_pt();
            let page_h = size.height().as_pt();
            let mut band = |content: &Option<BandContent<'_>>, y: f32, margin: f32| -> Result<_> {
                let Some(content) = content else {
                    return Ok(None);
                };
                let fragment = measure_band(ctx, content(&page)?, size, margin)?;
                fragment
                    .map(|fragment| {
                        Ok(PlacedFragment {
                            origin: Point::try_pt(0.0, y)?,
                            fragment,
                        })
                    })
                    .transpose()
            };
            let header = band(&decorations.header, 0.0, top)?;
            let footer = band(&decorations.footer, page_h - bottom, bottom)?;
            bands.push(PageBands {
                behind: measure_layer(ctx, decorations.behind.as_ref(), &page)?,
                header,
                footer,
                front: measure_layer(ctx, decorations.front.as_ref(), &page)?,
            });
        }
    }
    Ok(bands)
}

fn page_count(len: usize) -> Result<PageCount> {
    u32::try_from(len)
        .ok()
        .and_then(NonZeroU32::new)
        .map(PageCount)
        .ok_or(Error::InvalidGeometry)
}

/// `index` is zero-based. The result is the one-based page number.
fn page_index(index: usize) -> Result<NonZeroU32> {
    u32::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .and_then(NonZeroU32::new)
        .ok_or(Error::InvalidGeometry)
}

fn measure_layer(
    ctx: &mut LayoutCtx<'_>,
    content: Option<&LayerContent<'_>>,
    page: &PageInfo,
) -> Result<Option<PlacedFragment>> {
    let Some(content) = content else {
        return Ok(None);
    };
    let mut commands = Vec::new();
    for item in content(page)? {
        match item {
            LayerItem::Text(item) => {
                let style = Style {
                    align: TextAlign::Start,
                    ..item.style
                };
                let mut run = single_line_run(ctx.fonts, &item.text, &style, f32::INFINITY)?;
                run.origin = Point::try_pt(
                    item.at.x().as_pt() + run.origin.x().as_pt(),
                    item.at.y().as_pt() + run.origin.y().as_pt(),
                )?;
                commands.push(FragmentCommand::Glyphs(run));
            }
            LayerItem::Image { image, rect } => {
                commands.push(FragmentCommand::Image { image, rect });
            }
            LayerItem::Shape { path, ink } => match ink {
                Ink::Fill(color) => commands.push(FragmentCommand::Fill {
                    path,
                    paint: Paint::solid(color),
                }),
                Ink::Stroke(pen) => commands.push(FragmentCommand::Stroke {
                    path,
                    paint: Paint::solid(pen.color()),
                    stroke: Stroke::new(pen.width()),
                }),
                Ink::FillStroke(color, pen) => {
                    commands.push(FragmentCommand::Fill {
                        path: path.clone(),
                        paint: Paint::solid(color),
                    });
                    commands.push(FragmentCommand::Stroke {
                        path,
                        paint: Paint::solid(pen.color()),
                        stroke: Stroke::new(pen.width()),
                    });
                }
            },
        }
    }
    if commands.is_empty() {
        return Ok(None);
    }
    Ok(Some(PlacedFragment {
        origin: Point::try_pt(0.0, 0.0)?,
        fragment: Fragment {
            height: 0.0,
            spacing_after: 0.0,
            commands,
        },
    }))
}

fn measure_band(
    ctx: &mut LayoutCtx<'_>,
    nodes: Vec<FlowNode>,
    size: Size,
    margin: f32,
) -> Result<Option<Fragment>> {
    if nodes.is_empty() {
        return Ok(None);
    }
    check_content(Container::Band, &nodes)?;
    let page_h = size.height().as_pt();
    let frame = Frame {
        width: size.width().as_pt(),
        height: page_h,
        full_height: page_h,
    };
    let taller = |height| Error::DecorationTallerThanMargin { height, margin };
    match FlowNode::Stack(nodes).split(ctx, frame) {
        Ok(Split::Fit { height, fragment }) if height <= margin => Ok(Some(fragment)),
        Ok(Split::Fit { height, .. }) => Err(taller(height)),
        Ok(Split::Partial { .. } | Split::MoveToNextPage(_))
        | Err(Error::UnsplittableTallerThanFrame) => Err(taller(page_h)),
        Err(Error::UnbreakableWiderThanFrame { width, avail }) => {
            Err(Error::DecorationWiderThanFrame { width, avail })
        }
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::CanvasCommand;
    use crate::geom::{Color, Ink, NonNegative, Path, Pen, Positive, Rect};
    use crate::layout::columns::{ColumnWidth, ColumnsNode, Ratio};
    use crate::layout::paginate::PaginateConfig;
    use crate::layout::table::{TableNode, TableSpec};
    use crate::layout::test_util::{
        bits, ellipse_center_radii, fills, fonts_and_style, metrics, page, pt, render, runs, text,
        texts,
    };

    fn band(label: &'static str, style: &Style) -> BandContent<'static> {
        let style = style.clone();
        Box::new(move |_: &PageInfo| Ok(vec![text(label, &style)]))
    }

    fn numbered(style: &Style) -> BandContent<'static> {
        let style = style.clone();
        Box::new(move |page: &PageInfo| {
            let label = format!("Page {} / {}", page.number.get(), page.count.get());
            Ok(vec![text(&label, &style)])
        })
    }

    fn margins(top: f32, side: f32, bottom: f32) -> PaginateConfig {
        PaginateConfig {
            page_size: Size::try_pt(200.0, 400.0).unwrap(),
            margins: Margins::new(pt(top), pt(side), pt(bottom), pt(side)),
        }
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
        let (mut fonts, style) = fonts_and_style();
        let b = metrics(&fonts, &style).baseline;
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
        let (mut fonts, style) = fonts_and_style();
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
        let b = metrics(&fonts, &style).baseline;
        let expected = vec![
            ("A".to_owned(), 20.0f32.to_bits(), (30.0 + b).to_bits()),
            ("B".to_owned(), 20.0f32.to_bits(), (30.0 + b).to_bits()),
        ];
        assert_eq!(body(&bare.1), expected);
        assert_eq!(body(&dressed.1), expected);
    }

    #[test]
    fn header_with_zero_margin_errors_before_any_page_starts() {
        let (mut fonts, style) = fonts_and_style();
        let line = metrics(&fonts, &style).line_height;
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
            Err(Error::DecorationTallerThanMargin {
                height: line,
                margin: 0.0
            })
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn footer_taller_than_its_band_errors() {
        let (mut fonts, style) = fonts_and_style();
        let line = metrics(&fonts, &style).line_height;
        let decorations = Decorations {
            header: None,
            footer: Some(band("A\nB", &style)),
            ..Decorations::none()
        };
        let config = margins(0.0, 0.0, line + 1.0);
        let (result, rec) = render(text("A", &style), &mut fonts, config, &decorations);
        assert_eq!(
            result,
            Err(Error::DecorationTallerThanMargin {
                height: line * 2.0,
                margin: line + 1.0
            })
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn header_wider_than_page_errors() {
        let (mut fonts, style) = fonts_and_style();
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
            matches!(result, Err(Error::DecorationWiderThanFrame { avail, .. }) if avail.to_bits() == 200.0f32.to_bits())
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn band_rejects_images() {
        let (mut fonts, style) = fonts_and_style();
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
            header: Some(Box::new(move |_: &PageInfo| {
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
            Err(Error::InvalidCellContent {
                container: "header/footer band",
                node: "image"
            })
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn band_rejects_tables() {
        let (mut fonts, style) = fonts_and_style();
        let decorations = Decorations {
            header: Some(Box::new(|_: &PageInfo| {
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
            Err(Error::InvalidCellContent {
                container: "header/footer band",
                node: "table"
            })
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
        let (mut fonts, style) = fonts_and_style();
        let b = metrics(&fonts, &style).baseline;
        let header_style = style.clone();
        let decorations = Decorations {
            header: Some(Box::new(move |_: &PageInfo| {
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
        let (mut fonts, style) = fonts_and_style();
        let header_style = style.clone();
        let decorations = Decorations {
            header: Some(Box::new(move |_: &PageInfo| {
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
            Err(Error::InvalidCellContent {
                container: "header/footer band",
                node: "table"
            })
        );
        assert!(rec.commands().is_empty());
    }

    fn layer(label: &'static str, x: f32, y: f32, style: &Style) -> LayerContent<'static> {
        let style = style.clone();
        Box::new(move |_: &PageInfo| {
            Ok(vec![LayerItem::Text(LayerText {
                text: label.to_owned(),
                at: Point::try_pt(x, y).unwrap(),
                style: style.clone(),
            })])
        })
    }

    #[test]
    fn layers_paint_behind_header_body_footer_front() {
        let (mut fonts, style) = fonts_and_style();
        let b = metrics(&fonts, &style).baseline;
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
        let (mut fonts, style) = fonts_and_style();
        let b = metrics(&fonts, &style).baseline;
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
        let (mut fonts, style) = fonts_and_style();
        let decorations = Decorations {
            behind: Some(layer("\u{6f22}", 0.0, 0.0, &style)),
            ..Decorations::none()
        };
        let (result, rec) = render(
            text("A", &style),
            &mut fonts,
            page(200.0, 400.0),
            &decorations,
        );
        assert!(matches!(
            result,
            Err(Error::MissingGlyph {
                scalar: '\u{6f22}',
                ..
            })
        ));
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn path_behind_paints_before_header_glyphs() {
        let (mut fonts, style) = fonts_and_style();
        let path = Path::line(
            Point::try_pt(0.0, 0.0).unwrap(),
            Point::try_pt(40.0, 10.0).unwrap(),
        );
        let decorations = Decorations {
            header: Some(band("H", &style)),
            behind: Some(Box::new(move |_: &PageInfo| {
                Ok(vec![LayerItem::Shape {
                    path: path.clone(),
                    ink: Ink::Stroke(Pen::new(Color::rgb(1, 2, 3), Positive::pt(1.5).unwrap())),
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
        let (mut fonts, style) = fonts_and_style();
        let rect = Rect::try_pt(10.0, 20.0, 40.0, 20.0).unwrap();
        let path = Path::ellipse(rect).unwrap();
        let decorations = Decorations {
            behind: Some(Box::new(move |_: &PageInfo| {
                Ok(vec![LayerItem::Shape {
                    path: path.clone(),
                    ink: Ink::FillStroke(
                        Color::rgba(9, 8, 7, 40),
                        Pen::new(Color::rgb(1, 2, 3), Positive::pt(2.0).unwrap()),
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
        let (mut fonts, style) = fonts_and_style();
        let decorations = Decorations {
            header: Some(Box::new(|_: &PageInfo| Err(Error::EmptyLink))),
            ..Decorations::none()
        };
        let (result, rec) = render(
            text("A", &style),
            &mut fonts,
            page(200.0, 400.0),
            &decorations,
        );
        assert_eq!(result, Err(Error::EmptyLink));
        assert!(rec.commands().is_empty());
    }
}
