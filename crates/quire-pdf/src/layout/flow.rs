//! The concrete flow node enum and its split dispatch.
//!
//! Not part of the public API. Callers never implement this protocol.

use std::collections::VecDeque;

use crate::error::{Error, ErrorKind, Fallible as Result, GeometryKind};
use crate::geom::{Finite, Length, NonNegative};
use crate::layout::columns::ColumnsNode;
use crate::layout::frame::{natural_height, Flowable, Fragment, Frame, LayoutCtx, NodeKind, Split};
use crate::layout::hline::HLineNode;
use crate::layout::image::ImageNode;
use crate::layout::list::ListNode;
use crate::layout::paragraph::{ParagraphNode, TextNode};
use crate::layout::table::TableNode;
use crate::limits::{LimitKind, Limits};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpacerNode {
    pub(crate) height: Length<NonNegative>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedGroup {
    pub(crate) height: Length<NonNegative>,
    pub(crate) spacing_after: Length<NonNegative>,
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
        let h = self.height;
        if h == NonNegative::ZERO {
            return Ok(Split::Fit {
                height: NonNegative::ZERO,
                fragment: Fragment::empty(),
            });
        }
        let take = h.min(avail.height);
        let rest = (h - take).as_pt();
        let head = Fragment {
            height: take,
            spacing_after: NonNegative::ZERO,
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
                height: avail.full_height.into(),
                full_height: avail.full_height,
            };
            let (height, mut fragment) = natural_height(FlowNode::Stack(kids), ctx, full)?;
            let spacing_after = fragment.spacing_after;
            fragment.spacing_after = NonNegative::ZERO;
            PreparedGroup {
                height,
                spacing_after,
                fragment,
            }
        }
    };

    if prepared.height.as_pt() <= avail.height.as_pt() {
        let mut fragment = prepared.fragment;
        fragment.spacing_after = prepared.spacing_after;
        Ok(Split::Fit {
            height: prepared.height,
            fragment,
        })
    } else if prepared.height.as_pt() <= avail.full_height.as_pt() {
        Ok(Split::MoveToNextPage(FlowNode::KeepTogether(
            KeepTogetherNode::Prepared(prepared),
        )))
    } else {
        Err(ErrorKind::UnsplittableTallerThanFrame {
            height: prepared.height.as_pt(),
            avail: avail.full_height.as_pt(),
        }
        .into())
    }
}

