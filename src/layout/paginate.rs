//! Page breaking and page assembly over the flow.

#![allow(dead_code)]

use crate::canvas::Canvas;
use crate::error::{Error, Result};
use crate::geom::{Margins, Point, Size};
use crate::layout::decorate::{measure_bands, Decorations, PageBands};
use crate::layout::flow::{draw_fragment, FlowNode, Frame, LayoutCtx, PlacedFragment, Split};

/// Page geometry for pagination. Header and footer bands sit inside the margins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PaginateConfig {
    pub(crate) page_size: Size,
    pub(crate) margins: Margins,
}

impl PaginateConfig {
    pub(crate) fn body_width(self) -> f32 {
        self.page_size.width().as_pt() - self.margins.left().as_pt() - self.margins.right().as_pt()
    }

    pub(crate) fn body_height(self) -> f32 {
        self.page_size.height().as_pt() - self.margins.top().as_pt() - self.margins.bottom().as_pt()
    }

    pub(crate) fn body_origin(self) -> Result<Point> {
        Point::try_pt(self.margins.left().as_pt(), self.margins.top().as_pt())
    }

    fn validate(self) -> Result<()> {
        let w = self.body_width();
        let h = self.body_height();
        if !w.is_finite() || w <= 0.0 || !h.is_finite() || h <= 0.0 {
            return Err(Error::InvalidGeometry);
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

/// Paginate `root` and paint onto `canvas`.
///
/// Bands are measured after the body plan, so decorations can never move body content.
pub(crate) fn paginate_root(
    root: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    config: PaginateConfig,
    decorations: &Decorations,
    canvas: &mut dyn Canvas,
) -> Result<()> {
    let pages = plan_pages(root, ctx, config)?;
    let sizes: Vec<Size> = pages.iter().map(|page| page.size).collect();
    let bands = measure_bands(&sizes, config.margins, decorations, ctx)?;
    paint_pages(&pages, &bands, canvas)
}

fn plan_pages(
    root: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    config: PaginateConfig,
) -> Result<Vec<PagePlan>> {
    config.validate()?;
    let mut nodes = match root {
        FlowNode::Stack(kids) => kids,
        other => vec![other],
    };

    let body_w = config.body_width();
    let body_h = config.body_height();
    let body_origin = config.body_origin()?;

    let mut cursor = PageCursor::Fresh;
    let mut pages: Vec<PagePlan> = Vec::new();
    let mut current: Option<PagePlan> = None;
    let mut y = 0.0f32;

    while !nodes.is_empty() {
        let node = nodes.remove(0);
        match node {
            FlowNode::PageBreak => {
                cursor = apply_page_break(cursor, &mut pages, &mut current, config);
                y = 0.0;
            }
            node => {
                ensure_content_page(&mut current, config);
                let avail = Frame {
                    width: body_w,
                    height: (body_h - y).max(0.0),
                    full_height: body_h,
                };
                match node.split(ctx, avail)? {
                    Split::Fit { height, fragment } => {
                        let page = current.as_mut().expect("content page open");
                        let origin =
                            Point::try_pt(body_origin.x().as_pt(), body_origin.y().as_pt() + y)?;
                        let spacing = fragment.spacing_after;
                        page.placed.push(PlacedFragment { origin, fragment });
                        y += height;
                        if spacing > 0.0 {
                            let remain = (body_h - y).max(0.0);
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
                        let origin =
                            Point::try_pt(body_origin.x().as_pt(), body_origin.y().as_pt() + y)?;
                        page.placed.push(PlacedFragment {
                            origin,
                            fragment: head,
                        });
                        commit_page(&mut pages, &mut current);
                        nodes.insert(0, tail);
                        y = 0.0;
                        cursor = PageCursor::Fresh;
                    }
                    Split::MoveToNextPage(tail) => {
                        if y == 0.0 {
                            return Err(Error::UnsplittableTallerThanFrame);
                        }
                        commit_page(&mut pages, &mut current);
                        nodes.insert(0, tail);
                        y = 0.0;
                        cursor = PageCursor::Fresh;
                    }
                }
            }
        }
    }

    finalize_eof(cursor, &mut pages, &mut current, config);
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
) -> PageCursor {
    match cursor {
        PageCursor::Fresh => PageCursor::Fresh,
        PageCursor::HasBody => {
            commit_page(pages, current);
            PageCursor::PendingBlank
        }
        PageCursor::PendingBlank => {
            pages.push(PagePlan {
                size: config.page_size,
                placed: Vec::new(),
            });
            PageCursor::PendingBlank
        }
    }
}

fn commit_page(pages: &mut Vec<PagePlan>, current: &mut Option<PagePlan>) {
    if let Some(page) = current.take() {
        pages.push(page);
    }
}

fn finalize_eof(
    cursor: PageCursor,
    pages: &mut Vec<PagePlan>,
    current: &mut Option<PagePlan>,
    config: PaginateConfig,
) {
    match cursor {
        PageCursor::Fresh => {
            if pages.is_empty() {
                pages.push(PagePlan {
                    size: config.page_size,
                    placed: Vec::new(),
                });
            }
        }
        PageCursor::HasBody => {
            commit_page(pages, current);
        }
        PageCursor::PendingBlank => {
            current.take();
        }
    }
}

fn paint_pages(pages: &[PagePlan], bands: &[PageBands], canvas: &mut dyn Canvas) -> Result<()> {
    for (page, bands) in pages.iter().zip(bands) {
        canvas.start_page(page.size)?;
        let body = page.placed.iter();
        for placed in bands.header.iter().chain(body).chain(&bands.footer) {
            draw_fragment(&placed.fragment, canvas, placed.origin)?;
        }
        canvas.finish_page()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{CanvasCommand, RecordedPage, RecordingCanvas};
    use crate::fonts::{fixture_bytes, FontSource, FontStore};
    use crate::geom::{Color, NonNegative, Positive, A4};
    use crate::layout::flow::{KeepTogetherNode, SpacerNode};
    use crate::layout::paragraph::ParagraphNode;
    use crate::layout::style::{Style, TextAlign};
    use crate::text::measure::line_metrics;

    fn fonts_and_style(align: TextAlign, spacing: f32) -> (FontStore, Style) {
        let mut fonts = FontStore::new();
        let font = fonts
            .register(FontSource {
                bytes: fixture_bytes(),
                index: 0,
            })
            .unwrap();
        let style = Style::new(
            font,
            Positive::pt(12.0).unwrap(),
            1.2,
            Color::rgb(0, 0, 0),
            NonNegative::pt(spacing).unwrap(),
            align,
        )
        .unwrap();
        (fonts, style)
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
    fn start_center_end_x_and_baseline_y() {
        let body_w = 200.0f32;
        let page = Size::try_pt(body_w, 400.0).unwrap();
        for align in [TextAlign::Start, TextAlign::Center, TextAlign::End] {
            let (mut fonts, style) = fonts_and_style(align, 0.0);
            let para = ParagraphNode::layout(&mut fonts, "AB", &style, body_w).unwrap();
            let face = fonts.metrics(style.font).unwrap();
            let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
            let line_width = {
                // Recompute via first run advance.
                let run = &para.lines[0].run;
                run.glyphs
                    .iter()
                    .map(|g| g.advance_norm * style.font_size.as_pt())
                    .sum::<f32>()
            };
            let expected_x = match align {
                TextAlign::Start => 0.0,
                TextAlign::Center => (body_w - line_width) / 2.0,
                TextAlign::End => body_w - line_width,
            };
            let expected_y = metrics.baseline;

            let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        let (mut fonts, _) = fonts_and_style(TextAlign::Start, 0.0);
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        let (mut fonts, _) = fonts_and_style(TextAlign::Start, 0.0);
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let para = ParagraphNode::layout(&mut fonts, "A", &style, A4.width().as_pt()).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let para = ParagraphNode::layout(&mut fonts, "A", &style, A4.width().as_pt()).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let w = A4.width().as_pt();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, w).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, w).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let w = A4.width().as_pt();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, w).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, w).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let face = fonts.metrics(style.font).unwrap();
        let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
        let para = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let page = Size::try_pt(200.0, 50.0).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        assert_eq!(runs[0].1.to_bits(), (30.0 + metrics.baseline).to_bits());
    }

    #[test]
    fn paragraph_resumes_at_top_of_next_page() {
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let face = fonts.metrics(style.font).unwrap();
        let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
        let line_h = metrics.line_height;
        let page = Size::try_pt(200.0, line_h * 2.0).unwrap();
        let para = ParagraphNode::layout(&mut fonts, "A\nB\nC", &style, 200.0).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        assert_eq!(first[0].1.to_bits(), metrics.baseline.to_bits());
        assert_eq!(first[1].1.to_bits(), (line_h + metrics.baseline).to_bits());
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].0, "C");
        assert_eq!(second[0].1.to_bits(), metrics.baseline.to_bits());
    }

    #[test]
    fn spacing_after_dropped_when_no_room() {
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 20.0);
        let face = fonts.metrics(style.font).unwrap();
        let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
        let line_h = metrics.line_height;
        // Body fits exactly one line; spacing_after 20 cannot fit.
        let page = Size::try_pt(200.0, line_h).unwrap();
        let first = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let second = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        assert_eq!(origins[1].1.to_bits(), metrics.baseline.to_bits());
    }

