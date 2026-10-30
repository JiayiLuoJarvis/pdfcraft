//! Node strokes are black. Edge strokes and edge-arrow fills are byte 51.

use pdfcraft_orgchart::{generate_pdf, ChartRequest, Edge, FontBytes, Node};

fn hc_font() -> Option<FontBytes> {
    let path = std::env::var_os("PDFCRAFT_ORGCHART_FONT")
        .or_else(|| std::env::var_os("PDFCRAFT_CJK_FONT"))?;
    Some(FontBytes::new(
        std::fs::read(path).expect("read orgchart font"),
    ))
}

fn byte_channel(component: f64) -> u8 {
    let scaled = (component * 255.0).round();
    let mut byte = 0_u8;
    let mut cursor = 0.0;
    while cursor + 1.0 <= scaled && byte < 255 {
        cursor += 1.0;
        byte = byte.saturating_add(1);
    }
    byte
}

fn near(width: f64, expected: f64) -> bool {
    (width - expected).abs() < 0.05
}

/// Stroke operations as `(line width in pt, red, green, blue)` after decoding the page.
fn stroked(content: &str) -> Vec<(f64, [u8; 3])> {
    let tokens: Vec<&str> = content.split_whitespace().collect();
    let mut color = [0_u8; 3];
    let mut width = 0.0;
    let mut have_color = false;
    let mut out = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if *token == "RG" && index >= 3 {
            let parsed = [
                tokens[index - 3].parse::<f64>(),
                tokens[index - 2].parse::<f64>(),
                tokens[index - 1].parse::<f64>(),
            ];
            if let [Ok(red), Ok(green), Ok(blue)] = parsed {
                color = [byte_channel(red), byte_channel(green), byte_channel(blue)];
                have_color = true;
            }
        } else if *token == "w" && index >= 1 {
            if let Ok(parsed) = tokens[index - 1].parse::<f64>() {
                width = parsed;
            }
        } else if *token == "S" && have_color {
            out.push((width, color));
        }
    }
    out
}

fn filled(content: &str) -> Vec<[u8; 3]> {
    let tokens: Vec<&str> = content.split_whitespace().collect();
    let mut out = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if *token == "rg" && index >= 3 {
            let parsed = [
                tokens[index - 3].parse::<f64>(),
                tokens[index - 2].parse::<f64>(),
                tokens[index - 1].parse::<f64>(),
            ];
            if let [Ok(red), Ok(green), Ok(blue)] = parsed {
                out.push([byte_channel(red), byte_channel(green), byte_channel(blue)]);
            }
        }
    }
    out
}

#[test]
fn node_strokes_are_black_and_edge_strokes_are_byte_51() {
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
    let doc = lopdf::Document::load_mem(&pdf).expect("pdf");
    let page_id = doc.page_iter().next().expect("page");
    let bytes = doc.get_page_content(page_id);
    let content = String::from_utf8_lossy(&bytes);
    let strokes = stroked(&content);
    let node_strokes: Vec<_> = strokes
        .iter()
        .filter(|(width, _)| near(*width, 0.8) || near(*width, 1.5))
        .copied()
        .collect();
    let edge_strokes: Vec<_> = strokes
        .iter()
        .filter(|(width, _)| near(*width, 0.7))
        .copied()
        .collect();
    assert!(!node_strokes.is_empty(), "no node stroke in {content}");
    assert!(!edge_strokes.is_empty(), "no edge stroke in {content}");
    assert!(
        node_strokes.iter().all(|(_, color)| *color == [0, 0, 0]),
        "node strokes {node_strokes:?}"
    );
    assert!(
        edge_strokes.iter().all(|(_, color)| *color == [51, 51, 51]),
        "edge strokes {edge_strokes:?}"
    );
    assert!(
        strokes.iter().all(|(_, color)| *color != [20, 20, 20]),
        "old gray stroke still present {strokes:?}"
    );
    let fills = filled(&content);
    assert!(
        fills.contains(&[51, 51, 51]),
        "edge arrow fill missing {fills:?}"
    );
}
