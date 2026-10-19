//! Multi-page smoke demo on [`DocumentSpec`].
//!
//! ```text
//! cargo run --example hello
//! ```
//!
//! Writes `out/hello.pdf` (gitignored). Pass another path as argv[1].
//!
//! Set `PDFCRAFT_PDF_FONT` or `PDFCRAFT_CJK_FONT` to a face that covers the
//! demo text (ASCII, U+2022, and 第 / 页 / 共). Missing glyphs fail the document.

use std::env;
use std::fs;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;
use std::sync::Arc;

use pdfcraft_pdf::spec::{
    Alignment, BandInset, BandSlot, BandSpec, BlankSpec, BlankWidth, Block, CellBlock, CellSpec,
    ColumnSpec, ColumnsSpec, FontFace, FontFamily, FontRegistry, HLineSpec, ImageSpec, InlineRun,
    ListItemSpec, ListKind, ListSpec, ParagraphSpec, Resources, RunStyle, SectionSpec, TableSpec,
    TextRun,
};
use pdfcraft_pdf::{
    layout_spec, to_pdf, Color, ImageFormat, Length, Margins, NonNegative, Numbering, PageSize,
    Positive, Ratio, Result, Track,
};

const PIXEL: &[u8] = include_bytes!("../tests/fixtures/one-pixel.png");

fn load_font() -> Vec<u8> {
    let path = env::var_os("PDFCRAFT_PDF_FONT")
        .or_else(|| env::var_os("PDFCRAFT_CJK_FONT"))
        .unwrap_or_else(|| {
            panic!("set PDFCRAFT_PDF_FONT or PDFCRAFT_CJK_FONT to a .ttf/.otf path")
        });
    fs::read(&path)
        .unwrap_or_else(|error| panic!("read font {}: {error}", PathBuf::from(path).display()))
}

fn main() -> Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("out/hello.pdf"), PathBuf::from);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create output dir");
    }

    let font = Arc::new(load_font());
    let mut resources = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::from_shared(Arc::clone(&font), 0),
        bold: Some(FontFace::from_shared(font, 0)),
        italic: None,
        bold_italic: None,
    }));
    let logo = resources.add_image(PIXEL.to_vec(), ImageFormat::Png);
    let rendered = layout_spec(pdfcraft_pdf::DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_uniform(48.0)?,
            header: Some(BandSpec {
                inset: BandInset::PageMargins,
                left: Some(BandSlot::Link {
                    font: (),
                    uri: vec![TextRun::Literal("https://example.test/quire".into())],
                    text: vec![TextRun::Literal("quire demo".into())],
                    style: RunStyle::default(),
                }),
                right: Some(BandSlot::Text {
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
            }),
            footer: Some(BandSpec {
                inset: BandInset::PageMargins,
                left: Some(BandSlot::Text {
                    runs: vec![TextRun::Literal("layout smoke".into())],
                    font: (),
                    style: RunStyle::default(),
                }),
                right: Some(BandSlot::Link {
                    font: (),
                    uri: vec![TextRun::Literal("https://example.test/footer".into())],
                    text: vec![TextRun::Literal("footer link".into())],
                    style: RunStyle::default(),
                }),
            }),
            body: hello_body(logo)?,
            ..SectionSpec::default()
        }],
    })?;
    let pdf = to_pdf(rendered)?;
    fs::write(&out, &pdf).expect("write PDF");
    eprintln!("wrote {} ({} bytes)", out.display(), pdf.len());
    Ok(())
}

fn pos(value: f32) -> Result<Length<Positive>> {
    Positive::pt(value).map_err(pdfcraft_pdf::Errors::from)
}

fn nn(value: f32) -> Result<Length<NonNegative>> {
    NonNegative::pt(value).map_err(pdfcraft_pdf::Errors::from)
}

fn text(value: impl Into<String>) -> InlineRun<()> {
    InlineRun::Text {
        text: value.into(),
        font: (),
        style: RunStyle::default(),
    }
}

fn bold(value: impl Into<String>) -> InlineRun<()> {
    InlineRun::Text {
        text: value.into(),
        font: (),
        style: RunStyle {
            bold: Some(true),
            ..RunStyle::default()
        },
    }
}

fn link(uri: &str, runs: Vec<InlineRun<()>>) -> InlineRun<()> {
    InlineRun::Link {
        target: pdfcraft_pdf::LinkTarget::uri(uri).expect("example uri"),
        runs,
    }
}

