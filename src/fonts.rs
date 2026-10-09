//! Font identifiers and a crate-internal store for skrifa measurement.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::error::{Error, Result};

/// Opaque font identifier stored on a [`crate::canvas::GlyphRun`].
///
/// A store numbers faces from 0 in registration order. The first face is 0.
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

pub(crate) const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct FontSource {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FaceMetrics {
    pub(crate) units_per_em: f32,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) leading: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct FontFace {
    pub(crate) source: FontSource,
    pub(crate) metrics: FaceMetrics,
    pub(crate) advances: HashMap<u32, f32>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedFont {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) skrifa_units_per_em: f32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FontStore {
    faces: Vec<FontFace>,
    bold: Option<FontId>,
}

impl FontStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Parses `source` and appends the face.
    ///
    /// The returned id is the face's index in this store. The first face is 0.
    pub(crate) fn register(&mut self, source: FontSource) -> Result<FontId> {
        if source.bytes.is_empty() {
            return Err(Error::Font {
                index: source.index,
                message: "font bytes are empty".to_owned(),
            });
        }
        if source.bytes.len() > MAX_FONT_BYTES {
            return Err(Error::Font {
                index: source.index,
                message: format!(
                    "font is {} bytes; max is {MAX_FONT_BYTES}",
                    source.bytes.len()
                ),
            });
        }

        let font_ref =
            FontRef::from_index(source.bytes.as_slice(), source.index).map_err(|error| {
                Error::Font {
                    index: source.index,
                    message: error.to_string(),
                }
            })?;
        let metrics = font_ref.metrics(Size::unscaled(), LocationRef::default());
        if metrics.units_per_em == 0 {
            return Err(Error::Font {
                index: source.index,
                message: "units per em is 0".to_owned(),
            });
        }

        let id = FontId::from_raw(u32::try_from(self.faces.len()).map_err(|_| Error::Font {
            index: source.index,
            message: "too many registered fonts".to_owned(),
        })?);
        self.faces.push(FontFace {
            source,
            metrics: FaceMetrics {
                units_per_em: f32::from(metrics.units_per_em),
                ascent: metrics.ascent,
                descent: metrics.descent,
                leading: metrics.leading,
            },
            advances: HashMap::new(),
        });
        Ok(id)
    }

    pub(crate) fn metrics(&self, id: FontId) -> Result<&FaceMetrics> {
        Ok(&self.face(id)?.metrics)
    }

    pub(crate) fn set_bold(&mut self, id: FontId) {
        self.bold = Some(id);
    }

    pub(crate) fn bold(&self) -> Result<FontId> {
        self.bold.ok_or(Error::UnknownFont { id: 1 })
    }

    pub(crate) fn face(&self, id: FontId) -> Result<&FontFace> {
        self.faces
            .get(id.as_u32() as usize)
            .ok_or(Error::UnknownFont { id: id.as_u32() })
    }

    pub(crate) fn face_mut(&mut self, id: FontId) -> Result<&mut FontFace> {
        self.faces
            .get_mut(id.as_u32() as usize)
            .ok_or(Error::UnknownFont { id: id.as_u32() })
    }

    pub(crate) fn resolve(&self, id: FontId) -> Result<ResolvedFont> {
        let face = self.face(id)?;
        Ok(ResolvedFont {
            bytes: Arc::clone(&face.source.bytes),
            index: face.source.index,
            skrifa_units_per_em: face.metrics.units_per_em,
        })
    }
}

#[cfg(test)]
pub(crate) fn fixture_bytes() -> Arc<Vec<u8>> {
    Arc::new(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/quire-measure.ttf"
        ))
        .to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bytes_are_font_error() {
        let mut store = FontStore::new();
        let err = store
            .register(FontSource {
                bytes: Arc::new(Vec::new()),
                index: 0,
            })
            .unwrap_err();
        assert!(matches!(err, Error::Font { index: 0, .. }));
    }

    #[test]
    fn garbage_bytes_are_font_error() {
        let mut store = FontStore::new();
        let err = store
            .register(FontSource {
                bytes: Arc::new(b"not a font".to_vec()),
                index: 0,
            })
            .unwrap_err();
        assert!(matches!(err, Error::Font { index: 0, .. }));
    }

    #[test]
    fn fixture_descent_is_negative() {
        let mut store = FontStore::new();
        let id = store
            .register(FontSource {
                bytes: fixture_bytes(),
                index: 0,
            })
            .unwrap();
        assert_eq!(id.as_u32(), 0);
        let metrics = store.metrics(id).unwrap();
        assert!(metrics.descent < 0.0);
        assert!(metrics.units_per_em > 0.0);
    }

    #[test]
    fn oversize_bytes_are_font_error() {
        let mut store = FontStore::new();
        let err = store
            .register(FontSource {
                bytes: Arc::new(vec![0; MAX_FONT_BYTES + 1]),
                index: 3,
            })
            .unwrap_err();
        assert!(matches!(err, Error::Font { index: 3, .. }));
    }

    #[test]
    fn unknown_id_is_error() {
        let store = FontStore::new();
        let err = store.metrics(FontId::from_raw(9)).unwrap_err();
        assert!(matches!(err, Error::UnknownFont { id: 9 }));
    }

    #[test]
    fn bold_slot_rebinds_and_empty_slot_uses_conventional_id() {
        let mut store = FontStore::new();
        assert_eq!(store.bold().unwrap_err(), Error::UnknownFont { id: 1 });
        store.set_bold(FontId::from_raw(2));
        assert_eq!(store.bold().unwrap(), FontId::from_raw(2));
        store.set_bold(FontId::from_raw(4));
        assert_eq!(store.bold().unwrap(), FontId::from_raw(4));
    }
}
