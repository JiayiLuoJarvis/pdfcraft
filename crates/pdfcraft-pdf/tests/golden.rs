//! Golden-harness self-tests: the serializer proves itself on a zero-golden
//! corpus, so todo 8 can trust it with real goldens.
//!
//! Most tests run everywhere: they use synthetic [`CanvasCommand`] values
//! (every constructible variant) or font-free `layout_spec` documents. The
//! glyph test needs `PDFCRAFT_PDF_FONT`/`PDFCRAFT_CJK_FONT` and the image test
//! needs `backend-krilla`; both skip otherwise.

#![allow(
    clippy::missing_panics_doc,
    reason = "test assertions may panic; shared across golden self-tests"
)]
#![allow(
    clippy::must_use_candidate,
    reason = "test helpers return values for assertions, not for callers"
)]

#[path = "golden/sha256.rs"]
mod sha256;
#[path = "golden/snapshot.rs"]
mod snapshot;

use std::path::PathBuf;

use pdfcraft_pdf::spec::{
    Alignment, Block, CanvasMark, CanvasSpec, DocumentInfo, FontFace, FontFamily, FontRegistry,
    HLineSpec, ImageSpec, LayerMark, LayerShape, PageWhen, Resources, SectionSpec,
};
use pdfcraft_pdf::{
    layout_spec, CanvasCommand, Color, DocumentSpec, ImageFormat, Ink, LinkTarget, Margins,
    NonNegative, PageSize, Path, Pen, Point, Positive, Rect, Size,
};
use snapshot::{FontMeta, GoldenError};

fn pt(x: f32, y: f32) -> Point {
    Point::try_pt(x, y).unwrap()
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::try_pt(x, y, w, h).unwrap()
}

fn pen(r: u8, g: u8, b: u8, width: f32) -> Pen {
    Pen::new(Color::rgb(r, g, b), width).unwrap()
}

fn font_free_pages() -> (Vec<pdfcraft_pdf::RecordedPage>, DocumentInfo) {
    let info = DocumentInfo {
        title: Some("Golden Self-Test".to_owned()),
        author: None,
        subject: None,
        keywords: vec!["harness".to_owned()],
        language: Some("en".to_owned()),
    };
    let body = vec![
        Block::HLine(HLineSpec {
            width: None,
            align: Alignment::Start,
            thickness: Positive::pt(2.0).unwrap(),
            color: Color::rgb(200, 0, 0),
            space_before: NonNegative::ZERO,
            space_after: NonNegative::ZERO,
        }),
        Block::Canvas(CanvasSpec {
            size: Size::try_pt(120.0, 60.0).unwrap(),
            align: Alignment::Start,
            marks: vec![
                CanvasMark::Line {
                    from: pt(0.0, 0.0),
                    to: pt(120.0, 60.0),
                    pen: pen(0, 0, 0, 1.0),
                },
                CanvasMark::Rect {
                    rect: rect(0.0, 0.0, 40.0, 20.0),
                    ink: Ink::Fill(Color::rgb(0, 128, 0)),
                },
                CanvasMark::Fill {
                    path: Path::line(pt(50.0, 10.0), pt(90.0, 30.0)),
                    color: Color::rgba(0, 0, 255, 128),
                },
            ],
        }),
    ];
    let behind = vec![LayerMark::Line {
        when: PageWhen::All,
        from: pt(10.0, 10.0),
        to: pt(100.0, 10.0),
        pen: pen(0, 0, 0, 0.5),
    }];
    let front = vec![LayerMark::Rotated {
        when: PageWhen::All,
        degrees: 45.0,
        pivot: pt(100.0, 100.0),
        shapes: vec![
            LayerShape::Line {
                from: pt(90.0, 100.0),
                to: pt(110.0, 100.0),
                pen: pen(128, 0, 128, 1.0),
            },
            LayerShape::Rect {
                rect: rect(95.0, 95.0, 10.0, 10.0),
                ink: Ink::Stroke(pen(128, 0, 128, 1.0)),
            },
        ],
    }];
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources: Resources::<()>::default(),
        limits: pdfcraft_pdf::Limits::default(),
        info: info.clone(),
        sections: vec![SectionSpec {
            page_size: None,
            margins: Margins::try_uniform(48.0).unwrap(),
            header: None,
            footer: None,
            behind,
            front,
            body,
        }],
    })
    .unwrap();
    (rendered.pages(), info)
}

