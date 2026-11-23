//! Mutate the open document and write it back.

use std::collections::BTreeSet;

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::error::{Error, ErrorKind, FlattenReason, Result};

use super::read::Lookup;
use super::PdfDoc;

const MAX_PARENT: usize = 32;
/// How deep arrays, dictionaries, and stream dictionaries may nest inside one object.
const MAX_INLINE_DEPTH: usize = 64;

pub(crate) struct OverlayIds {
    pub(crate) font: Option<ObjectId>,
    pub(crate) opaque: ObjectId,
}

pub(crate) struct OverlayNames {
    pub(crate) font: Option<String>,
    pub(crate) opaque: String,
}

pub(crate) fn opaque_extgstate() -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", "ExtGState");
    dict.set("CA", 1);
    dict.set("ca", 1);
    dict.set("BM", "Normal");
    dict.set("SMask", "None");
    dict.set("AIS", false);
    dict.set("OP", false);
    dict.set("op", false);
    dict
}

impl PdfDoc {
    pub(crate) fn save(mut self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.0.save_to(&mut bytes).map_err(|err| {
            Error::new(ErrorKind::FlattenFailed(FlattenReason::Write)).with_source(err)
        })?;
        Ok(bytes)
    }

    pub(crate) fn add_object(&mut self, object: Object) -> ObjectId {
        self.0.add_object(object)
    }

    pub(crate) fn page_ids(&self) -> Vec<ObjectId> {
        self.0.page_iter().collect()
    }

    /// Overlay `overlay` on `page` so the original content's graphics state does not leak.
    pub(crate) fn append_isolated(
        &mut self,
        page: ObjectId,
        save: ObjectId,
        overlay: Vec<u8>,
    ) -> Result<()> {
        let original = self.original_content_refs(page)?;
        let overlay_id = self.add_object(Object::Stream(Stream::new(Dictionary::new(), overlay)));
        let mut contents = Vec::with_capacity(original.len() + 2);
        contents.push(Object::Reference(save));
        contents.extend(original);
        contents.push(Object::Reference(overlay_id));
        let page = self
            .0
            .get_object_mut(page)
            .and_then(Object::as_dict_mut)
            .map_err(|err| {
                Error::new(ErrorKind::FlattenFailed(FlattenReason::AppendContent)).with_source(err)
            })?;
        page.set("Contents", Object::Array(contents));
        Ok(())
    }

    pub(crate) fn attach_overlay_resources(
        &mut self,
        page: ObjectId,
        ids: &OverlayIds,
    ) -> Result<OverlayNames> {
        let mut resources = self.copied_resources(page)?;
        let mut states = self.copied_resource_dict(&resources, b"ExtGState");
        let opaque = fresh_name(
            &states,
            "PdfCraftOpaque",
            FlattenReason::GraphicsStateNameTaken,
        )?;
        states.set(opaque.as_str(), Object::Reference(ids.opaque));
        resources.set("ExtGState", Object::Dictionary(states));
        let font = match ids.font {
            Some(font_id) => {
                let mut fonts = self.copied_resource_dict(&resources, b"Font");
                let name = fresh_name(&fonts, "PdfCraftForms", FlattenReason::FontNameTaken)?;
                fonts.set(name.as_str(), Object::Reference(font_id));
                resources.set("Font", Object::Dictionary(fonts));
                Some(name)
            }
            None => None,
        };
        let resources_id = self.add_object(Object::Dictionary(resources));
        let page = self
            .0
            .get_object_mut(page)
            .and_then(Object::as_dict_mut)
            .map_err(|err| {
                Error::new(ErrorKind::FlattenFailed(FlattenReason::UpdateResources))
                    .with_source(err)
            })?;
        page.set("Resources", Object::Reference(resources_id));
        Ok(OverlayNames { font, opaque })
    }

    fn original_content_refs(&self, page: ObjectId) -> Result<Vec<Object>> {
        let dict = self.0.get_dictionary(page).map_err(|err| {
            Error::new(ErrorKind::FlattenFailed(FlattenReason::ReadPage)).with_source(err)
        })?;
        match dict.get(b"Contents").ok().cloned() {
            None => Ok(Vec::new()),
            Some(contents) => self.classify_contents(contents),
        }
    }

