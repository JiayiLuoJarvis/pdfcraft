//! One pagination decision for image and canvas.

use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::geom::{Length, NonNegative, Size};
use crate::layout::frame::Frame;
use crate::layout::style::TextAlign;

/// Where a fixed-size box goes. The node still builds its own fragment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AtomicPlace {
    /// The box fits the full body but not the remaining height.
    MoveToNextPage,
    /// Place at `x` and occupy `height`.
    Fit {
        /// Left edge in the frame.
        x: f32,
        /// Occupied height. Trailing spacing is zero.
        height: Length<NonNegative>,
    },
}

/// Width first, then full-body height, then the remainder. Strict `>`.
pub(crate) fn place_atomic(size: Size, align: TextAlign, avail: &Frame) -> Result<AtomicPlace> {
    let width = size.width();
    let height = size.height();
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
        return Ok(AtomicPlace::MoveToNextPage);
    }
    let x = match align {
        TextAlign::Start => 0.0,
        TextAlign::Center => (avail.width - width).as_pt() / 2.0,
        TextAlign::End => (avail.width - width).as_pt(),
    };
    Ok(AtomicPlace::Fit {
        x,
        height: height.into(),
    })
}
