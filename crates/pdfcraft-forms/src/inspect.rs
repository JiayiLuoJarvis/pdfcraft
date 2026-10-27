//! Reads `AcroForm` widgets and PDF structure helpers.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::{ArgumentReason, Error, ErrorKind, FlattenReason, Result};
use crate::fields::{FieldInfo, FieldType, FieldValue};

const MAX_DEREF: usize = 8;
const MAX_PARENT: usize = 32;
/// How deep arrays, dictionaries, and stream dictionaries may nest inside one object.
const MAX_INLINE_DEPTH: usize = 64;

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

    fn parse(obj: &Object) -> Option<Self> {
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
        Some(Self::new(n[0], n[1], n[2], n[3]))
    }
}

pub(crate) struct PdfDoc(Document);

enum Lookup {
    Found(Object),
    Missing,
}

pub(crate) enum Draw<'a> {
    Text(&'a str),
    Check,
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

    fn from_annot(doc: &PdfDoc, page_index: usize, annot: &Object) -> Option<Self> {
        let Lookup::Found(Object::Dictionary(dict)) = doc.lookup(annot.clone()) else {
            return None;
        };
        let is_widget = dict.get(b"Subtype").is_ok_and(|subtype| {
            matches!(
                doc.lookup(subtype.clone()),
                Lookup::Found(found) if name_str(&found) == Some("Widget")
            )
        });
        if !is_widget {
            return None;
        }
        let name = doc
            .inherited(&dict, b"T")
            .and_then(|obj| lopdf::decode_text_string(&obj).ok())?;
        let rect = doc
            .inherited(&dict, b"Rect")
            .and_then(|obj| PdfRect::parse(&obj))?;
        let ft = doc
            .inherited(&dict, b"FT")
            .and_then(|obj| name_str(&obj).map(str::to_owned));
        let flags = doc.inherited(&dict, b"Ff").map_or(0, |obj| flags_of(&obj));
        let field_type = field_type_of(ft.as_deref(), flags);
        let options = doc
            .inherited(&dict, b"Opt")
            .map_or_else(Vec::new, |obj| doc.read_options(&obj));
        Some(Self {
            name,
            field_type,
            page_index,
            rect,
            options,
        })
    }

    pub(crate) fn accept<'a>(&self, value: &'a FieldValue) -> Result<Option<Draw<'a>>> {
        match self.field_type {
            FieldType::Signature => Err(ErrorKind::SignatureNotWritable {
                field_name: self.name.clone(),
            }
            .into()),
            FieldType::RadioButton | FieldType::PushButton => Err(ErrorKind::WidgetUnsupported {
                field_name: self.name.clone(),
            }
            .into()),
            FieldType::CheckBox => match value {
                FieldValue::CheckBox(true) => Ok(Some(Draw::Check)),
                FieldValue::CheckBox(false) => Ok(None),
                FieldValue::Text(_) => Err(ErrorKind::InvalidCheckbox {
                    field_name: self.name.clone(),
                }
                .into()),
            },
            FieldType::Text => match value {
                FieldValue::Text(text) if text.is_empty() => Ok(None),
                FieldValue::Text(text) => Ok(Some(Draw::Text(text))),
                FieldValue::CheckBox(_) => Err(ErrorKind::InvalidArgument {
                    field_name: Some(self.name.clone()),
                    reason: ArgumentReason::CheckboxOnText,
                }
                .into()),
            },
            FieldType::ListBox | FieldType::ComboBox => match value {
                FieldValue::Text(text) => {
                    if !self.options.iter().any(|option| option == text) {
                        return Err(ErrorKind::IllegalChoice {
                            field_name: self.name.clone(),
                        }
                        .into());
                    }
                    if text.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(Draw::Text(text)))
                    }
                }
                FieldValue::CheckBox(_) => Err(ErrorKind::InvalidArgument {
                    field_name: Some(self.name.clone()),
                    reason: ArgumentReason::CheckboxOnChoice,
                }
                .into()),
            },
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
/// Returns [`ErrorKind::NotAPdf`] when the bytes are not a PDF or the catalog
/// cannot be read, [`ErrorKind::Encrypted`] when the file is encrypted, and
/// [`ErrorKind::HasXfa`] when `/XFA` is present. The lopdf error behind a
/// malformed PDF stays reachable through [`std::error::Error::source`].
pub fn list_fields(pdf: &[u8]) -> Result<Vec<FieldInfo>> {
    let doc = PdfDoc::open(pdf)?;
    Ok(doc.widgets().iter().map(WidgetSnap::info).collect())
}

