//! TrueType face loading and measurement.
//!
//! [`Font`] holds `Arc<Vec<u8>>`. Cloning a font does not copy the face bytes.
//! [`Font::from_shared`] stores the caller's `Arc`. [`Font::from_shared`] and
//! [`Font::from_shared_collection`] do not modify that `Arc`.

use std::fmt;
use std::sync::Arc;

use skrifa::charmap::Charmap;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::GlyphMetrics;
use skrifa::{FontRef, Tag};

use crate::error::{self, FontReason, Result};
#[derive(Clone, Copy)]
pub(crate) struct Glyph {
    pub(crate) id: u16,
    pub(crate) units: f32,
    /// Glyph ink `yMin` in font units. `0` when the face has no bounds.
    pub(crate) y_min: f32,
    /// Glyph ink `yMax` in font units. `0` when the face has no bounds.
    pub(crate) y_max: f32,
}

pub(crate) struct Face<'a> {
    charmap: Charmap<'a>,
    glyph_metrics: GlyphMetrics<'a>,
    upem: f32,
    ascent: f32,
    descent: f32,
}

#[derive(Clone, Copy)]
pub(crate) enum GlyphFault {
    Missing(char),
    NoAdvance,
}

impl<'a> Face<'a> {
    pub(crate) fn load(bytes: &'a [u8], index: u32) -> Result<Self> {
        use skrifa::MetadataProvider;
        let font = FontRef::from_index(bytes, index)
            .map_err(|err| error::font_unusable(FontReason::Unreadable).with_source(err))?;
        if font.table_data(Tag::new(b"glyf")).is_none() {
            return Err(error::font_unusable(FontReason::NoGlyf));
        }
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        if metrics.units_per_em == 0 {
            return Err(error::font_unusable(FontReason::UpemZero));
        }
        if !metrics.ascent.is_finite() || !metrics.descent.is_finite() {
            return Err(error::font_unusable(FontReason::Width));
        }
        let charmap = font.charmap();
        let glyph_metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        Ok(Self {
            charmap,
            glyph_metrics,
            upem: f32::from(metrics.units_per_em),
            ascent: metrics.ascent,
            descent: metrics.descent,
        })
    }

    pub(crate) fn upem(&self) -> f32 {
        self.upem
    }

    pub(crate) fn ascent(&self) -> f32 {
        self.ascent
    }

    pub(crate) fn descent(&self) -> f32 {
        self.descent
    }

    pub(crate) fn glyph(&self, scalar: char) -> std::result::Result<Glyph, GlyphFault> {
        let Some(gid) = self.charmap.map(scalar) else {
            return Err(GlyphFault::Missing(scalar));
        };
        let id = u16::try_from(gid.to_u32()).map_err(|_| GlyphFault::NoAdvance)?;
        let units = self
            .glyph_metrics
            .advance_width(gid)
            .ok_or(GlyphFault::NoAdvance)?;
        let (y_min, y_max) = match self.glyph_metrics.bounds(gid) {
            Some(bounds) if bounds.y_min.is_finite() && bounds.y_max.is_finite() => {
                (bounds.y_min, bounds.y_max)
            }
            _ => (0.0, 0.0),
        };
        Ok(Glyph {
            id,
            units,
            y_min,
            y_max,
        })
    }
}

/// One TrueType face with a `glyf` table.
///
/// The face bytes live in `Arc<Vec<u8>>`. [`Clone`] increments that `Arc` and
/// does not copy the bytes. [`Self::from_shared`] stores the caller's `Arc`.
/// [`Self::from_shared`] and [`Self::from_shared_collection`] do not modify it.
///
/// Construction reads the face and does not clear `head`. Subset embedding
/// clears `head` on the subset only.
#[derive(Clone)]
pub struct Font {
    bytes: Arc<Vec<u8>>,
    collection_index: Option<u32>,
    upem: f32,
    ascent: f32,
    descent: f32,
}

impl Font {
    /// One `sfnt`. Bytes that start with `ttcf` fail.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ErrorKind::FontNotConfigured`] when the bytes are a
    /// collection, the `head` table cannot be read, or the face has no usable
    /// `glyf` metrics.
    pub fn new(data: impl Into<Vec<u8>>) -> Result<Self> {
        Self::from_shared(Arc::new(data.into()))
    }

    /// One `sfnt`, keeping `data` as this font's byte store.
    ///
    /// # Errors
    ///
    /// See [`Self::new`].
    pub fn from_shared(data: Arc<Vec<u8>>) -> Result<Self> {
        if data.starts_with(b"ttcf") {
            return Err(error::font_unusable(FontReason::Collection));
        }
        load_face(data, None)
    }

