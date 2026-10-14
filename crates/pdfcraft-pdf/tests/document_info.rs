//! `DocumentSpec.info` reaches the PDF information dictionary.
//!
//! The check is `pdfinfo` from poppler-utils, the same package CI installs for
//! `pdftotext`. It reads `/Info` and prints `Title` and `Author`.

#![cfg(feature = "backend-krilla")]

use std::io::Write;
use std::process::{Command, Stdio};

use pdfcraft_pdf::spec::{DocumentInfo, Resources, SectionSpec};
use pdfcraft_pdf::{layout_spec, to_pdf, DocumentSpec, Limits, PageSize};

fn pdfinfo(pdf: &[u8]) -> String {
    let mut child = Command::new("pdfinfo")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("pdfinfo from poppler-utils");
    child
        .stdin
        .take()
        .expect("pdfinfo stdin")
        .write_all(pdf)
        .expect("write pdf to pdfinfo");
    let output = child.wait_with_output().expect("pdfinfo");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("pdfinfo utf-8")
}

fn field(info: &str, name: &str) -> String {
    info.lines()
        .find_map(|line| {
            let rest = line.strip_prefix(name)?;
            let rest = rest.strip_prefix(':')?;
            Some(rest.trim().to_owned())
        })
        .unwrap_or_default()
}

fn pdf_with(info: DocumentInfo) -> Vec<u8> {
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::<()>::default(),
        limits: Limits::default(),
        info,
        sections: vec![SectionSpec::<()>::default()],
    })
    .unwrap();
    to_pdf(rendered).unwrap()
}

#[test]
fn title_and_author_reach_pdf_info() {
    let text = pdfinfo(&pdf_with(DocumentInfo {
        title: Some("Quire Title".to_owned()),
        author: Some("Quire Author".to_owned()),
        ..DocumentInfo::default()
    }));
    assert_eq!(field(&text, "Title"), "Quire Title", "{text}");
    assert_eq!(field(&text, "Author"), "Quire Author", "{text}");
    assert_eq!(field(&text, "Creator"), "pdfcraft", "{text}");
    assert_eq!(field(&text, "Producer"), "", "{text}");
}

#[test]
fn empty_info_omits_title_and_author() {
    let text = pdfinfo(&pdf_with(DocumentInfo {
        title: Some(String::new()),
        author: Some(String::new()),
        ..DocumentInfo::default()
    }));
    assert_eq!(field(&text, "Title"), "", "{text}");
    assert_eq!(field(&text, "Author"), "", "{text}");
    assert_eq!(field(&text, "Creator"), "pdfcraft", "{text}");
    assert_eq!(field(&text, "Producer"), "", "{text}");
}
