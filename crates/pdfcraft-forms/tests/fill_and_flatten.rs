//! `fill_and_flatten` against the copied `AcroForm` fixtures.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use pdfcraft_forms::{
    fill_and_flatten, list_fields, AbsoluteLine, ArgumentReason, Error, ErrorKind, FieldInfo,
    FieldType, FieldValue, FillOptions, FlattenReason, FontBytes, FontReason, HAlign, MissingName,
    VAlign,
};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn font() -> Option<Vec<u8>> {
    let path = std::env::var_os("PDFCRAFT_FORMS_FONT")?;
    Some(fs::read(path).expect("read PDFCRAFT_FORMS_FONT"))
}

fn options(font_bytes: &[u8]) -> FillOptions<'_> {
    FillOptions {
        font: Some(FontBytes {
            bytes: font_bytes,
            index: 0,
        }),
        ..FillOptions::default()
    }
}

fn basic_fields() -> HashMap<String, FieldValue> {
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("Ada".to_string()),
    );
    fields.insert("agree".to_string(), FieldValue::CheckBox(true)); // checked
    fields.insert("level".to_string(), FieldValue::Text("Senior".to_string()));
    fields
}

fn filled_basic() -> Option<Vec<u8>> {
    let font_bytes = font()?;
    Some(
        fill_and_flatten(
            &fixture("basic_form.pdf"),
            &basic_fields(),
            &options(&font_bytes),
        )
        .unwrap(),
    )
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
    assert!(list_fields(&pdf).unwrap().is_empty());
    let text = pdftotext(&pdf);
    assert!(text.contains("Ada"), "{text:?}");
    assert!(text.contains("Senior"), "{text:?}");
    assert!(has_agree_mark(&doc), "{}", page_text(&doc));
}

#[test]
fn empty_map_strips_widgets() {
    let pdf = fill_and_flatten(
        &fixture("basic_form.pdf"),
        &HashMap::new(),
        &FillOptions::default(),
    )
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    assert!(doc.catalog().unwrap().get(b"AcroForm").is_err());
    assert!(list_fields(&pdf).unwrap().is_empty());
    assert!(!has_agree_mark(&doc), "{}", page_text(&doc));
}

#[test]
fn checkbox_only_fill_does_not_embed_a_font() {
    let mut fields = HashMap::new();
    fields.insert("agree".to_string(), FieldValue::CheckBox(true)); // checked
    let pdf =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &FillOptions::default()).unwrap();
    assert!(!pdf
        .windows(b"/FontFile2".len())
        .any(|window| window == b"/FontFile2"));
}

#[test]
fn unchecked_box_draws_no_mark() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut fields = HashMap::new();
    fields.insert("agree".to_string(), FieldValue::CheckBox(false)); // unchecked
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options(&font_bytes)).unwrap();
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    assert!(!has_agree_mark(&doc), "{}", page_text(&doc));
}

#[test]
fn two_fills_match_byte_for_byte() {
    let Some(a) = filled_basic() else {
        return;
    };
    assert_eq!(a, filled_basic().unwrap());
}

#[test]
fn rejects_illegal_choice() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut fields = HashMap::new();
    fields.insert("level".to_string(), FieldValue::Text("Nope".to_string()));
    let err =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options(&font_bytes)).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::IllegalChoice {
            field_name: "level".to_string(),
        }
    );
}

#[test]
fn rejects_a_checkbox_value_on_a_text_widget() {
    let mut fields = HashMap::new();
    fields.insert("employee_name".to_string(), FieldValue::CheckBox(true)); // checked
    let err =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &FillOptions::default()).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::InvalidArgument {
            field_name: Some("employee_name".to_string()),
            reason: ArgumentReason::CheckboxOnText,
        }
    );
}

#[test]
fn rejects_any_value_on_a_radio_widget() {
    let pdf = radio_widget_pdf();
    for value in [
        FieldValue::CheckBox(true), // checked
        FieldValue::Text("Ada".to_string()),
    ] {
        let mut fields = HashMap::new();
        fields.insert("radio".to_string(), value);
        let err = fill_and_flatten(&pdf, &fields, &FillOptions::default()).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::WidgetUnsupported {
                field_name: "radio".to_string(),
            }
        );
    }
}

