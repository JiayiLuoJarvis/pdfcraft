//! Painted and error behavior of values lowered by `layout_spec`.

use crate::canvas::{pt_near, CanvasCommand};
use crate::error::{ErrorKind, GeometryKind, Location, Region};
use crate::fonts::fixture_bytes;
use crate::geom::{Color, Ink, Margins, NonNegative, PageSize, Pen, Point, Positive, Rect, Size};
use crate::layout_spec;
use crate::spec::{
    Alignment, BlankSpec, BlankWidth, Block, InlineRun, LayerMark, LayerShape, ListItemSpec,
    ListSpec, PageWhen, ParagraphSpec, Resources, SectionSpec, TextRun, TextStyle,
};
use crate::DocumentSpec;

fn resources() -> Resources {
    let mut resources = Resources::new(fixture_bytes().as_ref().clone());
    resources.bold_font = Some(fixture_bytes().as_ref().clone());
    resources
}

fn lay(page: PageSize, section: SectionSpec) -> crate::Result<crate::Rendered> {
    layout_spec(DocumentSpec {
        page_size: page,
        resources: resources(),
        limits: crate::Limits::default(),
        sections: vec![section],
    })
}

fn body(blocks: Vec<Block>) -> SectionSpec {
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
            _ => None,
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
            _ => None,
        })
        .collect()
}

fn targets(rendered: &crate::Rendered) -> Vec<String> {
    rendered
        .pages()
        .iter()
        .flat_map(crate::canvas::RecordedPage::ops)
        .filter_map(|op| match op {
            CanvasCommand::AnnotateLink { target, .. } => Some(target.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn adjacent_same_face_runs_paint_as_one_glyph_run() {
    let rendered = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            runs: vec![
                InlineRun::Text("A".into()),
                InlineRun::Text(String::new()),
                InlineRun::Text("B".into()),
                InlineRun::Bold("C".into()),
                InlineRun::Bold("D".into()),
                InlineRun::Text("E".into()),
            ],
            ..ParagraphSpec::default()
        })]),
    )
    .unwrap();
    assert_eq!(texts(&rendered), ["AB", "CD", "E"]);
    assert_eq!(fonts(&rendered), [0, 1, 0]);
}

#[test]
fn a_blank_splits_adjacent_text_into_two_glyph_runs() {
    let rendered = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            runs: vec![
                InlineRun::Text("A".into()),
                InlineRun::Blank(BlankSpec {
                    width: BlankWidth::Fixed(NonNegative::pt(10.0).unwrap()),
                    thickness: Some(Positive::pt(0.5).unwrap()),
                    color: Some(Color::rgb(1, 2, 3)),
                    baseline_offset: Some(NonNegative::pt(2.0).unwrap()),
                }),
                InlineRun::Text("B".into()),
            ],
            ..ParagraphSpec::default()
        })]),
    )
    .unwrap();
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
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            InlineRun::Text("x".into()),
        ],
        vec![
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            InlineRun::Bold("x".into()),
        ],
        vec![
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::Fixed(NonNegative::ZERO),
                ..blank_default()
            }),
        ],
        vec![
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::Em(1),
                ..blank_default()
            }),
        ],
        vec![
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
            InlineRun::Blank(BlankSpec {
                width: BlankWidth::ToEndOfLine,
                ..blank_default()
            }),
        ],
    ] {
        let error = lay(
            PageSize::A4,
            body(vec![Block::Paragraph(ParagraphSpec {
                runs,
                ..ParagraphSpec::default()
            })]),
        )
        .err()
        .unwrap();
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
    let error = lay(
        PageSize::custom(Size::try_pt(100.0, 200.0).unwrap()),
        body(vec![Block::Paragraph(ParagraphSpec {
            runs: vec![InlineRun::Blank(BlankSpec {
                width: BlankWidth::Fixed(NonNegative::pt(80.0).unwrap()),
                ..blank_default()
            })],
            first_line_indent: NonNegative::pt(40.0).unwrap(),
            ..ParagraphSpec::default()
        })]),
    )
    .err()
    .unwrap();
    let ErrorKind::UnbreakableWiderThanFrame { width, avail, .. } = &error.first().kind else {
        panic!("{error:?}");
    };
    assert!(pt_near(*width, 80.0), "{width}");
    assert!(pt_near(*avail, 60.0), "{avail}");
}

#[test]
fn nested_links_restore_the_outer_target() {
    let rendered = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            runs: vec![
                InlineRun::Link {
                    uri: "https://a".into(),
                    runs: vec![
                        InlineRun::Text("A".into()),
                        InlineRun::Text("B".into()),
                        InlineRun::Bold("C".into()),
                        InlineRun::Link {
                            uri: "https://b".into(),
                            runs: vec![InlineRun::Text("D".into())],
                        },
                        InlineRun::Text("E".into()),
                    ],
                },
                InlineRun::Text("F".into()),
            ],
            ..ParagraphSpec::default()
        })]),
    )
    .unwrap();
    assert_eq!(texts(&rendered), ["AB", "C", "D", "E", "F"]);
    assert_eq!(fonts(&rendered), [0, 1, 0, 0, 0]);
    assert_eq!(targets(&rendered), ["https://a", "https://b", "https://a"]);
}

#[test]
fn an_empty_uri_fails_before_any_page() {
    let error = lay(
        PageSize::A4,
        body(vec![Block::Paragraph(ParagraphSpec {
            runs: vec![
                InlineRun::Text("A".into()),
                InlineRun::Link {
                    uri: String::new(),
                    runs: vec![InlineRun::Text("x".into())],
                },
            ],
            ..ParagraphSpec::default()
        })]),
    )
    .err()
    .unwrap();
    assert_eq!(error.first().kind, ErrorKind::EmptyLink);
}

