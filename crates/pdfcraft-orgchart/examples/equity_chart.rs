//! Draw a small equity chart to `out/equity_chart.pdf`.
//!
//! Requires `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT` (single-face TTF/OTF).
//! `Font::new` parses those file bytes. `Font::from_shared` reuses an
//! `Arc<Vec<u8>>` when the caller already shares that buffer.

#![expect(clippy::print_stdout, reason = "example reports the written PDF path")]

use std::env;
use std::path::PathBuf;

use pdfcraft_orgchart::{Chart, ChartRequest, Edge, Font, Node, PaidIn};

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
            Node::new("long", "北京某某股份有限公司"),
            Node::new("root", "主体公司"),
            Node {
                paid_in: Some(PaidIn::new("80万")),
                location: Some("珠海".to_owned()),
                approval_date: Some("2020-08-08".to_owned()),
                ..Node::new("invest", "乙投资")
            },
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
            Edge::new("holder", "root"),
            Edge::new("long", "root"),
            Edge::new("root", "invest"),
            Edge::new("root", "c1"),
            Edge::new("root", "c2"),
            Edge::new("root", "c3"),
            Edge::new("root", "c4"),
        ],
    );
    let pdf = Chart::new(request)?.to_pdf(&font)?;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/equity_chart.pdf");
    let parent = out.parent().ok_or("output path has a parent directory")?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(&out, &pdf)?;
    println!("wrote {}", out.display());
    Ok(())
}