#[test]
fn rejects_text_on_a_checkbox() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut fields = HashMap::new();
    fields.insert("agree".to_string(), FieldValue::Text("yes".to_string()));
    let err =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options(&font_bytes)).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::InvalidCheckbox {
            field_name: "agree".to_string(),
        }
    );
}

#[test]
fn rejects_a_missing_font_when_text_is_drawn() {
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("Ada".to_string()),
    );
    let err =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &FillOptions::default()).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::FontNotConfigured {
            reason: FontReason::Missing,
        }
    );
    assert!(std::error::Error::source(&err).is_none());
}

#[test]
fn rejects_unusable_font_bytes() {
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("Ada".to_string()),
    );
    let cases = [
        (b"ttcf".as_slice(), FontReason::Collection),
        (b"not a font".as_slice(), FontReason::Head),
    ];
    for (bytes, reason) in cases {
        let options = FillOptions {
            font: Some(FontBytes { bytes, index: 0 }),
            ..FillOptions::default()
        };
        let err = fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options).unwrap_err();
        assert_eq!(err.kind, ErrorKind::FontNotConfigured { reason });
        assert!(std::error::Error::source(&err).is_none());
    }
}

#[test]
fn rejects_a_font_size_that_is_not_positive() {
    for value in [0.0_f32, -2.5] {
        let options = FillOptions {
            font_size: value,
            ..FillOptions::default()
        };
        let err = fill_and_flatten(b"not a pdf", &HashMap::new(), &options).unwrap_err();
        assert_eq!(
            err.to_string(),
            "font_size must be finite and greater than 0"
        );
        assert_eq!(
            err.kind,
            ErrorKind::InvalidArgument {
                field_name: None,
                reason: ArgumentReason::FontSize { value },
            }
        );
    }
    let options = FillOptions {
        font_size: f32::NAN,
        ..FillOptions::default()
    };
    let err = fill_and_flatten(b"not a pdf", &HashMap::new(), &options).unwrap_err();
    assert_eq!(
        err.to_string(),
        "font_size must be finite and greater than 0"
    );
    match err.kind {
        ErrorKind::InvalidArgument {
            field_name: None,
            reason: ArgumentReason::FontSize { value },
        } => assert!(!value.is_finite()),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn rejects_a_font_size_above_256_pt_before_opening_the_pdf() {
    for value in [1000.0_f32, 1.0e20] {
        let options = FillOptions {
            font_size: value,
            ..FillOptions::default()
        };
        let err = fill_and_flatten(b"not a pdf", &HashMap::new(), &options).unwrap_err();
        assert_ne!(err.kind, ErrorKind::NotAPdf);
        match err.kind {
            ErrorKind::InvalidArgument {
                field_name: None,
                reason: ArgumentReason::FontSize { value: got },
            } => assert_eq!(got.to_bits(), value.to_bits()),
            other => panic!("unexpected {other:?}"),
        }
    }
    for value in [256.0_f32, 0.0001] {
        let options = FillOptions {
            font_size: value,
            ..FillOptions::default()
        };
        let err = fill_and_flatten(b"not a pdf", &HashMap::new(), &options).unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotAPdf);
    }
}

#[test]
fn rejects_a_missing_glyph() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("王".to_string()),
    );
    let err =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options(&font_bytes)).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::MissingGlyph {
            field_name: "employee_name".to_string(),
            scalar: '王',
        }
    );
}

#[test]
fn rejects_text_that_cannot_fit() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("x".repeat(500)),
    );
    let err =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options(&font_bytes)).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::BoxOverflow {
            field_name: "employee_name".to_string(),
        }
    );
}

#[test]
fn rejects_filling_a_signature() {
    let mut fields = HashMap::new();
    fields.insert("signature".to_string(), FieldValue::Text("Ada".to_string()));
    let err = fill_and_flatten(
        &fixture("signature_form.pdf"),
        &fields,
        &FillOptions::default(),
    )
    .unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::SignatureNotWritable {
            field_name: "signature".to_string(),
        }
    );
}

#[test]
fn rejects_an_unknown_name() {
    let mut fields = HashMap::new();
    fields.insert("missing".to_string(), FieldValue::CheckBox(true)); // checked
    let err =
        fill_and_flatten(&fixture("basic_form.pdf"), &fields, &FillOptions::default()).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::UnknownField {
            field_name: "missing".to_string(),
        }
    );
}

