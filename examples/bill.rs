//! Recreate the sample HC service bill (`00000955-…账单-….pdf`).
//!
//! Brand color is `#192d74`. Chinese face defaults to Source Han Serif HK via
//! `.fonts/body.ttf` / `.fonts/bold.ttf`, or override with `QUIRE_CJK_FONT` /
//! `QUIRE_CJK_BOLD_FONT`.
//!
//! ```text
//! cargo run --example bill
//! ```
//!
//! Writes `out/bill.pdf` (gitignored). Pass another path as argv[1].

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use quire_pdf::{
    document, Cell, Color, Flow, Image, ImageFormat, Ink, Layer, Margins, PageSize, Size, Track,
};

const NAVY: Color = Color::rgb(0x19, 0x2d, 0x74);
const GRAY: Color = Color::rgb(0xc5, 0xc5, 0xc5);
const ORANGE: Color = Color::rgb(0xff, 0x55, 0x20);
const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);
const BLACK: Color = Color::rgb(0x00, 0x00, 0x00);

const LOGO: &[u8] = include_bytes!("assets/logo.png");
const MARK: &[u8] = include_bytes!("assets/mark.png");
const DIAMOND: &[u8] = include_bytes!("assets/diamond.png");

const PAGE_W: f32 = 594.96;
const CONTENT_L: f32 = 33.08;
const CONTENT_R: f32 = 562.42;
const CONTENT_W: f32 = CONTENT_R - CONTENT_L;

fn main() -> quire_pdf::Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(default_out_path, PathBuf::from);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create output dir");
    }

    let body_font = load_font(
        "QUIRE_CJK_FONT",
        &[
            ".fonts/body.ttf",
            "/mnt/data/apps/hcsw-pdfcraft/backend/assets/fonts/SourceHanSerifHK/SourceHanSerifHK-Regular.ttf",
        ],
    )?;
    let bold_font = load_font(
        "QUIRE_CJK_BOLD_FONT",
        &[
            ".fonts/bold.ttf",
            "/mnt/data/apps/hcsw-pdfcraft/backend/assets/fonts/SourceHanSerifHK/SourceHanSerifHK-Bold.ttf",
        ],
    )?;

    let page = PageSize::custom(Size::try_pt(PAGE_W, 841.92)?);
    let pdf = document(page, |doc| {
        doc.font(body_font)?;
        doc.bold_font(bold_font)?;
        let logo = doc.load_image(LOGO.to_vec(), ImageFormat::Png)?;
        let mark = doc.load_image(MARK.to_vec(), ImageFormat::Png)?;
        let diamond = doc.load_image(DIAMOND.to_vec(), ImageFormat::Png)?;
        doc.margins(Margins::try_new(90.0, 32.54, 50.0, 33.08)?);
        doc.behind(move |layer| paint_chrome(layer, &logo, &mark));
        write_body(doc, &diamond);
        Ok(())
    })?
    .to_pdf()?;

    fs::write(&out, &pdf).expect("write PDF");
    eprintln!("wrote {} ({} bytes)", out.display(), pdf.len());
    Ok(())
}

fn paint_chrome(layer: &mut Layer, logo: &Image, mark: &Image) {
    layer.image_at(logo, (CONTENT_L, 33.08, 109.18, 13.90));
    layer
        .text_at("商 業 理 想 同 行 者", (471.8, 34.6))
        .font_size(7.9)
        .color(NAVY);
    layer.image_at(mark, (509.48, 72.78, 52.93, 52.93));
    layer.rect((CONTENT_L, 52.27, CONTENT_W, 0.66), Ink::Fill(NAVY));

    layer.rect((0.0, 797.97, PAGE_W, 4.63), Ink::Fill(ORANGE));
    layer.rect((0.0, 802.60, PAGE_W, 39.70), Ink::Fill(NAVY));
    layer.stack(
        |s| {
            s.font_size(7.9);
            s.color(WHITE);
        },
        |layer| {
            layer.text_at("恒誠商務有限公司", (57.2, 810.7));
            layer.text_at(
                "Add.: 香港九龍尖沙咀漆咸道南45-51號其士大廈801室",
                (144.5, 810.7),
            );
            layer.text_at("Tel.: 00852 - 2668 1868", (361.2, 810.7));
            layer.text_at("Web.: www.hc.biz", (472.5, 810.7));
        },
    );
}

fn write_body(flow: &mut Flow, diamond: &Image) {
    write_bill_to(flow);
    write_service_table(flow);
    write_payment(flow, diamond);
    write_cheque_notes(flow);
}

