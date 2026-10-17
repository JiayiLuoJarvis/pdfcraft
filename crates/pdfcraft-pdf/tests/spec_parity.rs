//! Behavior checks for the data-only document specification path.

use pdfcraft_pdf::canvas::pt_near;
#[cfg(feature = "backend-krilla")]
use pdfcraft_pdf::spec::{Alignment, CellBlock, CellSpec, ImageRef, ImageSpec, TableSpec};
use pdfcraft_pdf::spec::{
    BandInset, BandSlot, Block, FontFace, FontFamily, FontRegistry, LayerMark, PageWhen, Resources,
    RunStyle, SectionSpec, TextRun,
};
#[cfg(feature = "backend-krilla")]
use pdfcraft_pdf::to_pdf;
use pdfcraft_pdf::{
    layout_spec, CanvasCommand, DocumentSpec, Margins, NonNegative, PageSize, Point,
};
#[cfg(feature = "backend-krilla")]
use pdfcraft_pdf::{ErrorKind, ImageFormat};

#[path = "common/mod.rs"]
mod common;

fn skip_without_font() -> bool {
    common::test_font_bytes().is_none()
}

fn font_bytes() -> Vec<u8> {
    common::test_font_bytes().expect("set PDFCRAFT_PDF_FONT or PDFCRAFT_CJK_FONT")
}
fn baseline() -> f32 {
    common::body_line_12(&font_bytes()).0
}

fn texts(page: &pdfcraft_pdf::RecordedPage) -> Vec<&str> {
    page.ops()
        .iter()
        .filter_map(|command| match command {
            CanvasCommand::DrawGlyphRun { run } => Some(run.text.as_str()),
            _ => None,
        })
        .collect()
}

fn links(page: &pdfcraft_pdf::RecordedPage) -> Vec<&str> {
    page.ops()
        .iter()
        .filter_map(|command| match command {
            CanvasCommand::AnnotateLink { target, .. } => Some(target.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn two_page_spec_resolves_page_text_links_and_last_page_marks() {
    if skip_without_font() {
        return;
    }
    let margin = NonNegative::pt(36.0).unwrap();
    let mut section = SectionSpec {
        margins: Margins::uniform(margin),
        ..SectionSpec::default()
    };
    section.header = Some(pdfcraft_pdf::spec::BandSpec {
        inset: BandInset::PageMargins,
        left: Some(BandSlot::Link {
            font: (),
            uri: vec![
                TextRun::Literal("https://example.com/".to_owned()),
                TextRun::PageNumber,
            ],
            text: vec![TextRun::Literal("Home".to_owned())],
            style: RunStyle::default(),
        }),
        right: Some(BandSlot::Text {
            runs: vec![
                TextRun::Literal("第 ".to_owned()),
                TextRun::PageNumber,
                TextRun::Literal(" 页 / 共 ".to_owned()),
                TextRun::PageCount,
                TextRun::Literal(" 页".to_owned()),
            ],
            font: (),
            style: RunStyle::default(),
        }),
    });
    section.body = vec![
        Block::paragraph((), "Alpha"),
        Block::PageBreak,
        Block::paragraph((), "Beta"),
    ];
    section.front.push(LayerMark::Text {
        font: (),
        when: PageWhen::Last,
        text: vec![TextRun::Literal("SEAL".to_owned())],
        at: Point::try_pt(430.0, 680.0).unwrap(),
        style: RunStyle::default(),
    });

    let spec = DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(font_bytes(), 0),
            bold: None,
            italic: None,
            bold_italic: None,
        })),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![section],
    };
    let rendered = layout_spec(spec).unwrap();
    let pages = rendered.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(links(&pages[0]), ["https://example.com/1"]);
    assert_eq!(links(&pages[1]), ["https://example.com/2"]);
    assert!(texts(&pages[0]).contains(&"Alpha"));
    assert!(!texts(&pages[0]).contains(&"Beta"));
    assert!(!texts(&pages[0]).contains(&"SEAL"));
    assert!(texts(&pages[1]).contains(&"Beta"));
    assert!(texts(&pages[1]).contains(&"SEAL"));

    let seal = pages[1]
        .ops()
        .iter()
        .find_map(|command| match command {
            CanvasCommand::DrawGlyphRun { run } if run.text == "SEAL" => Some(run),
            _ => None,
        })
        .unwrap();
    assert!(pt_near(seal.origin.x().as_pt(), 430.0));
    assert!(pt_near(seal.origin.y().as_pt(), 680.0 + baseline()));

    #[cfg(feature = "backend-krilla")]
    {
        let pdf = to_pdf(rendered).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        // Integration tests run with cwd = this crate; workspace `target/` is two levels up.
        let out =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/spec-parity.pdf");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::write(&out, pdf).unwrap();
    }
}

#[cfg(feature = "backend-krilla")]
#[test]
fn foreign_cell_image_ref_is_unknown() {
    if skip_without_font() {
        return;
    }
    let png = include_bytes!("fixtures/one-pixel.png");
    let mut other = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(font_bytes(), 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }));
    let foreign = other.add_image(png.to_vec(), ImageFormat::Png);
    let mut dest = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(font_bytes(), 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }));
    let _owned = dest.add_image(png.to_vec(), ImageFormat::Png);
    let err = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: dest,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![cell_image_section(foreign)],
    })
    .err()
    .unwrap();
    assert_eq!(err.as_slice().len(), 1);
    assert!(matches!(
        err.first().kind.clone(),
        ErrorKind::UnknownImage { id: 0 }
    ));
    assert_eq!(err.first().at.as_ref().and_then(|at| at.section), Some(0));
}

#[cfg(feature = "backend-krilla")]
#[test]
fn local_cell_image_ref_draws() {
    if skip_without_font() {
        return;
    }
    let png = include_bytes!("fixtures/one-pixel.png");
    let mut dest = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(font_bytes(), 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }));
    let owned = dest.add_image(png.to_vec(), ImageFormat::Png);
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: dest,
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![cell_image_section(owned)],
    })
    .unwrap();
    let draws = rendered
        .pages()
        .iter()
        .flat_map(pdfcraft_pdf::RecordedPage::ops)
        .filter(|command| matches!(command, CanvasCommand::DrawImage { .. }))
        .count();
    assert_eq!(draws, 1);
}

#[cfg(feature = "backend-krilla")]
fn cell_image_section(image: ImageRef) -> SectionSpec<()> {
    let mut section = SectionSpec::default();
    section.body.push(Block::Table(TableSpec {
        body: vec![vec![CellSpec {
            content: vec![CellBlock::Image(ImageSpec {
                image,
                width: None,
                height: None,
                align: Alignment::Start,
            })],
            ..CellSpec::default()
        }]],
        ..TableSpec::default()
    }));
    section
}
