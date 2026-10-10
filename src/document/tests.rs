#![allow(clippy::wildcard_imports)]
use super::*;
use crate::fonts::fixture_bytes;
use crate::geom::{PageSize, Rect};

fn doc_with_font<'a>() -> Document<'a> {
    let mut doc = Document::new(PageSize::A4);
    doc.font(fixture_bytes().to_vec()).unwrap();

    doc
}

#[test]
fn paragraph_enters_the_flow_unwrapped() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| p.text("A B"));
    let Sections::Implicit(section) = &doc.body else {
        panic!("paragraph stays on the implicit section");
    };
    assert!(matches!(
        section.flow.nodes.as_slice(),
        [FlowNode::Text(text)] if *text == TextNode::new("A B", default_style())
    ));
}

#[test]
fn paragraph_coalesces_adjacent_faces_and_ignores_empty_appends() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| {
        p.text("A");
        p.text("");
        p.text("B");
        p.bold("C");
        p.bold("D");
        p.text("E");
    });
    let Sections::Implicit(section) = &doc.body else {
        panic!("paragraph stays on the implicit section");
    };
    let expected = TextNode::from_runs(
        vec![
            TextRun::new("AB", Face::Body),
            TextRun::new("CD", Face::Bold),
            TextRun::new("E", Face::Body),
        ],
        default_style(),
    );
    assert!(matches!(
        section.flow.nodes.as_slice(),
        [FlowNode::Text(text)] if *text == expected
    ));
}

fn flow_text(doc: &Document<'_>) -> TextNode {
    let Sections::Implicit(section) = &doc.body else {
        panic!("paragraph stays on the implicit section");
    };
    match section.flow.nodes.as_slice() {
        [FlowNode::Text(text)] => text.clone(),
        other => panic!("expected one text node, got {other:?}"),
    }
}

#[test]
fn blank_breaks_text_coalescing() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| {
        p.text("A");
        p.blank(NonNegative::pt(10.0).unwrap())
            .thickness(Positive::pt(0.5).unwrap())
            .color(Color::rgb(1, 2, 3))
            .baseline_offset(NonNegative::pt(2.0).unwrap());
        p.text("B");
    });
    let mut blank = InlineBlank::new(BlankWidth::Fixed(NonNegative::pt(10.0).unwrap()));
    blank.thickness = Positive::pt(0.5).unwrap();
    blank.color = Some(Color::rgb(1, 2, 3));
    blank.baseline_offset = NonNegative::pt(2.0).unwrap();
    let expected = TextNode::from_items(
        vec![
            InlineItem::Text(TextRun::new("A", Face::Body)),
            InlineItem::Blank(blank),
            InlineItem::Text(TextRun::new("B", Face::Body)),
        ],
        default_style(),
    );
    assert_eq!(flow_text(&doc), expected);
}

#[test]
fn fill_blank_coalesces_text_before_it_and_rejects_a_later_run() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| {
        p.text("A");
        p.text("B");
        p.blank_to_end();
        p.text("");
        p.bold("");
    });
    let expected = TextNode::from_items(
        vec![
            InlineItem::Text(TextRun::new("AB", Face::Body)),
            InlineItem::Blank(InlineBlank::new(BlankWidth::ToEndOfLine)),
        ],
        default_style(),
    );
    assert_eq!(flow_text(&doc), expected);

    for build in [
        |p: &mut Paragraph| {
            p.blank_to_end();
            p.text("x");
        },
        |p: &mut Paragraph| {
            p.blank_to_end();
            p.bold("x");
        },
        |p: &mut Paragraph| {
            p.blank_to_end();
            p.blank(NonNegative::ZERO);
        },
        |p: &mut Paragraph| {
            p.blank_to_end();
            p.blank_em(1);
        },
        |p: &mut Paragraph| {
            p.blank_to_end();
            p.blank_to_end();
        },
    ] {
        let mut doc = doc_with_font();
        doc.paragraph(build);
        let Sections::Implicit(section) = &doc.body else {
            panic!("paragraph stays on the implicit section");
        };
        assert_eq!(section.flow.error, Some(Error::RunAfterFillBlank));
        assert!(section.flow.nodes.is_empty());
    }
}

