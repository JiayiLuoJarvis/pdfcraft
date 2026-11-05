//! `Form` and `Fill` against the copied `AcroForm` fixtures.

#![expect(
    clippy::expect_used,
    reason = "integration test: a panic is the failure report"
)]
#![expect(
    clippy::unwrap_used,
    reason = "integration test: a panic is the failure report"
)]

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use pdfcraft_forms::{
    AbsoluteLine, ArgumentReason, Error, ErrorKind, FieldFill, FieldRule, FieldStyle, FieldType,
    FieldValue, Fill, FillRequest, FillStyle, Font, FontReason, Form, HAlign, MissingName,
    StrokeStyle, VAlign,
};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn font_bytes() -> Option<Vec<u8>> {
    let path = std::env::var_os("PDFCRAFT_FORMS_FONT")?;
    Some(fs::read(path).expect("read PDFCRAFT_FORMS_FONT"))
}

fn load_font(bytes: Vec<u8>) -> Font {
    if bytes.starts_with(b"ttcf") {
        Font::from_collection(bytes, 0).unwrap()
    } else {
        Font::new(bytes).unwrap()
    }
}

fn basic_request() -> FillRequest {
    FillRequest::new(BTreeMap::from([
        ("employee_name".to_owned(), FieldFill::text("Ada")),
        ("agree".to_owned(), FieldFill::check(true)),
        ("level".to_owned(), FieldFill::text("Senior")),
    ]))
}

fn draw(pdf: &[u8], request: FillRequest, font: Option<&Font>) -> Result<Vec<u8>, Error> {
    let form = Form::open(pdf)?;
    Fill::new(&form, request, font)?.to_pdf()
}

fn filled_basic() -> Option<Vec<u8>> {
    let font = load_font(font_bytes()?);
    Some(draw(&fixture("basic_form.pdf"), basic_request(), Some(&font)).unwrap())
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn form_fill_font_and_error_are_send_and_sync() {
    assert_send_sync::<Form>();
    assert_send_sync::<Fill>();
    assert_send_sync::<Font>();
    assert_send_sync::<Error>();
}

#[test]
fn fills_text_checkbox_and_choice_then_strips_the_form() {
    let Some(pdf) = filled_basic() else {
        return;
    };
    assert!(pdf.starts_with(b"%PDF"));
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    let catalog = doc.catalog().unwrap();
    assert!(catalog.get(b"AcroForm").is_err());
    assert!(Form::open(&pdf).unwrap().fields().is_empty());
    let text = pdftotext(&pdf);
    assert!(text.contains("Ada"), "{text:?}");
    assert!(text.contains("Senior"), "{text:?}");
    assert!(has_agree_mark(&doc), "{}", page_text(&doc));
    assert_subset_head_times_are_zero(&pdf);
}

#[test]
fn empty_map_strips_widgets() {
    let pdf = draw(&fixture("basic_form.pdf"), FillRequest::default(), None).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    assert!(doc.catalog().unwrap().get(b"AcroForm").is_err());
    assert!(Form::open(&pdf).unwrap().fields().is_empty());
    assert!(!has_agree_mark(&doc), "{}", page_text(&doc));
}

#[test]
fn checkbox_only_fill_ignores_the_font() {
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let request = FillRequest::new(BTreeMap::from([(
        "agree".to_owned(),
        FieldFill::check(true),
    )]));
    let bare = Fill::new(&form, request.clone(), None)
        .unwrap()
        .to_pdf()
        .unwrap();
    assert!(!bare
        .windows(b"/FontFile2".len())
        .any(|window| window == b"/FontFile2"));
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let with_font = Fill::new(&form, request, Some(&font))
        .unwrap()
        .to_pdf()
        .unwrap();
    assert_eq!(bare, with_font);
}

#[test]
fn unchecked_box_draws_no_mark() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest::new(BTreeMap::from([(
        "agree".to_owned(),
        FieldFill::check(false),
    )]));
    let pdf = draw(&fixture("basic_form.pdf"), request, Some(&font)).unwrap();
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    assert!(!has_agree_mark(&doc), "{}", page_text(&doc));
}

