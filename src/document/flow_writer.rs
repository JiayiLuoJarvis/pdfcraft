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

impl PlacedImage {
    pub(crate) fn push_on(self, flow: &mut Flow) {
        if let Some(node) = self.box_.lower(&self.image, flow.doc, &mut flow.errors) {
            flow.push(node);
        }
    }
}

impl seal::Sealed for PlacedImage {}
impl IntoFlowItem for PlacedImage {
    fn add_to(self, flow: &mut Flow) {
        self.push_on(flow);
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
