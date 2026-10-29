//! Integration tests for `split_pdfs`.

#[allow(
    dead_code,
    reason = "shared helpers may be unused in some test targets"
)]
mod common;

use std::fs;
use std::path::PathBuf;

use lopdf::Document;
use pdfcraft_post::{merge_pdfs, split_pdfs, Encrypt, EncryptionPreset, ErrorKind};

fn seeds_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/seeds")
}

fn read_seed(name: &str) -> Vec<u8> {
    fs::read(seeds_dir().join(name))
        .unwrap_or_else(|error| panic!("read split seed {name}: {error}"))
}

fn assert_single_page_output(out: &[u8]) -> Document {
    assert!(
        out.starts_with(b"%PDF"),
        "split output must start with %PDF"
    );
    let doc = Document::load_mem(out).expect("split output must load");
    assert_eq!(doc.get_pages().len(), 1, "split output holds one page");
    doc
}

#[test]
fn empty_splits_to_empty() {
    // `Count` 0 with empty `Kids`: prove once that `load_mem` accepts the
    // fixture, then require the split of it to be `Ok` with no outputs.
    let empty = common::pages_with_marks(&[]);
    let doc = Document::load_mem(&empty).expect("empty fixture must load");
    assert_eq!(doc.get_pages().len(), 0, "fixture holds no pages");
    let out = split_pdfs(&empty).expect("empty splits to empty");
    assert!(out.is_empty(), "no pages means no outputs");
}

#[test]
fn single_page_splits_to_one_identical_page() {
    let pdf = common::pages_with_marks(&["solo"]);
    let out = split_pdfs(&pdf).expect("split single page");
    assert_eq!(out.len(), 1, "one page means one output");
    let split_doc = assert_single_page_output(&out[0]);

    let src_doc = Document::load_mem(&pdf).expect("source must load");
    let src_id = src_doc.page_iter().next().expect("source has one page");
    let split_id = split_doc.page_iter().next().expect("output has one page");
    assert_eq!(
        split_doc.get_page_content(split_id),
        src_doc.get_page_content(src_id),
        "single-page split keeps the page content identical"
    );
    assert_eq!(
        common::page_texts(&out[0]),
        common::page_texts(&pdf),
        "single-page split keeps the readable text identical"
    );
}

#[test]
fn five_pages_split_in_order() {
    let marks = ["m0", "m1", "m2", "m3", "m4"];
    let pdf = common::pages_with_marks(&marks);
    let out = split_pdfs(&pdf).expect("split five pages");
    assert_eq!(out.len(), 5, "five pages mean five outputs");
    for (index, (bytes, mark)) in out.iter().zip(marks.iter()).enumerate() {
        assert_single_page_output(bytes);
        let texts = common::page_texts(bytes);
        assert_eq!(texts.len(), 1, "output {index} holds one page");
        assert!(
            texts[0].contains(mark),
            "output {index} must contain {mark}: {texts:?}"
        );
    }
}

#[test]
fn third_output_is_third_page() {
    let marks = ["first", "second", "third", "fourth"];
    let pdf = common::pages_with_marks(&marks);
    let out = split_pdfs(&pdf).expect("split four pages");
    assert_eq!(out.len(), 4);
    let texts = common::page_texts(&out[2]);
    assert_eq!(texts.len(), 1);
    assert!(
        texts[0].contains("third"),
        "third output must be the third page: {texts:?}"
    );
    assert!(
        !texts[0].contains("first") && !texts[0].contains("second"),
        "third output must not leak other pages: {texts:?}"
    );

    let src_doc = Document::load_mem(&pdf).expect("source must load");
    let src_ids: Vec<_> = src_doc.page_iter().collect();
    let split_doc = Document::load_mem(&out[2]).expect("third output must load");
    let split_id = split_doc.page_iter().next().expect("one page");
    assert_eq!(
        split_doc.get_page_content(split_id),
        src_doc.get_page_content(src_ids[2]),
        "third output keeps the third page content identical"
    );
}

#[test]
fn split_then_merge_round_trip() {
    let marks = ["r0", "r1", "r2", "r3", "r4"];
    let pdf = common::pages_with_marks(&marks);
    let parts = split_pdfs(&pdf).expect("split");
    assert_eq!(parts.len(), marks.len());
    let merged = merge_pdfs(parts.iter().map(Vec::as_slice), false).expect("merge split outputs");
    let merged_doc = Document::load_mem(&merged).expect("merged must load");
    assert_eq!(merged_doc.get_pages().len(), marks.len());
    assert_eq!(
        common::page_texts(&merged),
        common::page_texts(&pdf),
        "split-then-merge keeps page count and order"
    );
}

#[test]
fn encrypted_with_empty_user_is_rejected() {
    // Same anchor as the merge gate: `load_mem` auto-decrypts the empty-user
    // fixture, so the split must reject on the `was_encrypted` branch rather
    // than re-saving auto-decrypted bytes.
    let plain = common::pages_with_marks(&["secret"]);
    let enc = common::encrypted_with_empty_user(&plain);
    let direct = Document::load_mem(&enc).expect("fixture auto-decrypts");
    assert!(
        !direct.is_encrypted(),
        "load_mem auto-decrypts empty-user input"
    );
    assert!(
        direct.was_encrypted(),
        "fixture must exercise the was_encrypted branch"
    );
    let err = split_pdfs(&enc).expect_err("encrypted input");
    assert_eq!(err.kind, ErrorKind::Encrypted);
}

#[test]
fn encrypted_with_user_password_is_rejected() {
    let plain = common::pages_with_marks(&["locked"]);
    let enc = Encrypt {
        config: Some(EncryptionPreset::READ_ONLY),
        owner_password: Some("owner-secret".into()),
        user_password: Some("user-secret".into()),
    }
    .apply(&plain)
    .expect("encrypt fixture");
    assert_eq!(
        common::open_with_password(&enc, "user-secret")
            .expect("fixture opens with user password")
            .get_pages()
            .len(),
        1
    );
    let err = split_pdfs(&enc).expect_err("password-locked input");
    assert_eq!(err.kind, ErrorKind::Encrypted);
}

#[test]
fn not_a_pdf_seed_is_rejected() {
    let seed = read_seed("p03-not-a-pdf.bin");
    assert!(
        !seed.starts_with(b"%PDF"),
        "p03 seed must not look like a PDF"
    );
    let err = split_pdfs(&seed).expect_err("p03-not-a-pdf.bin");
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}