#[test]
fn two_fills_match_byte_for_byte() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let before = form.fields().to_vec();
    let font = load_font(bytes.clone());
    let fill = Fill::new(&form, basic_request(), Some(&font)).unwrap();
    let first = fill.to_pdf().unwrap();
    let second = fill.to_pdf().unwrap();
    assert_eq!(first, second);
    assert_eq!(form.fields(), before.as_slice());
    let other_font = load_font(bytes);
    let again = Fill::new(&form, basic_request(), Some(&other_font))
        .unwrap()
        .to_pdf()
        .unwrap();
    assert_eq!(first, again);
    let checks = FillRequest::new(BTreeMap::from([(
        "agree".to_owned(),
        FieldFill::check(true),
    )]));
    let from_shared = Fill::new(&form, checks.clone(), Some(&font))
        .unwrap()
        .to_pdf()
        .unwrap();
    let from_new = Fill::new(&form, checks, Some(&other_font))
        .unwrap()
        .to_pdf()
        .unwrap();
    assert_eq!(from_shared, from_new);
}

#[test]
fn rejects_illegal_choice() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest::new(BTreeMap::from([(
        "level".to_owned(),
        FieldFill::text("Nope"),
    )]));
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(&form, request, Some(&font)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::IllegalChoice);
    assert_eq!(err.field(), Some("level"));
}

#[test]
fn rejects_a_checkbox_value_on_a_text_widget() {
    let request = FillRequest::new(BTreeMap::from([(
        "employee_name".to_owned(),
        FieldFill::check(true),
    )]));
    let err = draw(&fixture("basic_form.pdf"), request, None).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::InvalidArgument(ArgumentReason::CheckboxOnText)
    );
    assert_eq!(err.field(), Some("employee_name"));
}

#[test]
fn rejects_any_value_on_a_radio_widget() {
    let pdf = radio_widget_pdf();
    for value in [FieldFill::check(true), FieldFill::text("Ada")] {
        let request = FillRequest::new(BTreeMap::from([("radio".to_owned(), value)]));
        let err = draw(&pdf, request, None).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::WidgetUnsupported);
        assert_eq!(err.field(), Some("radio"));
    }
}

#[test]
fn rejects_text_on_a_checkbox() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest::new(BTreeMap::from([(
        "agree".to_owned(),
        FieldFill::text("yes"),
    )]));
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(&form, request, Some(&font)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidCheckbox);
    assert_eq!(err.field(), Some("agree"));
}

#[test]
fn rejects_a_missing_font_when_text_is_drawn() {
    let request = FillRequest::new(BTreeMap::from([(
        "employee_name".to_owned(),
        FieldFill::text("Ada"),
    )]));
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(&form, request, None).unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::FontNotConfigured(FontReason::Missing)
    );
    assert_eq!(err.field(), None);
    assert!(std::error::Error::source(&err).is_none());
}

#[test]
fn rejects_unusable_font_bytes() {
    let cases = [
        (b"ttcf".as_slice(), FontReason::Collection),
        (b"not a font".as_slice(), FontReason::Head),
    ];
    for (bytes, reason) in cases {
        let err = Font::new(bytes.to_vec()).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::FontNotConfigured(reason));
        assert!(std::error::Error::source(&err).is_none());
    }
}

#[test]
fn rejects_a_font_size_that_is_not_positive() {
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    for value in [0.0_f32, -2.5] {
        let request = FillRequest {
            style: FillStyle {
                font_size: value,
                ..FillStyle::default()
            },
            ..FillRequest::default()
        };
        let err = Fill::new(&form, request, None).unwrap_err();
        assert_eq!(
            err.kind(),
            ErrorKind::InvalidArgument(ArgumentReason::FontSize { value })
        );
        assert_eq!(err.field(), None);
    }
    let request = FillRequest {
        style: FillStyle {
            font_size: f32::NAN,
            ..FillStyle::default()
        },
        ..FillRequest::default()
    };
    let err = Fill::new(&form, request, None).unwrap_err();
    match err.kind() {
        ErrorKind::InvalidArgument(ArgumentReason::FontSize { value }) => {
            assert!(!value.is_finite());
        }
        ErrorKind::InvalidArgument(_) | _ => panic!("unexpected {}", err.kind()),
    }
    assert_eq!(err.field(), None);
}

