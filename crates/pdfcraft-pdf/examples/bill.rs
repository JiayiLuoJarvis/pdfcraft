//! HC service bill example — brand colors kept, layout is a [`DocumentSpec`].
//!
//! Chinese face defaults to Source Han Serif HK via `.fonts/body.ttf` /
//! `.fonts/bold.ttf`, or override with `PDFCRAFT_CJK_FONT` / `PDFCRAFT_CJK_BOLD_FONT`.
//!
//! ```text
//! cargo run --example bill
//! ```
//!
//! Writes `out/bill.pdf` (gitignored). Pass another path as argv[1].

#![expect(clippy::print_stderr, reason = "example reports the written PDF path")]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use pdfcraft_pdf::spec::{
    Alignment, BandFn, BandInset, BandSlot, BandSpec, BandStyle, Block, CellSpec, CellStyle,
    ColumnSpec, ColumnStyle, ColumnsSpec, ColumnsStyle, FontFace, FontFamily, FontRegistry,
    HLineStyle, ImageRef, ImageSpec, ImageStyle, PageText, ParagraphSpec, ParagraphStyle,
    Resources, RunStyle, SectionSpec, SectionStyle, Span, TableBorder, TableHeader, TableRow,
    TableSpec, TableStyle,
};
use pdfcraft_pdf::{
    layout_spec, to_pdf, Color, DocumentSpec, ImageFormat, Length, Limits, Margins, NonNegative,
    PageSize, Positive, Ratio, Result, Track,
};

const NAVY: Color = Color::rgb(0x19, 0x2d, 0x74);
const GRAY: Color = Color::rgb(0xc5, 0xc5, 0xc5);
const BLACK: Color = Color::rgb(0x00, 0x00, 0x00);
const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);

const LOGO: &[u8] = include_bytes!("assets/logo.png");
const MARK: &[u8] = include_bytes!("assets/mark.png");
const DIAMOND: &[u8] = include_bytes!("assets/diamond.png");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let out = env::args()
        .nth(1)
        .map_or_else(default_out_path, PathBuf::from);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut resources = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(load_font("PDFCRAFT_CJK_FONT", &[".fonts/body.ttf"])?, 0),
        bold: Some(FontFace::new(
            load_font("PDFCRAFT_CJK_BOLD_FONT", &[".fonts/bold.ttf"])?,
            0,
        )),
        italic: None,
        bold_italic: None,
    }));
    let logo = resources.add_image(LOGO.to_vec(), ImageFormat::Png);
    let mark = resources.add_image(MARK.to_vec(), ImageFormat::Png);
    let diamond = resources.add_image(DIAMOND.to_vec(), ImageFormat::Png);

    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            style: SectionStyle {
                margins: Margins::try_new(28.0, 40.0, 28.0, 40.0)?,
                ..SectionStyle::default()
            },
            header: Some(BandFn::fixed(BandSpec {
                style: BandStyle {
                    inset: BandInset::PageMargins,
                },
                left: Some(BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal("恒誠商務有限公司".into())],
                    link: None,
                }),
                right: Some(BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![
                        PageText::Literal("第 ".into()),
                        PageText::PageNumber,
                        PageText::Literal(" / ".into()),
                        PageText::PageCount,
                        PageText::Literal(" 頁".into()),
                    ],
                    link: None,
                }),
                marks: Vec::new(),
            })),
            footer: Some(BandFn::fixed(BandSpec {
                style: BandStyle {
                    inset: BandInset::PageMargins,
                },
                left: Some(BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal("Tel.: 00852 - 2668 1868".into())],
                    link: None,
                }),
                right: Some(BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal("Web.: www.hc.biz".into())],
                    link: None,
                }),
                marks: Vec::new(),
            })),
            body: bill_body(logo, mark, diamond)?,
            ..SectionSpec::default()
        }],
    })?;
    let pdf = to_pdf(rendered)?;
    fs::write(&out, &pdf)?;
    eprintln!("wrote {} ({} bytes)", out.display(), pdf.len());
    Ok(())
}

