//! HC service bill example — brand colors kept, layout is flow-first.
//!
//! Chinese face defaults to Source Han Serif HK via `.fonts/body.ttf` /
//! `.fonts/bold.ttf`, or override with `QUIRE_CJK_FONT` / `QUIRE_CJK_BOLD_FONT`.
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
    document, to_pdf, Cell, Color, Document, FlowWriter, Image, ImageFormat, Margins, PageSize,
    Track,
};

const NAVY: Color = Color::rgb(0x19, 0x2d, 0x74);
const GRAY: Color = Color::rgb(0xc5, 0xc5, 0xc5);
const BLACK: Color = Color::rgb(0x00, 0x00, 0x00);

const LOGO: &[u8] = include_bytes!("assets/logo.png");
const MARK: &[u8] = include_bytes!("assets/mark.png");
const DIAMOND: &[u8] = include_bytes!("assets/diamond.png");

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

    let rendered = document(PageSize::A4, |doc| {
        doc.font(body_font)?;
        doc.bold_font(bold_font)?;
        let logo = doc.load_image(LOGO.to_vec(), ImageFormat::Png)?;
        let mark = doc.load_image(MARK.to_vec(), ImageFormat::Png)?;
        let diamond = doc.load_image(DIAMOND.to_vec(), ImageFormat::Png)?;
        decorate_pages(doc);
        write_letterhead(doc, &logo, &mark);
        write_body(doc, &diamond);
        write_company_block(doc);
        Ok(())
    })?;
    let pdf = to_pdf(rendered)?;

    fs::write(&out, &pdf).expect("write PDF");
    eprintln!("wrote {} ({} bytes)", out.display(), pdf.len());
    Ok(())
}

fn decorate_pages(doc: &mut Document<'_>) {
    // Top/bottom margins are also the header/footer band heights.
    doc.margins(Margins::try_new(28.0, 40.0, 28.0, 40.0).expect("margins"));
    doc.header(|h| {
        let m = h.page().margins;
        h.inset(m.left(), m.right());
        h.left("恒誠商務有限公司");
        h.right(format!("第 {} / {} 頁", h.page().number, h.page().count));
    });
    doc.footer(|f| {
        let m = f.page().margins;
        f.inset(m.left(), m.right());
        f.left("Tel.: 00852 - 2668 1868");
        f.right("Web.: www.hc.biz");
    });
}

fn write_letterhead(flow: &mut impl FlowWriter, logo: &Image, mark: &Image) {
    flow.columns(|row| {
        row.gap(12.0);
        row.col(|col| {
            col.image(logo, |image| {
                image.width(120.0);
            });
            col.spacer(6.0);
            col.stack(
                |s| {
                    s.font_size(9.0);
                    s.color(NAVY);
                },
                |col| {
                    col.paragraph(|p| p.text("商 業 理 想 同 行 者"));
                },
            );
            col.hline(|h| {
                h.thickness(0.75);
                h.color(NAVY);
                h.space_before(8.0);
                h.space_after(4.0);
            });
        });
        row.col_fixed(56.0, |col| {
            col.image(mark, |image| {
                image.width(52.0);
                image.end();
            });
        });
    });
    flow.spacer(16.0);
}

fn write_body(flow: &mut impl FlowWriter, diamond: &Image) {
    write_bill_to(flow);
    write_service_table(flow);
    write_payment(flow, diamond);
    write_cheque_notes(flow);
}

