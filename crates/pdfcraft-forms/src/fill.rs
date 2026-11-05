//! Opens a form, checks a fill request, and writes the flattened PDF.
//!
//! [`Form`] holds `Arc` of the parsed document, one rectangle per widget, and
//! an equal-length [`FieldInfo`] list. [`Fill::new`] clones that `Arc`, so a
//! [`Fill`] does not borrow the [`Form`]. [`Fill::to_pdf`] clones the document
//! and paints the copy.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::error::{self, ArgumentReason, Error, ErrorKind, FlattenReason, FontReason, Result};
use crate::font::{Face, Font};
use crate::layout::{self, LaidText};
use crate::paint::{paint, Mark, PaintFont, Stamp, Stroke};
use crate::pdf::{PdfDoc, WidgetSnap};
use crate::request::{
    AbsoluteLine, FieldInfo, FieldRule, FieldType, FieldValue, FillRequest, HAlign, MissingName,
    VAlign,
};

/// An opened PDF and its widget list.
///
/// [`Clone`] shares the parsed document. It does not copy the PDF object graph.
#[derive(Clone)]
#[must_use]
pub struct Form {
    doc: Arc<PdfDoc>,
    pages: usize,
    widgets: Vec<WidgetSnap>,
    fields: Vec<FieldInfo>,
}

impl Form {
    /// Opens `pdf`, lists widgets, and rejects encryption and `/XFA`.
    ///
    /// Widgets with no `/T` or no `/Rect` are skipped. An empty `/XFA` array
    /// still fails. This function does not read a font or a [`FillRequest`].
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::NotAPdf`] when the bytes are not a PDF or the catalog
    /// cannot be read, [`ErrorKind::Encrypted`] when the file is encrypted, and
    /// [`ErrorKind::HasXfa`] when `/XFA` is present. The lopdf error behind a
    /// malformed PDF stays reachable through [`std::error::Error::source`].
    pub fn open(pdf: &[u8]) -> Result<Self> {
        let doc = PdfDoc::open(pdf)?;
        let pages = doc.page_count();
        let (widgets, fields) = doc.widgets();
        Ok(Self {
            doc: Arc::new(doc),
            pages,
            widgets,
            fields,
        })
    }

    /// One [`FieldInfo`] per widget, in page order and then annotation order.
    pub fn fields(&self) -> &[FieldInfo] {
        &self.fields
    }
}

impl fmt::Debug for Form {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Form")
            .field("pages", &self.pages)
            .field("widgets", &self.fields.len())
            .finish_non_exhaustive()
    }
}

/// A checked fill, ready to embed and write.
///
/// [`Self::to_pdf`] does not change this value or the [`Form`] it was built from.
#[must_use]
pub struct Fill {
    doc: Arc<PdfDoc>,
    stamps: Vec<Stamp>,
    strokes: Vec<Stroke>,
    font: Option<Font>,
    used: BTreeMap<u16, (char, f32)>,
}

impl Fill {
    /// Checks `request` and lays out every text widget.
    ///
    /// `font` is read only when a non-empty string will be drawn. A draw with
    /// no text leaves the font unread and does not store it. The request is
    /// moved in. A failed call drops it. Retrying requires a clone the caller
    /// kept.
    ///
    /// # Errors
    ///
    /// Returns the first failure in this order:
    ///
    /// - [`ErrorKind::InvalidArgument`] for the document font size, a field font
    ///   size, a non-finite rule offset, a checkbox value on text or a choice,
    ///   a non-finite line, or a line whose page is outside the document.
    ///   [`Error::page`] is set only for [`ArgumentReason::PageOutOfRange`].
    /// - [`ErrorKind::UnknownField`] for a missing name when
    ///   [`MissingName::Error`] is set. Field names are checked before rule names.
    /// - [`ErrorKind::SignatureNotWritable`], [`ErrorKind::WidgetUnsupported`],
    ///   [`ErrorKind::InvalidCheckbox`], or [`ErrorKind::IllegalChoice`] for the
    ///   first widget of that name.
    /// - [`ErrorKind::FontNotConfigured`] with [`FontReason::Missing`] when text
    ///   is drawn and `font` is [`None`].
    /// - [`ErrorKind::MissingGlyph`], [`FontReason::NoAdvance`], or
    ///   [`ErrorKind::BoxOverflow`] while measuring text.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "失败时请求被丢弃；要重试须调用方先克隆"
    )]
    pub fn new(form: &Form, request: FillRequest, font: Option<&Font>) -> Result<Self> {
        let planned = compile_plan(form, &request)?;
        let needs_font = planned
            .values()
            .any(|intent| matches!(intent, Intent::Text { .. }));
        let stored_font = if needs_font {
            Some(
                font.ok_or_else(|| error::font_unusable(FontReason::Missing))?
                    .clone(),
            )
        } else {
            None
        };
        let mut used = BTreeMap::new();
        let mut stamps = Vec::new();
        if let Some(stored) = stored_font.as_ref() {
            let face = Face::load(stored.bytes(), 0)?;
            push_stamps(
                &form.widgets,
                &form.fields,
                &request,
                &planned,
                Some(&face),
                &mut used,
                &mut stamps,
            )?;
        } else {
            push_stamps(
                &form.widgets,
                &form.fields,
                &request,
                &planned,
                None,
                &mut used,
                &mut stamps,
            )?;
        }
        let strokes = request.lines.iter().map(stroke_of).collect();
        Ok(Self {
            doc: Arc::clone(&form.doc),
            stamps,
            strokes,
            font: stored_font,
            used,
        })
    }

    /// Embeds the subset, paints, strips the form, and writes PDF bytes.
    ///
    /// A second call on the same [`Fill`] writes the same bytes.
    /// [`Form::fields`] is left unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::FontNotConfigured`] when the subset or a width
    /// cannot be written, and [`ErrorKind::FlattenFailed`] when the page
    /// content, resources, or PDF write fails.
    pub fn to_pdf(&self) -> Result<Vec<u8>> {
        let mut doc = self.doc.duplicate();
        let embedded = self.font.as_ref().filter(|_| !self.used.is_empty());
        let paint_font = embedded.map(|font| PaintFont {
            bytes: font.bytes(),
            used: &self.used,
            upem: font.upem(),
            ascent: font.ascent(),
            descent: font.descent(),
        });
        paint(&mut doc, &self.stamps, &self.strokes, paint_font)?;
        doc.save()
    }
}

