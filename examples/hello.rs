//! Multi-page smoke demo that walks most of the public flow DSL.
//!
//! ```text
//! cargo run --example hello
//! ```
//!
//! Writes `out/hello.pdf` (gitignored). Pass another path as argv[1].
//! The bundled fixture maps printable ASCII, U+2022, and 第 / 页 / 共. Other
//! missing glyphs fail the document.

use std::env;
use std::fs;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::PathBuf;

use quire_pdf::{
    document, Cell, Color, Document, Flow, Image, ImageFormat, Margins, Numbering, PageSize, Track,
};

const FONT: &[u8] = include_bytes!("../tests/fixtures/quire-measure.ttf");
const PIXEL: &[u8] = include_bytes!("../tests/fixtures/one-pixel.png");

fn main() -> quire_pdf::Result<()> {
    let out = env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("out/hello.pdf"), PathBuf::from);
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create output dir");
    }

    let pdf = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.bold_font(FONT.to_vec())?;
        let logo = doc.load_image(PIXEL.to_vec(), ImageFormat::Png)?;
        decorate_pages(doc)?;
        write_title(doc, &logo);
        write_lists(doc);
        write_table(doc, &logo);
        doc.page_break();
        write_closing(doc);
        Ok(())
    })?
    .to_pdf()?;

    fs::write(&out, &pdf).expect("write PDF");
    eprintln!("wrote {} ({} bytes)", out.display(), pdf.len());
    Ok(())
}

fn decorate_pages(doc: &mut Document<'_>) -> quire_pdf::Result<()> {
    doc.margins(Margins::try_uniform(48.0)?);
    doc.header(|h| {
        let m = h.page().margins;
        h.inset(m.left(), m.right());
        h.left_link("https://example.test/quire", "quire demo");
        h.right(format!(
            "第 {} 页 / 共 {} 页",
            h.page().number,
            h.page().count
        ));
    });
    doc.footer(|f| {
        let m = f.page().margins;
        f.inset(m.left(), m.right());
        f.left("layout smoke");
        f.right_link("https://example.test/footer", "footer link");
    });
    Ok(())
}

fn write_title(flow: &mut Flow, logo: &Image) {
    flow.paragraph(|p| {
        p.center();
        p.bold("Quire layout tour");
    });
    flow.hline(|h| {
        h.width(220.0);
        h.thickness(1.25);
        h.color(Color::rgb(40, 40, 40));
        h.space_before(6.0);
        h.space_after(12.0);
    });
    flow.image(logo, |image| {
        image.width(36.0);
        image.center();
    });
    flow.spacer(10.0);
    flow.paragraph(|p| {
        p.first_line_indent(18.0);
        p.text("This example exercises paragraph runs, ");
        p.bold("bold");
        p.text(", ");
        p.link("https://example.test/docs", |p| p.text("links"));
        p.text(", blanks, lists, columns, tables, images, keep-together,");
        p.text(" and page breaks.");
    });
    flow.paragraph(|p| {
        p.text("Signed: ");
        p.blank(96.0).thickness(0.75).color(Color::rgb(0, 0, 0));
        p.text("  date ");
        p.blank_em(6).thickness(0.75).baseline_offset(1.5);
    });
    flow.paragraph(|p| {
        p.text("Notes");
        p.blank_to_end()
            .thickness(0.5)
            .color(Color::rgb(90, 90, 90));
    });
}

fn write_lists(flow: &mut Flow) {
    flow.columns(|row| {
        row.gap(16.0);
        row.col(|col| {
            col.paragraph(|p| p.bold("Bullets"));
            col.ul(|list| {
                list.marker_width(16.0);
                list.nest_indent(20.0);
                list.item(|item| {
                    item.paragraph(|p| p.text("measure with skrifa"));
                    item.ul(|inner| {
                        inner.item(|n| n.paragraph(|p| p.text("cluster")));
                        inner.item(|n| n.paragraph(|p| p.text("advance")));
                    });
                });
                list.item(|item| item.paragraph(|p| p.text("break lines in the frame")));
                list.item_marked("*", |item| {
                    item.paragraph(|p| p.text("custom marker item"));
                });
            });
        });
        row.col_ratio(1.2, |col| {
            col.paragraph(|p| p.bold("Ordered"));
            col.ol(Numbering::LowerAlpha, |list| {
                list.start(NonZeroU32::new(2).unwrap());
                list.item(|item| item.paragraph(|p| p.text("paginate")));
                list.item(|item| item.paragraph(|p| p.text("decorate")));
                list.item_marked("(x)", |item| {
                    item.paragraph(|p| p.text("marked, no number burn"));
                });
                list.item(|item| item.paragraph(|p| p.text("emit PDF")));
            });
        });
    });
    flow.hline(|h| {
        h.start();
        h.width(160.0);
        h.space_before(14.0);
        h.space_after(10.0);
    });
}

