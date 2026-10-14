//! Concatenate PDF pages in list order.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::error::{Error, ErrorKind};
use crate::metadata::{open_pdf, save_pdf};

const MAX_DEPTH: usize = 64;

/// Concatenate PDFs in order.
///
/// An empty `parts` list fails with [`ErrorKind::MergeEmpty`]. One part is
/// returned unchanged. Two or more parts append pages in list order.
///
/// Attachments and structure trees are not kept. `preserve_outlines: false`
/// drops outlines. `preserve_outlines: true` keeps the first part's outlines
/// and does not import outlines from later parts.
///
/// # Errors
///
/// [`ErrorKind::MergeEmpty`] when `parts` is empty, [`ErrorKind::NotAPdf`] when
/// a part is not a PDF, and [`ErrorKind::MergeFailed`] when a part is encrypted
/// or its pages cannot be copied.
#[must_use = "dropping the merged PDF discards the only copy"]
pub fn merge_pdfs<I, B>(parts: I, preserve_outlines: bool) -> Result<Vec<u8>, Error>
where
    I: IntoIterator<Item = B>,
    B: AsRef<[u8]>,
{
    let mut parts: Vec<Vec<u8>> = parts
        .into_iter()
        .map(|part| part.as_ref().to_vec())
        .collect();
    if parts.is_empty() {
        return Err(Error::from(ErrorKind::MergeEmpty));
    }
    if parts.len() == 1 {
        return Ok(parts.remove(0));
    }

    let bytes = if preserve_outlines {
        preserve_first(&parts)?
    } else {
        pages_only(&parts)?
    };
    Ok(bytes)
}

fn pages_only(parts: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let mut dest = Document::with_version("1.7");
    let pages_id = dest.new_object_id();
    dest.set_object(pages_id, Dictionary::new());

    let mut kids = Vec::new();
    for part in parts {
        let src = open_pdf(part, ErrorKind::MergeFailed)?;
        kids.extend(import_pages(&mut dest, &src, pages_id)?);
    }

    let count = i64::try_from(kids.len()).map_err(|_| Error::from(ErrorKind::MergeFailed))?;
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", count);
    pages.set(
        "Kids",
        kids.into_iter().map(Object::from).collect::<Vec<Object>>(),
    );
    dest.set_object(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", pages_id);
    let catalog_id = dest.add_object(Object::Dictionary(catalog));
    dest.trailer.set("Root", catalog_id);

    save_pdf(&mut dest).map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))
}

fn preserve_first(parts: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let mut dest = open_pdf(&parts[0], ErrorKind::MergeFailed)?;
    let pages_id = root_pages_id(&dest)?;
    let mut extra = Vec::new();
    for part in &parts[1..] {
        let src = open_pdf(part, ErrorKind::MergeFailed)?;
        extra.extend(import_pages(&mut dest, &src, pages_id)?);
    }
    append_kids(&mut dest, pages_id, &extra)?;
    strip_extras(&mut dest);
    save_pdf(&mut dest).map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))
}

fn root_pages_id(doc: &Document) -> Result<ObjectId, Error> {
    doc.catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))
}

fn append_kids(doc: &mut Document, pages_id: ObjectId, extra: &[ObjectId]) -> Result<(), Error> {
    if extra.is_empty() {
        return Ok(());
    }
    let mut kids = kids_array(doc, pages_id)?;
    for id in extra {
        kids.push(Object::Reference(*id));
    }
    let count = i64::try_from(doc.page_iter().count() + extra.len())
        .map_err(|_| Error::from(ErrorKind::MergeFailed))?;
    let pages = doc
        .get_dictionary_mut(pages_id)
        .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))?;
    pages.set("Kids", kids);
    pages.set("Count", count);
    Ok(())
}

fn kids_array(doc: &Document, pages_id: ObjectId) -> Result<Vec<Object>, Error> {
    let pages = doc
        .get_dictionary(pages_id)
        .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))?;
    let kids = pages
        .get(b"Kids")
        .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))?;
    match kids {
        Object::Array(items) => Ok(items.clone()),
        Object::Reference(id) => doc
            .get_object(*id)
            .and_then(Object::as_array)
            .cloned()
            .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err)),
        _ => Err(Error::from(ErrorKind::MergeFailed)),
    }
}

fn import_pages(
    dest: &mut Document,
    src: &Document,
    parent: ObjectId,
) -> Result<Vec<ObjectId>, Error> {
    let mut remapper = Remapper {
        src,
        map: HashMap::new(),
        depth: 0,
    };
    let page_ids: Vec<ObjectId> = src.page_iter().collect();
    let mut imported = Vec::with_capacity(page_ids.len());
    for page_id in page_ids {
        let page = materialized_page(src, page_id)?;
        let mut dict = remapper.remap_dict(dest, &page)?;
        dict.set("Parent", parent);
        dict.set("Type", Object::Name(b"Page".to_vec()));
        imported.push(dest.add_object(Object::Dictionary(dict)));
    }
    Ok(imported)
}

/// Copy inherited page attributes onto the page dict.
///
/// PDF pages may omit `Resources` and `MediaBox` when an ancestor `Pages`
/// node holds them. A merged page has a new parent, so those values have to
/// travel with the page.
fn materialized_page(src: &Document, page_id: ObjectId) -> Result<Dictionary, Error> {
    let mut page = src
        .get_dictionary(page_id)
        .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))?
        .clone();
    let mut current = page_id;
    for _ in 0..32 {
        let dict = src
            .get_dictionary(current)
            .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))?;
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
            .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))?;
        let typed_pages = parent_dict.get_type().ok() == Some(b"Pages".as_slice());
        if !typed_pages && !parent_dict.has(b"Kids") {
            break;
        }
        current = parent;
    }
    page.remove(b"Parent");
    Ok(page)
}

fn strip_extras(doc: &mut Document) {
    let Ok(root) = doc.trailer.get(b"Root").and_then(Object::as_reference) else {
        return;
    };
    let names_id = doc.get_dictionary(root).ok().and_then(|catalog| {
        catalog
            .get(b"Names")
            .ok()
            .and_then(|object| object.as_reference().ok())
    });
    if let Ok(catalog) = doc.get_dictionary_mut(root) {
        catalog.remove(b"StructTreeRoot");
        catalog.remove(b"MarkInfo");
    }
    if let Some(names_id) = names_id {
        if let Ok(names) = doc.get_dictionary_mut(names_id) {
            names.remove(b"EmbeddedFiles");
        }
    }
}

struct Remapper<'a> {
    src: &'a Document,
    map: HashMap<ObjectId, ObjectId>,
    depth: usize,
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
            return Err(Error::from(ErrorKind::MergeFailed));
        }
        self.depth += 1;
        let new_id = dest.new_object_id();
        self.map.insert(id, new_id);
        let object = self
            .src
            .get_object(id)
            .map_err(|err| Error::from(ErrorKind::MergeFailed).with_source(err))?
            .clone();
        let copied = self.remap(dest, &object)?;
        dest.set_object(new_id, copied);
        self.depth -= 1;
        Ok(new_id)
    }
}
