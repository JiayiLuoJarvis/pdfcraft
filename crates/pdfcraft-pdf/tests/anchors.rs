//! §12.4 anchors, in-document links, and outline (contract §6.2).

#![expect(
    clippy::expect_used,
    reason = "integration test: a panic is the failure report"
)]
#![expect(
    clippy::unwrap_used,
    reason = "integration test: a panic is the failure report"
)]

use pdfcraft_pdf::canvas::pt_near;
use pdfcraft_pdf::spec::{
    BandFn, BandSlot, BandSpec, Block, CellSpec, CellStyle, FontFace, FontFamily, FontRegistry,
    InsetStyle, OutlineEntry, PageText, ParagraphSpec, Resources, RunStyle, SectionSpec,
    SectionStyle, Span, TableBorder, TableHeader, TableRow, TableSpec, TableStyle,
};
use pdfcraft_pdf::{
    layout_spec, CanvasCommand, DocumentSpec, ErrorKind, LinkTarget, Margins, NonNegative,
    PageSize, Region, Rendered, Result, Track,
};

#[expect(
    dead_code,
    reason = "each integration binary uses a subset of the shared font helpers"
)]
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
            style: SectionStyle {
                margins: Margins::try_uniform(48.0).unwrap(),
                ..SectionStyle::default()
            },
            body,
            ..SectionSpec::default()
        }],
    })
}

fn para(text: &str) -> Block<()> {
    Block::paragraph((), text)
}

fn anchor(id: &str, outline: Option<OutlineEntry>, child: Block<()>) -> Block<()> {
    anchor_children(id, outline, vec![child])
}

fn anchor_children(id: &str, outline: Option<OutlineEntry>, children: Vec<Block<()>>) -> Block<()> {
    Block::Anchor {
        id: id.into(),
        outline,
        children,
    }
}

fn link_anchor(id: &str, text: &str) -> Span<()> {
    Span::Link {
        target: LinkTarget::anchor(id).unwrap(),
        children: vec![Span::Text {
            text: text.into(),
            font: (),
            style: RunStyle::default(),
        }],
    }
}

fn link_uri(uri: &str, text: &str) -> Span<()> {
    Span::Link {
        target: LinkTarget::uri(uri).unwrap(),
        children: vec![Span::Text {
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
            CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::StrokePath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::DrawGlyphRun { .. }
            | CanvasCommand::AnnotateLink { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform
            | _ => None,
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
            CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::StrokePath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::DrawGlyphRun { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform
            | _ => None,
        })
        .collect()
}

fn glyph_texts(rendered: &Rendered) -> Vec<String> {
    rendered
        .pages()
        .iter()
        .flat_map(pdfcraft_pdf::RecordedPage::ops)
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(run.text.clone()),
            CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::StrokePath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::AnnotateLink { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform
            | _ => None,
        })
        .collect()
}

#[test]
fn uri_links_still_record_as_uri() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![Block::Paragraph(ParagraphSpec {
        children: vec![link_uri("https://example.test/a", "go")],
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
            children: vec![link_anchor("sec", "jump")],
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
fn inset_shifts_anchor_x() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        Block::Inset {
            style: InsetStyle {
                left: NonNegative::pt(20.0).unwrap(),
                right: NonNegative::ZERO,
            },
            children: vec![anchor("sec", None, para("Section"))],
        },
        Block::Paragraph(ParagraphSpec {
            children: vec![link_anchor("sec", "jump")],
            ..ParagraphSpec::default()
        }),
    ])
    .unwrap();
    let hits = xyz_destinations(&rendered);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 0);
    assert!(pt_near(hits[0].1, 68.0));
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
            children: vec![link_anchor("gap", "to gap")],
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
fn anchor_skips_zero_height_children_and_lands_on_positive_spacer() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        anchor_children(
            "gap",
            None,
            vec![
                Block::Spacer(NonNegative::ZERO),
                Block::Spacer(NonNegative::pt(24.0).unwrap()),
                para("Section"),
            ],
        ),
        Block::Paragraph(ParagraphSpec {
            children: vec![link_anchor("gap", "to gap")],
            ..ParagraphSpec::default()
        }),
    ])
    .unwrap();
    let hits = xyz_destinations(&rendered);
    assert_eq!(hits, [(0, 48.0, 48.0)]);
}

