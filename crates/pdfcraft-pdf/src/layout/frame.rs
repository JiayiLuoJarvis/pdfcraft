//! The split/draw protocol shared by every flow node.
//!
//! Not part of the public API. Containers are generic over their child node,
//! so no node module names the concrete `FlowNode` enum.

use crate::canvas::{Canvas, DecodedImage, GlyphRun, LinkTarget};
use crate::error::{Error, ErrorBag, ErrorKind, Fallible as Result, FontFault, Location};
use crate::fonts::FontStore;
use crate::geom::{
    Affine, Finite, Length, NonNegative, Paint, Path, Point, Positive, Rect, Stroke,
};

/// Fonts and the document error bag available while splitting.
pub(crate) struct LayoutCtx<'a> {
    pub(crate) fonts: &'a mut FontStore,
    pub(crate) errors: &'a mut ErrorBag,
    /// Keep-together and cell measurement count spacing between children.
    /// Pagination leaves this false and drops spacing that does not fit.
    pub(crate) natural: bool,
}

impl LayoutCtx<'_> {
    /// Records a font failure and keeps scanning. Other failures stay on [`Result`].
    pub(crate) fn soft(&mut self, fault: FontFault) {
        self.errors.record(fault);
    }

    /// Fills unknown location fields on errors recorded by `f`, and on a hard error.
    pub(crate) fn located<T>(
        &mut self,
        at: &Location,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let mark = self.errors.mark();
        let out = f(self);
        self.errors.locate_since(mark, at);
        out.map_err(|mut error| {
            error.fill_missing(at);
            error
        })
    }
}

/// Remaining layout region. `height` may be zero; `width` and `full_height` stay positive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Frame {
    pub(crate) width: Length<Positive>,
    pub(crate) height: Length<NonNegative>,
    pub(crate) full_height: Length<Positive>,
}

/// One placed piece of content. Coordinates are relative to the fragment origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fragment {
    pub(crate) height: Length<NonNegative>,
    pub(crate) spacing_after: Length<NonNegative>,
    pub(crate) commands: Vec<FragmentCommand>,
}

impl Fragment {
    pub(crate) fn empty() -> Self {
        Self {
            height: NonNegative::ZERO,
            spacing_after: NonNegative::ZERO,
            commands: Vec::new(),
        }
    }

    pub(crate) fn with_offset(mut self, dx: Length<Finite>, dy: Length<Finite>) -> Result<Self> {
        let dx = dx.as_pt();
        let dy = dy.as_pt();
        for command in &mut self.commands {
            match command {
                FragmentCommand::Glyphs(run) => {
                    run.origin =
                        Point::try_pt(run.origin.x().as_pt() + dx, run.origin.y().as_pt() + dy)?;
                }
                FragmentCommand::Fill { path, .. } | FragmentCommand::Stroke { path, .. } => {
                    *path = path.translated(dx, dy)?;
                }
                FragmentCommand::Image { rect, .. } | FragmentCommand::Link { rect, .. } => {
                    *rect = rect.translated(dx, dy)?;
                }
                FragmentCommand::PushTransform { affine } => {
                    *affine = Affine::translate(dx, dy).then(*affine);
                }
                FragmentCommand::PopTransform => {}
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
    Fill {
        path: Path,
        paint: Paint,
    },
    Stroke {
        path: Path,
        paint: Paint,
        stroke: Stroke,
    },
    Image {
        image: DecodedImage,
        rect: Rect,
    },
    Link {
        rect: Rect,
        target: LinkTarget,
    },
    PushTransform {
        affine: Affine,
    },
    PopTransform,
}

/// A fragment pinned to page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedFragment {
    pub(crate) origin: Point,
    pub(crate) fragment: Fragment,
}

/// Result of consuming a node against a frame. `T` is what continues on later frames.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Split<T> {
    Fit {
        height: Length<NonNegative>,
        fragment: Fragment,
    },
    /// Head fits this frame; only `tail` continues on later frames.
    Partial {
        head_height: Length<NonNegative>,
        head: Fragment,
        tail: T,
    },
    MoveToNextPage(T),
}

impl<T> Split<T> {
    pub(crate) fn map<U>(self, f: impl FnOnce(T) -> U) -> Split<U> {
        match self {
            Split::Fit { height, fragment } => Split::Fit { height, fragment },
            Split::Partial {
                head_height,
                head,
                tail,
            } => Split::Partial {
                head_height,
                head,
                tail: f(tail),
            },
            Split::MoveToNextPage(tail) => Split::MoveToNextPage(f(tail)),
        }
    }
}

/// What the nesting rules tell apart. Text and laid-out paragraphs share one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKind {
    Paragraph,
    Spacer,
    PageBreak,
    KeepTogether,
    Stack,
    Columns,
    Table,
    List,
    Image,
    Canvas,
    HLine,
}

