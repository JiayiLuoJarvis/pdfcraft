//! Embed a used-glyph TrueType subset into the open PDF.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::error::{self, FontReason, Result};
use crate::font;
use crate::pdf::PdfDoc;
use crate::subset;

use super::PaintFont;

pub(crate) fn embed(prepared: &PaintFont<'_>, doc: &mut PdfDoc) -> Result<ObjectId> {
    let mut subset = subset::subset_sfnt(prepared.bytes, prepared.used)?;
    font::zero_head_times(&mut subset)?;
    let file = Stream::new(Dictionary::new(), subset).with_compression(false);
    let file_id = doc.add_object(Object::Stream(file));
    let descriptor = doc.add_object(Object::Dictionary(descriptor(
        file_id,
        prepared.upem,
        prepared.ascent,
        prepared.descent,
    )?));
    let cid = doc.add_object(Object::Dictionary(cid_font(
        descriptor,
        prepared.used,
        prepared.upem,
    )?));
    let unicode = Stream::new(Dictionary::new(), to_unicode(prepared.used).into_bytes());
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

fn to_unicode(used: &BTreeMap<u16, (char, f32)>) -> String {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /QuireForms def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<_> = used
        .iter()
        .map(|(gid, (scalar, _))| (*gid, *scalar))
        .collect();
    for chunk in entries.chunks(100) {
        write_cmap(&mut body, format_args!("{} beginbfchar\n", chunk.len()));
        for (gid, scalar) in chunk {
            write_cmap(
                &mut body,
                format_args!("<{gid:04X}> <{}>\n", utf16_hex(*scalar)),
            );
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
        write_cmap(&mut hex, format_args!("{unit:04X}"));
    }
    hex
}

fn write_cmap(body: &mut String, args: std::fmt::Arguments<'_>) {
    #[expect(
        clippy::expect_used,
        reason = "fmt::Write for String returns Ok after push_str"
    )]
    std::fmt::Write::write_fmt(body, args).expect("writing into a String cannot fail");
}

fn name(value: &str) -> Object {
    Object::Name(value.as_bytes().to_vec())
}

fn em1000(units: f32, upem: f32) -> Result<i64> {
    let rounded = (f64::from(units) * 1000.0 / f64::from(upem)).round();
    if !rounded.is_finite() || rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return Err(error::font_unusable(FontReason::Width));
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "已限制在 i32 范围内；Integer 是 i64"
    )]
    Ok(rounded as i64)
}

#[cfg(test)]
mod tests {
    use super::em1000;
    use crate::error::{ErrorKind, FontReason};

    #[test]
    fn em1000_rejects_non_finite_and_out_of_i32_range() {
        let nan = em1000(f32::NAN, 2048.0).unwrap_err();
        assert_eq!(nan.kind(), ErrorKind::FontNotConfigured(FontReason::Width));
        assert!(std::error::Error::source(&nan).is_none());

        let huge = em1000(1.0e20, 1.0).unwrap_err();
        assert_eq!(huge.kind(), ErrorKind::FontNotConfigured(FontReason::Width));
        assert!(std::error::Error::source(&huge).is_none());

        assert_eq!(em1000(100_000.0, 1.0).unwrap(), 100_000_000);
    }
}
