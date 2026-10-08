//! Internal flow nodes: consume-self `split` and fragment `draw`.
//!
//! Not part of the public API. Callers never implement this protocol.

#![allow(dead_code)]

use crate::canvas::{Canvas, GlyphRun};
use crate::error::{Error, Result};
use crate::fonts::FontStore;
use crate::geom::{Length, NonNegative, Point};
use crate::layout::paragraph::ParagraphNode;

/// Fonts available while splitting.
pub(crate) struct LayoutCtx<'a> {
    pub(crate) fonts: &'a mut FontStore,
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
        if !self.width.is_finite() || self.width <= 0.0 {
            return Err(Error::InvalidGeometry);
        }
        if !self.height.is_finite() || self.height < 0.0 {
            return Err(Error::InvalidGeometry);
        }
        if !self.full_height.is_finite() || self.full_height <= 0.0 {
            return Err(Error::InvalidGeometry);
        }
        Ok(())
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

    fn with_offset(mut self, dy: f32) -> Result<Self> {
        for command in &mut self.commands {
            match command {
                FragmentCommand::Glyphs(run) => {
                    run.origin =
                        Point::try_pt(run.origin.x().as_pt(), run.origin.y().as_pt() + dy)?;
                }
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FragmentCommand {
    Glyphs(GlyphRun),
}

/// Result of consuming a [`FlowNode`] against a frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Split {
    Fit {
        height: f32,
        fragment: Fragment,
    },
    /// Head fits this frame; only `tail` continues on later frames.
    Partial {
        head_height: f32,
        head: Fragment,
        tail: FlowNode,
    },
    MoveToNextPage(FlowNode),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpacerNode {
    pub(crate) height: Length<NonNegative>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedGroup {
    pub(crate) height: f32,
    pub(crate) spacing_after: f32,
    pub(crate) fragment: Fragment,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum KeepTogetherNode {
    Open(Vec<FlowNode>),
    Prepared(PreparedGroup),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FlowNode {
    Paragraph(ParagraphNode),
    Spacer(SpacerNode),
    PageBreak,
    KeepTogether(KeepTogetherNode),
    Stack(Vec<FlowNode>),
}

impl FlowNode {
    pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> {
        avail.validate()?;
        match self {
            FlowNode::PageBreak => Err(Error::InvalidGeometry),
            FlowNode::Paragraph(node) => node.split(avail),
            FlowNode::Spacer(node) => node.split(avail),
            FlowNode::KeepTogether(node) => split_keep_together(ctx, avail, node),
            FlowNode::Stack(kids) => split_stack(ctx, avail, kids),
        }
    }
}

impl SpacerNode {
    pub(crate) fn split(self, avail: Frame) -> Result<Split> {
        let h = self.height.as_pt();
        if h == 0.0 {
            return Ok(Split::Fit {
                height: 0.0,
                fragment: Fragment::empty(),
            });
        }
        let take = h.min(avail.height);
        let rest = h - take;
        let head = Fragment {
            height: take,
            spacing_after: 0.0,
            commands: Vec::new(),
        };
        if rest <= 0.0 {
            Ok(Split::Fit {
                height: take,
                fragment: head,
            })
        } else {
            Ok(Split::Partial {
                head_height: take,
                head,
                tail: FlowNode::Spacer(SpacerNode {
                    height: NonNegative::pt(rest)?,
                }),
            })
        }
    }
}

fn split_keep_together(
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    node: KeepTogetherNode,
) -> Result<Split> {
    let prepared = match node {
        KeepTogetherNode::Prepared(prepared) => prepared,
        KeepTogetherNode::Open(kids) => {
            let full = Frame {
                width: avail.width,
                height: avail.full_height,
                full_height: avail.full_height,
            };
            let (height, mut fragment) = natural_height(FlowNode::Stack(kids), ctx, full)?;
            let spacing_after = fragment.spacing_after;
            fragment.spacing_after = 0.0;
            PreparedGroup {
                height,
                spacing_after,
                fragment,
            }
        }
    };

    if prepared.height <= avail.height {
        let mut fragment = prepared.fragment;
        fragment.spacing_after = prepared.spacing_after;
        Ok(Split::Fit {
            height: prepared.height,
            fragment,
        })
    } else if prepared.height <= avail.full_height {
        Ok(Split::MoveToNextPage(FlowNode::KeepTogether(
            KeepTogetherNode::Prepared(prepared),
        )))
    } else {
        Err(Error::UnsplittableTallerThanFrame)
    }
}

fn split_stack(
    ctx: &mut LayoutCtx<'_>,
    mut avail: Frame,
    mut kids: Vec<FlowNode>,
) -> Result<Split> {
    if kids.is_empty() {
        return Ok(Split::Fit {
            height: 0.0,
            fragment: Fragment::empty(),
        });
    }

    let mut commands = Vec::new();
    let mut y = 0.0f32;
    let mut trailing_spacing = 0.0f32;
    let mut placed_any = false;

    while !kids.is_empty() {
        let child = kids.remove(0);
        match child.split(ctx, avail)? {
            Split::Fit {
                height,
                fragment: child_frag,
            } => {
                let spacing = child_frag.spacing_after;
                let offset = child_frag.with_offset(y)?;
                commands.extend(offset.commands);
                y += height;
                avail.height = (avail.height - height).max(0.0);
                placed_any = true;

                if kids.is_empty() {
                    trailing_spacing = spacing;
                } else if spacing > 0.0 && avail.height >= spacing {
                    y += spacing;
                    avail.height -= spacing;
                }
            }
            Split::Partial {
                head_height,
                head,
                tail,
            } => {
                let offset = head.with_offset(y)?;
                commands.extend(offset.commands);
                y += head_height;
                let mut rest = vec![tail];
                rest.append(&mut kids);
                return Ok(Split::Partial {
                    head_height: y,
                    head: Fragment {
                        height: y,
                        spacing_after: 0.0,
                        commands,
                    },
                    tail: FlowNode::Stack(rest),
                });
            }
            Split::MoveToNextPage(node) => {
                if !placed_any {
                    let mut rest = vec![node];
                    rest.append(&mut kids);
                    return Ok(Split::MoveToNextPage(FlowNode::Stack(rest)));
                }
                let mut rest = vec![node];
                rest.append(&mut kids);
                return Ok(Split::Partial {
                    head_height: y,
                    head: Fragment {
                        height: y,
                        spacing_after: 0.0,
                        commands,
                    },
                    tail: FlowNode::Stack(rest),
                });
            }
        }
    }

    Ok(Split::Fit {
        height: y,
        fragment: Fragment {
            height: y,
            spacing_after: trailing_spacing,
            commands,
        },
    })
}

/// Full-frame natural height. Non-Fit splits become [`Error::UnsplittableTallerThanFrame`].
pub(crate) fn natural_height(
    node: FlowNode,
    ctx: &mut LayoutCtx<'_>,
    full: Frame,
) -> Result<(f32, Fragment)> {
    match node.split(ctx, full)? {
        Split::Fit { height, fragment } => Ok((height, fragment)),
        Split::Partial { .. } | Split::MoveToNextPage(_) => Err(Error::UnsplittableTallerThanFrame),
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
        }
    }
    Ok(())
}
