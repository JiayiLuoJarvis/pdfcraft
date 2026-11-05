//! One-pass metadata write plus encryption.

use crate::encryption::{encrypt_document, Encrypt};
use crate::error::{Error, ErrorKind};
use crate::metadata::{write_docinfo, SetMetadata};
use crate::pdf::{open_pdf, save_pdf};

/// Write `/Info` then encrypt in a single open/save path.
///
/// Prefer this over `Pipeline` of [`SetMetadata`] then [`Encrypt`] when both
/// always run together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StampAndEncrypt {
    /// Metadata to stamp before encryption.
    pub metadata: SetMetadata,
    /// Encryption settings (config required).
    pub encrypt: Encrypt,
}

impl StampAndEncrypt {
    /// Stamp metadata and encrypt `pdf`.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotAPdf`] when `pdf` is not a PDF,
    /// [`ErrorKind::Encrypted`] when `pdf` is already encrypted (including
    /// files whose empty user password `load_mem` auto-decrypts),
    /// [`ErrorKind::MetadataFailed`] when a date is not `D:` syntax,
    /// [`ErrorKind::ConfigMissing`] when encryption settings are incomplete,
    /// and [`ErrorKind::EncryptFailed`] when encryption cannot be written.
    #[must_use = "dropping the PDF discards the only copy"]
    pub fn apply(&self, pdf: &[u8]) -> Result<Vec<u8>, Error> {
        let mut doc = open_pdf(pdf)?;
        write_docinfo(&mut doc, &self.metadata)?;
        encrypt_document(&mut doc, &self.encrypt)?;
        save_pdf(&mut doc).map_err(|err| Error::from(ErrorKind::EncryptFailed).with_source(err))
    }
}