fn split_stack(
    ctx: &mut LayoutCtx<'_>,
    mut avail: Frame,
    kids: Vec<FlowNode>,
) -> Result<Split<FlowNode>> {
    if kids.is_empty() {
        return Ok(Split::Fit {
            height: NonNegative::ZERO,
            fragment: Fragment::empty(),
        });
    }

    let mut kids: VecDeque<FlowNode> = kids.into();
    let mut commands = Vec::new();
    let mut y = NonNegative::ZERO;
    let mut trailing_spacing = NonNegative::ZERO;
    let mut placed_any = false;

    while let Some(child) = kids.pop_front() {
        match child.split(ctx, avail)? {
            Split::Fit {
                height,
                fragment: child_frag,
            } => {
                let spacing = child_frag.spacing_after;
                let offset = child_frag.with_offset(Finite::ZERO, y.into())?;
                commands.extend(offset.commands);
                y += height;
                avail.height = avail.height.saturating_sub(height);
                placed_any = true;

                if kids.is_empty() {
                    trailing_spacing = spacing;
                } else if spacing > NonNegative::ZERO && avail.height >= spacing {
                    y += spacing;
                    avail.height = avail.height.sub_exact(spacing);
                } else if spacing > NonNegative::ZERO && ctx.natural {
                    y += spacing;
                    avail.height = NonNegative::ZERO;
                }
            }
            Split::Partial {
                head_height,
                head,
                tail,
            } => {
                let offset = head.with_offset(Finite::ZERO, y.into())?;
                commands.extend(offset.commands);
                y += head_height;
                let mut rest = Vec::with_capacity(kids.len() + 1);
                rest.push(tail);
                rest.extend(kids);
                return Ok(Split::Partial {
                    head_height: y,
                    head: Fragment {
                        height: y,
                        // Only natural-height measurement needs a cut-off gap on the head.
                        spacing_after: if ctx.natural {
                            offset.spacing_after
                        } else {
                            NonNegative::ZERO
                        },
                        commands,
                    },
                    tail: FlowNode::Stack(rest),
                });
            }
            Split::MoveToNextPage(node) => {
                let mut rest = Vec::with_capacity(kids.len() + 1);
                rest.push(node);
                rest.extend(kids);
                if !placed_any {
                    return Ok(Split::MoveToNextPage(FlowNode::Stack(rest)));
                }
                return Ok(Split::Partial {
                    head_height: y,
                    head: Fragment {
                        height: y,
                        spacing_after: NonNegative::ZERO,
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

/// Rejects a body tree over `limits`.
///
/// A stack is a container and is not a block. Text inside table cells, columns,
/// lists, and keep-together groups counts. Header, footer, and layer hooks are
/// not in `roots`. Paragraph bytes are the UTF-8 length of text runs.
pub(crate) fn check_limits(roots: &[FlowNode], limits: &Limits) -> Result<()> {
    let census = census(roots);
    exceeded(
        LimitKind::Blocks,
        census.blocks,
        u64::from(limits.max_blocks()),
    )?;
    exceeded(
        LimitKind::TableCells,
        census.cells,
        u64::from(limits.max_table_cells()),
    )?;
    exceeded(
        LimitKind::ParagraphBytes,
        census.paragraph_bytes,
        limits.max_paragraph_bytes() as u64,
    )?;
    Ok(())
}

fn exceeded(kind: LimitKind, value: u64, max: u64) -> Result<()> {
    if value > max {
        Err(Error::from(ErrorKind::LimitExceeded { kind, value, max }))
    } else {
        Ok(())
    }
}

struct Census {
    blocks: u64,
    cells: u64,
    paragraph_bytes: u64,
}

fn census(roots: &[FlowNode]) -> Census {
    let mut census = Census {
        blocks: 0,
        cells: 0,
        paragraph_bytes: 0,
    };
    let mut pending: Vec<&FlowNode> = Vec::new();
    pending.extend(roots.iter().rev());
    while let Some(node) = pending.pop() {
        match node {
            FlowNode::Stack(kids) => pending.extend(kids.iter().rev()),
            FlowNode::KeepTogether(KeepTogetherNode::Open(kids)) => {
                census.blocks = census.blocks.saturating_add(1);
                pending.extend(kids.iter().rev());
            }
            FlowNode::KeepTogether(KeepTogetherNode::Prepared(_))
            | FlowNode::Spacer(_)
            | FlowNode::PageBreak
            | FlowNode::Image(_)
            | FlowNode::HLine(_) => {
                census.blocks = census.blocks.saturating_add(1);
            }
            FlowNode::Paragraph(paragraph) => {
                census.blocks = census.blocks.saturating_add(1);
                census.paragraph_bytes = census
                    .paragraph_bytes
                    .saturating_add(paragraph.text_bytes() as u64);
            }
            FlowNode::Text(text) => {
                census.blocks = census.blocks.saturating_add(1);
                census.paragraph_bytes = census
                    .paragraph_bytes
                    .saturating_add(text.byte_len() as u64);
            }
            FlowNode::Columns(columns) => {
                census.blocks = census.blocks.saturating_add(1);
                pending.extend(columns.column_stacks().iter().rev());
            }
            FlowNode::Table(table) => {
                census.blocks = census.blocks.saturating_add(1);
                census.cells = census.cells.saturating_add(table.open_cell_count() as u64);
                table.append_open_children(&mut pending);
            }
            FlowNode::List(list) => {
                census.blocks = census.blocks.saturating_add(1);
                list.append_bodies(&mut pending);
            }
        }
    }
    census
}
