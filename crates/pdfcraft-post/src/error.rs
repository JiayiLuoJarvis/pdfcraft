use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

/// A post-processing failure.
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
    /// use pdfcraft_post::{Error, ErrorKind};
    ///
    /// let err = Error::new(ErrorKind::NotAPdf);
    /// assert_eq!(err.kind, ErrorKind::NotAPdf);
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

/// What went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The bytes are not a PDF this crate can open.
    NotAPdf,
    /// The input PDF is encrypted. This crate never unlocks input;
    /// re-run the processor on the unencrypted source instead.
    Encrypted,
    /// [`crate::Encrypt`] or [`crate::StampAndEncrypt`] was missing a config or owner password.
    ConfigMissing,
    /// Encryption failed while preparing or writing the PDF.
    EncryptFailed,
    /// Writing `/Info` failed.
    MetadataFailed,
    /// [`crate::merge_pdfs`] received an empty part list.
    MergeEmpty,
    /// Merging PDFs failed.
    MergeFailed,
    /// Splitting a PDF into per-page PDFs failed.
    SplitFailed,
    /// Rendering or encoding the first-page thumbnail failed.
    ///
    /// Without the `thumbnail` feature this is also returned for any input
    /// that would otherwise render: enable the feature to get JPEG bytes.
    ThumbnailFailed,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAPdf => write!(f, "not a PDF"),
            Self::Encrypted => write!(f, "encrypted pdf: password required"),
            Self::ConfigMissing => {
                write!(f, "encryption config or owner password is missing")
            }
            Self::EncryptFailed => write!(f, "encrypt failed"),
            Self::MetadataFailed => write!(f, "metadata failed"),
            Self::MergeEmpty => write!(f, "merge_pdfs requires at least one part"),
            Self::MergeFailed => write!(f, "merge_pdfs failed"),
            Self::SplitFailed => write!(f, "split_pdfs failed"),
            Self::ThumbnailFailed => write!(f, "thumbnail failed"),
        }
    }
}
