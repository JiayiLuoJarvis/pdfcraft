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

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use pdfcraft_pdf::spec::{
    Alignment, BandInset, BandSlot, BandSpec, Block, CellBlock, CellSpec, ColumnSpec, ColumnsSpec,
    HLineSpec, ImageRef, ImageSpec, InlineRun, ParagraphSpec, Resources, SectionSpec, TableSpec,
    TextRun, TextStyle,
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

fn main() -> Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(default_out_path, PathBuf::from);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create output dir");
    }

    let mut resources = Resources::new(load_font(
        "PDFCRAFT_CJK_FONT",
        &[
            ".fonts/body.ttf",
            "/mnt/data/apps/hcsw-pdfcraft/backend/assets/fonts/SourceHanSerifHK/SourceHanSerifHK-Regular.ttf",
        ],
    )?);
    resources.bold_font = Some(load_font(
        "PDFCRAFT_CJK_BOLD_FONT",
        &[
            ".fonts/bold.ttf",
            "/mnt/data/apps/hcsw-pdfcraft/backend/assets/fonts/SourceHanSerifHK/SourceHanSerifHK-Bold.ttf",
        ],
    )?);
    let logo = resources.add_image(LOGO.to_vec(), ImageFormat::Png);
    let mark = resources.add_image(MARK.to_vec(), ImageFormat::Png);
    let diamond = resources.add_image(DIAMOND.to_vec(), ImageFormat::Png);

    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_new(28.0, 40.0, 28.0, 40.0)?,
            header: Some(BandSpec {
                inset: BandInset::PageMargins,
                left: Some(BandSlot::Text(vec![TextRun::Literal(
                    "恒誠商務有限公司".into(),
                )])),
                right: Some(BandSlot::Text(vec![
                    TextRun::Literal("第 ".into()),
                    TextRun::PageNumber,
                    TextRun::Literal(" / ".into()),
                    TextRun::PageCount,
                    TextRun::Literal(" 頁".into()),
                ])),
            }),
            footer: Some(BandSpec {
                inset: BandInset::PageMargins,
                left: Some(BandSlot::Text(vec![TextRun::Literal(
                    "Tel.: 00852 - 2668 1868".into(),
                )])),
                right: Some(BandSlot::Text(vec![TextRun::Literal(
                    "Web.: www.hc.biz".into(),
                )])),
            }),
            body: bill_body(logo, mark, diamond)?,
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

fn line(text: &str) -> Block {
    Block::paragraph(text)
}

fn aligned(text: &str, align: Alignment) -> Block {
    Block::Paragraph(ParagraphSpec {
        runs: vec![InlineRun::Text(text.into())],
        align,
        ..ParagraphSpec::default()
    })
}

fn styled(size: f32, color: Color, children: Vec<Block>) -> Result<Block> {
    Ok(Block::Styled {
        style: TextStyle {
            font_size: Some(pos(size)?),
            color: Some(color),
        },
        children,
    })
}

fn rule(
    width: Option<f32>,
    align: Alignment,
    thickness: f32,
    before: f32,
    after: f32,
) -> Result<Block> {
    Ok(Block::HLine(HLineSpec {
        width: width.map(pos).transpose()?,
        align,
        thickness: pos(thickness)?,
        color: NAVY,
        space_before: nn(before)?,
        space_after: nn(after)?,
    }))
}

fn ratio_col(weight: f32, blocks: Vec<Block>) -> ColumnSpec {
    ColumnSpec {
        width: Track::Ratio(Ratio::new(weight).expect("weight is positive")),
        blocks,
    }
}

fn filled(content: Vec<CellBlock>, fill: Color) -> CellSpec {
    CellSpec {
        content,
        fill: Some(fill),
        ..CellSpec::default()
    }
}

fn para_cell(text: &str, size: f32, color: Color) -> Result<CellSpec> {
    Ok(CellSpec {
        content: vec![CellBlock::Paragraph(ParagraphSpec {
            runs: vec![InlineRun::Text(text.into())],
            color: Some(color),
            font_size: Some(pos(size)?),
            ..ParagraphSpec::default()
        })],
        ..CellSpec::default()
    })
}

fn bill_body(logo: ImageRef, mark: ImageRef, diamond: ImageRef) -> Result<Vec<Block>> {
    let mut body = letterhead(logo, mark)?;
    body.extend(bill_to()?);
    body.extend(service_table()?);
    body.push(payment(diamond)?);
    body.extend(cheque_notes()?);
    body.extend(company_block()?);
    Ok(body)
}

fn letterhead(logo: ImageRef, mark: ImageRef) -> Result<Vec<Block>> {
    Ok(vec![
        Block::Columns(ColumnsSpec {
            gap: nn(12.0)?,
            columns: vec![
                ratio_col(
                    1.0,
                    vec![
                        Block::Image(ImageSpec {
                            image: logo,
                            width: Some(pos(120.0)?),
                            height: None,
                            align: Alignment::Start,
                        }),
                        Block::Spacer(nn(6.0)?),
                        styled(9.0, NAVY, vec![line("商 業 理 想 同 行 者")])?,
                        rule(None, Alignment::Center, 0.75, 8.0, 4.0)?,
                    ],
                ),
                ColumnSpec {
                    width: Track::Fixed(pos(56.0)?),
                    blocks: vec![Block::Image(ImageSpec {
                        image: mark,
                        width: Some(pos(52.0)?),
                        height: None,
                        align: Alignment::End,
                    })],
                },
            ],
        }),
        Block::Spacer(nn(16.0)?),
    ])
}

