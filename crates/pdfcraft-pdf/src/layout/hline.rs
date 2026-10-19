//! Atomic horizontal rule. Width resolves against the frame at split time.

use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::geom::{Color, Length, NonNegative, Paint, Path, Point, Positive, Stroke};
use crate::layout::frame::{Fragment, FragmentCommand, Frame, Split};
use crate::layout::style::TextAlign;

/// A solid horizontal line in the flow.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HLineNode {
    /// `None` means the full width of the frame the node lands in.
    width: Option<Length<Positive>>,
    align: TextAlign,
    thickness: Length<Positive>,
    color: Color,
    space_before: Length<NonNegative>,
    space_after: Length<NonNegative>,
}

impl HLineNode {
    pub(crate) fn new(
        width: Option<Length<Positive>>,
        align: TextAlign,
        thickness: Length<Positive>,
        color: Color,
        space_before: Length<NonNegative>,
        space_after: Length<NonNegative>,
    ) -> Self {
        Self {
            width,
            align,
            thickness,
            color,
            space_before,
            space_after,
        }
    }

    fn height(&self) -> Length<NonNegative> {
        self.space_before + Length::<NonNegative>::from(self.thickness) + self.space_after
    }

    pub(crate) fn split(self, avail: Frame) -> Result<Split<Self>> {
        let width = self.width.unwrap_or(avail.width);
        let height = self.height();
        if width.as_pt() > avail.width.as_pt() {
            return Err(Error::from(ErrorKind::UnbreakableWiderThanFrame {
                text: None,
                width: width.as_pt(),
                avail: avail.width.as_pt(),
            }));
        }
        if height.as_pt() > avail.full_height.as_pt() {
            return Err(ErrorKind::UnsplittableTallerThanFrame {
                height: height.as_pt(),
                avail: avail.full_height.as_pt(),
            }
            .into());
        }
        if height.as_pt() > avail.height.as_pt() {
            return Ok(Split::MoveToNextPage(self));
        }
        let x = match self.align {
            TextAlign::Start | TextAlign::Justify => 0.0,
            TextAlign::Center => (avail.width - width).as_pt() / 2.0,
            TextAlign::End => (avail.width - width).as_pt(),
        };
        let y = self.space_before.as_pt() + self.thickness.as_pt() / 2.0;
        let path = Path::line(Point::try_pt(x, y)?, Point::try_pt(x + width.as_pt(), y)?);
        Ok(Split::Fit {
            height,
            fragment: Fragment {
                height,
                spacing_after: NonNegative::ZERO,
                commands: vec![FragmentCommand::Stroke {
                    path,
                    paint: Paint::solid(self.color),
                    stroke: Stroke::new(self.thickness),
                }],
                anchors: Vec::new(),
            },
        })
    }
}