#[test]
fn anchor_page_break_uses_body_pagination_and_lands_after_it() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        para("before"),
        anchor_children("next", None, vec![Block::PageBreak, para("after")]),
        Block::Paragraph(ParagraphSpec {
            children: vec![link_anchor("next", "jump")],
            ..ParagraphSpec::default()
        }),
    ])
    .unwrap();
    let hits = xyz_destinations(&rendered);
    assert_eq!(rendered.pages().len(), 2);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, 1);
    assert!(pt_near(hits[0].2, 48.0));
}

#[test]
fn anchor_consecutive_page_breaks_preserve_the_blank_page() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        para("before"),
        anchor_children(
            "next",
            None,
            vec![Block::PageBreak, Block::PageBreak, para("after")],
        ),
        Block::Paragraph(ParagraphSpec {
            children: vec![link_anchor("next", "jump")],
            ..ParagraphSpec::default()
        }),
    ])
    .unwrap();
    assert_eq!(rendered.pages().len(), 3);
    assert!(rendered.pages()[1].ops().is_empty());
    assert_eq!(xyz_destinations(&rendered)[0].0, 2);
}

#[test]
fn nested_anchors_land_together() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![
        anchor_children(
            "outer",
            None,
            vec![anchor_children(
                "inner",
                None,
                vec![Block::PageBreak, para("after")],
            )],
        ),
        Block::Paragraph(ParagraphSpec {
            children: vec![link_anchor("outer", "outer"), link_anchor("inner", "inner")],
            ..ParagraphSpec::default()
        }),
    ])
    .unwrap();
    let hits = xyz_destinations(&rendered);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0], hits[1]);
}

#[test]
fn zero_inset_and_top_level_anchor_have_identical_output() {
    if skip_without_font() {
        return;
    }
    let make = |wrapped: bool| {
        let anchored = anchor_children(
            "same",
            None,
            vec![Block::Spacer(NonNegative::ZERO), para("Section")],
        );
        let anchored = if wrapped {
            Block::Inset {
                style: InsetStyle::default(),
                children: vec![anchored],
            }
        } else {
            anchored
        };
        lay(vec![
            anchored,
            Block::Paragraph(ParagraphSpec {
                children: vec![link_anchor("same", "jump")],
                ..ParagraphSpec::default()
            }),
        ])
        .unwrap()
    };
    assert_eq!(make(false).pages()[0].ops(), make(true).pages()[0].ops());
}

#[test]
fn nested_anchor_page_break_remains_rejected() {
    if skip_without_font() {
        return;
    }
    for wrapper in [
        Block::Inset {
            style: InsetStyle::default(),
            children: vec![anchor_children(
                "inset",
                None,
                vec![Block::PageBreak, para("after")],
            )],
        },
        Block::KeepTogether {
            children: vec![anchor_children(
                "kept",
                None,
                vec![Block::PageBreak, para("after")],
            )],
        },
    ] {
        let error = lay(vec![wrapper]).unwrap_err();
        assert_eq!(
            error.first().kind,
            ErrorKind::InvalidGeometry(pdfcraft_pdf::GeometryKind::NestedPageBreak)
        );
    }
}