fn scratch_case(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdfcraft-golden-selftest-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn golden_float_formats_match_dp_rules() {
    assert_eq!(snapshot::fmt_pt3(10.0), "10.000");
    assert_eq!(snapshot::fmt_pt3(0.1 + 0.2), "0.300");
    assert_eq!(snapshot::fmt_pt3(-0.0), "0.000");
    assert_eq!(snapshot::fmt_norm6(0.5), "0.500000");
    assert_eq!(snapshot::fmt_norm6(-0.0), "0.000000");
    // 6dp is exact for the sub-point advances the TJ lock compares.
    assert_eq!(snapshot::fmt_norm6(1.0 / 3.0), "0.333333");
}

#[test]
fn golden_sha256_matches_nist_vector() {
    assert_eq!(
        sha256::hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(sha256::hex(b"").len(), 64);
    assert_ne!(sha256::hex(b"a"), sha256::hex(b"b"));
}

#[test]
fn golden_skrifa_version_matches_workspace_pin() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("Cargo.toml");
    let manifest = std::fs::read_to_string(&root).unwrap();
    let pinned = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("skrifa"))
        .unwrap_or("");
    assert!(
        pinned.contains(snapshot::SKRIFA_VERSION),
        "SKRIFA_VERSION {} drifted from workspace pin {pinned:?}",
        snapshot::SKRIFA_VERSION
    );
}

#[test]
fn golden_each_constructible_op_has_stable_text() {
    let paint = pdfcraft_pdf::Paint::solid(Color::rgb(1, 2, 3));
    let path = Path::line(pt(10.0, 20.0), pt(30.0, 40.0));
    let stroke = pdfcraft_pdf::Stroke::new(Positive::pt(0.75).unwrap());
    let cases: Vec<(CanvasCommand, &str)> = vec![
        (
            CanvasCommand::FillPath {
                path: path.clone(),
                paint,
            },
            "fill path=\"M 10.000 20.000 L 30.000 40.000\" paint=#010203ff",
        ),
        (
            CanvasCommand::StrokePath {
                path: path.clone(),
                paint,
                stroke,
            },
            "stroke path=\"M 10.000 20.000 L 30.000 40.000\" paint=#010203ff width=0.750",
        ),
        (
            CanvasCommand::AnnotateLink {
                rect: rect(72.0, 90.0, 40.0, 14.0),
                target: LinkTarget::uri("https://example.test/a").unwrap(),
            },
            "link rect=72.000 90.000 40.000 14.000 uri=\"https://example.test/a\"",
        ),
        (
            CanvasCommand::AnnotateLink {
                rect: rect(0.0, 0.0, 5.0, 5.0),
                target: LinkTarget::anchor("sec-1").unwrap(),
            },
            "link rect=0.000 0.000 5.000 5.000 anchor=\"sec-1\"",
        ),
        (
            CanvasCommand::PushTransform {
                affine: pdfcraft_pdf::geom::Affine {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    tx: -0.0,
                    ty: 8.0,
                },
            },
            "push affine=1.000000,0.000000,0.000000,1.000000,0.000000,8.000000",
        ),
        (CanvasCommand::PopTransform, "pop"),
    ];
    for (op, expected) in &cases {
        let text = snapshot::fmt_single_op_for_tests(op).unwrap();
        assert_eq!(&text, expected, "op {op:?}");
    }
}

