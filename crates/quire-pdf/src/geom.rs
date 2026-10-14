//! Geometry and paint values for layout and canvas.
//!
//! # Coordinates
//!
//! - Unit: PDF point (`1/72` inch). See [`MM_TO_PT`] for millimeters.
//! - Origin: top-left of the page.
//! - Axes: `x` right, `y` **down**.
//!
//! # Lengths
//!
//! [`Length`] is branded with [`Finite`], [`NonNegative`], or [`Positive`] so
//! invalid numbers fail at construction. Page setup uses [`PageSize`],
//! [`Margins`], and optional [`MarginPreset`]. Drawing uses [`Color`],
//! [`Paint`], [`Stroke`], [`Path`] / [`PathBuilder`].

use std::cmp::Ordering;
use std::marker::PhantomData;

use crate::error::{Error, ErrorKind};

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
    /// Constants such as [`A4`] use this. Layout also uses it after arithmetic
    /// that preserves the brand, such as a sum of non-negative lengths.
    pub(crate) const fn from_trusted_pt(pt: f32) -> Self {
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

    pub(crate) fn min(self, other: Self) -> Self {
        match self.partial_cmp(&other) {
            Some(std::cmp::Ordering::Greater) => other,
            _ => self,
        }
    }

    pub(crate) fn max(self, other: Self) -> Self {
        match self.partial_cmp(&other) {
            Some(std::cmp::Ordering::Less) => other,
            _ => self,
        }
    }
}

/// Brand for any finite point value, including negatives and zero.
#[derive(Debug, Clone, Copy)]
pub enum Finite {}

impl Finite {
    pub(crate) const ZERO: Length<Self> = Length::from_trusted_pt(0.0);
}

/// Brand for a finite length greater than or equal to zero.
#[derive(Debug, Clone, Copy)]
pub enum NonNegative {}

/// Brand for a finite length strictly greater than zero.
#[derive(Debug, Clone, Copy)]
pub enum Positive {}

fn finite_pt(value: f32) -> std::result::Result<f32, Error> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::from(ErrorKind::NonFiniteLength { value }))
    }
}

fn pt_from_mm(mm: f32) -> std::result::Result<f32, Error> {
    let mm = finite_pt(mm)?;
    finite_pt(mm * MM_TO_PT)
}

impl Finite {
    /// Accepts a finite point value.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `value` is NaN or infinite.
    pub fn pt(value: f32) -> std::result::Result<Length<Self>, Error> {
        Ok(Length::from_trusted_pt(finite_pt(value)?))
    }

    /// Accepts a finite millimeter value and stores the point equivalent.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `value` or its point equivalent is NaN or infinite.
    pub fn mm(value: f32) -> std::result::Result<Length<Self>, Error> {
        Ok(Length::from_trusted_pt(pt_from_mm(value)?))
    }
}

impl NonNegative {
    /// Zero length.
    pub const ZERO: Length<Self> = Length::from_trusted_pt(0.0);

    /// Accepts a finite point value greater than or equal to zero.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `value` is NaN or infinite.
    /// [`ErrorKind::NegativeLength`] when `value` is negative.
    pub fn pt(value: f32) -> std::result::Result<Length<Self>, Error> {
        let value = finite_pt(value)?;
        if value < 0.0 {
            return Err(Error::from(ErrorKind::NegativeLength { value }));
        }
        Ok(Length::from_trusted_pt(value))
    }

    /// Accepts a finite millimeter value greater than or equal to zero.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `value` or its point equivalent is NaN or infinite.
    /// [`ErrorKind::NegativeLength`] when `value` or its point equivalent is negative.
    pub fn mm(value: f32) -> std::result::Result<Length<Self>, Error> {
        let mm = finite_pt(value)?;
        if mm < 0.0 {
            return Err(Error::from(ErrorKind::NegativeLength { value: mm }));
        }
        let pt = finite_pt(mm * MM_TO_PT)?;
        if pt < 0.0 {
            return Err(Error::from(ErrorKind::NegativeLength { value: pt }));
        }
        Ok(Length::from_trusted_pt(pt))
    }
}

impl Positive {
    /// Accepts a finite point value strictly greater than zero.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `value` is NaN or infinite.
    /// [`ErrorKind::NonPositiveLength`] when `value` is zero or negative.
    pub fn pt(value: f32) -> std::result::Result<Length<Self>, Error> {
        let value = finite_pt(value)?;
        if value <= 0.0 {
            return Err(Error::from(ErrorKind::NonPositiveLength { value }));
        }
        Ok(Length::from_trusted_pt(value))
    }