#[test]
fn rejects_an_unknown_h_align_override() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut overrides = HashMap::new();
    overrides.insert("nope".to_string(), HAlign::Center);
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font_bytes,
            index: 0,
        }),
        h_align_overrides: Some(&overrides),
        ..FillOptions::default()
    };
    let err = fill_and_flatten(&fixture("basic_form.pdf"), &basic_fields(), &options).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::UnknownField {
            field_name: "nope".to_string(),
        }
    );
}

#[test]
fn ignores_an_unknown_v_align_override_when_missing_is_ignore() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut overrides = HashMap::new();
    overrides.insert("nope".to_string(), VAlign::Top);
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font_bytes,
            index: 0,
        }),
        missing: MissingName::Ignore,
        v_align_overrides: Some(&overrides),
        ..FillOptions::default()
    };
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &basic_fields(), &options).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn fills_with_center_align_and_a_left_override() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut overrides = HashMap::new();
    overrides.insert("level".to_string(), HAlign::Left);
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font_bytes,
            index: 0,
        }),
        h_align: HAlign::Center,
        h_align_overrides: Some(&overrides),
        v_align: VAlign::Bottom,
        ..FillOptions::default()
    };
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &basic_fields(), &options).unwrap();
    let text = pdftotext(&pdf);
    assert!(text.contains("Ada"), "{text:?}");
    assert!(text.contains("Senior"), "{text:?}");
    let left = fill_and_flatten(
        &fixture("basic_form.pdf"),
        &basic_fields(),
        &FillOptions {
            font: Some(FontBytes {
                bytes: &font_bytes,
                index: 0,
            }),
            h_align: HAlign::Left,
            ..FillOptions::default()
        },
    )
    .unwrap();
    assert_ne!(pdf, left);
}

#[test]
fn draws_a_line_below_without_filling_the_field() {
    let mut below = HashMap::new();
    below.insert("employee_name".to_string(), 5.0);
    let options = FillOptions {
        line_below: Some(&below),
        ..FillOptions::default()
    };
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &HashMap::new(), &options).unwrap();
    let text = page_stream(&pdf);
    assert!(
        text.contains("0 0 0 RG\n1 w\n100 707 m\n300 707 l\nS\n"),
        "{text}"
    );
}

#[test]
fn rejects_non_finite_line_below_on_a_known_name_when_missing_is_ignore() {
    let mut below = HashMap::new();
    below.insert("employee_name".to_string(), f32::NAN);
    let options = FillOptions {
        missing: MissingName::Ignore,
        line_below: Some(&below),
        ..FillOptions::default()
    };
    let err = fill_and_flatten(&fixture("basic_form.pdf"), &HashMap::new(), &options).unwrap_err();
    match err.kind {
        ErrorKind::InvalidArgument {
            field_name: Some(name),
            reason: ArgumentReason::NonFinite { value },
        } => {
            assert_eq!(name, "employee_name");
            assert!(value.is_nan());
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn ignores_non_finite_line_below_on_an_unknown_name_when_missing_is_ignore() {
    let mut below = HashMap::new();
    below.insert("not_on_the_form".to_string(), f32::NAN);
    let options = FillOptions {
        missing: MissingName::Ignore,
        line_below: Some(&below),
        ..FillOptions::default()
    };
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &HashMap::new(), &options).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    assert!(list_fields(&pdf).unwrap().is_empty());
}

#[test]
fn rejects_an_unknown_line_below_name() {
    let mut below = HashMap::new();
    below.insert("nope".to_string(), 5.0);
    let options = FillOptions {
        line_below: Some(&below),
        ..FillOptions::default()
    };
    let err = fill_and_flatten(&fixture("basic_form.pdf"), &HashMap::new(), &options).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::UnknownField {
            field_name: "nope".to_string(),
        }
    );
}

#[test]
fn draws_an_absolute_line_and_rejects_a_missing_page() {
    let Some(font_bytes) = font() else {
        return;
    };
    let line = AbsoluteLine::new(10.0, 20.0, 30.0, 20.0);
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font_bytes,
            index: 0,
        }),
        absolute_lines: &[line],
        ..FillOptions::default()
    };
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &basic_fields(), &options).unwrap();
    let text = page_stream(&pdf);
    assert!(text.contains("1 0 0 RG"), "{text}");
    let missing = AbsoluteLine {
        page_index: 9,
        ..line
    };
    let options = FillOptions {
        absolute_lines: &[missing],
        ..FillOptions::default()
    };
    let err = fill_and_flatten(&fixture("basic_form.pdf"), &HashMap::new(), &options).unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::FlattenFailed {
            reason: FlattenReason::PageOutOfRange { page_index: 9 },
        }
    );
}

