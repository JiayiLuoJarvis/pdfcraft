//! Open existing PDF bytes and save a `lopdf` document back to bytes.

use lopdf::Document;

use crate::error::{Error, ErrorKind};

pub(crate) fn open_pdf(pdf: &[u8]) -> Result<Document, Error> {
    let doc = match Document::load_mem(pdf) {
        Ok(doc) => doc,
        Err(err) if matches!(err, lopdf::Error::Decryption(_)) => {
            return Err(Error::from(ErrorKind::Encrypted).with_source(err));
        }
        Err(err) => return Err(Error::from(ErrorKind::NotAPdf).with_source(err)),
    };
    // `load_mem` auto-decrypts files whose user password is empty: it
    // decrypts in memory, drops trailer `/Encrypt`, and records the state.
    // `is_encrypted` alone would pass those through, so reject every trace:
    // still-encrypted, auto-decrypted (`was_encrypted`), or a leftover
    // trailer `/Encrypt` entry. No processor re-saves auto-decrypted bytes.
    if doc.is_encrypted() || doc.was_encrypted() || doc.trailer.get(b"Encrypt").is_ok() {
        return Err(Error::from(ErrorKind::Encrypted));
    }
    Ok(doc)
}

pub(crate) fn save_pdf(doc: &mut Document) -> Result<Vec<u8>, lopdf::Error> {
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}
