//! Geometry and paint values for layout and canvas.
//!
//! Coordinates use the PDF point (1/72 inch). The origin is the top-left
//! corner, x increases to the right, and y increases downward.

use std::cmp::Ordering;
use std::marker::PhantomData;

use crate::error::{Error, Result};

/// PDF points in one millimeter (`72 / 25.4`).
pub const MM_TO_PT: f32 = 72.0 / 25.4;

/// A length in PDF points, branded by the constraint `C`.
///
/// `Finite`, `NonNegative`, and `Positive` are the brands. Widening
/// conversions go only from a stricter brand to a looser one.
#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
#[must_use]
pub struct Length<C> {
    pt: f32,
    brand: PhantomData<fn() -> C>,
}

impl<C> PartialEq for Length<C> {
    fn eq(&self, other: &Self) -> bool {
        #[allow(
            clippy::float_cmp,
            reason = "NaN never reaches a length, so IEEE equality is an equivalence"
        )]
        {
            self.pt == other.pt
        }
    }
}

impl<C> Eq for Length<C> {}

impl<C> PartialOrd for Length<C> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.pt.partial_cmp(&other.pt)
    }
}

impl<C> Length<C> {
    /// Builds a length from a point value that already satisfies `C`.
    ///
    /// Only for compile-time constants such as [`A4`]. Runtime values must
    /// go through a brand factory.
    const fn from_trusted_pt(pt: f32) -> Self {
        Self {
            pt,
            brand: PhantomData,
        }
    }

    /// The length in PDF points.
    #[must_use]
    pub const fn as_pt(self) -> f32 {
        self.pt
    }
}

/// Brand for any finite point value, including negatives and zero.
#[derive(Debug, Clone, Copy)]
pub enum Finite {}

/// Brand for a finite length greater than or equal to zero.
#[derive(Debug, Clone, Copy)]
pub enum NonNegative {}

/// Brand for a finite length strictly greater than zero.
#[derive(Debug, Clone, Copy)]
pub enum Positive {}

const fn finite_pt(value: f32) -> Result<f32> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::NonFiniteLength { value })
    }
}

fn pt_from_mm(mm: f32) -> Result<f32> {
    let mm = finite_pt(mm)?;
    finite_pt(mm * MM_TO_PT)
}

impl Finite {
    /// Accepts a finite point value.
    pub fn pt(value: f32) -> Result<Length<Self>> {
        Ok(Length::from_trusted_pt(finite_pt(value)?))
    }

    /// Accepts a finite millimeter value and stores the point equivalent.
    pub fn mm(value: f32) -> Result<Length<Self>> {
        Ok(Length::from_trusted_pt(pt_from_mm(value)?))
    }
}

impl NonNegative {
    /// Zero length.
    pub const ZERO: Length<Self> = Length::from_trusted_pt(0.0);

    /// Accepts a finite point value greater than or equal to zero.
    pub fn pt(value: f32) -> Result<Length<Self>> {
        let value = finite_pt(value)?;
        if value < 0.0 {
            return Err(Error::NegativeLength { value });
        }
        Ok(Length::from_trusted_pt(value))
    }

    /// Accepts a finite millimeter value greater than or equal to zero.
    pub fn mm(value: f32) -> Result<Length<Self>> {
        let mm = finite_pt(value)?;
        if mm < 0.0 {
            return Err(Error::NegativeLength { value: mm });
        }
        let pt = finite_pt(mm * MM_TO_PT)?;
        if pt < 0.0 {
            return Err(Error::NegativeLength { value: pt });
        }
        Ok(Length::from_trusted_pt(pt))
    }
}

impl Positive {
    /// Accepts a finite point value strictly greater than zero.
    pub fn pt(value: f32) -> Result<Length<Self>> {
        let value = finite_pt(value)?;
        if value <= 0.0 {
            return Err(Error::NonPositiveLength { value });
        }
        Ok(Length::from_trusted_pt(value))
    }

