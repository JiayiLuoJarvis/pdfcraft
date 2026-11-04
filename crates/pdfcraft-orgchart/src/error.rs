//! Stable protocol codes and the error that carries one.

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;

/// Why a chart request or a draw was rejected.
///
/// [`ErrorCode::as_str`] is the stable value. It matches Python `EquityErrorCode.value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// The request breaks a protocol rule, including a `top_n` fold rule.
    InvalidProtocol,
    /// A directed holding edge participates in a cycle.
    CycleDetected,
    /// `subject_id` is not a node id.
    SubjectNotFound,
    /// The node list is longer than the active cap.
    TooManyNodes,
    /// Font bytes could not be opened as one font.
    FontError,
    /// A character has no glyph in the font.
    MissingGlyph,
    /// A node box is taller than the portrait limit.
    NodeTooTall,
    /// Layout hit a broken internal invariant, or a page that cannot fit.
    LayoutFailed,
}

impl ErrorCode {
    /// Stable code string, matching Python `EquityErrorCode.value`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidProtocol => "INVALID_PROTOCOL",
            Self::CycleDetected => "CYCLE_DETECTED",
            Self::SubjectNotFound => "SUBJECT_NOT_FOUND",
            Self::TooManyNodes => "TOO_MANY_NODES",
            Self::FontError => "FONT_ERROR",
            Self::MissingGlyph => "MISSING_GLYPH",
            Self::NodeTooTall => "NODE_TOO_TALL",
            Self::LayoutFailed => "LAYOUT_FAILED",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A chart failure.
///
/// Match on [`code`](Self::code). [`message`](Self::message) is for logs.
/// Its wording is not a stable contract. [`subject`](Self::subject) names the
/// request value involved, when one exists. A parser or writer failure stays
/// on [`StdError::source`] and is not copied into the message.
///
/// There is no [`PartialEq`]. [`source`](StdError::source) cannot be compared.
#[derive(Debug, Clone)]
pub struct Error {
    code: ErrorCode,
    message: String,
    subject: Option<ErrorSubject>,
    // `Arc` keeps `Error: Clone`. `Box<dyn StdError>` is not `Clone`, and a
    // borrow would tie the error to the parser's lifetime.
    source: Option<Arc<dyn StdError + Send + Sync>>,
}

impl Error {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            subject: None,
            source: None,
        }
    }

    pub(crate) fn with_subject(mut self, subject: ErrorSubject) -> Self {
        self.subject = Some(subject);
        self
    }

    pub(crate) fn with_optional_subject(mut self, subject: Option<ErrorSubject>) -> Self {
        self.subject = subject;
        self
    }

    pub(crate) fn with_source(mut self, source: impl StdError + Send + Sync + 'static) -> Self {
        self.source = Some(Arc::new(source));
        self
    }

    /// Draw-path protocol failures are internal once [`crate::Chart::new`] has passed.
    pub(crate) fn into_layout_failed(mut self) -> Self {
        self.code = ErrorCode::LayoutFailed;
        self
    }

    /// Stable protocol code.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Detail for logs. Wording can change between releases.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The request value this failure is about, when the check names one.
    #[must_use]
    pub fn subject(&self) -> Option<&ErrorSubject> {
        self.subject.as_ref()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn StdError + 'static))
    }
}

/// Which request value a failure refers to.
///
/// Folded and overflow nodes are not listed. The message describes them.
/// A synthetic edge uses [`ErrorSubject::Node`] for its caller-authored end,
/// or `None` when both ends are synthetic.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorSubject {
    /// `index` is the position in [`ChartRequest::nodes`](crate::ChartRequest::nodes).
    Node {
        /// Position in the request node list.
        index: usize,
        /// Caller id, as stored.
        id: String,
    },
    /// `index` is the position in [`ChartRequest::edges`](crate::ChartRequest::edges).
    Edge {
        /// Position in the request edge list.
        index: usize,
        /// Holder id, as stored.
        holder: String,
        /// Held id, as stored.
        held: String,
    },
    /// A request field, such as `"subject_id"`, `"max_nodes"`, `"top_n"`, or `"style.margin_mm"`.
    Field(&'static str),
    /// A character with no glyph in the font.
    Character(char),
}

#[cfg(test)]
mod tests {
    use super::ErrorCode;

    #[test]
    fn code_strings_match_python_equity_error_code() {
        assert_eq!(ErrorCode::InvalidProtocol.as_str(), "INVALID_PROTOCOL");
        assert_eq!(ErrorCode::CycleDetected.as_str(), "CYCLE_DETECTED");
        assert_eq!(ErrorCode::SubjectNotFound.as_str(), "SUBJECT_NOT_FOUND");
        assert_eq!(ErrorCode::TooManyNodes.as_str(), "TOO_MANY_NODES");
        assert_eq!(ErrorCode::FontError.as_str(), "FONT_ERROR");
        assert_eq!(ErrorCode::MissingGlyph.as_str(), "MISSING_GLYPH");
        assert_eq!(ErrorCode::NodeTooTall.as_str(), "NODE_TOO_TALL");
        assert_eq!(ErrorCode::LayoutFailed.as_str(), "LAYOUT_FAILED");
        assert_eq!(ErrorCode::CycleDetected.to_string(), "CYCLE_DETECTED");
    }
}
