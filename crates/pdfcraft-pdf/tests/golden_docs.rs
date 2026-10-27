//! Checked-in golden documents: text flow, table spans, bands + layers.
//!
//! Each test lays out one small synthetic [`DocumentSpec`] and compares the
//! stable-text serialization against `tests/goldens/<name>.golden` via the
//! harness in `tests/golden/`.
//!
//! Bless workflow: `UPDATE_GOLDEN=1 cargo test -p pdfcraft-pdf
//! --features backend-krilla,test-utils golden_docs`, then review the diff
//! (`git diff crates/pdfcraft-pdf/tests/goldens/`) line by line as behavior
//! before keeping it. CI never blesses: without `UPDATE_GOLDEN=1` a missing
//! or different golden fails naming the file and writes
//! `target/golden/<name>.actual`. See `tests/README.md`.
//!
//! The glyph goldens (`text_flow`, `bands_layers`) are pinned to one font:
//! Ubuntu 24.04 `fonts-dejavu-core` `DejaVuSans` (`BLESSED_FONT_SHA256`). A
//! different `PDFCRAFT_PDF_FONT` skips those tests instead of failing, because
//! a font swap must show up as a reviewed re-bless, not a red gate. The
//! `table_spans` golden is font-free and verifies everywhere.

#[path = "golden/sha256.rs"]
mod sha256;

#[allow(
    dead_code,
    reason = "this target only blesses/verifies via assert_golden; self-test helpers stay for golden.rs"
)]
#[path = "golden/snapshot.rs"]
mod snapshot;

#[path = "common/mod.rs"]
mod common;

use std::num::NonZeroU16;

use pdfcraft_pdf::spec::{
    Alignment, BandInset, BandSlot, BandSpec, Block, CanvasMark, CanvasSpec, CellBlock, CellSpec,
    DocumentInfo, FontFace, FontFamily, FontRegistry, HLineSpec, InlineRun, LayerMark, LayerShape,
    PageWhen, ParagraphSpec, Resources, RunStyle, SectionSpec, TableBorder, TableRow, TableSpec,
    TextRun,
};
use pdfcraft_pdf::{
    layout_spec, Color, DocumentSpec, Finite, Ink, Length, Limits, Margins, NonNegative, PageSize,
    Path, Pen, Point, Positive, Rect, Size, Track,
};
use snapshot::FontMeta;

/// SHA-256 of the DejaVuSans.ttf these glyph goldens were blessed with
/// (Ubuntu 24.04 `fonts-dejavu-core`). A different font skips the glyph
/// goldens; re-bless deliberately and review the header diff.
const BLESSED_FONT_SHA256: &str =
    "ae7b7855e115a5966d8b1b3f80f254ccc117ec86f9965e202ee2940453837280";

fn nn(value: f32) -> Length<NonNegative> {
    NonNegative::pt(value).unwrap()
}

fn pos(value: f32) -> Length<Positive> {
    Positive::pt(value).unwrap()
}

fn fin(value: f32) -> Length<Finite> {
    Finite::pt(value).unwrap()
}

fn pt(x: f32, y: f32) -> Point {
    Point::try_pt(x, y).unwrap()
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::try_pt(x, y, w, h).unwrap()
}

fn pen(r: u8, g: u8, b: u8, width: f32) -> Pen {
    Pen::new(Color::rgb(r, g, b), width).unwrap()
}

/// Pinned font bytes plus its golden header entry, or `None` to skip.
///
/// Skips when no font is configured (suite convention) and when the
/// configured font is not the blessed one (a swap needs a reviewed re-bless,
/// not a red gate).
fn pinned_font() -> Option<(Vec<u8>, FontMeta)> {
    let bytes = common::test_font_bytes()?;
    if sha256::hex(&bytes) != BLESSED_FONT_SHA256 {
        eprintln!(
            "skipping golden_docs glyph test: font is not the blessed DejaVuSans; re-bless with UPDATE_GOLDEN=1 after review"
        );
        return None;
    }
    let upem = {
        use skrifa::instance::{LocationRef, Size};
        use skrifa::{FontRef, MetadataProvider};
        let font_ref = FontRef::new(&bytes).unwrap();
        f32::from(
            font_ref
                .metrics(Size::unscaled(), LocationRef::default())
                .units_per_em,
        )
    };
    let meta = FontMeta::new(sha256::hex(&bytes), upem);
    Some((bytes, meta))
}

fn font_resources(bytes: Vec<u8>) -> Resources<()> {
    Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(bytes, 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }))
}

fn doc_info(title: &str) -> DocumentInfo {
    DocumentInfo {
        title: Some(title.to_owned()),
        author: None,
        subject: None,
        keywords: vec!["golden".to_owned()],
        language: Some("en".to_owned()),
    }
}