/// A node containers can hold, split, and check.
pub(crate) trait Flowable: Sized {
    fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>>;

    /// Lays `nodes` out one after another.
    fn stack(nodes: Vec<Self>) -> Self;

    fn kind(&self) -> NodeKind;

    /// Children the nesting rules look into: stack and open keep-together members, column stacks.
    fn nested(&self) -> &[Self];
}

/// Full-frame natural height. Non-Fit splits become [`ErrorKind::UnsplittableTallerThanFrame`].
///
/// Spacing between children counts, including spacing that would not fit in `full`.
/// The last child's trailing spacing stays on the fragment and is not part of the height.
pub(crate) fn natural_height<N: Flowable>(
    node: N,
    ctx: &mut LayoutCtx<'_>,
    full: Frame,
) -> Result<(Length<NonNegative>, Fragment)> {
    let saved = ctx.natural;
    ctx.natural = true;
    let split = node.split(ctx, full);
    let result = match split {
        Ok(Split::Fit { height, fragment }) if height.as_pt() <= full.full_height.as_pt() => {
            Ok((height, fragment))
        }
        Ok(Split::Fit { height, .. }) => Err(too_tall(height.as_pt(), full.full_height.as_pt())),
        Ok(split) => match stacked_height(split, ctx, full) {
            Ok(height) => Err(too_tall(height.as_pt(), full.full_height.as_pt())),
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    };
    ctx.natural = saved;
    result
}

fn too_tall(height: f32, avail: f32) -> Error {
    ErrorKind::UnsplittableTallerThanFrame { height, avail }.into()
}

/// Height of `split` plus every later piece laid out in fresh `full` frames.
///
/// A head's `spacing_after` counts: it is the gap a break cut off inside a group.
/// Stops early when a fresh frame places nothing, so the sum never loops.
pub(crate) fn stacked_height<N: Flowable>(
    mut split: Split<N>,
    ctx: &mut LayoutCtx<'_>,
    full: Frame,
) -> Result<Length<NonNegative>> {
    let fresh = Frame {
        height: full.full_height.into(),
        ..full
    };
    let mut total = NonNegative::ZERO;
    loop {
        match split {
            Split::Fit { height, .. } => return Ok(total + height),
            Split::Partial {
                head_height,
                head,
                tail,
            } if head_height > NonNegative::ZERO => {
                total += head_height + head.spacing_after;
                split = tail.split(ctx, fresh)?;
            }
            Split::Partial { .. } | Split::MoveToNextPage(_) => return Ok(total),
        }
    }
}

pub(crate) fn draw_fragment(
    fragment: &Fragment,
    canvas: &mut dyn Canvas,
    origin: Point,
) -> Result<()> {
    let ox = origin.x().as_pt();
    let oy = origin.y().as_pt();
    let mut depth = 0usize;
    let drawn = (|| {
        for command in &fragment.commands {
            match command {
                FragmentCommand::Glyphs(run) => {
                    let mut placed = run.clone();
                    placed.origin =
                        Point::try_pt(ox + run.origin.x().as_pt(), oy + run.origin.y().as_pt())?;
                    canvas.draw_glyph_run(&placed)?;
                }
                FragmentCommand::Fill { path, paint } => {
                    canvas.fill_path(&path.translated(ox, oy)?, paint)?;
                }
                FragmentCommand::Stroke {
                    path,
                    paint,
                    stroke,
                } => canvas.stroke_path(&path.translated(ox, oy)?, paint, stroke)?,
                FragmentCommand::Image { image, rect } => {
                    canvas.draw_image(image, rect.translated(ox, oy)?)?;
                }
                FragmentCommand::Link { rect, target } => {
                    canvas.annotate_link(rect.translated(ox, oy)?, target.clone())?;
                }
                FragmentCommand::PushTransform { affine } => {
                    let composed = if ox == 0.0 && oy == 0.0 {
                        *affine
                    } else {
                        Affine::translate(ox, oy).then(*affine)
                    };
                    canvas.push_transform(&composed)?;
                    depth += 1;
                }
                FragmentCommand::PopTransform => {
                    canvas.pop_transform()?;
                    depth = depth.saturating_sub(1);
                }
            }
        }
        Ok(())
    })();
    while depth > 0 {
        let _ = canvas.pop_transform();
        depth -= 1;
    }
    drawn
}
