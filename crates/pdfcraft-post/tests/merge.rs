//! Integration tests for pdfcraft-post.

#[allow(
    dead_code,
    reason = "shared helpers may be unused in some test targets"
)]
mod common;

use lopdf::Document;
use pdfcraft_post::{merge_pdfs, Encrypt, EncryptionPreset, ErrorKind, Pipeline, Step};

#[test]
fn empty_parts_fail() {
    let err = merge_pdfs(Vec::<Vec<u8>>::new(), false).expect_err("empty");
    assert_eq!(err.kind, ErrorKind::MergeEmpty);
}

#[test]
fn single_part_is_identity() {
    let pdf = common::sample_pdf(Some("one"), None);
    let out = merge_pdfs([pdf.as_slice()], false).expect("merge");
    assert_eq!(out, pdf);
}

#[test]
fn two_parts_yield_two_pages() {
    let a = common::sample_pdf(Some("A"), None);
    let b = common::sample_pdf(Some("B"), None);
    let out = merge_pdfs([a.as_slice(), b.as_slice()], false).expect("merge");
    assert!(out.starts_with(b"%PDF"));
    let doc = Document::load_mem(&out).expect("open");
    assert_eq!(doc.get_pages().len(), 2);
}

#[test]
fn three_parts_yield_three_pages() {
    let parts: Vec<Vec<u8>> = (0..3)
        .map(|i| common::sample_pdf(Some(&format!("p{i}")), None))
        .collect();
    let out = merge_pdfs(parts.iter().map(Vec::as_slice), false).expect("merge");
    let doc = Document::load_mem(&out).expect("open");
    assert_eq!(doc.get_pages().len(), 3);
}

#[test]
fn page_order_follows_part_order() {
    let a = common::pages_with_marks(&["A0", "A1"]);
    let b = common::pages_with_marks(&["B0"]);
    let out = merge_pdfs([a.as_slice(), b.as_slice()], false).expect("merge");
    let texts = common::page_texts(&out);
    assert_eq!(texts.len(), 3);
    assert!(texts[0].contains("A0"), "{texts:?}");
    assert!(texts[1].contains("A1"), "{texts:?}");
    assert!(texts[2].contains("B0"), "{texts:?}");
}

#[test]
fn default_merge_drops_outlines() {
    let a = common::pdf_with_outline("Gone");
    let b = common::sample_pdf(None, None);
    let out = merge_pdfs([a.as_slice(), b.as_slice()], false).expect("merge");
    assert!(common::outline_titles(&out).is_empty());
    assert_eq!(Document::load_mem(&out).expect("open").get_pages().len(), 2);
}

#[test]
fn preserve_outlines_keeps_only_the_first() {
    let a = common::pdf_with_outline("First");
    let b = common::pdf_with_outline("Second");
    let out = merge_pdfs([a.as_slice(), b.as_slice()], true).expect("merge");
    assert_eq!(common::outline_titles(&out), vec!["First".to_string()]);
    assert_eq!(Document::load_mem(&out).expect("open").get_pages().len(), 2);
}

#[test]
fn bad_part_is_not_a_pdf() {
    let a = common::sample_pdf(None, None);
    let err = merge_pdfs([a.as_slice(), b"not-a-pdf".as_slice()], false).expect_err("bad");
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}

#[test]
fn preserve_outlines_false_still_merges_pages() {
    let a = common::sample_pdf(None, None);
    let b = common::sample_pdf(None, None);
    let out = merge_pdfs([a.as_slice(), b.as_slice()], true).expect("merge");
    let doc = Document::load_mem(&out).expect("open");
    assert_eq!(doc.get_pages().len(), 2);
}

#[test]
fn merge_then_encrypt_via_pipeline() {
    let a = common::sample_pdf(None, None);
    let b = common::sample_pdf(None, None);
    let merged = merge_pdfs([a.as_slice(), b.as_slice()], false).expect("merge");
    let out = Pipeline::new([Step::Encrypt(Encrypt {
        config: Some(EncryptionPreset::READ_ONLY_PRINTABLE),
        owner_password: Some("owner-secret".into()),
        user_password: None,
    })])
    .run(&merged)
    .expect("encrypt");
    let doc = common::open_with_password(&out, "").expect("open");
    assert!(common::is_encrypted_pdf(&out));
    assert_eq!(doc.get_pages().len(), 2);
}
