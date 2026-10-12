//! Failures from reading a PDF, checking a field, or writing the flattened file.
//!
//! [`Error`] carries one [`ErrorKind`] and an optional dependency [`source`](std::error::Error::source).
//! Equality compares `kind` only.

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

/// A forms failure.
///
/// Equality compares [`kind`](Self::kind) only. The dependency error behind
/// [`source`](StdError::source) is not compared.
#[derive(Debug, Clone)]
pub struct Error {
    /// What went wrong.
    pub kind: ErrorKind,
    source: Option<Arc<dyn StdError + Send + Sync>>,
}

impl Error {
    pub(crate) fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Arc::new(source));
        self
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn StdError + 'static))
    }
}

/// What went wrong.
///
/// Field variants carry the widget name from `/T`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The bytes are not a PDF this crate can open.
    NotAPdf,
    /// The PDF is encrypted.
    Encrypted,
    /// The catalog `AcroForm` dictionary has an `/XFA` key.
    HasXfa,
    /// `fields` names a widget that is not in the PDF.
    UnknownField {
        /// Widget name from the caller.
        field_name: String,
    },
    /// A choice string is not an export value or a display value in `/Opt`.
    IllegalChoice {
        /// Widget name.
        field_name: String,
    },
    /// A checkbox widget was given a value other than [`crate::FieldValue::CheckBox`].
    InvalidCheckbox {
        /// Widget name.
        field_name: String,
    },
    /// A size or value is not usable.
    InvalidArgument {
        /// What was rejected.
        message: String,
    },
    /// `scalar` has no glyph in the supplied font.
    MissingGlyph {
        /// Widget name.
        field_name: String,
        /// The character that is missing.
        scalar: char,
    },
    /// The text still exceeds the widget rectangle at the minimum font size.
    BoxOverflow {
        /// Widget name.
        field_name: String,
    },
    /// Text or a choice needs a font, and no usable font bytes were supplied.
    FontNotConfigured,
    /// `fields` names a signature widget.
    SignatureNotWritable {
        /// Widget name.
        field_name: String,
    },
    /// `fields` names a radio button or a push button.
    WidgetUnsupported {
        /// Widget name.
        field_name: String,
    },
    /// Overlay merge or PDF write failed.
    FlattenFailed {
        /// What the writer could not finish.
        message: String,
    },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAPdf => write!(f, "not a PDF"),
            Self::Encrypted => write!(f, "encrypted PDF"),
            Self::HasXfa => write!(f, "AcroForm contains XFA"),
            Self::UnknownField { field_name } => write!(f, "unknown field {field_name}"),
            Self::IllegalChoice { field_name } => write!(f, "illegal choice for {field_name}"),
            Self::InvalidCheckbox { field_name } => {
                write!(f, "invalid checkbox value for {field_name}")
            }
            Self::InvalidArgument { message } | Self::FlattenFailed { message } => {
                write!(f, "{message}")
            }
            Self::MissingGlyph { field_name, scalar } => {
                write!(f, "missing glyph {scalar:?} in {field_name}")
            }
            Self::BoxOverflow { field_name } => write!(f, "text does not fit in {field_name}"),
            Self::FontNotConfigured => write!(f, "font is missing or unusable"),
            Self::SignatureNotWritable { field_name } => {
                write!(f, "signature field {field_name} cannot be filled")
            }
            Self::WidgetUnsupported { field_name } => {
                write!(f, "field {field_name} cannot be filled")
            }
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
