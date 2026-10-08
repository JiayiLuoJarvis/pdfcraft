//! Failures from constructing lengths, geometry, canvas ops, fonts, and the PDF backend.

use std::fmt;

use crate::fonts::FontId;

/// A construction or drawing failure.
///
/// Length variants carry `value`, the argument of `pt` or `mm`, in the unit
/// the caller passed. When a finite millimeter converts to a non-finite point
/// value, or a positive millimeter underflows to a point value that is not
/// positive, `value` is that point value.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// `value` is NaN or infinite.
    NonFiniteLength {
        /// The rejected number.
        value: f32,
    },
    /// `value` is finite and below zero.
    NegativeLength {
        /// The rejected number.
        value: f32,
    },
    /// `value` is finite and not above zero.
    NonPositiveLength {
        /// The rejected number.
        value: f32,
    },
    /// A link URI was empty.
    EmptyLink,
    /// A `Canvas` call used an illegal page lifecycle state.
    InvalidCanvasState {
        /// The method that failed.
        operation: &'static str,
        /// The state that blocked it.
        state: &'static str,
    },
    /// A `Canvas` method this build does not support yet.
    UnsupportedCanvasOperation {
        /// The method name.
        operation: &'static str,
    },
    /// Geometry that brands allow but the backend cannot draw.
    InvalidGeometry,
    /// Font bytes could not be parsed or are unusable.
    Font {
        /// Collection index passed to the parser.
        index: u32,
        /// Parser or loader display text.
        message: String,
    },
    /// `scalar` has no glyph in `font`. The document is not produced.
    MissingGlyph {
        /// Font that was queried.
        font: FontId,
        /// Unmapped Unicode scalar.
        scalar: char,
    },
    /// `id` is not registered in the store this canvas uses.
    UnknownFont {
        /// Raw identifier that was not found.
        id: u32,
    },
    /// skrifa and krilla read different units per em from the same bytes.
    FontMetricsMismatch {
        /// Units per em from skrifa.
        skrifa_upem: f32,
        /// Units per em from krilla.
        krilla_upem: f32,
    },
    /// The PDF backend failed. The string is that failure's display text.
    Backend(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteLength { value } => write!(f, "{value} is not a finite length"),
            Self::NegativeLength { value } => write!(f, "{value} is a negative length"),
            Self::NonPositiveLength { value } => write!(f, "{value} is not a positive length"),
            Self::EmptyLink => f.write_str("link URI is empty"),
            Self::InvalidCanvasState { operation, state } => {
                write!(f, "{operation} is invalid while {state}")
            }
            Self::UnsupportedCanvasOperation { operation } => {
                write!(f, "{operation} is not supported yet")
            }
            Self::InvalidGeometry => f.write_str("geometry is invalid"),
            Self::Font { index, message } => write!(f, "font index {index}: {message}"),
            Self::MissingGlyph { font, scalar } => {
                write!(
                    f,
                    "font {} has no glyph for U+{:04X}",
                    font.as_u32(),
                    u32::from(*scalar)
                )
            }
            Self::UnknownFont { id } => write!(f, "font {id} is not registered"),
            Self::FontMetricsMismatch {
                skrifa_upem,
                krilla_upem,
            } => write!(
                f,
                "font units per em differ: skrifa {skrifa_upem}, krilla {krilla_upem}"
            ),
            Self::Backend(message) => write!(f, "PDF backend failed: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result of constructing a value or drawing to a canvas.
pub type Result<T> = std::result::Result<T, Error>;
