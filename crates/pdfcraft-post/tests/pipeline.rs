//! Integration tests for pdfcraft-post.

#[allow(
    dead_code,
    reason = "shared helpers may be unused in some test targets"
)]
mod common;

use pdfcraft_post::{Pipeline, SetMetadata, Step};

#[test]
fn empty_pipeline_returns_input_unchanged() {
    let pdf = common::sample_pdf(Some("t"), None);
    let out = Pipeline::new([]).run(&pdf).expect("run");
    assert_eq!(out, pdf);
}

#[test]
fn pipeline_runs_metadata_step() {
    let pdf = common::sample_pdf(None, None);
    let pipe = Pipeline::new([Step::Metadata(SetMetadata {
        title: Some("piped".into()),
        ..SetMetadata::default()
    })]);
    let out = pipe.run(&pdf).expect("run");
    assert_eq!(
        common::info_string(&out, b"Title").as_deref(),
        Some("piped")
    );
}