fn pos(value: f32) -> Result<Length<Positive>> {
    Positive::pt(value).map_err(pdfcraft_pdf::Errors::from)
}

fn nn(value: f32) -> Result<Length<NonNegative>> {
    NonNegative::pt(value).map_err(pdfcraft_pdf::Errors::from)
}

fn line(text: &str) -> Block<()> {
    Block::paragraph((), text)
}

fn aligned(text: &str, align: Alignment) -> Block<()> {
    Block::Paragraph(ParagraphSpec {
        style: ParagraphStyle {
            align,
            ..ParagraphStyle::default()
        },
        children: vec![Span::Text {
            text: text.into(),
            font: (),
            style: RunStyle::default(),
        }],
    })
}

fn styled(size: f32, color: Color, children: Vec<Block<()>>) -> Result<Block<()>> {
    Ok(Block::Styled {
        style: RunStyle {
            font_size: Some(pos(size)?),
            color: Some(color),
            ..RunStyle::default()
        },
        children,
    })
}

fn rule(width: Option<f32>, align: Alignment, thickness: f32) -> Result<Block<()>> {
    Ok(Block::HLine(HLineStyle {
        width: width.map(pos).transpose()?,
        align,
        thickness: pos(thickness)?,
        color: NAVY,
    }))
}

fn ratio_col(weight: f32, blocks: Vec<Block<()>>) -> Result<ColumnSpec<()>> {
    Ok(ColumnSpec {
        style: ColumnStyle {
            width: Track::Ratio(Ratio::new(weight).map_err(pdfcraft_pdf::Errors::from)?),
        },
        children: blocks,
    })
}

fn filled(content: Vec<Block<()>>, fill: Color) -> CellSpec<()> {
    CellSpec {
        style: CellStyle { fill: Some(fill) },
        children: content,
        ..CellSpec::default()
    }
}

fn para_cell(text: &str, size: f32, color: Color) -> Result<CellSpec<()>> {
    Ok(CellSpec {
        children: vec![Block::Paragraph(ParagraphSpec {
            style: ParagraphStyle {
                text: RunStyle {
                    color: Some(color),
                    font_size: Some(pos(size)?),
                    ..RunStyle::default()
                },
                ..ParagraphStyle::default()
            },
            children: vec![Span::Text {
                text: text.into(),
                font: (),
                style: RunStyle::default(),
            }],
        })],
        ..CellSpec::default()
    })
}

fn bill_body(logo: ImageRef, mark: ImageRef, diamond: ImageRef) -> Result<Vec<Block<()>>> {
    let mut body = letterhead(logo, mark)?;
    body.extend(bill_to()?);
    body.extend(service_table()?);
    body.push(payment(diamond)?);
    body.extend(cheque_notes()?);
    body.extend(company_block()?);
    Ok(body)
}

fn letterhead(logo: ImageRef, mark: ImageRef) -> Result<Vec<Block<()>>> {
    Ok(vec![
        Block::Columns(ColumnsSpec {
            style: ColumnsStyle { gap: nn(12.0)? },
            children: vec![
                ratio_col(
                    1.0,
                    vec![
                        Block::Image(ImageSpec {
                            image: logo,
                            style: ImageStyle {
                                width: Some(pos(120.0)?),
                                height: None,
                                align: Alignment::Start,
                            },
                        }),
                        Block::Spacer(nn(6.0)?),
                        styled(9.0, NAVY, vec![line("商 業 理 想 同 行 者")])?,
                        Block::Spacer(nn(8.0)?),
                        rule(None, Alignment::Center, 0.75)?,
                        Block::Spacer(nn(4.0)?),
                    ],
                )?,
                ColumnSpec {
                    style: ColumnStyle {
                        width: Track::Fixed(pos(56.0)?),
                    },
                    children: vec![Block::Image(ImageSpec {
                        image: mark,
                        style: ImageStyle {
                            width: Some(pos(52.0)?),
                            height: None,
                            align: Alignment::End,
                        },
                    })],
                },
            ],
        }),
        Block::Spacer(nn(16.0)?),
    ])
}

