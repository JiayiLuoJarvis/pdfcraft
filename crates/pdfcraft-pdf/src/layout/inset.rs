//! Horizontal inset. One child, a narrower frame, the same vertical placement.

use crate::error::{Fallible as Result, GeometryKind};
use crate::geom::{Finite, Length, NonNegative, Positive};
use crate::layout::frame::{Flowable, Fragment, Frame, LayoutCtx, Split};

/// One indented child. `left` and `right` are the only inset state.
///
/// Lowering collapses the public child list before this node exists. An empty
/// list is an empty stack inside `child`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InsetNode<N> {
    pub(crate) left: Length<NonNegative>,
    pub(crate) right: Length<NonNegative>,
    pub(crate) child: Box<N>,
}

impl<N: Flowable> InsetNode<N> {
    /// Narrows the frame, splits `child`, and shifts the fragment by `left`.
    ///
    /// The width check runs first, so an empty child still fails. A page break
    /// in the child is left to that child. An anchor swallows a page break.
    /// This node does not.
    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>> {
        let InsetNode { left, right, child } = self;
        // `Length<Positive>` minus two non-negative lengths is not a `Positive`.
        // The comparison is the same `f32` test a list uses for one indent.
        let room = avail.width.as_pt() - left.as_pt() - right.as_pt();
        if room <= 0.0 {
            return Err(GeometryKind::InsetWiderThanFrame {
                left: left.as_pt(),
                right: right.as_pt(),
                avail: avail.width.as_pt(),
            }
            .into());
        }
        let narrowed = Frame {
            width: Length::<Positive>::from_trusted_pt(room),
            ..avail
        };
        let shift = |fragment: Fragment| fragment.with_offset(left.into(), Finite::ZERO);
        let rewrap = |tail| InsetNode {
            left,
            right,
            child: Box::new(tail),
        };
        Ok(match (*child).split(ctx, narrowed)? {
            Split::Fit { height, fragment } => Split::Fit {
                height,
                fragment: shift(fragment)?,
            },
            Split::Partial {
                head_height,
                head,
                tail,
            } => Split::Partial {
                head_height,
                head: shift(head)?,
                tail: rewrap(tail),
            },
            Split::MoveToNextPage(tail) => Split::MoveToNextPage(rewrap(tail)),
        })
    }
}
