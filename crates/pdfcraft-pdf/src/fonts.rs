//! Font identifiers, the caller's font registry, and a crate-internal store for
//! skrifa measurement.
//!
//! A [`FontId`] on a recorded [`crate::canvas::GlyphRun`] is read-only. The
//! library assigns ids when it registers faces from a [`FontRegistry`]. Callers
//! do not build an id from a raw integer.
//!
//! ```compile_fail
//! let _id = pdfcraft_pdf::FontId::from_raw(0);
//! ```

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
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
pub(crate) struct LoadedFace {
    pub(crate) source: FontSource,
    pub(crate) metrics: FaceMetrics,
    pub(crate) advances: HashMap<u32, f32>,
}

/// One face in a font file. `index` selects a face inside a collection.
///
/// `bytes` are the font file. The library does not read a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct FontFace {
    /// Font file bytes.
    pub bytes: Vec<u8>,
    /// Face index inside `bytes`. Zero is the first face.
    pub index: u32,
}

/// A normal face and an optional bold face, both supplied by the caller.
///
/// There is no Latin slot and no fallback chain.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct FontFamily {
    /// Face used by [`crate::spec::InlineRun::Text`].
    pub normal: FontFace,
    /// Face used by [`crate::spec::InlineRun::Bold`]. `None` rejects bold runs.
    pub bold: Option<FontFace>,
}

/// Faces keyed by the caller's own type `K`.
///
/// `K` is `Copy + Eq + Hash + Debug`. A document font enum is the usual key.
/// [`Self::single`] covers a document that uses one family. Registering the
/// same key again replaces that family's faces. An empty registry is valid for
/// a document that draws no text.
#[derive(Debug, Clone)]
#[must_use]
pub struct FontRegistry<K> {
    entries: Vec<(K, FontFamily)>,
}

impl<K> FontRegistry<K> {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn into_entries(self) -> Vec<(K, FontFamily)> {
        self.entries
    }
}

impl<K> Default for FontRegistry<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K> FontRegistry<K>
where
    K: Copy + Eq + Hash + Debug,
{
    /// Inserts `family` under `key`. A repeated `key` replaces the previous family.
    pub fn register(&mut self, key: K, family: FontFamily) {
        if let Some((_, existing)) = self.entries.iter_mut().find(|(found, _)| *found == key) {
            *existing = family;
        } else {
            self.entries.push((key, family));
        }
    }
}

impl FontRegistry<()> {
    /// One family, addressed by the unit key `()`.
    pub fn single(family: FontFamily) -> Self {
        let mut fonts = Self::new();
        fonts.register((), family);
        fonts
    }
}

/// Key carried by a [`crate::spec::DocumentSpec`].
pub(crate) trait FontKey: Copy + Eq + Hash + Debug {}

impl<T> FontKey for T where T: Copy + Eq + Hash + Debug {}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedFont {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) skrifa_units_per_em: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct FontStore {
    faces: Vec<LoadedFace>,
    max_bytes: usize,
}

impl Default for FontStore {
    fn default() -> Self {
        Self {
            faces: Vec::new(),
            max_bytes: Limits::default().max_font_bytes(),
        }
    }
}

/// Ascent, descent, leading, and units per em, the same numbers [`FontStore::register`] stores.
pub(crate) fn face_metrics(font_ref: &FontRef<'_>, index: u32) -> Result<FaceMetrics> {
    let metrics = font_ref.metrics(Size::unscaled(), LocationRef::default());
    if metrics.units_per_em == 0 {
        return Err(Error::from(ErrorKind::Font {
            index,
            message: "units per em is 0".to_owned(),
        }));
    }
    Ok(FaceMetrics {
        units_per_em: f32::from(metrics.units_per_em),
        ascent: metrics.ascent,
        descent: metrics.descent,
        leading: metrics.leading,
    })
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
        let metrics = face_metrics(&font_ref, source.index)?;

        let id = FontId::from_raw(u32::try_from(self.faces.len()).map_err(|_| {
            Error::from(ErrorKind::Font {
                index: source.index,
                message: "too many registered fonts".to_owned(),
            })
        })?);
        self.faces.push(LoadedFace {
            source,
            metrics,
            advances: HashMap::new(),
        });
        Ok(id)
    }

    pub(crate) fn metrics(&self, id: FontId) -> Result<&FaceMetrics> {
        Ok(&self.face(id)?.metrics)
    }

    pub(crate) fn face(&self, id: FontId) -> Result<&LoadedFace> {
        self.faces
            .get(id.as_u32() as usize)
            .ok_or(Error::from(ErrorKind::UnknownFont { id: id.as_u32() }))
    }

    pub(crate) fn face_mut(&mut self, id: FontId) -> Result<&mut LoadedFace> {
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

/// Test face from `PDFCRAFT_PDF_FONT`, or `PDFCRAFT_CJK_FONT` if the former is unset.
///
/// Returns `None` when neither variable is set so font-dependent tests can skip.
#[cfg(test)]
pub(crate) fn fixture_bytes() -> Option<Arc<Vec<u8>>> {
    let path =
        std::env::var_os("PDFCRAFT_PDF_FONT").or_else(|| std::env::var_os("PDFCRAFT_CJK_FONT"))?;
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "read test font {}: {error}",
            std::path::Path::new(&path).display()
        )
    });
    Some(Arc::new(bytes))
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
        let Some(bytes) = fixture_bytes() else {
            return;
        };
        let mut store = FontStore::new();
        let id = store.register(FontSource { bytes, index: 0 }).unwrap();
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
    fn register_replaces_the_same_key_and_single_uses_unit() {
        let mut fonts = FontRegistry::new();
        fonts.register(
            "body",
            FontFamily {
                normal: FontFace {
                    bytes: b"one".to_vec(),
                    index: 0,
                },
                bold: None,
            },
        );
        fonts.register(
            "body",
            FontFamily {
                normal: FontFace {
                    bytes: b"two".to_vec(),
                    index: 3,
                },
                bold: None,
            },
        );
        let entries = fonts.into_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.normal.bytes, b"two");
        assert_eq!(entries[0].1.normal.index, 3);

        let single = FontRegistry::single(FontFamily {
            normal: FontFace {
                bytes: b"only".to_vec(),
                index: 1,
            },
            bold: None,
        });
        assert_eq!(single.into_entries()[0].0, ());
    }
}
