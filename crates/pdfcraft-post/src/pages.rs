//! Shared page-copy core: import pages from one PDF into another.
//!
//! Used by [`crate::merge_pdfs`] and [`crate::split_pdfs`]. Moving this here
//! keeps the deep-copy traversal (`Remapper`) and inherited-attribute
//! materialization in one place. Every failure maps to the caller-supplied
//! [`ErrorKind`], so merge and split share the traversal byte-for-byte while
//! reporting their own kind.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::error::{Error, ErrorKind};

pub(crate) const MAX_DEPTH: usize = 64;

pub(crate) fn import_pages(
    dest: &mut Document,
    src: &Document,
    parent: ObjectId,
) -> Result<Vec<ObjectId>, Error> {
    let page_ids: Vec<ObjectId> = src.page_iter().collect();
    import_pages_with(dest, src, parent, &page_ids, &ErrorKind::MergeFailed)
}

/// Import `page_ids` from `src` into `dest`, re-parented under `parent`.
///
/// Failures map to `kind`: [`ErrorKind::MergeFailed`] for merge callers,
/// [`ErrorKind::SplitFailed`] for split callers. The traversal is identical
/// either way; only the reported kind differs.
pub(crate) fn import_pages_with(
    dest: &mut Document,
    src: &Document,
    parent: ObjectId,
    page_ids: &[ObjectId],
    kind: &ErrorKind,
) -> Result<Vec<ObjectId>, Error> {
    let mut remapper = Remapper {
        src,
        map: HashMap::new(),
        depth: 0,
        kind: kind.clone(),
    };
    let mut imported = Vec::with_capacity(page_ids.len());
    for page_id in page_ids {
        let page = materialized_page_with(src, *page_id, kind)?;
        let mut dict = remapper.remap_dict(dest, &page)?;
        dict.set("Parent", parent);
        dict.set("Type", Object::Name(b"Page".to_vec()));
        imported.push(dest.add_object(Object::Dictionary(dict)));
    }
    Ok(imported)
}

/// Copy inherited page attributes onto the page dict, mapping failures to
/// `kind` (see [`import_pages_with`]).
pub(crate) fn materialized_page_with(
    src: &Document,
    page_id: ObjectId,
    kind: &ErrorKind,
) -> Result<Dictionary, Error> {
    let mut page = src
        .get_dictionary(page_id)
        .map_err(|err| Error::from(kind.clone()).with_source(err))?
        .clone();
    let mut current = page_id;
    for _ in 0..32 {
        let dict = src
            .get_dictionary(current)
            .map_err(|err| Error::from(kind.clone()).with_source(err))?;
        for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
            if !page.has(key) {
                if let Ok(value) = dict.get(key) {
                    page.set(key.to_vec(), value.clone());
                }
            }
        }
        let Some(parent) = dict
            .get(b"Parent")
            .ok()
            .and_then(|object| object.as_reference().ok())
        else {
            break;
        };
        let parent_dict = src
            .get_dictionary(parent)
            .map_err(|err| Error::from(kind.clone()).with_source(err))?;
        let typed_pages = parent_dict.get_type().ok() == Some(b"Pages".as_slice());
        if !typed_pages && !parent_dict.has(b"Kids") {
            break;
        }
        current = parent;
    }
    page.remove(b"Parent");
    Ok(page)
}

pub(crate) struct Remapper<'a> {
    src: &'a Document,
    map: HashMap<ObjectId, ObjectId>,
    depth: usize,
    kind: ErrorKind,
}

impl Remapper<'_> {
    fn remap_dict(&mut self, dest: &mut Document, dict: &Dictionary) -> Result<Dictionary, Error> {
        let mut out = Dictionary::new();
        for (key, value) in dict {
            out.set(key.clone(), self.remap(dest, value)?);
        }
        Ok(out)
    }

    fn remap(&mut self, dest: &mut Document, object: &Object) -> Result<Object, Error> {
        match object {
            Object::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.remap(dest, item)?);
                }
                Ok(Object::Array(out))
            }
            Object::Dictionary(dict) => Ok(Object::Dictionary(self.remap_dict(dest, dict)?)),
            Object::Stream(stream) => {
                let dict = self.remap_dict(dest, &stream.dict)?;
                Ok(Object::Stream(Stream {
                    dict,
                    content: stream.content.clone(),
                    allows_compression: stream.allows_compression,
                    start_position: None,
                }))
            }
            Object::Reference(id) => Ok(Object::Reference(self.import_ref(dest, *id)?)),
            other => Ok(other.clone()),
        }
    }

    fn import_ref(&mut self, dest: &mut Document, id: ObjectId) -> Result<ObjectId, Error> {
        if let Some(existing) = self.map.get(&id) {
            return Ok(*existing);
        }
        if self.depth >= MAX_DEPTH {
            return Err(Error::from(self.kind.clone()));
        }
        self.depth += 1;
        let new_id = dest.new_object_id();
        self.map.insert(id, new_id);
        let object = self
            .src
            .get_object(id)
            .map_err(|err| Error::from(self.kind.clone()).with_source(err))?
            .clone();
        let copied = self.remap(dest, &object)?;
        dest.set_object(new_id, copied);
        self.depth -= 1;
        Ok(new_id)
    }
}
