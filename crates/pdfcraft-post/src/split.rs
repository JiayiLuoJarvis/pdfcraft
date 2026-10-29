//! Split a PDF into one single-page PDF per page.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::{Error, ErrorKind};
use crate::metadata::{open_pdf, save_pdf};
use crate::pages::import_pages_with;

/// Split every page of `pdf` into its own single-page PDF.
///
/// Pages stay in reading order: output `i` holds page `i`. Each output is a
/// self-contained PDF with its own `Pages` parent (`Count` 1), the page's
/// materialized `MediaBox`/`Resources`/`CropBox`/`Rotate`, and its page-level
/// `Annots`. Document-level outlines, attachments, and structure trees are
/// not kept (same as [`crate::merge_pdfs`] without outline preservation).
///
/// A PDF with no pages splits to an empty vector. This is not
/// [`ErrorKind::MergeEmpty`]: splitting takes one document, so there is no
/// empty part list to reject.
///
/// # Errors
///
/// [`ErrorKind::NotAPdf`] when `pdf` is not a PDF,
/// [`ErrorKind::Encrypted`] when `pdf` is encrypted (including files whose
/// empty user password `load_mem` auto-decrypts; those are still rejected so
/// a split never re-saves auto-decrypted bytes), and
/// [`ErrorKind::SplitFailed`] when a page cannot be copied.
#[must_use = "dropping the split PDFs discards the only copies"]
pub fn split_pdfs(pdf: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
    let src = open_pdf(pdf, ErrorKind::Encrypted)?;
    let page_ids: Vec<ObjectId> = src.page_iter().collect();
    let mut out = Vec::with_capacity(page_ids.len());
    for page_id in page_ids {
        out.push(single_page(&src, page_id)?);
    }
    Ok(out)
}

fn single_page(src: &Document, page_id: ObjectId) -> Result<Vec<u8>, Error> {
    let mut dest = Document::with_version("1.7");
    let parent_id = dest.new_object_id();
    dest.set_object(parent_id, Dictionary::new());

    let mut imported = import_pages_with(
        &mut dest,
        src,
        parent_id,
        &[page_id],
        &ErrorKind::SplitFailed,
    )?;
    let Some(new_page) = imported.pop() else {
        return Err(Error::from(ErrorKind::SplitFailed));
    };

    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", 1);
    pages.set("Kids", vec![Object::from(new_page)]);
    dest.set_object(parent_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", parent_id);
    let catalog_id = dest.add_object(Object::Dictionary(catalog));
    dest.trailer.set("Root", catalog_id);

    save_pdf(&mut dest).map_err(|err| Error::from(ErrorKind::SplitFailed).with_source(err))
}
