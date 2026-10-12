//! Atomic raster block. Size is resolved at build time. Split only places it.

use crate::canvas::DecodedImage;
use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::geom::{Length, Positive, Rect, Size};
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
        if width > avail.width {
            return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: None,
                width,
                avail: avail.width,
            }));
        }
        if height > avail.full_height {
            return Err(ErrorKind::UnsplittableTallerThanFrame {
                height,
                avail: avail.full_height,
            }
            .into());
        }
        if height > avail.height {
            return Ok(Split::MoveToNextPage(self));
        }
        let x = match self.align {
            TextAlign::Start => 0.0,
            TextAlign::Center => (avail.width - width) / 2.0,
            TextAlign::End => avail.width - width,
        };
        Ok(Split::Fit {
            height,
            fragment: Fragment {
                height,
                spacing_after: 0.0,
                commands: vec![FragmentCommand::Image {
                    image: self.image,
                    rect: Rect::try_pt(x, 0.0, width, height)?,
                }],
            },
        })
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
