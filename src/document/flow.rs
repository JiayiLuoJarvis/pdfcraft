#![allow(clippy::wildcard_imports)]
use super::*;

/// Vertical content: the body, a column, a keep-together group, or a list item.
///
/// Body methods such as [`FlowWriter::paragraph`] come from [`FlowWriter`].
pub struct Flow {
    pub(crate) style: Style,
    pub(crate) doc: DocToken,
    pub(crate) nodes: Vec<FlowNode>,
    pub(crate) error: Option<Error>,
    pub(crate) reject_pushes: bool,
}

impl Flow {
    pub(crate) fn new(style: Style, doc: DocToken) -> Self {
        Self {
            style,
            doc,
            nodes: Vec::new(),
            error: None,
            reject_pushes: false,
        }
    }

    pub(crate) fn rejecting(style: Style, doc: DocToken) -> Self {
        Self {
            reject_pushes: true,
            ..Self::new(style, doc)
        }
    }

    pub(crate) fn written(&self) -> bool {
        !self.nodes.is_empty() || self.error.is_some()
    }

    pub(crate) fn child(&self) -> Self {
        Self::new(self.style.clone(), self.doc)
    }

    pub(crate) fn push(&mut self, node: Result<FlowNode>) {
        if self.error.is_some() {
            return;
        }
        if self.reject_pushes {
            self.error = Some(Error::from(ErrorKind::ContentOutsideSection));
            return;
        }
        match node {
            Ok(node) => self.nodes.push(node),
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn nested(&mut self, f: impl FnOnce(&mut Flow)) -> Result<Vec<FlowNode>> {
        let mut child = self.child();
        f(&mut child);
        child.into_nodes()
    }

    pub(crate) fn into_nodes(self) -> Result<Vec<FlowNode>> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.nodes),
        }
    }

    pub(crate) fn text(&self, text: String) -> FlowNode {
        FlowNode::Text(TextNode::new(text, self.style.clone()))
    }
}

pub(crate) fn lower_paragraph(paragraph: Paragraph, parent: &Style) -> Result<FlowNode> {
    if let Some(error) = paragraph.error {
        return Err(error);
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
    Ok(FlowNode::Text(TextNode::from_items(paragraph.items, style)))
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

pub(crate) fn assemble_list(
    parent: Flow,
    kind: ListKind,
    f: impl FnOnce(&mut List),
) -> Result<FlowNode> {
    let mut list = List {
        parent,
        items: Vec::new(),
        start: NonZeroU32::MIN,
        marker_width: Positive::pt(18.0).expect("18 pt is positive"),
        nest_indent: None,
        error: None,
    };
    f(&mut list);
    let kind = match kind {
        ListKind::Ordered { style, .. } => ListKind::Ordered {
            style,
            start: list.start,
        },
        ListKind::Bullet => ListKind::Bullet,
    };
    list.lower(kind)
}
