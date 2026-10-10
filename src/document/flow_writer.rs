#![allow(clippy::wildcard_imports)]
use super::*;

mod seal {
    pub trait Sealed {}
}

/// Value [`FlowWriter::add`] accepts.
///
/// This is the public admission trait. Each impl lowers the element and pushes
/// the node onto `flow`, so the crate-private node type stays out of the
/// signature. Paragraphs use `flow`'s style as the parent style. A table,
/// columns block, or list keeps the style of the flow it was built on.
///
/// The trait is sealed. Callers pass [`Paragraph`], [`Table`], [`Columns`],
/// [`HLine`], or [`PlacedImage`].
pub trait IntoFlowItem: seal::Sealed {
    /// Lowers `self` and appends it to `flow`.
    fn add_to(self, flow: &mut Flow);
}

impl seal::Sealed for Paragraph {}
impl IntoFlowItem for Paragraph {
    fn add_to(self, flow: &mut Flow) {
        flow.push(lower_paragraph(self, &flow.style));
    }
}

impl seal::Sealed for HLine {}
impl IntoFlowItem for HLine {
    fn add_to(self, flow: &mut Flow) {
        flow.push(self.lower());
    }
}

impl seal::Sealed for Table {}
impl IntoFlowItem for Table {
    fn add_to(self, flow: &mut Flow) {
        flow.push(self.lower());
    }
}

impl seal::Sealed for Columns {}
impl IntoFlowItem for Columns {
    fn add_to(self, flow: &mut Flow) {
        flow.push(self.lower());
    }
}

/// Image plus display box, ready for [`FlowWriter::add`].
#[derive(Debug)]
pub struct PlacedImage {
    image: Image,
    box_: ImageBox,
}

impl PlacedImage {
    /// Runs `f` on a default [`ImageBox`].
    pub fn build(image: &Image, f: impl FnOnce(&mut ImageBox)) -> Self {
        let mut box_ = ImageBox::default();
        f(&mut box_);
        Self {
            image: *image,
            box_,
        }
    }
}

impl seal::Sealed for PlacedImage {}
impl IntoFlowItem for PlacedImage {
    fn add_to(self, flow: &mut Flow) {
        flow.push(self.box_.lower(&self.image, flow.doc));
    }
}

struct BuiltList {
    list: List,
    kind: ListKind,
}

impl seal::Sealed for BuiltList {}
impl IntoFlowItem for BuiltList {
    fn add_to(self, flow: &mut Flow) {
        let kind = match self.kind {
            ListKind::Ordered { style, .. } => ListKind::Ordered {
                style,
                start: self.list.start,
            },
            ListKind::Bullet => ListKind::Bullet,
        };
        flow.push(self.list.lower(kind));
    }
}

/// Types that append body content: [`Flow`], [`Section`], and [`Document`].
///
/// Everyday callers use the sugar methods. Prefer [`FlowWriter::add`] when a
/// helper builds an element value and returns it.
pub trait FlowWriter {
    /// Target flow for this writer.
    ///
    /// On [`Document`], this is the implicit section or the rejecting flow after
    /// an explicit [`Document::section`]. Not the everyday entry point.
    fn flow_mut(&mut self) -> &mut Flow;

    /// Appends a pre-built flow item.
    ///
    /// Advanced path for factories that return an element. Everyday code should
    /// call the sugar methods instead. [`Self::stack`] and
    /// [`Self::keep_together`] do not build a value for this method. They write
    /// through [`Self::flow_mut`].
    fn add(&mut self, item: impl IntoFlowItem) {
        item.add_to(self.flow_mut());
    }