    /// Accepts a finite millimeter value strictly greater than zero.
    pub fn mm(value: f32) -> Result<Length<Self>> {
        let mm = finite_pt(value)?;
        if mm <= 0.0 {
            return Err(Error::NonPositiveLength { value: mm });
        }
        let pt = finite_pt(mm * MM_TO_PT)?;
        if pt <= 0.0 {
            return Err(Error::NonPositiveLength { value: pt });
        }
        Ok(Length::from_trusted_pt(pt))
    }
}

impl From<Length<Positive>> for Length<NonNegative> {
    fn from(value: Length<Positive>) -> Self {
        Self::from_trusted_pt(value.pt)
    }
}

impl From<Length<Positive>> for Length<Finite> {
    fn from(value: Length<Positive>) -> Self {
        Self::from_trusted_pt(value.pt)
    }
}

impl From<Length<NonNegative>> for Length<Finite> {
    fn from(value: Length<NonNegative>) -> Self {
        Self::from_trusted_pt(value.pt)
    }
}

/// A position in page space. Both axes are finite and may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Point {
    x: Length<Finite>,
    y: Length<Finite>,
}

impl Point {
    /// The origin, `(0, 0)`.
    pub const ZERO: Self = Self::new(Length::from_trusted_pt(0.0), Length::from_trusted_pt(0.0));

    /// Builds a point from lengths that are already finite.
    pub const fn new(x: Length<Finite>, y: Length<Finite>) -> Self {
        Self { x, y }
    }

    /// Builds a point from finite point values.
    pub fn try_pt(x: f32, y: f32) -> Result<Self> {
        Ok(Self::new(Finite::pt(x)?, Finite::pt(y)?))
    }

    /// Builds a point from finite millimeter values.
    pub fn try_mm(x: f32, y: f32) -> Result<Self> {
        Ok(Self::new(Finite::mm(x)?, Finite::mm(y)?))
    }

    /// Horizontal position.
    pub const fn x(self) -> Length<Finite> {
        self.x
    }

    /// Vertical position, increasing downward.
    pub const fn y(self) -> Length<Finite> {
        self.y
    }
}

/// A width and a height. Both are strictly positive, so a size is never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Size {
    width: Length<Positive>,
    height: Length<Positive>,
}

impl Size {
    const fn from_trusted_pt(width: f32, height: f32) -> Self {
        Self {
            width: Length::from_trusted_pt(width),
            height: Length::from_trusted_pt(height),
        }
    }

    /// Builds a size from lengths that are already positive.
    pub const fn new(width: Length<Positive>, height: Length<Positive>) -> Self {
        Self { width, height }
    }

    /// Builds a size from positive point values.
    pub fn try_pt(width: f32, height: f32) -> Result<Self> {
        Ok(Self::new(Positive::pt(width)?, Positive::pt(height)?))
    }

    /// Builds a size from positive millimeter values.
    pub fn try_mm(width: f32, height: f32) -> Result<Self> {
        Ok(Self::new(Positive::mm(width)?, Positive::mm(height)?))
    }

    /// Horizontal extent.
    pub const fn width(self) -> Length<Positive> {
        self.width
    }

    /// Vertical extent.
    pub const fn height(self) -> Length<Positive> {
        self.height
    }
}

/// ISO A4, 210 mm by 297 mm, converted with [`MM_TO_PT`].
pub const A4: Size = Size::from_trusted_pt(210.0 * MM_TO_PT, 297.0 * MM_TO_PT);

/// US Letter, 612 pt by 792 pt.
pub const LETTER: Size = Size::from_trusted_pt(612.0, 792.0);

/// An axis-aligned rectangle. The extent is a [`Size`], so it is never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Rect {
    origin: Point,
    size: Size,
}

impl Rect {
    /// Builds a rectangle from an origin and a size that are already valid.
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// Builds a rectangle from point values. Width and height must be positive.
    pub fn try_pt(x: f32, y: f32, width: f32, height: f32) -> Result<Self> {
        Ok(Self::new(
            Point::try_pt(x, y)?,
            Size::try_pt(width, height)?,
        ))
    }