#[test]
fn bad_pdf_is_rejected_before_an_illegal_font_size() {
    let err = Form::open(b"not a pdf").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotAPdf);
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    for value in [1000.0_f32, 1.0e20] {
        let request = FillRequest {
            style: FillStyle {
                font_size: value,
                ..FillStyle::default()
            },
            ..FillRequest::default()
        };
        let size_err = Fill::new(&form, request, None).unwrap_err();
        match size_err.kind() {
            ErrorKind::InvalidArgument(ArgumentReason::FontSize { value: got }) => {
                assert_eq!(got.to_bits(), value.to_bits());
            }
            ErrorKind::InvalidArgument(_) | _ => panic!("unexpected {}", size_err.kind()),
        }
        assert_eq!(size_err.field(), None);
    }
    for value in [256.0_f32, 0.0001] {
        let request = FillRequest {
            style: FillStyle {
                font_size: value,
                ..FillStyle::default()
            },
            ..FillRequest::default()
        };
        let _filled = Fill::new(&form, request, None).unwrap();
    }
}

#[test]
fn rejects_a_missing_glyph() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest::new(BTreeMap::from([(
        "employee_name".to_owned(),
        FieldFill::text("王"),
    )]));
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(&form, request, Some(&font)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::MissingGlyph);
    assert_eq!(err.field(), Some("employee_name"));
    assert_eq!(err.scalar(), Some('王'));
}

#[test]
fn a_non_finite_line_wins_over_a_missing_glyph() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let mut line = AbsoluteLine::new(10.0, 20.0, 30.0, 20.0);
    line.x1 = f32::NAN;
    let request = FillRequest {
        lines: vec![line],
        ..FillRequest::new(BTreeMap::from([(
            "employee_name".to_owned(),
            FieldFill::text("王"),
        )]))
    };
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(&form, request, Some(&font)).unwrap_err();
    match err.kind() {
        ErrorKind::InvalidArgument(ArgumentReason::NonFinite { value }) => {
            assert!(value.is_nan());
        }
        ErrorKind::InvalidArgument(_) | _ => panic!("unexpected {}", err.kind()),
    }
    assert_eq!(err.field(), None);
    assert_eq!(err.scalar(), None);
}

#[test]
fn rejects_text_that_cannot_fit() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest::new(BTreeMap::from([(
        "employee_name".to_owned(),
        FieldFill::text("x".repeat(500)),
    )]));
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(&form, request, Some(&font)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::BoxOverflow);
    assert_eq!(err.field(), Some("employee_name"));
}

#[test]
fn rejects_filling_a_signature() {
    let request = FillRequest::new(BTreeMap::from([(
        "signature".to_owned(),
        FieldFill::text("Ada"),
    )]));
    let err = draw(&fixture("signature_form.pdf"), request, None).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::SignatureNotWritable);
    assert_eq!(err.field(), Some("signature"));
}

#[test]
fn rejects_an_unknown_name() {
    let request = FillRequest::new(BTreeMap::from([(
        "missing".to_owned(),
        FieldFill::check(true),
    )]));
    let err = draw(&fixture("basic_form.pdf"), request, None).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnknownField);
    assert_eq!(err.field(), Some("missing"));
}

#[test]
fn rejects_an_unknown_field_that_also_sets_alignment() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let mut fields = basic_request().fields;
    fields.insert(
        "nope".to_owned(),
        FieldFill {
            style: FieldStyle {
                h_align: Some(HAlign::Center),
                ..FieldStyle::default()
            },
            ..FieldFill::text("x")
        },
    );
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(&form, FillRequest::new(fields), Some(&font)).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnknownField);
    assert_eq!(err.field(), Some("nope"));
}

