//! Total nesting depth of a document, checked before lowering.

use crate::error::{Error, ErrorKind, LimitKind};

use super::{Block, CellBlock, InlineRun, ListSpec, SectionSpec};

enum Node<'a, K> {
    Block(&'a Block<K>),
    Cell(&'a CellBlock<K>),
    Run(&'a InlineRun<K>),
}

/// Fails when any element sits deeper than `max`. Section body blocks are at depth 0.
///
/// The error carries the deepest level found and the first section that exceeds `max`.
pub(super) fn check_depth<K>(sections: &[SectionSpec<K>], max: u32) -> Result<(), Error> {
    let mut stack: Vec<(Node<'_, K>, u32)> = Vec::new();
    let mut deepest = 0;
    let mut first_over = None;
    for (index, section) in sections.iter().enumerate() {
        stack.extend(section.body.iter().map(|block| (Node::Block(block), 0)));
        while let Some((node, depth)) = stack.pop() {
            deepest = deepest.max(depth);
            if depth > max && first_over.is_none() {
                first_over = Some(index);
            }
            push_children(&node, depth, &mut stack);
        }
    }
    match first_over {
        None => Ok(()),
        Some(index) => Err(Error::from(ErrorKind::LimitExceeded {
            kind: LimitKind::NestingDepth,
            value: u64::from(deepest),
            max: u64::from(max),
        })
        .in_section(index)),
    }
}

fn push_children<'a, K>(node: &Node<'a, K>, depth: u32, stack: &mut Vec<(Node<'a, K>, u32)>) {
    let inner = depth.saturating_add(1);
    match *node {
        Node::Block(block) => match block {
            Block::Paragraph(paragraph) => {
                stack.extend(paragraph.runs.iter().map(|run| (Node::Run(run), depth)));
            }
            Block::KeepTogether(children) | Block::Styled { children, .. } => {
                stack.extend(children.iter().map(|child| (Node::Block(child), inner)));
            }
            Block::Anchor { child, .. } => stack.push((Node::Block(child), inner)),
            Block::Columns(columns) => {
                for column in &columns.columns {
                    stack.extend(
                        column
                            .blocks
                            .iter()
                            .map(|child| (Node::Block(child), inner)),
                    );
                }
            }
            Block::Table(table) => {
                for row in table.header.iter().chain(&table.body) {
                    for cell in &row.cells {
                        stack.extend(cell.content.iter().map(|child| (Node::Cell(child), inner)));
                    }
                }
            }
            Block::List(list) => push_items(list, inner, stack),
            Block::Spacer(_)
            | Block::PageBreak
            | Block::Image(_)
            | Block::Canvas(_)
            | Block::HLine(_) => {}
        },
        Node::Cell(cell) => match cell {
            CellBlock::Paragraph(paragraph) => {
                stack.extend(paragraph.runs.iter().map(|run| (Node::Run(run), depth)));
            }
            CellBlock::List(list) => push_items(list, inner, stack),
            CellBlock::Anchor { child, .. } => stack.push((Node::Cell(child), inner)),
            CellBlock::Text { .. }
            | CellBlock::HLine(_)
            | CellBlock::Image(_)
            | CellBlock::Canvas(_)
            | CellBlock::Spacer(_) => {}
        },
        Node::Run(run) => match run {
            InlineRun::Link { runs, .. } => {
                stack.extend(runs.iter().map(|child| (Node::Run(child), inner)));
            }
            InlineRun::Text { .. } | InlineRun::Blank(_) => {}
        },
    }
}

fn push_items<'a, K>(list: &'a ListSpec<K>, depth: u32, stack: &mut Vec<(Node<'a, K>, u32)>) {
    for item in &list.items {
        stack.extend(item.blocks.iter().map(|child| (Node::Block(child), depth)));
    }
}

enum Owned<K> {
    Block(Block<K>),
    Cell(CellBlock<K>),
    Run(InlineRun<K>),
}

/// Drops `sections` one node at a time.
///
/// Drop glue recurses once per nesting level, and a tree that failed
/// [`check_depth`] can be arbitrarily deep.
pub(super) fn dismantle<K>(sections: Vec<SectionSpec<K>>) {
    let mut stack: Vec<Owned<K>> = Vec::new();
    for section in sections {
        stack.extend(section.body.into_iter().map(Owned::Block));
        while let Some(node) = stack.pop() {
            match node {
                Owned::Block(block) => match block {
                    Block::Paragraph(paragraph) => {
                        stack.extend(paragraph.runs.into_iter().map(Owned::Run));
                    }
                    Block::KeepTogether(children) | Block::Styled { children, .. } => {
                        stack.extend(children.into_iter().map(Owned::Block));
                    }
                    Block::Anchor { child, .. } => stack.push(Owned::Block(*child)),
                    Block::Columns(columns) => {
                        for column in columns.columns {
                            stack.extend(column.blocks.into_iter().map(Owned::Block));
                        }
                    }
                    Block::Table(table) => {
                        for row in table.header.into_iter().chain(table.body) {
                            for cell in row.cells {
                                stack.extend(cell.content.into_iter().map(Owned::Cell));
                            }
                        }
                    }
                    Block::List(list) => {
                        for item in list.items {
                            stack.extend(item.blocks.into_iter().map(Owned::Block));
                        }
                    }
                    Block::Spacer(_)
                    | Block::PageBreak
                    | Block::Image(_)
                    | Block::Canvas(_)
                    | Block::HLine(_) => {}
                },
                Owned::Cell(cell) => match cell {
                    CellBlock::Paragraph(paragraph) => {
                        stack.extend(paragraph.runs.into_iter().map(Owned::Run));
                    }
                    CellBlock::List(list) => {
                        for item in list.items {
                            stack.extend(item.blocks.into_iter().map(Owned::Block));
                        }
                    }
                    CellBlock::Anchor { child, .. } => stack.push(Owned::Cell(*child)),
                    CellBlock::Text { .. }
                    | CellBlock::HLine(_)
                    | CellBlock::Image(_)
                    | CellBlock::Canvas(_)
                    | CellBlock::Spacer(_) => {}
                },
                Owned::Run(run) => match run {
                    InlineRun::Link { runs, .. } => {
                        stack.extend(runs.into_iter().map(Owned::Run));
                    }
                    InlineRun::Text { .. } | InlineRun::Blank(_) => {}
                },
            }
        }
    }
}
