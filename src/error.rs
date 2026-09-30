//! Failures from constructing lengths and geometry.

use std::fmt;

/// A raw number failed a length constraint.
///
/// `value` is the argument of `pt` or `mm`, in the unit the caller passed.
/// When a finite millimeter converts to a non-finite point value, or a
/// positive millimeter underflows to a point value that is not positive,
/// `value` is that point value.
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteLength { value } => write!(f, "{value} is not a finite length"),
            Self::NegativeLength { value } => write!(f, "{value} is a negative length"),
            Self::NonPositiveLength { value } => write!(f, "{value} is not a positive length"),
        }
    }
}

impl std::error::Error for Error {}

/// Result of constructing a length or an aggregate from raw numbers.
pub type Result<T> = std::result::Result<T, Error>;