    #[test]
    fn keep_together_moves_to_next_page() {
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let face = fonts.metrics(style.font).unwrap();
        let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
        let line_h = metrics.line_height;
        // Two lines fit in the full frame. One leading line leaves only one line of room.
        let page = Size::try_pt(200.0, line_h * 2.0 + 10.0).unwrap();
        let filler = ParagraphNode::layout(&mut fonts, "X", &style, 200.0).unwrap();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        assert_eq!(group[0].1.to_bits(), metrics.baseline.to_bits());
        assert_eq!(group[1].1.to_bits(), (line_h + metrics.baseline).to_bits());
    }

    #[test]
    fn keep_together_taller_than_frame_errors() {
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let face = fonts.metrics(style.font).unwrap();
        let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
        let line_h = metrics.line_height;
        let page = Size::try_pt(200.0, line_h + 1.0).unwrap();
        let a = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let b = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        assert!(matches!(err, Error::UnsplittableTallerThanFrame));
        assert!(rec.commands().is_empty());
    }

    #[test]
    fn spacing_after_is_kept_when_it_fits() {
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 8.0);
        let face = fonts.metrics(style.font).unwrap();
        let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
        let line_h = metrics.line_height;
        let page = Size::try_pt(200.0, line_h * 2.0 + 8.0).unwrap();
        let first = ParagraphNode::layout(&mut fonts, "A", &style, 200.0).unwrap();
        let second = ParagraphNode::layout(&mut fonts, "B", &style, 200.0).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
            (line_h + 8.0 + metrics.baseline).to_bits()
        );
    }

    #[test]
    fn body_origin_adds_margins() {
        let (mut fonts, style) = fonts_and_style(TextAlign::Start, 0.0);
        let face = fonts.metrics(style.font).unwrap();
        let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
        let para = ParagraphNode::layout(&mut fonts, "A", &style, 180.0).unwrap();
        let mut ctx = LayoutCtx { fonts: &mut fonts };
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
        assert_eq!(origins[0].1.to_bits(), (15.0 + metrics.baseline).to_bits());
    }
}