fn text_flow_body() -> Vec<Block<()>> {
    let lead = ParagraphSpec {
        runs: vec![
            InlineRun::Text {
                text: "Hello, golden flow. ".to_owned(),
                font: (),
                style: RunStyle::default(),
            },
            InlineRun::Text {
                text: "Sized up. ".to_owned(),
                font: (),
                style: RunStyle {
                    font_size: Some(pos(18.0)),
                    color: Some(Color::rgb(0, 0, 160)),
                    ..RunStyle::default()
                },
            },
            InlineRun::Text {
                text: "Underlined and tracked.".to_owned(),
                font: (),
                style: RunStyle {
                    underline: Some(true),
                    tracking: Some(fin(0.5)),
                    ..RunStyle::default()
                },
            },
        ],
        ..ParagraphSpec::default()
    };
    let centered = ParagraphSpec {
        align: Alignment::Center,
        first_line_indent: nn(12.0),
        ..ParagraphSpec::text((), "Centered with a first-line indent.")
    };
    let styled = Block::Styled {
        style: RunStyle {
            font_size: Some(pos(14.0)),
            color: Some(Color::rgb(0, 100, 0)),
            ..RunStyle::default()
        },
        children: vec![Block::paragraph((), "Styled inheritance.")],
    };
    vec![
        Block::Paragraph(lead),
        Block::HLine(HLineSpec {
            thickness: pos(2.0),
            color: Color::rgb(200, 0, 0),
            space_before: nn(6.0),
            space_after: nn(6.0),
            ..HLineSpec::default()
        }),
        styled,
        Block::Paragraph(centered),
    ]
}

#[test]
fn golden_doc_text_flow() {
    let Some((bytes, meta)) = pinned_font() else {
        eprintln!("skipping golden_doc_text_flow: needs the blessed font");
        return;
    };
    let info = doc_info("Golden Text Flow");
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: font_resources(bytes),
        limits: Limits::default(),
        info: info.clone(),
        sections: vec![SectionSpec {
            page_size: None,
            margins: Margins::try_uniform(48.0).unwrap(),
            header: None,
            footer: None,
            behind: vec![],
            front: vec![],
            body: text_flow_body(),
        }],
    })
    .unwrap();
    snapshot::assert_golden(&rendered.pages(), Some(&info), &[meta], "text_flow");
}

fn span_cell(content: Vec<CellBlock<()>>, cols: u16, rows: u16) -> CellSpec<()> {
    CellSpec {
        content,
        col_span: NonZeroU16::new(cols).unwrap(),
        row_span: NonZeroU16::new(rows).unwrap(),
        ..CellSpec::default()
    }
}

fn canvas_cell(w: f32, h: f32, marks: Vec<CanvasMark<()>>) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Canvas(CanvasSpec {
            size: Size::try_pt(w, h).unwrap(),
            align: Alignment::Start,
            marks,
        })],
        ..CellSpec::default()
    }
}

#[test]
fn golden_doc_table_spans() {
    let info = doc_info("Golden Table Spans");
    let table = Block::Table(TableSpec {
        header: vec![TableRow {
            cells: vec![
                span_cell(
                    vec![CellBlock::HLine(HLineSpec {
                        thickness: pos(1.5),
                        color: Color::rgb(0, 0, 0),
                        ..HLineSpec::default()
                    })],
                    2,
                    1,
                ),
                canvas_cell(
                    40.0,
                    16.0,
                    vec![CanvasMark::Rect {
                        rect: rect(0.0, 0.0, 40.0, 16.0),
                        ink: Ink::Fill(Color::rgb(0, 128, 0)),
                    }],
                ),
            ],
        }],
        body: vec![
            TableRow {
                cells: vec![
                    span_cell(
                        vec![CellBlock::Canvas(CanvasSpec {
                            size: Size::try_pt(200.0, 24.0).unwrap(),
                            align: Alignment::Start,
                            marks: vec![CanvasMark::Rect {
                                rect: rect(0.0, 0.0, 200.0, 24.0),
                                ink: Ink::Fill(Color::rgb(0, 0, 200)),
                            }],
                        })],
                        2,
                        1,
                    ),
                    CellSpec {
                        content: vec![CellBlock::Canvas(CanvasSpec {
                            size: Size::try_pt(40.0, 48.0).unwrap(),
                            align: Alignment::Start,
                            marks: vec![CanvasMark::Line {
                                from: pt(0.0, 0.0),
                                to: pt(40.0, 48.0),
                                pen: pen(0, 0, 0, 1.0),
                            }],
                        })],
                        col_span: NonZeroU16::new(1).unwrap(),
                        row_span: NonZeroU16::new(2).unwrap(),
                        fill: Some(Color::rgb(240, 240, 240)),
                    },
                ],
            },
            TableRow {
                cells: vec![span_cell(
                    vec![CellBlock::Canvas(CanvasSpec {
                        size: Size::try_pt(200.0, 12.0).unwrap(),
                        align: Alignment::Center,
                        marks: vec![CanvasMark::Fill {
                            path: Path::line(pt(0.0, 6.0), pt(200.0, 6.0)),
                            color: Color::rgb(160, 0, 160),
                        }],
                    })],
                    2,
                    1,
                )],
            },
        ],
        repeat_header: false,
        tracks: Some(vec![
            Track::Fixed(pos(140.0)),
            Track::Fixed(pos(140.0)),
            Track::Auto,
        ]),
        cell_padding: nn(4.0),
        border: TableBorder {
            width: nn(0.5),
            color: Color::rgb(80, 80, 80),
        },
    });
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::<()>::default(),
        limits: Limits::default(),
        info: info.clone(),
        sections: vec![SectionSpec {
            page_size: None,
            margins: Margins::try_uniform(48.0).unwrap(),
            header: None,
            footer: None,
            behind: vec![],
            front: vec![],
            body: vec![table],
        }],
    })
    .unwrap();
    snapshot::assert_golden(&rendered.pages(), Some(&info), &[], "table_spans");
}

