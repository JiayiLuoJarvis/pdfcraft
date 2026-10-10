//! Private PDF backend.
//!
//! Only this module tree may `use krilla::…`. Compiled under feature
//! `backend-krilla`.

#![allow(dead_code)]

pub(crate) mod convert;
pub(crate) mod emit;
pub(crate) mod images;
pub(crate) mod krilla_canvas;

use std::any::Any;
use std::sync::Arc;

use crate::canvas::{replay, ImageRegistry, RecordingCanvas};
use crate::error::Result;
use crate::fonts::FontStore;

pub(crate) use images::ImageTable;
pub(crate) use krilla_canvas::KrillaCanvas;

/// Replays `recording` into a PDF with the fonts and images layout used.
pub(crate) fn write_pdf(
    recording: &RecordingCanvas,
    fonts: FontStore,
    images: Box<dyn ImageRegistry>,
) -> Result<Vec<u8>> {
    let images: Box<dyn Any> = images;
    // Any other registry rejects every image, so it holds none.
    let images = images
        .downcast::<ImageTable>()
        .map_or_else(|_| ImageTable::default(), |table| *table);
    let mut pdf = KrillaCanvas::new_with_resources(Arc::new(fonts), images);
    replay(recording, &mut pdf)?;
    pdf.finish()
}
