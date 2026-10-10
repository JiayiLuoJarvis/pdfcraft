//! Public `document` DSL against recorded canvas commands.

use std::num::NonZeroU16;

#[cfg(feature = "backend-krilla")]
use quire_pdf::Paper;
use quire_pdf::{
    document, pt_near, CanvasCommand, Cell, Color, Error, Flow, ImageFormat, Ink, Margins,
    NonNegative, Numbering, PageSize, Path, PathVerb, Pen, Point, Positive, RecordedPage, Rect,
    Rendered, Result, Size, Stroke, A4,
};

const FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf");
const ONE_PIXEL: &[u8] = include_bytes!("fixtures/one-pixel.png");
#[cfg(feature = "backend-krilla")]
const BAD_IDAT: &[u8] = include_bytes!("fixtures/bad-idat.png");
/// Fixture ascent at 12 pt: the baseline below the line-box top.
const BASELINE: f32 = 13.812;
/// Fixture ascent minus descent at 12 pt, which beats the 1.2 factor.
const LINE: f32 = 17.244;

struct GlyphSample {
    text: String,
    font: u32,
    x: f32,
    y: f32,
    width: f32,
}

#[derive(PartialEq)]
enum Kind {
    A,
    B,
}

struct Item {
    name: String,
    amount: String,
}

struct Contract {
    id: String,
    kind: Kind,
    items: Vec<Item>,
    party_a: String,
    party_b: String,
}

fn contract(kind: Kind) -> Contract {
    let item = |name: &str, amount: &str| Item {
        name: name.into(),
        amount: amount.into(),
    };
    Contract {
        id: "C-18".into(),
        kind,
        items: vec![item("Pen", "3"), item("Ink", "5")],
        party_a: "Party A".into(),
        party_b: "Party B".into(),
    }
}

fn runs(page: &RecordedPage) -> Vec<GlyphSample> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(GlyphSample {
                text: run.text.clone(),
                font: run.font.as_u32(),
                x: run.origin.x().as_pt(),
                y: run.origin.y().as_pt(),
                width: run.glyphs.iter().map(|g| g.advance_norm).sum::<f32>()
                    * run.font_size.as_pt(),
            }),
            _ => None,
        })
        .collect()
}

fn texts(page: &RecordedPage) -> Vec<String> {
    runs(page).into_iter().map(|run| run.text).collect()
}

fn first_example(c: &Contract) -> Result<Rendered> {
    document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text(format!("Contract {}", c.id)));
        if c.kind == Kind::A {
            doc.paragraph(|p| p.text("Plan A."));
            doc.table(|t| {
                t.header(["Item", "Amount"]);
                for item in &c.items {
                    t.row([&item.name, &item.amount]);
                }
            });
        } else {
            doc.paragraph(|p| p.text("Plan B."));
            doc.paragraph(|p| p.text("Plan B has no table."));
        }
        Ok(())
    })
}

#[test]
fn kind_a_flow_is_two_paragraphs_then_a_table() {
    let pages = first_example(&contract(Kind::A)).unwrap().pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(
        texts(&pages[0]),
        [
            "Contract C-18",
            "Plan A.",
            "Item",
            "Amount",
            "Pen",
            "3",
            "Ink",
            "5"
        ]
    );
    let r = runs(&pages[0]);
    let half = A4.width().as_pt() / 2.0;
    assert!(pt_near(r[2].y, 2.0 * LINE + BASELINE));
    assert!(pt_near(r[3].x, half));
    assert!(pt_near(r[3].y, r[2].y));
}

#[test]
fn kind_b_flow_has_no_table() {
    let pages = first_example(&contract(Kind::B)).unwrap().pages();
    assert_eq!(
        texts(&pages[0]),
        ["Contract C-18", "Plan B.", "Plan B has no table."]
    );
}

fn full_template(c: &Contract, company: &str) -> Result<Rendered> {
    document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(36.0)?));
        doc.header(|h| {
            h.left(company);
            h.right(format!(
                "第 {} 页 / 共 {} 页",
                h.page().number,
                h.page().count
            ));
        });
        doc.paragraph(|p| {
            p.text("Contract no. ");
            p.text(&c.id);
        });
        doc.page_break();
        doc.columns(|row| {
            row.col(|col| col.paragraph(|p| p.text(&c.party_a)));
            row.col(|col| col.paragraph(|p| p.text(&c.party_b)));
        });
        doc.behind(|page| page.text_at("DRAFT", Point::try_pt(180.0, 400.0).unwrap()));
        doc.front(|page| {
            if page.is_last() {
                page.text_at("SEAL", Point::try_pt(430.0, 680.0).unwrap());
            }
        });
        Ok(())
    })
}

#[test]
fn header_numbers_every_page_and_layers_stack_behind_header_body_footer_front() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(
        texts(&pages[0]),
        ["DRAFT", "ACME", "第 1 页 / 共 2 页", "Contract no. C-18"]
    );
    assert_eq!(
        texts(&pages[1]),
        [
            "DRAFT",
            "ACME",
            "第 2 页 / 共 2 页",
            "Party A",
            "Party B",
            "SEAL"
        ]
    );
}