fn bands_section() -> SectionSpec<()> {
    SectionSpec {
        page_size: None,
        margins: Margins::try_uniform(48.0).unwrap(),
        header: Some(BandSpec {
            inset: BandInset::PageMargins,
            left: Some(BandSlot::Text {
                runs: vec![TextRun::Literal("Golden bands.".to_owned())],
                font: (),
                style: RunStyle::default(),
            }),
            right: Some(BandSlot::Text {
                runs: vec![
                    TextRun::PageNumber,
                    TextRun::Literal(" / ".to_owned()),
                    TextRun::PageCount,
                ],
                font: (),
                style: RunStyle::default(),
            }),
        }),
        footer: Some(BandSpec {
            inset: BandInset::PageMargins,
            left: Some(BandSlot::Text {
                runs: vec![TextRun::Literal("Footer left.".to_owned())],
                font: (),
                style: RunStyle::default(),
            }),
            right: Some(BandSlot::Link {
                uri: vec![TextRun::Literal("https://example.test/bands".to_owned())],
                text: vec![TextRun::Literal("Bands link".to_owned())],
                font: (),
                style: RunStyle::default(),
            }),
        }),
        behind: vec![
            LayerMark::Line {
                when: PageWhen::All,
                from: pt(20.0, 20.0),
                to: pt(200.0, 400.0),
                pen: pen(0, 0, 0, 0.5),
            },
            LayerMark::Ellipse {
                when: PageWhen::All,
                rect: rect(400.0, 40.0, 60.0, 30.0),
                ink: Ink::Stroke(pen(0, 0, 200, 1.0)),
            },
        ],
        front: vec![
            LayerMark::Rotated {
                when: PageWhen::All,
                degrees: 30.0,
                pivot: pt(297.0, 420.0),
                shapes: vec![
                    LayerShape::Line {
                        from: pt(250.0, 420.0),
                        to: pt(344.0, 420.0),
                        pen: pen(160, 0, 0, 1.0),
                    },
                    LayerShape::Rect {
                        rect: rect(287.0, 410.0, 20.0, 20.0),
                        ink: Ink::Fill(Color::rgb(200, 200, 0)),
                    },
                ],
            },
            LayerMark::Rect {
                when: PageWhen::Last,
                rect: rect(48.0, 780.0, 100.0, 12.0),
                ink: Ink::Fill(Color::rgba(0, 160, 0, 128)),
            },
        ],
        body: vec![
            Block::HLine(HLineSpec {
                thickness: pos(1.0),
                ..HLineSpec::default()
            }),
            Block::PageBreak,
            Block::HLine(HLineSpec {
                thickness: pos(3.0),
                color: Color::rgb(0, 0, 200),
                ..HLineSpec::default()
            }),
        ],
    }
}

#[test]
fn golden_doc_bands_layers() {
    let Some((bytes, meta)) = pinned_font() else {
        eprintln!("skipping golden_doc_bands_layers: needs the blessed font");
        return;
    };
    let info = doc_info("Golden Bands Layers");
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: font_resources(bytes),
        limits: Limits::default(),
        info: info.clone(),
        sections: vec![bands_section()],
    })
    .unwrap();
    snapshot::assert_golden(&rendered.pages(), Some(&info), &[meta], "bands_layers");
}
