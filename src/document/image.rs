#![allow(clippy::wildcard_imports)]
use super::*;

/// An image registered with one document. Copy it into page closures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Image {
    pub(crate) decoded: DecodedImage,
    pub(crate) doc: DocToken,
}

impl Image {
    /// Width in pixels.
    #[must_use]
    pub const fn pixel_width(self) -> u32 {
        self.decoded.pixel_width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn pixel_height(self) -> u32 {
        self.decoded.pixel_height
    }
}

/// Display size and placement of a flow or cell image.
///
/// Both sides stretch the image. One side derives the other from the pixel aspect.
/// When neither side is set, size is **1 px = 1 pt** (提案书 §2.7). The default
/// placement is the start of the frame.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct ImageBox {
    pub(crate) width: Option<Length<Positive>>,
    pub(crate) height: Option<Length<Positive>>,
    pub(crate) align: TextAlign,
    pub(crate) error: Option<Error>,
}

impl Default for ImageBox {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            align: TextAlign::Start,
            error: None,
        }
    }
}

impl ImageBox {
    /// Sets the display width. The height follows the pixel aspect when unset.
    pub fn width(&mut self, width: impl IntoPositive) {
        match width.into_positive() {
            Ok(width) => self.width = Some(width),
            Err(error) => self.note(error),
        }
    }

    /// Sets the display height. The width follows the pixel aspect when unset.
    pub fn height(&mut self, height: impl IntoPositive) {
        match height.into_positive() {
            Ok(height) => self.height = Some(height),
            Err(error) => self.note(error),
        }
    }

    /// Centers the image in the current frame when it is narrower than that frame.
    pub fn center(&mut self) {
        self.align = TextAlign::Center;
    }

    /// Places the image at the end of the current frame when it is narrower than that frame.
    pub fn end(&mut self) {
        self.align = TextAlign::End;
    }

    pub(crate) fn note(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }

    pub(crate) fn lower(self, image: &Image, doc: DocToken) -> Result<FlowNode> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if image.doc != doc {
            return Err(Error::UnknownImage {
                id: image.decoded.id.as_u32(),
            });
        }
        ImageNode::new(image.decoded, self.width, self.height, self.align).map(FlowNode::Image)
    }
}
