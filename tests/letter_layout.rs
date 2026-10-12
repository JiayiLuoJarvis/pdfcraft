//! Confirmation-letter layout on the CJK letter fixture.

#![cfg(feature = "backend-krilla")]

use quire_pdf::{
    document, pt_near, to_pdf, CanvasCommand, FlowWriter, ImageFormat, Margins, NonNegative,
    PageSize, Positive, RecordedPage, Result, A4,
};

const FONT: &[u8] = include_bytes!("fixtures/quire-letter.ttf");
const ONE_PIXEL: &[u8] = include_bytes!("fixtures/one-pixel.png");

const BODY_1: &str =
    "敬启者：兹就贵司与本公司于二零二六年十月所签合同事项，函请确认下列款项与交货日期。";
const BODY_2: &str = "请于收函后十日内书面回复。谢谢合作。";
const MIXED: &str = "Contract C-18 / 合同编号 C-18";

const MARGIN: f32 = 36.0;
// hhea, not OS/2 typo metrics. USE_TYPO_METRICS is clear on this face.
const HHEA_ASCENT: f32 = 1160.0;
const HHEA_DESCENT: f32 = -288.0;
const BASELINE: f32 = HHEA_ASCENT * (12.0 / 1000.0);
const LINE: f32 = (HHEA_ASCENT - HHEA_DESCENT) * (12.0 / 1000.0);

struct GlyphSample {
    text: String,
    x: f32,
    y: f32,
}

fn letter() -> Result<quire_pdf::Rendered> {
    document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(MARGIN)?));
        doc.footer(|footer| {
            let page = footer.page();
            footer.left(format!("第 {} 页 / 共 {} 页", page.number, page.count));
        });
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
        Ok(())
    })
}

fn runs(page: &RecordedPage) -> Vec<GlyphSample> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(GlyphSample {
                text: run.text.clone(),
                x: run.origin.x().as_pt(),
                y: run.origin.y().as_pt(),
            }),
            _ => None,
        })
        .collect()
}

fn texts(page: &RecordedPage) -> Vec<String> {
    runs(page).into_iter().map(|run| run.text).collect()
}

fn assert_at(run: &GlyphSample, x: f32, y: f32) {
    assert!(pt_near(run.x, x), "{} x {} != {x}", run.text, run.x);
    assert!(pt_near(run.y, y), "{} y {} != {y}", run.text, run.y);
}

#[test]
fn confirmation_letter_records_body_table_image_and_footer() {
    let rendered = letter().unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(
        texts(&pages[0]),
        [
            BODY_1,
            BODY_2,
            MIXED,
            "项目",
            "金额",
            "交货日",
            "设备",
            "12000",
            "2026-10-31",
            "服务",
            "8000",
            "2026-11-15",
            "第 1 页 / 共 1 页",
        ]
    );

    let drawn = runs(&pages[0]);
    let inner = A4.width().as_pt() - 2.0 * MARGIN;
    let col = inner / 3.0;
    let y = |index: f32| MARGIN + index * LINE + BASELINE;
    assert_at(&drawn[0], MARGIN, y(0.0));
    assert_at(&drawn[1], MARGIN, y(1.0));
    assert_at(&drawn[2], MARGIN, y(2.0));
    assert_at(&drawn[3], MARGIN, y(3.0));
    assert_at(&drawn[4], MARGIN + col, y(3.0));
    assert_at(&drawn[5], MARGIN + 2.0 * col, y(3.0));
    assert_at(&drawn[6], MARGIN, y(4.0));
    assert_at(&drawn[9], MARGIN, y(5.0));
    assert_at(&drawn[12], 0.0, A4.height().as_pt() - MARGIN + BASELINE);

    let images: Vec<_> = pages[0]
        .ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawImage { image, rect } => Some((image, rect)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].0.pixel_width, 1);
    assert_eq!(images[0].0.pixel_height, 1);
    let rect = images[0].1;
    assert!(pt_near(rect.origin().x().as_pt(), MARGIN));
    assert!(pt_near(rect.origin().y().as_pt(), MARGIN + 6.0 * LINE));
    assert!(pt_near(rect.size().width().as_pt(), 8.0));
    assert!(pt_near(rect.size().height().as_pt(), 8.0));
}

#[test]
fn confirmation_letter_pdf_starts_with_pdf_header() {
    let pdf = to_pdf(letter().unwrap()).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}
