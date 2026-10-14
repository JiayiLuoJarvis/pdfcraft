//! `generate_pdf` writes a PDF whose text includes the role header.

use std::path::PathBuf;
use std::process::Command;

use quire_orgchart::{generate_pdf, ChartRequest, Edge, FontBytes, Node};

fn hc_font() -> FontBytes {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fonts/SourceHanSerifHC-ExtraLight.ttf");
    FontBytes::new(
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
}

#[test]
fn a_small_chart_starts_with_pdf_and_names_the_role() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("holder", "甲股东"),
            Node::new("root", "主体公司"),
            Node::new("invest", "乙投资"),
        ],
        vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
    );
    let pdf = generate_pdf(&request, &hc_font()).expect("pdf");
    assert!(pdf.starts_with(b"%PDF"));
    let mut path = std::env::temp_dir();
    path.push("quire-orgchart-small.pdf");
    std::fs::write(&path, &pdf).expect("write");
    let text = Command::new("pdftotext")
        .arg("-layout")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext is required");
    assert!(text.status.success(), "pdftotext failed");
    let body = String::from_utf8_lossy(&text.stdout);
    assert!(body.contains("股权架构图"), "{body}");
    assert!(body.contains("主体公司"), "{body}");
}
