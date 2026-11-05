//! Open a PDF and list its widgets.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::{Error, ErrorKind, Result};
use crate::request::{FieldInfo, FieldType};

use super::snapshot::WidgetSnap;
use super::PdfDoc;

const MAX_DEREF: usize = 8;
const MAX_PARENT: usize = 32;

pub(super) enum Lookup {
    Found(Object),
    Missing,
}

fn open_error(err: lopdf::Error) -> Error {
    match err {
        err @ lopdf::Error::Decryption(_) => Error::new(ErrorKind::Encrypted).with_source(err),
        err @ (lopdf::Error::Unimplemented(_)
        | lopdf::Error::ObjectType { .. }
        | lopdf::Error::DictType { .. }
        | lopdf::Error::AlreadyEncrypted
        | lopdf::Error::CharacterEncoding
        | lopdf::Error::Decompress(_)
        | lopdf::Error::Parse(_)
        | lopdf::Error::DictKey(_)
        | lopdf::Error::InvalidInlineImage(_)
        | lopdf::Error::InvalidOutline(_)
        | lopdf::Error::InvalidStream(_)
        | lopdf::Error::InvalidObjectStream(_)
        | lopdf::Error::InvalidOffset(_)
        | lopdf::Error::IO(_)
        | lopdf::Error::NoOutline
        | lopdf::Error::NotEncrypted
        | lopdf::Error::InvalidPassword
        | lopdf::Error::MissingXrefEntry
        | lopdf::Error::ObjectNotFound(_)
        | lopdf::Error::ReferenceCycle(_)
        | lopdf::Error::PageNumberNotFound(_)
        | lopdf::Error::NumericCast(_)
        | lopdf::Error::ReferenceLimit
        | lopdf::Error::RecursionLimit
        | lopdf::Error::TextStringDecode
        | lopdf::Error::Xref(_)
        | lopdf::Error::IndirectObject { .. }
        | lopdf::Error::ObjectIdMismatch
        | lopdf::Error::Syntax(_)
        | lopdf::Error::ToUnicodeCMap(_)
        | lopdf::Error::TryFromInt(_)
        | lopdf::Error::UnsupportedSecurityHandler(_)
        | lopdf::Error::InvalidEncodingDifferenceCode { .. }
        | lopdf::Error::InvalidEncodingDifferenceGlyph { .. }) => {
            Error::new(ErrorKind::NotAPdf).with_source(err)
        }
    }
}

impl PdfDoc {
    pub(crate) fn open(pdf: &[u8]) -> Result<Self> {
        let doc = Document::load_mem(pdf).map_err(open_error)?;
        if doc.is_encrypted() {
            return Err(Error::new(ErrorKind::Encrypted));
        }
        let doc = Self(doc);
        if doc.has_xfa()? {
            return Err(Error::new(ErrorKind::HasXfa));
        }
        Ok(doc)
    }

    pub(crate) fn page_count(&self) -> usize {
        self.0.page_iter().count()
    }

    pub(crate) fn widgets(&self) -> (Vec<WidgetSnap>, Vec<FieldInfo>) {
        let mut rects = Vec::new();
        let mut fields = Vec::new();
        for (page_index, page_id) in self.0.page_iter().enumerate() {
            for annot in self.page_annots(page_id) {
                let Some((snap, info)) = WidgetSnap::from_annot(self, page_index, &annot) else {
                    continue;
                };
                rects.push(snap);
                fields.push(info);
            }
        }
        (rects, fields)
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
            Lookup::Found(
                Object::Null
                | Object::Boolean(_)
                | Object::Integer(_)
                | Object::Real(_)
                | Object::Name(_)
                | Object::String(_, _)
                | Object::Dictionary(_)
                | Object::Stream(_)
                | Object::Reference(_),
            )
            | Lookup::Missing => Vec::new(),
        }
    }
    pub(super) fn inherited(&self, inline: &Dictionary, key: &[u8]) -> Option<Object> {
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
    pub(super) fn lookup(&self, mut obj: Object) -> Lookup {
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
    fn has_xfa(&self) -> Result<bool> {
        let catalog = self
            .0
            .catalog()
            .map_err(|err| Error::new(ErrorKind::NotAPdf).with_source(err))?;
        let Ok(acro) = catalog.get(b"AcroForm") else {
            return Ok(false);
        };
        match self.lookup(acro.clone()) {
            Lookup::Found(Object::Dictionary(acro)) => Ok(acro.has(b"XFA")),
            Lookup::Found(
                Object::Null
                | Object::Boolean(_)
                | Object::Integer(_)
                | Object::Real(_)
                | Object::Name(_)
                | Object::String(_, _)
                | Object::Array(_)
                | Object::Stream(_)
                | Object::Reference(_),
            )
            | Lookup::Missing => Ok(false),
        }
    }
    pub(super) fn read_options(&self, obj: &Object) -> Vec<String> {
        let Lookup::Found(Object::Array(items)) = self.lookup(obj.clone()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for item in items {
            match self.lookup(item) {
                Lookup::Found(Object::Array(pair)) => {
                    for part in pair {
                        if let Lookup::Found(found) = self.lookup(part) {
                            push_option(&mut out, &found);
                        }
                    }
                }
                Lookup::Found(other) => push_option(&mut out, &other),
                Lookup::Missing => {}
            }
        }
        out
    }
}

enum Step {
    Found(Object),
    Parent(ObjectId),
    Stop,
}

pub(super) fn name_str(obj: &Object) -> Option<&str> {
    std::str::from_utf8(obj.as_name().ok()?).ok()
}

pub(super) fn flags_of(obj: &Object) -> i64 {
    obj.as_i64().unwrap_or(0)
}

pub(super) fn field_type_of(ft: Option<&str>, flags: i64) -> FieldType {
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

fn push_option(out: &mut Vec<String>, obj: &Object) {
    let Ok(text) = lopdf::decode_text_string(obj) else {
        return;
    };
    if !out.contains(&text) {
        out.push(text);
    }
}
