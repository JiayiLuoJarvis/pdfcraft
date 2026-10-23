use std::collections::BTreeMap;

use lopdf::{Dictionary, Object, Stream};

use crate::inspect::PdfDoc;
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, Tag};

use crate::error::{self, FontReason, Result};
use crate::subset;

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
    font: FontRef<'a>,
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
        Ok(Self {
            font,
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
        use skrifa::MetadataProvider;
        let Some(gid) = self.font.charmap().map(scalar) else {
            return Err(GlyphFault::Missing(scalar));
        };
        let id = u16::try_from(gid.to_u32()).map_err(|_| GlyphFault::NoAdvance)?;
        let metrics = self
            .font
            .glyph_metrics(Size::unscaled(), LocationRef::default());
        let units = metrics.advance_width(gid).ok_or(GlyphFault::NoAdvance)?;
        let (y_min, y_max) = match metrics.bounds(gid) {
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

pub(crate) struct FontFile {
    bytes: Vec<u8>,
}

impl FontFile {
    /// Copies one TrueType face. A collection is reduced to the face at `index`.
    pub(crate) fn from_source(source: &[u8], index: u32) -> Result<Self> {
        let bytes = if source.starts_with(b"ttcf") {
            extract_collection_face(source, index)?
        } else {
            source.to_vec()
        };
        let mut file = Self { bytes };
        file.zero_head_times()?;
        Ok(file)
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn zero_head_times(&mut self) -> Result<()> {
        if self.bytes.len() < 12 {
            return Err(error::font_unusable(FontReason::Head));
        }
        let tables = usize::from(u16::from_be_bytes([self.bytes[4], self.bytes[5]]));
        let mut head_off = None;
        let mut head_len = 0_usize;
        let mut record = 0_usize;
        for index in 0..tables {
            let at = 12 + index * 16;
            if at + 16 > self.bytes.len() {
                return Err(error::font_unusable(FontReason::Head));
            }
            if &self.bytes[at..at + 4] == b"head" {
                head_off = Some(
                    usize::try_from(u32be(&self.bytes[at + 8..at + 12])).unwrap_or(usize::MAX),
                );
                head_len = usize::try_from(u32be(&self.bytes[at + 12..at + 16])).unwrap_or(0);
                record = at;
            }
        }
        let Some(off) = head_off else {
            return Err(error::font_unusable(FontReason::Head));
        };
        if head_len < 54 || off.saturating_add(head_len) > self.bytes.len() {
            return Err(error::font_unusable(FontReason::Head));
        }
        self.bytes[off + 20..off + 36].fill(0);
        self.bytes[off + 8..off + 12].fill(0);
        let sum = checksum(&self.bytes[off..off + head_len]);
        self.bytes[record + 4..record + 8].copy_from_slice(&sum.to_be_bytes());
        let whole = checksum(&self.bytes);
        let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(whole);
        self.bytes[off + 8..off + 12].copy_from_slice(&adjustment.to_be_bytes());
        Ok(())
    }
}

pub(crate) struct PreparedFont<'a> {
    face: Face<'a>,
    bytes: &'a [u8],
    used: BTreeMap<u16, (char, f32)>,
}

impl<'a> PreparedFont<'a> {
    pub(crate) fn load(file: &'a FontFile, index: u32) -> Result<Self> {
        Ok(Self {
            face: Face::load(file.bytes(), index)?,
            bytes: file.bytes(),
            used: BTreeMap::new(),
        })
    }

    pub(crate) fn face(&self) -> &Face<'a> {
        &self.face
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.used.is_empty()
    }

    pub(crate) fn record(&mut self, id: u16, scalar: char, units: f32) {
        self.used.entry(id).or_insert((scalar, units));
    }

    pub(crate) fn embed(&self, doc: &mut PdfDoc) -> Result<ObjectId> {
        let subset = subset::subset_sfnt(self.bytes, &self.used)?;
        let stamped = FontFile::from_source(&subset, 0)?;
        let file = Stream::new(Dictionary::new(), stamped.bytes().to_vec()).with_compression(false);
        let file_id = doc.add_object(Object::Stream(file));
        let descriptor = doc.add_object(Object::Dictionary(descriptor(
            file_id,
            self.face.upem(),
            self.face.ascent(),
            self.face.descent(),
        )?));
        let cid = doc.add_object(Object::Dictionary(cid_font(
            descriptor,
            &self.used,
            self.face.upem(),
        )?));
        let unicode = Stream::new(Dictionary::new(), to_unicode(&self.used).into_bytes());
        let unicode_id = doc.add_object(Object::Stream(unicode));
        let mut font = Dictionary::new();
        font.set("Type", name("Font"));
        font.set("Subtype", name("Type0"));
        font.set("BaseFont", name("QuireForms"));
        font.set("Encoding", name("Identity-H"));
        font.set(
            "DescendantFonts",
            Object::Array(vec![Object::Reference(cid)]),
        );
        font.set("ToUnicode", Object::Reference(unicode_id));
        Ok(doc.add_object(Object::Dictionary(font)))
    }
}

type ObjectId = lopdf::ObjectId;

fn descriptor(file_id: ObjectId, upem: f32, ascent: f32, descent: f32) -> Result<Dictionary> {
    let ascent = em1000(ascent, upem)?;
    let descent = em1000(descent, upem)?;
    let mut dict = Dictionary::new();
    dict.set("Type", name("FontDescriptor"));
    dict.set("FontName", name("QuireForms"));
    dict.set("Flags", 32_i64);
    dict.set(
        "FontBBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(descent),
            Object::Integer(1000),
            Object::Integer(ascent),
        ]),
    );
    dict.set("ItalicAngle", 0_i64);
    dict.set("Ascent", ascent);
    dict.set("Descent", descent);
    dict.set("CapHeight", ascent);
    dict.set("StemV", 80_i64);
    dict.set("FontFile2", Object::Reference(file_id));
    Ok(dict)
}

