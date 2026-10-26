//! Public `Limits` caps through `layout_spec`.

use pdfcraft_pdf::spec::{
    Block, CellBlock, CellSpec, FontFace, FontFamily, FontRegistry, Resources, SectionSpec,
    TableRow, TableSpec,
};
#[cfg(feature = "backend-krilla")]
use pdfcraft_pdf::ImageFormat;
use pdfcraft_pdf::{layout_spec, DocumentSpec, ErrorKind, LimitKind, Limits, PageSize};

#[path = "common/mod.rs"]
mod common;

fn tighten(edit: impl FnOnce(&mut Limits)) -> Limits {
    let mut limits = Limits::default();
    edit(&mut limits);
    limits
}

fn lay(
    resources: Resources<()>,
    limits: Limits,
    section: SectionSpec<()>,
) -> pdfcraft_pdf::Result<pdfcraft_pdf::Rendered> {
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits,
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![section],
    })
}

fn fonted(
    limits: Limits,
    body: Vec<Block<()>>,
) -> Option<pdfcraft_pdf::Result<pdfcraft_pdf::Rendered>> {
    let font = common::test_font_bytes()?;
    Some(lay(
        Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(font, 0),
            bold: None,
            italic: None,
            bold_italic: None,
        })),
        limits,
        SectionSpec {
            body,
            ..SectionSpec::default()
        },
    ))
}

#[test]
fn font_byte_limit_rejects_before_parse() {
    let err = lay(
        Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(b"ab".to_vec(), 0),
            bold: None,
            italic: None,
            bold_italic: None,
        })),
        tighten(|limits| limits.set_max_font_bytes(1).unwrap()),
        SectionSpec::default(),
    )
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
    let mut resources = Resources::default();
    let _ = resources.add_image(vec![1, 2], ImageFormat::Png);
    let err = lay(
        resources,
        tighten(|limits| limits.set_max_image_bytes(1).unwrap()),
        SectionSpec::default(),
    )
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
    let Some(rendered) = fonted(
        tighten(|limits| limits.set_max_pages(1).unwrap()),
        vec![Block::paragraph((), "A")],
    ) else {
        return;
    };
    let rendered = rendered.unwrap();
    assert_eq!(rendered.pages().len(), 1);
}

#[test]
fn page_limit_fails_when_the_next_page_is_committed() {
    let Some(err) = fonted(
        tighten(|limits| limits.set_max_pages(1).unwrap()),
        vec![
            Block::paragraph((), "A"),
            Block::PageBreak,
            Block::PageBreak,
            Block::paragraph((), "B"),
        ],
    ) else {
        return;
    };
    let err = err.err().unwrap();
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
    let Some(rendered) = fonted(
        tighten(|limits| limits.set_max_pages(2).unwrap()),
        vec![
            Block::paragraph((), "A"),
            Block::PageBreak,
            Block::paragraph((), "B"),
        ],
    ) else {
        return;
    };
    let rendered = rendered.unwrap();
    assert_eq!(rendered.pages().len(), 2);
}

#[test]
fn block_limit_counts_body_paragraphs() {
    let Some(err) = fonted(
        tighten(|limits| limits.set_max_blocks(1).unwrap()),
        vec![Block::paragraph((), "A"), Block::paragraph((), "B")],
    ) else {
        return;
    };
    let err = err.err().unwrap();
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
    let Some(err) = fonted(
        tighten(|limits| limits.set_max_table_cells(1).unwrap()),
        vec![Block::Table(TableSpec {
            body: vec![TableRow {
                cells: vec![
                    CellSpec {
                        content: vec![CellBlock::Text {
                            text: "a".into(),
                            font: (),
                        }],
                        ..CellSpec::default()
                    },
                    CellSpec {
                        content: vec![CellBlock::Text {
                            text: "b".into(),
                            font: (),
                        }],
                        ..CellSpec::default()
                    },
                ],
            }],
            ..TableSpec::default()
        })],
    ) else {
        return;
    };
    let err = err.err().unwrap();
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
    let Some(err) = fonted(
        tighten(|limits| limits.set_max_paragraph_bytes(1).unwrap()),
        vec![Block::paragraph((), "ab")],
    ) else {
        return;
    };
    let err = err.err().unwrap();
    assert_eq!(
        err.first().kind,
        ErrorKind::LimitExceeded {
            kind: LimitKind::ParagraphBytes,
            value: 2,
            max: 1,
        }
    );
}