#[test]
fn run_after_fill_blank_paints_nothing() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| {
        p.text("A");
        p.blank_to_end();
        p.text("x");
    });
    assert_fails_with_empty_canvas(doc, &Error::RunAfterFillBlank);
}

#[test]
fn fixed_blank_wider_than_the_indented_line_fails_before_any_page() {
    let mut doc = Document::new(PageSize::custom(Size::try_pt(100.0, 200.0).unwrap()));
    doc.font(fixture_bytes().to_vec()).unwrap();
    doc.paragraph(|p| {
        p.first_line_indent(NonNegative::pt(40.0).unwrap());
        p.blank(NonNegative::pt(80.0).unwrap());
    });
    let mut rec = RecordingCanvas::new();
    let Err(error) = doc.paint(&mut rec) else {
        panic!("expected the blank to be wider than the indented line");
    };
    let Error::UnbreakableWiderThanFrame { width, avail } = error else {
        panic!("{error:?}");
    };
    assert!(crate::canvas::pt_near(width, 80.0), "{width}");
    assert!(crate::canvas::pt_near(avail, 60.0), "{avail}");
    assert!(rec.commands().is_empty());
}

#[test]
fn bold_font_requires_body_and_rebinds_to_the_latest_face() {
    let mut doc = Document::new(PageSize::A4);
    assert_eq!(
        doc.bold_font(fixture_bytes().to_vec()).unwrap_err(),
        Error::Font {
            index: 0,
            message: "body font is not registered".to_owned(),
        }
    );
    doc.font(fixture_bytes().to_vec()).unwrap();
    doc.bold_font(fixture_bytes().to_vec()).unwrap();
    assert_eq!(doc.fonts.bold().unwrap(), FontId::from_raw(1));
    doc.bold_font(fixture_bytes().to_vec()).unwrap();
    assert_eq!(doc.fonts.bold().unwrap(), FontId::from_raw(2));
}

fn assert_fails_with_empty_canvas(doc: Document<'_>, expected: &Error) {
    let mut rec = RecordingCanvas::new();
    let result = doc.paint(&mut rec);
    assert_eq!(result.err().as_ref(), Some(expected));
    assert!(rec.commands().is_empty());
}

#[test]
fn link_coalesces_on_face_and_uri_and_restores_the_outer_target() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| {
        p.link("https://a", |p| {
            p.text("A");
            p.text("B");
            p.bold("C");
            p.link("https://b", |p| p.text("D"));
            p.text("E");
        });
        p.text("F");
    });
    let a = Some(LinkTarget::uri("https://a").unwrap());
    let b = Some(LinkTarget::uri("https://b").unwrap());
    let expected = TextNode::from_runs(
        vec![
            TextRun::new("AB", Face::Body).with_link(a.clone()),
            TextRun::new("C", Face::Bold).with_link(a),
            TextRun::new("D", Face::Body).with_link(b),
            TextRun::new("E", Face::Body).with_link(Some(LinkTarget::uri("https://a").unwrap())),
            TextRun::new("F", Face::Body),
        ],
        default_style(),
    );
    assert_eq!(flow_text(&doc), expected);
}

#[test]
fn blank_inside_a_link_stays_unlinked() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| {
        p.link("u", |p| {
            p.text("A");
            p.blank(NonNegative::pt(10.0).unwrap());
            p.text("B");
        });
    });
    let link = Some(LinkTarget::uri("u").unwrap());
    let expected = TextNode::from_items(
        vec![
            InlineItem::Text(TextRun::new("A", Face::Body).with_link(link.clone())),
            InlineItem::Blank(InlineBlank::new(BlankWidth::Fixed(
                NonNegative::pt(10.0).unwrap(),
            ))),
            InlineItem::Text(TextRun::new("B", Face::Body).with_link(link)),
        ],
        default_style(),
    );
    assert_eq!(flow_text(&doc), expected);
}

