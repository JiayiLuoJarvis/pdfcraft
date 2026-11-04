//! Painted and error behavior of values lowered by `layout_spec`.

use std::sync::Arc;

use crate::canvas::{pt_near, CanvasCommand, LinkTarget};
use crate::error::{ErrorKind, GeometryKind, Location, Region};
use crate::fonts::fixture_bytes;
use crate::geom::{Color, Ink, Margins, NonNegative, PageSize, Pen, Point, Positive, Rect, Size};
use crate::layout_spec;
use crate::spec::{
    Alignment, BlankSpec, BlankWidth, Block, FontFace, FontFamily, FontRegistry, LayerMark,
    LayerShape, ListItemSpec, ListKind, ListSpec, ListStyle, Mark, MarkerFormat, Numbering,
    PageText, PageWhen, ParagraphSpec, ParagraphStyle, Resources, RunStyle, SectionSpec,
    SectionStyle, Span,
};
use crate::DocumentSpec;

fn resources() -> Option<Resources<()>> {
    let bytes = fixture_bytes()?;
    Some(Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::from_shared(Arc::clone(&bytes), 0),
        bold: Some(FontFace::from_shared(bytes, 0)),
        italic: None,
        bold_italic: None,
    })))
}

fn lay(page: PageSize, section: SectionSpec<()>) -> Option<crate::Result<crate::Rendered>> {
    Some(layout_spec(DocumentSpec {
        page_size: page,
        resources: resources()?,
        limits: crate::Limits::default(),
        info: crate::spec::DocumentInfo::default(),
        sections: vec![section],
    }))
}

fn body(blocks: Vec<Block<()>>) -> SectionSpec<()> {
    SectionSpec {
        body: blocks,
        ..SectionSpec::default()
    }
}

fn texts(rendered: &crate::Rendered) -> Vec<String> {
    rendered
        .pages()
        .iter()
        .flat_map(crate::canvas::RecordedPage::ops)
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
            | CanvasCommand::PopTransform => None,
        })
        .collect()
}

fn fonts(rendered: &crate::Rendered) -> Vec<u32> {
    rendered
        .pages()
        .iter()
        .flat_map(crate::canvas::RecordedPage::ops)
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some(run.font.as_u32()),
            CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::StrokePath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::AnnotateLink { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform => None,
        })
        .collect()
}

fn targets(rendered: &crate::Rendered) -> Vec<String> {
    rendered
        .pages()
        .iter()
        .flat_map(crate::canvas::RecordedPage::ops)
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
            | CanvasCommand::PopTransform => None,
        })
        .collect()
}

#[test]
fn adjacent_same_face_runs_paint_as_one_glyph_run() {
    let Some(rendered) = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            children: vec![
                Span::Text {
                    text: "A".into(),
                    font: (),
                    style: RunStyle::default(),
                },
                Span::Text {
                    text: String::new(),
                    font: (),
                    style: RunStyle::default(),
                },
                Span::Text {
                    text: "B".into(),
                    font: (),
                    style: RunStyle::default(),
                },
                Span::Text {
                    text: "C".into(),
                    font: (),
                    style: RunStyle {
                        bold: Some(true),
                        ..RunStyle::default()
                    },
                },
                Span::Text {
                    text: "D".into(),
                    font: (),
                    style: RunStyle {
                        bold: Some(true),
                        ..RunStyle::default()
                    },
                },
                Span::Text {
                    text: "E".into(),
                    font: (),
                    style: RunStyle::default(),
                },
            ],
            ..ParagraphSpec::default()
        })]),
    ) else {
        return;
    };
    let rendered = rendered.unwrap();
    assert_eq!(texts(&rendered), ["AB", "CD", "E"]);
    assert_eq!(fonts(&rendered), [0, 1, 0]);
}