    /// The face at `index` inside a `ttcf` collection.
    ///
    /// Bytes that do not start with `ttcf` fail. The extracted face is a new
    /// byte buffer. `data` is left unchanged when this is reached through
    /// [`Self::from_shared_collection`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::ErrorKind::FontNotConfigured`] when the bytes are not a
    /// collection, `index` does not select a face, or the extracted face fails
    /// the same checks as [`Self::new`].
    pub fn from_collection(data: impl Into<Vec<u8>>, index: u32) -> Result<Self> {
        Self::from_shared_collection(Arc::new(data.into()), index)
    }

    /// [`Self::from_collection`] without modifying `data`.
    ///
    /// # Errors
    ///
    /// See [`Self::from_collection`].
    #[expect(
        clippy::needless_pass_by_value,
        reason = "按值接收 Arc，与 from_shared 的签名一致；函数只读这份 Arc，不保存它"
    )]
    pub fn from_shared_collection(data: Arc<Vec<u8>>, index: u32) -> Result<Self> {
        if !data.starts_with(b"ttcf") {
            return Err(error::font_unusable(FontReason::Collection));
        }
        let face = extract_collection_face(&data, index)?;
        load_face(Arc::new(face), Some(index))
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn upem(&self) -> f32 {
        self.upem
    }

    pub(crate) fn ascent(&self) -> f32 {
        self.ascent
    }

    pub(crate) fn descent(&self) -> f32 {
        self.descent
    }
}

impl fmt::Debug for Font {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = formatter.debug_struct("Font");
        builder.field("bytes", &self.bytes.len());
        if let Some(index) = self.collection_index {
            builder.field("index", &index);
        }
        builder.field("units_per_em", &self.upem);
        builder.finish_non_exhaustive()
    }
}

fn load_face(bytes: Arc<Vec<u8>>, collection_index: Option<u32>) -> Result<Font> {
    check_head(&bytes)?;
    let (upem, ascent, descent) = {
        let face = Face::load(&bytes, 0)?;
        (face.upem(), face.ascent(), face.descent())
    };
    Ok(Font {
        bytes,
        collection_index,
        upem,
        ascent,
        descent,
    })
}

fn check_head(bytes: &[u8]) -> Result<()> {
    head_span(bytes).map(|_| ())
}