#[test]
fn header_slots_inset_by_side_margins_and_body_sits_below_the_band() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let r = runs(&pages[0]);
    let (left, right, body) = (&r[1], &r[2], &r[3]);
    assert!(pt_near(left.x, 36.0));
    assert!(pt_near(left.y, BASELINE));
    assert!(pt_near(right.x + right.width, A4.width().as_pt() - 36.0));
    assert!(pt_near(right.y, BASELINE));
    assert!(pt_near(body.x, 36.0));
    assert!(pt_near(body.y, 36.0 + BASELINE));
}

#[test]
fn columns_split_body_width_with_a_12pt_gap() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let r = runs(&pages[1]);
    let col_w = (A4.width().as_pt() - 72.0 - 12.0) / 2.0;
    assert!(pt_near(r[3].x, 36.0));
    assert!(pt_near(r[4].x, 36.0 + col_w + 12.0));
    assert!(pt_near(r[3].y, 36.0 + BASELINE));
    assert!(pt_near(r[4].y, 36.0 + BASELINE));
}

#[test]
fn text_at_places_the_top_left_of_the_line_box() {
    let pages = full_template(&contract(Kind::A), "ACME").unwrap().pages();
    let draft = &runs(&pages[0])[0];
    assert!(pt_near(draft.x, 180.0));
    assert!(pt_near(draft.y, 400.0 + BASELINE));
    let seal = runs(&pages[1]).pop().unwrap();
    assert!(pt_near(seal.x, 430.0));
    assert!(pt_near(seal.y, 680.0 + BASELINE));
}

#[test]
fn footer_sees_section_counters_equal_to_document_totals() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(20.0)?));
        doc.footer(|f| {
            let page = f.page();
            f.left(format!("{}/{}", page.section_number, page.section_count));
        });
        doc.paragraph(|p| p.text("A"));
        doc.page_break();
        doc.paragraph(|p| p.text("B"));
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(texts(&pages[0]), ["A", "1/2"]);
    assert_eq!(texts(&pages[1]), ["B", "2/2"]);
    let footer = &runs(&pages[0])[1];
    assert!(pt_near(footer.y, A4.height().as_pt() - 20.0 + BASELINE));
}

#[test]
fn a_later_header_call_replaces_the_earlier_one() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.margins(Margins::uniform(NonNegative::pt(20.0)?));
        doc.header(|h| h.left("old"));
        doc.header(|h| h.right("new"));
        doc.paragraph(|p| p.text("A"));
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(texts(&pages[0]), ["new", "A"]);
}

#[test]
fn header_with_zero_margin_is_rejected() {
    let result = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.header(|h| h.left("H"));
        doc.paragraph(|p| p.text("A"));
        Ok(())
    });
    assert!(matches!(
        result,
        Err(Error::DecorationTallerThanMargin { height, margin })
            if pt_near(height, LINE) && margin == 0.0
    ));
}

#[test]
fn lists_number_bullet_and_custom_markers() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.ul(|l| l.item(|i| i.paragraph(|p| p.text("dot"))));
        doc.ol(Numbering::UpperRoman, |l| {
            l.item(|i| i.paragraph(|p| p.text("one")));
            l.item(|i| i.paragraph(|p| p.text("two")));
            l.item_marked("(c)", |i| i.paragraph(|p| p.text("custom")));
        });
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(
        texts(&pages[0]),
        ["\u{2022}", "dot", "I.", "one", "II.", "two", "(c)", "custom"]
    );
    let r = runs(&pages[0]);
    assert!(pt_near(r[0].x, 0.0));
    assert!(pt_near(r[1].x, 18.0));
}

#[test]
fn table_spans_and_repeat_header_switch() {
    let build = |repeat: bool| {
        document(PageSize::A4, |doc| {
            doc.font(FONT.to_vec())?;
            doc.table(|t| {
                t.repeat_header(repeat);
                t.header(["Item", "Amount"]);
                for n in 0..80 {
                    t.row([format!("row {n}"), n.to_string()]);
                }
                t.row([Cell::new("Total").col_span(NonZeroU16::new(2).unwrap())]);
            });
            Ok(())
        })
        .unwrap()
        .pages()
    };
    let on = build(true);
    let off = build(false);
    assert_eq!(on.len(), 2);
    assert_eq!(texts(&on[1])[..3], ["Item", "Amount", "row 47"]);
    assert_eq!(texts(&off[1])[..2], ["row 47", "47"]);
    let total = runs(on.last().unwrap()).pop().unwrap();
    assert_eq!(total.text, "Total");
    assert!(pt_near(total.x, 0.0));
}

#[test]
fn keep_together_spacer_and_page_break() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text("A"));
        doc.spacer(NonNegative::pt(A4.height().as_pt() - 20.0)?);
        doc.keep_together(|g| {
            g.paragraph(|p| p.text("B1"));
            g.paragraph(|p| p.text("B2"));
        });
        doc.page_break();
        doc.paragraph(|p| p.text("C"));
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(
        pages.iter().map(texts).collect::<Vec<_>>(),
        [vec!["A"], vec!["B1", "B2"], vec!["C"]]
    );
    assert!(pt_near(runs(&pages[1])[0].y, BASELINE));
}

