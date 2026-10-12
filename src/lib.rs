//! Quire: PDF document layout on top of a sealed krilla backend.
//!
//! Public surface grows only after items in `docs/提案书.md` are confirmed.

#![forbid(unsafe_code)]

pub mod error;
pub mod geom;
pub mod limits;

pub use error::{
    CellIndex, CellSpanReason, ColumnWidthsReason, Error, ErrorKind, Errors, GeometryKind,
    Location, Region, Result,
};
pub use geom::{
    Color, Finite, Ink, IntoFinite, IntoNonNegative, IntoPoint, IntoPositive, IntoRect, Length,
    MarginPreset, Margins, NonNegative, Orientation, PageSize, Paint, Paper, Path, PathBuilder,
    PathVerb, Pen, Point, Positive, Rect, Size, Stroke, A4, LETTER, MM_TO_PT,
};
pub use limits::{LimitKind, Limits};

pub mod canvas;
pub mod document;
pub mod fonts;
pub mod spec;

pub use document::{Document, Image, Numbering, Ratio, Rendered, Track};
pub use spec::{layout_spec, DocumentSpec};

#[cfg(feature = "test-utils")]
pub use canvas::{pt_near, PT_ABS_TOLERANCE};
pub use canvas::{
    Canvas, CanvasCommand, DecodedImage, GlyphPlacement, GlyphRun, ImageFormat, ImageId,
    LinkTarget, RecordedPage, RecordingCanvas,
};
pub use fonts::FontId;

pub(crate) mod text;

pub(crate) mod layout;

#[cfg(feature = "backend-krilla")]
pub(crate) mod backend;

/// Writes the pages of `rendered` as PDF bytes.
///
/// JPEG, GIF, and WebP pixel data that passed header checks while the document
/// was laid out can still fail here. That [`ErrorKind::Backend`] value has no
/// page. PNG pixel data fails during [`layout_spec`] instead.
#[cfg(feature = "backend-krilla")]
pub fn to_pdf(rendered: Rendered) -> Result<Vec<u8>> {
    backend::write_pdf(&rendered.recording, rendered.fonts, rendered.images).map_err(Errors::from)
}
