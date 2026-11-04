//! `generate_pdf` writes a PDF whose text includes the role header.

#![expect(
    clippy::expect_used,
    reason = "integration test: a panic is the failure report"
)]

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

fn ratio_node(id: &str, ratio: f64) -> Node {
    Node {
        ratio: Some(ratio),
        ..Node::new(id, id)
    }
}

fn edge_with_ratio(from: &str, to: &str, ratio: f64) -> Edge {
    Edge {
        ratio: Some(ratio),
        ..Edge::new(from, to)
    }
}

/// 30 holders, two upstreams each, and nine investments. Review measured 33 pages.
fn multi_page_request() -> ChartRequest {
    let mut nodes = vec![Node::new("root", "主体公司")];
    let mut edges = Vec::new();
    for index in 0..30 {
        let holder = format!("h{index}");
        nodes.push(ratio_node(&holder, 1.0 + f64::from(index) / 10.0));
        edges.push(edge_with_ratio(&holder, "root", 1.0));
        for sub in 0..2 {
            let upstream = format!("h{index}u{sub}");
            nodes.push(ratio_node(&upstream, 50.0));
            edges.push(edge_with_ratio(&upstream, &holder, 50.0));
        }
    }
    for index in 0..9 {
        let held = format!("c{index}");
        nodes.push(ratio_node(&held, 60.0));
        edges.push(edge_with_ratio("root", &held, 60.0));
    }
    ChartRequest::new("root", nodes, edges)
}

#[test]
fn the_same_multipage_request_twice_yields_identical_bytes() {
    let Some(font) = hc_font() else {
        return;
    };
    let request = multi_page_request();
    let first = match generate_pdf(&request, &font) {
        Ok(pdf) => pdf,
        Err(error) if error.code == pdfcraft_orgchart::ErrorCode::MissingGlyph => return,
        Err(error) => panic!("pdf: {error}"),
    };
    let second = generate_pdf(&request, &font).expect("second pdf");
    let pages = lopdf::Document::load_mem(&first)
        .expect("pdf")
        .page_iter()
        .count();
    assert!(
        pages > 1,
        "fixture must span more than one page, got {pages}"
    );
    assert_eq!(first, second);
}
