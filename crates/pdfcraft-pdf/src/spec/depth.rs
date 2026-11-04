//! Total nesting depth of a document, checked before lowering.

use crate::error::{Error, ErrorKind, LimitKind};

use super::{Block, ListSpec, SectionSpec, Span};

enum Node<'a, K> {
    Block(&'a Block<K>),
    Span(&'a Span<K>),
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
                stack.extend(
                    paragraph
                        .children
                        .iter()
                        .map(|span| (Node::Span(span), depth)),
                );
            }
            Block::KeepTogether { children }
            | Block::Styled { children, .. }
            | Block::Inset { children, .. }
            | Block::Anchor { children, .. } => {
                stack.extend(children.iter().map(|child| (Node::Block(child), inner)));
            }
            Block::Columns(columns) => {
                for column in &columns.children {
                    stack.extend(
                        column
                            .children
                            .iter()
                            .map(|child| (Node::Block(child), inner)),
                    );
                }
            }
            Block::Table(table) => {
                let headers = table.header.iter().flat_map(|header| &header.rows);
                for row in headers.chain(&table.body) {
                    for cell in &row.children {
                        stack.extend(
                            cell.children
                                .iter()
                                .map(|child| (Node::Block(child), inner)),
                        );
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
        Node::Span(span) => match span {
            Span::Link { children, .. } => {
                stack.extend(children.iter().map(|child| (Node::Span(child), inner)));
            }
            Span::Text { .. } | Span::Blank(_) => {}
        },
    }
}

fn push_items<'a, K>(list: &'a ListSpec<K>, depth: u32, stack: &mut Vec<(Node<'a, K>, u32)>) {
    for item in &list.children {
        stack.extend(
            item.children
                .iter()
                .map(|child| (Node::Block(child), depth)),
        );
    }
}

enum Owned<K> {
    Block(Block<K>),
    Span(Span<K>),
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
                        stack.extend(paragraph.children.into_iter().map(Owned::Span));
                    }
                    Block::KeepTogether { children }
                    | Block::Styled { children, .. }
                    | Block::Inset { children, .. }
                    | Block::Anchor { children, .. } => {
                        stack.extend(children.into_iter().map(Owned::Block));
                    }
                    Block::Columns(columns) => {
                        for column in columns.children {
                            stack.extend(column.children.into_iter().map(Owned::Block));
                        }
                    }
                    Block::Table(table) => {
                        let headers = table.header.into_iter().flat_map(|header| header.rows);
                        for row in headers.chain(table.body) {
                            for cell in row.children {
                                stack.extend(cell.children.into_iter().map(Owned::Block));
                            }
                        }
                    }
                    Block::List(list) => {
                        for item in list.children {
                            stack.extend(item.children.into_iter().map(Owned::Block));
                        }
                    }
                    Block::Spacer(_)
                    | Block::PageBreak
                    | Block::Image(_)
                    | Block::Canvas(_)
                    | Block::HLine(_) => {}
                },
                Owned::Span(span) => match span {
                    Span::Link { children, .. } => {
                        stack.extend(children.into_iter().map(Owned::Span));
                    }
                    Span::Text { .. } | Span::Blank(_) => {}
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::NonNegative;

    #[test]
    fn anchor_depth_walks_every_child() {
        let sections: Vec<SectionSpec<()>> = vec![SectionSpec {
            body: vec![Block::Anchor {
                id: "outer".into(),
                outline: None,
                children: vec![
                    Block::Spacer(NonNegative::ZERO),
                    Block::Anchor {
                        id: "inner".into(),
                        outline: None,
                        children: vec![Block::Spacer(NonNegative::ZERO)],
                    },
                ],
            }],
            ..SectionSpec::default()
        }];
        let error = check_depth(&sections, 1).unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::LimitExceeded {
                kind: LimitKind::NestingDepth,
                value: 2,
                max: 1,
            }
        );
    }

    #[test]
    fn dismantle_drops_a_deep_anchor_chain_iteratively() {
        let mut block: Block<()> = Block::Spacer(NonNegative::ZERO);
        for level in 0..10_000 {
            block = Block::Anchor {
                id: level.to_string(),
                outline: None,
                children: vec![block],
            };
        }
        dismantle(vec![SectionSpec {
            body: vec![block],
            ..SectionSpec::default()
        }]);
    }
}
