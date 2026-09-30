//! Quire: PDF document layout on top of a sealed krilla backend.
//!
//! Public surface grows only after items in `docs/提案书.md` are confirmed.
//! Geometry types and construction errors are exported from the crate root.

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

pub mod text;

pub mod layout;

#[cfg(feature = "backend-krilla")]
pub(crate) mod backend;
