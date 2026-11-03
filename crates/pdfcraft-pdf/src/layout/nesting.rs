//! Which nodes each nested container may hold.
//!
//! Holds the one content rule matrix, plus the checked bodies of table cells and list items.

use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::layout::frame::{Flowable, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Container {
    Column,
    TableCell,
    ListItem,
    Band,
}

impl Container {
    fn name(self) -> &'static str {
        match self {
            Container::Column => "column",
            Container::TableCell => "table cell",
            Container::ListItem => "list item",
            Container::Band => "header/footer band",
        }
    }
}

/// What `container` does with one child node.
enum Rule {
    Allow,
    /// Allow, and check the node's [`Flowable::nested`] children under the same container.
    Nested,
    /// Allow, and check every list item body under the same container.
    ListBodies,
    Deny,
}

fn rule(container: Container, kind: NodeKind) -> Rule {
    use Container::{Band, Column, ListItem, TableCell};
    match (kind, container) {
        (
            NodeKind::Paragraph | NodeKind::Spacer | NodeKind::HLine,
            Column | TableCell | ListItem | Band,
        )
        | (NodeKind::Columns | NodeKind::Table | NodeKind::List, Column | ListItem)
        | (NodeKind::Image | NodeKind::Canvas, Column | TableCell | ListItem) => Rule::Allow,
        (NodeKind::Stack | NodeKind::Anchor, Column | TableCell | ListItem | Band)
        | (NodeKind::KeepTogether, Column | ListItem)
        | (NodeKind::Columns, Band) => Rule::Nested,
        (NodeKind::List, TableCell) => Rule::ListBodies,
        (NodeKind::KeepTogether | NodeKind::Table, TableCell | Band)
        | (NodeKind::Columns, TableCell)
        | (NodeKind::List | NodeKind::Image | NodeKind::Canvas, Band)
        | (NodeKind::PageBreak, Column | TableCell | ListItem | Band) => Rule::Deny,
    }
}

/// Rejects nodes `container` may not hold, recursing where [`rule`] says so.
pub(crate) fn check_content<N: Flowable>(container: Container, nodes: &[N]) -> Result<()> {
    for node in nodes {
        let kind = node.kind();
        match rule(container, kind) {
            Rule::Allow => {}
            Rule::Nested => check_content(container, node.nested())?,
            Rule::ListBodies => {
                for body in node.list_bodies() {
                    check_content(container, std::slice::from_ref(body))?;
                }
            }
            Rule::Deny => {
                return Err(Error::from(ErrorKind::InvalidCellContent {
                    container: container.name(),
                    node: kind_name(kind),
                }));
            }
        }
    }
    Ok(())
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Paragraph => "paragraph",
        NodeKind::Spacer => "spacer",
        NodeKind::PageBreak => "page break",
        NodeKind::KeepTogether => "keep together",
        NodeKind::Anchor => "anchor",
        NodeKind::Stack => "stack",
        NodeKind::Columns => "columns",
        NodeKind::Table => "table",
        NodeKind::List => "list",
        NodeKind::Image => "image",
        NodeKind::Canvas => "canvas",
        NodeKind::HLine => "horizontal line",
    }
}

/// Table cell content that passed [`check_content`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellBody<N>(Vec<N>);

impl<N: Flowable> CellBody<N> {
    pub(crate) fn new(nodes: impl IntoIterator<Item = N>) -> Result<Self> {
        let nodes: Vec<N> = nodes.into_iter().collect();
        check_content(Container::TableCell, &nodes)?;
        Ok(Self(nodes))
    }

    pub(crate) fn into_node(self) -> N {
        N::stack(self.0)
    }

    pub(crate) fn nodes(&self) -> &[N] {
        &self.0
    }
}

/// List item content that passed [`check_content`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ItemBody<N>(Vec<N>);

impl<N: Flowable> ItemBody<N> {
    pub(crate) fn new(nodes: impl IntoIterator<Item = N>) -> Result<Self> {
        let nodes: Vec<N> = nodes.into_iter().collect();
        check_content(Container::ListItem, &nodes)?;
        Ok(Self(nodes))
    }

    pub(crate) fn into_node(self) -> N {
        N::stack(self.0)
    }
}
