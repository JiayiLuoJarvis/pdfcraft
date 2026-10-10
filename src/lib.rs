//! Quire: PDF document layout on top of a sealed krilla backend.
//!
//! Public surface grows only after items in `docs/提案书.md` are confirmed.

#![forbid(unsafe_code)]

pub mod error;
pub mod geom;
pub mod limits;

pub use error::{
    CellIndex, CellSpanReason, ColumnWidthsReason, Error, ErrorKind, GeometryKind, Location,
    Region, Result,
};
pub use geom::{
    Color, Finite, Ink, IntoFinite, IntoNonNegative, IntoPoint, IntoPositive, IntoRect, Length,
    MarginPreset, Margins, NonNegative, Orientation, PageSize, Paint, Paper, Path, PathBuilder,
    PathVerb, Pen, Point, Positive, Rect, Size, Stroke, A4, LETTER, MM_TO_PT,
};

pub mod canvas;
pub mod document;
pub mod fonts;

pub use document::{
    document, Band, Blank, Border, Cell, Columns, Document, Flow, FlowWriter, HLine, Image,
    ImageBox, IntoFlowItem, Layer, List, Numbering, Page, Paragraph, PlacedImage, Rendered,
    Section, Shapes, Table, TextAt, TextProps, Track,
};

pub use canvas::{
    pt_near, Canvas, CanvasCommand, DecodedImage, DocumentInfo, GlyphPlacement, GlyphRun,
    ImageFormat, ImageId, LinkTarget, RecordedPage, RecordingCanvas, PT_ABS_TOLERANCE,
};
pub use fonts::FontId;

pub(crate) mod text;

pub(crate) mod layout;

#[cfg(feature = "backend-krilla")]
pub(crate) mod backend;
