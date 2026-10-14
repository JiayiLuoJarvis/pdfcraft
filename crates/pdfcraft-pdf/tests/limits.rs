//! Public `Limits` caps through `layout_spec`.

use pdfcraft_pdf::spec::{Block, CellBlock, CellSpec, Resources, SectionSpec, TableSpec};
#[cfg(feature = "backend-krilla")]
use pdfcraft_pdf::ImageFormat;
use pdfcraft_pdf::{layout_spec, DocumentSpec, ErrorKind, LimitKind, Limits, PageSize};

const FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf");

fn tighten(edit: impl FnOnce(&mut Limits)) -> Limits {
    let mut limits = Limits::default();
    edit(&mut limits);
    limits
}

fn lay(
    resources: Resources,
    limits: Limits,
    section: SectionSpec,
) -> pdfcraft_pdf::Result<pdfcraft_pdf::Rendered> {
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits,
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![section],
    })
}

fn fonted(limits: Limits, body: Vec<Block>) -> pdfcraft_pdf::Result<pdfcraft_pdf::Rendered> {
    lay(
        Resources::new(FONT.to_vec()),
        limits,
        SectionSpec {
            body,
            ..SectionSpec::default()
        },
    )
}

#[test]
fn font_byte_limit_rejects_before_parse() {
    let err = lay(
        Resources::new(b"ab".to_vec()),
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
    let rendered = fonted(
        tighten(|limits| limits.set_max_pages(1).unwrap()),
        vec![Block::paragraph("A")],
    )
    .unwrap();
    assert_eq!(rendered.pages().len(), 1);
}

#[test]
fn page_limit_fails_when_the_next_page_is_committed() {
    let err = fonted(
        tighten(|limits| limits.set_max_pages(1).unwrap()),
        vec![
            Block::paragraph("A"),
            Block::PageBreak,
            Block::PageBreak,
            Block::paragraph("B"),
        ],
    )
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
    let rendered = fonted(
        tighten(|limits| limits.set_max_pages(2).unwrap()),
        vec![
            Block::paragraph("A"),
            Block::PageBreak,
            Block::paragraph("B"),
        ],
    )
    .unwrap();
    assert_eq!(rendered.pages().len(), 2);
}

#[test]
fn block_limit_counts_body_paragraphs() {
    let err = fonted(
        tighten(|limits| limits.set_max_blocks(1).unwrap()),
        vec![Block::paragraph("A"), Block::paragraph("B")],
    )
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
    let err = fonted(
        tighten(|limits| limits.set_max_table_cells(1).unwrap()),
        vec![Block::Table(TableSpec {
            body: vec![vec![
                CellSpec {
                    content: vec![CellBlock::Text("a".into())],
                    ..CellSpec::default()
                },
                CellSpec {
                    content: vec![CellBlock::Text("b".into())],
                    ..CellSpec::default()
                },
            ]],
            ..TableSpec::default()
        })],
    )
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
    let err = fonted(
        tighten(|limits| limits.set_max_paragraph_bytes(1).unwrap()),
        vec![Block::paragraph("ab")],
    )
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