#[test]
fn ignores_an_unknown_name_when_missing_is_ignore() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let mut fields = basic_request().fields;
    fields.insert(
        "nope".to_owned(),
        FieldFill {
            style: FieldStyle {
                v_align: Some(VAlign::Top),
                ..FieldStyle::default()
            },
            ..FieldFill::text("x")
        },
    );
    let request = FillRequest {
        missing: MissingName::Ignore,
        ..FillRequest::new(fields)
    };
    let pdf = draw(&fixture("basic_form.pdf"), request, Some(&font)).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn fills_with_center_align_and_a_left_override() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest {
        style: FillStyle {
            h_align: HAlign::Center,
            v_align: VAlign::Bottom,
            ..FillStyle::default()
        },
        ..FillRequest::new(BTreeMap::from([
            ("employee_name".to_owned(), FieldFill::text("Ada")),
            ("agree".to_owned(), FieldFill::check(true)),
            (
                "level".to_owned(),
                FieldFill {
                    style: FieldStyle {
                        h_align: Some(HAlign::Left),
                        ..FieldStyle::default()
                    },
                    ..FieldFill::text("Senior")
                },
            ),
        ]))
    };
    let pdf = draw(&fixture("basic_form.pdf"), request, Some(&font)).unwrap();
    let text = pdftotext(&pdf);
    assert!(text.contains("Ada"), "{text:?}");
    assert!(text.contains("Senior"), "{text:?}");
    let left = draw(&fixture("basic_form.pdf"), basic_request(), Some(&font)).unwrap();
    assert_ne!(pdf, left);
}

#[test]
fn draws_a_line_below_without_filling_the_field() {
    let request = FillRequest {
        rules: BTreeMap::from([("employee_name".to_owned(), FieldRule::new(5.0))]),
        ..FillRequest::default()
    };
    let pdf = draw(&fixture("basic_form.pdf"), request, None).unwrap();
    let text = page_stream(&pdf);
    assert!(
        text.contains("0 0 0 RG\n1 w\n100 707 m\n300 707 l\nS\n"),
        "{text}"
    );
}

#[test]
fn draws_a_value_and_a_rule_for_the_same_name() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest {
        rules: BTreeMap::from([("employee_name".to_owned(), FieldRule::new(5.0))]),
        ..FillRequest::new(BTreeMap::from([(
            "employee_name".to_owned(),
            FieldFill::text("Ada"),
        )]))
    };
    let pdf = draw(&fixture("basic_form.pdf"), request, Some(&font)).unwrap();
    assert!(pdftotext(&pdf).contains("Ada"));
    let text = page_stream(&pdf);
    assert!(
        text.contains("0 0 0 RG\n1 w\n100 707 m\n300 707 l\nS\n"),
        "{text}"
    );
}

#[test]
fn rejects_non_finite_line_below_on_a_known_name_when_missing_is_ignore() {
    let request = FillRequest {
        missing: MissingName::Ignore,
        rules: BTreeMap::from([("employee_name".to_owned(), FieldRule::new(f32::NAN))]),
        ..FillRequest::default()
    };
    let err = draw(&fixture("basic_form.pdf"), request, None).unwrap_err();
    match err.kind() {
        ErrorKind::InvalidArgument(ArgumentReason::NonFinite { value }) => {
            assert_eq!(err.field(), Some("employee_name"));
            assert!(value.is_nan());
        }
        ErrorKind::InvalidArgument(_) | _ => panic!("unexpected {}", err.kind()),
    }
}

#[test]
fn ignores_non_finite_line_below_on_an_unknown_name_when_missing_is_ignore() {
    let request = FillRequest {
        missing: MissingName::Ignore,
        rules: BTreeMap::from([("not_on_the_form".to_owned(), FieldRule::new(f32::NAN))]),
        ..FillRequest::default()
    };
    let pdf = draw(&fixture("basic_form.pdf"), request, None).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(Form::open(&pdf).unwrap().fields().is_empty());
}

#[test]
fn rejects_an_unknown_line_below_name() {
    let request = FillRequest {
        rules: BTreeMap::from([("nope".to_owned(), FieldRule::new(5.0))]),
        ..FillRequest::default()
    };
    let err = draw(&fixture("basic_form.pdf"), request, None).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::UnknownField);
    assert_eq!(err.field(), Some("nope"));
}

#[test]
fn draws_an_absolute_line_and_rejects_a_missing_page() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let line = AbsoluteLine::new(10.0, 20.0, 30.0, 20.0);
    let request = FillRequest {
        lines: vec![line],
        ..basic_request()
    };
    let pdf = draw(&fixture("basic_form.pdf"), request, Some(&font)).unwrap();
    let text = page_stream(&pdf);
    assert!(text.contains("1 0 0 RG"), "{text}");
    let missing = AbsoluteLine {
        page_index: 9,
        ..line
    };
    let form = Form::open(&fixture("basic_form.pdf")).unwrap();
    let err = Fill::new(
        &form,
        FillRequest {
            lines: vec![missing],
            ..FillRequest::default()
        },
        None,
    )
    .unwrap_err();
    assert_eq!(
        err.kind(),
        ErrorKind::InvalidArgument(ArgumentReason::PageOutOfRange)
    );
    assert_eq!(err.page(), Some(9));
    assert_eq!(err.field(), None);
}

