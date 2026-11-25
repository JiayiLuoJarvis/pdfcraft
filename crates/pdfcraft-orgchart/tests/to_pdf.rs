//! `Chart::to_pdf` writes a PDF whose text includes the role header.

use std::process::Command;

use pdfcraft_orgchart::{Chart, ChartRequest, Edge, Font, Node, PaidIn};

mod common;

fn hc_font() -> Option<Font> {
    common::env_font()
}

/// `pdftotext -layout` often drops ASCII spaces between CJK runs.
fn pdftotext_has(body: &str, needle: &str) -> bool {
    let compact = |text: &str| {
        text.chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
    };
    compact(body).contains(&compact(needle))
}

fn draw(request: ChartRequest, font: &Font) -> Result<Vec<u8>, pdfcraft_orgchart::Error> {
    Chart::new(request)?.to_pdf(font)
}

#[test]
fn a_small_chart_starts_with_pdf_and_names_the_role() {
    // Equity-only cone so the page title is `股权架构图` (structure-spec §11).
    let request = ChartRequest::new(
        "root",
        vec![Node::new("holder", "甲股东"), Node::new("root", "主体公司")],
        vec![Edge::new("holder", "root")],
    );
    let Some(font) = hc_font() else {
        return;
    };
    let pdf = match draw(request, &font) {
        Ok(pdf) => pdf,
        Err(error) if common::skip_missing_glyph(&error) => return,
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
    assert!(pdftotext_has(&body, "股权架构图"), "{body}");
    assert!(pdftotext_has(&body, "主体公司"), "{body}");
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
    let pdf = match draw(request, &font) {
        Ok(pdf) => pdf,
        Err(error) if common::skip_missing_glyph(&error) => return,
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
    let chart = match Chart::new(multi_page_request()) {
        Ok(chart) => chart,
        Err(error) => panic!("chart: {error}"),
    };
    let first = match chart.to_pdf(&font) {
        Ok(pdf) => pdf,
        Err(error) if common::skip_missing_glyph(&error) => return,
        Err(error) => panic!("pdf: {error}"),
    };
    let second = chart.to_pdf(&font).expect("second pdf");
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

#[test]
fn mixed_names_and_investment_fields_are_extractable() {
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("short", "甲"),
            Node::new("long", "北京某某股份有限公司"),
            Node::new("root", "主体公司"),
            Node {
                paid_in: Some(PaidIn::new("100万")),
                location: Some("深圳".to_owned()),
                approval_date: Some("2024-06-01".to_owned()),
                ..Node::new("c1", "深圳子公司")
            },
            Node {
                paid_in: Some(PaidIn::new("200万")),
                location: Some("广州".to_owned()),
                approval_date: Some("2023-01-15".to_owned()),
                ..Node::new("c2", "广州科技发展有限公司")
            },
            Node {
                paid_in: Some(PaidIn::new("50万")),
                location: Some("杭州".to_owned()),
                approval_date: Some("2022-12-31".to_owned()),
                ..Node::new("c3", "杭州合伙企业")
            },
            Node {
                paid_in: Some(PaidIn::new("10万")),
                location: Some("上海".to_owned()),
                approval_date: Some("2021-05-20".to_owned()),
                ..Node::new("c4", "乙")
            },
        ],
        vec![
            Edge::new("short", "root"),
            Edge::new("long", "root"),
            Edge::new("root", "c1"),
            Edge::new("root", "c2"),
            Edge::new("root", "c3"),
            Edge::new("root", "c4"),
        ],
    );
    let Some(font) = hc_font() else {
        return;
    };
    let pdf = match draw(request, &font) {
        Ok(pdf) => pdf,
        Err(error) if common::skip_missing_glyph(&error) => return,
        Err(error) => panic!("pdf: {error}"),
    };
    assert!(pdf.starts_with(b"%PDF"));
    let mut path = std::env::temp_dir();
    path.push("pdfcraft-orgchart-mixed.pdf");
    std::fs::write(&path, &pdf).expect("write");
    let text = Command::new("pdftotext")
        .arg("-layout")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext is required");
    assert!(text.status.success(), "pdftotext failed");
    let body = String::from_utf8_lossy(&text.stdout);
    assert!(pdftotext_has(&body, "主体公司"), "{body}");
    assert!(pdftotext_has(&body, "甲"), "{body}");
    assert!(pdftotext_has(&body, "北京某某股份有限公司"), "{body}");
    assert!(pdftotext_has(&body, "深圳 · 2024-06-01"), "{body}");
    assert!(pdftotext_has(&body, "实缴 100万"), "{body}");
    assert!(pdftotext_has(&body, "广州科技发展有限公司"), "{body}");
}
