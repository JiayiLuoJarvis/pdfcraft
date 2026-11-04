//! Draw a small equity chart to `out/equity_chart.pdf`.
//!
//! Requires `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT` (single-face TTF/OTF).
//! `Font::new` parses those file bytes. `Font::from_shared` reuses an
//! `Arc<Vec<u8>>` when the caller already shares that buffer.

#![expect(clippy::print_stdout, reason = "example reports the written PDF path")]

use std::env;
use std::path::PathBuf;

use pdfcraft_orgchart::{Chart, ChartRequest, Edge, Font, Node};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font_path = env::var_os("PDFCRAFT_ORGCHART_FONT")
        .or_else(|| env::var_os("PDFCRAFT_CJK_FONT"))
        .ok_or("set PDFCRAFT_ORGCHART_FONT or PDFCRAFT_CJK_FONT to a single-face .ttf")?;
    let font = Font::new(
        std::fs::read(&font_path)
            .map_err(|error| format!("read {}: {error}", PathBuf::from(&font_path).display()))?,
    )?;
    let request = ChartRequest::new(
        "root",
        vec![
            Node::new("holder", "甲股东"),
            Node::new("root", "主体公司"),
            Node::new("invest", "乙投资"),
        ],
        vec![Edge::new("holder", "root"), Edge::new("root", "invest")],
    );
    let pdf = Chart::new(request)?.to_pdf(&font)?;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/equity_chart.pdf");
    let parent = out.parent().ok_or("output path has a parent directory")?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(&out, &pdf)?;
    println!("wrote {}", out.display());
    Ok(())
}