#[test]
fn font_size_override_changes_the_drawn_size() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest::new(BTreeMap::from([
        (
            "employee_name".to_owned(),
            FieldFill {
                style: FieldStyle {
                    font_size: Some(8.0),
                    ..FieldStyle::default()
                },
                ..FieldFill::text("Ada")
            },
        ),
        ("agree".to_owned(), FieldFill::check(true)),
        ("level".to_owned(), FieldFill::text("Senior")),
    ]));
    let pdf = draw(&fixture("basic_form.pdf"), request, Some(&font)).unwrap();
    let text = page_stream(&pdf);
    assert!(text.contains("8 Tf") || text.contains("8.0 Tf"), "{text}");
}

#[test]
fn fills_chinese_text_when_pdfcraft_forms_font_is_set() {
    let Some(path) =
        std::env::var_os("PDFCRAFT_CJK_FONT").or_else(|| std::env::var_os("PDFCRAFT_FORMS_FONT"))
    else {
        return;
    };
    let bytes = fs::read(path).unwrap();
    let font = load_font(bytes.clone());
    let request = FillRequest::new(BTreeMap::from([(
        "employee_name_zh".to_owned(),
        FieldFill::text("王"),
    )]));
    let pdf = match draw(&fixture("chinese_form.pdf"), request, Some(&font)) {
        Ok(pdf) => pdf,
        Err(error) if error.kind() == ErrorKind::MissingGlyph => return,
        Err(error) => panic!("{error}"),
    };
    assert!(pdftotext(&pdf).contains('王'));
    if bytes.len() > 1024 * 1024 {
        let lengths = font_file2_lengths(&pdf);
        assert_eq!(lengths.len(), 1);
        assert!(lengths[0] < 256 * 1024, "FontFile2 is {} bytes", lengths[0]);
        assert!(pdf.len() < 512 * 1024, "PDF is {} bytes", pdf.len());
    }
}

#[test]
#[ignore = "CJK fill is deferred; this branch does not ship a CJK font"]
fn fills_chinese_text() {
    let Some(bytes) = font_bytes() else {
        return;
    };
    let font = load_font(bytes);
    let request = FillRequest::new(BTreeMap::from([(
        "employee_name_zh".to_owned(),
        FieldFill::text("王"),
    )]));
    let pdf = draw(&fixture("chinese_form.pdf"), request, Some(&font)).unwrap();
    assert!(pdftotext(&pdf).contains('王'));
}

#[test]
fn drops_prev_xrefstm_and_trailer_keys_that_point_at_orphans() {
    let (input, orphan) = trailer_orphan_pdf();
    let input_text = String::from_utf8_lossy(&input);
    assert!(input_text.contains("/Prev "), "{input_text}");
    assert!(input_text.contains("/XRefStm "), "{input_text}");
    let before = lopdf::Document::load_mem(&input).unwrap();
    // lopdf 0.45 deletes `/Prev` while resolving the xref chain, including a
    // non-integer value. `/XRefStm` stays when that loop does not run.
    assert!(before.trailer.get(b"Prev").is_err());
    assert!(before.trailer.get(b"XRefStm").is_ok());
    assert!(before.get_object(orphan).is_ok());
    let pdf = draw(&input, FillRequest::default(), None).unwrap();
    let out = lopdf::Document::load_mem(&pdf).unwrap();
    assert!(out.trailer.get(b"Prev").is_err());
    assert!(out.trailer.get(b"XRefStm").is_err());
    assert!(out.trailer.get(b"QuireOrphan").is_err());
    assert!(out.trailer.get(b"Root").is_ok());
    assert!(out.get_object(orphan).is_err());
}