#[test]
fn a_blank_splits_adjacent_text_into_two_glyph_runs() {
    let Some(rendered) = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            children: vec![
                Span::Text {
                    text: "A".into(),
                    font: (),
                    style: RunStyle::default(),
                },
                Span::Blank(BlankSpec {
                    width: BlankWidth::Fixed(NonNegative::pt(10.0).unwrap()),
                    thickness: Some(Positive::pt(0.5).unwrap()),
                    color: Some(Color::rgb(1, 2, 3)),
                    baseline_offset: Some(NonNegative::pt(2.0).unwrap()),
                }),
                Span::Text {
                    text: "B".into(),
                    font: (),
                    style: RunStyle::default(),
                },
            ],
            ..ParagraphSpec::default()
        })]),
    ) else {
        return;
    };
    let rendered = rendered.unwrap();
    assert_eq!(texts(&rendered), ["A", "B"]);
    let pages = rendered.pages();
    let strokes = pages[0]
        .ops()
        .iter()
        .filter(|op| matches!(op, CanvasCommand::StrokePath { .. }))
        .count();
    assert_eq!(strokes, 1);
}

#[test]
fn a_run_after_fill_blank_fails_before_any_page() {
    for runs in [
        vec![
            Span::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            Span::Text {
                text: "x".into(),
                font: (),
                style: RunStyle::default(),
            },
        ],
        vec![
            Span::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            Span::Text {
                text: "x".into(),
                font: (),
                style: RunStyle {
                    bold: Some(true),
                    ..RunStyle::default()
                },
            },
        ],
        vec![
            Span::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            Span::Blank(BlankSpec {
                width: BlankWidth::Fixed(NonNegative::ZERO),
                ..blank_default()
            }),
        ],
        vec![
            Span::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            Span::Blank(BlankSpec {
                width: BlankWidth::Em(1),
                ..blank_default()
            }),
        ],
        vec![
            Span::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            Span::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
        ],
    ] {
        let Some(error) = lay(
            PageSize::A4,
            body(vec![Block::Paragraph(ParagraphSpec {
                children: runs,
                ..ParagraphSpec::default()
            })]),
        ) else {
            return;
        };
        let error = error.err().unwrap();
        assert_eq!(error.first().kind, ErrorKind::RunAfterFillBlank);
    }
}

fn blank_default() -> BlankSpec {
    BlankSpec {
        width: BlankWidth::ToEndOfLine,
        thickness: None,
        color: None,
        baseline_offset: None,
    }
}

#[test]
fn a_fixed_blank_wider_than_the_indented_line_fails() {
    let Some(error) = lay(
        PageSize::custom(Size::try_pt(100.0, 200.0).unwrap()),
        body(vec![Block::Paragraph(ParagraphSpec {
            style: ParagraphStyle {
                first_line_indent: NonNegative::pt(40.0).unwrap(),
                ..ParagraphStyle::default()
            },
            children: vec![Span::Blank(BlankSpec {
                width: BlankWidth::Fixed(NonNegative::pt(80.0).unwrap()),
                ..blank_default()
            })],
        })]),
    ) else {
        return;
    };
    let error = error.err().unwrap();
    let ErrorKind::UnbreakableWiderThanFrame { width, avail, .. } = &error.first().kind else {
        panic!("{error:?}");
    };
    assert!(pt_near(*width, 80.0), "{width}");
    assert!(pt_near(*avail, 60.0), "{avail}");
}

#[test]
fn nested_links_restore_the_outer_target() {
    let Some(rendered) = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            children: vec![
                Span::Link {
                    target: LinkTarget::uri("https://a").unwrap(),
                    children: vec![
                        Span::Text {
                            text: "A".into(),
                            font: (),
                            style: RunStyle::default(),
                        },
                        Span::Text {
                            text: "B".into(),
                            font: (),
                            style: RunStyle::default(),
                        },
                        Span::Text {
                            text: "C".into(),
                            font: (),
                            style: RunStyle {
                                bold: Some(true),
                                ..RunStyle::default()
                            },
                        },
                        Span::Link {
                            target: LinkTarget::uri("https://b").unwrap(),
                            children: vec![Span::Text {
                                text: "D".into(),
                                font: (),
                                style: RunStyle::default(),
                            }],
                        },
                        Span::Text {
                            text: "E".into(),
                            font: (),
                            style: RunStyle::default(),
                        },
                    ],
                },
                Span::Text {
                    text: "F".into(),
                    font: (),
                    style: RunStyle::default(),
                },
            ],
            ..ParagraphSpec::default()
        })]),
    ) else {
        return;
    };
    let rendered = rendered.unwrap();
    assert_eq!(texts(&rendered), ["AB", "C", "D", "E", "F"]);
    assert_eq!(fonts(&rendered), [0, 1, 0, 0, 0]);
    assert_eq!(targets(&rendered), ["https://a", "https://b", "https://a"]);
}