    fn classify_contents(&self, contents: Object) -> Result<Vec<Object>> {
        match contents {
            Object::Null => Ok(Vec::new()),
            Object::Reference(id) => match self.lookup(Object::Reference(id)) {
                Lookup::Missing | Lookup::Found(Object::Null) => Ok(Vec::new()),
                Lookup::Found(Object::Stream(_)) => Ok(vec![Object::Reference(id)]),
                Lookup::Found(Object::Array(items)) => Ok(items),
                Lookup::Found(
                    Object::Boolean(_)
                    | Object::Integer(_)
                    | Object::Real(_)
                    | Object::Name(_)
                    | Object::String(_, _)
                    | Object::Dictionary(_)
                    | Object::Reference(_),
                ) => Err(Error::new(ErrorKind::FlattenFailed(
                    FlattenReason::AppendContent,
                ))),
            },
            Object::Array(items) => Ok(items),
            Object::Boolean(_)
            | Object::Integer(_)
            | Object::Real(_)
            | Object::Name(_)
            | Object::String(_, _)
            | Object::Dictionary(_)
            | Object::Stream(_) => Err(Error::new(ErrorKind::FlattenFailed(
                FlattenReason::AppendContent,
            ))),
        }
    }

    fn copied_resource_dict(&self, resources: &Dictionary, key: &[u8]) -> Dictionary {
        match resources.get(key).ok().cloned() {
            None => Dictionary::new(),
            Some(obj) => match self.lookup(obj) {
                Lookup::Found(Object::Dictionary(dict)) => dict,
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
                | Lookup::Missing => Dictionary::new(),
            },
        }
    }

    pub(crate) fn strip_form(&mut self) -> Result<()> {
        if let Ok(catalog) = self.0.catalog_mut() {
            catalog.remove(b"AcroForm");
        }
        let pages = self.page_ids();
        for page_id in pages {
            let kept = {
                let page = self.0.get_dictionary(page_id).map_err(|err| {
                    Error::new(ErrorKind::FlattenFailed(FlattenReason::ReadPage)).with_source(err)
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
                    Error::new(ErrorKind::FlattenFailed(FlattenReason::UpdateAnnotations))
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
    /// lopdf 0.47 drops `/Prev` and `/XRefStm` while reading. This still removes `/Prev`
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
    fn copied_resources(&self, page_id: ObjectId) -> Result<Dictionary> {
        let mut id = page_id;
        for _ in 0..MAX_PARENT {
            let found = {
                let dict = self.0.get_dictionary(id).map_err(|err| {
                    Error::new(ErrorKind::FlattenFailed(FlattenReason::ReadPage)).with_source(err)
                })?;
                let resources = dict.get(b"Resources").ok().cloned();
                let parent = dict.get(b"Parent").and_then(Object::as_reference).ok();
                (resources, parent)
            };
            if let Some(resources) = found.0 {
                return match self.lookup(resources) {
                    Lookup::Found(Object::Dictionary(dict)) => Ok(dict),
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
                    | Lookup::Missing => Err(Error::new(ErrorKind::FlattenFailed(
                        FlattenReason::ResourcesNotDictionary,
                    ))),
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
        let is_widget = dict.get(b"Subtype").is_ok_and(|subtype| {
            matches!(
                self.lookup(subtype.clone()),
                Lookup::Found(found) if super::read::name_str(&found) == Some("Widget")
            )
        });
        !is_widget
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

fn fresh_name(dict: &Dictionary, base: &str, taken: FlattenReason) -> Result<String> {
    for n in 0..64 {
        let candidate = if n == 0 {
            base.to_owned()
        } else {
            format!("{}{}", base, n + 1)
        };
        if !dict.has(candidate.as_bytes()) {
            return Ok(candidate);
        }
    }
    Err(Error::new(ErrorKind::FlattenFailed(taken)))
}

fn collect_refs(obj: &Object, out: &mut Vec<ObjectId>) -> Result<()> {
    let mut stack = vec![(obj, 0_usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > MAX_INLINE_DEPTH {
            return Err(Error::new(ErrorKind::FlattenFailed(
                FlattenReason::NestingTooDeep { depth },
            )));
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
            Object::Null
            | Object::Boolean(_)
            | Object::Integer(_)
            | Object::Real(_)
            | Object::Name(_)
            | Object::String(_, _) => {}
        }
    }
    Ok(())
}

fn refs_dangle(obj: &Object, seen: &BTreeSet<ObjectId>) -> Result<bool> {
    let mut stack = vec![(obj, 0_usize)];
    while let Some((current, depth)) = stack.pop() {
        if depth > MAX_INLINE_DEPTH {
            return Err(Error::new(ErrorKind::FlattenFailed(
                FlattenReason::NestingTooDeep { depth },
            )));
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
            Object::Null
            | Object::Boolean(_)
            | Object::Integer(_)
            | Object::Real(_)
            | Object::Name(_)
            | Object::String(_, _) => {}
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
            err.kind(),
            ErrorKind::FlattenFailed(FlattenReason::NestingTooDeep { depth: 65 })
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
