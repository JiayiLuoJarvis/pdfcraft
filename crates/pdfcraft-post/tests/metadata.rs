//! Integration tests for pdfcraft-post.

#[allow(dead_code)]
mod common;

use pdfcraft_post::{ErrorKind, SetMetadata};

#[test]
fn writes_title_and_author() {
    let pdf = common::sample_pdf(None, None);
    let out = SetMetadata {
        title: Some("Letter".into()),
        author: Some("Ada".into()),
        ..SetMetadata::default()
    }
    .apply(&pdf)
    .expect("apply");
    assert!(out.starts_with(b"%PDF"));
    assert_eq!(
        common::info_string(&out, b"Title").as_deref(),
        Some("Letter")
    );
    assert_eq!(common::info_string(&out, b"Author").as_deref(), Some("Ada"));
}

#[test]
fn leaves_unset_creator_alone() {
    let pdf = common::sample_pdf(Some("Old"), Some("KeepMe"));
    let out = SetMetadata {
        title: Some("New".into()),
        ..SetMetadata::default()
    }
    .apply(&pdf)
    .expect("apply");
    assert_eq!(common::info_string(&out, b"Title").as_deref(), Some("New"));
    assert_eq!(
        common::info_string(&out, b"Creator").as_deref(),
        Some("KeepMe")
    );
}

#[test]
fn writes_creator_and_producer() {
    let pdf = common::sample_pdf(None, None);
    let out = SetMetadata {
        creator: Some("post-creator".into()),
        producer: Some("post-producer".into()),
        ..SetMetadata::default()
    }
    .apply(&pdf)
    .expect("apply");
    assert_eq!(
        common::info_string(&out, b"Creator").as_deref(),
        Some("post-creator")
    );
    assert_eq!(
        common::info_string(&out, b"Producer").as_deref(),
        Some("post-producer")
    );
}

#[test]
fn joins_keywords_with_commas() {
    let pdf = common::sample_pdf(None, None);
    let out = SetMetadata {
        keywords: Some(vec!["a".into(), "b".into(), "c".into()]),
        ..SetMetadata::default()
    }
    .apply(&pdf)
    .expect("apply");
    assert_eq!(
        common::info_string(&out, b"Keywords").as_deref(),
        Some("a, b, c")
    );
}

#[test]
fn writes_pdf_date() {
    let pdf = common::sample_pdf(None, None);
    let out = SetMetadata {
        creation_date: Some("D:20240102030405".into()),
        modification_date: Some("D:20240102030405+08'00'".into()),
        ..SetMetadata::default()
    }
    .apply(&pdf)
    .expect("apply");
    assert_eq!(
        common::info_string(&out, b"CreationDate").as_deref(),
        Some("D:20240102030405")
    );
    assert_eq!(
        common::info_string(&out, b"ModDate").as_deref(),
        Some("D:20240102030405+08'00'")
    );
}

#[test]
fn rejects_a_date_that_is_not_pdf_syntax() {
    let pdf = common::sample_pdf(None, None);
    let err = SetMetadata {
        creation_date: Some("2024-01-02".into()),
        ..SetMetadata::default()
    }
    .apply(&pdf)
    .expect_err("date");
    assert_eq!(err.kind, ErrorKind::MetadataFailed);
}

#[test]
fn empty_keywords_clear_the_field() {
    let pdf = common::sample_pdf(None, None);
    let first = SetMetadata {
        keywords: Some(vec!["lease".into()]),
        ..SetMetadata::default()
    }
    .apply(&pdf)
    .expect("first");
    let second = SetMetadata {
        keywords: Some(vec![]),
        ..SetMetadata::default()
    }
    .apply(&first)
    .expect("second");
    assert_eq!(
        common::info_string(&second, b"Keywords").as_deref(),
        Some("")
    );
}

#[test]
fn invalid_bytes_are_not_a_pdf() {
    let err = SetMetadata::default()
        .apply(b"not-a-pdf")
        .expect_err("must fail");
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}
