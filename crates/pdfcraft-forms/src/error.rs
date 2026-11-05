//! Failures from opening a PDF, reading widgets, and drawing values into them.
//!
//! The kind names the failure. The widget name, page index, and missing
//! character live on [`Error`] and are read with [`Error::field`],
//! [`Error::page`], and [`Error::scalar`].

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

/// A forms failure.
///
/// Compare [`kind`](Self::kind). This type does not implement [`PartialEq`].
/// A parser or writer error stays on [`source`](StdError::source) and is not
/// copied into [`Display`](fmt::Display).
#[derive(Debug, Clone)]
pub struct Error {
    kind: ErrorKind,
    field: Option<String>,
    page: Option<usize>,
    scalar: Option<char>,
    source: Option<Arc<dyn StdError + Send + Sync>>,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            field: None,
            page: None,
            scalar: None,
            source: None,
        }
    }

    pub(crate) fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub(crate) fn with_page(mut self, page: usize) -> Self {
        self.page = Some(page);
        self
    }

    pub(crate) fn with_scalar(mut self, scalar: char) -> Self {
        self.scalar = Some(scalar);
        self
    }

    pub(crate) fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Arc::new(source));
        self
    }

    /// What failed, without the widget name, page, or character.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Widget name for this failure, when the failure names one.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// Page index when a line names a page this document does not have.
    #[must_use]
    pub fn page(&self) -> Option<usize> {
        self.page
    }

    /// Character missing from the font. Only [`ErrorKind::MissingGlyph`] sets this.
    #[must_use]
    pub fn scalar(&self) -> Option<char> {
        self.scalar
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(field) = &self.field {
            write!(f, " ({field})")?;
        }
        if let Some(page) = self.page {
            write!(f, " page {page}")?;
        }
        if let Some(scalar) = self.scalar {
            write!(f, " {scalar:?}")?;
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn StdError + 'static))
    }
}

/// Why [`ErrorKind::InvalidArgument`] rejected the call.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ArgumentReason {
    /// `font_size` is not a finite positive number, its millipoint form does not
    /// fit in `i64`, or it is greater than 256 pt.
    FontSize {
        /// Rejected `font_size`, in points.
        value: f32,
    },
    /// A text widget was given [`crate::FieldValue::CheckBox`].
    CheckboxOnText,
    /// A list or combo box was given [`crate::FieldValue::CheckBox`].
    CheckboxOnChoice,
    /// A coordinate, width, color channel, or offset is not finite.
    NonFinite {
        /// Rejected number.
        value: f32,
    },
    /// A line's `page_index` is not a page in the document.
    PageOutOfRange,
}

/// Why [`ErrorKind::FlattenFailed`] could not finish the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FlattenReason {
    /// The page content stream could not be extended.
    AppendContent,
    /// The page `Resources` entry could not be replaced.
    UpdateResources,
    /// A page `Resources` value was present and was not a dictionary.
    ResourcesNotDictionary,
    /// A page dictionary could not be read.
    ReadPage,
    /// A page `Annots` array could not be written back.
    UpdateAnnotations,
    /// `QuireForms` through `QuireForms64` are already used on the page.
    FontNameTaken,
    /// `Document::save_to` failed.
    Write,
    /// A PDF object nests deeper than this crate will walk.
    ///
    /// `depth` is the rejected level. The object where a walk starts is depth 0.
    NestingTooDeep {
        /// Rejected nesting depth.
        depth: usize,
    },
    /// A content-stream number is not finite.
    NonFinite,
}

