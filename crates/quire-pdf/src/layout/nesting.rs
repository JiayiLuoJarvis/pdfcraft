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

/// Rejects nodes `container` may not hold, recursing into stacks and keep-together groups.
pub(crate) fn check_content<N: Flowable>(container: Container, nodes: &[N]) -> Result<()> {
    for node in nodes {
        let kind = node.kind();
        let allowed = match kind {
            NodeKind::Paragraph | NodeKind::Spacer | NodeKind::HLine => true,
            NodeKind::Stack => {
                check_content(container, node.nested())?;
                true
            }
            NodeKind::KeepTogether => {
                if container == Container::Column {
                    check_content(container, node.nested())?;
                }
                container == Container::Column
            }
            NodeKind::Columns => match container {
                Container::Column => true,
                Container::Band => {
                    check_content(container, node.nested())?;
                    true
                }
                Container::TableCell | Container::ListItem => false,
            },
            NodeKind::Table => container == Container::Column,
            NodeKind::List => container != Container::Band,
            NodeKind::Image => matches!(container, Container::Column | Container::TableCell),
            NodeKind::PageBreak => false,
        };
        if !allowed {
            return Err(Error::from(ErrorKind::InvalidCellContent {
                container: container.name(),
                node: kind_name(kind),
            }));
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
        NodeKind::Stack => "stack",
        NodeKind::Columns => "columns",
        NodeKind::Table => "table",
        NodeKind::List => "list",
        NodeKind::Image => "image",
        NodeKind::HLine => "horizontal line",
    }
}

/// Table cell content that passed [`check_content`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellBody<N>(Vec<N>);

impl<N: Flowable> CellBody<N> {
    pub(crate) fn new(nodes: Vec<N>) -> Result<Self> {
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
    pub(crate) fn new(nodes: Vec<N>) -> Result<Self> {
        check_content(Container::ListItem, &nodes)?;
        Ok(Self(nodes))
    }

    pub(crate) fn into_node(self) -> N {
        N::stack(self.0)
    }
}
