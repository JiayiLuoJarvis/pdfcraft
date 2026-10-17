//! Page breaking and page assembly over the flow.

use std::collections::VecDeque;
use std::num::NonZeroU32;

use crate::canvas::Canvas;
use crate::error::{Error, ErrorKind, Fallible as Result, GeometryKind, Location, Region};
use crate::geom::{Length, Margins, NonNegative, Point, Positive, Size};
use crate::layout::decorate::{measure_bands, Decorations, PageBands, SectionSpan};
use crate::layout::flow::FlowNode;
use crate::layout::frame::{draw_fragment, Flowable, Frame, LayoutCtx, PlacedFragment, Split};
use crate::limits::LimitKind;
#[cfg(test)]
use crate::limits::Limits;

/// Page geometry for pagination. Header and footer bands sit inside the margins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PaginateConfig {
    pub(crate) page_size: Size,
    pub(crate) margins: Margins,
}

impl PaginateConfig {
    pub(crate) fn body_width(self) -> Length<crate::geom::Finite> {
        Length::from_trusted_pt(
            self.page_size.width().as_pt()
                - self.margins.left().as_pt()
                - self.margins.right().as_pt(),
        )
    }

    pub(crate) fn body_height(self) -> Length<crate::geom::Finite> {
        Length::from_trusted_pt(
            self.page_size.height().as_pt()
                - self.margins.top().as_pt()
                - self.margins.bottom().as_pt(),
        )
    }

    pub(crate) fn body_origin(self) -> Result<Point> {
        Point::try_pt(self.margins.left().as_pt(), self.margins.top().as_pt())
    }