#[test]
fn an_empty_uri_fails_before_any_page() {
    let Some(error) = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            children: vec![
                Span::Text {
                    text: "A".into(),
                    font: (),
                    style: RunStyle::default(),
                },
                Span::Link {
                    target: LinkTarget::Uri(String::new()),
                    children: vec![Span::Text {
                        text: "x".into(),
                        font: (),
                        style: RunStyle::default(),
                    }],
                },
            ],
            ..ParagraphSpec::default()
        })]),
    ) else {
        return;
    };
    let error = error.err().unwrap();
    assert_eq!(error.first().kind, ErrorKind::EmptyLink);
}

#[test]
fn an_empty_band_uri_is_located_in_the_header() {
    let Some(error) = lay(
        PageSize::A4,
        SectionSpec {
            style: SectionStyle {
                margins: Margins::uniform(NonNegative::pt(36.0).unwrap()),
                ..SectionStyle::default()
            },
            header: Some(crate::spec::BandFn::fixed(crate::spec::BandSpec {
                left: Some(crate::spec::BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal("Home".into())],
                    link: Some(vec![PageText::Literal(String::new())]),
                }),
                right: Some(crate::spec::BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal("Later".into())],
                    link: Some(vec![PageText::Literal("https://example.com".into())]),
                }),
                ..crate::spec::BandSpec::default()
            })),
            body: vec![Block::paragraph((), "A")],
            ..SectionSpec::default()
        },
    ) else {
        return;
    };
    let error = error.err().unwrap();
    assert_eq!(error.first().kind, ErrorKind::EmptyLink);
    assert_eq!(
        error.first().at,
        Some(Location {
            section: Some(0),
            page: Some(1),
            region: Some(Region::Header),
            cell: None,
        })
    );
}

/// Not a Unicode scalar that any cmap should map (same idea as measure tests).
const MISSING: char = '\u{FFFF}';

#[test]
fn a_missing_header_glyph_is_located_in_the_header() {
    let Some(error) = lay(
        PageSize::A4,
        SectionSpec {
            style: SectionStyle {
                margins: Margins::uniform(NonNegative::pt(36.0).unwrap()),
                ..SectionStyle::default()
            },
            header: Some(crate::spec::BandFn::fixed(crate::spec::BandSpec {
                left: Some(crate::spec::BandSlot {
                    font: (),
                    style: RunStyle::default(),
                    children: vec![PageText::Literal(MISSING.to_string())],
                    link: None,
                }),
                ..crate::spec::BandSpec::default()
            })),
            body: vec![Block::paragraph((), "A")],
            ..SectionSpec::default()
        },
    ) else {
        return;
    };
    let error = error.err().unwrap();
    assert_eq!(
        error.first().kind,
        ErrorKind::MissingGlyph {
            font: 0,
            scalar: MISSING,
        }
    );
    assert_eq!(
        error.first().at.as_ref().and_then(|at| at.region),
        Some(Region::Header)
    );
}

