//! AES-256 encryption (Standard Security Handler revision 6 / lopdf V5).

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{EncryptionState, EncryptionVersion, Permissions};

use crate::error::{Error, ErrorKind};
use crate::metadata::{open_pdf, save_pdf};

/// Full PDF encryption configuration.
///
/// Start from a [`EncryptionPreset`] value and override fields as needed.
/// Passwords on this struct may be overridden per call on [`Encrypt`].
///
/// Permission fields mirror the PDF security handler bits one-for-one, so the
/// bool count is intentional.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct EncryptionConfig {
    /// Owner password. Required after overrides are applied (this crate does
    /// not invent one).
    pub owner_password: Option<String>,
    /// User password. Empty string is a valid "anyone can open" setup.
    pub user_password: String,
    /// Allow low-quality printing.
    pub allow_print: bool,
    /// Allow high-quality printing.
    pub allow_high_quality_print: bool,
    /// Allow copy / extract.
    pub allow_copy: bool,
    /// Allow content edits other than annotate / fill / assemble.
    pub allow_edit: bool,
    /// Allow annotations.
    pub allow_annotate: bool,
    /// Allow filling forms.
    pub allow_fill_forms: bool,
    /// Allow accessibility extraction (PDF bit; presets keep this true).
    pub allow_accessibility: bool,
    /// Allow page assemble.
    pub allow_assemble: bool,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        EncryptionPreset::READ_ONLY
    }
}

/// Permission presets. Passwords stay unset.
///
/// Supply the owner password on [`Encrypt`] or on
/// [`EncryptionConfig::owner_password`]. This crate does not invent one.
/// Every preset leaves accessibility extraction on.
#[derive(Debug, Clone, Copy)]
pub struct EncryptionPreset;

impl EncryptionPreset {
    /// No print, copy, edit, annotate, fill, or assemble. Accessibility stays on.
    pub const READ_ONLY: EncryptionConfig = EncryptionConfig {
        owner_password: None,
        user_password: String::new(),
        allow_print: false,
        allow_high_quality_print: false,
        allow_copy: false,
        allow_edit: false,
        allow_annotate: false,
        allow_fill_forms: false,
        allow_accessibility: true,
        allow_assemble: false,
    };

    /// Print, including high quality. Other edits stay off. Accessibility stays on.
    pub const READ_ONLY_PRINTABLE: EncryptionConfig = EncryptionConfig {
        owner_password: None,
        user_password: String::new(),
        allow_print: true,
        allow_high_quality_print: true,
        allow_copy: false,
        allow_edit: false,
        allow_annotate: false,
        allow_fill_forms: false,
        allow_accessibility: true,
        allow_assemble: false,
    };

    /// Every permission bit this crate exposes, including accessibility.
    pub const FULL_PERMISSIONS: EncryptionConfig = EncryptionConfig {
        owner_password: None,
        user_password: String::new(),
        allow_print: true,
        allow_high_quality_print: true,
        allow_copy: true,
        allow_edit: true,
        allow_annotate: true,
        allow_fill_forms: true,
        allow_accessibility: true,
        allow_assemble: true,
    };
}

/// Encrypt a PDF with AES-256 (revision 6).
///
/// `config` is required. `owner_password` / `user_password` override the
/// config when set. This crate never invents an owner password.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Encrypt {
    /// Permission and default password settings.
    pub config: Option<EncryptionConfig>,
    /// Overrides [`EncryptionConfig::owner_password`] when `Some`.
    pub owner_password: Option<String>,
    /// Overrides [`EncryptionConfig::user_password`] when `Some`.
    pub user_password: Option<String>,
}

impl Encrypt {
    /// Encrypt `pdf` and return the ciphertext bytes.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::NotAPdf`] when `pdf` is not a PDF,
    /// [`ErrorKind::ConfigMissing`] when `config` is missing or the owner
    /// password is empty, and [`ErrorKind::EncryptFailed`] when the file is
    /// already encrypted or lopdf cannot write the security handler.
    #[must_use = "dropping the encrypted PDF discards the only copy"]
    pub fn apply(&self, pdf: &[u8]) -> Result<Vec<u8>, Error> {
        let mut doc = open_pdf(pdf, ErrorKind::EncryptFailed)?;
        encrypt_document(&mut doc, self)?;
        save_pdf(&mut doc).map_err(|err| Error::from(ErrorKind::EncryptFailed).with_source(err))
    }
}

pub(crate) fn encrypt_document(doc: &mut lopdf::Document, encrypt: &Encrypt) -> Result<(), Error> {
    let (owner, user, permissions) = resolve_passwords(encrypt)?;
    let mut file_key = [0u8; 32];
    getrandom::fill(&mut file_key)
        .map_err(|err| Error::from(ErrorKind::EncryptFailed).with_source(err))?;

    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        file_encryption_key: &file_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: owner.as_str(),
        user_password: user.as_str(),
        permissions,
    };
    let state = EncryptionState::try_from(version)
        .map_err(|err| Error::from(ErrorKind::EncryptFailed).with_source(err))?;
    if doc.is_encrypted() {
        return Err(Error::from(ErrorKind::EncryptFailed));
    }
    // Revision 6 is a PDF 2.0 security handler.
    if doc.version.as_str() < "2.0" {
        "2.0".clone_into(&mut doc.version);
    }
    doc.encrypt(&state)
        .map_err(|err| Error::from(ErrorKind::EncryptFailed).with_source(err))?;
    Ok(())
}

fn resolve_passwords(encrypt: &Encrypt) -> Result<(String, String, Permissions), Error> {
    let cfg = encrypt.config.as_ref().ok_or(ErrorKind::ConfigMissing)?;
    let owner = encrypt
        .owner_password
        .clone()
        .or_else(|| cfg.owner_password.clone())
        .filter(|s| !s.is_empty())
        .ok_or(ErrorKind::ConfigMissing)?;
    let user = encrypt
        .user_password
        .clone()
        .unwrap_or_else(|| cfg.user_password.clone());
    Ok((owner, user, permissions_from_config(cfg)))
}

fn permissions_from_config(cfg: &EncryptionConfig) -> Permissions {
    let mut perms = Permissions::empty();
    if cfg.allow_print {
        perms |= Permissions::PRINTABLE;
    }
    if cfg.allow_high_quality_print {
        perms |= Permissions::PRINTABLE_IN_HIGH_QUALITY;
    }
    if cfg.allow_copy {
        perms |= Permissions::COPYABLE;
    }
    if cfg.allow_edit {
        perms |= Permissions::MODIFIABLE;
    }
    if cfg.allow_annotate {
        perms |= Permissions::ANNOTABLE;
    }
    if cfg.allow_fill_forms {
        perms |= Permissions::FILLABLE;
    }
    if cfg.allow_accessibility {
        perms |= Permissions::COPYABLE_FOR_ACCESSIBILITY;
    }
    if cfg.allow_assemble {
        perms |= Permissions::ASSEMBLABLE;
    }
    perms
}
