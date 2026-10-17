//! Reproducible wall-clock baseline for batch-readiness work item P0.
//!
//! ```text
//! cargo test --release --features test-utils --test batch_baseline -- --ignored --nocapture
//! ```
//!
//! Prints machine identity, git tip when available, and three samples:
//! letter pages, a wide table, and many short paragraphs. Each sample reports
//! layout time, `to_pdf` time, page count, PDF bytes, and peak RSS.

#![cfg(feature = "backend-krilla")]

use std::fs;
use std::process::Command;
use std::time::Instant;

use pdfcraft_pdf::spec::{
    Alignment, BandSlot, BandSpec, Block, CellBlock, CellSpec, FontFace, FontFamily, FontRegistry,
    ImageSpec, Resources, RunStyle, SectionSpec, TableSpec, TextRun,
};
use pdfcraft_pdf::{
    layout_spec, to_pdf, DocumentSpec, ImageFormat, Margins, NonNegative, PageSize, Positive,
    Ratio, Rendered, Result, Track, A4,
};

#[path = "common/mod.rs"]
mod common;

const ONE_PIXEL: &[u8] = include_bytes!("fixtures/one-pixel.png");

const BODY_1: &str =
    "敬启者：兹就贵司与本公司于二零二六年十月所签合同事项，函请确认下列款项与交货日期。";
const BODY_2: &str = "请于收函后十日内书面回复。谢谢合作。";
const MIXED: &str = "Contract C-18 / 合同编号 C-18";

#[test]
#[ignore = "P0 wall-clock baseline; run with --ignored --nocapture"]
fn batch_readiness_p0_baseline() -> Result<()> {
    println!("pdfcraft-pdf batch-readiness P0 baseline");
    println!("host={}", hostname());
    println!("uname={}", uname());
    println!("commit={}", git_tip());
    println!("page=A4 {}x{} pt", A4.width().as_pt(), A4.height().as_pt());
    println!();

    time_sample("letter_x8", || letter_doc(8))?;
    time_sample("wide_table_40x12", || wide_table_doc(40, 12))?;
    time_sample("paragraphs_400", || paragraphs_doc(400))?;
    Ok(())
}

fn time_sample(name: &str, build: impl FnOnce() -> Result<Rendered>) -> Result<()> {
    let before = peak_rss_kib();
    let start = Instant::now();
    let rendered = build()?;
    let layout_ms = start.elapsed().as_secs_f64() * 1000.0;
    let pages = rendered.pages().len();

    let start = Instant::now();
    let pdf = to_pdf(rendered)?;
    let pdf_ms = start.elapsed().as_secs_f64() * 1000.0;
    let after = peak_rss_kib();

    println!(
        "{name}: layout_ms={layout_ms:.2} to_pdf_ms={pdf_ms:.2} pages={pages} pdf_bytes={} peak_rss_kib_before={before} peak_rss_kib_after={after}",
        pdf.len()
    );
    Ok(())
}

fn letter_doc(copies: usize) -> Result<Rendered> {
    let mut resources = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(common::cjk_font_bytes().expect("PDFCRAFT_CJK_FONT"), 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }));
    let mut body = Vec::new();
    for index in 0..copies {
        if index > 0 {
            body.push(Block::PageBreak);
        }
        body.push(Block::paragraph((), BODY_1));
        body.push(Block::paragraph((), BODY_2));
        body.push(Block::paragraph((), MIXED));
        body.push(Block::Table(TableSpec {
            header: vec![vec![
                text_cell("项目"),
                text_cell("金额"),
                text_cell("交货日"),
            ]],
            body: vec![
                vec![
                    text_cell("设备"),
                    text_cell("12000"),
                    text_cell("2026-10-31"),
                ],
                vec![
                    text_cell("服务"),
                    text_cell("8000"),
                    text_cell("2026-11-15"),
                ],
            ],
            ..TableSpec::default()
        }));
        let mark = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
        body.push(Block::Image(ImageSpec {
            image: mark,
            width: Some(Positive::pt(8.0)?),
            height: None,
            align: Alignment::Start,
        }));
    }
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::uniform(NonNegative::pt(36.0)?),
            footer: Some(BandSpec {
                left: Some(BandSlot::Text {
                    runs: vec![
                        TextRun::Literal("第 ".into()),
                        TextRun::PageNumber,
                        TextRun::Literal(" 页 / 共 ".into()),
                        TextRun::PageCount,
                        TextRun::Literal(" 页".into()),
                    ],
                    font: (),
                    style: RunStyle::default(),
                }),
                ..BandSpec::default()
            }),
            body,
            ..SectionSpec::default()
        }],
    })
}

fn text_cell(text: &str) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Text {
            text: text.into(),
            font: (),
        }],
        ..CellSpec::default()
    }
}

fn wide_table_doc(rows: usize, cols: usize) -> Result<Rendered> {
    let tracks = (0..cols)
        .map(|_| Track::Ratio(Ratio::new(1.0).expect("1 is positive")))
        .collect();
    let header = (0..cols).map(|c| text_cell(&format!("C{c}"))).collect();
    let mut body = Vec::with_capacity(rows);
    for r in 0..rows {
        let cells = (0..cols).map(|c| text_cell(&format!("r{r}c{c}"))).collect();
        body.push(cells);
    }
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(
                common::test_font_bytes().expect("PDFCRAFT_PDF_FONT or PDFCRAFT_CJK_FONT"),
                0,
            ),
            bold: None,
            italic: None,
            bold_italic: None,
        })),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::uniform(NonNegative::pt(36.0)?),
            body: vec![Block::Table(TableSpec {
                header: vec![header],
                body,
                tracks: Some(tracks),
                ..TableSpec::default()
            })],
            ..SectionSpec::default()
        }],
    })
}

fn paragraphs_doc(count: usize) -> Result<Rendered> {
    let body = (0..count)
        .map(|i| {
            Block::paragraph(
                (),
                format!(
                    "Paragraph {i}: the quick brown fox jumps over the lazy dog and back again."
                ),
            )
        })
        .collect();
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(
                common::test_font_bytes().expect("PDFCRAFT_PDF_FONT or PDFCRAFT_CJK_FONT"),
                0,
            ),
            bold: None,
            italic: None,
            bold_italic: None,
        })),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::uniform(NonNegative::pt(36.0)?),
            body,
            ..SectionSpec::default()
        }],
    })
}

fn hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "unknown".into())
        .trim()
        .to_owned()
}

fn uname() -> String {
    Command::new("uname")
        .arg("-a")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map_or_else(|| "unknown".into(), |s| s.trim().to_owned())
}

fn git_tip() -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map_or_else(|| "unknown".into(), |s| s.trim().to_owned())
}

fn peak_rss_kib() -> u64 {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let digits: String = rest.chars().filter(char::is_ascii_digit).collect();
            return digits.parse().unwrap_or(0);
        }
    }
    0
}
