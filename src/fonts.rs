//! Font identifiers shared by measurement and drawing.

/// Opaque font identifier stored on a [`crate::canvas::GlyphRun`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct FontId(pub(crate) u32);

impl FontId {
    /// Wraps a raw identifier allocated for a font.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw identifier.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}
