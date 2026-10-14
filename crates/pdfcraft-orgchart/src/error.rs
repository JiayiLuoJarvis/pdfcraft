//! Stable protocol codes and the error that carries one.

use std::fmt;

/// Why a chart request was rejected.
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
    /// Layout hit a broken internal invariant.
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

/// A chart-protocol failure.
///
/// Match on [`code`](Self::code). [`message`](Self::message) is detail for logs.
/// Its wording is not a stable contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// Stable protocol code.
    pub code: ErrorCode,
    /// Detail for logs. Wording can change between releases.
    pub message: String,
}

impl Error {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

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