fn radio_widget_pdf() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    let widget_id = doc.add_object(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name(b"Annot".to_vec())),
        ("Subtype", lopdf::Object::Name(b"Widget".to_vec())),
        ("T", lopdf::Object::string_literal("radio")),
        ("FT", lopdf::Object::Name(b"Btn".to_vec())),
        ("Ff", lopdf::Object::Integer(1 << 15)),
        (
            "Rect",
            lopdf::Object::Array(vec![
                lopdf::Object::Integer(10),
                lopdf::Object::Integer(10),
                lopdf::Object::Integer(30),
                lopdf::Object::Integer(30),
            ]),
        ),
    ]));
    doc.set_object(
        page,
        lopdf::Dictionary::from_iter([
            ("Type", lopdf::Object::Name(b"Page".to_vec())),
            ("Parent", lopdf::Object::Reference(page_tree)),
            (
                "MediaBox",
                lopdf::Object::Array(vec![
                    lopdf::Object::Integer(0),
                    lopdf::Object::Integer(0),
                    lopdf::Object::Integer(200),
                    lopdf::Object::Integer(200),
                ]),
            ),
            (
                "Annots",
                lopdf::Object::Array(vec![lopdf::Object::Reference(widget_id)]),
            ),
        ]),
    );
    doc.set_object(
        page_tree,
        lopdf::Dictionary::from_iter([
            ("Type", lopdf::Object::Name(b"Pages".to_vec())),
            ("Count", lopdf::Object::Integer(1)),
            (
                "Kids",
                lopdf::Object::Array(vec![lopdf::Object::Reference(page)]),
            ),
        ]),
    );
    let catalog_id = doc.add_object(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name(b"Catalog".to_vec())),
        ("Pages", lopdf::Object::Reference(page_tree)),
    ]));
    doc.trailer
        .set("Root", lopdf::Object::Reference(catalog_id));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

fn trailer_orphan_pdf() -> (Vec<u8>, lopdf::ObjectId) {
    let mut doc = lopdf::Document::with_version("1.4");
    let orphan = doc.add_object(lopdf::Object::string_literal("orphan"));
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    doc.set_object(
        page,
        lopdf::Dictionary::from_iter([
            ("Type", lopdf::Object::Name(b"Page".to_vec())),
            ("Parent", lopdf::Object::Reference(page_tree)),
            (
                "MediaBox",
                lopdf::Object::Array(vec![
                    lopdf::Object::Integer(0),
                    lopdf::Object::Integer(0),
                    lopdf::Object::Integer(200),
                    lopdf::Object::Integer(200),
                ]),
            ),
        ]),
    );
    doc.set_object(
        page_tree,
        lopdf::Dictionary::from_iter([
            ("Type", lopdf::Object::Name(b"Pages".to_vec())),
            ("Count", lopdf::Object::Integer(1)),
            (
                "Kids",
                lopdf::Object::Array(vec![lopdf::Object::Reference(page)]),
            ),
        ]),
    );
    let catalog_id = doc.add_object(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name(b"Catalog".to_vec())),
        ("Pages", lopdf::Object::Reference(page_tree)),
    ]));
    doc.trailer
        .set("Root", lopdf::Object::Reference(catalog_id));
    doc.trailer
        .set("QuireOrphan", lopdf::Object::Reference(orphan));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let start = startxref_offset(&bytes);
    let dict = bytes[start..]
        .windows(2)
        .position(|window| window == b"<<")
        .unwrap();
    let at = start + dict + 2;
    // A string `/Prev` is not an xref offset. lopdf 0.45 still deletes that key
    // on load and leaves `/XRefStm`. `prune` has to drop `XRefStm`.
    let extra = format!("/Prev (0) /XRefStm {start}");
    let mut with_offsets = Vec::with_capacity(bytes.len() + extra.len());
    with_offsets.extend_from_slice(&bytes[..at]);
    with_offsets.extend_from_slice(extra.as_bytes());
    with_offsets.extend_from_slice(&bytes[at..]);
    (with_offsets, orphan)
}

fn startxref_offset(bytes: &[u8]) -> usize {
    let marker = b"startxref\n";
    let at = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)
        .unwrap()
        + marker.len();
    let digits = bytes[at..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .copied()
        .collect::<Vec<_>>();
    String::from_utf8(digits).unwrap().parse().unwrap()
}