    /// Appends a paragraph. It wraps at the width of wherever it lands.
    fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) {
        self.add(Paragraph::build(f));
    }

    /// Appends columns side by side. `col` stays equal-ratio. Gap defaults to 12 pt.
    /// Needs at least two columns.
    fn columns(&mut self, f: impl FnOnce(&mut Columns)) {
        let item = {
            let flow = self.flow_mut();
            Columns::build_on(flow, f)
        };
        self.add(item);
    }

    /// Appends a borderless table with equal-width columns.
    ///
    /// The column count is the span sum of the first row.
    fn table(&mut self, f: impl FnOnce(&mut Table)) {
        let item = {
            let flow = self.flow_mut();
            Table::build_on(flow, f)
        };
        self.add(item);
    }

    /// Appends a bulleted list.
    fn ul(&mut self, f: impl FnOnce(&mut List)) {
        let item = {
            let flow = self.flow_mut();
            BuiltList {
                list: List::build_on(flow, f),
                kind: ListKind::Bullet,
            }
        };
        self.add(item);
    }

    /// Appends a numbered list. Numbering starts at 1 unless [`List::start`] says otherwise.
    fn ol(&mut self, numbering: Numbering, f: impl FnOnce(&mut List)) {
        let item = {
            let flow = self.flow_mut();
            BuiltList {
                list: List::build_on(flow, f),
                kind: ListKind::Ordered {
                    style: numbering_kind(numbering),
                    start: NonZeroU32::MIN,
                },
            }
        };
        self.add(item);
    }

    /// Appends an image that moves to the next page whole instead of splitting.
    ///
    /// Wider than the frame, or taller than the page body, is an error. The image never shrinks.
    fn image(&mut self, image: &Image, f: impl FnOnce(&mut ImageBox)) {
        self.add(PlacedImage::build(image, f));
    }

    /// Appends a horizontal line that moves to the next page whole instead of splitting.
    ///
    /// Longer than the frame, or taller than the page body, is an error. The line never shrinks.
    fn hline(&mut self, f: impl FnOnce(&mut HLine)) {
        self.add(HLine::build(f));
    }

    /// Appends vertical space. The part that does not fit continues on the next page.
    fn spacer(&mut self, height: impl IntoNonNegative) {
        self.flow_mut().push(
            height
                .into_non_negative()
                .map(|height| FlowNode::Spacer(SpacerNode { height })),
        );
    }

    /// Starts the next content on a new page. Only allowed in the body.
    fn page_break(&mut self) {
        self.flow_mut().push(Ok(FlowNode::PageBreak));
    }

    /// Appends content that moves to the next page as one piece instead of splitting.
    ///
    /// Flow-control sugar. It writes through [`Self::flow_mut`] and is not an
    /// independent block value like [`Table`].
    fn keep_together(&mut self, f: impl FnOnce(&mut Flow)) {
        let flow = self.flow_mut();
        let node = flow
            .nested(f)
            .map(|nodes| FlowNode::KeepTogether(KeepTogetherNode::Open(nodes)));
        flow.push(node);
    }

    /// Runs `body` with a temporary parent text style. Children inherit unless they override.
    ///
    /// Like pdfmake's `stack` carrying `fontSize` / `color`: unset fields keep the outer style,
    /// and the previous parent style is restored when `body` returns.
    ///
    /// Flow-control sugar. It writes through [`Self::flow_mut`] and is not an
    /// independent block value like [`Table`].
    fn stack(&mut self, style: impl FnOnce(&mut TextProps), body: impl FnOnce(&mut Flow)) {
        let flow = self.flow_mut();
        let saved = flow.style.clone();
        let mut props = TextProps::default();
        style(&mut props);
        if let Err(error) = props.apply(&mut flow.style) {
            if flow.error.is_none() {
                flow.error = Some(error);
            }
            flow.style = saved;
            return;
        }
        body(flow);
        flow.style = saved;
    }
}

impl FlowWriter for Flow {
    fn flow_mut(&mut self) -> &mut Flow {
        self
    }
}

impl FlowWriter for Section<'_> {
    fn flow_mut(&mut self) -> &mut Flow {
        &mut self.flow
    }
}

impl FlowWriter for Document<'_> {
    fn flow_mut(&mut self) -> &mut Flow {
        match &mut self.body {
            Sections::Implicit(section) => &mut section.flow,
            Sections::Explicit { closed, .. } => closed,
        }
    }
}
