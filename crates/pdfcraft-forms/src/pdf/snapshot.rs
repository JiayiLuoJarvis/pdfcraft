//! Widget snapshots taken while the PDF is opened.

use lopdf::Object;

use crate::geom::PdfRect;
use crate::request::{FieldInfo, FieldType};

use super::read::Lookup;
use super::PdfDoc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WidgetSnap {
    pub(crate) rect: PdfRect,
}

impl WidgetSnap {
    pub(super) fn from_annot(
        doc: &PdfDoc,
        page_index: usize,
        annot: &Object,
    ) -> Option<(Self, FieldInfo)> {
        let Lookup::Found(Object::Dictionary(dict)) = doc.lookup(annot.clone()) else {
            return None;
        };
        let is_widget = dict.get(b"Subtype").is_ok_and(|subtype| {
            matches!(
                doc.lookup(subtype.clone()),
                Lookup::Found(found) if super::read::name_str(&found) == Some("Widget")
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
            .and_then(|obj| parse_rect(&obj))?;
        let ft = doc
            .inherited(&dict, b"FT")
            .and_then(|obj| super::read::name_str(&obj).map(str::to_owned));
        let flags = doc
            .inherited(&dict, b"Ff")
            .map_or(0, |obj| super::read::flags_of(&obj));
        let field_type = super::read::field_type_of(ft.as_deref(), flags);
        let options = doc
            .inherited(&dict, b"Opt")
            .map_or_else(Vec::new, |obj| doc.read_options(&obj));
        let stored = match field_type {
            FieldType::ListBox | FieldType::ComboBox => Some(options),
            FieldType::Text
            | FieldType::CheckBox
            | FieldType::Signature
            | FieldType::RadioButton
            | FieldType::PushButton => None,
        };
        Some((
            Self { rect },
            FieldInfo::new(name, field_type, page_index, stored),
        ))
    }
}

fn parse_rect(obj: &Object) -> Option<PdfRect> {
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