#[test]
fn an_empty_band_uri_from_fn_is_located_in_the_header() {
    let Some(error) = lay(
        PageSize::A4,
        SectionSpec {
            style: SectionStyle {
                margins: Margins::uniform(NonNegative::pt(36.0).unwrap()),
                ..SectionStyle::default()
            },
            header: Some(crate::spec::BandFn::from_fn(|_| {
                Ok(Some(crate::spec::BandSpec {
                    left: Some(crate::spec::BandSlot {
                        font: (),
                        style: RunStyle::default(),
                        children: vec![PageText::Literal("Home".into())],
                        link: Some(vec![PageText::Literal(String::new())]),
                    }),
                    right: Some(crate::spec::BandSlot {
                        font: (),
                        style: RunStyle::default(),
                        children: vec![PageText::Literal("Later".into())],
                        link: Some(vec![PageText::Literal("https://example.com".into())]),
                    }),
                    ..crate::spec::BandSpec::default()
                }))
            })),
            body: vec![Block::paragraph((), "A")],
            ..SectionSpec::default()
        },
    ) else {
        return;
    };
    let error = error.err().unwrap();
    assert_eq!(error.first().kind, ErrorKind::EmptyLink);
    assert_eq!(
        error.first().at,
        Some(Location {
            section: Some(0),
            page: Some(1),
            region: Some(Region::Header),
            cell: None,
        })
    );
}

#[test]
fn a_missing_header_glyph_from_fn_is_located_in_the_header() {
    let Some(error) = lay(
        PageSize::A4,
        SectionSpec {
            style: SectionStyle {
                margins: Margins::uniform(NonNegative::pt(36.0).unwrap()),
                ..SectionStyle::default()
            },
            header: Some(crate::spec::BandFn::from_fn(|_| {
                Ok(Some(crate::spec::BandSpec {
                    left: Some(crate::spec::BandSlot {
                        font: (),
                        style: RunStyle::default(),
                        children: vec![PageText::Literal(MISSING.to_string())],
                        link: None,
                    }),
                    ..crate::spec::BandSpec::default()
                }))
            })),
            body: vec![Block::paragraph((), "A")],
            ..SectionSpec::default()
        },
    ) else {
        return;
    };
    let error = error.err().unwrap();
    assert_eq!(
        error.first().kind,
        ErrorKind::MissingGlyph {
            font: 0,
            scalar: MISSING,
        }
    );
    assert_eq!(
        error.first().at.as_ref().and_then(|at| at.region),
        Some(Region::Header)
    );
}

#[test]
fn a_zero_length_layer_segment_fails_before_any_page() {
    let point = Point::try_pt(4.0, 5.0).unwrap();
    let error = layout_spec(DocumentSpec {
        page_size: PageSize::custom(Size::try_pt(200.0, 100.0).unwrap()),
        resources: Resources::<()>::default(),
        limits: crate::Limits::default(),
        info: crate::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            behind: vec![
                LayerMark {
                    when: PageWhen::All,
                    child: Mark::Shape(LayerShape::Line {
                        from: point,
                        to: point,
                        pen: Pen::new(Color::rgb(0, 0, 0), 1.0).unwrap(),
                    }),
                },
                LayerMark {
                    when: PageWhen::All,
                    child: Mark::Shape(LayerShape::Rect {
                        rect: Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap(),
                        ink: Ink::Fill(Color::rgb(0, 0, 0)),
                    }),
                },
            ],
            ..SectionSpec::default()
        }],
    })
    .err()
    .unwrap();
    assert_eq!(
        error.first().kind,
        ErrorKind::InvalidGeometry(GeometryKind::ZeroLengthSegment)
    );
    assert_eq!(
        error.first().at.as_ref().and_then(|at| at.region),
        Some(Region::Behind)
    );
}

#[test]
fn a_forced_break_in_layer_text_fails_before_any_page() {
    for sample in ["A\nB", "A\r\nB"] {
        let Some(error) = lay(
            PageSize::A4,
            SectionSpec {
                behind: vec![LayerMark {
                    when: PageWhen::All,
                    child: Mark::Shape(LayerShape::Text {
                        text: vec![PageText::Literal(sample.into())],
                        font: (),
                        at: Point::try_pt(10.0, 10.0).unwrap(),
                        style: RunStyle::default(),
                    }),
                }],
                ..SectionSpec::default()
            },
        ) else {
            return;
        };
        let error = error.err().unwrap();
        assert_eq!(
            error.first().kind,
            ErrorKind::UnexpectedLineBreakInSingleLine {
                text: "A\nB".to_owned(),
            },
            "{sample:?}"
        );
        assert_eq!(
            error.first().at.as_ref().and_then(|at| at.region),
            Some(Region::Behind)
        );
    }
}

