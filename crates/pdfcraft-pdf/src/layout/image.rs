//! Atomic raster block. Size is resolved at build time. Split only places it.

use crate::canvas::DecodedImage;
use crate::error::Fallible as Result;
use crate::geom::{Length, NonNegative, Positive, Rect, Size};
use crate::layout::atomic::{self, AtomicPlace};
use crate::layout::frame::{Fragment, FragmentCommand, Frame, Split};
use crate::layout::style::TextAlign;

/// An image in the flow. `size` is resolved when the node is built.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageNode {
    image: DecodedImage,
    size: Size,
    align: TextAlign,
}

impl ImageNode {
    /// Pixel sides of a registered image are non-zero. A zero side fails in [`resolve_size`].
    pub(crate) fn new(
        image: DecodedImage,
        width: Option<Length<Positive>>,
        height: Option<Length<Positive>>,
        align: TextAlign,
    ) -> Result<Self> {
        let size = resolve_size(image.pixel_width, image.pixel_height, width, height)?;
        Ok(Self { image, size, align })
    }

    pub(crate) fn split(self, avail: Frame) -> Result<Split<Self>> {
        let width = self.size.width().as_pt();
        let height = self.size.height().as_pt();
        match atomic::place_atomic(self.size, self.align, &avail)? {
            AtomicPlace::MoveToNextPage => Ok(Split::MoveToNextPage(self)),
            AtomicPlace::Fit {
                x,
                height: occupied,
            } => Ok(Split::Fit {
                height: occupied,
                fragment: Fragment {
                    height: occupied,
                    spacing_after: NonNegative::ZERO,
                    commands: vec![FragmentCommand::Image {
                        image: self.image,
                        rect: Rect::try_pt(x, 0.0, width, height)?,
                    }],
                    anchors: Vec::new(),
                },
            }),
        }
    }
}

/// §2.7 size rules.
///
/// Both sides are used as given. One side derives the other from the pixel aspect.
/// Neither side means 1 px = 1 pt.
#[allow(
    clippy::cast_precision_loss,
    reason = "display size is f32 points, so a pixel edge above 2^24 cannot stay exact"
)]
pub(crate) fn resolve_size(
    px_w: u32,
    px_h: u32,
    width: Option<Length<Positive>>,
    height: Option<Length<Positive>>,
) -> Result<Size> {
    let px = |value: u32| value as f32;
    match (width, height) {
        (Some(width), Some(height)) => Ok(Size::new(width, height)),
        (Some(width), None) => {
            let height = Positive::pt(width.as_pt() * px(px_h) / px(px_w))?;
            Ok(Size::new(width, height))
        }
        (None, Some(height)) => {
            let width = Positive::pt(height.as_pt() * px(px_w) / px(px_h))?;
            Ok(Size::new(width, height))
        }
        (None, None) => Size::try_pt(px(px_w), px(px_h)),
    }
}