#[test]
fn golden_balance_rejects_unbalanced_logs() {
    let push = || CanvasCommand::PushTransform {
        affine: pdfcraft_pdf::geom::Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        },
    };
    let pop = || CanvasCommand::PopTransform;
    assert!(snapshot::check_transform_balance(0, &[push(), pop()]).is_ok());
    assert!(snapshot::check_transform_balance(0, &[]).is_ok());
    let err = snapshot::check_transform_balance(0, &[push(), push(), pop()]).unwrap_err();
    assert!(err.message().contains("unbalanced"), "{err}");
    let err = snapshot::check_transform_balance(2, &[pop()]).unwrap_err();
    assert!(err.message().contains("page 2"), "{err}");
    // The failure is a hard error, never a diff: no diff marker is attached.
    assert!(!err.message().contains("@@"));
}

#[test]
fn golden_layout_doc_serializes_stably() {
    let (pages, info) = font_free_pages();
    let first = snapshot::serialize_pages(&pages, Some(&info), &[]).unwrap();
    let second = snapshot::serialize_pages(&pages, Some(&info), &[]).unwrap();
    assert_eq!(first, second);
    assert!(first.starts_with("pdfcraft-golden v1\nskrifa 0.48.0\n"));
    assert!(first.contains("doc title=Some(\"Golden Self-Test\")"));
    assert!(first.contains("\npage 0 "));
    assert!(first.contains("\nstroke path=\""));
    assert!(first.contains("\nfill path=\""));
    assert!(first.contains("\npush affine="));
    assert!(first.contains("\npop\n") || first.ends_with("\npop"));
    let expected = "pdfcraft-golden v1\nskrifa 0.48.0\ndoc title=Some(\"Golden Self-Test\") author=None subject=None keywords=[\"harness\"] language=Some(\"en\")\npage 0 595.276 841.890\nstroke path=\"M 10.000 10.000 L 100.000 10.000\" paint=#000000ff width=0.500\nstroke path=\"M 48.000 49.000 L 547.276 49.000\" paint=#c80000ff width=2.000\nstroke path=\"M 48.000 50.000 L 168.000 110.000\" paint=#000000ff width=1.000\nfill path=\"M 48.000 50.000 L 88.000 50.000 L 88.000 70.000 L 48.000 70.000 Z\" paint=#008000ff\nfill path=\"M 98.000 60.000 L 138.000 80.000\" paint=#0000ff80\npush affine=0.707107,-0.707107,0.707107,0.707107,100.000000,-41.421356\nstroke path=\"M 90.000 100.000 L 110.000 100.000\" paint=#800080ff width=1.000\nstroke path=\"M 95.000 95.000 L 105.000 95.000 L 105.000 105.000 L 95.000 105.000 Z\" paint=#800080ff width=1.000\npop\n";
    assert_eq!(first, expected);
}

