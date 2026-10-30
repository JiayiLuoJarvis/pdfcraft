//! Page text, `MediaBox`, dash operands, and link annotations.

use std::process::Command;

use pdfcraft_orgchart::{
    generate_pdf, ChartOptions, ChartRequest, Edge, FontBytes, Node, NodeType,
};

fn hc_font() -> Option<FontBytes> {
    let path = std::env::var_os("PDFCRAFT_ORGCHART_FONT")
        .or_else(|| std::env::var_os("PDFCRAFT_CJK_FONT"))?;
    Some(FontBytes::new(
        std::fs::read(path).expect("read orgchart font"),
    ))
}

fn pdf_of(request: &ChartRequest) -> Option<Vec<u8>> {
    let font = hc_font()?;
    match generate_pdf(request, &font) {
        Ok(pdf) => Some(pdf),
        Err(error) if error.code == pdfcraft_orgchart::ErrorCode::MissingGlyph => None,
        Err(error) => panic!("pdf: {error}"),
    }
}

fn text_of(pdf: &[u8], name: &str) -> String {
    let mut path = std::env::temp_dir();
    path.push(name);
    std::fs::write(&path, pdf).expect("write");
    let text = Command::new("pdftotext")
        .arg("-layout")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext is required");
    assert!(text.status.success(), "pdftotext failed");
    String::from_utf8_lossy(&text.stdout).into_owned()
}

fn real(object: &lopdf::Object) -> f64 {
    match object {
        lopdf::Object::Integer(value) => f64::from(i32::try_from(*value).unwrap_or(i32::MAX)),
        lopdf::Object::Real(value) => f64::from(*value),
        other => panic!("number {other:?}"),
    }
}

fn page_ids(doc: &lopdf::Document) -> Vec<lopdf::ObjectId> {
    doc.page_iter().collect()
}