    /// Builds a rectangle from millimeter values. Width and height must be positive.
    pub fn try_mm(x: f32, y: f32, width: f32, height: f32) -> Result<Self> {
        Ok(Self::new(
            Point::try_mm(x, y)?,
            Size::try_mm(width, height)?,
        ))
    }

    /// Top-left corner.
    pub const fn origin(self) -> Point {
        self.origin
    }

    /// Width and height.
    pub const fn size(self) -> Size {
        self.size
    }

    pub(crate) fn translated(self, dx: f32, dy: f32) -> Result<Self> {
        Ok(Self::new(
            Point::try_pt(self.origin.x().as_pt() + dx, self.origin.y().as_pt() + dy)?,
            self.size,
        ))
    }
}

/// Four page margins. Each side is finite and at least zero.
///
/// [`Default`] is four zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Margins {
    top: Length<NonNegative>,
    right: Length<NonNegative>,
    bottom: Length<NonNegative>,
    left: Length<NonNegative>,
}

impl Margins {
    /// Builds margins from lengths that are already non-negative.
    pub const fn new(
        top: Length<NonNegative>,
        right: Length<NonNegative>,
        bottom: Length<NonNegative>,
        left: Length<NonNegative>,
    ) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// The same length on every side.
    pub const fn uniform(value: Length<NonNegative>) -> Self {
        Self::new(value, value, value, value)
    }

    /// Top margin.
    pub const fn top(self) -> Length<NonNegative> {
        self.top
    }

    /// Right margin.
    pub const fn right(self) -> Length<NonNegative> {
        self.right
    }

    /// Bottom margin.
    pub const fn bottom(self) -> Length<NonNegative> {
        self.bottom
    }

    /// Left margin.
    pub const fn left(self) -> Length<NonNegative> {
        self.left
    }
}

impl Default for Margins {
    fn default() -> Self {
        Self::uniform(NonNegative::ZERO)
    }
}

/// An 8-bit RGBA color. Channels are already in range, so construction is total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl Color {
    /// Opaque color. Alpha is 255.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    /// Color with an explicit alpha channel.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// `(red, green, blue, alpha)`.
    #[must_use]
    pub const fn channels(self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }
}

/// A solid fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Paint {
    color: Color,
}

impl Paint {
    /// Solid paint of `color`.
    pub const fn solid(color: Color) -> Self {
        Self { color }
    }

    /// The fill color.
    pub const fn color(self) -> Color {
        self.color
    }
}

/// A stroke whose width is strictly positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Stroke {
    width: Length<Positive>,
}

impl Stroke {
    /// Builds a stroke from a width that is already positive.
    pub const fn new(width: Length<Positive>) -> Self {
        Self { width }
    }

    /// Stroke width.
    pub const fn width(self) -> Length<Positive> {
        self.width
    }
}

/// One command in a [`Path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathVerb {
    /// Moves the pen without drawing.
    MoveTo(Point),
    /// Draws a straight segment.
    LineTo(Point),
    /// Closes the current subpath back to its most recent `MoveTo`.
    Close,
}

/// A non-empty path. The first verb is [`PathVerb::MoveTo`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Path {
    verbs: Box<[PathVerb]>,
}

/// Consuming builder returned by [`Path::begin`].
#[derive(Debug)]
#[must_use]
pub struct PathBuilder {
    verbs: Vec<PathVerb>,
}

impl Path {
    /// Starts a path whose only verb is `MoveTo(at)`.
    pub fn begin(at: Point) -> PathBuilder {
        PathBuilder {
            verbs: vec![PathVerb::MoveTo(at)],
        }
    }

    /// A single straight segment from `from` to `to`.
    pub fn line(from: Point, to: Point) -> Self {
        Self::begin(from).line_to(to).finish()
    }