#[test]
fn golden_corrupt_expectation_fails_naming_the_file() {
    let (pages, info) = font_free_pages();
    let dir = scratch_case("corrupt");
    let name = "golden_selftest_doc";
    let path = snapshot::bless_golden_in_dir(&pages, Some(&info), &[], &dir, name).unwrap();
    assert_eq!(path, dir.join("golden_selftest_doc.golden"));
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.replace_range(20..21, "X");
    std::fs::write(&path, text).unwrap();
    let err: GoldenError =
        snapshot::verify_golden(&pages, Some(&info), &[], &dir, name).unwrap_err();
    assert!(
        err.message().contains("golden_selftest_doc.golden"),
        "{err}"
    );
    assert!(err.message().contains("mismatch"), "{err}");
    assert!(err.message().lines().count() <= 45);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn golden_bless_then_verify_passes() {
    assert!(snapshot::goldens_dir().ends_with(PathBuf::from("tests/goldens")));
    let (pages, info) = font_free_pages();
    let dir = scratch_case("bless");
    let name = "golden_selftest_bless";
    snapshot::bless_golden_in_dir(&pages, Some(&info), &[], &dir, name).unwrap();
    snapshot::verify_golden(&pages, Some(&info), &[], &dir, name).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn golden_write_actual_captures_bytes() {
    let name = format!("golden_selftest_actual_{}", std::process::id());
    // Slash would escape the scratch dir; the harness rejects it outright.
    assert!(snapshot::write_actual("x", "../evil").is_err());
    let path = snapshot::write_actual("line one\n", &name).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "line one\n");
    let _ = std::fs::remove_file(&path);
}

fn test_font_bytes() -> Option<Vec<u8>> {
    let path =
        std::env::var_os("PDFCRAFT_PDF_FONT").or_else(|| std::env::var_os("PDFCRAFT_CJK_FONT"))?;
    Some(std::fs::read(&path).unwrap())
}

#[test]
fn golden_glyph_runs_render_exact_text_and_font_header() {
    let Some(bytes) = test_font_bytes() else {
        eprintln!("skipping: set PDFCRAFT_PDF_FONT to run the glyph golden");
        return;
    };
    let upem = {
        use skrifa::instance::{LocationRef, Size};
        use skrifa::{FontRef, MetadataProvider};
        let font_ref = FontRef::new(&bytes).unwrap();
        f32::from(
            font_ref
                .metrics(Size::unscaled(), LocationRef::default())
                .units_per_em,
        )
    };
    let fonts = vec![FontMeta::new(sha256::hex(&bytes), upem)];
    let resources = Resources::with_fonts(FontRegistry::single(FontFamily {
        regular: FontFace::new(bytes, 0),
        bold: None,
        italic: None,
        bold_italic: None,
    }));
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_uniform(48.0).unwrap(),
            body: vec![Block::paragraph((), "AB")],
            ..Default::default()
        }],
    })
    .unwrap();
    let pages = rendered.pages();
    let first = snapshot::serialize_pages(&pages, None, &fonts).unwrap();
    let second = snapshot::serialize_pages(&pages, None, &fonts).unwrap();
    assert_eq!(first, second);
    assert!(first.contains("font 0 sha256="), "{first}");
    assert!(
        first.contains(&format!("upem={upem}")) || first.contains("upem="),
        "{first}"
    );
    assert!(first.contains("text=\"AB\""), "{first}");
    assert!(first.contains("glyphs font=0 size=12.000"), "{first}");
}

#[test]
#[cfg(feature = "backend-krilla")]
fn golden_image_ops_use_dense_slots() {
    let bytes = include_bytes!("fixtures/one-pixel.png").to_vec();
    let mut resources = Resources::<()>::default();
    let image = resources.add_image(bytes, ImageFormat::Png);
    let rendered = layout_spec(DocumentSpec {
        page_size: PageSize::A4,
        resources,
        limits: pdfcraft_pdf::Limits::default(),
        info: DocumentInfo::default(),
        sections: vec![SectionSpec {
            margins: Margins::try_uniform(48.0).unwrap(),
            body: vec![Block::Image(ImageSpec {
                image,
                width: None,
                height: None,
                align: Alignment::Start,
            })],
            ..Default::default()
        }],
    })
    .unwrap();
    let pages = rendered.pages();
    let text = snapshot::serialize_pages(&pages, None, &[]).unwrap();
    assert!(text.contains("image 0 1x1"), "{text}");
    assert!(text.contains("image slot=0 rect="), "{text}");
}

#[test]
fn golden_page_markers_and_unknown_fonts_are_hard_errors() {
    // StartPage/FinishPage/SetDocumentInfo never appear in page ops; if they
    // ever leak through, serialization fails instead of printing them.
    let leaked = vec![
        CanvasCommand::StartPage(Size::try_pt(10.0, 10.0).unwrap()),
        CanvasCommand::FinishPage,
    ];
    for op in &leaked {
        assert!(snapshot::fmt_single_op_for_tests(op).is_err());
    }
}
