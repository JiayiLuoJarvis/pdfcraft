//! The concrete flow node enum and its split dispatch.
//!
//! Not part of the public API. Callers never implement this protocol.

#![cfg_attr(not(test), allow(dead_code))]

use crate::error::{ErrorKind, GeometryKind, Result};
use crate::geom::{Length, NonNegative};
use crate::layout::columns::ColumnsNode;
use crate::layout::frame::{natural_height, Flowable, Fragment, Frame, LayoutCtx, NodeKind, Split};
use crate::layout::hline::HLineNode;
use crate::layout::image::ImageNode;
use crate::layout::list::ListNode;
use crate::layout::paragraph::{ParagraphNode, TextNode};
use crate::layout::table::TableNode;

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
    Text(TextNode),
    Spacer(SpacerNode),
    PageBreak,
    KeepTogether(KeepTogetherNode),
    Stack(Vec<FlowNode>),
    Columns(ColumnsNode<FlowNode>),
    Table(TableNode<FlowNode>),
    List(ListNode<FlowNode>),
    Image(ImageNode),
    HLine(HLineNode),
}

impl Flowable for FlowNode {
    fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split<Self>> {
        avail.validate()?;
        Ok(match self {
            FlowNode::PageBreak => return Err(GeometryKind::NestedPageBreak.into()),
            FlowNode::Paragraph(node) => node.split(avail)?.map(FlowNode::Paragraph),
            FlowNode::Text(node) => node.split(ctx, avail)?.map(FlowNode::Paragraph),
            FlowNode::Spacer(node) => node.split(avail)?.map(FlowNode::Spacer),
            FlowNode::KeepTogether(node) => split_keep_together(ctx, avail, node)?,
            FlowNode::Stack(kids) => split_stack(ctx, avail, kids)?,
            FlowNode::Columns(node) => node.split(ctx, avail)?.map(FlowNode::Columns),
            FlowNode::Table(node) => node.split(ctx, avail)?.map(FlowNode::Table),
            FlowNode::List(node) => node.split(ctx, avail)?.map(FlowNode::List),
            FlowNode::Image(node) => node.split(avail)?.map(FlowNode::Image),
            FlowNode::HLine(node) => node.split(avail)?.map(FlowNode::HLine),
        })
    }

    fn stack(nodes: Vec<Self>) -> Self {
        FlowNode::Stack(nodes)
    }

    fn kind(&self) -> NodeKind {
        match self {
            FlowNode::Paragraph(_) | FlowNode::Text(_) => NodeKind::Paragraph,
            FlowNode::Spacer(_) => NodeKind::Spacer,
            FlowNode::PageBreak => NodeKind::PageBreak,
            FlowNode::KeepTogether(_) => NodeKind::KeepTogether,
            FlowNode::Stack(_) => NodeKind::Stack,
            FlowNode::Columns(_) => NodeKind::Columns,
            FlowNode::Table(_) => NodeKind::Table,
            FlowNode::List(_) => NodeKind::List,
            FlowNode::Image(_) => NodeKind::Image,
            FlowNode::HLine(_) => NodeKind::HLine,
        }
    }

    fn nested(&self) -> &[Self] {
        match self {
            FlowNode::Stack(kids) | FlowNode::KeepTogether(KeepTogetherNode::Open(kids)) => kids,
            FlowNode::Columns(columns) => columns.column_stacks(),
            _ => &[],
        }
    }
}

impl SpacerNode {
    pub(crate) fn split(self, avail: Frame) -> Result<Split<Self>> {
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
                tail: SpacerNode {
                    height: NonNegative::pt(rest)?,
                },
            })
        }
    }
}

fn split_keep_together(
    ctx: &mut LayoutCtx<'_>,
    avail: Frame,
    node: KeepTogetherNode,
) -> Result<Split<FlowNode>> {
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
        Err(ErrorKind::UnsplittableTallerThanFrame {
            height: prepared.height,
            avail: avail.full_height,
        }
        .into())
    }
}

fn split_stack(
    ctx: &mut LayoutCtx<'_>,
    mut avail: Frame,
    mut kids: Vec<FlowNode>,
) -> Result<Split<FlowNode>> {
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
                let offset = child_frag.with_offset(0.0, y)?;
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
                let offset = head.with_offset(0.0, y)?;
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
