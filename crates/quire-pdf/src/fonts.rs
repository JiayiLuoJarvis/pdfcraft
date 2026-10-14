//! Font identifiers and a crate-internal store for skrifa measurement.
//!
//! A [`FontId`] on a recorded [`crate::canvas::GlyphRun`] is read-only. The
//! library assigns ids. Callers do not build an id from a raw integer.
//!
//! ```compile_fail
//! let _id = quire_pdf::FontId::from_raw(0);
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::error::{Error, ErrorKind, Fallible as Result};
use crate::limits::{LimitKind, Limits};

/// Opaque font identifier stored on a [`crate::canvas::GlyphRun`].
///
/// The library assigns ids in registration order. The first face is 0. Read an
/// id with [`Self::as_u32`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct FontId(pub(crate) u32);

impl FontId {
    /// Wraps a raw identifier allocated for a font.
    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw identifier.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

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

#[derive(Debug, Clone)]
pub(crate) struct FontStore {
    faces: Vec<FontFace>,
    bold: Option<FontId>,
    max_bytes: usize,
}

impl Default for FontStore {
    fn default() -> Self {
        Self {
            faces: Vec::new(),
            bold: None,
            max_bytes: Limits::default().max_font_bytes(),
        }
    }
}

/// Parses face `index` of `bytes`. skrifa's error stays reachable as the source.
pub(crate) fn parse_face(bytes: &[u8], index: u32) -> Result<FontRef<'_>> {
    FontRef::from_index(bytes, index).map_err(|error| {
        Error::from(ErrorKind::Font {
            index,
            message: "cannot parse font data".to_owned(),
        })
        .with_source(error)
    })
}

impl FontStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_max_bytes(&mut self, max_bytes: usize) {
        self.max_bytes = max_bytes;
    }

    /// Parses `source` and appends the face.
    ///
    /// The returned id is the face's index in this store. The first face is 0.
    pub(crate) fn register(&mut self, source: FontSource) -> Result<FontId> {
        if source.bytes.is_empty() {
            return Err(Error::from(ErrorKind::Font {
                index: source.index,
                message: "font bytes are empty".to_owned(),
            }));
        }
        if source.bytes.len() > self.max_bytes {
            return Err(Error::from(ErrorKind::LimitExceeded {
                kind: LimitKind::FontBytes,
                value: source.bytes.len() as u64,
                max: self.max_bytes as u64,
            }));
        }

        let font_ref = parse_face(source.bytes.as_slice(), source.index)?;
        let metrics = font_ref.metrics(Size::unscaled(), LocationRef::default());
        if metrics.units_per_em == 0 {
            return Err(Error::from(ErrorKind::Font {
                index: source.index,
                message: "units per em is 0".to_owned(),
            }));
        }

        let id = FontId::from_raw(u32::try_from(self.faces.len()).map_err(|_| {
            Error::from(ErrorKind::Font {
                index: source.index,
                message: "too many registered fonts".to_owned(),
            })
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
        self.bold.ok_or(ErrorKind::BoldFaceRequired.into())
    }

    pub(crate) fn face(&self, id: FontId) -> Result<&FontFace> {
        self.faces
            .get(id.as_u32() as usize)
            .ok_or(Error::from(ErrorKind::UnknownFont { id: id.as_u32() }))
    }

    pub(crate) fn face_mut(&mut self, id: FontId) -> Result<&mut FontFace> {
        self.faces
            .get_mut(id.as_u32() as usize)
            .ok_or(Error::from(ErrorKind::UnknownFont { id: id.as_u32() }))
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
        assert!(matches!(err.kind, ErrorKind::Font { index: 0, .. }));
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
        assert!(matches!(err.kind, ErrorKind::Font { index: 0, .. }));
        assert!(std::error::Error::source(&err).is_some());
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
    fn oversize_bytes_are_a_font_byte_limit() {
        const MAX: usize = 32 * 1024 * 1024;
        let mut store = FontStore::new();
        let err = store
            .register(FontSource {
                bytes: Arc::new(vec![0; MAX + 1]),
                index: 3,
            })
            .unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::LimitExceeded {
                kind: LimitKind::FontBytes,
                value: (MAX + 1) as u64,
                max: MAX as u64,
            }
        );
        assert!(std::error::Error::source(&err).is_none());
    }

    #[test]
    fn unknown_id_is_error() {
        let store = FontStore::new();
        let err = store.metrics(FontId::from_raw(9)).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnknownFont { id: 9 }));
    }

    #[test]
    fn bold_slot_rebinds_and_empty_slot_requires_bold_face() {
        let mut store = FontStore::new();
        assert_eq!(
            store.bold().unwrap_err(),
            ErrorKind::BoldFaceRequired.into()
        );
        store.set_bold(FontId::from_raw(2));
        assert_eq!(store.bold().unwrap(), FontId::from_raw(2));
        store.set_bold(FontId::from_raw(4));
        assert_eq!(store.bold().unwrap(), FontId::from_raw(4));
    }
}