/// What failed.
///
/// The widget name, page index, and missing character are not stored here.
/// Read them from [`Error`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The bytes are not a PDF this crate can open.
    NotAPdf,
    /// The PDF is encrypted.
    Encrypted,
    /// The catalog `AcroForm` dictionary has an `/XFA` key.
    HasXfa,
    /// A request names a widget that is not in the PDF.
    UnknownField,
    /// A choice string is not an export value or a display value in `/Opt`.
    IllegalChoice,
    /// A checkbox widget was given a value other than [`crate::FieldValue::CheckBox`].
    InvalidCheckbox,
    /// A size or a value is not usable.
    InvalidArgument(ArgumentReason),
    /// A character has no glyph in the supplied font.
    MissingGlyph,
    /// The text still exceeds the widget rectangle at the minimum font size.
    BoxOverflow,
    /// Text needs a font, and the font is missing or unusable.
    FontNotConfigured(FontReason),
    /// The request names a signature widget.
    SignatureNotWritable,
    /// The request names a radio button or a push button.
    WidgetUnsupported,
    /// Overlay merge or PDF write failed.
    FlattenFailed(FlattenReason),
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAPdf => write!(f, "not a PDF"),
            Self::Encrypted => write!(f, "encrypted PDF"),
            Self::HasXfa => write!(f, "AcroForm contains XFA"),
            Self::UnknownField => write!(f, "unknown field"),
            Self::IllegalChoice => write!(f, "illegal choice"),
            Self::InvalidCheckbox => write!(f, "invalid checkbox value"),
            Self::InvalidArgument(reason) => match reason {
                ArgumentReason::FontSize { .. } => {
                    write!(f, "font_size must be finite and greater than 0")
                }
                ArgumentReason::CheckboxOnText | ArgumentReason::CheckboxOnChoice => {
                    write!(f, "widget does not take a checkbox value")
                }
                ArgumentReason::NonFinite { .. } => write!(f, "value must be finite"),
                ArgumentReason::PageOutOfRange => write!(f, "page is outside the document"),
            },
            Self::FlattenFailed(reason) => match reason {
                FlattenReason::AppendContent => write!(f, "could not append page content"),
                FlattenReason::UpdateResources => write!(f, "could not update page resources"),
                FlattenReason::ResourcesNotDictionary => {
                    write!(f, "page resources are not a dictionary")
                }
                FlattenReason::ReadPage => write!(f, "could not read a page"),
                FlattenReason::UpdateAnnotations => {
                    write!(f, "could not update page annotations")
                }
                FlattenReason::FontNameTaken => write!(f, "page font resource name is taken"),
                FlattenReason::Write => write!(f, "could not write PDF"),
                FlattenReason::NestingTooDeep { depth } => {
                    write!(f, "PDF object nesting reached depth {depth}")
                }
                FlattenReason::NonFinite => write!(f, "non-finite number in page content"),
            },
            Self::MissingGlyph => write!(f, "missing glyph"),
            Self::BoxOverflow => write!(f, "text does not fit"),
            Self::FontNotConfigured(reason) => write!(f, "{reason}"),
            Self::SignatureNotWritable => write!(f, "signature field cannot be filled"),
            Self::WidgetUnsupported => write!(f, "field cannot be filled"),
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// Why [`ErrorKind::FontNotConfigured`] rejected a font.
///
/// [`Self::Unreadable`] keeps the parser error on [`Error::source`].
/// The other variants have no source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontReason {
    /// Text is drawn and no font was passed.
    Missing,
    /// A single face was given a collection, or a collection was given a single face.
    Collection,
    /// The face has no `glyf` table.
    NoGlyf,
    /// `units_per_em` is 0.
    UpemZero,
    /// The glyph has no advance, or its id does not fit in `u16`.
    NoAdvance,
    /// The `head` table is missing, too short, or its checksum could not be written.
    Head,
    /// A width in em-1000 units is non-finite or outside `i32`.
    Width,
    /// The face bytes could not be parsed.
    Unreadable,
}

impl fmt::Display for FontReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Missing => "font is missing",
            Self::Collection => "font collection face could not be read",
            Self::NoGlyf => "font has no glyf table",
            Self::UpemZero => "units per em is 0",
            Self::NoAdvance => "glyph has no advance",
            Self::Head => "font head table could not be read",
            Self::Width => "font width is not a finite i32",
            Self::Unreadable => "font face could not be parsed",
        })
    }
}

pub(crate) fn font_unusable(reason: FontReason) -> Error {
    Error::new(ErrorKind::FontNotConfigured(reason))
}

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;
    use std::io;

    use super::{Error, ErrorKind};

    #[test]
    fn display_omits_source_text() {
        let err =
            Error::new(ErrorKind::NotAPdf).with_source(io::Error::other("unique-source-token"));
        let shown = err.to_string();
        let source = StdError::source(&err).unwrap().to_string();
        assert!(source.contains("unique-source-token"));
        assert!(!shown.contains("unique-source-token"));
    }
}
