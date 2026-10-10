#![allow(clippy::wildcard_imports)]
use super::*;

/// Identifies one [`Document`].
///
/// Image ids are dense indexes. A handle moved into another document would paint
/// that document's image at the same index, so each document gets its own token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DocToken(u32);

impl DocToken {
    pub(crate) fn next() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}