#[test]
fn unknown_anchor_fails() {
    if skip_without_font() {
        return;
    }
    let Err(err) = lay(vec![Block::Paragraph(ParagraphSpec {
        children: vec![link_anchor("missing", "x")],
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
        children: vec![para("x")],
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
        anchor_children("empty", None, Vec::new()),
        anchor("e0", None, Block::Spacer(NonNegative::ZERO)),
        anchor("e1", None, Block::PageBreak),
        anchor_children(
            "e-break",
            None,
            vec![Block::Spacer(NonNegative::ZERO), Block::PageBreak],
        ),
        anchor(
            "e2",
            None,
            Block::KeepTogether {
                children: Vec::new(),
            },
        ),
    ];
    for case in cases {
        let Err(err) = lay(vec![case]) else {
            panic!("expected error");
        };
        assert_eq!(err.first().kind, ErrorKind::AnchorWithoutFragment);
    }
}

#[test]
fn empty_anchor_does_not_land_on_a_later_sibling() {
    if skip_without_font() {
        return;
    }
    let error = lay(vec![
        anchor_children("empty", None, vec![Block::Spacer(NonNegative::ZERO)]),
        para("later"),
    ])
    .unwrap_err();
    assert_eq!(error.first().kind, ErrorKind::AnchorWithoutFragment);
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
    assert!(!rendered.outline_is_empty());
}

#[test]
fn no_outline_entries_leave_outline_empty() {
    if skip_without_font() {
        return;
    }
    let rendered = lay(vec![anchor("only", None, para("x"))]).unwrap();
    assert_eq!(rendered.outline_len(), 0);
    assert!(rendered.outline_is_empty());
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
            style: SectionStyle {
                margins: Margins::try_new(48.0, 48.0, 48.0, 48.0).unwrap(),
                ..SectionStyle::default()
            },
            header: Some(BandFn::fixed(BandSpec {
                left: Some(BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal("H".into())],
                    link: Some(vec![PageText::Literal("https://band.example".into())]),
                }),
                ..BandSpec::default()
            })),
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
            style: SectionStyle {
                margins: Margins::try_new(48.0, 48.0, 48.0, 48.0).unwrap(),
                ..SectionStyle::default()
            },
            header: Some(BandFn::fixed(BandSpec {
                left: Some(BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal("H".into())],
                    link: Some(vec![PageText::Literal(String::new())]),
                }),
                ..BandSpec::default()
            })),
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
    let cell = |blocks: Vec<Block<()>>| CellSpec {
        style: CellStyle::default(),
        children: blocks,
        col_span: std::num::NonZeroU16::new(1).unwrap(),
        row_span: std::num::NonZeroU16::new(1).unwrap(),
    };
    let header_anchor = Block::Anchor {
        id: "hdr".into(),
        outline: None,
        children: vec![Block::paragraph((), "H")],
    };
    let row = |text: &str| TableRow {
        children: vec![cell(vec![Block::paragraph((), text)])],
    };
    // One A4 body is about 746 pt under 48 pt margins. Line height is at least
    // 12 pt * 1.2, so 40 rows can still fit. 80 rows cannot, on any face.
    let mut body_rows = Vec::new();
    for i in 0..80 {
        body_rows.push(row(&format!("row{i}")));
    }
    let table = Block::Table(TableSpec {
        style: TableStyle {
            tracks: Some(vec![Track::Auto]),
            cell_padding: NonNegative::ZERO,
            border: TableBorder::default(),
        },
        header: Some(TableHeader {
            repeat: true,
            rows: vec![TableRow {
                children: vec![cell(vec![header_anchor])],
            }],
        }),
        body: body_rows,
    });
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: font_resources(),
        limits: pdfcraft_pdf::Limits::default(),
        info: pdfcraft_pdf::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            style: SectionStyle {
                margins: Margins::try_uniform(48.0).unwrap(),
                ..SectionStyle::default()
            },
            body: vec![
                table,
                Block::Paragraph(ParagraphSpec {
                    children: vec![link_anchor("hdr", "to header")],
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

#[test]
fn empty_table_header_is_equivalent_to_none() {
    if skip_without_font() {
        return;
    }
    let make = |header| {
        lay(vec![Block::Table(TableSpec {
            header,
            body: vec![TableRow {
                children: vec![CellSpec {
                    children: vec![para("body")],
                    ..CellSpec::default()
                }],
            }],
            ..TableSpec::default()
        })])
        .unwrap()
    };
    let none = make(None);
    let empty = make(Some(TableHeader {
        repeat: true,
        rows: Vec::new(),
    }));
    assert_eq!(none.pages()[0].ops(), empty.pages()[0].ops());
}

#[test]
fn non_repeating_table_header_only_paints_once() {
    if skip_without_font() {
        return;
    }
    let row = |text: String| TableRow {
        children: vec![CellSpec {
            children: vec![para(&text)],
            ..CellSpec::default()
        }],
    };
    let rendered = lay(vec![Block::Table(TableSpec {
        header: Some(TableHeader {
            repeat: false,
            rows: vec![row("HEADER".into())],
        }),
        body: (0..80).map(|index| row(format!("row{index}"))).collect(),
        ..TableSpec::default()
    })])
    .unwrap();
    assert!(rendered.pages().len() >= 2);
    assert_eq!(
        glyph_texts(&rendered)
            .iter()
            .filter(|text| text.as_str() == "HEADER")
            .count(),
        1
    );
}
