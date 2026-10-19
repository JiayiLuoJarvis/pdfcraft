//! §12.4 anchors, in-document links, and outline (contract §6.2).

use pdfcraft_pdf::canvas::pt_near;
use pdfcraft_pdf::spec::{
    BandSlot, BandSpec, Block, CellBlock, CellSpec, FontFace, FontFamily, FontRegistry, InlineRun,
    OutlineEntry, ParagraphSpec, Resources, RunStyle, SectionSpec, TableSpec, TextRun,
};
use pdfcraft_pdf::{
    layout_spec, CanvasCommand, Color, DocumentSpec, ErrorKind, LinkTarget, Margins, NonNegative,
    PageSize, Region, Rendered, Result, Track,
};

#[path = "common/mod.rs"]
mod common;

fn skip_without_font() -> bool {
    common::test_font_bytes().is_none()
}

fn font_resources() -> Resources<()> {
    let bytes = common::test_font_bytes().expect("font");
    Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(bytes, 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }))
}

fn lay(body: Vec<Block<()>>) -> Result<Rendered> {
    layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: font_resources(),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_uniform(48.0).unwrap(),
            body,
            ..SectionSpec::default()
        }],
    })
}

fn para(text: &str) -> Block<()> {
    Block::paragraph((), text)
}

fn anchor(id: &str, outline: Option<OutlineEntry>, child: Block<()>) -> Block<()> {
    Block::Anchor {
        id: id.into(),
        outline,
        child: Box::new(child),
    }
}

fn link_anchor(id: &str, text: &str) -> InlineRun<()> {
    InlineRun::Link {
        target: LinkTarget::anchor(id).unwrap(),
        runs: vec![InlineRun::Text {
            text: text.into(),
            font: (),
            style: RunStyle::default(),
        }],
    }
}

fn link_uri(uri: &str, text: &str) -> InlineRun<()> {
    InlineRun::Link {
        target: LinkTarget::uri(uri).unwrap(),
        runs: vec![InlineRun::Text {
            text: text.into(),
            font: (),
            style: RunStyle::default(),
        }],
    }
}

fn xyz_destinations(rendered: &Rendered) -> Vec<(usize, f32, f32)> {
    let pages = rendered.pages();
    pages
        .iter()
        .flat_map(pdfcraft_pdf::RecordedPage::ops)
        .filter_map(|op| match op {
            CanvasCommand::AnnotateLink {
                target: LinkTarget::Xyz(xyz),
                ..
            } => Some((
                xyz.page_index(),
                xyz.point().x().as_pt(),
                xyz.point().y().as_pt(),
            )),
            _ => None,
        })
        .collect()
}

fn uri_hits(rendered: &Rendered) -> Vec<String> {
    let pages = rendered.pages();
    pages
        .iter()
        .flat_map(pdfcraft_pdf::RecordedPage::ops)
        .filter_map(|op| match op {
            CanvasCommand::AnnotateLink { target, .. } => target.uri_str().map(str::to_owned),
            _ => None,
        })
        .collect()
}

#[test]
fn uri_links_still_record_as_uri() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![Block::Paragraph(ParagraphSpec {
        runs: vec![link_uri("https://example.test/a", "go")],
        ..ParagraphSpec::default()
    })])
    .unwrap();
    assert_eq!(uri_hits(&rendered), ["https://example.test/a".to_owned()]);
    assert!(xyz_destinations(&rendered).is_empty());
}

#[test]
fn anchor_link_resolves_to_xyz_at_body_origin() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        anchor("sec", None, para("Section")),
        Block::Paragraph(ParagraphSpec {
            runs: vec![link_anchor("sec", "jump")],
            ..ParagraphSpec::default()
        }),
    ])
    .unwrap();
    let hits = xyz_destinations(&rendered);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 0);
    assert!(pt_near(hits[0].1, 48.0));
    assert!(pt_near(hits[0].2, 48.0));
}