#[test]
fn font_size_override_changes_the_drawn_size() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut overrides = HashMap::new();
    overrides.insert("employee_name".to_string(), 8.0);
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font_bytes,
            index: 0,
        }),
        font_size_overrides: Some(&overrides),
        ..FillOptions::default()
    };
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &basic_fields(), &options).unwrap();
    let text = page_stream(&pdf);
    assert!(text.contains("8 Tf") || text.contains("8.0 Tf"), "{text}");
}

#[test]
fn fills_chinese_text_when_pdfcraft_forms_font_is_set() {
    // Needs a face that maps CJK; ASCII fixtures use PDFCRAFT_FORMS_FONT (often DejaVu).
    let Some(path) =
        std::env::var_os("PDFCRAFT_CJK_FONT").or_else(|| std::env::var_os("PDFCRAFT_FORMS_FONT"))
    else {
        return;
    };
    let font_bytes = fs::read(path).unwrap();
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name_zh".to_string(),
        FieldValue::Text("王".to_string()),
    );
    let pdf = match fill_and_flatten(&fixture("chinese_form.pdf"), &fields, &options(&font_bytes)) {
        Ok(pdf) => pdf,
        Err(error) if matches!(error.kind, ErrorKind::MissingGlyph { .. }) => return,
        Err(error) => panic!("{error}"),
    };
    assert!(pdftotext(&pdf).contains('王'));
    if font_bytes.len() > 1024 * 1024 {
        let lengths = font_file2_lengths(&pdf);
        assert_eq!(lengths.len(), 1);
        assert!(lengths[0] < 256 * 1024, "FontFile2 is {} bytes", lengths[0]);
        assert!(pdf.len() < 512 * 1024, "PDF is {} bytes", pdf.len());
    }
}

#[test]
#[ignore = "CJK fill is deferred; this branch does not ship a CJK font"]
fn fills_chinese_text() {
    let Some(font_bytes) = font() else {
        return;
    };
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name_zh".to_string(),
        FieldValue::Text("王".to_string()),
    );
    let pdf =
        fill_and_flatten(&fixture("chinese_form.pdf"), &fields, &options(&font_bytes)).unwrap();
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
    let pdf = fill_and_flatten(&input, &HashMap::new(), &FillOptions::default()).unwrap();
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
            assert!(stream.dict.get(b"Filter").is_err());
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

#[test]
fn non_exhaustive_enums_match_with_a_wildcard() {
    fn describe(value: &FieldValue) -> &'static str {
        match value {
            FieldValue::Text(_) => "text",
            FieldValue::CheckBox(_) => "checkbox",
            _ => "future",
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
            _ => "future",
        }
    }
    assert_eq!(describe(&FieldValue::Text("Ada".into())), "text");
    assert_eq!(describe(&FieldValue::CheckBox(true)), "checkbox");
    assert_eq!(kind_name(FieldType::Text), "text");
    assert_eq!(kind_name(FieldType::PushButton), "push");
}

#[test]
fn error_new_pairs_with_from() {
    let err = Error::new(ErrorKind::NotAPdf);
    assert_eq!(err, Error::from(ErrorKind::NotAPdf));
    assert_eq!(err.to_string(), "not a PDF");
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
    assert_eq!(placed.stroke_width.to_bits(), 0.8_f32.to_bits());
}

#[test]
fn field_info_reads_through_a_rest_pattern() {
    let fields = list_fields(&fixture("basic_form.pdf")).expect("list fields");
    let first = fields.first().expect("one widget");
    let FieldInfo { name, .. } = first;
    assert!(!name.is_empty());
}

#[test]
fn shared_arc_is_borrowed_as_font_bytes() {
    let shared = std::sync::Arc::new(b"shared-face".to_vec());
    let font = FontBytes {
        bytes: shared.as_slice(),
        index: 0,
    };
    assert!(std::ptr::eq(font.bytes, shared.as_slice()));
    assert_eq!(font.bytes, b"shared-face");
}
