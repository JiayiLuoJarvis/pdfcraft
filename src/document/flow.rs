#![allow(clippy::wildcard_imports)]
use super::*;

/// Vertical content: the body, a column, a keep-together group, or a list item.
pub struct Flow {
    pub(crate) style: Style,
    pub(crate) doc: DocToken,
    pub(crate) nodes: Vec<FlowNode>,
    pub(crate) errors: ErrorBag,
}

impl Flow {
    pub(crate) fn new(style: Style, doc: DocToken) -> Self {
        Self {
            style,
            doc,
            nodes: Vec::new(),
            errors: ErrorBag::default(),
        }
    }

    pub(crate) fn child(&self) -> Self {
        Self::new(self.style.clone(), self.doc)
    }

    /// Appends a lowered node.
    pub(crate) fn push(&mut self, node: FlowNode) {
        self.nodes.push(node);
    }

    /// Records `error` and does not append a node. Later siblings still run.
    pub(crate) fn push_err(&mut self, error: Error) {
        self.errors.record(error);
    }

    /// Runs `f` on a child flow and merges its bag. Failed nodes are already absent.
    pub(crate) fn nested(&mut self, f: impl FnOnce(&mut Flow)) -> Vec<FlowNode> {
        let mut child = self.child();
        f(&mut child);
        let (nodes, errors) = child.into_parts();
        self.errors.absorb(errors);
        nodes
    }

    pub(crate) fn into_parts(self) -> (Vec<FlowNode>, ErrorBag) {
        (self.nodes, self.errors)
    }

    pub(crate) fn text(&self, text: String) -> FlowNode {
        FlowNode::Text(TextNode::new(text, self.style.clone()))
    }
}

pub(crate) fn lower_paragraph(
    paragraph: Paragraph,
    parent: &Style,
    into: &mut ErrorBag,
) -> Option<FlowNode> {
    let failed = !paragraph.errors.is_empty();
    into.absorb(paragraph.errors);
    if failed {
        return None;
    }
    let mut style = parent.clone();
    style.first_line_indent = paragraph.first_line_indent;
    style.align = paragraph.align;
    if let Some(color) = paragraph.color {
        style.color = color;
    }
    if let Some(font_size) = paragraph.font_size {
        style.font_size = font_size;
    }
    Some(FlowNode::Text(TextNode::from_items(paragraph.items, style)))
}

pub(crate) fn numbering_kind(numbering: Numbering) -> MarkerKind {
    match numbering {
        Numbering::Decimal => MarkerKind::Decimal,
        Numbering::UpperAlpha => MarkerKind::UpperAlpha,
        Numbering::LowerAlpha => MarkerKind::LowerAlpha,
        Numbering::UpperRoman => MarkerKind::UpperRoman,
        Numbering::LowerRoman => MarkerKind::LowerRoman,
    }
}
