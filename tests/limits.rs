//! Public `Limits` caps through `document`.

use quire_pdf::{document, ErrorKind, FlowWriter, LimitKind, Limits, PageSize};

#[cfg(feature = "backend-krilla")]
use quire_pdf::ImageFormat;

const FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf");

fn tighten(edit: impl FnOnce(&mut Limits)) -> Limits {
    let mut limits = Limits::default();
    edit(&mut limits);
    limits
}

#[test]
fn font_byte_limit_rejects_before_parse() {
    let err = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| limits.set_max_font_bytes(1).unwrap()));
        doc.font(b"ab".to_vec())?;
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        err.first().kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::FontBytes,
            value: 2,
            max: 1,
        }
    );
}

#[cfg(feature = "backend-krilla")]
#[test]
fn image_byte_limit_rejects_before_decode() {
    let err = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| limits.set_max_image_bytes(1).unwrap()));
        let _ = doc.load_image(vec![1, 2], ImageFormat::Png)?;
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        err.first().kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::ImageBytes,
            value: 2,
            max: 1,
        }
    );
}

#[test]
fn one_page_fits_a_page_limit_of_one() {
    let rendered = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| limits.set_max_pages(1).unwrap()));
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text("A"));
        Ok(())
    })
    .unwrap();
    assert_eq!(rendered.pages().len(), 1);
}

#[test]
fn page_limit_fails_when_the_next_page_is_committed() {
    let err = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| limits.set_max_pages(1).unwrap()));
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text("A"));
        doc.page_break();
        doc.page_break();
        doc.paragraph(|p| p.text("B"));
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        err.first().kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::Pages,
            value: 2,
            max: 1,
        }
    );
}

#[test]
fn two_pages_fit_when_the_limit_is_two() {
    let rendered = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| limits.set_max_pages(2).unwrap()));
        doc.font(FONT.to_vec())?;
        doc.paragraph(|p| p.text("A"));
        doc.page_break();
        doc.paragraph(|p| p.text("B"));
        Ok(())
    })
    .unwrap();
    assert_eq!(rendered.pages().len(), 2);
}

#[test]
fn block_limit_counts_body_paragraphs() {
    let err = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| limits.set_max_blocks(1).unwrap()));
        doc.paragraph(|p| p.text("A"));
        doc.paragraph(|p| p.text("B"));
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        err.first().kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::Blocks,
            value: 2,
            max: 1,
        }
    );
}

#[test]
fn table_cell_limit_counts_cells() {
    let err = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| limits.set_max_table_cells(1).unwrap()));
        doc.table(|table| {
            table.row(["a", "b"]);
        });
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        err.first().kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::TableCells,
            value: 2,
            max: 1,
        }
    );
}

#[test]
fn paragraph_byte_limit_counts_utf8() {
    let err = document(PageSize::A4, |doc| {
        doc.set_limits(tighten(|limits| {
            limits.set_max_paragraph_bytes(1).unwrap();
        }));
        doc.paragraph(|p| p.text("ab"));
        Ok(())
    })
    .err()
    .unwrap();
    assert_eq!(
        err.first().kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::ParagraphBytes,
            value: 2,
            max: 1,
        }
    );
}