fn blank(
    width: BlankWidth,
    thickness: f32,
    color: Option<Color>,
    offset: Option<f32>,
) -> Result<InlineRun<()>> {
    Ok(InlineRun::Blank(BlankSpec {
        width,
        thickness: Some(pos(thickness)?),
        color,
        baseline_offset: offset.map(nn).transpose()?,
    }))
}

fn paragraph(runs: Vec<InlineRun<()>>, align: Alignment, indent: f32) -> Result<Block<()>> {
    Ok(Block::Paragraph(ParagraphSpec {
        runs,
        first_line_indent: nn(indent)?,
        align,
        ..ParagraphSpec::default()
    }))
}

fn rule(
    width: Option<f32>,
    align: Alignment,
    thickness: f32,
    color: Color,
    before: f32,
    after: f32,
) -> Result<Block<()>> {
    Ok(Block::HLine(HLineSpec {
        width: width.map(pos).transpose()?,
        align,
        thickness: pos(thickness)?,
        color,
        space_before: nn(before)?,
        space_after: nn(after)?,
    }))
}

fn ratio(weight: f32, blocks: Vec<Block<()>>) -> ColumnSpec<()> {
    ColumnSpec {
        width: Track::Ratio(Ratio::new(weight).expect("weight is positive")),
        blocks,
    }
}

fn bullets() -> Result<Block<()>> {
    Ok(Block::List(ListSpec {
        marker_width: pos(16.0)?,
        nest_indent: Some(pos(20.0)?),
        items: vec![
            ListItemSpec {
                marker: None,
                blocks: vec![
                    paragraph(vec![text("measure with skrifa")], Alignment::Start, 0.0)?,
                    Block::List(ListSpec {
                        items: vec![
                            ListItemSpec {
                                marker: None,
                                blocks: vec![paragraph(
                                    vec![text("cluster")],
                                    Alignment::Start,
                                    0.0,
                                )?],
                            },
                            ListItemSpec {
                                marker: None,
                                blocks: vec![paragraph(
                                    vec![text("advance")],
                                    Alignment::Start,
                                    0.0,
                                )?],
                            },
                        ],
                        ..ListSpec::default()
                    }),
                ],
            },
            ListItemSpec {
                marker: None,
                blocks: vec![paragraph(
                    vec![text("break lines in the frame")],
                    Alignment::Start,
                    0.0,
                )?],
            },
            ListItemSpec {
                marker: Some("*".into()),
                blocks: vec![paragraph(
                    vec![text("custom marker item")],
                    Alignment::Start,
                    0.0,
                )?],
            },
        ],
        ..ListSpec::default()
    }))
}

fn ordered() -> Result<Block<()>> {
    Ok(Block::List(ListSpec {
        kind: ListKind::Ordered(Numbering::LowerAlpha),
        start: NonZeroU32::new(2).unwrap(),
        items: vec![
            item(paragraph(vec![text("paginate")], Alignment::Start, 0.0)?),
            item(paragraph(vec![text("decorate")], Alignment::Start, 0.0)?),
            marked(
                "(x)",
                paragraph(vec![text("marked, no number burn")], Alignment::Start, 0.0)?,
            ),
            item(paragraph(vec![text("emit PDF")], Alignment::Start, 0.0)?),
        ],
        ..ListSpec::default()
    }))
}

fn item(block: Block<()>) -> ListItemSpec<()> {
    ListItemSpec {
        marker: None,
        blocks: vec![block],
    }
}

fn marked(marker: &str, block: Block<()>) -> ListItemSpec<()> {
    ListItemSpec {
        marker: Some(marker.into()),
        blocks: vec![block],
    }
}

fn head_cell(label: &str) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Paragraph(ParagraphSpec {
            runs: vec![bold(label)],
            color: Some(Color::rgb(255, 255, 255)),
            ..ParagraphSpec::default()
        })],
        fill: Some(Color::rgb(40, 40, 40)),
        ..CellSpec::default()
    }
}

fn text_cell(value: impl Into<String>) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Text {
            text: value.into(),
            font: (),
        }],
        ..CellSpec::default()
    }
}