    /// Accepts a finite millimeter value strictly greater than zero.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `value` or its point equivalent is NaN or infinite.
    /// [`ErrorKind::NonPositiveLength`] when `value` or its point equivalent is zero or negative.
    pub fn mm(value: f32) -> std::result::Result<Length<Self>, Error> {
        let mm = finite_pt(value)?;
        if mm <= 0.0 {
            return Err(Error::from(ErrorKind::NonPositiveLength { value: mm }));
        }
        let pt = finite_pt(mm * MM_TO_PT)?;
        if pt <= 0.0 {
            return Err(Error::from(ErrorKind::NonPositiveLength { value: pt }));
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

impl Length<Finite> {
    pub(crate) fn clamp_non_negative(self) -> Length<NonNegative> {
        Length::from_trusted_pt(self.pt.max(0.0))
    }

    pub(crate) fn try_positive(self) -> Option<Length<Positive>> {
        if self.pt.is_finite() && self.pt > 0.0 {
            Some(Length::from_trusted_pt(self.pt))
        } else {
            None
        }
    }
}

impl Length<NonNegative> {
    /// `self - rhs`, then `max(0)`. Matches the frame's remaining-space update.
    pub(crate) fn saturating_sub(self, rhs: Self) -> Self {
        Self::from_trusted_pt((self.pt - rhs.pt).max(0.0))
    }

    /// `self - rhs` when the caller already knows `self >= rhs`.
    pub(crate) fn sub_exact(self, rhs: Self) -> Self {
        Self::from_trusted_pt(self.pt - rhs.pt)
    }
}

impl std::ops::Add for Length<NonNegative> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::from_trusted_pt(self.pt + rhs.pt)
    }
}

impl std::ops::Add<Length<Positive>> for Length<NonNegative> {
    type Output = Self;

    fn add(self, rhs: Length<Positive>) -> Self {
        Self::from_trusted_pt(self.pt + rhs.as_pt())
    }
}

impl std::ops::Add<Length<NonNegative>> for Length<Positive> {
    type Output = Length<NonNegative>;

    fn add(self, rhs: Length<NonNegative>) -> Length<NonNegative> {
        Length::from_trusted_pt(self.as_pt() + rhs.as_pt())
    }
}

impl std::ops::AddAssign for Length<NonNegative> {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl std::ops::AddAssign<Length<Positive>> for Length<NonNegative> {
    fn add_assign(&mut self, rhs: Length<Positive>) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub for Length<NonNegative> {
    type Output = Length<Finite>;

    fn sub(self, rhs: Self) -> Length<Finite> {
        Length::from_trusted_pt(self.pt - rhs.pt)
    }
}

impl std::ops::Sub<Length<Positive>> for Length<Positive> {
    type Output = Length<Finite>;

    fn sub(self, rhs: Self) -> Length<Finite> {
        Length::from_trusted_pt(self.as_pt() - rhs.as_pt())
    }
}

impl std::ops::Sub<Length<NonNegative>> for Length<Positive> {
    type Output = Length<Finite>;

    fn sub(self, rhs: Length<NonNegative>) -> Length<Finite> {
        Length::from_trusted_pt(self.as_pt() - rhs.as_pt())
    }
}

impl std::ops::Sub<Length<Positive>> for Length<NonNegative> {
    type Output = Length<Finite>;