fn pdftotext(pdf: &[u8]) -> String {
    let mut child = Command::new("pdftotext")
        .arg("-")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(pdf).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn font_file2_lengths(pdf: &[u8]) -> Vec<usize> {
    let doc = lopdf::Document::load_mem(pdf).unwrap();
    doc.objects
        .values()
        .filter_map(|object| {
            let dictionary = object.as_dict().ok()?;
            let id = dictionary.get(b"FontFile2").ok()?.as_reference().ok()?;
            let stream = doc.get_object(id).ok()?.as_stream().ok()?;
            assert!(
                stream.dict.get(b"Filter").is_err(),
                "subset font is uncompressed"
            );
            Some(stream.content.len())
        })
        .collect()
}

const AGREE_MARK: &[&str] = &[
    "q", "100", "662", "30", "30", "re", "W", "n", "2.94", "w", "1", "J", "1", "j", "108.28",
    "677.42", "m", "113.95", "671.12", "l", "122.14", "683.72", "l", "S", "Q",
];

fn has_agree_mark(doc: &lopdf::Document) -> bool {
    let text = page_text(doc);
    let tokens: Vec<_> = text.split_whitespace().collect();
    tokens
        .windows(AGREE_MARK.len())
        .any(|window| window == AGREE_MARK)
}

fn page_stream(pdf: &[u8]) -> String {
    let doc = lopdf::Document::load_mem(pdf).unwrap();
    page_text(&doc)
}

fn page_text(doc: &lopdf::Document) -> String {
    let page_id = doc.page_iter().next().unwrap();
    let bytes = doc.get_page_content(page_id);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn assert_subset_head_times_are_zero(pdf: &[u8]) {
    let doc = lopdf::Document::load_mem(pdf).unwrap();
    let mut found = false;
    for object in doc.objects.values() {
        let Ok(dictionary) = object.as_dict() else {
            continue;
        };
        let Ok(id) = dictionary
            .get(b"FontFile2")
            .and_then(lopdf::Object::as_reference)
        else {
            continue;
        };
        let bytes = &doc.get_object(id).unwrap().as_stream().unwrap().content;
        let tables = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        for index in 0..tables {
            let at = 12 + index * 16;
            if &bytes[at..at + 4] != b"head" {
                continue;
            }
            let off = u32::from_be_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
            assert_eq!(
                &bytes[off + 20..off + 36],
                &[0; 16],
                "subset head timestamps are not zero"
            );
            found = true;
        }
    }
    assert!(found, "embedded subset has a head table");
}

#[test]
fn matches_field_value_and_field_type() {
    fn describe(value: &FieldValue) -> &'static str {
        match value {
            FieldValue::Text(_) => "text",
            FieldValue::CheckBox(_) => "checkbox",
        }
    }
    fn kind_name(field_type: FieldType) -> &'static str {
        match field_type {
            FieldType::Text => "text",
            FieldType::CheckBox => "checkbox",
            FieldType::ListBox => "list",
            FieldType::ComboBox => "combo",
            FieldType::Signature => "signature",
            FieldType::RadioButton => "radio",
            FieldType::PushButton => "push",
        }
    }
    assert_eq!(describe(&FieldValue::Text("Ada".into())), "text");
    assert_eq!(describe(&FieldValue::CheckBox(true)), "checkbox");
    assert_eq!(kind_name(FieldType::Text), "text");
    assert_eq!(kind_name(FieldType::PushButton), "push");
}

#[test]
fn absolute_line_default_matches_new_at_origin() {
    assert_eq!(
        AbsoluteLine::default(),
        AbsoluteLine::new(0.0, 0.0, 0.0, 0.0)
    );
    let placed = AbsoluteLine {
        x1: 1.0,
        y1: 2.0,
        ..Default::default()
    };
    assert_eq!(placed.x1.to_bits(), 1.0_f32.to_bits());
    assert_eq!(placed.style.width.to_bits(), 0.8_f32.to_bits());
    assert_eq!(placed.style, StrokeStyle::default());
}

#[test]
fn field_info_name_is_readable() {
    let fields = Form::open(&fixture("basic_form.pdf"))
        .unwrap()
        .fields()
        .to_vec();
    let first = fields.first().expect("one widget");
    assert!(!first.name().is_empty());
}