    /// The verbs in order. The slice is never empty and starts with `MoveTo`.
    #[must_use]
    pub fn verbs(&self) -> &[PathVerb] {
        &self.verbs
    }

    pub(crate) fn translated(&self, dx: f32, dy: f32) -> Result<Self> {
        let shift = |p: Point| Point::try_pt(p.x().as_pt() + dx, p.y().as_pt() + dy);
        let verbs = self
            .verbs
            .iter()
            .map(|verb| {
                Ok(match *verb {
                    PathVerb::MoveTo(p) => PathVerb::MoveTo(shift(p)?),
                    PathVerb::LineTo(p) => PathVerb::LineTo(shift(p)?),
                    PathVerb::Close => PathVerb::Close,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            verbs: verbs.into_boxed_slice(),
        })
    }
}

impl PathBuilder {
    /// Appends a line and returns the builder.
    pub fn line_to(mut self, point: Point) -> Self {
        self.verbs.push(PathVerb::LineTo(point));
        self
    }

    /// Starts another open subpath.
    pub fn move_to(mut self, point: Point) -> Self {
        self.verbs.push(PathVerb::MoveTo(point));
        self
    }

    /// Finishes the path without appending `Close`.
    pub fn finish(self) -> Path {
        Path {
            verbs: self.verbs.into_boxed_slice(),
        }
    }

    /// Appends `Close` and finishes the path.
    pub fn close(mut self) -> Path {
        self.verbs.push(PathVerb::Close);
        self.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs_err_pt(actual: f32, expected: f64) -> f64 {
        (f64::from(actual) - expected).abs()
    }

    #[test]
    fn mm_to_pt_converts_an_inch_and_a4_stays_near_the_f64_formula() {
        assert!(abs_err_pt(MM_TO_PT, 72.0 / 25.4) < 1e-3);

        let inch = Finite::mm(25.4).unwrap();
        assert!(abs_err_pt(inch.as_pt(), 72.0) < 1e-3);

        let width = 210.0_f64 * 72.0 / 25.4;
        let height = 297.0_f64 * 72.0 / 25.4;
        assert!(abs_err_pt(A4.width().as_pt(), width) < 1e-3);
        assert!(abs_err_pt(A4.height().as_pt(), height) < 1e-3);
        assert_eq!(A4, Size::try_mm(210.0, 297.0).unwrap());
    }

    #[test]
    fn letter_is_exactly_612_by_792() {
        assert_eq!(LETTER.width().as_pt().to_bits(), 612.0f32.to_bits());
        assert_eq!(LETTER.height().as_pt().to_bits(), 792.0f32.to_bits());
    }

    #[test]
    fn finite_rejects_nan_and_infinity() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(matches!(
                Finite::pt(value),
                Err(Error::NonFiniteLength { .. })
            ));
            assert!(matches!(
                Finite::mm(value),
                Err(Error::NonFiniteLength { .. })
            ));
            assert!(matches!(
                NonNegative::pt(value),
                Err(Error::NonFiniteLength { .. })
            ));
            assert!(matches!(
                Positive::pt(value),
                Err(Error::NonFiniteLength { .. })
            ));
        }
    }

    #[test]
    fn non_negative_rejects_negative_and_accepts_zero() {
        match NonNegative::pt(-1.5) {
            Err(Error::NegativeLength { value }) => {
                assert_eq!(value.to_bits(), (-1.5f32).to_bits());
            }
            other => panic!("expected NegativeLength, got {other:?}"),
        }
        assert!(matches!(
            NonNegative::mm(-1.0),
            Err(Error::NegativeLength { .. })
        ));
        assert_eq!(NonNegative::pt(0.0).unwrap(), NonNegative::ZERO);
        assert_eq!(NonNegative::mm(0.0).unwrap(), NonNegative::ZERO);
    }

    #[test]
    fn positive_rejects_zero_and_negative() {
        match Positive::pt(0.0) {
            Err(Error::NonPositiveLength { value }) => {
                assert_eq!(value.to_bits(), 0.0f32.to_bits());
            }
            other => panic!("expected NonPositiveLength, got {other:?}"),
        }
        assert!(matches!(
            Positive::pt(-0.25),
            Err(Error::NonPositiveLength { .. })
        ));
        assert!(matches!(
            Positive::mm(0.0),
            Err(Error::NonPositiveLength { .. })
        ));
        assert!(matches!(
            Positive::mm(-3.0),
            Err(Error::NonPositiveLength { .. })
        ));
        assert!(Positive::pt(f32::MIN_POSITIVE).is_ok());
    }

    #[test]
    fn default_margins_are_zero() {
        let margins = Margins::default();
        assert_eq!(margins.top(), NonNegative::ZERO);
        assert_eq!(margins.right(), NonNegative::ZERO);
        assert_eq!(margins.bottom(), NonNegative::ZERO);
        assert_eq!(margins.left(), NonNegative::ZERO);
    }

    #[test]
    fn point_zero_is_the_origin() {
        assert_eq!(Point::ZERO.x(), Finite::pt(0.0).unwrap());
        assert_eq!(Point::ZERO.y(), Finite::pt(0.0).unwrap());
    }

    #[test]
    fn path_begin_is_non_empty_and_starts_with_move_to() {
        let start = Point::try_pt(1.0, 2.0).unwrap();
        let next = Point::try_pt(4.0, 5.0).unwrap();
        let begun = Path::begin(start).finish();
        assert!(matches!(begun.verbs(), [PathVerb::MoveTo(point)] if *point == start));

        let segment = Path::line(start, next);
        assert!(matches!(
            segment.verbs(),
            [PathVerb::MoveTo(from), PathVerb::LineTo(to)] if *from == start && *to == next
        ));

        let closed = Path::begin(start).line_to(next).close();
        assert!(matches!(
            closed.verbs(),
            [PathVerb::MoveTo(_), PathVerb::LineTo(_), PathVerb::Close]
        ));
    }

    #[test]
    fn stroke_rejects_zero_through_positive() {
        assert!(matches!(
            Positive::pt(0.0),
            Err(Error::NonPositiveLength { .. })
        ));
        let width = Positive::pt(0.75).unwrap();
        let stroke = Stroke::new(width);
        assert_eq!(stroke.width(), width);
    }

    #[test]
    fn positive_widens_only_toward_looser_brands() {
        let positive = Positive::pt(2.5).unwrap();
        let non_negative = Length::<NonNegative>::from(positive);
        let finite = Length::<Finite>::from(positive);
        let finite_from_non_negative = Length::<Finite>::from(non_negative);
        assert_eq!(non_negative.as_pt().to_bits(), positive.as_pt().to_bits());
        assert_eq!(finite.as_pt().to_bits(), positive.as_pt().to_bits());
        assert_eq!(
            finite_from_non_negative.as_pt().to_bits(),
            positive.as_pt().to_bits()
        );
    }

    #[test]
    fn rect_reports_origin_and_extent_in_points() {
        let rect = Rect::try_pt(1.0, 2.0, 3.0, 4.0).unwrap();
        assert_eq!(rect.origin().x().as_pt().to_bits(), 1.0f32.to_bits());
        assert_eq!(rect.origin().y().as_pt().to_bits(), 2.0f32.to_bits());
        assert_eq!(rect.size().width().as_pt().to_bits(), 3.0f32.to_bits());
        assert_eq!(rect.size().height().as_pt().to_bits(), 4.0f32.to_bits());
    }

    #[test]
    fn length_errors_include_the_rejected_number() {
        assert_eq!(
            Finite::pt(f32::NAN).unwrap_err().to_string(),
            "NaN is not a finite length"
        );
        assert_eq!(
            NonNegative::mm(-1.0).unwrap_err().to_string(),
            "-1 is a negative length"
        );
        assert_eq!(
            Positive::pt(0.0).unwrap_err().to_string(),
            "0 is not a positive length"
        );
    }
}
