//! Integration tests for pdfcraft-post.

#[allow(
    dead_code,
    reason = "shared helpers may be unused in some test targets"
)]
mod common;

use pdfcraft_post::{Encrypt, EncryptionPreset, ErrorKind};

fn encrypt_with_owner(config: pdfcraft_post::EncryptionConfig) -> Encrypt {
    Encrypt {
        config: Some(config),
        owner_password: Some("owner-secret".into()),
        user_password: None,
    }
}

#[test]
fn requires_config() {
    let pdf = common::sample_pdf(None, None);
    let err = Encrypt::default().apply(&pdf).expect_err("missing config");
    assert_eq!(err.kind, ErrorKind::ConfigMissing);
}

#[test]
fn requires_owner_password() {
    let pdf = common::sample_pdf(None, None);
    let err = Encrypt {
        config: Some(EncryptionPreset::READ_ONLY),
        owner_password: None,
        user_password: None,
    }
    .apply(&pdf)
    .expect_err("missing owner");
    assert_eq!(err.kind, ErrorKind::ConfigMissing);
}

#[test]
fn read_only_printable_opens_with_empty_user() {
    let pdf = common::sample_pdf(None, None);
    let out = encrypt_with_owner(EncryptionPreset::READ_ONLY_PRINTABLE)
        .apply(&pdf)
        .expect("encrypt");
    assert!(out.starts_with(b"%PDF-2.0"));
    assert!(out.windows(4).any(|window| window == b"/V 5"));
    assert!(out.windows(4).any(|window| window == b"/R 6"));
    assert!(out.windows(10).any(|window| window == b"/CFM/AESV3"));
    let doc = common::open_with_password(&out, "").expect("open empty user");
    assert_eq!(doc.get_pages().len(), 1);
    assert!(common::is_encrypted_pdf(&out));
    assert!(common::perm_bit(&out, 2), "print");
    assert!(common::perm_bit(&out, 11), "high quality print");
    assert!(!common::perm_bit(&out, 3), "edit stays off");
    assert!(common::perm_bit(&out, 9), "accessibility");
}

#[test]
fn read_only_encrypts() {
    let pdf = common::sample_pdf(None, None);
    let out = encrypt_with_owner(EncryptionPreset::READ_ONLY)
        .apply(&pdf)
        .expect("encrypt");
    let doc = common::open_with_password(&out, "").expect("open");
    assert_eq!(doc.get_pages().len(), 1);
    assert!(common::is_encrypted_pdf(&out));
    assert!(!common::perm_bit(&out, 2), "print stays off");
    assert!(!common::perm_bit(&out, 3), "edit stays off");
    assert!(common::perm_bit(&out, 9), "accessibility");
}

#[test]
fn full_permissions_encrypts() {
    let pdf = common::sample_pdf(None, None);
    let out = encrypt_with_owner(EncryptionPreset::FULL_PERMISSIONS)
        .apply(&pdf)
        .expect("encrypt");
    let doc = common::open_with_password(&out, "").expect("open");
    assert_eq!(doc.get_pages().len(), 1);
    assert!(common::is_encrypted_pdf(&out));
    assert!(common::perm_bit(&out, 3), "edit");
    assert!(common::perm_bit(&out, 4), "copy");
    assert!(common::perm_bit(&out, 10), "assemble");
}

#[test]
fn user_password_override_rejects_empty_open() {
    let pdf = common::sample_pdf(None, None);
    let out = Encrypt {
        config: Some(EncryptionPreset::READ_ONLY),
        owner_password: Some("owner-secret".into()),
        user_password: Some("user-pw".into()),
    }
    .apply(&pdf)
    .expect("encrypt");

    assert!(
        common::open_with_password(&out, "").is_err(),
        "empty user must fail"
    );
    let doc = common::open_with_password(&out, "user-pw").expect("user pw");
    assert_eq!(doc.get_pages().len(), 1);
    assert!(common::is_encrypted_pdf(&out));
    common::open_with_password(&out, "owner-secret").expect("owner pw");
}

#[test]
fn invalid_bytes_are_not_a_pdf() {
    let err = encrypt_with_owner(EncryptionPreset::READ_ONLY)
        .apply(b"not-a-pdf")
        .expect_err("bad bytes");
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}
