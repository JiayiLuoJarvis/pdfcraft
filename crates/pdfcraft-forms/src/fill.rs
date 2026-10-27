//! Draws field values onto pages and strips the form.

use std::collections::{BTreeMap, HashMap};

use crate::content::{Mark, Stamp, Stroke};
use crate::error::{self, ArgumentReason, Error, ErrorKind, FontReason, Result};
use crate::fields::{AbsoluteLine, FieldValue, FillOptions, HAlign, MissingName, VAlign};
use crate::font::{FontFile, PreparedFont};
use crate::inspect::{Draw, PdfDoc, WidgetSnap};
use crate::layout::{self, LaidText};

/// Draws `fields` onto `pdf` and strips every widget plus the catalog `AcroForm` entry.
///
/// An empty map still returns a PDF with the form removed. Text and choices are
/// drawn only when the string is non-empty, and only when [`FillOptions::font`]
/// holds a TrueType font with a `glyf` table. A checked box is a path clipped
/// to the widget rectangle. The library does not write field values or appearance streams.
///
/// # Errors
///
/// Returns the first failure as one [`Error`].
///
/// - Opening `pdf` reports the same kinds as [`crate::list_fields`]:
///   [`ErrorKind::NotAPdf`], [`ErrorKind::Encrypted`], or [`ErrorKind::HasXfa`].
/// - Names and arguments: [`ErrorKind::UnknownField`] for a name that is not in
///   the file unless [`MissingName::Ignore`] is set (alignment override maps
///   follow the same rule); [`ErrorKind::InvalidArgument`] for a bad `font_size`,
///   a non-finite [`AbsoluteLine`] or `line_below` offset, or a
///   [`FieldValue::CheckBox`] sent to a text or choice widget. A checkbox widget
///   given text is [`ErrorKind::InvalidCheckbox`], and a choice outside `/Opt`
///   is [`ErrorKind::IllegalChoice`]. Signature fields are
///   [`ErrorKind::SignatureNotWritable`]; radio and push buttons are
///   [`ErrorKind::WidgetUnsupported`].
/// - Fonts and text: [`ErrorKind::FontNotConfigured`] when text needs a face that
///   is missing or unusable ([`FontReason`] names the reason),
///   [`ErrorKind::MissingGlyph`] when the face has no glyph, and
///   [`ErrorKind::BoxOverflow`] when the text cannot fit at the minimum size.
/// - Writing: [`ErrorKind::FlattenFailed`] when the overlay or PDF write fails;
///   [`FlattenReason`](crate::FlattenReason) names the step.
///
/// A parser, font, or lopdf error behind a failure stays reachable through
/// [`std::error::Error::source`].
pub fn fill_and_flatten<S: std::hash::BuildHasher>(
    pdf: &[u8],
    fields: &HashMap<String, FieldValue, S>,
    options: &FillOptions<'_>,
) -> Result<Vec<u8>> {
    check_font_size(options.font_size)?;
    let mut doc = PdfDoc::open(pdf)?;
    let snaps = doc.widgets();
    let planned = plan(&snaps, fields, options)?;
    let needs_font = planned.values().any(|draw| matches!(draw, Draw::Text(_)));
    let font_file = if needs_font {
        let font = options
            .font
            .as_ref()
            .ok_or_else(|| error::font_unusable(FontReason::Missing))?;
        Some(FontFile::from_source(font.bytes, font.index)?)
    } else {
        None
    };
    let extracted = options
        .font
        .as_ref()
        .is_some_and(|font| font.bytes.starts_with(b"ttcf"));
    let face_index = if extracted {
        0
    } else {
        options.font.as_ref().map_or(0, |font| font.index)
    };
    let mut prepared = match font_file.as_ref() {
        Some(file) => Some(PreparedFont::load(file, face_index)?),
        None => None,
    };
    let mut stamps = Vec::new();
    for snap in &snaps {
        if let Some(draw) = planned.get(snap.name.as_str()) {
            let mark = match draw {
                Draw::Check => Mark::Check,
                Draw::Text(text) => {
                    let Some(font) = prepared.as_mut() else {
                        return Err(error::font_unusable(FontReason::Missing));
                    };
                    let v_align = v_align_for(options, snap.name.as_str());
                    let laid = LaidText::fit(
                        font.face(),
                        &snap.name,
                        text,
                        snap.rect,
                        font_size_for(options, snap.name.as_str()),
                        v_align,
                    )?;
                    for glyph in laid.glyphs() {
                        font.record(glyph.id, glyph.scalar, glyph.units);
                    }
                    Mark::Text {
                        laid,
                        h_align: h_align_for(options, snap.name.as_str()),
                    }
                }
            };
            stamps.push(Stamp {
                page_index: snap.page_index,
                rect: snap.rect,
                mark,
            });
        }
        if let Some(offset) = line_below_for(options, snap.name.as_str()) {
            stamps.push(Stamp {
                page_index: snap.page_index,
                rect: snap.rect,
                mark: Mark::LineBelow { offset },
            });
        }
    }
    let strokes = strokes_for(options.absolute_lines)?;
    doc.paint(&stamps, &strokes, prepared.as_ref())?;
    doc.save()
}