#[test]
fn a_paragraph_wraps_at_the_width_of_its_column() {
    let page = Size::try_pt(200.0, 400.0).unwrap();
    let words = "AA AA AA AA AA AA";
    let lines = |in_column: bool| {
        let pages = document(PageSize::custom(page), |doc| {
            doc.font(FONT.to_vec())?;
            if in_column {
                doc.columns(|row| {
                    row.col(|c| c.paragraph(|p| p.text(words)));
                    row.col(|c| c.paragraph(|p| p.text("B")));
                });
            } else {
                doc.paragraph(|p| p.text(words));
            }
            Ok(())
        })
        .unwrap()
        .pages();
        runs(&pages[0])
    };
    let col_w = (200.0 - 12.0) / 2.0;
    let column = lines(true);
    let wrapped: Vec<_> = column.iter().filter(|run| run.text != "B").collect();
    assert!(wrapped.len() > 1);
    assert!(wrapped.iter().all(|run| run.x + run.width <= col_w));
    assert_eq!(lines(false).len(), 1);
}

#[test]
fn nesting_rules_come_from_layout() {
    let result = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.columns(|row| {
            row.col(Flow::page_break);
            row.col(|c| c.paragraph(|p| p.text("x")));
        });
        Ok(())
    });
    assert_eq!(
        result.err(),
        Some(Error::InvalidCellContent {
            container: "column",
            node: "page break"
        })
    );
}

#[test]
fn first_builder_error_wins_and_later_calls_are_ignored() {
    let result = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.columns(|row| row.col(|c| c.paragraph(|p| p.text("only one"))));
        doc.table(|t| {
            t.row(["a", "b"]);
            t.row(["c"]);
        });
        Ok(())
    });
    assert_eq!(
        result.err(),
        Some(Error::ColumnWidths {
            reason: "column-count"
        })
    );
}

#[test]
fn closure_errors_propagate_unchanged() {
    let result = document(PageSize::A4, |doc| {
        doc.spacer(NonNegative::pt(-1.0)?);
        Ok(())
    });
    assert_eq!(result.err(), Some(Error::NegativeLength { value: -1.0 }));
}

#[test]
fn text_without_a_font_fails() {
    let result = document(PageSize::A4, |doc| {
        doc.paragraph(|p| p.text("A"));
        Ok(())
    });
    assert_eq!(result.err(), Some(Error::UnknownFont { id: 0 }));
}

#[test]
fn mixed_body_and_bold_runs_keep_fonts_and_origins() {
    let page = &document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.bold_font(FONT.to_vec())?;
        doc.paragraph(|p| {
            p.text("A");
            p.bold("B");
            p.text("C");
        });
        Ok(())
    })
    .unwrap()
    .pages()[0];
    let recorded = runs(page);
    assert_eq!(
        recorded
            .iter()
            .map(|run| (run.text.as_str(), run.font))
            .collect::<Vec<_>>(),
        [("A", 0), ("B", 1), ("C", 0)]
    );
    assert!(pt_near(recorded[1].x, recorded[0].x + recorded[0].width));
    assert!(pt_near(recorded[2].x, recorded[1].x + recorded[1].width));
    assert!(recorded.iter().all(|run| pt_near(run.y, recorded[0].y)));
}

#[test]
fn first_line_indent_wraps_inside_the_inset_and_does_not_repeat_after_pagination() {
    let rendered = document(PageSize::custom(Size::try_pt(24.0, LINE).unwrap()), |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(12.0).unwrap());
            p.text("第第第");
        });
        Ok(())
    })
    .unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    let first = runs(&pages[0]);
    let second = runs(&pages[1]);
    assert_eq!(first[0].text, "第");
    assert!(pt_near(first[0].x, 12.0));
    assert_eq!(second[0].text, "第第");
    assert!(pt_near(second[0].x, 0.0));
}

#[test]
fn bold_and_indent_configuration_errors_are_specific() {
    let before_body = document(PageSize::A4, |doc| {
        doc.bold_font(FONT.to_vec())?;
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        before_body,
        Error::Font {
            index: 0,
            message: "body font is not registered".to_owned(),
        }
    );

    let missing_bold = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.bold("A"));
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(missing_bold, Error::UnknownFont { id: 1 });

    let invalid_indent = document(PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()), |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(24.0).unwrap());
            p.text("A");
        });
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(invalid_indent, Error::InvalidGeometry);

    let overwide = document(PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()), |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(12.0).unwrap());
            p.text("AA");
        });
        Ok(())
    })
    .err()
    .unwrap();
    assert!(
        matches!(
            overwide,
            Error::UnbreakableWiderThanFrame { avail, .. } if pt_near(avail, 12.0)
        ),
        "{overwide:?}"
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn mixed_bold_and_indent_write_a_pdf() {
    let pdf = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.bold_font(FONT.to_vec())?;
        doc.paragraph(|p| {
            p.first_line_indent(NonNegative::pt(24.0).unwrap());
            p.text("Contract ");
            p.bold("A");
        });
        Ok(())
    })
    .unwrap()
    .to_pdf()
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[cfg(feature = "backend-krilla")]
fn image_rects(page: &RecordedPage) -> Vec<(u32, u32, u32, Rect)> {
    page.ops()
        .iter()
        .filter_map(|op| match op {
            CanvasCommand::DrawImage { image, rect } => Some((
                image.id.as_u32(),
                image.pixel_width,
                image.pixel_height,
                *rect,
            )),
            _ => None,
        })
        .collect()
}