impl fmt::Debug for Fill {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Fill")
            .field("instructions", &self.stamps.len())
            .field("lines", &self.strokes.len())
            .field("font", &self.font.is_some())
            .finish_non_exhaustive()
    }
}

fn compile_plan<'a>(form: &'a Form, request: &'a FillRequest) -> Result<BTreeMap<&'a str, Intent>> {
    check_font_size(request.style.font_size, None)?;
    let known = first_widgets(&form.fields);
    if request.missing == MissingName::Error {
        if let Some(name) = first_unknown(&known, request.fields.keys()) {
            return Err(Error::new(ErrorKind::UnknownField).with_field(name));
        }
        if let Some(name) = first_unknown(&known, request.rules.keys()) {
            return Err(Error::new(ErrorKind::UnknownField).with_field(name));
        }
    }
    for (name, fill) in &request.fields {
        if known.contains_key(name.as_str()) {
            if let Some(size) = fill.style.font_size {
                check_font_size(size, Some(name))?;
            }
        }
    }
    for (name, rule) in &request.rules {
        if known.contains_key(name.as_str()) && !rule.offset.is_finite() {
            return Err(
                Error::new(ErrorKind::InvalidArgument(ArgumentReason::NonFinite {
                    value: rule.offset,
                }))
                .with_field(name.clone()),
            );
        }
    }
    let mut planned = BTreeMap::new();
    for (name, fill) in &request.fields {
        let Some(info) = known.get(name.as_str()) else {
            continue;
        };
        let Some(accepted) = accept(info, &fill.value)? else {
            continue;
        };
        let intent = match accepted {
            Accepted::Check => Intent::Check,
            Accepted::Text(text) => Intent::Text {
                text,
                font_size: fill.style.font_size.unwrap_or(request.style.font_size),
                h_align: fill.style.h_align.unwrap_or(request.style.h_align),
                v_align: fill.style.v_align.unwrap_or(request.style.v_align),
            },
        };
        planned.insert(name.as_str(), intent);
    }
    for line in &request.lines {
        for value in line_numbers(line) {
            if !value.is_finite() {
                return Err(Error::new(ErrorKind::InvalidArgument(
                    ArgumentReason::NonFinite { value },
                )));
            }
        }
    }
    for line in &request.lines {
        if line.page_index >= form.pages {
            return Err(
                Error::new(ErrorKind::InvalidArgument(ArgumentReason::PageOutOfRange))
                    .with_page(line.page_index),
            );
        }
    }
    Ok(planned)
}

enum Intent {
    Text {
        text: String,
        font_size: f32,
        h_align: HAlign,
        v_align: VAlign,
    },
    Check,
}

fn first_widgets(fields: &[FieldInfo]) -> BTreeMap<&str, &FieldInfo> {
    let mut known = BTreeMap::new();
    for info in fields {
        known.entry(info.name()).or_insert(info);
    }
    known
}

fn first_unknown<'a>(
    known: &BTreeMap<&str, &FieldInfo>,
    names: impl Iterator<Item = &'a String>,
) -> Option<&'a str> {
    names
        .map(String::as_str)
        .find(|name| !known.contains_key(name))
}

enum Accepted {
    Text(String),
    Check,
}

