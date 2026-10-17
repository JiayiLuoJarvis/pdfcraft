//! Shared fixtures for layout tests.

use crate::canvas::{CanvasCommand, RecordedPage, RecordingCanvas};
use crate::error::{ErrorBag, Fallible as Result};
use crate::fonts::{fixture_bytes, FontSource, FontStore};
use crate::geom::{Color, Margins, NonNegative, Path, PathVerb, Positive, Size};
use crate::layout::decorate::Decorations;
use crate::layout::flow::FlowNode;
use crate::layout::frame::LayoutCtx;
use crate::layout::paginate::{paginate_root, PaginateConfig};
use crate::layout::paragraph::TextNode;
use crate::layout::style::{GlyphStyle, Style, TextAlign};
use crate::text::measure::{FaceExtent, LineMetrics};

pub(crate) fn fonts_and_style() -> Option<(FontStore, Style)> {
    let bytes = fixture_bytes()?;
    let mut fonts = FontStore::new();
    let font = fonts.register(FontSource { bytes, index: 0 }).unwrap();
    let style = Style::new(
        font,
        Positive::pt(12.0).unwrap(),
        1.2,
        Color::rgb(0, 0, 0),
        NonNegative::ZERO,
        TextAlign::Start,
    )
    .unwrap();
    Some((fonts, style))
}

pub(crate) fn metrics(fonts: &FontStore, style: &Style) -> LineMetrics {
    let face = fonts
        .metrics(style.font.expect("test style has a face"))
        .unwrap();
    LineMetrics::new(
        FaceExtent::scaled(face, style.font_size).into(),
        style.line_height_factor,
    )
    .unwrap()
}

pub(crate) fn text(s: &str, style: &Style) -> FlowNode {
    FlowNode::Text(TextNode::new(s, GlyphStyle::plain(style), style.clone()))
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

/// A 200 x 400 pt page with equal side margins.
pub(crate) fn margins(top: f32, side: f32, bottom: f32) -> PaginateConfig {
    PaginateConfig {
        page_size: Size::try_pt(200.0, 400.0).unwrap(),
        margins: Margins::new(pt(top), pt(side), pt(bottom), pt(side)),
    }
}

pub(crate) fn render(
    root: FlowNode,
    fonts: &mut FontStore,
    config: PaginateConfig,
    decorations: &Decorations<'_>,
) -> (Result<()>, RecordingCanvas) {
    let mut errors = ErrorBag::default();
    let mut ctx = LayoutCtx {
        natural: false,
        fonts,
        errors: &mut errors,
    };
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

/// Points of every stroked path on `page`. `None` marks `Close`. A cubic contributes its end point.
pub(crate) fn strokes(page: &RecordedPage) -> Vec<Vec<Option<(f32, f32)>>> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::StrokePath { path, .. } => Some(path_points(path)),
            _ => None,
        })
        .collect()
}

/// Points of every filled path on `page`, in the same shape as [`strokes`].
pub(crate) fn fills(page: &RecordedPage) -> Vec<Vec<Option<(f32, f32)>>> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::FillPath { path, .. } => Some(path_points(path)),
            _ => None,
        })
        .collect()
}

/// `(cx, cy, rx, ry)` recovered from a four-cubic ellipse's endpoints.
pub(crate) fn ellipse_center_radii(path: &Path) -> Option<(f32, f32, f32, f32)> {
    let [PathVerb::MoveTo(right), PathVerb::CubicTo { to: bottom, .. }, PathVerb::CubicTo { to: left, .. }, PathVerb::CubicTo { to: top, .. }, PathVerb::CubicTo { .. }, PathVerb::Close] =
        path.verbs()
    else {
        return None;
    };
    let cx = f32::midpoint(right.x().as_pt(), left.x().as_pt());
    let cy = f32::midpoint(bottom.y().as_pt(), top.y().as_pt());
    let rx = (right.x().as_pt() - left.x().as_pt()) * 0.5;
    let ry = (bottom.y().as_pt() - top.y().as_pt()) * 0.5;
    Some((cx, cy, rx, ry))
}

fn path_points(path: &Path) -> Vec<Option<(f32, f32)>> {
    path.verbs()
        .iter()
        .map(|verb| match verb {
            PathVerb::MoveTo(point) | PathVerb::LineTo(point) => {
                Some((point.x().as_pt(), point.y().as_pt()))
            }
            PathVerb::CubicTo { to, .. } => Some((to.x().as_pt(), to.y().as_pt())),
            PathVerb::Close => None,
        })
        .collect()
}
