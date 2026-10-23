//! Failures from opening a PDF, reading `AcroForm` widgets, and drawing values
//! into them.
//!
//! # Shape
//!
//! - [`Error`] — one failure: [`Error::kind`] plus an optional dependency error
//!   behind [`std::error::Error::source`]
//! - [`ErrorKind`] — what went wrong; [`ArgumentReason`], [`FlattenReason`], and
//!   [`FontReason`] subdivide the three largest families
//! - [`Result<T>`](crate::Result) — `Result<T, Error>`
//!
//! # Reporting model
//!
//! Every public forms operation reads caller-supplied PDF or font bytes, so its
//! failures are recoverable: [`crate::list_fields`] and
//! [`crate::fill_and_flatten`] return `Result` with a specific [`ErrorKind`].
//! Parser and writer failures are not flattened into the message; they stay
//! reachable through [`std::error::Error::source`]. No public forms API returns
//! `Option` for a failure, and none panics on malformed input.

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
    // `Arc` shares the dependency failure across `Error` clones. `Error` is
    // `Clone` and `Box<dyn StdError>` is not; a borrow would tie every error
    // to the dependency's lifetime.
    source: Option<Arc<dyn StdError + Send + Sync>>,
}

impl Error {
    /// The explicit constructor paired with `impl From<ErrorKind> for Error`.
    ///
    /// Prefer this over `.into()` at call sites that name the type.
    ///
    /// # Example
    ///
    /// ```
    /// use pdfcraft_forms::{Error, ErrorKind};
    ///
    /// let err = Error::new(ErrorKind::NotAPdf);
    /// assert_eq!(err.to_string(), "not a PDF");
    /// ```
    #[must_use]
    pub fn new(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }

    pub(crate) fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Arc::new(source));
        self
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self::new(kind)
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

/// Why [`ErrorKind::InvalidArgument`] rejected the call.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ArgumentReason {
    /// `font_size` 不是有限正数、毫点不是有限值或放不进 `i64`，或大于 256 pt。
    ///
    /// `Display` 不区分这几种。句子故意仍是 `font_size must be finite and greater than 0`。
    FontSize {
        /// Rejected `font_size`, in points.
        value: f32,
    },
    /// 文本 widget 收到了 `FieldValue::CheckBox`。
    CheckboxOnText,
    /// 列表或组合框收到了 `FieldValue::CheckBox`。
    CheckboxOnChoice,
    /// A coordinate or offset is not finite.
    NonFinite {
        /// Rejected number.
        value: f32,
    },
}

/// Why [`ErrorKind::FlattenFailed`] could not finish the PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FlattenReason {
    /// The stamp names a page index this document does not have.
    PageOutOfRange {
        /// Index into the document page list.
        page_index: usize,
    },
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
    /// 内容流数字不是有限值。建议表里没有这一项；`pdf_num` 不允许再写成 `0`，所以必须有。
    NonFinite,
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
    /// A size or a value is not usable.
    InvalidArgument {
        /// Widget name when the failure is about one field.
        ///
        /// `None` when `reason` is [`ArgumentReason::FontSize`].
        field_name: Option<String>,
        /// Why the argument was rejected.
        reason: ArgumentReason,
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
    /// Text or a choice needs a font, and the font is missing or unusable.
    ///
    /// [`FontReason::Unreadable`] keeps the parser error on [`Error::source`].
    /// The other reasons have no source.
    FontNotConfigured {
        /// Why the font cannot be used.
        reason: FontReason,
    },
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
        /// Which step failed.
        reason: FlattenReason,
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
            Self::InvalidArgument { field_name, reason } => match reason {
                ArgumentReason::FontSize { .. } => {
                    write!(f, "font_size must be finite and greater than 0")
                }
                ArgumentReason::CheckboxOnText | ArgumentReason::CheckboxOnChoice => {
                    let name = field_name.as_deref().unwrap_or("field");
                    write!(f, "{name} does not take a checkbox value")
                }
                ArgumentReason::NonFinite { .. } => write!(f, "value must be finite"),
            },
            Self::FlattenFailed { reason } => match reason {
                FlattenReason::PageOutOfRange { page_index } => {
                    write!(f, "page {page_index} is outside the document")
                }
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
            Self::MissingGlyph { field_name, scalar } => {
                write!(f, "missing glyph {scalar:?} in {field_name}")
            }
            Self::BoxOverflow { field_name } => write!(f, "text does not fit in {field_name}"),
            Self::FontNotConfigured { reason } => write!(f, "{reason}"),
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

/// Why [`ErrorKind::FontNotConfigured`] rejected a font.
///
/// [`Self::Unreadable`] keeps the parser error on [`Error::source`].
/// The other variants have no source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontReason {
    /// Text is drawn and [`crate::FillOptions::font`] is missing, or a later check found no face.
    Missing,
    /// A `ttcf` header or face offset could not be read.
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
            Self::Head => "font head table could not be patched",
            Self::Width => "font width is not a finite i32",
            Self::Unreadable => "font face could not be parsed",
        })
    }
}

pub(crate) fn font_unusable(reason: FontReason) -> Error {
    ErrorKind::FontNotConfigured { reason }.into()
}
