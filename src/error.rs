//! Failures from constructing lengths, geometry, canvas ops, and the PDF backend.

use std::fmt;

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
            Self::Backend(message) => write!(f, "PDF backend failed: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result of constructing a value or drawing to a canvas.
pub type Result<T> = std::result::Result<T, Error>;
