//! Header and footer drawing inside margin bands (does not shrink the body frame).

use std::num::NonZeroU32;

use crate::error::{
    Error, ErrorBag, ErrorKind, Fallible as Result, GeometryKind, Location, Region,
};
use crate::geom::{Ink, Margins, NonNegative, Paint, Path, Point, Size, Stroke};
use crate::layout::flow::FlowNode;
use crate::layout::frame::{
    stacked_height, Flowable, Fragment, FragmentCommand, Frame, LayoutCtx, PlacedFragment, Split,
};
use crate::layout::line_run::single_line_run;
use crate::layout::nesting::{check_content, Container};
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

pub(crate) type BandContent<'a> =
    Box<dyn Fn(&PageInfo, &mut ErrorBag) -> Result<Vec<FlowNode>> + 'a>;

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

pub(crate) type LayerContent<'a> =
    Box<dyn Fn(&PageInfo, &mut ErrorBag) -> Result<Vec<LayerItem>> + 'a>;

#[derive(Default)]
pub(crate) struct Decorations<'a> {
    pub(crate) header: Option<BandContent<'a>>,
    pub(crate) footer: Option<BandContent<'a>>,
    pub(crate) behind: Option<LayerContent<'a>>,
    pub(crate) front: Option<LayerContent<'a>>,
}

impl Decorations<'_> {
    #[cfg(test)]
    pub(crate) fn none() -> Self {
        Self::default()
    }
}

/// Pages `[start, start + len)` share one section's margins and decorations.
pub(crate) struct SectionSpan<'a> {
    pub(crate) start: usize,
    pub(crate) len: NonZeroU32,
    pub(crate) margins: Margins,
    pub(crate) decorations: &'a Decorations<'a>,
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
    spans: &[SectionSpan<'_>],
    ctx: &mut LayoutCtx<'_>,
) -> Result<Vec<PageBands>> {
    let count = page_count(sizes.len())?;
    let mut covered = 0usize;
    for span in spans {
        if span.start != covered {
            return Err(GeometryKind::SectionSpans.into());
        }
        covered = usize::try_from(span.len.get())
            .ok()
            .and_then(|len| covered.checked_add(len))
            .ok_or(GeometryKind::SectionSpans)?;
    }
    if covered != sizes.len() {
        return Err(GeometryKind::SectionSpans.into());
    }
    let mut bands = Vec::with_capacity(sizes.len());
    for (section, span) in spans.iter().enumerate() {
        let section_count = PageCount(span.len);
        let margins = span.margins;
        let decorations = span.decorations;
        for offset in 0..span.len.get() {
            let index = span
                .start
                .checked_add(offset as usize)
                .ok_or(GeometryKind::SectionSpans)?;
            let size = sizes
                .get(index)
                .copied()
                .ok_or(GeometryKind::SectionSpans)?;
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
            let mut place = |region: Region,
                             content: &Option<BandContent<'_>>,
                             y: f32,
                             margin: f32|
             -> Result<_> {
                let Some(content) = content else {
                    return Ok(None);
                };
                let mut at = Location::page_index(index).with_region(region);
                at.section = Some(section);
                let fragment = ctx.located(&at, |ctx| {
                    let nodes = content(&page, ctx.errors)?;
                    measure_band(ctx, nodes, size, margin)
                })?;
                fragment
                    .map(|fragment| {
                        Ok(PlacedFragment {
                            origin: Point::try_pt(0.0, y)?,
                            fragment,
                        })
                    })
                    .transpose()
            };
            let header = place(Region::Header, &decorations.header, 0.0, top)?;
            let footer = place(Region::Footer, &decorations.footer, page_h - bottom, bottom)?;
            let behind = {
                let mut at = Location::page_index(index).with_region(Region::Behind);
                at.section = Some(section);
                ctx.located(&at, |ctx| {
                    measure_layer(ctx, decorations.behind.as_ref(), &page)
                })?
            };
            let front = {
                let mut at = Location::page_index(index).with_region(Region::Front);
                at.section = Some(section);
                ctx.located(&at, |ctx| {
                    measure_layer(ctx, decorations.front.as_ref(), &page)
                })?
            };
            bands.push(PageBands {
                behind,
                header,
                footer,
                front,
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
        .ok_or(GeometryKind::PageCount { pages: len }.into())
}

/// `index` is zero-based. The result is the one-based page number.
fn page_index(index: usize) -> Result<NonZeroU32> {
    u32::try_from(index)
        .ok()
        .and_then(|value| value.checked_add(1))
        .and_then(NonZeroU32::new)
        .ok_or(
            GeometryKind::PageCount {
                pages: index.saturating_add(1),
            }
            .into(),
        )
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
    for item in content(page, ctx.errors)? {
        match item {
            LayerItem::Text(item) => {
                let style = Style {
                    align: TextAlign::Start,
                    ..item.style
                };
                let mut run = single_line_run(ctx, &item.text, &style, None)?;
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
            height: NonNegative::ZERO,
            spacing_after: NonNegative::ZERO,
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
    let frame = Frame {
        width: size.width(),
        height: size.height().into(),
        full_height: size.height(),
    };
    let taller = |height| Error::from(ErrorKind::DecorationTallerThanMargin { height, margin });
    match FlowNode::Stack(nodes).split(ctx, frame) {
        Ok(Split::Fit { height, fragment }) if height.as_pt() <= margin => Ok(Some(fragment)),
        Ok(Split::Fit { height, .. }) => Err(taller(height.as_pt())),
        Err(Error {
            kind: ErrorKind::UnsplittableTallerThanFrame { height, .. },
            ..
        }) => Err(taller(height)),
        Ok(split) => Err(taller(stacked_height(split, ctx, frame)?.as_pt())),
        Err(Error {
            kind: ErrorKind::UnbreakableWiderThanFrame { width, avail, .. },
            ..
        }) => Err(Error::from(ErrorKind::DecorationWiderThanFrame {
            width,
            avail,
        })),
        Err(other) => Err(other),
    }
}
