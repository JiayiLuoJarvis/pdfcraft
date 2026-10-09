//! Atomic raster block. Size is resolved at build time. Split only places it.

use crate::canvas::DecodedImage;
use crate::error::{Error, Result};
use crate::geom::{Length, Positive, Rect, Size};
use crate::layout::flow::{FlowNode, Fragment, FragmentCommand, Frame, Split};
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

    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
        let width = self.size.width().as_pt();
        let height = self.size.height().as_pt();
        if width > avail.width {
            return Err(Error::UnbreakableWiderThanFrame {
                width,
                avail: avail.width,
            });
        }
        if height > avail.full_height {
            return Err(Error::UnsplittableTallerThanFrame);
        }
        if height > avail.height {
            return Ok(Split::MoveToNextPage(FlowNode::Image(self)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{CanvasCommand, ImageId};
    use crate::fonts::FontStore;
    use crate::layout::columns::{ColumnWidth, ColumnsNode, Ratio};
    use crate::layout::decorate::Decorations;
    use crate::layout::test_util::{page, render};

    fn pixels(width: u32, height: u32) -> DecodedImage {
        DecodedImage {
            id: ImageId::from_raw(0),
            pixel_width: width,
            pixel_height: height,
        }
    }

    fn frame(width: f32, height: f32, full: f32) -> Frame {
        Frame {
            width,
            height,
            full_height: full,
        }
    }

    fn rect_bits(rect: Rect) -> (u32, u32, u32, u32) {
        (
            rect.origin().x().as_pt().to_bits(),
            rect.origin().y().as_pt().to_bits(),
            rect.size().width().as_pt().to_bits(),
            rect.size().height().as_pt().to_bits(),
        )
    }

    #[test]
    fn size_rules_and_end_placement() {
        let both = resolve_size(
            10,
            10,
            Some(Positive::pt(40.0).unwrap()),
            Some(Positive::pt(10.0).unwrap()),
        )
        .unwrap();
        assert_eq!(both.width().as_pt().to_bits(), 40.0f32.to_bits());
        assert_eq!(both.height().as_pt().to_bits(), 10.0f32.to_bits());

        let one = resolve_size(200, 100, Some(Positive::pt(100.0).unwrap()), None).unwrap();
        assert_eq!(one.height().as_pt().to_bits(), 50.0f32.to_bits());

        let native = resolve_size(8, 4, None, None).unwrap();
        assert_eq!(native.width().as_pt().to_bits(), 8.0f32.to_bits());
        assert_eq!(native.height().as_pt().to_bits(), 4.0f32.to_bits());

        let node = ImageNode::new(
            pixels(200, 100),
            Some(Positive::pt(100.0).unwrap()),
            None,
            TextAlign::End,
        )
        .unwrap();
        let Split::Fit { fragment, .. } = node.split(frame(300.0, 400.0, 400.0)).unwrap() else {
            panic!("expected fit");
        };
        assert_eq!(fragment.spacing_after.to_bits(), 0.0f32.to_bits());
        let FragmentCommand::Image { rect, .. } = &fragment.commands[0] else {
            panic!("expected image");
        };
        let expected = Rect::try_pt(200.0, 0.0, 100.0, 50.0).unwrap();
        assert_eq!(rect_bits(*rect), rect_bits(expected));
    }

    #[test]
    fn recording_pins_the_placed_rect() {
        let node = ImageNode::new(
            pixels(200, 100),
            Some(Positive::pt(100.0).unwrap()),
            None,
            TextAlign::End,
        )
        .unwrap();
        let mut fonts = FontStore::new();
        let (result, rec) = render(
            FlowNode::Image(node),
            &mut fonts,
            page(300.0, 400.0),
            &Decorations::none(),
        );
        result.unwrap();
        let pages = rec.pages();
        let CanvasCommand::DrawImage { image, rect } = &pages[0].ops()[0] else {
            panic!("expected draw image");
        };
        assert_eq!(image.id.as_u32(), 0);
        assert_eq!(image.pixel_width, 200);
        assert_eq!(image.pixel_height, 100);
        let expected = Rect::try_pt(200.0, 0.0, 100.0, 50.0).unwrap();
        assert_eq!(rect_bits(*rect), rect_bits(expected));
    }

    #[test]
    fn wider_beats_taller_and_short_frame_moves() {
        let wide = ImageNode::new(
            pixels(1, 1),
            Some(Positive::pt(400.0).unwrap()),
            Some(Positive::pt(150.0).unwrap()),
            TextAlign::Start,
        )
        .unwrap();
        let err = wide.split(frame(300.0, 100.0, 100.0)).unwrap_err();
        assert_eq!(
            err,
            Error::UnbreakableWiderThanFrame {
                width: 400.0,
                avail: 300.0,
            }
        );

        let tall = ImageNode::new(
            pixels(1, 1),
            Some(Positive::pt(10.0).unwrap()),
            Some(Positive::pt(150.0).unwrap()),
            TextAlign::Start,
        )
        .unwrap();
        assert_eq!(
            tall.split(frame(300.0, 100.0, 100.0)).unwrap_err(),
            Error::UnsplittableTallerThanFrame
        );

        let moved = ImageNode::new(
            pixels(1, 1),
            Some(Positive::pt(10.0).unwrap()),
            Some(Positive::pt(40.0).unwrap()),
            TextAlign::Center,
        )
        .unwrap();
        assert!(matches!(
            moved.split(frame(300.0, 10.0, 100.0)).unwrap(),
            Split::MoveToNextPage(FlowNode::Image(_))
        ));
    }

    #[test]
    fn column_offsets_the_image() {
        let node = ImageNode::new(
            pixels(1, 1),
            Some(Positive::pt(10.0).unwrap()),
            Some(Positive::pt(10.0).unwrap()),
            TextAlign::Start,
        )
        .unwrap();
        let even = ColumnWidth::Ratio(Ratio::new(1.0).unwrap());
        let cols = ColumnsNode::new(
            crate::geom::NonNegative::ZERO,
            vec![
                (even, vec![FlowNode::Image(node.clone())]),
                (even, vec![FlowNode::Image(node)]),
            ],
        )
        .unwrap();
        let mut fonts = FontStore::new();
        let (result, rec) = render(
            FlowNode::Columns(cols),
            &mut fonts,
            page(200.0, 100.0),
            &Decorations::none(),
        );
        result.unwrap();
        let images: Vec<_> = rec.pages()[0]
            .ops()
            .iter()
            .filter_map(|op| match op {
                CanvasCommand::DrawImage { rect, .. } => Some(rect_bits(*rect)),
                _ => None,
            })
            .collect();
        let left = Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap();
        let right = Rect::try_pt(100.0, 0.0, 10.0, 10.0).unwrap();
        assert_eq!(images, vec![rect_bits(left), rect_bits(right)]);
    }
}