fn media_box(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> [f64; 4] {
    let object = doc.get_object(page_id).expect("page");
    let dict = object.as_dict().expect("page dict");
    let array = dict
        .get(b"MediaBox")
        .expect("mediabox")
        .as_array()
        .expect("box");
    [
        real(&array[0]),
        real(&array[1]),
        real(&array[2]),
        real(&array[3]),
    ]
}

fn annotation_objects(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> Vec<&lopdf::Dictionary> {
    let object = doc.get_object(page_id).expect("page");
    let dict = object.as_dict().expect("page dict");
    let Ok(annots) = dict.get(b"Annots") else {
        return Vec::new();
    };
    let items = match annots {
        lopdf::Object::Array(items) => items.as_slice(),
        lopdf::Object::Reference(id) => {
            let Some(object) = doc.get_object(*id).ok() else {
                return Vec::new();
            };
            let Ok(items) = object.as_array() else {
                return Vec::new();
            };
            items.as_slice()
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| match item {
            lopdf::Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok(),
            lopdf::Object::Dictionary(dict) => Some(dict),
            _ => None,
        })
        .collect()
}

fn annotation_count(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> usize {
    annotation_objects(doc, page_id).len()
}

fn link_target_page(doc: &lopdf::Document, annot: &lopdf::Dictionary) -> lopdf::ObjectId {
    let dest = annot.get(b"Dest").expect("link dest");
    let dest = match dest {
        lopdf::Object::Reference(id) => doc.get_object(*id).expect("dest object"),
        other => other,
    };
    let array = dest
        .as_array()
        .unwrap_or_else(|_| panic!("dest array: {dest:?}"));
    match array.first() {
        Some(lopdf::Object::Reference(id)) => *id,
        other => panic!("dest page {other:?} in {dest:?}"),
    }
}

fn has_dash_4_3(content: &str) -> bool {
    let mut rest = content;
    while let Some(open) = rest.find('[') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find(']') else {
            return false;
        };
        let inside = &rest[..close];
        let nums: Vec<f64> = inside
            .split_whitespace()
            .filter_map(|item| item.parse().ok())
            .collect();
        let after = rest[close + 1..].trim_start();
        if nums.len() >= 2
            && (nums[0] - 4.0).abs() < 0.05
            && (nums[1] - 3.0).abs() < 0.05
            && (after.starts_with('d') || after.starts_with("0 d") || after.starts_with("0d"))
        {
            return true;
        }
        rest = &rest[close + 1..];
    }
    false
}

fn person(id: &str, name: &str, ratio: f64) -> Node {
    Node {
        ratio: Some(ratio),
        node_type: Some(NodeType::Person),
        ..Node::new(id, name)
    }
}

fn holding(from: &str, to: &str, ratio: f64) -> Edge {
    Edge {
        ratio: Some(ratio),
        ..Edge::new(from, to)
    }
}

#[test]
fn one_portrait_page_has_a4_mediabox_role_text_and_no_links() {
    let Some(pdf) = pdf_of(&ChartRequest::new(
        "root",
        vec![Node::new("holder", "甲股东"), Node::new("root", "主体公司")],
        vec![Edge::new("holder", "root")],
    )) else {
        return;
    };
    let doc = lopdf::Document::load_mem(&pdf).expect("pdf");
    let pages = page_ids(&doc);
    assert_eq!(pages.len(), 1);
    let [x0, y0, x1, y1] = media_box(&doc, pages[0]);
    let width = x1 - x0;
    let height = y1 - y0;
    assert!((width - 210.0 * 72.0 / 25.4).abs() < 0.5, "{width}");
    assert!((height - 297.0 * 72.0 / 25.4).abs() < 0.5, "{height}");
    assert_eq!(annotation_count(&doc, pages[0]), 0);
    let body = text_of(&pdf, "pdfcraft-orgchart-one-page.pdf");
    assert!(body.contains("股权架构图"), "{body}");
    assert!(body.contains("主体公司"), "{body}");
    assert!(body.contains("第 1 / 1 頁"), "{body}");
    assert!(!body.contains("详见"), "{body}");
    assert!(!body.contains("附件"), "{body}");
}

#[test]
fn a_folded_holder_name_appears_and_the_dropped_names_do_not() {
    let mut request = ChartRequest::new(
        "root",
        vec![
            person("s1", "甲", 40.0),
            person("s2", "乙", 30.0),
            person("s3", "丙", 20.0),
            person("s4", "丁", 10.0),
            Node::new("root", "主体公司"),
        ],
        vec![
            holding("s1", "root", 40.0),
            holding("s2", "root", 30.0),
            holding("s3", "root", 20.0),
            holding("s4", "root", 10.0),
        ],
    );
    request.options = ChartOptions {
        top_n: Some(2),
        ..ChartOptions::default()
    };
    let Some(pdf) = pdf_of(&request) else {
        return;
    };
    let body = text_of(&pdf, "pdfcraft-orgchart-fold.pdf");
    assert!(body.contains("其他股东(2家)"), "{body}");
    assert!(body.contains("甲"), "{body}");
    assert!(!body.contains("丙"), "{body}");
    assert!(!body.contains("丁"), "{body}");
}

#[test]
fn an_overflow_page_uses_dash_4_3_and_in_range_link_targets() {
    let mut nodes = vec![Node::new("root", "主体公司")];
    let mut edges = Vec::new();
    let mut held = "root".to_owned();
    for index in 0..16 {
        let id = format!("a{index}");
        nodes.push(Node {
            legal_person: Some("法人".to_owned()),
            ..Node::new(&id, format!("上层{index}"))
        });
        edges.push(Edge::new(&id, &held));
        held = id;
    }
    let Some(pdf) = pdf_of(&ChartRequest::new("root", nodes, edges)) else {
        return;
    };
    let doc = lopdf::Document::load_mem(&pdf).expect("pdf");
    let pages = page_ids(&doc);
    assert!(
        pages.len() > 1,
        "expected a split chart, got {}",
        pages.len()
    );
    let mut saw_dash = false;
    let mut link_count = 0_usize;
    for page_id in &pages {
        let bytes = doc.get_page_content(*page_id);
        let content = String::from_utf8_lossy(&bytes);
        saw_dash |= has_dash_4_3(&content);
        for annot in annotation_objects(&doc, *page_id) {
            let target = link_target_page(&doc, annot);
            assert!(
                pages.contains(&target),
                "link target {target:?} is outside {pages:?}"
            );
            link_count += 1;
        }
    }
    assert!(saw_dash, "missing 4/3 dash");
    assert!(link_count > 0, "split chart has no link annotations");
}
