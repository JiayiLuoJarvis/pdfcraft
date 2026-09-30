//! Crate-wide error type.
//!
//! Implementations land with the first geometry / limits PR. Do not invent
//! public variants ahead of confirmed proposal items.

/// Placeholder until the error surface is designed against `docs/提案书.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Temporary stub so the crate compiles before real variants exist.
    #[doc(hidden)]
    Unimplemented(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unimplemented(msg) => write!(f, "unimplemented: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;
