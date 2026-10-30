//! `generate_pdf` writes a PDF whose text includes the role header.

use std::process::Command;

use pdfcraft_orgchart::{generate_pdf, ChartRequest, Edge, FontBytes, Node};

fn hc_font() -> Option<FontBytes> {
    let path = std::env::var_os("PDFCRAFT_ORGCHART_FONT")
        .or_else(|| std::env::var_os("PDFCRAFT_CJK_FONT"))?;
    Some(FontBytes::new(
        std::fs::read(path).expect("read orgchart font"),
    ))
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
    let Some(font) = hc_font() else {
        return;
    };
    let pdf = match generate_pdf(&request, &font) {
        Ok(pdf) => pdf,
        Err(error) if error.code == pdfcraft_orgchart::ErrorCode::MissingGlyph => return,
        Err(error) => panic!("pdf: {error}"),
    };
    assert!(pdf.starts_with(b"%PDF"));
    let mut path = std::env::temp_dir();
    path.push("pdfcraft-orgchart-small.pdf");
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

#[test]
fn a_vertical_stub_prints_forty_percent() {
    let mut holding = Edge::new("holder", "root");
    holding.ratio = Some(40.0);
    let request = ChartRequest::new(
        "root",
        vec![Node::new("holder", "甲股东"), Node::new("root", "主体公司")],
        vec![holding],
    );
    let Some(font) = hc_font() else {
        return;
    };
    let pdf = match generate_pdf(&request, &font) {
        Ok(pdf) => pdf,
        Err(error) if error.code == pdfcraft_orgchart::ErrorCode::MissingGlyph => return,
        Err(error) => panic!("pdf: {error}"),
    };
    let mut path = std::env::temp_dir();
    path.push("pdfcraft-orgchart-ratio.pdf");
    std::fs::write(&path, &pdf).expect("write");
    let text = Command::new("pdftotext")
        .arg("-layout")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext is required");
    assert!(text.status.success(), "pdftotext failed");
    let body = String::from_utf8_lossy(&text.stdout);
    assert!(body.contains("40.00%"), "{body}");
}