fn bill_to() -> Result<Vec<Block<()>>> {
    Ok(vec![
        styled(16.0, NAVY, vec![line("Service Bill 服務賬單")])?,
        Block::Spacer(nn(18.0)?),
        styled(12.0, NAVY, vec![line("Bill To 計入客欠")])?,
        Block::Spacer(nn(4.0)?),
        rule(Some(96.0), Alignment::Center, 0.75)?,
        Block::Spacer(nn(4.0)?),
        rule(None, Alignment::End, 2.5)?,
        Block::Spacer(nn(10.0)?),
        styled(
            11.0,
            BLACK,
            vec![
                line("00000955"),
                line("教育有限公司"),
                line("Educationist Limited"),
            ],
        )?,
        Block::Spacer(nn(16.0)?),
    ])
}

fn service_table() -> Result<Vec<Block<()>>> {
    let mut body = vec![TableRow {
        children: vec![
            para_cell("綜合服務費用（含代支）", 11.0, BLACK)?,
            para_cell("505.00 HKD", 10.5, BLACK)?,
        ],
    }];
    for _ in 0..3 {
        body.push(TableRow {
            children: vec![
                CellSpec {
                    children: vec![Block::Spacer(nn(18.0)?)],
                    ..CellSpec::default()
                },
                CellSpec {
                    children: vec![Block::Spacer(nn(18.0)?)],
                    ..CellSpec::default()
                },
            ],
        });
    }
    Ok(vec![
        Block::Columns(ColumnsSpec {
            style: ColumnsStyle { gap: nn(12.0)? },
            children: vec![
                ratio_col(
                    1.0,
                    vec![styled(12.0, NAVY, vec![line("Service Summary 服務概要")])?],
                )?,
                ratio_col(
                    1.0,
                    vec![styled(
                        9.0,
                        NAVY,
                        vec![aligned("日期：2025年10月28日", Alignment::End)],
                    )?],
                )?,
            ],
        }),
        Block::Spacer(nn(8.0)?),
        Block::Table(TableSpec {
            style: TableStyle {
                tracks: Some(vec![
                    Track::Ratio(Ratio::new(2.2).map_err(pdfcraft_pdf::Errors::from)?),
                    Track::Ratio(Ratio::new(1.0).map_err(pdfcraft_pdf::Errors::from)?),
                ]),
                cell_padding: nn(8.0)?,
                border: TableBorder {
                    width: nn(0.66)?,
                    color: GRAY,
                },
            },
            header: Some(TableHeader {
                repeat: true,
                rows: vec![TableRow {
                    children: vec![
                        filled(
                            para_cell("Key item (s) / outlay (s) 主要項目/代支", 11.0, WHITE)?
                                .children,
                            NAVY,
                        ),
                        filled(para_cell("Amt. 金額", 10.5, WHITE)?.children, NAVY),
                    ],
                }],
            }),
            body,
        }),
        Block::Spacer(nn(8.0)?),
        rule(None, Alignment::End, 2.5)?,
        Block::Spacer(nn(8.0)?),
        styled(
            14.0,
            NAVY,
            vec![aligned("Bill Total 賬單總額: 505.00 HKD", Alignment::End)],
        )?,
        Block::Spacer(nn(16.0)?),
    ])
}

