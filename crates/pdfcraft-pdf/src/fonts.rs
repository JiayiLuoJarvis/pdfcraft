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
use skrifa::FontRef;

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
    // One file buffer shared by registry faces, stored faces, and resolved
    // marks. Owners have independent lifetimes; copying per owner would
    // duplicate multi-MB font data.
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
    /// Shared font file bytes.
    ///
    /// Faces cut from one file (regular, bold, italic, bold italic) hold
    /// clones instead of copies. A borrow would thread lifetimes through the
    /// public registry API for no runtime gain in this single-threaded pipeline.
    pub bytes: Arc<Vec<u8>>,
    /// Face index inside `bytes`. Zero is the first face.
    pub index: u32,
}

impl FontFace {
    /// Wrap owned bytes without parsing them.
    pub fn new(bytes: impl Into<Vec<u8>>, index: u32) -> Self {
        Self {
            bytes: Arc::new(bytes.into()),
            index,
        }
    }

    /// Reuse an existing shared font buffer without copying its contents.
    pub fn from_shared(bytes: Arc<Vec<u8>>, index: u32) -> Self {
        Self { bytes, index }
    }
}

/// One regular face plus optional bold, italic, and bold italic faces.
///
/// A run picks one face from its resolved [`crate::spec::RunStyle`] bold and italic flags.
/// Underline and strikethrough positions come from `regular`. The stroke is 1 pt.
/// There is no Latin slot. [`crate::spec::InlineRun::split_latin`] assigns keys.
/// There is no fallback chain and no synthetic bold or slant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct FontFamily {
    /// Face for text that is neither bold nor italic.
    pub regular: FontFace,
    /// Bold face. `None` fails bold runs with [`crate::ErrorKind::BoldFaceRequired`].
    pub bold: Option<FontFace>,
    /// Italic face. `None` fails italic runs with [`crate::ErrorKind::ItalicFaceRequired`].
    pub italic: Option<FontFace>,
    /// Bold italic face. `None` fails bold italic runs with
    /// [`crate::ErrorKind::BoldItalicFaceRequired`].
    pub bold_italic: Option<FontFace>,
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
    ///
    /// Faces are parsed when [`crate::layout_spec`] runs, not here. An unusable
    /// face is reported there as [`ErrorKind::Font`] or a
    /// [`LimitKind::FontBytes`] cap, with the parser error on
    /// [`std::error::Error::source`].
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
    // Clone of the stored face buffer, owned by one mark batch. Keeps the
    // page plan lifetime-free after the canvas drops its store handle.
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
    use skrifa::MetadataProvider;
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

/// Baseline to a decoration's top edge, in em, y up. Always finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EmOffset(f32);

impl EmOffset {
    /// `None` when `em` is not finite.
    pub(crate) fn new(em: f32) -> Option<Self> {
        em.is_finite().then_some(Self(em))
    }

    /// The offset in em, y up.
    pub(crate) fn em(self) -> f32 {
        self.0
    }
}

/// Underline from `post` and strikethrough from `OS/2`. `None` when the table
/// is missing or the offset is not finite. The font's thickness is not stored.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct FaceDecorations {
    pub(crate) underline: Option<EmOffset>,
    pub(crate) strikethrough: Option<EmOffset>,
}

fn em_offset(offset: Option<f32>, units_per_em: f32) -> Option<EmOffset> {
    EmOffset::new(offset? / units_per_em)
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

    pub(crate) fn decorations(&self, id: FontId) -> Result<FaceDecorations> {
        use skrifa::MetadataProvider;

        let face = self.face(id)?;
        let font_ref = parse_face(face.source.bytes.as_slice(), face.source.index)?;
        let metrics = font_ref.metrics(Size::unscaled(), LocationRef::default());
        let units_per_em = face.metrics.units_per_em;
        Ok(FaceDecorations {
            underline: em_offset(metrics.underline.map(|line| line.offset), units_per_em),
            strikethrough: em_offset(metrics.strikeout.map(|line| line.offset), units_per_em),
        })
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
    use skrifa::metrics::Decoration;
    use skrifa::MetadataProvider;

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
    fn fixture_decorations_are_em_scaled_post_and_os2_values() {
        let Some(bytes) = fixture_bytes() else {
            return;
        };
        let font_ref = FontRef::from_index(bytes.as_slice(), 0).unwrap();
        let raw = font_ref.metrics(Size::unscaled(), LocationRef::default());
        let upem = f32::from(raw.units_per_em);
        let mut store = FontStore::new();
        let id = store.register(FontSource { bytes, index: 0 }).unwrap();
        let found = store.decorations(id).unwrap();
        let underline = found.underline.expect("fixture has post underline");
        let post = raw.underline.unwrap();
        assert_eq!(underline.em().to_bits(), (post.offset / upem).to_bits());
        assert!(
            underline.em() < 0.0,
            "post underline sits below the baseline"
        );
        let strike = found.strikethrough.expect("fixture has OS/2 strikeout");
        assert!(strike.em() > 0.0, "OS/2 strikeout sits above the baseline");
    }

    #[test]
    fn zero_thickness_with_a_finite_offset_stays_usable() {
        let zero = Decoration {
            offset: -100.0,
            thickness: 0.0,
        };
        let found = em_offset(Some(zero.offset), 1000.0).unwrap();
        assert_eq!(found.em().to_bits(), (-0.1f32).to_bits());
        assert_eq!(em_offset(None, 1000.0), None);
        assert_eq!(em_offset(Some(f32::NAN), 1000.0), None);
        assert_eq!(em_offset(Some(f32::INFINITY), 1000.0), None);
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
                regular: FontFace::new(b"one".to_vec(), 0),
                bold: None,
                italic: None,
                bold_italic: None,
            },
        );
        fonts.register(
            "body",
            FontFamily {
                regular: FontFace::new(b"two".to_vec(), 3),
                bold: None,
                italic: None,
                bold_italic: None,
            },
        );
        let entries = fonts.into_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.regular.bytes.as_slice(), b"two");
        assert_eq!(entries[0].1.regular.index, 3);

        let single = FontRegistry::single(FontFamily {
            regular: FontFace::new(b"only".to_vec(), 1),
            bold: None,
            italic: None,
            bold_italic: None,
        });
        assert_eq!(single.into_entries()[0].0, ());
    }

    #[test]
    fn new_wraps_owned_bytes() {
        let face = FontFace::new(b"face".to_vec(), 0);
        assert_eq!(face.bytes.as_slice(), b"face");
    }

    #[test]
    fn from_shared_keeps_the_arc() {
        let source = Arc::new(b"face".to_vec());
        let face = FontFace::from_shared(Arc::clone(&source), 0);
        assert!(Arc::ptr_eq(&source, &face.bytes));
    }
}