fn accept(info: &FieldInfo, value: &FieldValue) -> Result<Option<Accepted>> {
    let name = info.name();
    match info.field_type() {
        FieldType::Signature => Err(Error::new(ErrorKind::SignatureNotWritable).with_field(name)),
        FieldType::RadioButton | FieldType::PushButton => {
            Err(Error::new(ErrorKind::WidgetUnsupported).with_field(name))
        }
        FieldType::CheckBox => match value {
            FieldValue::CheckBox(true) => Ok(Some(Accepted::Check)),
            FieldValue::CheckBox(false) => Ok(None),
            FieldValue::Text(_) => Err(Error::new(ErrorKind::InvalidCheckbox).with_field(name)),
        },
        FieldType::Text => match value {
            FieldValue::Text(text) if text.is_empty() => Ok(None),
            FieldValue::Text(text) => Ok(Some(Accepted::Text(text.clone()))),
            FieldValue::CheckBox(_) => Err(Error::new(ErrorKind::InvalidArgument(
                ArgumentReason::CheckboxOnText,
            ))
            .with_field(name)),
        },
        FieldType::ListBox | FieldType::ComboBox => match value {
            FieldValue::Text(text) => {
                let options = info.options().unwrap_or(&[]);
                if !options.iter().any(|option| option == text) {
                    return Err(Error::new(ErrorKind::IllegalChoice).with_field(name));
                }
                if text.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(Accepted::Text(text.clone())))
                }
            }
            FieldValue::CheckBox(_) => Err(Error::new(ErrorKind::InvalidArgument(
                ArgumentReason::CheckboxOnChoice,
            ))
            .with_field(name)),
        },
    }
}

fn push_stamps(
    widgets: &[WidgetSnap],
    fields: &[FieldInfo],
    request: &FillRequest,
    planned: &BTreeMap<&str, Intent>,
    face: Option<&Face<'_>>,
    used: &mut BTreeMap<u16, (char, f32)>,
    stamps: &mut Vec<Stamp>,
) -> Result<()> {
    for (widget, info) in widgets.iter().zip(fields.iter()) {
        match planned.get(info.name()) {
            Some(Intent::Check) => stamps.push(Stamp {
                page_index: info.page_index(),
                rect: widget.rect,
                mark: Mark::Check,
            }),
            Some(Intent::Text {
                text,
                font_size,
                h_align,
                v_align,
            }) => {
                let Some(face) = face else {
                    return Err(Error::new(ErrorKind::FlattenFailed(
                        FlattenReason::ReadPage,
                    )));
                };
                let laid =
                    LaidText::fit(face, info.name(), text, widget.rect, *font_size, *v_align)?;
                for glyph in laid.glyphs() {
                    used.entry(glyph.id).or_insert((glyph.scalar, glyph.units));
                }
                stamps.push(Stamp {
                    page_index: info.page_index(),
                    rect: widget.rect,
                    mark: Mark::Text {
                        laid,
                        h_align: *h_align,
                    },
                });
            }
            None => {}
        }
        if let Some(rule) = request.rules.get(info.name()) {
            stamps.push(rule_stamp(info, widget, *rule));
        }
    }
    Ok(())
}

fn check_font_size(size: f32, field: Option<&str>) -> Result<()> {
    if !size.is_finite() || size <= 0.0 {
        return Err(bad_font_size(size, field));
    }
    let mp = (f64::from(size) * 1000.0).round();
    #[expect(
        clippy::cast_precision_loss,
        reason = "比较的是毫点能否放进 i64，不是字号精度"
    )]
    if !mp.is_finite() || mp > i64::MAX as f64 {
        return Err(bad_font_size(size, field));
    }
    #[expect(clippy::cast_precision_loss, reason = "256_000 在 f64 里是精确值")]
    if mp > layout::MAX_START_MP as f64 {
        return Err(bad_font_size(size, field));
    }
    Ok(())
}

fn bad_font_size(value: f32, field: Option<&str>) -> Error {
    let error = Error::new(ErrorKind::InvalidArgument(ArgumentReason::FontSize {
        value,
    }));
    match field {
        Some(field) => error.with_field(field),
        None => error,
    }
}

fn line_numbers(line: &AbsoluteLine) -> [f32; 8] {
    [
        line.x1,
        line.y1,
        line.x2,
        line.y2,
        line.style.width,
        line.style.red,
        line.style.green,
        line.style.blue,
    ]
}

fn stroke_of(line: &AbsoluteLine) -> Stroke {
    Stroke {
        page_index: line.page_index,
        x1: line.x1,
        y1: line.y1,
        x2: line.x2,
        y2: line.y2,
        width: line.style.width,
        red: line.style.red,
        green: line.style.green,
        blue: line.style.blue,
    }
}

fn rule_stamp(info: &FieldInfo, widget: &WidgetSnap, rule: FieldRule) -> Stamp {
    Stamp {
        page_index: info.page_index(),
        rect: widget.rect,
        mark: Mark::LineBelow {
            offset: rule.offset,
        },
    }
}
