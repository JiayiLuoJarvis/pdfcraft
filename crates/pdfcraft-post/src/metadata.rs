//! Write PDF `/Info` fields on an existing document.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::{Error, ErrorKind};

/// Write or overwrite selected `/Info` fields.
///
/// Every field is optional. `None` leaves the existing value untouched.
/// `keywords` are joined with `", "` when written.
///
/// Unlike `pdfcraft-pdf`'s `DocumentSpec.info`, this type allows setting
/// `creator` and `producer`. Layout generation keeps Creator fixed as
/// `pdfcraft` and does not expose Producer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetMetadata {
    /// `/Title`.
    pub title: Option<String>,
    /// `/Author`.
    pub author: Option<String>,
    /// `/Subject`.
    pub subject: Option<String>,
    /// `/Keywords` entries. Written as a comma-separated string.
    pub keywords: Option<Vec<String>>,
    /// `/Creator`. Allowed here; layout generation does not open this field.
    pub creator: Option<String>,
    /// `/Producer`. Allowed here; layout generation does not set this field.
    pub producer: Option<String>,
    /// `/CreationDate` in PDF `D:` date syntax.
    pub creation_date: Option<String>,
    /// `/ModDate` in PDF `D:` date syntax.
    pub modification_date: Option<String>,
}

impl SetMetadata {
    /// Apply the non-`None` fields and return new PDF bytes.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotAPdf`] when `pdf` is not a PDF, [`ErrorKind::MetadataFailed`]
    /// when the file is encrypted, a date is not `D:` syntax, or the `/Info`
    /// dictionary cannot be written.
    #[must_use = "dropping the PDF discards the only copy"]
    pub fn apply(&self, pdf: &[u8]) -> Result<Vec<u8>, Error> {
        let mut doc = open_pdf(pdf, ErrorKind::MetadataFailed)?;
        write_docinfo(&mut doc, self)?;
        save_pdf(&mut doc).map_err(|err| Error::from(ErrorKind::MetadataFailed).with_source(err))
    }
}

pub(crate) fn open_pdf(pdf: &[u8], encrypted: ErrorKind) -> Result<Document, Error> {
    let doc = match Document::load_mem(pdf) {
        Ok(doc) => doc,
        Err(err) if matches!(err, lopdf::Error::Decryption(_)) => {
            return Err(Error::from(encrypted).with_source(err));
        }
        Err(err) => return Err(Error::from(ErrorKind::NotAPdf).with_source(err)),
    };
    if doc.is_encrypted() {
        return Err(Error::from(encrypted));
    }
    Ok(doc)
}

pub(crate) fn save_pdf(doc: &mut Document) -> Result<Vec<u8>, lopdf::Error> {
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

pub(crate) fn write_docinfo(doc: &mut Document, meta: &SetMetadata) -> Result<(), Error> {
    if let Some(date) = meta.creation_date.as_deref() {
        check_pdf_date(date)?;
    }
    if let Some(date) = meta.modification_date.as_deref() {
        check_pdf_date(date)?;
    }

    let info_id = info_object_id(doc)?;

    let info = doc
        .objects
        .get_mut(&info_id)
        .and_then(|obj| obj.as_dict_mut().ok())
        .ok_or_else(|| Error::from(ErrorKind::MetadataFailed))?;

    set_str(info, b"Title", meta.title.as_deref());
    set_str(info, b"Author", meta.author.as_deref());
    set_str(info, b"Subject", meta.subject.as_deref());
    if let Some(keywords) = &meta.keywords {
        set_str(info, b"Keywords", Some(&keywords.join(", ")));
    }
    set_str(info, b"Creator", meta.creator.as_deref());
    set_str(info, b"Producer", meta.producer.as_deref());
    set_str(info, b"CreationDate", meta.creation_date.as_deref());
    set_str(info, b"ModDate", meta.modification_date.as_deref());
    Ok(())
}

fn info_object_id(doc: &mut Document) -> Result<ObjectId, Error> {
    match doc.trailer.get(b"Info") {
        Ok(Object::Reference(id)) => Ok(*id),
        Ok(Object::Dictionary(dict)) => {
            let dict = dict.clone();
            let id = doc.add_object(Object::Dictionary(dict));
            doc.trailer.set("Info", Object::Reference(id));
            Ok(id)
        }
        Ok(_) => Err(Error::from(ErrorKind::MetadataFailed)),
        Err(_) => {
            let id = doc.add_object(Object::Dictionary(Dictionary::new()));
            doc.trailer.set("Info", Object::Reference(id));
            Ok(id)
        }
    }
}

fn set_str(info: &mut Dictionary, key: &[u8], value: Option<&str>) {
    if let Some(value) = value {
        info.set(key, lopdf::text_string(value));
    }
}

fn check_pdf_date(value: &str) -> Result<(), Error> {
    if value.is_ascii() && value.starts_with("D:") && value.len() >= 6 {
        Ok(())
    } else {
        Err(Error::from(ErrorKind::MetadataFailed).with_source(PdfDateSyntax))
    }
}

#[derive(Debug)]
struct PdfDateSyntax;

impl std::fmt::Display for PdfDateSyntax {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PDF date must use D: syntax")
    }
}

impl std::error::Error for PdfDateSyntax {}