    fn validate(self) -> Result<()> {
        let w = self.body_width().as_pt();
        let h = self.body_height().as_pt();
        if !w.is_finite() || w <= 0.0 || !h.is_finite() || h <= 0.0 {
            return Err(GeometryKind::BodyFrame {
                width: w,
                height: h,
            }
            .into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageCursor {
    Fresh,
    HasBody,
    PendingBlank,
}

#[derive(Debug, Clone, PartialEq)]
struct PagePlan {
    size: Size,
    placed: Vec<PlacedFragment>,
}

/// One section's body, paper, and page decorations.
pub(crate) struct SectionJob<'a> {
    pub(crate) config: PaginateConfig,
    pub(crate) root: FlowNode,
    pub(crate) decorations: &'a Decorations<'a>,
}

/// Paginate one section and paint onto `canvas`.
///
/// Bands are measured after the body plan, so decorations can never move body content.
#[cfg(test)]
pub(crate) fn paginate_root(
    root: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    config: PaginateConfig,
    decorations: &Decorations<'_>,
    canvas: &mut dyn Canvas,
) -> Result<()> {
    paginate_sections(
        vec![SectionJob {
            config,
            root,
            decorations,
        }],
        ctx,
        canvas,
    )
}

pub(crate) struct LaidDocument {
    pages: Vec<PagePlan>,
    bands: Vec<PageBands>,
}

/// Plan every section and measure bands. Painting is separate so a dirty bag can skip it.
pub(crate) fn layout_pages(
    jobs: Vec<SectionJob<'_>>,
    ctx: &mut LayoutCtx<'_>,
    max_pages: u32,
) -> Result<LaidDocument> {
    let mut pages = Vec::new();
    let mut spans = Vec::with_capacity(jobs.len());
    for (section, job) in jobs.into_iter().enumerate() {
        let start = pages.len();
        let planned = ctx.located(&Location::section(section), |ctx| {
            plan_pages(job.root, ctx, job.config, start, max_pages)
        })?;
        let len = u32::try_from(planned.len())
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(GeometryKind::PageCount {
                pages: planned.len(),
            })
            .map_err(|kind| Error::from(kind).in_section(section))?;
        pages.extend(planned);
        spans.push(SectionSpan {
            start,
            len,
            margins: job.config.margins,
            decorations: job.decorations,
        });
    }
    let sizes: Vec<Size> = pages.iter().map(|page| page.size).collect();
    let bands = measure_bands(&sizes, &spans, ctx)?;
    Ok(LaidDocument { pages, bands })
}

/// Plan every section, then measure and paint the flat page list.
#[cfg(test)]
pub(crate) fn paginate_sections(
    jobs: Vec<SectionJob<'_>>,
    ctx: &mut LayoutCtx<'_>,
    canvas: &mut dyn Canvas,
) -> Result<()> {
    let laid = match layout_pages(jobs, ctx, Limits::default().max_pages()) {
        Ok(laid) => laid,
        Err(hard) => {
            let bag = std::mem::take(ctx.errors);
            return Err(bag.fail(hard).into_iter().next().expect("fail keeps one"));
        }
    };
    if !ctx.errors.is_empty() {
        let bag = std::mem::take(ctx.errors);
        let errors = bag.finish(()).expect_err("bag was not empty");
        return Err(errors.first().clone());
    }
    paint_pages(&laid.pages, &laid.bands, canvas)
}

pub(crate) fn paint_laid(laid: &LaidDocument, canvas: &mut dyn Canvas) -> Result<()> {
    paint_pages(&laid.pages, &laid.bands, canvas)
}

/// `first_page` is the zero-based index of this section's first page in the document.
fn plan_pages(
    root: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    config: PaginateConfig,
    first_page: usize,
    max_pages: u32,
) -> Result<Vec<PagePlan>> {
    config.validate()?;
    let mut nodes: VecDeque<FlowNode> = match root {
        FlowNode::Stack(kids) => kids.into(),
        other => VecDeque::from([other]),
    };

    let body_w = Length::<Positive>::from_trusted_pt(config.body_width().as_pt());
    let body_h = Length::<Positive>::from_trusted_pt(config.body_height().as_pt());
    let body_origin = config.body_origin()?;

    let mut cursor = PageCursor::Fresh;
    let mut pages: Vec<PagePlan> = Vec::new();
    let mut current: Option<PagePlan> = None;
    let mut y = NonNegative::ZERO;

    while let Some(node) = nodes.pop_front() {
        match node {
            FlowNode::PageBreak => {
                cursor = apply_page_break(
                    cursor,
                    &mut pages,
                    &mut current,
                    config,
                    first_page,
                    max_pages,
                )?;
                y = NonNegative::ZERO;
            }
            node => {
                ensure_content_page(&mut current, config);
                let avail = Frame {
                    width: body_w,
                    height: (body_h - y).clamp_non_negative(),
                    full_height: body_h,
                };
                let split = ctx.located(
                    &Location::page_index(first_page + pages.len()).with_region(Region::Body),
                    |ctx| node.split(ctx, avail),
                )?;
                match split {
                    Split::Fit { height, fragment } => {
                        let page = current.as_mut().expect("content page open");
                        let origin = Point::try_pt(
                            body_origin.x().as_pt(),
                            body_origin.y().as_pt() + y.as_pt(),
                        )?;
                        let spacing = fragment.spacing_after;
                        page.placed.push(PlacedFragment { origin, fragment });
                        y += height;
                        if spacing > NonNegative::ZERO {
                            let remain = (body_h - y).clamp_non_negative();
                            if remain >= spacing {
                                y += spacing;
                            }
                        }
                        cursor = PageCursor::HasBody;
                    }
                    Split::Partial {
                        head_height: _,
                        head,
                        tail,
                    } => {
                        let page = current.as_mut().expect("content page open");
                        let origin = Point::try_pt(
                            body_origin.x().as_pt(),
                            body_origin.y().as_pt() + y.as_pt(),
                        )?;
                        page.placed.push(PlacedFragment {
                            origin,
                            fragment: head,
                        });
                        commit_page(&mut pages, &mut current, first_page, max_pages)?;
                        nodes.push_front(tail);
                        y = NonNegative::ZERO;
                        cursor = PageCursor::Fresh;
                    }
                    Split::MoveToNextPage(tail) => {
                        if y == NonNegative::ZERO {
                            return Err(Error::from(GeometryKind::StalledOnFreshPage {
                                height: body_h.as_pt(),
                            })
                            .in_region(Region::Body)
                            .on_page_index(first_page + pages.len()));
                        }
                        commit_page(&mut pages, &mut current, first_page, max_pages)?;
                        nodes.push_front(tail);
                        y = NonNegative::ZERO;
                        cursor = PageCursor::Fresh;
                    }
                }
            }
        }
    }

    finalize_eof(
        cursor,
        &mut pages,
        &mut current,
        config,
        first_page,
        max_pages,
    )?;
    Ok(pages)
}

fn ensure_content_page(current: &mut Option<PagePlan>, config: PaginateConfig) {
    if current.is_none() {
        *current = Some(PagePlan {
            size: config.page_size,
            placed: Vec::new(),
        });
    }
}

fn apply_page_break(
    cursor: PageCursor,
    pages: &mut Vec<PagePlan>,
    current: &mut Option<PagePlan>,
    config: PaginateConfig,
    first_page: usize,
    max_pages: u32,
) -> Result<PageCursor> {
    Ok(match cursor {
        PageCursor::Fresh => PageCursor::Fresh,
        PageCursor::HasBody => {
            commit_page(pages, current, first_page, max_pages)?;
            PageCursor::PendingBlank
        }
        PageCursor::PendingBlank => {
            push_page(
                pages,
                PagePlan {
                    size: config.page_size,
                    placed: Vec::new(),
                },
                first_page,
                max_pages,
            )?;
            PageCursor::PendingBlank
        }
    })
}

fn commit_page(
    pages: &mut Vec<PagePlan>,
    current: &mut Option<PagePlan>,
    first_page: usize,
    max_pages: u32,
) -> Result<()> {
    if let Some(page) = current.take() {
        push_page(pages, page, first_page, max_pages)?;
    }
    Ok(())
}

fn push_page(
    pages: &mut Vec<PagePlan>,
    page: PagePlan,
    first_page: usize,
    max_pages: u32,
) -> Result<()> {
    let next = first_page.saturating_add(pages.len()).saturating_add(1);
    let next = u64::try_from(next).unwrap_or(u64::MAX);
    let max = u64::from(max_pages);
    if next > max {
        return Err(Error::from(ErrorKind::LimitExceeded {
            kind: LimitKind::Pages,
            value: next,
            max,
        }));
    }
    pages.push(page);
    Ok(())
}

fn finalize_eof(
    cursor: PageCursor,
    pages: &mut Vec<PagePlan>,
    current: &mut Option<PagePlan>,
    config: PaginateConfig,
    first_page: usize,
    max_pages: u32,
) -> Result<()> {
    match cursor {
        PageCursor::Fresh => {
            if pages.is_empty() {
                push_page(
                    pages,
                    PagePlan {
                        size: config.page_size,
                        placed: Vec::new(),
                    },
                    first_page,
                    max_pages,
                )?;
            }
            Ok(())
        }
        PageCursor::HasBody => commit_page(pages, current, first_page, max_pages),
        PageCursor::PendingBlank => {
            current.take();
            Ok(())
        }
    }
}

fn paint_pages(pages: &[PagePlan], bands: &[PageBands], canvas: &mut dyn Canvas) -> Result<()> {
    for (index, (page, bands)) in pages.iter().zip(bands).enumerate() {
        paint_page(page, bands, canvas).map_err(|error| error.on_page_index(index))?;
    }
    Ok(())
}

fn paint_page(page: &PagePlan, bands: &PageBands, canvas: &mut dyn Canvas) -> Result<()> {
    canvas.start_page(page.size)?;
    let stacked = (bands.behind.iter())
        .chain(&bands.header)
        .chain(&page.placed)
        .chain(&bands.footer)
        .chain(&bands.front);
    for placed in stacked {
        draw_fragment(&placed.fragment, canvas, placed.origin)?;
    }
    canvas.finish_page()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{CanvasCommand, RecordedPage, RecordingCanvas};
    use crate::error::ErrorKind;
    use crate::fonts::{fixture_bytes, FontSource, FontStore};
    use crate::geom::{Color, NonNegative, Positive, A4};
    use crate::layout::flow::{KeepTogetherNode, SpacerNode};
    use crate::layout::line_run::LinePiece;
    use crate::layout::paragraph::ParagraphNode;
    use crate::layout::style::{Style, TextAlign};
    use crate::text::measure::{FaceExtent, LineMetrics};

    fn fonts_and_style(align: TextAlign, spacing: f32) -> Option<(FontStore, Style)> {
        let bytes = fixture_bytes()?;
        let mut fonts = FontStore::new();
        let font = fonts.register(FontSource { bytes, index: 0 }).unwrap();
        let style = Style::new(
            font,
            Positive::pt(12.0).unwrap(),
            1.2,
            Color::rgb(0, 0, 0),
            NonNegative::pt(spacing).unwrap(),
            align,
        )
        .unwrap();
        Some((fonts, style))
    }

    fn config(page: Size) -> PaginateConfig {
        PaginateConfig {
            page_size: page,
            margins: Margins::default(),
        }
    }

    fn page_count(rec: &RecordingCanvas) -> usize {
        rec.commands()
            .iter()
            .filter(|c| matches!(c, CanvasCommand::FinishPage))
            .count()
    }

    fn glyph_origins(rec: &RecordingCanvas) -> Vec<(f32, f32)> {
        rec.commands()
            .iter()
            .filter_map(|c| match c {
                CanvasCommand::DrawGlyphRun { run } => {
                    Some((run.origin.x().as_pt(), run.origin.y().as_pt()))
                }
                _ => None,
            })
            .collect()
    }

    fn page_runs(page: &RecordedPage) -> Vec<(String, f32)> {
        page.ops()
            .iter()
            .filter_map(|op| match op {
                CanvasCommand::DrawGlyphRun { run } => {
                    Some((run.text.clone(), run.origin.y().as_pt()))
                }
                _ => None,
            })
            .collect()
    }

    fn page_texts(page: &RecordedPage) -> Vec<String> {
        page_runs(page).into_iter().map(|(text, _)| text).collect()
    }

    #[test]
    fn first_line_indent_uses_the_inset_alignment_box() {
        let body_w = 200.0f32;
        let indent = 24.0f32;
        let page = Size::try_pt(body_w, 400.0).unwrap();
        for align in [TextAlign::Start, TextAlign::Center, TextAlign::End] {
            let Some((mut fonts, mut style)) = fonts_and_style(align, 0.0) else {
                return;
            };
            style.first_line_indent = NonNegative::pt(indent).unwrap();
            let para = ParagraphNode::layout(&mut fonts, "AB", &style, body_w).unwrap();
            let face = fonts
                .metrics(style.font.expect("test style has a face"))
                .unwrap();
            let metrics = LineMetrics::new(
                FaceExtent::scaled(face, style.font_size).into(),
                style.line_height_factor,
            )
            .unwrap();
            let line_width = {
                // Recompute via first run advance.
                let LinePiece::Glyphs(run) = &para.lines[0].pieces[0] else {
                    panic!("text line starts with glyphs");
                };
                run.glyphs
                    .iter()
                    .map(|g| g.advance_norm * style.font_size.as_pt())
                    .sum::<f32>()
            };
            let expected_x = match align {
                TextAlign::Start => indent,
                TextAlign::Center => indent + (body_w - indent - line_width) / 2.0,
                TextAlign::End => body_w - line_width,
            };
            let expected_y = metrics.baseline.as_pt();

            let mut errors = crate::error::ErrorBag::default();
            let mut ctx = LayoutCtx {
                natural: false,
                fonts: &mut fonts,
                errors: &mut errors,
            };
            let mut rec = RecordingCanvas::new();
            paginate_root(
                FlowNode::Paragraph(para),
                &mut ctx,
                config(page),
                &Decorations::none(),
                &mut rec,
            )
            .unwrap();
            let origins = glyph_origins(&rec);
            assert_eq!(origins.len(), 1);
            assert_eq!(origins[0].0.to_bits(), expected_x.to_bits());
            assert_eq!(origins[0].1.to_bits(), expected_y.to_bits());
        }
    }

    #[test]
    fn leading_page_break_is_noop_one_blank_page() {
        let Some((mut fonts, _)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![FlowNode::PageBreak]),
            &mut ctx,
            config(A4),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(page_count(&rec), 1);
        assert!(glyph_origins(&rec).is_empty());
    }

    #[test]
    fn leading_two_page_breaks_still_one_page() {
        let Some((mut fonts, _)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![FlowNode::PageBreak, FlowNode::PageBreak]),
            &mut ctx,
            config(A4),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(page_count(&rec), 1);
    }

    #[test]
    fn content_then_one_page_break_drops_pending() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let para = ParagraphNode::layout(&mut fonts, "A", &style, A4.width().as_pt()).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![FlowNode::Paragraph(para), FlowNode::PageBreak]),
            &mut ctx,
            config(A4),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(page_count(&rec), 1);
        assert_eq!(glyph_origins(&rec).len(), 1);
    }

    #[test]
    fn content_then_two_page_breaks_yields_blank_body_page() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let para = ParagraphNode::layout(&mut fonts, "A", &style, A4.width().as_pt()).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![
                FlowNode::Paragraph(para),
                FlowNode::PageBreak,
                FlowNode::PageBreak,
            ]),
            &mut ctx,
            config(A4),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(page_count(&rec), 2);
        assert_eq!(glyph_origins(&rec).len(), 1);
    }

    #[test]
    fn content_break_content_is_two_pages() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let w = A4.width().as_pt();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, w).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, w).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![
                FlowNode::Paragraph(a),
                FlowNode::PageBreak,
                FlowNode::Paragraph(b),
            ]),
            &mut ctx,
            config(A4),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(page_count(&rec), 2);
        assert_eq!(glyph_origins(&rec).len(), 2);
    }

    #[test]
    fn content_break_break_content_is_three_pages() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let w = A4.width().as_pt();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, w).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, w).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![
                FlowNode::Paragraph(a),
                FlowNode::PageBreak,
                FlowNode::PageBreak,
                FlowNode::Paragraph(b),
            ]),
            &mut ctx,
            config(A4),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(page_count(&rec), 3);
        let pages = rec.pages();
        assert!(pages[1].ops().is_empty());
        assert_eq!(page_texts(&pages[0]), vec!["A".to_owned()]);
        assert_eq!(page_texts(&pages[2]), vec!["B".to_owned()]);
    }

    #[test]
    fn spacer_remainder_leads_next_page() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let para = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let page = Size::try_pt(200.0, 50.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![
                FlowNode::Spacer(SpacerNode {
                    height: NonNegative::pt(80.0).unwrap(),
                }),
                FlowNode::Paragraph(para),
            ]),
            &mut ctx,
            config(page),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        assert!(pages[0].ops().is_empty());
        let runs = page_runs(&pages[1]);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].0, "A");
        assert_eq!(
            runs[0].1.to_bits(),
            (30.0 + metrics.baseline.as_pt()).to_bits()
        );
    }

    #[test]
    fn paragraph_resumes_at_top_of_next_page() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let line_h = metrics.line_height.as_pt();
        let page = Size::try_pt(200.0, line_h * 2.0).unwrap();
        let para = ParagraphNode::layout(&mut fonts, "A\nB\nC", &style, 200.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Paragraph(para),
            &mut ctx,
            config(page),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        let first = page_runs(&pages[0]);
        let second = page_runs(&pages[1]);
        assert_eq!(
            first.iter().map(|run| run.0.as_str()).collect::<Vec<_>>(),
            vec!["A", "B"]
        );
        assert_eq!(first[0].1.to_bits(), metrics.baseline.as_pt().to_bits());
        assert_eq!(
            first[1].1.to_bits(),
            (line_h + metrics.baseline.as_pt()).to_bits()
        );
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].0, "C");
        assert_eq!(second[0].1.to_bits(), metrics.baseline.as_pt().to_bits());
    }

    #[test]
    fn spacing_after_dropped_when_no_room() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 20.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let line_h = metrics.line_height.as_pt();
        // Body fits exactly one line; spacing_after 20 cannot fit.
        let page = Size::try_pt(200.0, line_h).unwrap();
        let first = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let second = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![
                FlowNode::Paragraph(first),
                FlowNode::Paragraph(second),
            ]),
            &mut ctx,
            config(page),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(page_count(&rec), 2);
        let origins = glyph_origins(&rec);
        assert_eq!(origins.len(), 2);
        // Second page first glyph at body top baseline, not pushed by leftover spacing.
        assert_eq!(origins[1].1.to_bits(), metrics.baseline.as_pt().to_bits());
    }

    #[test]
    fn keep_together_moves_to_next_page() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let line_h = metrics.line_height.as_pt();
        // Two lines fit in the full frame. One leading line leaves only one line of room.
        let page = Size::try_pt(200.0, line_h * 2.0 + 10.0).unwrap();
        let filler = ParagraphNode::layout(&mut fonts, "X", &style, 200.0).unwrap();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![
                FlowNode::Paragraph(filler),
                FlowNode::KeepTogether(KeepTogetherNode::Open(vec![
                    FlowNode::Paragraph(a),
                    FlowNode::Paragraph(b),
                ])),
            ]),
            &mut ctx,
            config(page),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(page_texts(&pages[0]), vec!["X".to_owned()]);
        let group = page_runs(&pages[1]);
        assert_eq!(group[0].0, "A");
        assert_eq!(group[1].0, "B");
        assert_eq!(group[0].1.to_bits(), metrics.baseline.as_pt().to_bits());
        assert_eq!(
            group[1].1.to_bits(),
            (line_h + metrics.baseline.as_pt()).to_bits()
        );
    }

    #[test]
    fn keep_together_taller_than_frame_errors() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let line_h = metrics.line_height.as_pt();
        let page = Size::try_pt(200.0, line_h + 1.0).unwrap();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        let err = paginate_root(
            FlowNode::KeepTogether(KeepTogetherNode::Open(vec![
                FlowNode::Paragraph(a),
                FlowNode::Paragraph(b),
            ])),
            &mut ctx,
            config(page),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap_err();
        assert_eq!(
            err,
            Error::from(ErrorKind::UnsplittableTallerThanFrame {
                height: line_h * 2.0,
                avail: line_h + 1.0,
            })
            .in_region(Region::Body)
            .on_page_index(0)
            .in_section(0)
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn keep_together_counts_spacing_between_members() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let line_h = metrics.line_height.as_pt();
        let spacing = line_h + 1.0;
        let mut spaced = style;
        spaced.paragraph_spacing = NonNegative::pt(spacing).unwrap();
        let page = Size::try_pt(200.0, line_h * 2.0).unwrap();
        let a = ParagraphNode::layout(&mut fonts, "A", &spaced, 200.0).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &spaced, 200.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        let err = paginate_root(
            FlowNode::KeepTogether(KeepTogetherNode::Open(vec![
                FlowNode::Paragraph(a),
                FlowNode::Paragraph(b),
            ])),
            &mut ctx,
            config(page),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap_err();
        assert_eq!(
            err,
            Error::from(ErrorKind::UnsplittableTallerThanFrame {
                height: line_h + spacing + line_h,
                avail: line_h * 2.0,
            })
            .in_region(Region::Body)
            .on_page_index(0)
            .in_section(0)
        );
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn spacing_after_is_kept_when_it_fits() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 8.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let line_h = metrics.line_height.as_pt();
        let page = Size::try_pt(200.0, line_h * 2.0 + 8.0).unwrap();
        let first = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let second = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Stack(vec![
                FlowNode::Paragraph(first),
                FlowNode::Paragraph(second),
            ]),
            &mut ctx,
            config(page),
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        assert_eq!(rec.pages().len(), 1);
        let runs = page_runs(&rec.pages()[0]);
        assert_eq!(runs[1].0, "B");
        assert_eq!(
            runs[1].1.to_bits(),
            (line_h + 8.0 + metrics.baseline.as_pt()).to_bits()
        );
    }

    #[test]
    fn body_origin_adds_margins() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let para = ParagraphNode::layout(&mut fonts, "A", &style, 180.0).unwrap();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_root(
            FlowNode::Paragraph(para),
            &mut ctx,
            PaginateConfig {
                page_size: Size::try_pt(200.0, 400.0).unwrap(),
                margins: Margins::new(
                    NonNegative::pt(15.0).unwrap(),
                    NonNegative::pt(0.0).unwrap(),
                    NonNegative::pt(0.0).unwrap(),
                    NonNegative::pt(10.0).unwrap(),
                ),
            },
            &Decorations::none(),
            &mut rec,
        )
        .unwrap();
        let origins = glyph_origins(&rec);
        assert_eq!(origins[0].0.to_bits(), 10.0f32.to_bits());
        assert_eq!(
            origins[0].1.to_bits(),
            (15.0 + metrics.baseline.as_pt()).to_bits()
        );
    }

    #[test]
    fn two_sections_keep_page_size_and_margins_when_the_first_page_is_not_full() {
        let Some((mut fonts, style)) = fonts_and_style(TextAlign::Start, 0.0) else {
            return;
        };
        let face = fonts
            .metrics(style.font.expect("test style has a face"))
            .unwrap();
        let metrics = LineMetrics::new(
            FaceExtent::scaled(face, style.font_size).into(),
            style.line_height_factor,
        )
        .unwrap();
        let first = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let second_width = 400.0 - 30.0 - 8.0;
        let second = ParagraphNode::layout(&mut fonts, "B", &style, second_width).unwrap();
        let decorations = Decorations::none();
        let mut errors = crate::error::ErrorBag::default();
        let mut ctx = LayoutCtx {
            natural: false,
            fonts: &mut fonts,
            errors: &mut errors,
        };
        let mut rec = RecordingCanvas::new();
        paginate_sections(
            vec![
                SectionJob {
                    config: PaginateConfig {
                        page_size: Size::try_pt(200.0, 400.0).unwrap(),
                        margins: Margins::default(),
                    },
                    root: FlowNode::Paragraph(first),
                    decorations: &decorations,
                },
                SectionJob {
                    config: PaginateConfig {
                        page_size: Size::try_pt(400.0, 200.0).unwrap(),
                        margins: Margins::new(
                            NonNegative::pt(12.0).unwrap(),
                            NonNegative::pt(8.0).unwrap(),
                            NonNegative::pt(12.0).unwrap(),
                            NonNegative::pt(30.0).unwrap(),
                        ),
                    },
                    root: FlowNode::Paragraph(second),
                    decorations: &decorations,
                },
            ],
            &mut ctx,
            &mut rec,
        )
        .unwrap();
        let pages = rec.pages();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].size(), Size::try_pt(200.0, 400.0).unwrap());
        assert_eq!(pages[1].size(), Size::try_pt(400.0, 200.0).unwrap());
        let origin = |page: &RecordedPage| {
            page.ops().iter().find_map(|op| match op {
                CanvasCommand::DrawGlyphRun { run } => {
                    Some((run.origin.x().as_pt(), run.origin.y().as_pt()))
                }
                _ => None,
            })
        };
        let (first_x, first_y) = origin(&pages[0]).unwrap();
        let (second_x, second_y) = origin(&pages[1]).unwrap();
        assert_eq!(first_x.to_bits(), 0.0f32.to_bits());
        assert_eq!(first_y.to_bits(), metrics.baseline.as_pt().to_bits());
        assert_eq!(second_x.to_bits(), 30.0f32.to_bits());
        assert_eq!(
            second_y.to_bits(),
            (12.0 + metrics.baseline.as_pt()).to_bits()
        );
    }
}