#[test]
fn empty_uri_does_not_run_the_link_closure() {
    let mut doc = doc_with_font();
    let mut ran = false;
    doc.paragraph(|p| {
        p.text("A");
        p.link("", |_| ran = true);
    });
    assert!(!ran);
    assert_fails_with_empty_canvas(doc, &Error::EmptyLink);
}

#[test]
fn empty_band_uri_paints_nothing() {
    let mut doc = doc_with_font();
    doc.margins(Margins::uniform(NonNegative::pt(36.0).unwrap()));
    doc.header(|h| {
        h.left_link("", "Home");
        h.right_link("https://example.com", "Later");
    });
    doc.paragraph(|p| p.text("A"));
    assert_fails_with_empty_canvas(doc, &Error::EmptyLink);
}

const MISSING: char = '\u{6f22}';

#[test]
fn bold_and_indent_failures_happen_before_any_page() {
    let mut missing_bold = doc_with_font();
    missing_bold.paragraph(|p| p.bold("A"));
    assert_fails_with_empty_canvas(missing_bold, &Error::UnknownFont { id: 1 });

    let mut missing_glyph = doc_with_font();
    missing_glyph.bold_font(fixture_bytes().to_vec()).unwrap();
    missing_glyph.paragraph(|p| p.bold(MISSING.to_string()));
    assert_fails_with_empty_canvas(
        missing_glyph,
        &Error::MissingGlyph {
            font: FontId::from_raw(1),
            scalar: MISSING,
        },
    );

    let mut invalid_indent = Document::new(PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()));
    invalid_indent.font(fixture_bytes().to_vec()).unwrap();
    invalid_indent.paragraph(|p| {
        p.first_line_indent(NonNegative::pt(24.0).unwrap());
        p.text("A");
    });
    assert_fails_with_empty_canvas(invalid_indent, &Error::InvalidGeometry);

    let mut overwide = Document::new(PageSize::custom(Size::try_pt(24.0, 40.0).unwrap()));
    overwide.font(fixture_bytes().to_vec()).unwrap();
    overwide.paragraph(|p| {
        p.first_line_indent(NonNegative::pt(12.0).unwrap());
        p.text("AA");
    });
    let mut rec = RecordingCanvas::new();
    let error = overwide.paint(&mut rec).err().unwrap();
    assert!(
        matches!(
            error,
            Error::UnbreakableWiderThanFrame { avail, .. }
                if crate::canvas::pt_near(avail, 12.0)
        ),
        "{error:?}"
    );
    assert!(rec.commands().is_empty());
}

#[test]
fn missing_glyph_in_header_fails_before_any_page() {
    let mut doc = doc_with_font();
    doc.margins(Margins::uniform(NonNegative::pt(36.0).unwrap()));
    doc.header(|h| h.left(MISSING.to_string()));
    doc.paragraph(|p| p.text("A"));
    assert_fails_with_empty_canvas(
        doc,
        &Error::MissingGlyph {
            font: FontId::from_raw(0),
            scalar: MISSING,
        },
    );
}

#[test]
fn degenerate_layer_line_fails_before_any_page() {
    let mut doc = Document::new(PageSize::custom(Size::try_pt(200.0, 100.0).unwrap()));
    let point = Point::try_pt(4.0, 5.0).unwrap();
    let pen = Pen::new(Color::rgb(0, 0, 0), 1.0).unwrap();
    doc.behind(|layer| {
        layer.line(point, point, pen);
        layer.rect(
            Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap(),
            Ink::Fill(Color::rgb(0, 0, 0)),
        );
    });
    assert_fails_with_empty_canvas(doc, &Error::InvalidGeometry);
}

#[test]
fn non_finite_rotation_does_not_draw() {
    use std::cell::Cell;

    let ran = Cell::new(false);
    let mut doc = Document::new(PageSize::custom(Size::try_pt(200.0, 100.0).unwrap()));
    doc.behind(|layer| {
        layer.rotated(f32::NAN, Point::ZERO, |_| ran.set(true));
    });
    assert_fails_with_empty_canvas(doc, &Error::InvalidGeometry);
    assert!(!ran.get());
}

