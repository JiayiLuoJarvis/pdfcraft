//! Public `document` DSL against recorded canvas commands.

use std::num::NonZeroU16;

use quire_pdf::{
    document, pt_near, CanvasCommand, Cell, Error, Flow, Margins, NonNegative, Numbering, Point,
    RecordedPage, Rendered, Result, Size, A4,
};

const FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf");
/// Fixture ascent at 12 pt: the baseline below the line-box top.
const BASELINE: f32 = 13.812;
/// Fixture ascent minus descent at 12 pt, which beats the 1.2 factor.
const LINE: f32 = 17.244;

struct GlyphSample {
    text: String,
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
    document(A4, |doc| {
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
    document(A4, |doc| {
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
    let pages = document(A4, |doc| {
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
    let pages = document(A4, |doc| {
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
    let result = document(A4, |doc| {
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
    let pages = document(A4, |doc| {
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
        document(A4, |doc| {
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
    let pages = document(A4, |doc| {
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
        let pages = document(page, |doc| {
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
    let result = document(A4, |doc| {
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
    let result = document(A4, |doc| {
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
    let result = document(A4, |doc| {
        doc.spacer(NonNegative::pt(-1.0)?);
        Ok(())
    });
    assert_eq!(result.err(), Some(Error::NegativeLength { value: -1.0 }));
}

#[test]
fn text_without_a_font_fails() {
    let result = document(A4, |doc| {
        doc.paragraph(|p| p.text("A"));
        Ok(())
    });
    assert_eq!(result.err(), Some(Error::UnknownFont { id: 0 }));
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