fn write_table(flow: &mut Flow, logo: &Image) {
    flow.paragraph(|p| p.bold("Shipment table"));
    flow.table(|t| {
        t.repeat_header(true);
        t.cell_padding(4.0);
        t.border(0.5, Color::rgb(30, 30, 30));
        t.tracks([
            Track::Fixed(54.0),
            Track::Ratio(2.0),
            Track::Ratio(1.0),
            Track::Fixed(48.0),
        ]);
        t.header([
            Cell::paragraph(|p| {
                p.color(Color::rgb(255, 255, 255));
                p.bold("Code");
            })
            .fill(Color::rgb(40, 40, 40)),
            Cell::paragraph(|p| {
                p.color(Color::rgb(255, 255, 255));
                p.bold("Item");
            })
            .fill(Color::rgb(40, 40, 40)),
            Cell::paragraph(|p| {
                p.color(Color::rgb(255, 255, 255));
                p.bold("Notes");
            })
            .fill(Color::rgb(40, 40, 40)),
            Cell::paragraph(|p| {
                p.color(Color::rgb(255, 255, 255));
                p.bold("Qty");
            })
            .fill(Color::rgb(40, 40, 40)),
        ]);
        t.header([Cell::new("group A").col_span(NonZeroU16::new(4).unwrap())]);
        t.row([
            Cell::new("A1").row_span(NonZeroU16::new(2).unwrap()),
            Cell::paragraph(|p| {
                p.text("Pen ");
                p.bold("set");
            }),
            Cell::ul(|list| {
                list.item(|item| item.paragraph(|p| p.text("black")));
                list.item(|item| item.paragraph(|p| p.text("blue")));
            }),
            Cell::new("12"),
        ]);
        t.row([
            Cell::paragraph(|p| {
                p.link("https://example.test/ink", |p| p.text("Ink"));
            }),
            Cell::ol(Numbering::Decimal, |list| {
                list.item(|item| item.paragraph(|p| p.text("bottle")));
                list.item_marked("+", |item| item.paragraph(|p| p.text("refill")));
            }),
            Cell::new("3"),
        ]);
        t.row([
            Cell::new("B2"),
            Cell::image(logo, |image| {
                image.width(18.0);
                image.center();
            }),
            Cell::hline(|h| {
                h.thickness(0.75);
                h.color(Color::rgb(120, 0, 0));
            }),
            Cell::spacer(8.0),
        ]);
        for n in 0..36 {
            t.row([
                format!("R{n}"),
                format!("part {n}"),
                "stock".to_owned(),
                format!("{}", (n % 9) + 1),
            ]);
        }
        t.row([Cell::new("Total rows above").col_span(NonZeroU16::new(4).unwrap())]);
    });
}

fn write_closing(flow: &mut Flow) {
    flow.keep_together(|g| {
        g.paragraph(|p| {
            p.center();
            p.bold("Keep-together block");
        });
        g.paragraph(|p| {
            p.text("These two lines move as one piece when the remaining");
            p.text(" frame is too short.");
        });
        g.hline(|h| {
            h.end();
            h.width(120.0);
            h.space_before(4.0);
        });
    });
    flow.spacer(8.0);
    flow.columns(|row| {
        row.col_fixed(140.0, |col| {
            col.paragraph(|p| p.bold("Fixed column"));
            col.paragraph(|p| p.text("Narrow frame wraps AA AA AA AA AA words here."));
            col.ol(Numbering::UpperRoman, |list| {
                list.item(|item| item.paragraph(|p| p.text("one")));
                list.item(|item| item.paragraph(|p| p.text("two")));
            });
        });
        row.col_auto(|col| {
            col.paragraph(|p| p.bold("Auto column"));
            col.table(|t| {
                t.border(0.4, Color::rgb(0, 0, 0));
                t.cell_padding(3.0);
                t.header(["Side", "Val"]);
                t.row(["L", "1"]);
                t.row(["R", "2"]);
            });
            col.paragraph(|p| {
                p.end();
                p.text("end-aligned note");
            });
        });
    });
    flow.spacer(16.0);
    flow.paragraph(|p| {
        p.center();
        p.text("End of tour.");
    });
}
