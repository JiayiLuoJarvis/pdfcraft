//! Confirmation-letter layout on the CJK letter fixture.

#![cfg(feature = "backend-krilla")]

use pdfcraft_pdf::canvas::pt_near;
use pdfcraft_pdf::spec::{
    Alignment, BandSlot, BandSpec, Block, CellBlock, CellSpec, ImageSpec, Resources, SectionSpec,
    TableSpec, TextRun,
};
use pdfcraft_pdf::{
    layout_spec, to_pdf, CanvasCommand, DocumentSpec, ImageFormat, Margins, NonNegative, PageSize,
    Positive, RecordedPage, Result, A4,
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

fn letter() -> Result<pdfcraft_pdf::Rendered> {
    let mut resources = Resources::new(FONT.to_vec());
    let mark = resources.add_image(ONE_PIXEL.to_vec(), ImageFormat::Png);
    let margin = NonNegative::pt(MARGIN)?;
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::uniform(margin),
            footer: Some(BandSpec {
                left: Some(BandSlot::Text(vec![
                    TextRun::Literal("第 ".into()),
                    TextRun::PageNumber,
                    TextRun::Literal(" 页 / 共 ".into()),
                    TextRun::PageCount,
                    TextRun::Literal(" 页".into()),
                ])),
                ..BandSpec::default()
            }),
            body: vec![
                Block::paragraph(BODY_1),
                Block::paragraph(BODY_2),
                Block::paragraph(MIXED),
                Block::Table(TableSpec {
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
                }),
                Block::Image(ImageSpec {
                    image: mark,
                    width: Some(Positive::pt(8.0)?),
                    height: None,
                    align: Alignment::Start,
                }),
            ],
            ..SectionSpec::default()
        }],
    })
}

fn text_cell(text: &str) -> CellSpec {
    CellSpec {
        content: vec![CellBlock::Text(text.into())],
        ..CellSpec::default()
    }
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
