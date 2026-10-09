//! Shared fixtures for layout tests.

use crate::canvas::{CanvasCommand, RecordedPage, RecordingCanvas};
use crate::error::Result;
use crate::fonts::{fixture_bytes, FontSource, FontStore};
use crate::geom::{Color, Margins, NonNegative, PathVerb, Positive, Size};
use crate::layout::decorate::Decorations;
use crate::layout::flow::{FlowNode, LayoutCtx};
use crate::layout::paginate::{paginate_root, PaginateConfig};
use crate::layout::paragraph::TextNode;
use crate::layout::style::{Style, TextAlign};
use crate::text::measure::{line_metrics, LineMetrics};

pub(crate) fn fonts_and_style() -> (FontStore, Style) {
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
        NonNegative::ZERO,
        TextAlign::Start,
    )
    .unwrap();
    (fonts, style)
}

pub(crate) fn metrics(fonts: &FontStore, style: &Style) -> LineMetrics {
    let face = fonts.metrics(style.font).unwrap();
    line_metrics(face, style.font_size, style.line_height_factor).unwrap()
}

pub(crate) fn text(s: &str, style: &Style) -> FlowNode {
    FlowNode::Text(TextNode::new(s, style.clone()))
}

pub(crate) fn pt(value: f32) -> crate::geom::Length<NonNegative> {
    NonNegative::pt(value).unwrap()
}

pub(crate) fn page(width: f32, height: f32) -> PaginateConfig {
    PaginateConfig {
        page_size: Size::try_pt(width, height).unwrap(),
        margins: Margins::default(),
    }
}

pub(crate) fn render(
    root: FlowNode,
    fonts: &mut FontStore,
    config: PaginateConfig,
    decorations: &Decorations<'_>,
) -> (Result<()>, RecordingCanvas) {
    let mut ctx = LayoutCtx { fonts };
    let mut rec = RecordingCanvas::new();
    let result = paginate_root(root, &mut ctx, config, decorations, &mut rec);
    (result, rec)
}

/// `(text, x, y)` of every glyph run on `page`, in draw order.
pub(crate) fn runs(page: &RecordedPage) -> Vec<(String, f32, f32)> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some((
                run.text.clone(),
                run.origin.x().as_pt(),
                run.origin.y().as_pt(),
            )),
            _ => None,
        })
        .collect()
}

pub(crate) fn texts(page: &RecordedPage) -> Vec<String> {
    runs(page).into_iter().map(|(text, _, _)| text).collect()
}

/// Bit patterns of a run, for exact coordinate comparison.
pub(crate) fn bits(run: &(String, f32, f32)) -> (&str, u32, u32) {
    (run.0.as_str(), run.1.to_bits(), run.2.to_bits())
}

/// Points of every stroked path on `page`; `None` marks `Close`.
pub(crate) fn strokes(page: &RecordedPage) -> Vec<Vec<Option<(f32, f32)>>> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::StrokePath { path, .. } => Some(
                path.verbs()
                    .iter()
                    .map(|verb| match verb {
                        PathVerb::MoveTo(p) | PathVerb::LineTo(p) => {
                            Some((p.x().as_pt(), p.y().as_pt()))
                        }
                        PathVerb::Close => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}
