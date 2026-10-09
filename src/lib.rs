//! Quire: PDF document layout on top of a sealed krilla backend.
//!
//! Public surface grows only after items in `docs/提案书.md` are confirmed.

#![forbid(unsafe_code)]

pub mod error;
pub mod geom;
pub mod limits;

pub use error::{Error, Result};
pub use geom::{
    Color, Finite, Length, Margins, NonNegative, Paint, Path, PathBuilder, PathVerb, Point,
    Positive, Rect, Size, Stroke, A4, LETTER, MM_TO_PT,
};

pub mod canvas;
pub mod document;
pub mod fonts;

pub use document::{
    document, Band, Cell, Columns, Document, Flow, Layer, List, Numbering, Page, Paragraph,
    Rendered, Table,
};

pub use canvas::{
    pt_near, Canvas, CanvasCommand, DecodedImage, DocumentInfo, GlyphPlacement, GlyphRun, ImageId,
    LinkTarget, RecordedPage, RecordingCanvas, PT_ABS_TOLERANCE,
};
pub use fonts::FontId;

pub mod text;

pub mod layout;

#[cfg(feature = "backend-krilla")]
pub(crate) mod backend;