#[cfg(feature = "backend-krilla")]
fn rect_bits(rect: Rect) -> (u32, u32, u32, u32) {
    (
        rect.origin().x().as_pt().to_bits(),
        rect.origin().y().as_pt().to_bits(),
        rect.size().width().as_pt().to_bits(),
        rect.size().height().as_pt().to_bits(),
    )
}

#[cfg(not(feature = "backend-krilla"))]
#[test]
fn load_image_without_decoder_is_an_image_error() {
    let err = document(PageSize::A4, |doc| {
        let _ = doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?;
        Ok(())
    })
    .err()
    .unwrap();
    assert!(matches!(err, Error::Image { .. }));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn image_only_document_needs_no_font() {
    let page = Size::try_pt(200.0, 400.0).unwrap();
    let rendered = document(PageSize::custom(page), |doc| {
        let logo = doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?;
        assert_eq!(logo.pixel_width(), 1);
        assert_eq!(logo.pixel_height(), 1);
        let width = Positive::pt(40.0)?;
        doc.image(&logo, |image| {
            image.width(width);
            image.center();
        });
        Ok(())
    })
    .unwrap();
    let images = image_rects(&rendered.pages()[0]);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].0, 0);
    assert_eq!(images[0].1, 1);
    assert_eq!(images[0].2, 1);
    let expected = Rect::try_pt(80.0, 0.0, 40.0, 40.0).unwrap();
    assert_eq!(rect_bits(images[0].3), rect_bits(expected));
    assert!(rendered.to_pdf().unwrap().starts_with(b"%PDF"));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn cell_image_and_last_page_seal_share_coordinates() {
    let page = Size::try_pt(200.0, 80.0).unwrap();
    let rendered = document(PageSize::custom(page), |doc| {
        doc.font(FONT.to_vec())?;
        let seal = doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?;
        let mark = seal;
        doc.table(|table| {
            let side = Positive::pt(10.0).unwrap();
            table.row([
                Cell::new("A"),
                Cell::image(&seal, |image| image.width(side)),
            ]);
        });
        doc.page_break();
        doc.paragraph(|p| p.text("B"));
        doc.front(move |layer| {
            if layer.is_last() {
                layer.image_at(&mark, Rect::try_pt(150.0, 40.0, 20.0, 20.0).unwrap());
            }
        });
        Ok(())
    })
    .unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    let cell = image_rects(&pages[0]);
    assert_eq!(cell.len(), 1);
    let expected = Rect::try_pt(100.0, 0.0, 10.0, 10.0).unwrap();
    assert_eq!(rect_bits(cell[0].3), rect_bits(expected));
    let seal = image_rects(&pages[1]);
    assert_eq!(seal.len(), 1);
    let placed = Rect::try_pt(150.0, 40.0, 20.0, 20.0).unwrap();
    assert_eq!(rect_bits(seal[0].3), rect_bits(placed));
    assert_eq!(seal[0].0, cell[0].0);
}

#[cfg(feature = "backend-krilla")]
#[test]
fn three_pages_of_one_image_share_one_xobject() {
    let page = Size::try_pt(120.0, 40.0).unwrap();
    let pdf = document(PageSize::custom(page), |doc| {
        let seal = doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?;
        doc.image(&seal, |_| {});
        doc.page_break();
        doc.image(&seal, |_| {});
        doc.page_break();
        doc.image(&seal, |_| {});
        Ok(())
    })
    .unwrap()
    .to_pdf()
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let text = String::from_utf8_lossy(&pdf);
    let count = text.matches("/Subtype /Image").count() + text.matches("/Subtype/Image").count();
    assert_eq!(count, 1, "{text}");
}