#[test]
fn a_forced_break_in_a_custom_marker_fails_before_any_page() {
    let Some(error) = lay(
        PageSize::A4,
        body(vec![Block::List(ListSpec {
            children: vec![ListItemSpec {
                marker: Some("A\nB".into()),
                children: vec![Block::paragraph((), "body")],
            }],
            ..ListSpec::default()
        })]),
    ) else {
        return;
    };
    let error = error.err().unwrap();
    assert_eq!(
        error.first().kind,
        ErrorKind::UnexpectedLineBreakInSingleLine {
            text: "A\nB".to_owned(),
        }
    );
    assert_eq!(
        error.first().at.as_ref().and_then(|at| at.region),
        Some(Region::Body)
    );
}

#[test]
fn a_missing_glyph_on_the_last_page_front_is_located_there() {
    let Some(error) = lay(
        PageSize::A4,
        SectionSpec {
            body: vec![Block::paragraph((), "A")],
            front: vec![LayerMark {
                when: PageWhen::Last,
                child: Mark::Shape(LayerShape::Text {
                    text: vec![PageText::Literal(MISSING.to_string())],
                    font: (),
                    at: Point::try_pt(10.0, 10.0).unwrap(),
                    style: RunStyle::default(),
                }),
            }],
            ..SectionSpec::default()
        },
    ) else {
        return;
    };
    let error = error.err().unwrap();
    assert_eq!(
        error.first().kind,
        ErrorKind::MissingGlyph {
            font: 0,
            scalar: MISSING,
        }
    );
    assert_eq!(
        error.first().at.as_ref().and_then(|at| at.region),
        Some(Region::Front)
    );
}

#[test]
fn keep_together_taller_than_the_page_names_the_body() {
    let Some(error) = lay(
        PageSize::A4,
        body(vec![Block::KeepTogether {
            children: (0..100).map(|_| Block::paragraph((), "line")).collect(),
        }]),
    ) else {
        return;
    };
    let error = error.err().unwrap();
    let page_h = PageSize::A4.resolve().height().as_pt();
    assert!(
        matches!(
            error.first().kind,
            ErrorKind::UnsplittableTallerThanFrame { height, avail }
                if height > 2.0 * page_h && avail.to_bits() == page_h.to_bits()
        ),
        "{error:?}"
    );
    assert_eq!(
        error.first().at,
        Some(Location {
            section: Some(0),
            page: Some(1),
            region: Some(Region::Body),
            cell: None,
        })
    );
}

#[test]
fn one_value_path_paints_a_paragraph_and_a_cell() {
    let Some(rendered) = lay(
        PageSize::A4,
        body(vec![
            Block::paragraph((), "Risk"),
            Block::Table(crate::spec::TableSpec {
                body: vec![crate::spec::TableRow {
                    children: vec![crate::spec::CellSpec {
                        children: vec![Block::paragraph((), "cell")],
                        ..crate::spec::CellSpec::default()
                    }],
                }],
                ..crate::spec::TableSpec::default()
            }),
        ]),
    ) else {
        return;
    };
    let rendered = rendered.unwrap();
    assert_eq!(texts(&rendered), ["Risk", "cell"]);
}

#[test]
fn paragraph_align_end_wins_when_center_is_not_set() {
    let Some(rendered) = lay(
        PageSize::custom(Size::try_pt(120.0, 80.0).unwrap()),
        body(vec![Block::Paragraph(ParagraphSpec {
            style: ParagraphStyle {
                align: Alignment::End,
                ..ParagraphStyle::default()
            },
            children: vec![Span::Text {
                text: "D".into(),
                font: (),
                style: RunStyle::default(),
            }],
        })]),
    ) else {
        return;
    };
    let rendered = rendered.unwrap();
    let pages = rendered.pages();
    let CanvasCommand::DrawGlyphRun { run } = &pages[0].ops()[0] else {
        panic!("glyph");
    };
    let x = run.origin.x().as_pt();
    let width = run
        .glyphs
        .iter()
        .map(|glyph| glyph.advance_norm)
        .sum::<f32>()
        * run.font_size.as_pt();
    assert!(pt_near(x + width, 120.0), "{x}");
}

