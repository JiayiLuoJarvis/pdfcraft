//! PDF open, widget snapshot, and write primitives.

mod read;
mod snapshot;
mod write;

use lopdf::Document;

pub(crate) use snapshot::WidgetSnap;

pub(crate) struct PdfDoc(pub(super) Document);

impl PdfDoc {
    /// A separate document. Painting this copy leaves the original in place.
    pub(crate) fn duplicate(&self) -> Self {
        Self(self.0.clone())
    }
}