/// Clears `head` timestamps and `checkSumAdjustment`, then rewrites both checksums.
pub(crate) fn zero_head_times(bytes: &mut [u8]) -> Result<()> {
    let (off, head_len, record) = head_span(bytes)?;
    bytes[off + 20..off + 36].fill(0);
    bytes[off + 8..off + 12].fill(0);
    let sum = crate::sfnt::checksum(&bytes[off..off + head_len]);
    bytes[record + 4..record + 8].copy_from_slice(&sum.to_be_bytes());
    let whole = crate::sfnt::checksum(bytes);
    let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(whole);
    bytes[off + 8..off + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(())
}

fn head_span(bytes: &[u8]) -> Result<(usize, usize, usize)> {
    if bytes.len() < 12 {
        return Err(error::font_unusable(FontReason::Head));
    }
    let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    let mut head_off = None;
    let mut head_len = 0_usize;
    let mut record = 0_usize;
    for index in 0..tables {
        let at = 12 + index * 16;
        if at + 16 > bytes.len() {
            return Err(error::font_unusable(FontReason::Head));
        }
        if &bytes[at..at + 4] == b"head" {
            head_off = Some(usize::try_from(u32be(&bytes[at + 8..at + 12])).unwrap_or(usize::MAX));
            head_len = usize::try_from(u32be(&bytes[at + 12..at + 16])).unwrap_or(0);
            record = at;
        }
    }
    let Some(off) = head_off else {
        return Err(error::font_unusable(FontReason::Head));
    };
    if head_len < 54 || off.saturating_add(head_len) > bytes.len() {
        return Err(error::font_unusable(FontReason::Head));
    }
    Ok((off, head_len, record))
}

/// Copies one `sfnt` out of a `ttcf` collection. Table bytes are shared in the
/// collection, so the face is rebuilt rather than sliced.
fn extract_collection_face(bytes: &[u8], index: u32) -> Result<Vec<u8>> {
    if bytes.len() < 12 {
        return Err(error::font_unusable(FontReason::Collection));
    }
    let count = u32be(&bytes[8..12]);
    if index >= count {
        return Err(error::font_unusable(FontReason::Collection));
    }
    let index = usize::try_from(index).map_err(|_| error::font_unusable(FontReason::Collection))?;
    let entry = 12 + index * 4;
    if entry + 4 > bytes.len() {
        return Err(error::font_unusable(FontReason::Collection));
    }
    let sfnt = usize::try_from(u32be(&bytes[entry..entry + 4]))
        .map_err(|_| error::font_unusable(FontReason::Collection))?;
    if sfnt + 12 > bytes.len() {
        return Err(error::font_unusable(FontReason::Collection));
    }
    let tables = usize::from(u16::from_be_bytes([bytes[sfnt + 4], bytes[sfnt + 5]]));
    let dir = sfnt + 12;
    if dir + tables * 16 > bytes.len() {
        return Err(error::font_unusable(FontReason::Head));
    }
    let mut records = Vec::with_capacity(tables);
    for table in 0..tables {
        let at = dir + table * 16;
        let mut tag = [0_u8; 4];
        let mut checksum = [0_u8; 4];
        tag.copy_from_slice(&bytes[at..at + 4]);
        checksum.copy_from_slice(&bytes[at + 4..at + 8]);
        let off = usize::try_from(u32be(&bytes[at + 8..at + 12]))
            .map_err(|_| error::font_unusable(FontReason::Head))?;
        let len = usize::try_from(u32be(&bytes[at + 12..at + 16]))
            .map_err(|_| error::font_unusable(FontReason::Head))?;
        if off.saturating_add(len) > bytes.len() {
            return Err(error::font_unusable(FontReason::Head));
        }
        records.push((tag, checksum, bytes[off..off + len].to_vec()));
    }
    let count_tables =
        u16::try_from(records.len()).map_err(|_| error::font_unusable(FontReason::Head))?;
    let mut search_units = 1_u16;
    let mut entry_selector = 0_u16;
    while usize::from(search_units) * 2 <= records.len() {
        search_units *= 2;
        entry_selector += 1;
    }
    let search_range = search_units.saturating_mul(16);
    let range_shift = count_tables.saturating_mul(16).saturating_sub(search_range);
    let mut out = vec![0_u8; 12 + records.len() * 16];
    out[0..4].copy_from_slice(&bytes[sfnt..sfnt + 4]);
    out[4..6].copy_from_slice(&count_tables.to_be_bytes());
    out[6..8].copy_from_slice(&search_range.to_be_bytes());
    out[8..10].copy_from_slice(&entry_selector.to_be_bytes());
    out[10..12].copy_from_slice(&range_shift.to_be_bytes());
    let mut cursor = out.len();
    for (table, (tag, checksum, data)) in records.iter().enumerate() {
        let at = 12 + table * 16;
        out[at..at + 4].copy_from_slice(tag);
        out[at + 4..at + 8].copy_from_slice(checksum);
        let pad = (4 - (cursor % 4)) % 4;
        out.extend(std::iter::repeat_n(0, pad));
        cursor += pad;
        let off = u32::try_from(cursor).map_err(|_| error::font_unusable(FontReason::Head))?;
        let len = u32::try_from(data.len()).map_err(|_| error::font_unusable(FontReason::Head))?;
        out[at + 8..at + 12].copy_from_slice(&off.to_be_bytes());
        out[at + 12..at + 16].copy_from_slice(&len.to_be_bytes());
        out.extend_from_slice(data);
        cursor = out.len();
    }
    Ok(out)
}

fn u32be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::error::{ErrorKind, FontReason};
    use crate::subset::abc_test_font;

    use super::Font;

    #[test]
    fn extracts_one_face_from_a_collection_when_present() {
        let path = std::path::Path::new(
            "/mnt/data/dev/hcsw-pdfcraft/backend/assets/fonts/SourceHanSerifHK/SourceHanSerifHK-Regular.ttc",
        );
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(path).unwrap();
        let font = Font::from_collection(bytes, 0).unwrap();
        assert!(!font.bytes().starts_with(b"ttcf"));
        let face = super::Face::load(font.bytes(), 0).unwrap();
        assert!(face.glyph('中').is_ok());
    }

    #[test]
    fn from_shared_keeps_the_caller_arc_and_leaves_head_times() {
        let bytes = abc_test_font();
        assert!(bytes.windows(16).any(|window| window == [0x55; 16]));
        let shared = Arc::new(bytes);
        let font = Font::from_shared(Arc::clone(&shared)).unwrap();
        assert!(Arc::ptr_eq(&font.bytes, &shared));
        assert!(shared.windows(16).any(|window| window == [0x55; 16]));
        let shown = format!("{font:?}");
        assert!(!shown.contains("UUUUUUUUUUUUUUUU"));
        assert!(!shown.contains("index"));
        assert!(shown.contains("units_per_em"));
    }

    #[test]
    fn new_rejects_a_collection_and_from_collection_rejects_one_face() {
        let face = abc_test_font();
        let collection = collection_of(&face);
        let single = Font::new(collection.clone()).unwrap_err();
        assert_eq!(
            single.kind(),
            ErrorKind::FontNotConfigured(FontReason::Collection)
        );
        let wrapped = Font::from_collection(face, 0).unwrap_err();
        assert_eq!(
            wrapped.kind(),
            ErrorKind::FontNotConfigured(FontReason::Collection)
        );
        let font = Font::from_collection(collection.clone(), 0).unwrap();
        assert!(format!("{font:?}").contains("index"));
        assert!(!font.bytes().starts_with(b"ttcf"));
        let shared = Arc::new(collection);
        let before = Arc::strong_count(&shared);
        let _extracted = Font::from_shared_collection(Arc::clone(&shared), 0).unwrap();
        assert!(shared.starts_with(b"ttcf"));
        assert_eq!(Arc::strong_count(&shared), before);
    }

    #[test]
    fn construction_reports_head_metrics_and_parser_failures() {
        let empty = Font::new(Vec::new()).unwrap_err();
        assert_eq!(empty.kind(), ErrorKind::FontNotConfigured(FontReason::Head));
        assert!(std::error::Error::source(&empty).is_none());

        let mut short = abc_test_font();
        set_head_len(&mut short, 53);
        let head = Font::new(short).unwrap_err();
        assert_eq!(head.kind(), ErrorKind::FontNotConfigured(FontReason::Head));

        let mut no_glyf = abc_test_font();
        rename_tag(&mut no_glyf, *b"glyf", *b"glyX");
        let missing = Font::new(no_glyf).unwrap_err();
        assert_eq!(
            missing.kind(),
            ErrorKind::FontNotConfigured(FontReason::NoGlyf)
        );

        let mut zero_upem = abc_test_font();
        set_head_u16(&mut zero_upem, 18, 0);
        let upem = Font::new(zero_upem).unwrap_err();
        assert_eq!(
            upem.kind(),
            ErrorKind::FontNotConfigured(FontReason::UpemZero)
        );

        let mut garbage = abc_test_font();
        garbage[..4].fill(0);
        let unread = Font::new(garbage).unwrap_err();
        assert_eq!(
            unread.kind(),
            ErrorKind::FontNotConfigured(FontReason::Unreadable)
        );
        assert!(std::error::Error::source(&unread).is_some());

        let bad_index = Font::from_collection(collection_of(&abc_test_font()), 9).unwrap_err();
        assert_eq!(
            bad_index.kind(),
            ErrorKind::FontNotConfigured(FontReason::Collection)
        );
    }

    fn collection_of(face: &[u8]) -> Vec<u8> {
        let mut face = face.to_vec();
        let tables = usize::from(u16::from_be_bytes([face[4], face[5]]));
        for index in 0..tables {
            let at = 12 + index * 16;
            let off = u32::from_be_bytes(face[at + 8..at + 12].try_into().unwrap());
            face[at + 8..at + 12].copy_from_slice(&(off + 16).to_be_bytes());
        }
        let mut out = Vec::with_capacity(16 + face.len());
        out.extend_from_slice(b"ttcf");
        out.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        out.extend_from_slice(&1_u32.to_be_bytes());
        out.extend_from_slice(&16_u32.to_be_bytes());
        out.extend_from_slice(&face);
        out
    }

    fn set_head_len(bytes: &mut [u8], len: u32) {
        let at = tag_record(bytes, *b"head");
        bytes[at + 12..at + 16].copy_from_slice(&len.to_be_bytes());
    }

    fn set_head_u16(bytes: &mut [u8], offset: usize, value: u16) {
        let at = tag_record(bytes, *b"head");
        let off = u32::from_be_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
        bytes[off + offset..off + offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn rename_tag(bytes: &mut [u8], from: [u8; 4], to: [u8; 4]) {
        let at = tag_record(bytes, from);
        bytes[at..at + 4].copy_from_slice(&to);
    }

    fn tag_record(bytes: &[u8], tag: [u8; 4]) -> usize {
        let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        (0..tables)
            .map(|index| 12 + index * 16)
            .find(|at| bytes[*at..*at + 4] == tag)
            .unwrap()
    }
}