fn payment(diamond: ImageRef) -> Result<Block<()>> {
    let rows = [
        (
            "Beneficiary Bank Name：Hang Seng Bank Ltd",
            "銀行名稱：恒生銀行",
        ),
        ("Beneficiary Bank Code：024", "銀行代號：024"),
        ("Swift Code：HASEHKHH", "國際匯款識別碼：HASEHKHH"),
        (
            "Beneficiary Name： HC BUSINESS LIMITED",
            "收款人名稱：恒誠商務有限公司",
        ),
        (
            "Beneficiary Account No.：788669331883",
            "收款人戶口號碼：788669331883",
        ),
    ];
    let mut body = Vec::new();
    for (en, zh) in rows {
        body.push(TableRow {
            children: vec![
                CellSpec {
                    children: vec![Block::Image(ImageSpec {
                        image: diamond,
                        style: ImageStyle {
                            width: Some(pos(8.0)?),
                            height: None,
                            align: Alignment::Start,
                        },
                    })],
                    ..CellSpec::default()
                },
                para_cell(en, 8.0, NAVY)?,
                para_cell(zh, 8.0, NAVY)?,
            ],
        });
    }
    Ok(Block::Table(TableSpec {
        style: TableStyle {
            tracks: Some(vec![
                Track::Fixed(pos(22.0)?),
                Track::Ratio(Ratio::new(1.0).map_err(pdfcraft_pdf::Errors::from)?),
                Track::Ratio(Ratio::new(1.0).map_err(pdfcraft_pdf::Errors::from)?),
            ]),
            cell_padding: nn(4.0)?,
            border: TableBorder {
                width: nn(0.66)?,
                color: GRAY,
            },
        },
        header: Some(TableHeader {
            repeat: true,
            rows: vec![TableRow {
                children: vec![
                    filled(vec![Block::Spacer(nn(1.0)?)], NAVY),
                    filled(
                        para_cell("Payment by bank transfer (T/T) and other info", 8.0, WHITE)?
                            .children,
                        NAVY,
                    ),
                    filled(
                        para_cell("以電匯付款及其他注意事項", 8.0, WHITE)?.children,
                        NAVY,
                    ),
                ],
            }],
        }),
        body,
    }))
}

fn cheque_notes() -> Result<Vec<Block<()>>> {
    Ok(vec![
        Block::Spacer(nn(12.0)?),
        styled(
            9.0,
            NAVY,
            vec![Block::Columns(ColumnsSpec {
                style: ColumnsStyle { gap: nn(16.0)? },
                children: vec![
                    ratio_col(
                        1.0,
                        vec![
                            line("Cheque made payable to \"HC BUSINESS LIMITED\""),
                            line("支票付款請以 \"恒誠商務有限公司\" 抬頭"),
                        ],
                    )?,
                    ratio_col(
                        1.0,
                        vec![
                            line("FPS Unique ID: 3952652 (HK local bank transactions only)"),
                            line("快速支付編號: 3952652 (僅限香港本地銀行交易)"),
                        ],
                    )?,
                ],
            })],
        )?,
    ])
}

fn company_block() -> Result<Vec<Block<()>>> {
    Ok(vec![
        Block::Spacer(nn(20.0)?),
        rule(None, Alignment::Center, 0.75)?,
        Block::Spacer(nn(8.0)?),
        styled(
            8.0,
            NAVY,
            vec![
                line("恒誠商務有限公司 · HC BUSINESS LIMITED"),
                line("Add.: 香港九龍尖沙咀漆咸道南45-51號其士大廈801室"),
            ],
        )?,
    ])
}

fn default_out_path() -> PathBuf {
    PathBuf::from("out/bill.pdf")
}

fn load_font(env_key: &str, candidates: &[&str]) -> Result<Vec<u8>> {
    if let Ok(path) = env::var(env_key) {
        return fs::read(&path)
            .map_err(|error| font_error(format!("read {path}: {error}")))
            .map_err(pdfcraft_pdf::Errors::from);
    }
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return fs::read(path)
                .map_err(|error| font_error(format!("read {}: {error}", path.display())))
                .map_err(pdfcraft_pdf::Errors::from);
        }
    }
    Err(pdfcraft_pdf::Errors::from(font_error(format!(
        "set {env_key} or place a CJK TTF at {}",
        candidates[0]
    ))))
}

fn font_error(message: String) -> pdfcraft_pdf::Error {
    pdfcraft_pdf::Error::from(pdfcraft_pdf::ErrorKind::Font { index: 0, message })
}
