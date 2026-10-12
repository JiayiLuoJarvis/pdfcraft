use std::collections::BTreeMap;
use std::fmt::{self, Write};

use lopdf::{Dictionary, Document, Object, Stream};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider, Tag};

use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Copy)]
pub(crate) struct Glyph {
    pub(crate) id: u16,
    pub(crate) units: f32,
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
        if bytes.starts_with(b"ttcf") {
            return Err(Error::from(ErrorKind::FontNotConfigured)
                .with_source(FontFault("font collections are not embedded")));
        }
        let font = FontRef::from_index(bytes, index)
            .map_err(|err| Error::from(ErrorKind::FontNotConfigured).with_source(err))?;
        if font.table_data(Tag::new(b"glyf")).is_none() {
            return Err(Error::from(ErrorKind::FontNotConfigured)
                .with_source(FontFault("font has no glyf table")));
        }
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        if metrics.units_per_em == 0 {
            return Err(Error::from(ErrorKind::FontNotConfigured)
                .with_source(FontFault("units per em is 0")));
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
        let Some(gid) = self.font.charmap().map(scalar) else {
            return Err(GlyphFault::Missing(scalar));
        };
        let id = u16::try_from(gid.to_u32()).map_err(|_| GlyphFault::NoAdvance)?;
        let units = self
            .font
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(gid)
            .ok_or(GlyphFault::NoAdvance)?;
        Ok(Glyph { id, units })
    }
}

pub(crate) fn embed(
    doc: &mut Document,
    bytes: &[u8],
    used: &BTreeMap<u16, (char, f32)>,
    upem: f32,
    ascent: f32,
    descent: f32,
) -> Result<ObjectId> {
    let file = Stream::new(Dictionary::new(), stable_font_bytes(bytes)?).with_compression(false);
    let file_id = doc.add_object(file);
    let descriptor = doc.add_object(descriptor(file_id, upem, ascent, descent));
    let cid = doc.add_object(cid_font(descriptor, used, upem));
    let unicode = Stream::new(Dictionary::new(), to_unicode(used).into_bytes());
    let unicode_id = doc.add_object(unicode);
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
    Ok(doc.add_object(font))
}

type ObjectId = lopdf::ObjectId;

fn descriptor(file_id: ObjectId, upem: f32, ascent: f32, descent: f32) -> Dictionary {
    let ascent = em1000(ascent, upem);
    let descent = em1000(descent, upem);
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
    dict
}

fn cid_font(descriptor: ObjectId, used: &BTreeMap<u16, (char, f32)>, upem: f32) -> Dictionary {
    let mut system = Dictionary::new();
    system.set("Registry", Object::string_literal("Adobe"));
    system.set("Ordering", Object::string_literal("Identity"));
    system.set("Supplement", 0_i64);
    let mut widths = Vec::new();
    for (gid, (_, units)) in used {
        widths.push(Object::Integer(i64::from(*gid)));
        widths.push(Object::Array(vec![Object::Integer(em1000(*units, upem))]));
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
    dict
}

fn to_unicode(used: &BTreeMap<u16, (char, f32)>) -> String {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /QuireForms def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<_> = used
        .iter()
        .map(|(gid, (scalar, _))| (*gid, *scalar))
        .collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(body, "{} beginbfchar", chunk.len());
        for (gid, scalar) in chunk {
            let _ = writeln!(body, "<{gid:04X}> <{}>", utf16_hex(*scalar));
        }
        body.push_str("endbfchar\n");
    }
    body.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    body
}

fn utf16_hex(scalar: char) -> String {
    let mut buf = [0_u16; 2];
    let mut hex = String::new();
    for unit in scalar.encode_utf16(&mut buf) {
        let _ = write!(hex, "{unit:04X}");
    }
    hex
}

fn name(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

fn em1000(units: f32, upem: f32) -> i64 {
    let rounded = (units * 1000.0 / upem).round();
    if !rounded.is_finite() || !(f32::from(i16::MIN)..=f32::from(i16::MAX)).contains(&rounded) {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    i64::from(rounded as i16)
}

fn stable_font_bytes(source: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = source.to_vec();
    if !zero_head_times(&mut bytes) {
        return Err(ErrorKind::FontNotConfigured.into());
    }
    Ok(bytes)
}

/// `head.checkSumAdjustment` is written last, after the table checksum treats that field as zero.
fn zero_head_times(font: &mut [u8]) -> bool {
    if font.len() < 12 || font.starts_with(b"ttcf") {
        return false;
    }
    let tables = usize::from(u16::from_be_bytes([font[4], font[5]]));
    let mut head_off = None;
    let mut head_len = 0_usize;
    let mut record = 0_usize;
    for index in 0..tables {
        let at = 12 + index * 16;
        if at + 16 > font.len() {
            return false;
        }
        if &font[at..at + 4] == b"head" {
            head_off = Some(usize::try_from(u32be(&font[at + 8..at + 12])).unwrap_or(usize::MAX));
            head_len = usize::try_from(u32be(&font[at + 12..at + 16])).unwrap_or(0);
            record = at;
        }
    }
    let Some(off) = head_off else {
        return false;
    };
    if head_len < 54 || off.saturating_add(head_len) > font.len() {
        return false;
    }
    font[off + 20..off + 36].fill(0);
    font[off + 8..off + 12].fill(0);
    let sum = checksum(&font[off..off + head_len]);
    font[record + 4..record + 8].copy_from_slice(&sum.to_be_bytes());
    let whole = checksum(font);
    let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(whole);
    font[off + 8..off + 12].copy_from_slice(&adjustment.to_be_bytes());
    true
}

fn u32be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn checksum(data: &[u8]) -> u32 {
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

#[derive(Debug)]
struct FontFault(&'static str);

impl fmt::Display for FontFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for FontFault {}