impl PdfDoc {
    pub(crate) fn open(pdf: &[u8]) -> Result<Self> {
        let doc = Document::load_mem(pdf).map_err(|err| match err {
            lopdf::Error::Decryption(_) => Error::from(ErrorKind::Encrypted).with_source(err),
            other => Error::from(ErrorKind::NotAPdf).with_source(other),
        })?;
        if doc.is_encrypted() {
            return Err(ErrorKind::Encrypted.into());
        }
        let doc = Self(doc);
        if doc.has_xfa()? {
            return Err(ErrorKind::HasXfa.into());
        }
        Ok(doc)
    }

    pub(crate) fn widgets(&self) -> Vec<WidgetSnap> {
        let mut found = Vec::new();
        for (page_index, page_id) in self.0.page_iter().enumerate() {
            for annot in self.page_annots(page_id) {
                let Some(snap) = WidgetSnap::from_annot(self, page_index, &annot) else {
                    continue;
                };
                found.push(snap);
            }
        }
        found
    }

    pub(crate) fn save(mut self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.0.save_to(&mut bytes).map_err(|err| {
            Error::from(ErrorKind::FlattenFailed {
                reason: FlattenReason::Write,
            })
            .with_source(err)
        })?;
        Ok(bytes)
    }

    pub(crate) fn add_object(&mut self, object: Object) -> ObjectId {
        self.0.add_object(object)
    }

    pub(crate) fn page_ids(&self) -> Vec<ObjectId> {
        self.0.page_iter().collect()
    }

    pub(crate) fn add_page_contents(&mut self, page: ObjectId, bytes: Vec<u8>) -> Result<()> {
        self.0.add_page_contents(page, bytes).map_err(|err| {
            Error::from(ErrorKind::FlattenFailed {
                reason: FlattenReason::AppendContent,
            })
            .with_source(err)
        })
    }

    fn page_annots(&self, page_id: ObjectId) -> Vec<Object> {
        let Ok(page) = self.0.get_dictionary(page_id) else {
            return Vec::new();
        };
        let Ok(annots) = page.get(b"Annots") else {
            return Vec::new();
        };
        match self.lookup(annots.clone()) {
            Lookup::Found(Object::Array(items)) => items,
            _ => Vec::new(),
        }
    }

    fn inherited(&self, inline: &Dictionary, key: &[u8]) -> Option<Object> {
        if let Ok(value) = inline.get(key) {
            return match self.lookup(value.clone()) {
                Lookup::Found(obj) => Some(obj),
                Lookup::Missing => None,
            };
        }
        let mut id = inline.get(b"Parent").and_then(Object::as_reference).ok()?;
        for _ in 0..MAX_PARENT {
            let step = {
                let dict = self.0.get_dictionary(id).ok()?;
                if let Ok(value) = dict.get(key) {
                    match self.lookup(value.clone()) {
                        Lookup::Found(obj) => Step::Found(obj),
                        Lookup::Missing => Step::Stop,
                    }
                } else if let Ok(parent) = dict.get(b"Parent").and_then(Object::as_reference) {
                    Step::Parent(parent)
                } else {
                    Step::Stop
                }
            };
            match step {
                Step::Found(obj) => return Some(obj),
                Step::Parent(parent) => id = parent,
                Step::Stop => return None,
            }
        }
        None
    }

    fn lookup(&self, mut obj: Object) -> Lookup {
        for _ in 0..MAX_DEREF {
            let Object::Reference(id) = obj else {
                return Lookup::Found(obj);
            };
            match self.0.get_object(id) {
                Ok(next) => obj = next.clone(),
                Err(_) => return Lookup::Missing,
            }
        }
        if matches!(obj, Object::Reference(_)) {
            Lookup::Missing
        } else {
            Lookup::Found(obj)
        }
    }