#[cfg(feature = "backend-krilla")]
#[test]
fn garbage_empty_and_foreign_handles_are_image_errors() {
    let garbage = document(PageSize::A4, |doc| {
        let _ = doc.load_image(b"not a png".to_vec(), ImageFormat::Png)?;
        Ok(())
    })
    .err()
    .unwrap();
    assert!(matches!(garbage, Error::Image { .. }));

    let empty = document(PageSize::A4, |doc| {
        let _ = doc.load_image(Vec::new(), ImageFormat::Png)?;
        Ok(())
    })
    .err()
    .unwrap();
    assert!(matches!(empty, Error::Image { message } if message == "bytes are empty"));

    let mut stolen = None;
    let _ = document(PageSize::A4, |doc| {
        stolen = Some(doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?);
        Ok(())
    })
    .unwrap();
    let foreign = document(PageSize::A4, |doc| {
        doc.image(stolen.as_ref().unwrap(), |_| {});
        Ok(())
    })
    .err()
    .unwrap();
    assert!(matches!(foreign, Error::UnknownImage { id: 0 }));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn list_item_and_overwide_image_fail() {
    let listed = document(PageSize::A4, |doc| {
        let logo = doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?;
        doc.ul(|list| list.item(|item| item.image(&logo, |_| {})));
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        listed,
        Error::InvalidCellContent {
            container: "list item",
            node: "image"
        }
    );

    let wide = document(
        PageSize::custom(Size::try_pt(100.0, 100.0).unwrap()),
        |doc| {
            let logo = doc.load_image(ONE_PIXEL.to_vec(), ImageFormat::Png)?;
            let width = Positive::pt(101.0)?;
            doc.image(&logo, |image| image.width(width));
            Ok(())
        },
    )
    .err()
    .unwrap();
    assert_eq!(
        wide,
        Error::UnbreakableWiderThanFrame {
            width: 101.0,
            avail: 100.0,
        }
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn damaged_pixels_fail_at_pdf_time() {
    let rendered = document(PageSize::A4, |doc| {
        let logo = doc.load_image(BAD_IDAT.to_vec(), ImageFormat::Png)?;
        doc.image(&logo, |_| {});
        Ok(())
    })
    .unwrap();
    let err = rendered.to_pdf().unwrap_err();
    assert!(matches!(err, Error::Backend(_)), "{err:?}");
}

#[cfg(feature = "backend-krilla")]
#[test]
fn to_pdf_writes_a_pdf() {
    let pdf = full_template(&contract(Kind::A), "ACME")
        .unwrap()
        .to_pdf()
        .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

fn section_footer(band: &mut quire_pdf::Band) {
    let page = band.page();
    band.left(format!("{}/{}", page.number, page.count));
    band.right(format!("{}/{}", page.section_number, page.section_count));
}

#[test]
fn two_sections_number_document_pages_and_section_pages() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        let margin = NonNegative::pt(20.0)?;
        doc.section(|section| {
            section.margins(Margins::uniform(margin));
            section.footer(section_footer);
            section.paragraph(|p| p.text("A"));
            section.page_break();
            section.paragraph(|p| p.text("B"));
            Ok(())
        })?;
        doc.section(|section| {
            section.margins(Margins::uniform(margin));
            section.footer(section_footer);
            section.paragraph(|p| p.text("C"));
            Ok(())
        })?;
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 3);
    assert_eq!(texts(&pages[0]), ["A", "1/3", "1/2"]);
    assert_eq!(texts(&pages[1]), ["B", "2/3", "2/2"]);
    assert_eq!(texts(&pages[2]), ["C", "3/3", "1/1"]);
}

#[test]
fn second_section_header_insets_by_that_sections_left_margin() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.section(|section| {
            section.margins(Margins::uniform(NonNegative::pt(20.0)?));
            section.header(|header| header.left("One"));
            section.paragraph(|p| p.text("A"));
            Ok(())
        })?;
        doc.section(|section| {
            section.margins(Margins::new(
                NonNegative::pt(20.0)?,
                NonNegative::pt(10.0)?,
                NonNegative::pt(20.0)?,
                NonNegative::pt(48.0)?,
            ));
            section.header(|header| header.left("Two"));
            section.paragraph(|p| p.text("B"));
            Ok(())
        })?;
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert!(pt_near(runs(&pages[0])[0].x, 20.0));
    assert_eq!(runs(&pages[0])[0].text, "One");
    assert!(pt_near(runs(&pages[1])[0].x, 48.0));
    assert_eq!(runs(&pages[1])[0].text, "Two");
}

#[test]
fn paragraph_then_section_is_content_outside_section() {
    let err = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text("A"));
        doc.section(|section| {
            section.paragraph(|p| p.text("B"));
            Ok(())
        })?;
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(err, Error::ContentOutsideSection);
}

#[test]
fn empty_section_is_one_page_and_still_draws_its_footer() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.section(|section| {
            section.margins(Margins::uniform(NonNegative::pt(20.0)?));
            section.footer(|footer| footer.left("foot"));
            Ok(())
        })?;
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(texts(&pages[0]), ["foot"]);
}

#[test]
fn trailing_page_break_inside_a_section_does_not_add_a_page() {
    let pages = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.section(|section| {
            section.paragraph(|p| p.text("A"));
            section.page_break();
            Ok(())
        })?;
        Ok(())
    })
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(texts(&pages[0]), ["A"]);
}

#[cfg(feature = "backend-krilla")]
#[test]
fn two_sections_on_different_paper_write_a_pdf() {
    let rendered = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.section(|section| {
            section.paragraph(|p| p.text("A"));
            Ok(())
        })?;
        doc.section(|section| {
            section.page_size(Paper::Letter.landscape());
            section.paragraph(|p| p.text("B"));
            Ok(())
        })?;
        Ok(())
    })
    .unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].size(), A4);
    assert_eq!(pages[1].size(), Paper::Letter.landscape().resolve());
    assert!(rendered.to_pdf().unwrap().starts_with(b"%PDF"));
}

/// `(cx, cy, rx, ry)` from a four-cubic ellipse's endpoints.
fn ellipse_center_radii(path: &Path) -> Option<(f32, f32, f32, f32)> {
    let [PathVerb::MoveTo(right), PathVerb::CubicTo { to: bottom, .. }, PathVerb::CubicTo { to: left, .. }, PathVerb::CubicTo { to: top, .. }, PathVerb::CubicTo { .. }, PathVerb::Close] =
        path.verbs()
    else {
        return None;
    };
    Some((
        f32::midpoint(right.x().as_pt(), left.x().as_pt()),
        f32::midpoint(bottom.y().as_pt(), top.y().as_pt()),
        (right.x().as_pt() - left.x().as_pt()) * 0.5,
        (bottom.y().as_pt() - top.y().as_pt()) * 0.5,
    ))
}

