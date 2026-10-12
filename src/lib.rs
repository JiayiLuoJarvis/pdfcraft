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

pub use document::{
    Band, Blank, Border, Cell, Columns, Document, Flow, FlowWriter, HLine, Image, ImageBox,
    IntoCellItem, IntoFlowItem, Layer, List, ListBlock, Numbering, Page, Paragraph, PlacedImage,
    Rendered, Section, Shapes, Spacer, Table, TextAt, TextProps, Track,
};

#[cfg(feature = "test-utils")]
pub use canvas::{pt_near, PT_ABS_TOLERANCE};
pub use canvas::{
    Canvas, CanvasCommand, DecodedImage, DocumentInfo, GlyphPlacement, GlyphRun, ImageFormat,
    ImageId, LinkTarget, RecordedPage, RecordingCanvas,
};
pub use fonts::FontId;

pub(crate) mod text;

pub(crate) mod layout;

#[cfg(feature = "backend-krilla")]
pub(crate) mod backend;

/// Lays out a document whose default paper is `page_size`.
///
/// `build` runs once. Builder mistakes and missing glyphs are collected, up to
/// 64, and returned together. Any of them fails the whole document before a
/// page is drawn. An `Err` returned from `build` is included and stops layout.
///
/// Text uses the first font passed to [`Document::font`] at 12 pt with line
/// height 1.2, black, start-aligned. Margins default to zero.
pub fn document<'a>(
    page_size: PageSize,
    build: impl FnOnce(&mut Document<'a>) -> Result<()>,
) -> Result<Rendered> {
    let mut doc = Document::new(page_size);
    #[cfg(feature = "backend-krilla")]
    {
        doc.images = Box::new(backend::ImageTable::default());
    }
    let returned = build(&mut doc);
    doc.render(returned)
}

/// Writes the pages of `rendered` as PDF bytes.
#[cfg(feature = "backend-krilla")]
pub fn to_pdf(rendered: Rendered) -> Result<Vec<u8>> {
    backend::write_pdf(&rendered.recording, rendered.fonts, rendered.images).map_err(Errors::from)
}