fn cid_font(
    descriptor: ObjectId,
    used: &BTreeMap<u16, (char, f32)>,
    upem: f32,
) -> Result<Dictionary> {
    let mut system = Dictionary::new();
    system.set("Registry", Object::string_literal("Adobe"));
    system.set("Ordering", Object::string_literal("Identity"));
    system.set("Supplement", 0_i64);
    let mut widths = Vec::new();
    for (gid, (_, units)) in used {
        widths.push(Object::Integer(i64::from(*gid)));
        widths.push(Object::Array(vec![Object::Integer(em1000(*units, upem)?)]));
    }
    let mut dict = Dictionary::new();
    dict.set("Type", name("Font"));
    dict.set("Subtype", name("CIDFontType2"));
    dict.set("BaseFont", name("QuireForms"));
    dict.set("CIDSystemInfo", system);
    dict.set("FontDescriptor", Object::Reference(descriptor));
    dict.set("CIDToGIDMap", name("Identity"));
    dict.set("DW", 1000_i64);
    dict.set("W", Object::Array(widths));
    Ok(dict)
}

#[allow(clippy::format_push_string)] // 规格要求 push_str / format!，不用丢弃 write! 的 Result
fn to_unicode(used: &BTreeMap<u16, (char, f32)>) -> String {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /QuireForms def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<_> = used
        .iter()
        .map(|(gid, (scalar, _))| (*gid, *scalar))
        .collect();
    for chunk in entries.chunks(100) {
        body.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, scalar) in chunk {
            body.push_str(&format!("<{gid:04X}> <{}>\n", utf16_hex(*scalar)));
        }
        body.push_str("endbfchar\n");
    }
    body.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    body
}

#[allow(clippy::format_push_string)] // 规格要求 push_str / format!，不用丢弃 write! 的 Result
fn utf16_hex(scalar: char) -> String {
    let mut buf = [0_u16; 2];
    let mut hex = String::new();
    for unit in scalar.encode_utf16(&mut buf) {
        hex.push_str(&format!("{unit:04X}"));
    }
    hex
}

fn name(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

fn em1000(units: f32, upem: f32) -> Result<i64> {
    let rounded = (f64::from(units) * 1000.0 / f64::from(upem)).round();
    if !rounded.is_finite() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return Err(error::font_unusable(FontReason::Width));
    }
    #[allow(clippy::cast_possible_truncation)] // 已限制在 i32 范围内；Integer 是 i64
    Ok(rounded as i64)
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

pub(crate) fn checksum(data: &[u8]) -> u32 {
    let mut sum = 0_u32;
    let mut index = 0;
    while index + 4 <= data.len() {
        sum = sum.wrapping_add(u32be(&data[index..index + 4]));
        index += 4;
    }
    if index < data.len() {
        let mut tail = [0_u8; 4];
        tail[..data.len() - index].copy_from_slice(&data[index..]);
        sum = sum.wrapping_add(u32::from_be_bytes(tail));
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::em1000;
    use crate::error::{ErrorKind, FontReason};

    #[test]
    fn em1000_rejects_non_finite_and_out_of_i32_range() {
        let nan = em1000(f32::NAN, 2048.0).unwrap_err();
        assert_eq!(
            nan.kind,
            ErrorKind::FontNotConfigured {
                reason: FontReason::Width,
            }
        );
        assert!(std::error::Error::source(&nan).is_none());

        let huge = em1000(1.0e20, 1.0).unwrap_err();
        assert_eq!(
            huge.kind,
            ErrorKind::FontNotConfigured {
                reason: FontReason::Width,
            }
        );
        assert!(std::error::Error::source(&huge).is_none());

        assert_eq!(em1000(100_000.0, 1.0).unwrap(), 100_000_000);
    }

    #[test]
    fn extracts_one_face_from_a_collection_when_present() {
        let path = std::path::Path::new(
            "/mnt/data/dev/hcsw-pdfcraft/backend/assets/fonts/SourceHanSerifHK/SourceHanSerifHK-Regular.ttc",
        );
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(path).unwrap();
        let file = super::FontFile::from_source(&bytes, 0).unwrap();
        assert!(!file.bytes().starts_with(b"ttcf"));
        let face = super::Face::load(file.bytes(), 0).unwrap();
        assert!(face.glyph('中').is_ok());
    }
}