fn painted_layer_paths() -> Rendered {
    document(
        PageSize::custom(Size::try_pt(200.0, 160.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.margins(Margins::uniform(NonNegative::pt(20.0)?));
            doc.header(|header| header.left("H"));
            doc.paragraph(|p| p.text("A"));
            doc.behind(|layer| {
                layer.line(
                    Point::try_pt(0.0, 0.0).unwrap(),
                    Point::try_pt(40.0, 40.0).unwrap(),
                    Pen::new(Color::rgba(200, 0, 0, 64), Positive::pt(0.8).unwrap()),
                );
                layer.rect(
                    Rect::try_pt(4.0, 6.0, 10.0, 12.0).unwrap(),
                    Ink::Stroke(Pen::new(Color::rgb(0, 0, 0), Positive::pt(0.5).unwrap())),
                );
            });
            doc.front(|layer| {
                layer.text_at("SEAL", Point::try_pt(12.0, 30.0).unwrap());
                layer.ellipse(
                    Rect::try_pt(40.0, 50.0, 20.0, 10.0).unwrap(),
                    Ink::FillStroke(
                        Color::rgba(200, 0, 0, 40),
                        Pen::new(Color::rgb(200, 0, 0), Positive::pt(2.0).unwrap()),
                    ),
                );
                layer.rotated(90.0, Point::try_pt(10.0, 12.0).unwrap(), |shapes| {
                    shapes.rect(
                        Rect::try_pt(10.0, 12.0, 20.0, 8.0).unwrap(),
                        Ink::Fill(Color::rgba(0, 0, 255, 128)),
                    );
                });
                layer.rotated(0.0, Point::ZERO, |shapes| {
                    shapes.line(
                        Point::try_pt(1.0, 2.0).unwrap(),
                        Point::try_pt(8.0, 9.0).unwrap(),
                        Pen::new(Color::rgb(0, 0, 0), Positive::pt(1.0).unwrap()),
                    );
                });
            });
            Ok(())
        },
    )
    .unwrap()
}

#[test]
fn layer_paths_keep_stack_order_and_bake_rotation() {
    let rendered = painted_layer_paths();
    let pages = rendered.pages();
    let page = &pages[0];
    let ops = page.ops();
    assert!(matches!(ops[0], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[1], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[2], CanvasCommand::DrawGlyphRun { .. }));
    assert!(matches!(ops[3], CanvasCommand::DrawGlyphRun { .. }));
    assert!(matches!(ops[4], CanvasCommand::DrawGlyphRun { .. }));
    assert!(matches!(ops[5], CanvasCommand::FillPath { .. }));
    assert!(matches!(ops[6], CanvasCommand::StrokePath { .. }));
    assert!(matches!(ops[7], CanvasCommand::FillPath { .. }));
    assert!(matches!(ops[8], CanvasCommand::StrokePath { .. }));
    assert_eq!(texts(page), ["H", "A", "SEAL"]);
    assert_behind_paths(ops);
    assert_front_paths(page, ops);
}

fn assert_behind_paths(ops: &[CanvasCommand]) {
    let CanvasCommand::StrokePath {
        path,
        paint,
        stroke,
    } = &ops[0]
    else {
        unreachable!();
    };
    assert_eq!(paint.color(), Color::rgba(200, 0, 0, 64));
    assert_eq!(stroke.width().as_pt().to_bits(), 0.8f32.to_bits());
    assert_eq!(
        path.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(0.0, 0.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(40.0, 40.0).unwrap()),
        ]
    );
    let CanvasCommand::StrokePath {
        path: frame,
        paint: frame_paint,
        stroke: frame_stroke,
    } = &ops[1]
    else {
        unreachable!();
    };
    assert_eq!(frame_paint.color(), Color::rgb(0, 0, 0));
    assert_eq!(frame_stroke.width().as_pt().to_bits(), 0.5f32.to_bits());
    assert_eq!(
        frame.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(4.0, 6.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(14.0, 6.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(14.0, 18.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(4.0, 18.0).unwrap()),
            PathVerb::Close,
        ]
    );
}

fn assert_front_paths(page: &RecordedPage, ops: &[CanvasCommand]) {
    let CanvasCommand::FillPath {
        path: filled,
        paint: fill_paint,
    } = &ops[5]
    else {
        unreachable!();
    };
    let CanvasCommand::StrokePath {
        path: stroked,
        paint: stroke_paint,
        stroke: ellipse_stroke,
    } = &ops[6]
    else {
        unreachable!();
    };
    assert_eq!(filled, stroked);
    assert_eq!(fill_paint.color(), Color::rgba(200, 0, 0, 40));
    assert_eq!(stroke_paint.color(), Color::rgb(200, 0, 0));
    assert_eq!(ellipse_stroke.width().as_pt().to_bits(), 2.0f32.to_bits());
    let (cx, cy, rx, ry) = ellipse_center_radii(filled).unwrap();
    assert_eq!(cx.to_bits(), 50.0f32.to_bits());
    assert_eq!(cy.to_bits(), 55.0f32.to_bits());
    assert_eq!(rx.to_bits(), 10.0f32.to_bits());
    assert_eq!(ry.to_bits(), 5.0f32.to_bits());

    let CanvasCommand::FillPath {
        path: turned,
        paint: turned_paint,
    } = &ops[7]
    else {
        unreachable!();
    };
    assert_eq!(turned_paint.color(), Color::rgba(0, 0, 255, 128));
    let points: Vec<(f32, f32)> = turned
        .verbs()
        .iter()
        .filter_map(|verb| match verb {
            PathVerb::MoveTo(point) | PathVerb::LineTo(point) => {
                Some((point.x().as_pt(), point.y().as_pt()))
            }
            PathVerb::Close => None,
            PathVerb::CubicTo { .. } => panic!("rotated rect stays lines"),
        })
        .collect();
    for (got, expected) in points
        .iter()
        .zip([(10.0, 12.0), (10.0, 32.0), (2.0, 32.0), (2.0, 12.0)])
    {
        assert!(pt_near(got.0, expected.0));
        assert!(pt_near(got.1, expected.1));
    }

    let CanvasCommand::StrokePath { path: flat, .. } = &ops[8] else {
        unreachable!();
    };
    assert_eq!(
        flat.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(1.0, 2.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(8.0, 9.0).unwrap()),
        ]
    );
    let seal = runs(page)
        .into_iter()
        .find(|run| run.text == "SEAL")
        .unwrap();
    assert!(pt_near(seal.x, 12.0));
    assert!(pt_near(seal.y, 30.0 + BASELINE));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn layer_paths_write_a_pdf() {
    let pdf = document(PageSize::custom(Size::try_pt(80.0, 80.0).unwrap()), |doc| {
        doc.behind(|layer| {
            layer.rect(
                Rect::try_pt(10.0, 10.0, 20.0, 20.0).unwrap(),
                Ink::Fill(Color::rgba(0, 0, 0, 128)),
            );
            layer.ellipse(
                Rect::try_pt(30.0, 30.0, 16.0, 12.0).unwrap(),
                Ink::Stroke(Pen::new(Color::rgb(0, 0, 0), Positive::pt(1.0).unwrap())),
            );
        });
        Ok(())
    })
    .unwrap()
    .to_pdf()
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn hline_moves_whole_and_pins_stroke() {
    let thick = Positive::pt(2.0).unwrap();
    let gap = NonNegative::pt(4.0).unwrap();
    let pages = document(
        PageSize::custom(Size::try_pt(100.0, 40.0).unwrap()),
        |doc| {
            doc.spacer(NonNegative::pt(31.0)?);
            doc.hline(|h| {
                h.thickness(thick);
                h.space_before(gap);
                h.space_after(gap);
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert!(pages[0]
        .ops()
        .iter()
        .all(|op| !matches!(op, CanvasCommand::StrokePath { .. })));
    let CanvasCommand::StrokePath {
        path,
        paint,
        stroke,
    } = &pages[1].ops()[0]
    else {
        panic!("expected stroke on page 2");
    };
    assert_eq!(paint.color(), Color::rgb(0, 0, 0));
    assert_eq!(*stroke, Stroke::new(thick));
    assert_eq!(
        path.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(0.0, 5.0).unwrap()),
            PathVerb::LineTo(Point::try_pt(100.0, 5.0).unwrap()),
        ]
    );
}

#[test]
fn hline_taller_than_body_fails_before_any_page() {
    let gap = NonNegative::pt(12.0).unwrap();
    let result = document(
        PageSize::custom(Size::try_pt(100.0, 20.0).unwrap()),
        |doc| {
            doc.hline(|h| {
                h.space_before(gap);
                h.space_after(gap);
            });
            Ok(())
        },
    );
    assert_eq!(result.err(), Some(Error::UnsplittableTallerThanFrame));
}

#[test]
fn hline_in_list_item_and_cell() {
    let width = Positive::pt(40.0).unwrap();
    let page = &document(
        PageSize::custom(Size::try_pt(200.0, 200.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.ul(|list| {
                list.item(|item| {
                    item.hline(|h| {
                        h.width(width);
                        h.start();
                    });
                });
            });
            doc.table(|t| {
                t.row([Cell::hline(|h| h.color(Color::rgb(9, 9, 9)))]);
            });
            Ok(())
        },
    )
    .unwrap()
    .pages()[0];
    let strokes: Vec<_> = page
        .ops()
        .iter()
        .filter(|op| matches!(op, CanvasCommand::StrokePath { .. }))
        .collect();
    assert!(strokes.len() >= 2);
    let CanvasCommand::StrokePath { path, .. } = strokes[0] else {
        panic!("list hline");
    };
    // Marker column is 18 pt; body starts at x=18. Start-aligned short line.
    assert_eq!(
        path.verbs(),
        &[
            PathVerb::MoveTo(Point::try_pt(18.0, 0.5).unwrap()),
            PathVerb::LineTo(Point::try_pt(58.0, 0.5).unwrap()),
        ]
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn hline_writes_a_pdf() {
    let pdf = document(PageSize::A4, |doc| {
        doc.hline(|_| {});
        Ok(())
    })
    .unwrap()
    .to_pdf()
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

struct StrokeSample {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    width: f32,
    color: Color,
}

fn stroke_samples(page: &RecordedPage) -> Vec<StrokeSample> {
    page.ops()
        .iter()
        .filter_map(|op| {
            let CanvasCommand::StrokePath {
                path,
                paint,
                stroke,
            } = op
            else {
                return None;
            };
            let [PathVerb::MoveTo(from), PathVerb::LineTo(to)] = path.verbs() else {
                return None;
            };
            Some(StrokeSample {
                x0: from.x().as_pt(),
                y0: from.y().as_pt(),
                x1: to.x().as_pt(),
                y1: to.y().as_pt(),
                width: stroke.width().as_pt(),
                color: paint.color(),
            })
        })
        .collect()
}

fn glyph(page: &RecordedPage, text: &str) -> GlyphSample {
    runs(page)
        .into_iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("missing glyph run {text}"))
}

#[test]
fn middle_blank_shifts_the_next_glyph_and_strokes_the_gap() {
    let gap = NonNegative::pt(30.0).unwrap();
    let thick = Positive::pt(2.0).unwrap();
    let offset = NonNegative::pt(4.0).unwrap();
    let pages = document(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.paragraph(|p| {
                p.text("AB");
                p.blank(gap).thickness(thick).baseline_offset(offset);
                p.text("C");
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    let ab = glyph(&pages[0], "AB");
    let c = glyph(&pages[0], "C");
    assert!(pt_near(c.x, ab.x + ab.width + 30.0), "c.x {}", c.x);
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, ab.x + ab.width));
    assert!(pt_near(stroke.x1, c.x));
    assert!(pt_near(stroke.y0, ab.y + 4.0));
    assert!(pt_near(stroke.y1, stroke.y0));
    assert!(pt_near(stroke.width, 2.0));
    assert_eq!(stroke.color, Color::rgb(0, 0, 0));
}

#[test]
fn blank_does_not_change_line_height() {
    let y_of_b = |with_blank: bool| {
        let gap = NonNegative::pt(20.0).unwrap();
        let pages = document(PageSize::A4, |doc| {
            doc.font(FONT.to_vec())?;
            doc.paragraph(|p| {
                if with_blank {
                    p.text("A");
                    p.blank(gap);
                    p.text("\nB");
                } else {
                    p.text("A\nB");
                }
            });
            Ok(())
        })
        .unwrap()
        .pages();
        glyph(&pages[0], "B").y
    };
    assert!(pt_near(y_of_b(true), y_of_b(false)));
}

#[test]
fn fill_blank_reaches_the_frame_edge_with_the_default_stroke() {
    let pages = document(
        PageSize::custom(Size::try_pt(120.0, 400.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.paragraph(|p| {
                p.text("A");
                p.blank_to_end();
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    let a = glyph(&pages[0], "A");
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, a.x + a.width));
    assert!(pt_near(stroke.x1, 120.0));
    assert!(pt_near(stroke.y0, a.y + 1.0));
    assert!(pt_near(stroke.width, 1.0));
    assert_eq!(stroke.color, Color::rgb(0, 0, 0));
}

#[test]
fn blank_em_is_font_size_times_count_and_needs_no_bold_face() {
    let pages = document(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.paragraph(|p| {
                p.blank_em(2);
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(
        pt_near(stroke.x1 - stroke.x0, 24.0),
        "{}",
        stroke.x1 - stroke.x0
    );
    assert!(runs(&pages[0]).iter().any(|run| run.text.is_empty()));
}

#[test]
fn zero_width_blank_draws_no_stroke_and_does_not_shift() {
    let pages = document(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.paragraph(|p| {
                p.text("A");
                p.blank(NonNegative::ZERO);
                p.blank_em(0);
                p.text("B");
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    assert!(stroke_samples(&pages[0]).is_empty());
    let a = glyph(&pages[0], "A");
    let b = glyph(&pages[0], "B");
    assert!(pt_near(b.x, a.x + a.width));
}

#[test]
fn list_fill_blank_starts_after_the_marker_column() {
    let pages = document(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.ul(|list| {
                list.item(|item| {
                    item.paragraph(|p| {
                        p.text("A");
                        p.blank_to_end();
                    });
                });
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    let a = glyph(&pages[0], "A");
    assert!(pt_near(a.x, 18.0), "{}", a.x);
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, a.x + a.width));
    assert!(pt_near(stroke.x1, 200.0));
}

#[test]
fn column_fill_blank_is_shifted_with_its_column() {
    let pages = document(
        PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.columns(|row| {
                row.col(|col| col.paragraph(|p| p.text("L")));
                row.col(|col| {
                    col.paragraph(|p| {
                        p.text("A");
                        p.blank_to_end();
                    });
                });
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    let a = glyph(&pages[0], "A");
    let col = (200.0 - 12.0) / 2.0;
    assert!(pt_near(a.x, col + 12.0), "{}", a.x);
    let stroke = &stroke_samples(&pages[0])[0];
    assert!(pt_near(stroke.x0, a.x + a.width));
    assert!(pt_near(stroke.x1, 200.0), "{}", stroke.x1);
}

#[test]
fn blank_stays_on_its_line_when_the_paragraph_splits() {
    let pages = document(
        PageSize::custom(Size::try_pt(200.0, LINE).unwrap()),
        |doc| {
            doc.font(FONT.to_vec())?;
            doc.paragraph(|p| {
                p.text("A");
                p.blank(NonNegative::pt(20.0).unwrap());
                p.text("\nB");
            });
            Ok(())
        },
    )
    .unwrap()
    .pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(stroke_samples(&pages[0]).len(), 1);
    assert!(stroke_samples(&pages[1]).is_empty());
    let b = glyph(&pages[1], "B");
    assert!(pt_near(b.x, 0.0));
    assert!(pt_near(b.y, BASELINE));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn inline_blank_writes_a_pdf() {
    let pdf = document(PageSize::A4, |doc| {
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| {
            p.text("A");
            p.blank(NonNegative::pt(12.0).unwrap());
        });
        Ok(())
    })
    .unwrap()
    .to_pdf()
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}
