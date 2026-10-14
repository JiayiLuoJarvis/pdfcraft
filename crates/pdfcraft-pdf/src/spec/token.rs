//! Spec-local resource identity.

use std::sync::atomic::{AtomicU32, Ordering};

/// Identifies one [`super::Resources`] table.
///
/// Image indexes are dense. A handle from another resources table must not resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocToken(u32);

impl DocToken {
    pub(crate) fn next() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}
