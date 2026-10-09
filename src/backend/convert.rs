//! Identity mapping from crate geometry to krilla geometry.
//!
//! krilla 0.8.2 `Surface` is already top-left, y-down. Do not pre-flip here.
//! Content-stream and annotation numbers may still differ from API coordinates
//! because krilla applies `page_root_transform` when writing the PDF.

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point as KPoint, Rect as KRect, Size as KSize};
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, FillRule, Stroke as KStroke};

use crate::error::{Error, Result};
use crate::geom::{Paint, Path, PathVerb, Point, Rect, Size, Stroke};

/// Identity point mapping.
pub(crate) fn to_krilla_point(point: Point) -> KPoint {
    KPoint::from_xy(point.x().as_pt(), point.y().as_pt())
}

/// Identity size mapping. `None` only if krilla rejects a positive size.
pub(crate) fn to_krilla_size(size: Size) -> Option<KSize> {
    KSize::from_wh(size.width().as_pt(), size.height().as_pt())
}

/// Identity rect mapping. `None` only if krilla rejects a positive extent.
pub(crate) fn to_krilla_rect(rect: Rect) -> Option<KRect> {
    KRect::from_xywh(
        rect.origin().x().as_pt(),
        rect.origin().y().as_pt(),
        rect.size().width().as_pt(),
        rect.size().height().as_pt(),
    )
}

/// Builds a krilla path. A lone `MoveTo` that krilla rejects is [`Error::InvalidGeometry`].
pub(crate) fn to_krilla_path(path: &Path) -> Result<krilla::geom::Path> {
    let mut builder = PathBuilder::new();
    for verb in path.verbs() {
        match *verb {
            PathVerb::MoveTo(point) => {
                builder.move_to(point.x().as_pt(), point.y().as_pt());
            }
            PathVerb::LineTo(point) => {
                builder.line_to(point.x().as_pt(), point.y().as_pt());
            }
            PathVerb::CubicTo { c1, c2, to } => {
                builder.cubic_to(
                    c1.x().as_pt(),
                    c1.y().as_pt(),
                    c2.x().as_pt(),
                    c2.y().as_pt(),
                    to.x().as_pt(),
                    to.y().as_pt(),
                );
            }
            PathVerb::Close => builder.close(),
        }
    }
    builder.finish().ok_or(Error::InvalidGeometry)
}

/// Solid fill from a crate paint.
pub(crate) fn to_fill(paint: Paint) -> Result<Fill> {
    let (r, g, b, a) = paint.color().channels();
    let opacity = NormalizedF32::new(f32::from(a) / 255.0).ok_or(Error::InvalidGeometry)?;
    Ok(Fill {
        paint: rgb::Color::new(r, g, b).into(),
        opacity,
        rule: FillRule::NonZero,
    })
}

/// Stroke that keeps krilla defaults for cap, join, miter, and dash.
pub(crate) fn to_stroke(paint: Paint, stroke: Stroke) -> Result<KStroke> {
    let (r, g, b, a) = paint.color().channels();
    let opacity = NormalizedF32::new(f32::from(a) / 255.0).ok_or(Error::InvalidGeometry)?;
    Ok(KStroke {
        paint: rgb::Color::new(r, g, b).into(),
        width: stroke.width().as_pt(),
        opacity,
        ..KStroke::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Positive, Rect};

    #[test]
    fn to_krilla_point_is_identity() {
        let point = Point::try_pt(3.0, 4.0).unwrap();
        let got = to_krilla_point(point);
        assert_eq!(got.x.to_bits(), 3.0f32.to_bits());
        assert_eq!(got.y.to_bits(), 4.0f32.to_bits());
    }

    #[test]
    fn to_krilla_size_preserves_positive_edges() {
        let size = Size::try_pt(200.0, 200.0).unwrap();
        let got = to_krilla_size(size).unwrap();
        assert_eq!(got.width().to_bits(), 200.0f32.to_bits());
        assert_eq!(got.height().to_bits(), 200.0f32.to_bits());
    }

    #[test]
    fn to_krilla_rect_is_identity() {
        let rect = Rect::try_pt(10.0, 30.0, 80.0, 44.0).unwrap();
        let got = to_krilla_rect(rect).unwrap();
        assert_eq!(got.left().to_bits(), 10.0f32.to_bits());
        assert_eq!(got.top().to_bits(), 30.0f32.to_bits());
        assert_eq!(got.right().to_bits(), 90.0f32.to_bits());
        assert_eq!(got.bottom().to_bits(), 74.0f32.to_bits());
    }

    #[test]
    fn lone_move_to_path_is_invalid_geometry() {
        let path = Path::begin(Point::try_pt(5.0, 5.0).unwrap()).finish();
        assert!(matches!(to_krilla_path(&path), Err(Error::InvalidGeometry)));
    }

    #[test]
    fn ellipse_cubics_convert() {
        let path = Path::ellipse(Rect::try_pt(1.0, 2.0, 8.0, 6.0).unwrap()).unwrap();
        assert!(to_krilla_path(&path).is_ok());
    }

    #[test]
    fn stroke_width_comes_from_branded_stroke() {
        let paint = Paint::solid(crate::geom::Color::rgb(0, 0, 0));
        let stroke = Stroke::new(Positive::pt(2.5).unwrap());
        let got = to_stroke(paint, stroke).unwrap();
        assert_eq!(got.width.to_bits(), 2.5f32.to_bits());
    }
}