fn span_text(value: &str, cols: u16) -> CellSpec<()> {
    CellSpec {
        content: vec![CellBlock::Text {
            text: value.into(),
            font: (),
        }],
        col_span: NonZeroU16::new(cols).unwrap(),
        ..CellSpec::default()
    }
}

fn shipment_rows(logo: pdfcraft_pdf::spec::ImageRef) -> Result<Vec<Vec<CellSpec<()>>>> {
    let mut rows = vec![
        vec![
            CellSpec {
                content: vec![CellBlock::Text {
                    text: "A1".into(),
                    font: (),
                }],
                row_span: NonZeroU16::new(2).unwrap(),
                ..CellSpec::default()
            },
            CellSpec {
                content: vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![text("Pen "), bold("set")],
                    ..ParagraphSpec::default()
                })],
                ..CellSpec::default()
            },
            CellSpec {
                content: vec![CellBlock::List(ListSpec {
                    items: vec![
                        item(paragraph(vec![text("black")], Alignment::Start, 0.0)?),
                        item(paragraph(vec![text("blue")], Alignment::Start, 0.0)?),
                    ],
                    ..ListSpec::default()
                })],
                ..CellSpec::default()
            },
            text_cell("12"),
        ],
        vec![
            CellSpec {
                content: vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![link("https://example.test/ink", vec![text("Ink")])],
                    ..ParagraphSpec::default()
                })],
                ..CellSpec::default()
            },
            CellSpec {
                content: vec![CellBlock::List(ListSpec {
                    kind: ListKind::Ordered(Numbering::Decimal),
                    items: vec![
                        item(paragraph(vec![text("bottle")], Alignment::Start, 0.0)?),
                        marked("+", paragraph(vec![text("refill")], Alignment::Start, 0.0)?),
                    ],
                    ..ListSpec::default()
                })],
                ..CellSpec::default()
            },
            text_cell("3"),
        ],
        vec![
            text_cell("B2"),
            CellSpec {
                content: vec![CellBlock::Image(ImageSpec {
                    image: logo,
                    width: Some(pos(18.0)?),
                    height: None,
                    align: Alignment::Center,
                })],
                ..CellSpec::default()
            },
            CellSpec {
                content: vec![CellBlock::HLine(HLineSpec {
                    thickness: pos(0.75)?,
                    color: Color::rgb(120, 0, 0),
                    ..HLineSpec::default()
                })],
                ..CellSpec::default()
            },
            CellSpec {
                content: vec![CellBlock::Spacer(nn(8.0)?)],
                ..CellSpec::default()
            },
        ],
    ];
    for n in 0..36 {
        rows.push(vec![
            text_cell(format!("R{n}")),
            text_cell(format!("part {n}")),
            text_cell("stock"),
            text_cell(format!("{}", (n % 9) + 1)),
        ]);
    }
    rows.push(vec![span_text("Total rows above", 4)]);
    Ok(rows)
}

fn title_blocks(logo: pdfcraft_pdf::spec::ImageRef) -> Result<Vec<Block<()>>> {
    Ok(vec![
        paragraph(vec![bold("Quire layout tour")], Alignment::Center, 0.0)?,
        rule(
            Some(220.0),
            Alignment::Center,
            1.25,
            Color::rgb(40, 40, 40),
            6.0,
            12.0,
        )?,
        Block::Image(ImageSpec {
            image: logo,
            width: Some(pos(36.0)?),
            height: None,
            align: Alignment::Center,
        }),
        Block::Spacer(nn(10.0)?),
        paragraph(
            vec![
                text("This example exercises paragraph runs, "),
                bold("bold"),
                text(", "),
                link("https://example.test/docs", vec![text("links")]),
                text(", blanks, lists, columns, tables, images, keep-together,"),
                text(" and page breaks."),
            ],
            Alignment::Start,
            18.0,
        )?,
        paragraph(
            vec![
                text("Signed: "),
                blank(
                    BlankWidth::Fixed(nn(96.0)?),
                    0.75,
                    Some(Color::rgb(0, 0, 0)),
                    None,
                )?,
                text("  date "),
                blank(BlankWidth::Em(6), 0.75, None, Some(1.5))?,
            ],
            Alignment::Start,
            0.0,
        )?,
        paragraph(
            vec![
                text("Notes"),
                blank(
                    BlankWidth::ToEndOfLine,
                    0.5,
                    Some(Color::rgb(90, 90, 90)),
                    None,
                )?,
            ],
            Alignment::Start,
            0.0,
        )?,
        Block::Columns(ColumnsSpec {
            gap: nn(16.0)?,
            columns: vec![
                ratio(
                    1.0,
                    vec![
                        paragraph(vec![bold("Bullets")], Alignment::Start, 0.0)?,
                        bullets()?,
                    ],
                ),
                ratio(
                    1.2,
                    vec![
                        paragraph(vec![bold("Ordered")], Alignment::Start, 0.0)?,
                        ordered()?,
                    ],
                ),
            ],
        }),
        rule(
            Some(160.0),
            Alignment::Start,
            1.0,
            Color::rgb(0, 0, 0),
            14.0,
            10.0,
        )?,
    ])
}