#[test]
fn an_empty_band_uri_is_located_in_the_header() {
    let error = lay(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(NonNegative::pt(36.0).unwrap()),
            header: Some(crate::spec::BandSpec {
                left: Some(crate::spec::BandSlot::Link {
                    uri: vec![TextRun::Literal(String::new())],
                    text: vec![TextRun::Literal("Home".into())],
                }),
                right: Some(crate::spec::BandSlot::Link {
                    uri: vec![TextRun::Literal("https://example.com".into())],
                    text: vec![TextRun::Literal("Later".into())],
                }),
                ..crate::spec::BandSpec::default()
            }),
            body: vec![Block::paragraph("A")],
            ..SectionSpec::default()
        },
    )
    .err()
    .unwrap();
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

const MISSING: char = '\u{6f22}';

#[test]
fn a_missing_header_glyph_is_located_in_the_header() {
    let error = lay(
        PageSize::A4,
        SectionSpec {
            margins: Margins::uniform(NonNegative::pt(36.0).unwrap()),
            header: Some(crate::spec::BandSpec {
                left: Some(crate::spec::BandSlot::Text(vec![TextRun::Literal(
                    MISSING.to_string(),
                )])),
                ..crate::spec::BandSpec::default()
            }),
            body: vec![Block::paragraph("A")],
            ..SectionSpec::default()
        },
    )
    .err()
    .unwrap();
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
        resources: Resources::default(),
        limits: crate::Limits::default(),
        sections: vec![SectionSpec {
            behind: vec![
                LayerMark::Line {
                    when: PageWhen::All,
                    from: point,
                    to: point,
                    pen: Pen::new(Color::rgb(0, 0, 0), 1.0).unwrap(),
                },
                LayerMark::Rect {
                    when: PageWhen::All,
                    rect: Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap(),
                    ink: Ink::Fill(Color::rgb(0, 0, 0)),
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
fn a_non_finite_rotation_fails_before_any_page() {
    let error = layout_spec(DocumentSpec {
        page_size: PageSize::custom(Size::try_pt(200.0, 100.0).unwrap()),
        resources: Resources::default(),
        limits: crate::Limits::default(),
        sections: vec![SectionSpec {
            behind: vec![LayerMark::Rotated {
                when: PageWhen::All,
                degrees: f32::NAN,
                pivot: Point::ZERO,
                shapes: vec![LayerShape::Rect {
                    rect: Rect::try_pt(0.0, 0.0, 10.0, 10.0).unwrap(),
                    ink: Ink::Fill(Color::rgb(0, 0, 0)),
                }],
            }],
            ..SectionSpec::default()
        }],
    })
    .err()
    .unwrap();
    assert!(
        matches!(
            error.first().kind,
            ErrorKind::InvalidGeometry(GeometryKind::NonFiniteRotation { degrees }) if degrees.is_nan()
        ),
        "{error:?}"
    );
}

#[test]
fn a_forced_break_in_layer_text_fails_before_any_page() {
    for sample in ["A\nB", "A\r\nB"] {
        let error = lay(
            PageSize::A4,
            SectionSpec {
                behind: vec![LayerMark::Text {
                    when: PageWhen::All,
                    text: vec![TextRun::Literal(sample.into())],
                    at: Point::try_pt(10.0, 10.0).unwrap(),
                    style: TextStyle::default(),
                }],
                ..SectionSpec::default()
            },
        )
        .err()
        .unwrap();
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
    let error = lay(
        PageSize::A4,
        body(vec![Block::List(ListSpec {
            items: vec![ListItemSpec {
                marker: Some("A\nB".into()),
                blocks: vec![Block::paragraph("body")],
            }],
            ..ListSpec::default()
        })]),
    )
    .err()
    .unwrap();
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
    let error = lay(
        PageSize::A4,
        SectionSpec {
            body: vec![Block::paragraph("A")],
            front: vec![LayerMark::Text {
                when: PageWhen::Last,
                text: vec![TextRun::Literal(MISSING.to_string())],
                at: Point::try_pt(10.0, 10.0).unwrap(),
                style: TextStyle::default(),
            }],
            ..SectionSpec::default()
        },
    )
    .err()
    .unwrap();
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
    let error = lay(
        PageSize::A4,
        body(vec![Block::KeepTogether(
            (0..100).map(|_| Block::paragraph("line")).collect(),
        )]),
    )
    .err()
    .unwrap();
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
    let rendered = lay(
        PageSize::A4,
        body(vec![
            Block::paragraph("Risk"),
            Block::Table(crate::spec::TableSpec {
                body: vec![vec![crate::spec::CellSpec {
                    content: vec![crate::spec::CellBlock::Text("cell".into())],
                    ..crate::spec::CellSpec::default()
                }]],
                ..crate::spec::TableSpec::default()
            }),
        ]),
    )
    .unwrap();
    assert_eq!(texts(&rendered), ["Risk", "cell"]);
}

#[test]
fn paragraph_align_end_wins_when_center_is_not_set() {
    let rendered = lay(
        PageSize::custom(Size::try_pt(120.0, 80.0).unwrap()),
        body(vec![Block::Paragraph(ParagraphSpec {
            runs: vec![InlineRun::Text("D".into())],
            align: Alignment::End,
            ..ParagraphSpec::default()
        })]),
    )
    .unwrap();
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