    fn sub(self, rhs: Length<Positive>) -> Length<Finite> {
        Length::from_trusted_pt(self.pt - rhs.as_pt())
    }
}

impl std::iter::Sum for Length<NonNegative> {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(NonNegative::ZERO, std::ops::Add::add)
    }
}

/// Converts a value into a non-negative length at a DSL or geometry boundary.
pub trait IntoNonNegative {
    /// Rejects non-finite or negative inputs.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] or [`ErrorKind::NegativeLength`] for a raw `f32`.
    /// A length that is already [`NonNegative`] or [`Positive`] is returned as-is.
    fn into_non_negative(self) -> std::result::Result<Length<NonNegative>, Error>;
}

impl IntoNonNegative for Length<NonNegative> {
    fn into_non_negative(self) -> std::result::Result<Length<NonNegative>, Error> {
        Ok(self)
    }
}

impl IntoNonNegative for Length<Positive> {
    fn into_non_negative(self) -> std::result::Result<Length<NonNegative>, Error> {
        Ok(self.into())
    }
}

impl IntoNonNegative for f32 {
    fn into_non_negative(self) -> std::result::Result<Length<NonNegative>, Error> {
        NonNegative::pt(self)
    }
}

/// Converts a value into a strictly positive length at a DSL or geometry boundary.
pub trait IntoPositive {
    /// Rejects non-finite or non-positive inputs.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] or [`ErrorKind::NonPositiveLength`] for a raw `f32`.
    /// A length that is already [`Positive`] is returned as-is.
    fn into_positive(self) -> std::result::Result<Length<Positive>, Error>;
}

impl IntoPositive for Length<Positive> {
    fn into_positive(self) -> std::result::Result<Length<Positive>, Error> {
        Ok(self)
    }
}

impl IntoPositive for f32 {
    fn into_positive(self) -> std::result::Result<Length<Positive>, Error> {
        Positive::pt(self)
    }
}

/// Converts a value into a finite length at a DSL or geometry boundary.
pub trait IntoFinite {
    /// Rejects non-finite inputs.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] for a raw `f32`.
    /// A length that is already finite, non-negative, or positive is returned as-is.
    fn into_finite(self) -> std::result::Result<Length<Finite>, Error>;
}

impl IntoFinite for Length<Finite> {
    fn into_finite(self) -> std::result::Result<Length<Finite>, Error> {
        Ok(self)
    }
}

impl IntoFinite for Length<NonNegative> {
    fn into_finite(self) -> std::result::Result<Length<Finite>, Error> {
        Ok(self.into())
    }
}

impl IntoFinite for Length<Positive> {
    fn into_finite(self) -> std::result::Result<Length<Finite>, Error> {
        Ok(self.into())
    }
}

impl IntoFinite for f32 {
    fn into_finite(self) -> std::result::Result<Length<Finite>, Error> {
        Finite::pt(self)
    }
}

/// Converts a value into a [`Point`] at a DSL or geometry boundary.
pub trait IntoPoint {
    /// Rejects non-finite coordinates.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when either coordinate is NaN or infinite.
    /// An existing [`Point`] is returned as-is.
    fn into_point(self) -> std::result::Result<Point, Error>;
}

impl IntoPoint for Point {
    fn into_point(self) -> std::result::Result<Point, Error> {
        Ok(self)
    }
}

impl IntoPoint for (f32, f32) {
    fn into_point(self) -> std::result::Result<Point, Error> {
        Point::try_pt(self.0, self.1)
    }
}

/// Converts a value into a [`Rect`] at a DSL or geometry boundary.
pub trait IntoRect {
    /// Rejects non-finite origins or non-positive size.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] or [`ErrorKind::NonPositiveLength`] when a raw tuple
    /// cannot form a [`Rect`]. An existing [`Rect`] is returned as-is.
    fn into_rect(self) -> std::result::Result<Rect, Error>;
}

impl IntoRect for Rect {
    fn into_rect(self) -> std::result::Result<Rect, Error> {
        Ok(self)
    }
}

impl IntoRect for (f32, f32, f32, f32) {
    fn into_rect(self) -> std::result::Result<Rect, Error> {
        Rect::try_pt(self.0, self.1, self.2, self.3)
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
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `x` or `y` is NaN or infinite.
    pub fn try_pt(x: f32, y: f32) -> std::result::Result<Self, Error> {
        Ok(Self::new(Finite::pt(x)?, Finite::pt(y)?))
    }

    /// Builds a point from finite millimeter values.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `x`, `y`, or either point equivalent is NaN or infinite.
    pub fn try_mm(x: f32, y: f32) -> std::result::Result<Self, Error> {
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
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when `width` or `height` is NaN or infinite.
    /// [`ErrorKind::NonPositiveLength`] when either value is zero or negative.
    pub fn try_pt(width: f32, height: f32) -> std::result::Result<Self, Error> {
        Ok(Self::new(Positive::pt(width)?, Positive::pt(height)?))
    }

    /// Builds a size from positive millimeter values.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when a side or its point equivalent is NaN or infinite.
    /// [`ErrorKind::NonPositiveLength`] when a side or its point equivalent is zero or negative.
    pub fn try_mm(width: f32, height: f32) -> std::result::Result<Self, Error> {
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

/// A standard paper size. Portrait dimensions are the short side by the long side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[must_use]
pub enum Paper {
    /// ISO A0, 841 mm by 1189 mm.
    A0,
    /// ISO A1, 594 mm by 841 mm.
    A1,
    /// ISO A2, 420 mm by 594 mm.
    A2,
    /// ISO A3, 297 mm by 420 mm.
    A3,
    /// ISO A4, 210 mm by 297 mm.
    A4,
    /// ISO A5, 148 mm by 210 mm.
    A5,
    /// ISO A6, 105 mm by 148 mm.
    A6,
    /// US Letter, 612 pt by 792 pt.
    Letter,
    /// US Legal, 612 pt by 1008 pt.
    Legal,
}

impl Paper {
    const fn portrait(self) -> Size {
        match self {
            Self::A0 => Size::from_trusted_pt(841.0 * MM_TO_PT, 1189.0 * MM_TO_PT),
            Self::A1 => Size::from_trusted_pt(594.0 * MM_TO_PT, 841.0 * MM_TO_PT),
            Self::A2 => Size::from_trusted_pt(420.0 * MM_TO_PT, 594.0 * MM_TO_PT),
            Self::A3 => Size::from_trusted_pt(297.0 * MM_TO_PT, 420.0 * MM_TO_PT),
            Self::A4 => A4,
            Self::A5 => Size::from_trusted_pt(148.0 * MM_TO_PT, 210.0 * MM_TO_PT),
            Self::A6 => Size::from_trusted_pt(105.0 * MM_TO_PT, 148.0 * MM_TO_PT),
            Self::Letter => LETTER,
            Self::Legal => Size::from_trusted_pt(612.0, 1008.0),
        }
    }

    /// This paper with width and height swapped.
    pub const fn landscape(self) -> PageSize {
        PageSize::Paper(self, Orientation::Landscape)
    }
}

/// Which way a [`Paper`] is turned. A [`PageSize::Custom`] size is already final.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Orientation {
    /// The paper's listed width and height.
    Portrait,
    /// The paper's height and width swapped.
    Landscape,
}

/// Page size for a document or a section.
///
/// Portrait shortcuts are the associated constants. Landscape is
/// [`Paper::landscape`] or [`Orientation::Landscape`] on [`PageSize::Paper`].
/// [`PageSize::Custom`] is a final size and does not take an orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum PageSize {
    /// A standard paper and an orientation.
    Paper(Paper, Orientation),
    /// Width and height already in PDF points.
    Custom(Size),
}

impl PageSize {
    /// ISO A0, portrait.
    pub const A0: Self = Self::Paper(Paper::A0, Orientation::Portrait);
    /// ISO A1, portrait.
    pub const A1: Self = Self::Paper(Paper::A1, Orientation::Portrait);
    /// ISO A2, portrait.
    pub const A2: Self = Self::Paper(Paper::A2, Orientation::Portrait);
    /// ISO A3, portrait.
    pub const A3: Self = Self::Paper(Paper::A3, Orientation::Portrait);
    /// ISO A4, portrait.
    pub const A4: Self = Self::Paper(Paper::A4, Orientation::Portrait);
    /// ISO A5, portrait.
    pub const A5: Self = Self::Paper(Paper::A5, Orientation::Portrait);
    /// ISO A6, portrait.
    pub const A6: Self = Self::Paper(Paper::A6, Orientation::Portrait);
    /// US Letter, portrait.
    pub const LETTER: Self = Self::Paper(Paper::Letter, Orientation::Portrait);
    /// US Legal, portrait.
    pub const LEGAL: Self = Self::Paper(Paper::Legal, Orientation::Portrait);

    /// A final page size. Orientation does not apply.
    pub const fn custom(size: Size) -> Self {
        Self::Custom(size)
    }

    /// The page width and height in PDF points.
    pub const fn resolve(self) -> Size {
        match self {
            Self::Paper(paper, Orientation::Portrait) => paper.portrait(),
            Self::Paper(paper, Orientation::Landscape) => {
                let portrait = paper.portrait();
                Size::new(portrait.height(), portrait.width())
            }
            Self::Custom(size) => size,
        }
    }
}

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
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when any argument is NaN or infinite.
    /// [`ErrorKind::NonPositiveLength`] when `width` or `height` is zero or negative.
    pub fn try_pt(x: f32, y: f32, width: f32, height: f32) -> std::result::Result<Self, Error> {
        Ok(Self::new(
            Point::try_pt(x, y)?,
            Size::try_pt(width, height)?,
        ))
    }

    /// Builds a rectangle from millimeter values. Width and height must be positive.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] when any argument or its point equivalent is NaN or infinite.
    /// [`ErrorKind::NonPositiveLength`] when `width` or `height`, or its point equivalent, is zero or negative.
    pub fn try_mm(x: f32, y: f32, width: f32, height: f32) -> std::result::Result<Self, Error> {
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

    pub(crate) fn translated(self, dx: f32, dy: f32) -> std::result::Result<Self, Error> {
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

    /// The same length on every side, from a length or pt value.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] or [`ErrorKind::NegativeLength`] when `value` is a raw `f32`
    /// that is not finite and at least zero. An existing non-negative length is accepted.
    pub fn try_uniform(value: impl IntoNonNegative) -> std::result::Result<Self, Error> {
        Ok(Self::uniform(value.into_non_negative()?))
    }

    /// Four sides from lengths or pt values.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] or [`ErrorKind::NegativeLength`] when a side is a raw `f32`
    /// that is not finite and at least zero. An existing non-negative length is accepted.
    pub fn try_new(
        top: impl IntoNonNegative,
        right: impl IntoNonNegative,
        bottom: impl IntoNonNegative,
        left: impl IntoNonNegative,
    ) -> std::result::Result<Self, Error> {
        Ok(Self::new(
            top.into_non_negative()?,
            right.into_non_negative()?,
            bottom.into_non_negative()?,
            left.into_non_negative()?,
        ))
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

/// A named set of four equal page margins.
///
/// [`MarginPreset::Inch`] is exactly 72 PDF points, one international inch.
/// [`MarginPreset::HalfInch`] is exactly 36 PDF points. Choosing a preset sets
/// all four sides, including the header and footer band heights.
///
/// This enum has no [`Default`]. [`Margins::default`] stays four zeros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[must_use]
pub enum MarginPreset {
    /// Exactly 72 PDF points (one international inch) on every side.
    Inch,
    /// Exactly 36 PDF points (one half international inch) on every side.
    HalfInch,
}

impl MarginPreset {
    /// Uniform margins for this preset.
    pub const fn into_margins(self) -> Margins {
        let side = match self {
            Self::Inch => Length::from_trusted_pt(72.0),
            Self::HalfInch => Length::from_trusted_pt(36.0),
        };
        Margins::uniform(side)
    }
}

impl From<MarginPreset> for Margins {
    fn from(value: MarginPreset) -> Self {
        value.into_margins()
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

/// A colored stroke. Width is strictly positive, so a pen always leaves a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Pen {
    color: Color,
    stroke: Stroke,
}

impl Pen {
    /// Stroke of `color` and `width` (a positive length or pt value).
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NonFiniteLength`] or [`ErrorKind::NonPositiveLength`] when `width` is a raw `f32`
    /// that is not finite and strictly greater than zero. An existing positive length is accepted.
    pub fn new(color: Color, width: impl IntoPositive) -> std::result::Result<Self, Error> {
        Ok(Self {
            color,
            stroke: Stroke::new(width.into_positive()?),
        })
    }

    /// Stroke color, including alpha.
    pub const fn color(self) -> Color {
        self.color
    }

    /// Stroke width.
    pub const fn width(self) -> Length<Positive> {
        self.stroke.width()
    }
}

/// How a closed shape is painted. Every arm paints something.
///
/// Opacity comes from the colors' alpha. [`Ink::FillStroke`] paints the fill first, then the stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// Fills the shape.
    Fill(Color),
    /// Strokes the shape.
    Stroke(Pen),
    /// Fills the shape, then strokes it.
    FillStroke(Color, Pen),
}

/// One command in a [`Path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathVerb {
    /// Moves the pen without drawing.
    MoveTo(Point),
    /// Draws a straight segment.
    LineTo(Point),
    /// Cubic Bézier from the current point through two control points to `to`.
    CubicTo {
        /// First control point.
        c1: Point,
        /// Second control point.
        c2: Point,
        /// End point.
        to: Point,
    },
    /// Closes the current subpath back to its most recent `MoveTo`.
    Close,
}

// 4/3 * (sqrt(2) - 1). Four cubics stay within about 0.027% of a true quarter ellipse.
const KAPPA: f32 = 0.552_284_8;

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

    /// Closed rectangle. `MoveTo` the top-left, then three `LineTo` clockwise, then `Close`.
    ///
    /// Fails when `origin + extent` is not finite.
    pub(crate) fn rect(rect: Rect) -> std::result::Result<Self, Error> {
        let x = rect.origin().x().as_pt();
        let y = rect.origin().y().as_pt();
        let right = x + rect.size().width().as_pt();
        let bottom = y + rect.size().height().as_pt();
        Ok(Self::begin(Point::try_pt(x, y)?)
            .line_to(Point::try_pt(right, y)?)
            .line_to(Point::try_pt(right, bottom)?)
            .line_to(Point::try_pt(x, bottom)?)
            .close())
    }

    /// Ellipse inscribed in `rect`.
    ///
    /// `MoveTo` the right-middle, then four `CubicTo` quarter arcs clockwise (y down), then `Close`.
    /// Each control point is `KAPPA` radii off its endpoint.
    /// Fails when a constructed point is not finite.
    pub(crate) fn ellipse(rect: Rect) -> std::result::Result<Self, Error> {
        let rx = rect.size().width().as_pt() / 2.0;
        let ry = rect.size().height().as_pt() / 2.0;
        let cx = rect.origin().x().as_pt() + rx;
        let cy = rect.origin().y().as_pt() + ry;
        let kx = KAPPA * rx;
        let ky = KAPPA * ry;
        let right = Point::try_pt(cx + rx, cy)?;
        let bottom = Point::try_pt(cx, cy + ry)?;
        let left = Point::try_pt(cx - rx, cy)?;
        let top = Point::try_pt(cx, cy - ry)?;
        Ok(Self {
            verbs: vec![
                PathVerb::MoveTo(right),
                PathVerb::CubicTo {
                    c1: Point::try_pt(cx + rx, cy + ky)?,
                    c2: Point::try_pt(cx + kx, cy + ry)?,
                    to: bottom,
                },
                PathVerb::CubicTo {
                    c1: Point::try_pt(cx - kx, cy + ry)?,
                    c2: Point::try_pt(cx - rx, cy + ky)?,
                    to: left,
                },
                PathVerb::CubicTo {
                    c1: Point::try_pt(cx - rx, cy - ky)?,
                    c2: Point::try_pt(cx - kx, cy - ry)?,
                    to: top,
                },
                PathVerb::CubicTo {
                    c1: Point::try_pt(cx + kx, cy - ry)?,
                    c2: Point::try_pt(cx + rx, cy - ky)?,
                    to: right,
                },
                PathVerb::Close,
            ]
            .into_boxed_slice(),
        })
    }

    /// The verbs in order. The slice is never empty and starts with `MoveTo`.
    #[must_use]
    pub fn verbs(&self) -> &[PathVerb] {
        &self.verbs
    }

    pub(crate) fn translated(&self, dx: f32, dy: f32) -> std::result::Result<Self, Error> {
        self.map_points(|point| Point::try_pt(point.x().as_pt() + dx, point.y().as_pt() + dy))
    }

    /// Rotates every point, including control points, by `degrees` about `about`.
    ///
    /// Positive is clockwise on the page, matching krilla `Transform::from_rotate`.
    /// Fails when a rotated coordinate is not finite.
    pub(crate) fn rotated(&self, degrees: f32, about: Point) -> std::result::Result<Self, Error> {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let ox = about.x().as_pt();
        let oy = about.y().as_pt();
        self.map_points(|point| {
            let dx = point.x().as_pt() - ox;
            let dy = point.y().as_pt() - oy;
            // y grows downward, so this matrix is clockwise for a positive angle.
            Point::try_pt(ox + cos * dx - sin * dy, oy + sin * dx + cos * dy)
        })
    }

    fn map_points(
        &self,
        mut map: impl FnMut(Point) -> std::result::Result<Point, Error>,
    ) -> std::result::Result<Self, Error> {
        let verbs = self
            .verbs
            .iter()
            .map(|verb| {
                Ok(match *verb {
                    PathVerb::MoveTo(point) => PathVerb::MoveTo(map(point)?),
                    PathVerb::LineTo(point) => PathVerb::LineTo(map(point)?),
                    PathVerb::CubicTo { c1, c2, to } => PathVerb::CubicTo {
                        c1: map(c1)?,
                        c2: map(c2)?,
                        to: map(to)?,
                    },
                    PathVerb::Close => PathVerb::Close,
                })
            })
            .collect::<std::result::Result<Vec<_>, Error>>()?;
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
    fn paper_portrait_matches_a4_and_letter_bits_and_the_millimeter_formula() {
        let a4 = PageSize::A4.resolve();
        assert_eq!(a4.width().as_pt().to_bits(), A4.width().as_pt().to_bits());
        assert_eq!(a4.height().as_pt().to_bits(), A4.height().as_pt().to_bits());

        let letter = PageSize::LETTER.resolve();
        assert_eq!(
            letter.width().as_pt().to_bits(),
            LETTER.width().as_pt().to_bits()
        );
        assert_eq!(
            letter.height().as_pt().to_bits(),
            LETTER.height().as_pt().to_bits()
        );

        let legal = PageSize::LEGAL.resolve();
        assert_eq!(legal.width().as_pt().to_bits(), 612.0f32.to_bits());
        assert_eq!(legal.height().as_pt().to_bits(), 1008.0f32.to_bits());

        let landscape = PageSize::Paper(Paper::A4, Orientation::Landscape).resolve();
        assert_eq!(landscape, Size::new(A4.height(), A4.width()));
        assert_eq!(Paper::A4.landscape().resolve(), landscape);

        let custom = Size::try_pt(100.0, 50.0).unwrap();
        assert_eq!(PageSize::custom(custom).resolve(), custom);

        let series = [
            (Paper::A0, 841.0, 1189.0),
            (Paper::A1, 594.0, 841.0),
            (Paper::A2, 420.0, 594.0),
            (Paper::A3, 297.0, 420.0),
            (Paper::A4, 210.0, 297.0),
            (Paper::A5, 148.0, 210.0),
            (Paper::A6, 105.0, 148.0),
        ];
        for (paper, width_mm, height_mm) in series {
            let size = PageSize::Paper(paper, Orientation::Portrait).resolve();
            assert!(abs_err_pt(size.width().as_pt(), width_mm * 72.0 / 25.4) < 1e-3);
            assert!(abs_err_pt(size.height().as_pt(), height_mm * 72.0 / 25.4) < 1e-3);
        }
    }

    #[test]
    fn finite_rejects_nan_and_infinity() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(matches!(
                Finite::pt(value).map_err(|error| error.kind),
                Err(ErrorKind::NonFiniteLength { .. })
            ));
            assert!(matches!(
                Finite::mm(value).map_err(|error| error.kind),
                Err(ErrorKind::NonFiniteLength { .. })
            ));
            assert!(matches!(
                NonNegative::pt(value).map_err(|error| error.kind),
                Err(ErrorKind::NonFiniteLength { .. })
            ));
            assert!(matches!(
                Positive::pt(value).map_err(|error| error.kind),
                Err(ErrorKind::NonFiniteLength { .. })
            ));
        }
    }

    #[test]
    fn into_length_traits_accept_f32_and_typed_lengths() {
        assert_eq!(
            12.0f32.into_non_negative().unwrap(),
            NonNegative::pt(12.0).unwrap()
        );
        assert_eq!(
            NonNegative::pt(3.0).unwrap().into_non_negative().unwrap(),
            NonNegative::pt(3.0).unwrap()
        );
        assert_eq!(1.5f32.into_positive().unwrap(), Positive::pt(1.5).unwrap());
        assert!(matches!(
            (-1.0f32).into_non_negative().map_err(|error| error.kind),
            Err(ErrorKind::NegativeLength { .. })
        ));
        assert!(matches!(
            0.0f32.into_positive().map_err(|error| error.kind),
            Err(ErrorKind::NonPositiveLength { .. })
        ));
        assert_eq!(
            (1.0f32, 2.0f32).into_point().unwrap(),
            Point::try_pt(1.0, 2.0).unwrap()
        );
        assert_eq!(
            (1.0f32, 2.0f32, 3.0f32, 4.0f32).into_rect().unwrap(),
            Rect::try_pt(1.0, 2.0, 3.0, 4.0).unwrap()
        );
        assert_eq!(
            Margins::try_uniform(36.0).unwrap(),
            Margins::uniform(NonNegative::pt(36.0).unwrap())
        );
        assert_eq!(
            Pen::new(Color::rgb(1, 2, 3), 0.75).unwrap().width(),
            Positive::pt(0.75).unwrap()
        );
    }

    #[test]
    fn non_negative_rejects_negative_and_accepts_zero() {
        match NonNegative::pt(-1.5) {
            Err(Error {
                kind: ErrorKind::NegativeLength { value },
                ..
            }) => {
                assert_eq!(value.to_bits(), (-1.5f32).to_bits());
            }
            other => panic!("expected NegativeLength, got {other:?}"),
        }
        assert!(matches!(
            NonNegative::mm(-1.0).map_err(|error| error.kind),
            Err(ErrorKind::NegativeLength { .. })
        ));
        assert_eq!(NonNegative::pt(0.0).unwrap(), NonNegative::ZERO);
        assert_eq!(NonNegative::mm(0.0).unwrap(), NonNegative::ZERO);
    }

    #[test]
    fn positive_rejects_zero_and_negative() {
        match Positive::pt(0.0) {
            Err(Error {
                kind: ErrorKind::NonPositiveLength { value },
                ..
            }) => {
                assert_eq!(value.to_bits(), 0.0f32.to_bits());
            }
            other => panic!("expected NonPositiveLength, got {other:?}"),
        }
        assert!(matches!(
            Positive::pt(-0.25).map_err(|error| error.kind),
            Err(ErrorKind::NonPositiveLength { .. })
        ));
        assert!(matches!(
            Positive::mm(0.0).map_err(|error| error.kind),
            Err(ErrorKind::NonPositiveLength { .. })
        ));
        assert!(matches!(
            Positive::mm(-3.0).map_err(|error| error.kind),
            Err(ErrorKind::NonPositiveLength { .. })
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
    fn inch_margin_preset_is_uniform_72pt() {
        let expected = Margins::uniform(NonNegative::pt(72.0).unwrap());
        assert_eq!(MarginPreset::Inch.into_margins(), expected);
        assert_eq!(Margins::from(MarginPreset::Inch), expected);
    }

    #[test]
    fn half_inch_margin_preset_is_uniform_36pt() {
        let expected = Margins::uniform(NonNegative::pt(36.0).unwrap());
        assert_eq!(MarginPreset::HalfInch.into_margins(), expected);
        assert_eq!(Margins::from(MarginPreset::HalfInch), expected);
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
            Positive::pt(0.0).map_err(|error| error.kind),
            Err(ErrorKind::NonPositiveLength { .. })
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

    fn pt(x: f32, y: f32) -> Point {
        Point::try_pt(x, y).unwrap()
    }

    #[test]
    fn rect_walks_clockwise_from_the_top_left() {
        let path = Path::rect(Rect::try_pt(1.0, 2.0, 3.0, 4.0).unwrap()).unwrap();
        assert_eq!(
            path.verbs(),
            &[
                PathVerb::MoveTo(pt(1.0, 2.0)),
                PathVerb::LineTo(pt(4.0, 2.0)),
                PathVerb::LineTo(pt(4.0, 6.0)),
                PathVerb::LineTo(pt(1.0, 6.0)),
                PathVerb::Close,
            ]
        );
    }

    #[test]
    fn ellipse_endpoints_and_kappa_controls_run_clockwise() {
        let path = Path::ellipse(Rect::try_pt(0.0, 0.0, 4.0, 2.0).unwrap()).unwrap();
        let k = 0.552_284_8_f32;
        let (cx, cy, rx, ry) = (2.0_f32, 1.0, 2.0, 1.0);
        let (kx, ky) = (k * rx, k * ry);
        assert_eq!(
            path.verbs(),
            &[
                PathVerb::MoveTo(pt(4.0, 1.0)),
                PathVerb::CubicTo {
                    c1: pt(cx + rx, cy + ky),
                    c2: pt(cx + kx, cy + ry),
                    to: pt(2.0, 2.0),
                },
                PathVerb::CubicTo {
                    c1: pt(cx - kx, cy + ry),
                    c2: pt(cx - rx, cy + ky),
                    to: pt(0.0, 1.0),
                },
                PathVerb::CubicTo {
                    c1: pt(cx - rx, cy - ky),
                    c2: pt(cx - kx, cy - ry),
                    to: pt(2.0, 0.0),
                },
                PathVerb::CubicTo {
                    c1: pt(cx + kx, cy - ry),
                    c2: pt(cx + rx, cy - ky),
                    to: pt(4.0, 1.0),
                },
                PathVerb::Close,
            ]
        );
    }

    #[test]
    fn rotated_quarter_turn_is_clockwise() {
        let about = pt(10.0, 20.0);
        let path = Path::line(pt(13.0, 20.0), pt(10.0, 24.0));
        let turned = path.rotated(90.0, about).unwrap();
        let [PathVerb::MoveTo(start), PathVerb::LineTo(end)] = turned.verbs() else {
            panic!("expected a segment, got {:?}", turned.verbs());
        };
        assert!(crate::canvas::pt_near(start.x().as_pt(), 10.0));
        assert!(crate::canvas::pt_near(start.y().as_pt(), 23.0));
        assert!(crate::canvas::pt_near(end.x().as_pt(), 6.0));
        assert!(crate::canvas::pt_near(end.y().as_pt(), 20.0));

        let ellipse = Path::ellipse(Rect::try_pt(0.0, 0.0, 4.0, 2.0).unwrap()).unwrap();
        let spun = ellipse.rotated(90.0, pt(2.0, 1.0)).unwrap();
        let PathVerb::MoveTo(right) = spun.verbs()[0] else {
            panic!("expected move");
        };
        assert!(crate::canvas::pt_near(right.x().as_pt(), 2.0));
        assert!(crate::canvas::pt_near(right.y().as_pt(), 3.0));
        let PathVerb::CubicTo { c1, .. } = spun.verbs()[1] else {
            panic!("expected cubic");
        };
        let ky = 0.552_284_8_f32;
        assert!(crate::canvas::pt_near(c1.x().as_pt(), 2.0 - ky));
        assert!(crate::canvas::pt_near(c1.y().as_pt(), 3.0));
    }

    #[test]
    fn translated_shifts_cubic_controls() {
        let path = Path::ellipse(Rect::try_pt(0.0, 0.0, 4.0, 2.0).unwrap()).unwrap();
        let shifted = path.translated(5.0, 6.0).unwrap();
        for (before, after) in path.verbs().iter().zip(shifted.verbs()) {
            match (before, after) {
                (PathVerb::Close, PathVerb::Close) => {}
                (PathVerb::MoveTo(from), PathVerb::MoveTo(to))
                | (PathVerb::LineTo(from), PathVerb::LineTo(to)) => {
                    assert_eq!(to.x().as_pt().to_bits(), (from.x().as_pt() + 5.0).to_bits());
                    assert_eq!(to.y().as_pt().to_bits(), (from.y().as_pt() + 6.0).to_bits());
                }
                (
                    PathVerb::CubicTo {
                        c1: a1,
                        c2: a2,
                        to: a3,
                    },
                    PathVerb::CubicTo {
                        c1: b1,
                        c2: b2,
                        to: b3,
                    },
                ) => {
                    for (from, to) in [(a1, b1), (a2, b2), (a3, b3)] {
                        assert_eq!(to.x().as_pt().to_bits(), (from.x().as_pt() + 5.0).to_bits());
                        assert_eq!(to.y().as_pt().to_bits(), (from.y().as_pt() + 6.0).to_bits());
                    }
                }
                _ => panic!("verb shape changed"),
            }
        }
    }
}
