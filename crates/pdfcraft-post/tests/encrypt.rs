//! Integration tests for pdfcraft-post.

#[expect(
    dead_code,
    reason = "shared helpers may be unused in some test targets"
)]
mod common;

use lopdf::Document;
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
fn v5_stdcf_declares_length_32() {
    // Strict readers (Adobe Acrobat, poppler 24.02) reject R6 output whose
    // `StdCF` lacks `/Length 32`; lenient readers (qpdf, browsers) open it.
    // The password envelope (U/O/UE/Perms/content chain) was verified
    // separately, so this test pins only the framing dictionary on the raw
    // bytes. `Document::load_mem` drops `/Encrypt` from the trailer for
    // xref-stream files, so a reload round-trip alone cannot prove this.
    let pdf = common::sample_pdf(None, None);
    let out = encrypt_with_owner(EncryptionPreset::READ_ONLY_PRINTABLE)
        .apply(&pdf)
        .expect("encrypt");
    assert!(out.windows(4).any(|window| window == b"/V 5"), "V5");
    assert!(out.windows(4).any(|window| window == b"/R 6"), "R6");
    let stdcf = out
        .windows(6)
        .position(|window| window == b"/StdCF")
        .expect("StdCF present");
    let window = &out[stdcf..stdcf.saturating_add(400).min(out.len())];
    assert!(
        window.windows(10).any(|w| w == b"/CFM/AESV3"),
        "StdCF keeps /CFM /AESV3"
    );
    assert!(
        window.windows(10).any(|w| w == b"/Length 32"),
        "StdCF must declare /Length 32 (AESV3 = 32 bytes)"
    );
    assert!(
        window.windows(18).any(|w| w == b"/AuthEvent/DocOpen"),
        "StdCF carries /AuthEvent /DocOpen for qpdf parity"
    );
    // Top-level `/Length 256` (bits) mirrors qpdf output; strict readers
    // key off the entry even though it is deprecated for V5. Scope the
    // search to the `/Encrypt` object so stream `/Length` entries elsewhere
    // cannot false-positive. lopdf 0.47 writes `/EncryptMetadata` before some
    // other keys, so the first `/Encrypt` substring is not the dict start.
    let (enc_n, enc_g) = encrypt_object_id(&out).expect("trailer or xref /Encrypt N G R");
    let enc_header = format!("{enc_n} {enc_g} obj");
    let enc_start = out
        .windows(enc_header.len())
        .position(|bytes| bytes == enc_header.as_bytes())
        .expect("Encrypt object");
    let enc_end = out[enc_start..]
        .windows(6)
        .position(|bytes| bytes == b"endobj")
        .map_or(out.len(), |offset| enc_start + offset);
    let enc_dict = &out[enc_start..enc_end];
    assert!(
        enc_dict.windows(11).any(|w| w == b"/Length 256"),
        "Encrypt dict must declare top-level /Length 256"
    );
    // A trailer `/ID` is standard producer output; assert presence only
    // (writer emits `/ID [` with a space; allow either spacing).
    let id_pos = out
        .windows(3)
        .position(|bytes| bytes == b"/ID")
        .expect("trailer must carry /ID");
    let after_id = &out[id_pos + 3..];
    let bracket = after_id
        .iter()
        .take(4)
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(usize::MAX);
    assert_eq!(
        after_id.get(bracket),
        Some(&b'['),
        "trailer /ID must open an array"
    );
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
fn encrypted_input_is_rejected_without_unlocking() {
    // Empty-user-password output is the trap: `load_mem` auto-decrypts it,
    // so the old `is_encrypted`-only gate let it through and re-saved the
    // auto-decrypted bytes. Anchor that the fixture really takes the
    // `was_encrypted` branch, then require `Encrypted`.
    let pdf = common::sample_pdf(None, None);
    let enc = common::encrypted_with_empty_user(&pdf);
    let direct = Document::load_mem(&enc).expect("fixture auto-decrypts");
    assert!(
        !direct.is_encrypted(),
        "load_mem auto-decrypts empty-user input"
    );
    assert!(
        direct.was_encrypted(),
        "fixture must exercise the was_encrypted branch"
    );
    let err = encrypt_with_owner(EncryptionPreset::READ_ONLY)
        .apply(&enc)
        .expect_err("encrypted input");
    assert_eq!(err.kind, ErrorKind::Encrypted);
    assert_eq!(
        format!("{}", ErrorKind::Encrypted),
        "encrypted pdf: password required"
    );
}

#[test]
fn invalid_bytes_are_not_a_pdf() {
    let err = encrypt_with_owner(EncryptionPreset::READ_ONLY)
        .apply(b"not-a-pdf")
        .expect_err("bad bytes");
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}

#[test]
fn truncated_bytes_are_not_a_pdf() {
    let pdf = common::sample_pdf(None, None);
    let cut = pdf.len() / 2;
    let err = encrypt_with_owner(EncryptionPreset::READ_ONLY)
        .apply(&pdf[..cut])
        .expect_err("truncated");
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}

/// Trailer or xref-stream `/Encrypt N G R`, skipping `/EncryptMetadata`.
fn encrypt_object_id(pdf: &[u8]) -> Option<(u32, u16)> {
    let needle = b"/Encrypt";
    let mut pos = 0;
    while let Some(rel) = pdf[pos..]
        .windows(needle.len())
        .position(|window| window == needle)
    {
        let after = pos + rel + needle.len();
        if pdf
            .get(after..)
            .is_some_and(|rest| rest.starts_with(b"Metadata"))
        {
            pos = after;
            continue;
        }
        let rest = skip_pdf_ws(pdf.get(after..)?);
        let (n, rest) = parse_pdf_u32(rest)?;
        let rest = skip_pdf_ws(rest);
        let (g, rest) = parse_pdf_u32(rest)?;
        let rest = skip_pdf_ws(rest);
        if rest.first() == Some(&b'R') {
            let g = u16::try_from(g).ok()?;
            return Some((n, g));
        }
        pos = after;
    }
    None
}

fn skip_pdf_ws(bytes: &[u8]) -> &[u8] {
    let n = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    &bytes[n..]
}

fn parse_pdf_u32(bytes: &[u8]) -> Option<(u32, &[u8])> {
    let n = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if n == 0 {
        return None;
    }
    let value = std::str::from_utf8(&bytes[..n]).ok()?.parse().ok()?;
    Some((value, &bytes[n..]))
}