fn table_block(logo: pdfcraft_pdf::spec::ImageRef) -> Result<Block<()>> {
    Ok(Block::Table(TableSpec {
        header: vec![
            vec![
                head_cell("Code"),
                head_cell("Item"),
                head_cell("Notes"),
                head_cell("Qty"),
            ],
            vec![span_text("group A", 4)],
        ],
        body: shipment_rows(logo)?,
        tracks: Some(vec![
            Track::Fixed(pos(54.0)?),
            Track::Ratio(Ratio::new(2.0).unwrap()),
            Track::Ratio(Ratio::new(1.0).unwrap()),
            Track::Fixed(pos(48.0)?),
        ]),
        cell_padding: nn(4.0)?,
        border_width: nn(0.5)?,
        border_color: Color::rgb(30, 30, 30),
        ..TableSpec::default()
    }))
}

fn closing_blocks() -> Result<Vec<Block<()>>> {
    Ok(vec![
        Block::KeepTogether(vec![
            paragraph(vec![bold("Keep-together block")], Alignment::Center, 0.0)?,
            paragraph(
                vec![
                    text("These two lines move as one piece when the remaining"),
                    text(" frame is too short."),
                ],
                Alignment::Start,
                0.0,
            )?,
            rule(
                Some(120.0),
                Alignment::End,
                1.0,
                Color::rgb(0, 0, 0),
                4.0,
                0.0,
            )?,
        ]),
        Block::Spacer(nn(8.0)?),
        Block::Columns(ColumnsSpec {
            gap: nn(12.0)?,
            columns: vec![
                ColumnSpec {
                    width: Track::Fixed(pos(140.0)?),
                    blocks: vec![
                        paragraph(vec![bold("Fixed column")], Alignment::Start, 0.0)?,
                        paragraph(
                            vec![text("Narrow frame wraps AA AA AA AA AA words here.")],
                            Alignment::Start,
                            0.0,
                        )?,
                        Block::List(ListSpec {
                            kind: ListKind::Ordered(Numbering::UpperRoman),
                            items: vec![
                                item(paragraph(vec![text("one")], Alignment::Start, 0.0)?),
                                item(paragraph(vec![text("two")], Alignment::Start, 0.0)?),
                            ],
                            ..ListSpec::default()
                        }),
                    ],
                },
                ColumnSpec {
                    width: Track::Auto,
                    blocks: vec![
                        paragraph(vec![bold("Auto column")], Alignment::Start, 0.0)?,
                        Block::Table(TableSpec {
                            header: vec![vec![text_cell("Side"), text_cell("Val")]],
                            body: vec![
                                vec![text_cell("L"), text_cell("1")],
                                vec![text_cell("R"), text_cell("2")],
                            ],
                            cell_padding: nn(3.0)?,
                            border_width: nn(0.4)?,
                            ..TableSpec::default()
                        }),
                        paragraph(vec![text("end-aligned note")], Alignment::End, 0.0)?,
                    ],
                },
            ],
        }),
        Block::Spacer(nn(16.0)?),
        paragraph(vec![text("End of tour.")], Alignment::Center, 0.0)?,
    ])
}

fn hello_body(logo: pdfcraft_pdf::spec::ImageRef) -> Result<Vec<Block<()>>> {
    let mut body = title_blocks(logo)?;
    body.push(paragraph(
        vec![bold("Shipment table")],
        Alignment::Start,
        0.0,
    )?);
    body.push(table_block(logo)?);
    body.push(Block::PageBreak);
    body.extend(closing_blocks()?);
    Ok(body)
}
