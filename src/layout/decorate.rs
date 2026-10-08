//! Header and footer drawing inside margin bands (does not shrink the body frame).

#![allow(dead_code)]

use std::num::NonZeroU32;

use crate::error::{Error, Result};
use crate::geom::{Margins, Point, Size};
use crate::layout::flow::{FlowNode, Fragment, Frame, LayoutCtx, PlacedFragment, Split};
use crate::layout::lower::{check_content, Container};

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
    pub(crate) size: Size,
    pub(crate) margins: Margins,
}

pub(crate) type BandContent = Box<dyn Fn(&PageInfo) -> Result<Vec<FlowNode>>>;

#[derive(Default)]
pub(crate) struct Decorations {
    pub(crate) header: Option<BandContent>,
    pub(crate) footer: Option<BandContent>,
}

impl Decorations {
    pub(crate) fn none() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PageBands {
    pub(crate) header: Option<PlacedFragment>,
    pub(crate) footer: Option<PlacedFragment>,
}

/// Lays out every band before anything is painted, so a failure leaves the canvas untouched.
///
/// Bands span the full page width: header `y` in `[0, margin.top)`, footer in
/// `[page_h - margin.bottom, page_h)`.
pub(crate) fn measure_bands(
    sizes: &[Size],
    margins: Margins,
    decorations: &Decorations,
    ctx: &mut LayoutCtx<'_>,
) -> Result<Vec<PageBands>> {
    let count = u32::try_from(sizes.len())
        .ok()
        .and_then(NonZeroU32::new)
        .map(PageCount)
        .ok_or(Error::InvalidGeometry)?;
    let mut bands = Vec::with_capacity(sizes.len());
    for (number, size) in (1u32..).zip(sizes) {
        let page = PageInfo {
            number: PageNumber(NonZeroU32::new(number).expect("starts at one")),
            count,
            size: *size,
            margins,
        };
        let top = margins.top().as_pt();
        let bottom = margins.bottom().as_pt();
        let page_h = size.height().as_pt();
        let mut band = |content: &Option<BandContent>, y: f32, margin: f32| -> Result<_> {
            let Some(content) = content else {
                return Ok(None);
            };
            let fragment = measure_band(ctx, content(&page)?, *size, margin)?;
            fragment
                .map(|fragment| {
                    Ok(PlacedFragment {
                        origin: Point::try_pt(0.0, y)?,
                        fragment,
                    })
                })
                .transpose()
        };
        bands.push(PageBands {
            header: band(&decorations.header, 0.0, top)?,
            footer: band(&decorations.footer, page_h - bottom, bottom)?,
        });
    }
    Ok(bands)
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
    use crate::layout::columns::ColumnWidth;
    use crate::layout::paginate::PaginateConfig;
    use crate::layout::style::Style;
    use crate::layout::table::{TableNode, TableSpec};
    use crate::layout::test_util::{
        bits, fonts_and_style, metrics, page, pt, render, runs, text, texts,
    };

    fn band(label: &'static str, style: &Style) -> BandContent {
        let style = style.clone();
        Box::new(move |_: &PageInfo| Ok(vec![text(label, &style)]))
    }

    fn numbered(style: &Style) -> BandContent {
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
    fn band_rejects_tables() {
        let (mut fonts, style) = fonts_and_style();
        let decorations = Decorations {
            header: Some(Box::new(|_: &PageInfo| {
                Ok(vec![FlowNode::Table(TableNode::new(TableSpec::new(
                    vec![ColumnWidth::Auto],
                ))?)])
            })),
            footer: None,
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
}