#[test]
fn missing_glyph_in_front_fails_before_any_page() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| p.text("A"));
    doc.front(|page| {
        if page.is_last() {
            page.text_at(MISSING.to_string(), Point::try_pt(10.0, 10.0).unwrap());
        }
    });
    assert_fails_with_empty_canvas(
        doc,
        &Error::MissingGlyph {
            font: FontId::from_raw(0),
            scalar: MISSING,
        },
    );
}

#[test]
fn header_without_margin_fails_before_any_page() {
    let mut doc = doc_with_font();
    doc.header(|h| h.left("H"));
    doc.paragraph(|p| p.text("A"));
    let mut rec = RecordingCanvas::new();
    let result = doc.paint(&mut rec);
    assert!(matches!(
        result,
        Err(Error::DecorationTallerThanMargin { margin, .. }) if margin == 0.0
    ));
    assert!(rec.commands().is_empty());
}

#[test]
fn keep_together_taller_than_the_page_fails_before_any_page() {
    let mut doc = doc_with_font();
    doc.keep_together(|g| {
        for _ in 0..100 {
            g.paragraph(|p| p.text("line"));
        }
    });
    assert_fails_with_empty_canvas(doc, &Error::UnsplittableTallerThanFrame);
}

#[test]
fn section_after_paragraph_fails_before_any_page() {
    let mut doc = doc_with_font();
    doc.paragraph(|p| p.text("A"));
    let err = doc
        .section(|section| {
            section.paragraph(|p| p.text("B"));
            Ok(())
        })
        .unwrap_err();
    assert_eq!(err, Error::ContentOutsideSection);
    assert_eq!(
        err.to_string(),
        "body or decorations written both on the document and in sections"
    );
    assert_fails_with_empty_canvas(doc, &Error::ContentOutsideSection);
}

#[test]
fn paragraph_after_section_fails_before_any_page() {
    let mut doc = doc_with_font();
    doc.section(|section| {
        section.paragraph(|p| p.text("A"));
        Ok(())
    })
    .unwrap();
    doc.paragraph(|p| p.text("B"));
    assert_fails_with_empty_canvas(doc, &Error::ContentOutsideSection);
}

#[test]
fn add_after_section_fails_before_any_page() {
    let mut doc = doc_with_font();
    doc.section(|section| {
        section.paragraph(|p| p.text("A"));
        Ok(())
    })
    .unwrap();
    doc.add(Paragraph::build(|p| p.text("B")));
    assert_fails_with_empty_canvas(doc, &Error::ContentOutsideSection);
}

#[test]
fn add_places_a_built_paragraph_and_table() {
    let mut via_add = doc_with_font();
    via_add.add(Paragraph::build(|p| p.text("Risk")));
    via_add.add(Table::build(|t| t.row(["cell"])));

    let mut via_sugar = doc_with_font();
    via_sugar.paragraph(|p| p.text("Risk"));
    via_sugar.table(|t| t.row(["cell"]));

    let Sections::Implicit(added) = &via_add.body else {
        panic!("add stays on the implicit section");
    };
    let Sections::Implicit(sugar) = &via_sugar.body else {
        panic!("sugar stays on the implicit section");
    };
    assert!(matches!(
        added.flow.nodes.first(),
        Some(FlowNode::Text(text)) if *text == TextNode::new("Risk", default_style())
    ));
    assert_eq!(added.flow.nodes, sugar.flow.nodes);
}

#[test]
fn margins_after_section_fail_before_any_page() {
    let mut doc = doc_with_font();
    doc.section(|section| {
        section.paragraph(|p| p.text("A"));
        Ok(())
    })
    .unwrap();
    doc.margins(Margins::uniform(NonNegative::pt(12.0).unwrap()));
    assert_fails_with_empty_canvas(doc, &Error::ContentOutsideSection);
}
