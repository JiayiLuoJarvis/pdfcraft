use lopdf::{Document, Object};

use crate::error::{Error, ErrorKind, Result};
use crate::fields::{FieldInfo, FieldType};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PdfRect {
    pub(crate) x0: f32,
    pub(crate) y0: f32,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
}

impl PdfRect {
    pub(crate) fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        let (x0, x1) = ordered(x0, x1);
        let (y0, y1) = ordered(y0, y1);
        Self { x0, y0, x1, y1 }
    }

    pub(crate) fn width(self) -> f32 {
        self.x1 - self.x0
    }

    pub(crate) fn height(self) -> f32 {
        self.y1 - self.y0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WidgetSnap {
    pub(crate) name: String,
    pub(crate) field_type: FieldType,
    pub(crate) page_index: usize,
    pub(crate) rect: PdfRect,
    pub(crate) options: Vec<String>,
}

impl WidgetSnap {
    pub(crate) fn info(&self) -> FieldInfo {
        let options = match self.field_type {
            FieldType::ListBox | FieldType::ComboBox => Some(self.options.clone()),
            _ => None,
        };
        FieldInfo {
            name: self.name.clone(),
            field_type: self.field_type,
            page_index: self.page_index,
            options,
        }
    }
}

/// Lists `AcroForm` widgets in `pdf`.
///
/// Each widget on a page becomes one [`FieldInfo`]. Widgets that have no `/T`
/// or no `/Rect` are skipped. An encrypted PDF fails. An `AcroForm` dictionary
/// that contains `/XFA` fails, including when the value is an empty array.
///
/// # Errors
///
/// Returns [`ErrorKind::NotAPdf`] when the bytes are not a PDF, [`ErrorKind::Encrypted`]
/// when the file is encrypted, and [`ErrorKind::HasXfa`] when `/XFA` is present.
pub fn list_fields(pdf: &[u8]) -> Result<Vec<FieldInfo>> {
    let doc = open(pdf)?;
    Ok(widgets(&doc).iter().map(WidgetSnap::info).collect())
}

pub(crate) fn open(pdf: &[u8]) -> Result<Document> {
    let doc = Document::load_mem(pdf).map_err(|err| match err {
        lopdf::Error::Decryption(_) => Error::from(ErrorKind::Encrypted).with_source(err),
        other => Error::from(ErrorKind::NotAPdf).with_source(other),
    })?;
    if doc.is_encrypted() {
        return Err(ErrorKind::Encrypted.into());
    }
    if has_xfa(&doc)? {
        return Err(ErrorKind::HasXfa.into());
    }
    Ok(doc)
}

pub(crate) fn widgets(doc: &Document) -> Vec<WidgetSnap> {
    let mut found = Vec::new();
    for (page_index, page_id) in doc.page_iter().enumerate() {
        for annot in page_annots(doc, page_id) {
            let Some(snap) = widget_snap(doc, page_index, &annot) else {
                continue;
            };
            found.push(snap);
        }
    }
    found
}

fn has_xfa(doc: &Document) -> Result<bool> {
    let catalog = doc
        .catalog()
        .map_err(|err| Error::from(ErrorKind::NotAPdf).with_source(err))?;
    let Ok(acro) = catalog.get(b"AcroForm") else {
        return Ok(false);
    };
    let Object::Dictionary(acro) = deep_deref(doc, acro.clone()) else {
        return Ok(false);
    };
    Ok(acro.has(b"XFA"))
}

fn page_annots(doc: &Document, page_id: lopdf::ObjectId) -> Vec<Object> {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    let Ok(annots) = page.get(b"Annots") else {
        return Vec::new();
    };
    match deep_deref(doc, annots.clone()) {
        Object::Array(items) => items,
        _ => Vec::new(),
    }
}

fn widget_snap(doc: &Document, page_index: usize, annot: &Object) -> Option<WidgetSnap> {
    let dict = annot_dict(doc, annot)?;
    let subtype = dict.get(b"Subtype").ok().cloned();
    let subtype = subtype.map(|obj| deep_deref(doc, obj));
    if subtype.as_ref().and_then(|obj| obj.as_name_str().ok()) != Some("Widget") {
        return None;
    }
    let name = inherited(doc, &dict, b"T").and_then(|obj| lopdf::decode_text_string(&obj).ok())?;
    let rect = inherited(doc, &dict, b"Rect").and_then(|obj| read_rect(&obj))?;
    let ft =
        inherited(doc, &dict, b"FT").and_then(|obj| obj.as_name_str().ok().map(str::to_string));
    let flags = inherited(doc, &dict, b"Ff").map_or(0, |obj| flags_of(&obj));
    let field_type = classify(ft.as_deref(), flags);
    let options =
        inherited(doc, &dict, b"Opt").map_or_else(Vec::new, |obj| read_options(doc, &obj));
    Some(WidgetSnap {
        name,
        field_type,
        page_index,
        rect,
        options,
    })
}

fn annot_dict(doc: &Document, annot: &Object) -> Option<lopdf::Dictionary> {
    match deep_deref(doc, annot.clone()) {
        Object::Dictionary(dict) => Some(dict),
        _ => None,
    }
}

fn inherited(doc: &Document, inline: &lopdf::Dictionary, key: &[u8]) -> Option<Object> {
    if let Some(value) = inline.get(key).ok().cloned() {
        return Some(deep_deref(doc, value));
    }
    let mut id = inline.get(b"Parent").and_then(Object::as_reference).ok()?;
    for _ in 0..32 {
        let step = {
            let dict = doc.get_dictionary(id).ok()?;
            if let Some(value) = dict.get(key).ok().cloned() {
                Step::Found(value)
            } else if let Ok(parent) = dict.get(b"Parent").and_then(Object::as_reference) {
                Step::Parent(parent)
            } else {
                Step::Stop
            }
        };
        match step {
            Step::Found(value) => return Some(deep_deref(doc, value)),
            Step::Parent(parent) => id = parent,
            Step::Stop => return None,
        }
    }
    None
}

enum Step {
    Found(Object),
    Parent(lopdf::ObjectId),
    Stop,
}

pub(crate) fn deep_deref(doc: &Document, mut obj: Object) -> Object {
    for _ in 0..8 {
        let Object::Reference(id) = obj else {
            break;
        };
        obj = doc.get_object(id).cloned().unwrap_or(Object::Null);
    }
    obj
}

fn flags_of(obj: &Object) -> i64 {
    obj.as_i64().unwrap_or(0)
}

fn classify(ft: Option<&str>, flags: i64) -> FieldType {
    const PUSH: i64 = 1 << 16;
    const RADIO: i64 = 1 << 15;
    const COMBO: i64 = 1 << 17;
    match ft {
        Some("Btn") if flags & PUSH != 0 => FieldType::PushButton,
        Some("Btn") if flags & RADIO != 0 => FieldType::RadioButton,
        Some("Btn") => FieldType::CheckBox,
        Some("Ch") if flags & COMBO != 0 => FieldType::ComboBox,
        Some("Ch") => FieldType::ListBox,
        Some("Sig") => FieldType::Signature,
        _ => FieldType::Text,
    }
}

fn read_rect(obj: &Object) -> Option<PdfRect> {
    let items = obj.as_array().ok()?;
    if items.len() < 4 {
        return None;
    }
    let mut n = [0.0_f32; 4];
    for (index, item) in items.iter().take(4).enumerate() {
        let value = item.as_float().ok()?;
        if !value.is_finite() {
            return None;
        }
        n[index] = value;
    }
    Some(PdfRect::new(n[0], n[1], n[2], n[3]))
}

fn ordered(a: f32, b: f32) -> (f32, f32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn read_options(doc: &Document, obj: &Object) -> Vec<String> {
    let Object::Array(items) = obj else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        match deep_deref(doc, item.clone()) {
            Object::Array(pair) => {
                for part in pair {
                    push_option(&mut out, &deep_deref(doc, part));
                }
            }
            other => push_option(&mut out, &other),
        }
    }
    out
}

fn push_option(out: &mut Vec<String>, obj: &Object) {
    let Ok(text) = lopdf::decode_text_string(obj) else {
        return;
    };
    if !out.contains(&text) {
        out.push(text);
    }
}