fn bill_to() -> Result<Vec<Block>> {
    Ok(vec![
        styled(16.0, NAVY, vec![line("Service Bill 服務賬單")])?,
        Block::Spacer(nn(18.0)?),
        styled(12.0, NAVY, vec![line("Bill To 計入客欠")])?,
        rule(Some(96.0), Alignment::Center, 0.75, 4.0, 4.0)?,
        rule(None, Alignment::End, 2.5, 0.0, 10.0)?,
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

fn service_table() -> Result<Vec<Block>> {
    let mut body = vec![vec![
        para_cell("綜合服務費用（含代支）", 11.0, BLACK)?,
        para_cell("505.00 HKD", 10.5, BLACK)?,
    ]];
    for _ in 0..3 {
        body.push(vec![
            CellSpec {
                content: vec![CellBlock::Spacer(nn(18.0)?)],
                ..CellSpec::default()
            },
            CellSpec {
                content: vec![CellBlock::Spacer(nn(18.0)?)],
                ..CellSpec::default()
            },
        ]);
    }
    Ok(vec![
        Block::Columns(ColumnsSpec {
            gap: nn(12.0)?,
            columns: vec![
                ratio_col(
                    1.0,
                    vec![styled(12.0, NAVY, vec![line("Service Summary 服務概要")])?],
                ),
                ratio_col(
                    1.0,
                    vec![styled(
                        9.0,
                        NAVY,
                        vec![aligned("日期：2025年10月28日", Alignment::End)],
                    )?],
                ),
            ],
        }),
        Block::Spacer(nn(8.0)?),
        Block::Table(TableSpec {
            header: vec![vec![
                filled(
                    vec![CellBlock::Paragraph(ParagraphSpec {
                        runs: vec![InlineRun::Text(
                            "Key item (s) / outlay (s) 主要項目/代支".into(),
                        )],
                        color: Some(WHITE),
                        font_size: Some(pos(11.0)?),
                        ..ParagraphSpec::default()
                    })],
                    NAVY,
                ),
                filled(
                    vec![CellBlock::Paragraph(ParagraphSpec {
                        runs: vec![InlineRun::Text("Amt. 金額".into())],
                        color: Some(WHITE),
                        font_size: Some(pos(10.5)?),
                        ..ParagraphSpec::default()
                    })],
                    NAVY,
                ),
            ]],
            body,
            tracks: Some(vec![
                Track::Ratio(Ratio::new(2.2).unwrap()),
                Track::Ratio(Ratio::new(1.0).unwrap()),
            ]),
            cell_padding: nn(8.0)?,
            border_width: nn(0.66)?,
            border_color: GRAY,
            ..TableSpec::default()
        }),
        Block::Spacer(nn(8.0)?),
        rule(None, Alignment::End, 2.5, 0.0, 8.0)?,
        styled(
            14.0,
            NAVY,
            vec![aligned("Bill Total 賬單總額: 505.00 HKD", Alignment::End)],
        )?,
        Block::Spacer(nn(16.0)?),
    ])
}

fn payment(diamond: ImageRef) -> Result<Block> {
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
        body.push(vec![
            CellSpec {
                content: vec![CellBlock::Image(ImageSpec {
                    image: diamond,
                    width: Some(pos(8.0)?),
                    height: None,
                    align: Alignment::Start,
                })],
                ..CellSpec::default()
            },
            para_cell(en, 8.0, NAVY)?,
            para_cell(zh, 8.0, NAVY)?,
        ]);
    }
    Ok(Block::Table(TableSpec {
        header: vec![vec![
            filled(vec![CellBlock::Spacer(nn(1.0)?)], NAVY),
            filled(
                vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![InlineRun::Text(
                        "Payment by bank transfer (T/T) and other info".into(),
                    )],
                    color: Some(WHITE),
                    font_size: Some(pos(8.0)?),
                    ..ParagraphSpec::default()
                })],
                NAVY,
            ),
            filled(
                vec![CellBlock::Paragraph(ParagraphSpec {
                    runs: vec![InlineRun::Text("以電匯付款及其他注意事項".into())],
                    color: Some(WHITE),
                    font_size: Some(pos(8.0)?),
                    ..ParagraphSpec::default()
                })],
                NAVY,
            ),
        ]],
        body,
        tracks: Some(vec![
            Track::Fixed(pos(22.0)?),
            Track::Ratio(Ratio::new(1.0).unwrap()),
            Track::Ratio(Ratio::new(1.0).unwrap()),
        ]),
        cell_padding: nn(4.0)?,
        border_width: nn(0.66)?,
        border_color: GRAY,
        ..TableSpec::default()
    }))
}

fn cheque_notes() -> Result<Vec<Block>> {
    Ok(vec![
        Block::Spacer(nn(12.0)?),
        styled(
            9.0,
            NAVY,
            vec![Block::Columns(ColumnsSpec {
                gap: nn(16.0)?,
                columns: vec![
                    ratio_col(
                        1.0,
                        vec![
                            line("Cheque made payable to \"HC BUSINESS LIMITED\""),
                            line("支票付款請以 \"恒誠商務有限公司\" 抬頭"),
                        ],
                    ),
                    ratio_col(
                        1.0,
                        vec![
                            line("FPS Unique ID: 3952652 (HK local bank transactions only)"),
                            line("快速支付編號: 3952652 (僅限香港本地銀行交易)"),
                        ],
                    ),
                ],
            })],
        )?,
    ])
}

fn company_block() -> Result<Vec<Block>> {
    Ok(vec![
        Block::Spacer(nn(20.0)?),
        rule(None, Alignment::Center, 0.75, 0.0, 8.0)?,
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
