use std::collections::{BTreeMap, HashMap};

use crate::error::{ArgumentReason, Error, ErrorKind, FontFault, Result};
use crate::fields::{FieldValue, FillOptions, MissingName};
use crate::font::Face;
use crate::inspect::{Draw, PdfDoc, WidgetSnap};
use crate::layout::{self, LaidText};
use crate::overlay::{self, Mark, Stamp};

/// Draws `fields` onto `pdf` and strips every widget plus the catalog `AcroForm` entry.
///
/// An empty map still returns a PDF with the form removed. Text and choices are
/// drawn only when the string is non-empty, and only when [`FillOptions::font`]
/// holds a TrueType font with a `glyf` table. A checked box is a path clipped
/// to the widget rectangle. The library does not write field values or appearance streams.
///
/// # Errors
///
/// Returns the same open failures as [`crate::list_fields`]. A name that is not
/// in the file is [`ErrorKind::UnknownField`] unless [`MissingName::Ignore`] is set.
/// Signature fields are [`ErrorKind::SignatureNotWritable`]. Radio and push
/// buttons are [`ErrorKind::WidgetUnsupported`]. A checkbox given text is
/// [`ErrorKind::InvalidCheckbox`]. A choice outside `/Opt` is
/// [`ErrorKind::IllegalChoice`]. Missing glyphs and text that cannot fit are
/// [`ErrorKind::MissingGlyph`] and [`ErrorKind::BoxOverflow`].
pub fn fill_and_flatten<S: std::hash::BuildHasher>(
    pdf: &[u8],
    fields: &HashMap<String, FieldValue, S>,
    options: &FillOptions<'_>,
) -> Result<Vec<u8>> {
    check_font_size(options.font_size)?;
    let mut doc = PdfDoc::open(pdf)?;
    let snaps = doc.widgets();
    let planned = plan(&snaps, fields, options.missing)?;
    let needs_font = planned.values().any(|draw| matches!(draw, Draw::Text(_)));
    let face = if needs_font {
        let font = options.font.as_ref().ok_or_else(|| {
            Error::from(ErrorKind::FontNotConfigured).with_source(FontFault::Missing)
        })?;
        Some(Face::load(font.bytes, font.index)?)
    } else {
        None
    };
    let mut used = BTreeMap::new();
    let mut stamps = Vec::new();
    for snap in &snaps {
        let Some(draw) = planned.get(snap.name.as_str()) else {
            continue;
        };
        let mark = match draw {
            Draw::Check => Mark::Check,
            Draw::Text(text) => {
                let face = face.as_ref().ok_or_else(|| {
                    Error::from(ErrorKind::FontNotConfigured).with_source(FontFault::Missing)
                })?;
                let laid = LaidText::fit(face, &snap.name, text, snap.rect, options.font_size)?;
                for glyph in laid.glyphs() {
                    used.entry(glyph.id).or_insert((glyph.scalar, glyph.units));
                }
                Mark::Text(laid)
            }
        };
        stamps.push(Stamp {
            page_index: snap.page_index,
            rect: snap.rect,
            mark,
        });
    }
    let font_bytes = options.font.as_ref().map(|font| font.bytes);
    overlay::paint(&mut doc, &stamps, font_bytes, &used, face.as_ref())?;
    doc.save()
}

fn plan<'s, 'f, S: std::hash::BuildHasher>(
    snaps: &'s [WidgetSnap],
    fields: &'f HashMap<String, FieldValue, S>,
    missing: MissingName,
) -> Result<BTreeMap<&'s str, Draw<'f>>> {
    let mut first: BTreeMap<&str, &WidgetSnap> = BTreeMap::new();
    for snap in snaps {
        first.entry(snap.name.as_str()).or_insert(snap);
    }
    if missing == MissingName::Error {
        let mut unknown: Vec<&String> = fields
            .keys()
            .filter(|name| !first.contains_key(name.as_str()))
            .collect();
        unknown.sort();
        if let Some(name) = unknown.first() {
            return Err(ErrorKind::UnknownField {
                field_name: (*name).clone(),
            }
            .into());
        }
    }
    let mut names: Vec<&str> = fields
        .keys()
        .map(String::as_str)
        .filter(|name| first.contains_key(name))
        .collect();
    names.sort_unstable();
    let mut planned = BTreeMap::new();
    for name in names {
        let snap = first[name];
        let value = &fields[name];
        if let Some(draw) = snap.accept(value)? {
            planned.insert(snap.name.as_str(), draw);
        }
    }
    Ok(planned)
}

fn check_font_size(size: f32) -> Result<()> {
    if !size.is_finite() || size <= 0.0 {
        return Err(bad_font_size(size));
    }
    let mp = (f64::from(size) * 1000.0).round();
    #[allow(clippy::cast_precision_loss)] // 比较的是毫点能否放进 i64，不是字号精度
    if !mp.is_finite() || mp > i64::MAX as f64 {
        return Err(bad_font_size(size));
    }
    #[allow(clippy::cast_precision_loss)] // 256_000 在 f64 里是精确值
    if mp > layout::MAX_START_MP as f64 {
        return Err(bad_font_size(size));
    }
    Ok(())
}

fn bad_font_size(value: f32) -> Error {
    ErrorKind::InvalidArgument {
        field_name: None,
        reason: ArgumentReason::FontSize { value },
    }
    .into()
}
