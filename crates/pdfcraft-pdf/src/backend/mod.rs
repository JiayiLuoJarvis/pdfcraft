//! Private PDF backend.
//!
//! Only this module tree may `use krilla::…`. Compiled under feature
//! `backend-krilla`.

pub(crate) mod convert;
pub(crate) mod emit;
pub(crate) mod images;
pub(crate) mod krilla_canvas;

use std::sync::Arc;

use crate::canvas::{self, Canvas, DocumentInfo, OutlineItem, RecordingCanvas};
use crate::error::Fallible as Result;
use crate::fonts::FontStore;

pub(crate) use images::ImageTable;
pub(crate) use krilla_canvas::KrillaCanvas;

/// Replays `recording` into a PDF with the fonts and images layout used.
pub(crate) fn write_pdf(
    recording: &RecordingCanvas,
    fonts: FontStore,
    images: ImageTable,
    info: &DocumentInfo,
    outline: &[OutlineItem],
) -> Result<Vec<u8>> {
    let mut pdf = KrillaCanvas::new_with_resources(Arc::new(fonts), images);
    pdf.set_document_info(info)?;
    pdf.set_outline(outline);
    canvas::replay(recording, &mut pdf)?;
    pdf.finish()
}