    pub(crate) fn attach_font(&mut self, page_id: ObjectId, font_id: ObjectId) -> Result<String> {
        let mut resources = self.copied_resources(page_id)?;
        let mut fonts = match resources.get(b"Font").ok().cloned() {
            Some(obj) => match self.lookup(obj) {
                Lookup::Found(Object::Dictionary(dict)) => dict,
                _ => Dictionary::new(),
            },
            None => Dictionary::new(),
        };
        let resource_name = fresh_font_name(&fonts)?;
        fonts.set(resource_name.as_str(), Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(fonts));
        let resources_id = self.add_object(Object::Dictionary(resources));
        let page = self
            .0
            .get_object_mut(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(|err| {
                Error::from(ErrorKind::FlattenFailed {
                    reason: FlattenReason::UpdateResources,
                })
                .with_source(err)
            })?;
        page.set("Resources", Object::Reference(resources_id));
        Ok(resource_name)
    }

    pub(crate) fn strip_form(&mut self) -> Result<()> {
        if let Ok(catalog) = self.0.catalog_mut() {
            catalog.remove(b"AcroForm");
        }
        let pages = self.page_ids();
        for page_id in pages {
            let kept = {
                let page = self.0.get_dictionary(page_id).map_err(|err| {
                    Error::from(ErrorKind::FlattenFailed {
                        reason: FlattenReason::ReadPage,
                    })
                    .with_source(err)
                })?;
                match page.get(b"Annots").ok().cloned() {
                    Some(annots) => self.filter_annots(annots),
                    None => None,
                }
            };
            let Some(kept) = kept else {
                continue;
            };
            let page = self
                .0
                .get_object_mut(page_id)
                .and_then(Object::as_dict_mut)
                .map_err(|err| {
                    Error::from(ErrorKind::FlattenFailed {
                        reason: FlattenReason::UpdateAnnotations,
                    })
                    .with_source(err)
                })?;
            if kept.is_empty() {
                page.remove(b"Annots");
            } else {
                page.set("Annots", Object::Array(kept));
            }
        }
        Ok(())
    }

    /// Roots are `Root` and `Info`. `save_to` writes the rest of the trailer as-is, apart from `Size`.
    ///
    /// lopdf 0.45 already drops `/Prev` while reading. This still removes `/Prev`
    /// and `/XRefStm` so a trailer that kept them is not written back.
    pub(crate) fn prune(&mut self) -> Result<()> {
        let reachable = self.reachable()?;
        let keys: Vec<Vec<u8>> = self.0.trailer.iter().map(|(key, _)| key.clone()).collect();
        for key in keys {
            if key.as_slice() == b"Prev" || key.as_slice() == b"XRefStm" {
                self.0.trailer.remove(key.as_slice());
                continue;
            }
            if key.as_slice() == b"Root" || key.as_slice() == b"Info" {
                continue;
            }
            let dangling = match self.0.trailer.get(key.as_slice()) {
                Ok(value) => refs_dangle(value, &reachable)?,
                Err(_) => false,
            };
            if dangling {
                self.0.trailer.remove(key.as_slice());
            }
        }
        self.0.objects.retain(|id, _| reachable.contains(id));
        Ok(())
    }

    fn has_xfa(&self) -> Result<bool> {
        let catalog = self
            .0
            .catalog()
            .map_err(|err| Error::from(ErrorKind::NotAPdf).with_source(err))?;
        let Ok(acro) = catalog.get(b"AcroForm") else {
            return Ok(false);
        };
        match self.lookup(acro.clone()) {
            Lookup::Found(Object::Dictionary(acro)) => Ok(acro.has(b"XFA")),
            _ => Ok(false),
        }
    }

    fn copied_resources(&self, page_id: ObjectId) -> Result<Dictionary> {
        let mut id = page_id;
        for _ in 0..MAX_PARENT {
            let found = {
                let dict = self.0.get_dictionary(id).map_err(|err| {
                    Error::from(ErrorKind::FlattenFailed {
                        reason: FlattenReason::ReadPage,
                    })
                    .with_source(err)
                })?;
                let resources = dict.get(b"Resources").ok().cloned();
                let parent = dict.get(b"Parent").and_then(Object::as_reference).ok();
                (resources, parent)
            };
            if let Some(resources) = found.0 {
                return match self.lookup(resources) {
                    Lookup::Found(Object::Dictionary(dict)) => Ok(dict),
                    _ => Err(ErrorKind::FlattenFailed {
                        reason: FlattenReason::ResourcesNotDictionary,
                    }
                    .into()),
                };
            }
            let Some(parent) = found.1 else {
                break;
            };
            id = parent;
        }
        Ok(Dictionary::new())
    }

    fn filter_annots(&self, annots: Object) -> Option<Vec<Object>> {
        let Lookup::Found(Object::Array(items)) = self.lookup(annots) else {
            return None;
        };
        Some(
            items
                .into_iter()
                .filter(|annot| self.keep_annot(annot))
                .collect(),
        )
    }

    fn keep_annot(&self, annot: &Object) -> bool {
        let Lookup::Found(Object::Dictionary(dict)) = self.lookup(annot.clone()) else {
            return true;
        };
        dict.get(b"Subtype").ok().and_then(name_str) != Some("Widget")
    }

    fn read_options(&self, obj: &Object) -> Vec<String> {
        let Lookup::Found(Object::Array(items)) = self.lookup(obj.clone()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for item in items {
            match self.lookup(item) {
                Lookup::Found(Object::Array(pair)) => {
                    for part in pair {
                        if let Lookup::Found(obj) = self.lookup(part) {
                            push_option(&mut out, &obj);
                        }
                    }
                }
                Lookup::Found(other) => push_option(&mut out, &other),
                Lookup::Missing => {}
            }
        }
        out
    }

    fn reachable(&self) -> Result<BTreeSet<ObjectId>> {
        let mut stack = Vec::new();
        for key in [b"Root".as_slice(), b"Info".as_slice()] {
            if let Ok(obj) = self.0.trailer.get(key) {
                collect_refs(obj, &mut stack)?;
            }
        }
        let mut seen = BTreeSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(obj) = self.0.objects.get(&id) {
                collect_refs(obj, &mut stack)?;
            }
        }
        Ok(seen)
    }
}

enum Step {
    Found(Object),
    Parent(ObjectId),
    Stop,
}

fn fresh_font_name(fonts: &Dictionary) -> Result<String> {
    for n in 0..64 {
        let candidate = if n == 0 {
            "QuireForms".to_string()
        } else {
            format!("QuireForms{}", n + 1)
        };
        if !fonts.has(candidate.as_bytes()) {
            return Ok(candidate);
        }
    }
    Err(ErrorKind::FlattenFailed {
        reason: FlattenReason::FontNameTaken,
    }
    .into())
}

// lopdf 0.45 returns name bytes. `/FT` and `/Subtype` in this crate are ASCII.
fn name_str(obj: &Object) -> Option<&str> {
    std::str::from_utf8(obj.as_name().ok()?).ok()
}

fn flags_of(obj: &Object) -> i64 {
    obj.as_i64().unwrap_or(0)
}

fn field_type_of(ft: Option<&str>, flags: i64) -> FieldType {
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

fn ordered(a: f32, b: f32) -> (f32, f32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn push_option(out: &mut Vec<String>, obj: &Object) {
    let Ok(text) = lopdf::decode_text_string(obj) else {
        return;
    };
    if !out.contains(&text) {
        out.push(text);
    }
}

fn collect_refs(obj: &Object, out: &mut Vec<ObjectId>) -> Result<()> {
    let mut stack = vec![(obj, 0_usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > MAX_INLINE_DEPTH {
            return Err(ErrorKind::FlattenFailed {
                reason: FlattenReason::NestingTooDeep { depth },
            }
            .into());
        }
        match current {
            Object::Reference(id) => out.push(*id),
            Object::Array(items) => {
                for item in items {
                    stack.push((item, depth + 1));
                }
            }
            Object::Dictionary(dict) => {
                for (_, value) in dict {
                    stack.push((value, depth + 1));
                }
            }
            Object::Stream(stream) => {
                for (_, value) in &stream.dict {
                    stack.push((value, depth + 1));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn refs_dangle(obj: &Object, seen: &BTreeSet<ObjectId>) -> Result<bool> {
    let mut stack = vec![(obj, 0_usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > MAX_INLINE_DEPTH {
            return Err(ErrorKind::FlattenFailed {
                reason: FlattenReason::NestingTooDeep { depth },
            }
            .into());
        }
        match current {
            Object::Reference(id) => {
                if !seen.contains(id) {
                    return Ok(true);
                }
            }
            Object::Array(items) => {
                for item in items {
                    stack.push((item, depth + 1));
                }
            }
            Object::Dictionary(dict) => {
                for (_, value) in dict {
                    stack.push((value, depth + 1));
                }
            }
            Object::Stream(stream) => {
                for (_, value) in &stream.dict {
                    stack.push((value, depth + 1));
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::collect_refs;
    use crate::error::{ErrorKind, FlattenReason};
    use lopdf::Object;

    #[test]
    fn rejects_object_nesting_past_the_cap() {
        let mut obj = Object::Integer(1);
        for _ in 0..70 {
            obj = Object::Array(vec![obj]);
        }
        let err = collect_refs(&obj, &mut Vec::new()).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::FlattenFailed {
                reason: FlattenReason::NestingTooDeep { depth: 65 },
            }
        );
    }

    #[test]
    fn collects_a_reference_inside_one_array() {
        let obj = Object::Array(vec![Object::Reference((1, 0))]);
        let mut refs = Vec::new();
        collect_refs(&obj, &mut refs).unwrap();
        assert_eq!(refs, vec![(1, 0)]);
    }
}