#[test]
fn spacer_anchor_is_a_valid_destination() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        anchor("gap", None, Block::Spacer(NonNegative::pt(24.0).unwrap())),
        Block::Paragraph(ParagraphSpec {
            runs: vec![link_anchor("gap", "to gap")],
            ..ParagraphSpec::default()
        }),
    ])
    .unwrap();
    let hits = xyz_destinations(&rendered);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 0);
    assert!(pt_near(hits[0].2, 48.0));
}

#[test]
fn unknown_anchor_fails() {
    if skip_without_font() {
        return;
    }
    let Err(err) = lay(vec![Block::Paragraph(ParagraphSpec {
        runs: vec![link_anchor("missing", "x")],
        ..ParagraphSpec::default()
    })]) else {
        panic!("expected error");
    };
    assert_eq!(err.first().kind, ErrorKind::UnknownAnchor);
}

#[test]
fn empty_anchor_id_fails() {
    if skip_without_font() {
        return;
    }
    let Err(err) = lay(vec![Block::Anchor {
        id: String::new(),
        outline: None,
        child: Box::new(para("x")),
    }]) else {
        panic!("expected error");
    };
    assert_eq!(err.first().kind, ErrorKind::EmptyAnchorId);
}

#[test]
fn duplicate_anchor_id_fails_on_later() {
    if skip_without_font() {
        return;
    }
    let Err(err) = lay(vec![
        anchor("same", None, para("a")),
        anchor("same", None, para("b")),
    ]) else {
        panic!("expected error");
    };
    assert_eq!(err.first().kind, ErrorKind::DuplicateAnchorId);
}

#[test]
fn anchor_without_fragment_fails() {
    if skip_without_font() {
        return;
    }
    let cases = [
        anchor("e0", None, Block::Spacer(NonNegative::ZERO)),
        anchor("e1", None, Block::PageBreak),
        anchor("e2", None, Block::KeepTogether(Vec::new())),
    ];
    for case in cases {
        let Err(err) = lay(vec![case]) else {
            panic!("expected error");
        };
        assert_eq!(err.first().kind, ErrorKind::AnchorWithoutFragment);
    }
}

#[test]
fn empty_outline_title_fails() {
    if skip_without_font() {
        return;
    }
    let Err(err) = lay(vec![anchor(
        "a",
        Some(OutlineEntry {
            title: String::new(),
            level: 1,
        }),
        para("x"),
    )]) else {
        panic!("expected error");
    };
    assert_eq!(err.first().kind, ErrorKind::EmptyOutlineTitle);
}

#[test]
fn invalid_outline_level_first_not_one() {
    if skip_without_font() {
        return;
    }
    let Err(err) = lay(vec![anchor(
        "a",
        Some(OutlineEntry {
            title: "A".into(),
            level: 2,
        }),
        para("x"),
    )]) else {
        panic!("expected error");
    };
    assert_eq!(err.first().kind, ErrorKind::InvalidOutlineLevel);
}

#[test]
fn invalid_outline_level_deepens_too_far() {
    if skip_without_font() {
        return;
    }
    let Err(err) = lay(vec![
        anchor(
            "a",
            Some(OutlineEntry {
                title: "A".into(),
                level: 1,
            }),
            para("x"),
        ),
        anchor(
            "b",
            Some(OutlineEntry {
                title: "B".into(),
                level: 3,
            }),
            para("y"),
        ),
    ]) else {
        panic!("expected error");
    };
    assert_eq!(err.first().kind, ErrorKind::InvalidOutlineLevel);
}

#[test]
fn outline_level_may_jump_up() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        anchor(
            "a",
            Some(OutlineEntry {
                title: "A".into(),
                level: 1,
            }),
            para("x"),
        ),
        anchor(
            "b",
            Some(OutlineEntry {
                title: "B".into(),
                level: 2,
            }),
            para("y"),
        ),
        anchor(
            "c",
            Some(OutlineEntry {
                title: "C".into(),
                level: 3,
            }),
            para("z"),
        ),
        anchor(
            "d",
            Some(OutlineEntry {
                title: "D".into(),
                level: 1,
            }),
            para("w"),
        ),
    ])
    .unwrap();
    assert_eq!(rendered.outline_len(), 4);
}

