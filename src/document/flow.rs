#![allow(clippy::wildcard_imports)]
use super::*;

/// Vertical content: the body, a column, a keep-together group, or a list item.
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
            self.error = Some(Error::ContentOutsideSection);
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

    /// Runs `body` with a temporary parent text style. Children inherit unless they override.
    ///
    /// Like pdfmake's `stack` carrying `fontSize` / `color`: unset fields keep the outer style,
    /// and the previous parent style is restored when `body` returns.
    pub fn stack(&mut self, style: impl FnOnce(&mut TextProps), body: impl FnOnce(&mut Flow)) {
        let saved = self.style.clone();
        let mut props = TextProps::default();
        style(&mut props);
        if let Err(error) = props.apply(&mut self.style) {
            if self.error.is_none() {
                self.error = Some(error);
            }
            self.style = saved;
            return;
        }
        body(self);
        self.style = saved;
    }

    /// Appends an image that moves to the next page whole instead of splitting.
    ///
    /// Wider than the frame, or taller than the page body, is an error. The image never shrinks.
    pub fn image(&mut self, image: &Image, f: impl FnOnce(&mut ImageBox)) {
        let mut box_ = ImageBox::default();
        f(&mut box_);
        self.push(box_.lower(image, self.doc));
    }

    /// Appends a paragraph. It wraps at the width of wherever it lands.
    pub fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
        let mut paragraph = Paragraph::default();
        f(&mut paragraph);
        self.push(lower_paragraph(paragraph, &self.style));
    }

    /// Appends columns side by side. `col` stays equal-ratio. Gap defaults to 12 pt.
    /// Needs at least two columns.
    pub fn columns(&mut self, f: impl FnOnce(&mut Columns)) {
        let mut columns = Columns {
            parent: self.child(),
            cols: Vec::new(),
            gap: default_column_gap(),
        };
        f(&mut columns);
        let node = columns.lower();
        self.push(node);
    }

    /// Appends a borderless table with equal-width columns.
    ///
    /// The column count is the span sum of the first row.
    pub fn table(&mut self, f: impl FnOnce(&mut Table)) {
        let mut table = Table {
            parent: self.child(),
            header: Vec::new(),
            body: Vec::new(),
            repeat_header: true,
            tracks: None,
            padding: NonNegative::ZERO,
            border: Border {
                width: NonNegative::ZERO,
                color: Color::rgb(0, 0, 0),
            },
            error: None,
        };
        f(&mut table);
        let node = table.lower();
        self.push(node);
    }

    /// Appends content that moves to the next page as one piece instead of splitting.
    pub fn keep_together(&mut self, f: impl FnOnce(&mut Flow)) {
        let node = self
            .nested(f)
            .map(|nodes| FlowNode::KeepTogether(KeepTogetherNode::Open(nodes)));
        self.push(node);
    }

    /// Appends vertical space. The part that does not fit continues on the next page.
    pub fn spacer(&mut self, height: impl IntoNonNegative) {
        self.push(
            height
                .into_non_negative()
                .map(|height| FlowNode::Spacer(SpacerNode { height })),
        );
    }

    /// Appends a horizontal line that moves to the next page whole instead of splitting.
    ///
    /// Longer than the frame, or taller than the page body, is an error. The line never shrinks.
    pub fn hline(&mut self, f: impl FnOnce(&mut HLine)) {
        let mut line = HLine::default();
        f(&mut line);
        self.push(line.lower());
    }

    /// Starts the next content on a new page. Only allowed in the body.
    pub fn page_break(&mut self) {
        self.push(Ok(FlowNode::PageBreak));
    }

    /// Appends a bulleted list.
    pub fn ul(&mut self, f: impl FnOnce(&mut List)) {
        self.list(ListKind::Bullet, f);
    }

    /// Appends a numbered list. Numbering starts at 1 unless [`List::start`] says otherwise.
    pub fn ol(&mut self, numbering: Numbering, f: impl FnOnce(&mut List)) {
        self.list(
            ListKind::Ordered {
                style: numbering_kind(numbering),
                start: NonZeroU32::MIN,
            },
            f,
        );
    }

    pub(crate) fn list(&mut self, kind: ListKind, f: impl FnOnce(&mut List)) {
        self.push(assemble_list(self.child(), kind, f));
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
