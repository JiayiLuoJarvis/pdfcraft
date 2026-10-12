#![allow(clippy::wildcard_imports)]
use super::*;

/// One page's behind or front layer. Coordinates are page coordinates, not inset by margins.
pub struct Layer {
    pub(crate) page: Page,
    pub(crate) style: Style,
    pub(crate) doc: DocToken,
    pub(crate) items: Vec<LayerItem>,
    pub(crate) errors: ErrorBag,
}

impl Layer {
    /// The page being drawn.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page
    }

    /// Draws `s` on one line with the top-left of its line box at `at`. It is not wrapped or clipped.
    ///
    /// Inherits the layer's current parent style. Chain [`TextAt::font_size`] / [`TextAt::color`]
    /// to override this call only.
    pub fn text_at(&mut self, s: impl AsRef<str>, at: impl IntoPoint) -> TextAt<'_> {
        let at = match at.into_point() {
            Ok(at) => at,
            Err(error) => {
                self.note(error);
                return TextAt {
                    layer: self,
                    index: usize::MAX,
                };
            }
        };
        let index = self.items.len();
        self.items.push(LayerItem::Text(LayerText {
            text: s.as_ref().to_owned(),
            at,
            style: self.style.clone(),
        }));
        TextAt { layer: self, index }
    }

    /// Draws `image` stretched into `rect`, in page coordinates.
    ///
    /// The rectangle is not clipped and is not inset by margins.
    pub fn image_at(&mut self, image: &Image, rect: impl IntoRect) {
        let rect = match rect.into_rect() {
            Ok(rect) => rect,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        if image.doc != self.doc {
            self.note(Error::from(ErrorKind::UnknownImage {
                id: image.decoded.id.as_u32(),
            }));
            return;
        }
        self.items.push(LayerItem::Image {
            image: image.decoded,
            rect,
        });
    }

    /// Straight segment in page coordinates. `from == to` fails the document with
    /// [`GeometryKind::ZeroLengthSegment`] before any page is drawn.
    pub fn line(&mut self, from: impl IntoPoint, to: impl IntoPoint, pen: Pen) {
        let (from, to) = match (from.into_point(), to.into_point()) {
            (Ok(from), Ok(to)) => (from, to),
            (Err(error), _) | (_, Err(error)) => {
                self.note(error);
                return;
            }
        };
        self.push_shape(straight_path(from, to), Ink::Stroke(pen), None);
    }

    /// Rectangle in page coordinates. Not inset by margins and not clipped.
    pub fn rect(&mut self, rect: impl IntoRect, ink: Ink) {
        let rect = match rect.into_rect() {
            Ok(rect) => rect,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        self.push_shape(Path::rect(rect), ink, None);
    }

    /// Ellipse inscribed in `rect`, in page coordinates.
    pub fn ellipse(&mut self, rect: impl IntoRect, ink: Ink) {
        let rect = match rect.into_rect() {
            Ok(rect) => rect,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        self.push_shape(Path::ellipse(rect), ink, None);
    }

    /// Draws the paths added in `draw`, rotated by `degrees` about `about`.
    ///
    /// `draw` receives [`Shapes`], which can add a line, a rectangle, or an ellipse.
    /// [`Layer::text_at`] and [`Layer::image_at`] are not rotated.
    /// Positive is clockwise on the page. A non-finite angle fails the document with
    /// [`GeometryKind::NonFiniteRotation`] and does not run `draw`. Shapes keep their order relative
    /// to the layer's other items.
    pub fn rotated(
        &mut self,
        degrees: f32,
        about: impl IntoPoint,
        draw: impl FnOnce(&mut Shapes<'_>),
    ) {
        if !degrees.is_finite() {
            self.note(GeometryKind::NonFiniteRotation { degrees }.into());
            return;
        }
        let about = match about.into_point() {
            Ok(about) => about,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        let mut shapes = Shapes {
            layer: self,
            degrees,
            about,
        };
        draw(&mut shapes);
    }

    pub(crate) fn note(&mut self, error: Error) {
        self.errors.record(error);
    }

    fn push_shape(&mut self, path: Fallible<Path>, ink: Ink, turn: Option<(f32, Point)>) {
        let path = path.and_then(|path| match turn {
            Some((degrees, about)) if !is_zero_angle(degrees) => path.rotated(degrees, about),
            _ => Ok(path),
        });
        match path {
            Ok(path) => self.items.push(LayerItem::Shape { path, ink }),
            Err(error) => self.errors.record(error),
        }
    }
}

/// Parent text style patch for [`Flow::stack`] / [`Layer::stack`]. Unset fields inherit.
/// Per-call style overrides for [`Layer::text_at`]. Inherits the layer parent style by default.
pub struct TextAt<'a> {
    pub(crate) layer: &'a mut Layer,
    pub(crate) index: usize,
}

impl TextAt<'_> {
    /// Overrides font size for this text only.
    pub fn font_size(&mut self, size: impl IntoPositive) -> &mut Self {
        match size.into_positive() {
            Ok(size) => {
                if let Some(LayerItem::Text(item)) = self.layer.items.get_mut(self.index) {
                    item.style.font_size = size;
                }
            }
            Err(error) => self.layer.note(error),
        }
        self
    }

    /// Overrides text color for this text only.
    pub fn color(&mut self, color: Color) -> &mut Self {
        if let Some(LayerItem::Text(item)) = self.layer.items.get_mut(self.index) {
            item.style.color = color;
        }
        self
    }
}

/// Path-only view of a [`Layer`] inside [`Layer::rotated`].
///
/// Text and images stay on [`Layer`]. Rotation does not apply to them.
pub struct Shapes<'l> {
    pub(crate) layer: &'l mut Layer,
    pub(crate) degrees: f32,
    pub(crate) about: Point,
}

impl Shapes<'_> {
    /// Straight segment. See [`Layer::line`].
    pub fn line(&mut self, from: impl IntoPoint, to: impl IntoPoint, pen: Pen) {
        let (from, to) = match (from.into_point(), to.into_point()) {
            (Ok(from), Ok(to)) => (from, to),
            (Err(error), _) | (_, Err(error)) => {
                self.layer.note(error);
                return;
            }
        };
        self.layer.push_shape(
            straight_path(from, to),
            Ink::Stroke(pen),
            Some((self.degrees, self.about)),
        );
    }

    /// Rectangle. See [`Layer::rect`].
    pub fn rect(&mut self, rect: impl IntoRect, ink: Ink) {
        let rect = match rect.into_rect() {
            Ok(rect) => rect,
            Err(error) => {
                self.layer.note(error);
                return;
            }
        };
        self.layer
            .push_shape(Path::rect(rect), ink, Some((self.degrees, self.about)));
    }

    /// Ellipse inscribed in `rect`. See [`Layer::ellipse`].
    pub fn ellipse(&mut self, rect: impl IntoRect, ink: Ink) {
        let rect = match rect.into_rect() {
            Ok(rect) => rect,
            Err(error) => {
                self.layer.note(error);
                return;
            }
        };
        self.layer
            .push_shape(Path::ellipse(rect), ink, Some((self.degrees, self.about)));
    }
}

pub(crate) fn straight_path(from: Point, to: Point) -> Fallible<Path> {
    if from == to {
        Err(GeometryKind::ZeroLengthSegment.into())
    } else {
        Ok(Path::line(from, to))
    }
}

pub(crate) fn is_zero_angle(degrees: f32) -> bool {
    degrees.to_bits() == 0 || degrees.to_bits() == (-0.0f32).to_bits()
}