#[test]
fn no_outline_entries_leave_outline_empty() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![anchor("only", None, para("x"))]).unwrap();
    assert_eq!(rendered.outline_len(), 0);
}

#[test]
fn band_link_stays_uri_only() {
    if skip_without_font() {
        return;
    }
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: font_resources(),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_new(48.0, 48.0, 48.0, 48.0).unwrap(),
            header: Some(BandSpec {
                left: Some(BandSlot::Link {
                    font: (),
                    uri: vec![TextRun::Literal("https://band.example".into())],
                    text: vec![TextRun::Literal("H".into())],
                    style: RunStyle::default(),
                }),
                ..BandSpec::default()
            }),
            body: vec![para("body")],
            ..SectionSpec::default()
        }],
    })
    .unwrap();
    assert!(uri_hits(&rendered)
        .iter()
        .any(|u| u == "https://band.example"));
}

#[test]
fn empty_band_uri_is_empty_link_in_header() {
    if skip_without_font() {
        return;
    }
    let Err(err) = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: font_resources(),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_new(48.0, 48.0, 48.0, 48.0).unwrap(),
            header: Some(BandSpec {
                left: Some(BandSlot::Link {
                    font: (),
                    uri: vec![TextRun::Literal(String::new())],
                    text: vec![TextRun::Literal("H".into())],
                    style: RunStyle::default(),
                }),
                ..BandSpec::default()
            }),
            body: vec![para("body")],
            ..SectionSpec::default()
        }],
    }) else {
        panic!("expected error");
    };
    assert_eq!(err.first().kind, ErrorKind::EmptyLink);
    assert_eq!(
        err.first().at.as_ref().and_then(|at| at.region),
        Some(Region::Header)
    );
}

#[test]
fn repeating_header_anchor_keeps_first_destination() {
    if skip_without_font() {
        return;
    }
    let cell = |blocks: Vec<CellBlock<()>>| CellSpec {
        content: blocks,
        col_span: std::num::NonZeroU16::new(1).unwrap(),
        row_span: std::num::NonZeroU16::new(1).unwrap(),
        fill: None,
    };
    let header_anchor = CellBlock::Anchor {
        id: "hdr".into(),
        outline: None,
        child: Box::new(CellBlock::Text {
            text: "H".into(),
            font: (),
        }),
    };
    let row = |text: &str| {
        vec![cell(vec![CellBlock::Text {
            text: text.into(),
            font: (),
        }])]
    };
    // Enough body rows + small page to force the repeating header onto page 2.
    let mut body_rows = Vec::new();
    for i in 0..40 {
        body_rows.push(row(&format!("row{i}")));
    }
    let table = Block::Table(TableSpec {
        tracks: Some(vec![Track::Auto]),
        header: vec![vec![cell(vec![header_anchor])]],
        body: body_rows,
        repeat_header: true,
        cell_padding: NonNegative::ZERO,
        border_width: NonNegative::ZERO,
        border_color: Color::rgb(0, 0, 0),
    });
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: font_resources(),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_uniform(48.0).unwrap(),
            body: vec![
                table,
                Block::Paragraph(ParagraphSpec {
                    runs: vec![link_anchor("hdr", "to header")],
                    ..ParagraphSpec::default()
                }),
            ],
            ..SectionSpec::default()
        }],
    })
    .unwrap();
    assert!(
        rendered.pages().len() >= 2,
        "expected header to repeat across pages, got {}",
        rendered.pages().len()
    );
    let hits = xyz_destinations(&rendered);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 0, "first-wins must keep page 0 destination");
}