fn plan<'s, 'f, S: std::hash::BuildHasher>(
    snaps: &'s [WidgetSnap],
    fields: &'f HashMap<String, FieldValue, S>,
    options: &FillOptions<'_>,
) -> Result<BTreeMap<&'s str, Draw<'f>>> {
    let mut first: BTreeMap<&str, &WidgetSnap> = BTreeMap::new();
    for snap in snaps {
        first.entry(snap.name.as_str()).or_insert(snap);
    }
    if options.missing == MissingName::Error {
        if let Some(name) = first_unknown(&first, fields.keys().map(String::as_str)) {
            return Err(ErrorKind::UnknownField {
                field_name: name.to_string(),
            }
            .into());
        }
        if let Some(map) = options.h_align_overrides {
            if let Some(name) = first_unknown(&first, map.keys().map(String::as_str)) {
                return Err(ErrorKind::UnknownField {
                    field_name: name.to_string(),
                }
                .into());
            }
        }
        if let Some(map) = options.v_align_overrides {
            if let Some(name) = first_unknown(&first, map.keys().map(String::as_str)) {
                return Err(ErrorKind::UnknownField {
                    field_name: name.to_string(),
                }
                .into());
            }
        }
    }
    if let Some(map) = options.font_size_overrides {
        if options.missing == MissingName::Error {
            if let Some(name) = first_unknown(&first, map.keys().map(String::as_str)) {
                return Err(ErrorKind::UnknownField {
                    field_name: name.to_string(),
                }
                .into());
            }
        }
        for (name, size) in map {
            if first.contains_key(name.as_str()) {
                check_font_size_named(*size, Some(name.clone()))?;
            }
        }
    }
    if let Some(map) = options.line_below {
        if options.missing == MissingName::Error {
            if let Some(name) = first_unknown(&first, map.keys().map(String::as_str)) {
                return Err(ErrorKind::UnknownField {
                    field_name: name.to_string(),
                }
                .into());
            }
        }
        for (name, offset) in map {
            if first.contains_key(name.as_str()) && !offset.is_finite() {
                return Err(ErrorKind::InvalidArgument {
                    field_name: Some(name.clone()),
                    reason: ArgumentReason::NonFinite { value: *offset },
                }
                .into());
            }
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

fn first_unknown<'a>(
    known: &BTreeMap<&str, &WidgetSnap>,
    names: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let mut unknown: Vec<&str> = names.filter(|name| !known.contains_key(name)).collect();
    unknown.sort_unstable();
    unknown.into_iter().next()
}

fn h_align_for(options: &FillOptions<'_>, name: &str) -> HAlign {
    options
        .h_align_overrides
        .and_then(|map| map.get(name).copied())
        .unwrap_or(options.h_align)
}

fn v_align_for(options: &FillOptions<'_>, name: &str) -> VAlign {
    options
        .v_align_overrides
        .and_then(|map| map.get(name).copied())
        .unwrap_or(options.v_align)
}

fn font_size_for(options: &FillOptions<'_>, name: &str) -> f32 {
    options
        .font_size_overrides
        .and_then(|map| map.get(name).copied())
        .unwrap_or(options.font_size)
}

fn line_below_for(options: &FillOptions<'_>, name: &str) -> Option<f32> {
    options.line_below.and_then(|map| map.get(name).copied())
}

fn strokes_for(lines: &[AbsoluteLine]) -> Result<Vec<Stroke>> {
    let mut strokes = Vec::with_capacity(lines.len());
    for line in lines {
        for value in [
            line.x1,
            line.y1,
            line.x2,
            line.y2,
            line.stroke_width,
            line.red,
            line.green,
            line.blue,
        ] {
            if !value.is_finite() {
                return Err(ErrorKind::InvalidArgument {
                    field_name: None,
                    reason: ArgumentReason::NonFinite { value },
                }
                .into());
            }
        }
        strokes.push(Stroke {
            page_index: line.page_index,
            x1: line.x1,
            y1: line.y1,
            x2: line.x2,
            y2: line.y2,
            width: line.stroke_width,
            red: line.red,
            green: line.green,
            blue: line.blue,
        });
    }
    Ok(strokes)
}

fn check_font_size(size: f32) -> Result<()> {
    check_font_size_named(size, None)
}

fn check_font_size_named(size: f32, field_name: Option<String>) -> Result<()> {
    if !size.is_finite() || size <= 0.0 {
        return Err(bad_font_size_named(size, field_name));
    }
    let mp = (f64::from(size) * 1000.0).round();
    #[expect(
        clippy::cast_precision_loss,
        reason = "比较的是毫点能否放进 i64，不是字号精度"
    )]
    if !mp.is_finite() || mp > i64::MAX as f64 {
        return Err(bad_font_size_named(size, field_name));
    }
    #[expect(clippy::cast_precision_loss, reason = "256_000 在 f64 里是精确值")]
    if mp > layout::MAX_START_MP as f64 {
        return Err(bad_font_size_named(size, field_name.clone()));
    }
    Ok(())
}

fn bad_font_size_named(value: f32, field_name: Option<String>) -> Error {
    ErrorKind::InvalidArgument {
        field_name,
        reason: ArgumentReason::FontSize { value },
    }
    .into()
}
