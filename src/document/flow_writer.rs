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
/// Adding a [`Table`] rebinds its document token to `flow`, so a cell image
/// registered on that document resolves. Text style stays the build-time style.
///
/// The trait is sealed. Callers pass [`Paragraph`], [`Table`], [`Columns`],
/// [`HLine`], [`PlacedImage`], [`Spacer`], or [`ListBlock`].
pub trait IntoFlowItem: seal::Sealed {
    /// Lowers `self` and appends it to `flow`.
    fn add_to(self, flow: &mut Flow);
}

impl seal::Sealed for Paragraph {}
impl IntoFlowItem for Paragraph {
    fn add_to(self, flow: &mut Flow) {
        if let Some(node) = lower_paragraph(self, &flow.style, &mut flow.errors) {
            flow.push(node);
        }
    }
}

impl seal::Sealed for HLine {}
impl IntoFlowItem for HLine {
    fn add_to(self, flow: &mut Flow) {
        if let Some(node) = self.lower(&mut flow.errors) {
            flow.push(node);
        }
    }
}

impl seal::Sealed for Table {}
impl IntoFlowItem for Table {
    fn add_to(mut self, flow: &mut Flow) {
        // `Table::build` mints a fresh token. Cell images belong to `flow`'s document.
        self.parent.doc = flow.doc;
        if let Some(node) = self.lower(&mut flow.errors) {
            flow.push(node);
        }
    }
}

impl seal::Sealed for Columns {}
impl IntoFlowItem for Columns {
    fn add_to(self, flow: &mut Flow) {
        if let Some(node) = self.lower(&mut flow.errors) {
            flow.push(node);
        }
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
        if let Some(node) = self.box_.lower(&self.image, flow.doc, &mut flow.errors) {
            flow.push(node);
        }
    }
}

/// Vertical space that paints nothing. Height may be zero.
///
/// A negative or non-finite height is reported when the spacer is added, and
/// no space is inserted. In a body flow the part that does not fit continues on
/// the next page. In a cell it does not split; taller than the cell's inner
/// full frame is [`ErrorKind::UnsplittableTallerThanFrame`].
#[derive(Debug)]
#[must_use]
pub struct Spacer {
    height: Fallible<Length<NonNegative>>,
}

impl Spacer {
    /// Space `height` tall.
    pub fn new(height: impl IntoNonNegative) -> Self {
        Self {
            height: height.into_non_negative(),
        }
    }

    pub(crate) fn into_result(self) -> Fallible<Length<NonNegative>> {
        self.height
    }
}

impl seal::Sealed for Spacer {}
impl IntoFlowItem for Spacer {
    fn add_to(self, flow: &mut Flow) {
        match self.into_result() {
            Ok(height) => flow.push(FlowNode::Spacer(SpacerNode { height })),
            Err(error) => flow.push_err(error),
        }
    }
}

impl seal::Sealed for ListBlock<'_> {}
impl IntoFlowItem for ListBlock<'_> {
    fn add_to(self, flow: &mut Flow) {
        let built = self.build_on(flow);
        if let Some(node) = built.lower(&mut flow.errors) {
            flow.push(node);
        }
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
        self.add(ListBlock::bullet(f));
    }

    /// Appends a numbered list. Numbering starts at 1 unless [`List::start`] says otherwise.
    fn ol(&mut self, numbering: Numbering, f: impl FnOnce(&mut List)) {
        self.add(ListBlock::numbered(numbering, f));
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
        self.add(Spacer::new(height));
    }

    /// Starts the next content on a new page. Only allowed in the body.
    fn page_break(&mut self) {
        self.flow_mut().push(FlowNode::PageBreak);
    }

    /// Appends content that moves to the next page as one piece instead of splitting.
    ///
    /// Flow-control sugar. It writes through [`Self::flow_mut`] and is not an
    /// independent block value like [`Table`].
    fn keep_together(&mut self, f: impl FnOnce(&mut Flow)) {
        let flow = self.flow_mut();
        let nodes = flow.nested(f);
        flow.push(FlowNode::KeepTogether(KeepTogetherNode::Open(nodes)));
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
        if !props.apply(&mut flow.style, &mut flow.errors) {
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
