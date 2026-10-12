//! The split/draw protocol shared by every flow node.
//!
//! Not part of the public API. Containers are generic over their child node,
//! so no node module names the concrete `FlowNode` enum.

use crate::canvas::{Canvas, DecodedImage, GlyphRun, LinkTarget};
use crate::error::{ErrorBag, ErrorKind, Fallible as Result, FontFault, GeometryKind, Location};
use crate::fonts::FontStore;
use crate::geom::{Paint, Path, Point, Rect, Stroke};

/// Fonts and the document error bag available while splitting.
pub(crate) struct LayoutCtx<'a> {
    pub(crate) fonts: &'a mut FontStore,
    pub(crate) errors: &'a mut ErrorBag,
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
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) full_height: f32,
}

impl Frame {
    pub(crate) fn validate(&self) -> Result<()> {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        if positive(self.width)
            && self.height.is_finite()
            && self.height >= 0.0
            && positive(self.full_height)
        {
            Ok(())
        } else {
            Err(GeometryKind::LayoutFrame {
                width: self.width,
                height: self.height,
                full_height: self.full_height,
            }
            .into())
        }
    }
}

/// One placed piece of content. Coordinates are relative to the fragment origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fragment {
    pub(crate) height: f32,
    pub(crate) spacing_after: f32,
    pub(crate) commands: Vec<FragmentCommand>,
}

impl Fragment {
    pub(crate) fn empty() -> Self {
        Self {
            height: 0.0,
            spacing_after: 0.0,
            commands: Vec::new(),
        }
    }

    pub(crate) fn with_offset(mut self, dx: f32, dy: f32) -> Result<Self> {
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
        height: f32,
        fragment: Fragment,
    },
    /// Head fits this frame; only `tail` continues on later frames.
    Partial {
        head_height: f32,
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
pub(crate) fn natural_height<N: Flowable>(
    node: N,
    ctx: &mut LayoutCtx<'_>,
    full: Frame,
) -> Result<(f32, Fragment)> {
    match node.split(ctx, full)? {
        Split::Fit { height, fragment } => Ok((height, fragment)),
        split => Err(ErrorKind::UnsplittableTallerThanFrame {
            height: stacked_height(split, ctx, full)?,
            avail: full.full_height,
        }
        .into()),
    }
}

/// Height of `split` plus every later piece laid out in fresh `full` frames.
///
/// Stops early when a fresh frame places nothing, so the sum never loops.
pub(crate) fn stacked_height<N: Flowable>(
    mut split: Split<N>,
    ctx: &mut LayoutCtx<'_>,
    full: Frame,
) -> Result<f32> {
    let fresh = Frame {
        height: full.full_height,
        ..full
    };
    let mut total = 0.0f32;
    loop {
        match split {
            Split::Fit { height, .. } => return Ok(total + height),
            Split::Partial {
                head_height, tail, ..
            } if head_height > 0.0 => {
                total += head_height;
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
        }
    }
    Ok(())
}
