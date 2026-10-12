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

    fn height(&self) -> f32 {
        self.space_before.as_pt() + self.thickness.as_pt() + self.space_after.as_pt()
    }

    pub(crate) fn split(self, avail: Frame) -> Result<Split<Self>> {
        let width = self.width.map_or(avail.width, Length::as_pt);
        let height = self.height();
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
        let y = self.space_before.as_pt() + self.thickness.as_pt() / 2.0;
        let path = Path::line(Point::try_pt(x, y)?, Point::try_pt(x + width, y)?);
        Ok(Split::Fit {
            height,
            fragment: Fragment {
                height,
                spacing_after: 0.0,
                commands: vec![FragmentCommand::Stroke {
                    path,
                    paint: Paint::solid(self.color),
                    stroke: Stroke::new(self.thickness),
                }],
            },
        })
    }
}
