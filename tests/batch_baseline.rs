//! Reproducible wall-clock baseline for batch-readiness work item P0.
//!
//! ```text
//! cargo test --release --features test-utils --test batch_baseline -- --ignored --nocapture
//! ```
//!
//! Prints machine identity, git tip when available, and three samples:
//! letter pages, a wide table, and many short paragraphs. Each sample reports
//! `document` time, `to_pdf` time, page count, PDF bytes, and peak RSS.

#![cfg(feature = "backend-krilla")]

use std::fs;
use std::process::Command;
use std::time::Instant;

use quire_pdf::{
    document, to_pdf, FlowWriter, ImageFormat, Margins, NonNegative, PageSize, Positive, Ratio,
    Rendered, Result, Track, A4,
};

const LETTER_FONT: &[u8] = include_bytes!("fixtures/quire-letter.ttf");
const MEASURE_FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf");
const ONE_PIXEL: &[u8] = include_bytes!("fixtures/one-pixel.png");

const BODY_1: &str =
    "敬启者：兹就贵司与本公司于二零二六年十月所签合同事项，函请确认下列款项与交货日期。";
const BODY_2: &str = "请于收函后十日内书面回复。谢谢合作。";
const MIXED: &str = "Contract C-18 / 合同编号 C-18";

#[test]
#[ignore = "P0 wall-clock baseline; run with --ignored --nocapture"]
fn batch_readiness_p0_baseline() -> Result<()> {
    println!("quire-pdf batch-readiness P0 baseline");
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
    document(PageSize::A4, |doc| {
        doc.font(LETTER_FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(36.0)?));
        doc.footer(|footer| {
            let page = footer.page();
            footer.left(format!("第 {} 页 / 共 {} 页", page.number, page.count));
        });
        for index in 0..copies {
            if index > 0 {
                doc.page_break();
            }
            doc.paragraph(|p| p.text(BODY_1));
            doc.paragraph(|p| p.text(BODY_2));
            doc.paragraph(|p| p.text(MIXED));
            doc.table(|table| {
                table.header(["项目", "金额", "交货日"]);
                table.row(["设备", "12000", "2026-10-31"]);
                table.row(["服务", "8000", "2026-11-15"]);
            });
            let mark = doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?;
            let side = Positive::pt(8.0)?;
            doc.image(&mark, |image| image.width(side));
        }
        Ok(())
    })
}

fn wide_table_doc(rows: usize, cols: usize) -> Result<Rendered> {
    document(PageSize::A4, |doc| {
        doc.font(MEASURE_FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(36.0)?));
        let tracks: Vec<Track> = (0..cols)
            .map(|_| Track::Ratio(Ratio::new(1.0).expect("1 is positive")))
            .collect();
        doc.table(|table| {
            table.tracks(tracks);
            let header: Vec<String> = (0..cols).map(|c| format!("C{c}")).collect();
            table.header(header.iter().map(String::as_str));
            for r in 0..rows {
                let cells: Vec<String> = (0..cols).map(|c| format!("r{r}c{c}")).collect();
                table.row(cells.iter().map(String::as_str));
            }
        });
        Ok(())
    })
}

fn paragraphs_doc(count: usize) -> Result<Rendered> {
    document(PageSize::A4, |doc| {
        doc.font(MEASURE_FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(36.0)?));
        for i in 0..count {
            let line = format!(
                "Paragraph {i}: the quick brown fox jumps over the lazy dog and back again."
            );
            doc.paragraph(|p| p.text(line));
        }
        Ok(())
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