fn write_bill_to(flow: &mut Flow) {
    flow.spacer(40.0);
    flow.stack(
        |s| {
            s.font_size(16.5);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| p.text("Service Bill 服務賬單"));
        },
    );
    flow.spacer(24.0);
    flow.stack(
        |s| {
            s.font_size(12.6);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| p.text("Bill To 計入客欠"));
        },
    );
    flow.spacer(4.0);
    flow.hline(|h| {
        h.width(92.63);
        h.thickness(0.66);
        h.color(NAVY);
        h.space_after(8.0);
    });
    flow.hline(|h| {
        h.end();
        h.thickness(3.31);
        h.color(NAVY);
        h.space_after(10.0);
    });
    flow.stack(
        |s| {
            s.font_size(11.2);
            s.color(BLACK);
        },
        |flow| {
            flow.paragraph(|p| p.text("00000955"));
            flow.paragraph(|p| p.text("教育有限公司"));
            flow.paragraph(|p| p.text("Educationist Limited"));
        },
    );
    flow.spacer(16.0);
}

fn write_service_table(flow: &mut Flow) {
    flow.stack(
        |s| {
            s.font_size(12.6);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| p.text("Service Summary 服務概要"));
        },
    );
    flow.stack(
        |s| {
            s.font_size(9.3);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| {
                p.end();
                p.text("日期：2025年10月28日");
            });
        },
    );
    flow.spacer(8.0);
    flow.table(|t| {
        t.cell_padding(8.0);
        t.border(0.66, GRAY);
        t.tracks([Track::Ratio(2.2), Track::Ratio(1.0)]);
        t.header([
            Cell::paragraph(|p| {
                p.color(WHITE);
                p.font_size(11.2);
                p.text("Key item (s) / outlay (s) 主要項目/代支");
            })
            .fill(NAVY),
            Cell::paragraph(|p| {
                p.color(WHITE);
                p.font_size(10.6);
                p.text("Amt. 金額");
            })
            .fill(NAVY),
        ]);
        t.row([
            Cell::paragraph(|p| {
                p.font_size(11.2);
                p.text("綜合服務費用（含代支）");
            }),
            Cell::paragraph(|p| {
                p.font_size(10.6);
                p.text("505.00 HKD");
            }),
        ]);
        for _ in 0..3 {
            t.row([Cell::spacer(18.0), Cell::spacer(18.0)]);
        }
    });
    flow.spacer(8.0);
    flow.hline(|h| {
        h.end();
        h.thickness(3.31);
        h.color(NAVY);
        h.space_after(8.0);
    });
    flow.stack(
        |s| {
            s.font_size(14.6);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| {
                p.end();
                p.text("Bill Total 賬單總額: 505.00 HKD");
            });
        },
    );
    flow.spacer(16.0);
}

fn write_cheque_notes(flow: &mut Flow) {
    flow.spacer(12.0);
    flow.stack(
        |s| {
            s.font_size(9.3);
            s.color(NAVY);
        },
        |flow| {
            flow.columns(|row| {
                row.col(|col| {
                    col.paragraph(|p| {
                        p.text("Cheque made payable to \"HC BUSINESS LIMITED\"");
                    });
                    col.paragraph(|p| {
                        p.text("支票付款請以 \"恒誠商務有限公司\" 抬頭");
                    });
                });
                row.col(|col| {
                    col.paragraph(|p| {
                        p.text("FPS Unique ID: 3952652 (HK local bank transactions only)");
                    });
                    col.paragraph(|p| {
                        p.text("快速支付編號: 3952652 (僅限香港本地銀行交易)");
                    });
                });
            });
        },
    );
}

fn write_payment(flow: &mut Flow, diamond: &Image) {
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

    flow.table(|t| {
        t.cell_padding(4.0);
        t.border(0.66, GRAY);
        t.tracks([Track::Fixed(22.0), Track::Ratio(1.0), Track::Ratio(1.0)]);
        t.header([
            Cell::spacer(1.0).fill(NAVY),
            Cell::paragraph(|p| {
                p.color(WHITE);
                p.font_size(7.9);
                p.text("Payment by bank transfer (T/T) and other info");
            })
            .fill(NAVY),
            Cell::paragraph(|p| {
                p.color(WHITE);
                p.font_size(7.9);
                p.text("以電匯付款及其他注意事項");
            })
            .fill(NAVY),
        ]);
        for (en, zh) in rows {
            t.row([
                Cell::image(diamond, |image| {
                    image.width(7.94);
                }),
                Cell::paragraph(|p| {
                    p.font_size(7.9);
                    p.color(NAVY);
                    p.text(en);
                }),
                Cell::paragraph(|p| {
                    p.font_size(7.9);
                    p.color(NAVY);
                    p.text(zh);
                }),
            ]);
        }
    });
}

fn default_out_path() -> PathBuf {
    PathBuf::from("out/bill.pdf")
}

fn load_font(env_key: &str, candidates: &[&str]) -> quire_pdf::Result<Vec<u8>> {
    if let Ok(path) = env::var(env_key) {
        return fs::read(&path).map_err(|error| quire_pdf::Error::Font {
            index: 0,
            message: format!("read {path}: {error}"),
        });
    }
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return fs::read(path).map_err(|error| quire_pdf::Error::Font {
                index: 0,
                message: format!("read {}: {error}", path.display()),
            });
        }
    }
    Err(quire_pdf::Error::Font {
        index: 0,
        message: format!("set {env_key} or place a CJK TTF at {}", candidates[0]),
    })
}