fn write_bill_to(flow: &mut impl FlowWriter) {
    flow.stack(
        |s| {
            s.font_size(16.0);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| p.text("Service Bill 服務賬單"));
        },
    );
    flow.spacer(18.0);
    flow.stack(
        |s| {
            s.font_size(12.0);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| p.text("Bill To 計入客欠"));
        },
    );
    flow.hline(|h| {
        h.width(96.0);
        h.thickness(0.75);
        h.color(NAVY);
        h.space_before(4.0);
        h.space_after(4.0);
    });
    flow.hline(|h| {
        h.end();
        h.thickness(2.5);
        h.color(NAVY);
        h.space_after(10.0);
    });
    flow.stack(
        |s| {
            s.font_size(11.0);
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

fn write_service_table(flow: &mut impl FlowWriter) {
    flow.columns(|row| {
        row.col(|col| {
            col.stack(
                |s| {
                    s.font_size(12.0);
                    s.color(NAVY);
                },
                |col| {
                    col.paragraph(|p| p.text("Service Summary 服務概要"));
                },
            );
        });
        row.col(|col| {
            col.stack(
                |s| {
                    s.font_size(9.0);
                    s.color(NAVY);
                },
                |col| {
                    col.paragraph(|p| {
                        p.end();
                        p.text("日期：2025年10月28日");
                    });
                },
            );
        });
    });
    flow.spacer(8.0);
    flow.table(|t| {
        t.cell_padding(8.0);
        t.border(0.66, GRAY);
        t.tracks([Track::Ratio(2.2), Track::Ratio(1.0)]);
        t.header([
            Cell::paragraph(|p| {
                p.color(Color::rgb(0xff, 0xff, 0xff));
                p.font_size(11.0);
                p.text("Key item (s) / outlay (s) 主要項目/代支");
            })
            .fill(NAVY),
            Cell::paragraph(|p| {
                p.color(Color::rgb(0xff, 0xff, 0xff));
                p.font_size(10.5);
                p.text("Amt. 金額");
            })
            .fill(NAVY),
        ]);
        t.row([
            Cell::paragraph(|p| {
                p.font_size(11.0);
                p.text("綜合服務費用（含代支）");
            }),
            Cell::paragraph(|p| {
                p.font_size(10.5);
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
        h.thickness(2.5);
        h.color(NAVY);
        h.space_after(8.0);
    });
    flow.stack(
        |s| {
            s.font_size(14.0);
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

fn write_payment(flow: &mut impl FlowWriter, diamond: &Image) {
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
                p.color(Color::rgb(0xff, 0xff, 0xff));
                p.font_size(8.0);
                p.text("Payment by bank transfer (T/T) and other info");
            })
            .fill(NAVY),
            Cell::paragraph(|p| {
                p.color(Color::rgb(0xff, 0xff, 0xff));
                p.font_size(8.0);
                p.text("以電匯付款及其他注意事項");
            })
            .fill(NAVY),
        ]);
        for (en, zh) in rows {
            t.row([
                Cell::image(diamond, |image| {
                    image.width(8.0);
                }),
                Cell::paragraph(|p| {
                    p.font_size(8.0);
                    p.color(NAVY);
                    p.text(en);
                }),
                Cell::paragraph(|p| {
                    p.font_size(8.0);
                    p.color(NAVY);
                    p.text(zh);
                }),
            ]);
        }
    });
}

fn write_cheque_notes(flow: &mut impl FlowWriter) {
    flow.spacer(12.0);
    flow.stack(
        |s| {
            s.font_size(9.0);
            s.color(NAVY);
        },
        |flow| {
            flow.columns(|row| {
                row.gap(16.0);
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

fn write_company_block(flow: &mut impl FlowWriter) {
    flow.spacer(20.0);
    flow.hline(|h| {
        h.thickness(0.75);
        h.color(NAVY);
        h.space_after(8.0);
    });
    flow.stack(
        |s| {
            s.font_size(8.0);
            s.color(NAVY);
        },
        |flow| {
            flow.paragraph(|p| p.text("恒誠商務有限公司 · HC BUSINESS LIMITED"));
            flow.paragraph(|p| {
                p.text("Add.: 香港九龍尖沙咀漆咸道南45-51號其士大廈801室");
            });
        },
    );
}

fn default_out_path() -> PathBuf {
    PathBuf::from("out/bill.pdf")
}

fn load_font(env_key: &str, candidates: &[&str]) -> quire_pdf::Result<Vec<u8>> {
    if let Ok(path) = env::var(env_key) {
        return fs::read(&path).map_err(|error| {
            quire_pdf::Error::from(quire_pdf::ErrorKind::Font {
                index: 0,
                message: format!("read {path}: {error}"),
            })
        });
    }
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return fs::read(path).map_err(|error| {
                quire_pdf::Error::from(quire_pdf::ErrorKind::Font {
                    index: 0,
                    message: format!("read {}: {error}", path.display()),
                })
            });
        }
    }
    Err(quire_pdf::Error::from(quire_pdf::ErrorKind::Font {
        index: 0,
        message: format!("set {env_key} or place a CJK TTF at {}", candidates[0]),
    }))
}
