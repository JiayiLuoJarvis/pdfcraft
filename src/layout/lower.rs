//! Lower public document nodes into `FlowNode`s.
//!
//! Holds the one content rule matrix for nested containers.

#![allow(dead_code)]

use crate::error::{Error, Result};
use crate::layout::flow::{FlowNode, KeepTogetherNode};

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
pub(crate) fn check_content(container: Container, nodes: &[FlowNode]) -> Result<()> {
    for node in nodes {
        let allowed = match node {
            FlowNode::Text(_)
            | FlowNode::Paragraph(_)
            | FlowNode::Spacer(_)
            | FlowNode::HLine(_) => true,
            FlowNode::Stack(kids) => {
                check_content(container, kids)?;
                true
            }
            FlowNode::KeepTogether(group) => {
                if let (Container::Column, KeepTogetherNode::Open(kids)) = (container, group) {
                    check_content(container, kids)?;
                }
                container == Container::Column
            }
            FlowNode::Columns(columns) => match container {
                Container::Column => true,
                Container::Band => {
                    check_content(container, columns.column_stacks())?;
                    true
                }
                Container::TableCell | Container::ListItem => false,
            },
            FlowNode::Table(_) => container == Container::Column,
            FlowNode::List(_) => container != Container::Band,
            FlowNode::Image(_) => matches!(container, Container::Column | Container::TableCell),
            FlowNode::PageBreak => false,
        };
        if !allowed {
            return Err(Error::InvalidCellContent {
                container: container.name(),
                node: node_name(node),
            });
        }
    }
    Ok(())
}

fn node_name(node: &FlowNode) -> &'static str {
    match node {
        FlowNode::Paragraph(_) | FlowNode::Text(_) => "paragraph",
        FlowNode::Spacer(_) => "spacer",
        FlowNode::PageBreak => "page break",
        FlowNode::KeepTogether(_) => "keep together",
        FlowNode::Stack(_) => "stack",
        FlowNode::Columns(_) => "columns",
        FlowNode::Table(_) => "table",
        FlowNode::List(_) => "list",
        FlowNode::Image(_) => "image",
        FlowNode::HLine(_) => "horizontal line",
    }
}

/// Table cell content that passed [`check_content`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellBody(Vec<FlowNode>);

impl CellBody {
    pub(crate) fn new(nodes: Vec<FlowNode>) -> Result<Self> {
        check_content(Container::TableCell, &nodes)?;
        Ok(Self(nodes))
    }

    pub(crate) fn into_node(self) -> FlowNode {
        FlowNode::Stack(self.0)
    }
}

/// List item content that passed [`check_content`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ItemBody(Vec<FlowNode>);

impl ItemBody {
    pub(crate) fn new(nodes: Vec<FlowNode>) -> Result<Self> {
        check_content(Container::ListItem, &nodes)?;
        Ok(Self(nodes))
    }

    pub(crate) fn into_node(self) -> FlowNode {
        FlowNode::Stack(self.0)
    }
}
