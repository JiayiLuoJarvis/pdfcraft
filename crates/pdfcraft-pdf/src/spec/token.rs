//! Spec-local resource identity.

use std::sync::Arc;

/// Identifies one [`super::Resources`] table.
///
/// Image indexes are dense. A handle from another resources table must not resolve.
#[derive(Debug, Clone)]
pub(crate) struct DocToken(Arc<()>);

impl DocToken {
    pub(crate) fn new() -> Self {
        Self(Arc::new(()))
    }

    pub(crate) fn is_same_resources(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl PartialEq for DocToken {
    fn eq(&self, other: &Self) -> bool {
        self.is_same_resources(other)
    }
}

impl Eq for DocToken {}

impl Default for DocToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tokens_are_distinct() {
        let a = DocToken::new();
        let b = DocToken::new();
        assert_ne!(a, b);
        assert!(!a.is_same_resources(&b));
    }

    #[test]
    fn cloned_tokens_are_identical() {
        let a = DocToken::new();
        let b = a.clone();
        assert_eq!(a, b);
        assert!(a.is_same_resources(&b));
    }

    #[test]
    fn default_creates_distinct_token() {
        let a = DocToken::default();
        let b = DocToken::default();
        assert_ne!(a, b);
    }
}