#[test]
fn an_unregistered_key_names_itself() {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    enum DocFont {
        Body,
    }

    let Err(error) = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::new()),
        limits: crate::Limits::default(),
        info: crate::spec::DocumentInfo::default(),
        sections: vec![SectionSpec {
            body: vec![Block::paragraph(DocFont::Body, "A")],
            ..SectionSpec::default()
        }],
    }) else {
        panic!("unregistered key should fail layout");
    };
    assert_eq!(
        error.first().kind.clone(),
        ErrorKind::UnregisteredFont {
            key: "Body".to_owned(),
        }
    );
    assert_eq!(
        error.first().kind.to_string(),
        "font Body is not registered"
    );
}

#[test]
fn a_bad_collection_index_is_the_font_error() {
    let Err(error) = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::with_fonts(FontRegistry::single(FontFamily {
            regular: FontFace::new(b"not a font".to_vec(), 4),
            bold: None,
            italic: None,
            bold_italic: None,
        })),
        limits: crate::Limits::default(),
        info: crate::spec::DocumentInfo::default(),
        sections: vec![SectionSpec::default()],
    }) else {
        panic!("a bad collection index should fail layout");
    };
    assert!(matches!(
        error.first().kind,
        ErrorKind::Font { index: 4, .. }
    ));
}

fn glyph_ys(rendered: &crate::Rendered) -> Vec<(String, f32)> {
    rendered
        .pages()
        .iter()
        .flat_map(crate::canvas::RecordedPage::ops)
        .filter_map(|op| match op {
            CanvasCommand::DrawGlyphRun { run } => Some((run.text.clone(), run.origin.y().as_pt())),
            CanvasCommand::SetDocumentInfo(_)
            | CanvasCommand::StartPage(_)
            | CanvasCommand::FinishPage
            | CanvasCommand::FillPath { .. }
            | CanvasCommand::StrokePath { .. }
            | CanvasCommand::DrawImage { .. }
            | CanvasCommand::AnnotateLink { .. }
            | CanvasCommand::PushTransform { .. }
            | CanvasCommand::PopTransform => None,
        })
        .collect()
}

fn marker_and_body(blocks: Vec<Block<()>>) -> Option<(u32, u32)> {
    let rendered = lay(
        PageSize::A4,
        body(vec![Block::List(ListSpec {
            style: ListStyle {
                kind: ListKind::Ordered {
                    numbering: Numbering::Decimal,
                    start: std::num::NonZeroU32::MIN,
                    format: MarkerFormat::default(),
                },
                ..ListStyle::default()
            },
            children: vec![ListItemSpec {
                marker: None,
                children: blocks,
            }],
        })]),
    )?;
    let rendered = rendered.expect("styled list lays out");
    let ys = glyph_ys(&rendered);
    let marker = ys
        .iter()
        .find(|row| row.0 == "1.")
        .expect("marker")
        .1
        .to_bits();
    let text = ys
        .iter()
        .find(|row| row.0 == "A")
        .expect("paragraph")
        .1
        .to_bits();
    Some((marker, text))
}

#[test]
fn an_empty_styled_block_is_skipped_for_the_marker() {
    let Some((marker, text)) = marker_and_body(vec![
        Block::Styled {
            style: RunStyle::default(),
            children: Vec::new(),
        },
        Block::paragraph((), "A"),
    ]) else {
        return;
    };
    assert_eq!(marker, text);
}

#[test]
fn a_styled_paragraph_keeps_the_marker_on_its_first_line() {
    let Some((marker, text)) = marker_and_body(vec![Block::Styled {
        style: RunStyle::default(),
        children: vec![Block::paragraph((), "A")],
    }]) else {
        return;
    };
    assert_eq!(marker, text);
}
