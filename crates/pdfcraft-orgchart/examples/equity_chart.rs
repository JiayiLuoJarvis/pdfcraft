//! Draw a small equity chart to `out/equity_chart.pdf`.

use std::path::PathBuf;

use pdfcraft_orgchart::{generate_pdf, ChartRequest, Edge, FontBytes, Node};

fn main() {
    let font_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fonts/SourceHanSerifHC-ExtraLight.ttf");
    let font = FontBytes::new(
        std::fs::read(&font_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", font_path.display())),
    );
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("holder", "甲股东"),
            Node::new("root", "主体公司"),
            Node::new("invest", "乙投资"),
        ],
        vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
    );
    let pdf = generate_pdf(&request, &font).expect("chart");
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/equity_chart.pdf");
    std::fs::create_dir_all(out.parent().expect("parent")).expect("mkdir");
    std::fs::write(&out, &pdf).expect("write");
    println!("wrote {}", out.display());
}
