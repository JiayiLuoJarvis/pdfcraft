//! `fill_and_flatten` against the copied `AcroForm` fixtures.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use quire_forms::{
    fill_and_flatten, list_fields, ArgumentReason, ErrorKind, FieldValue, FillOptions, FontBytes,
};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn font() -> Vec<u8> {
    fixture("DejaVuSans.ttf")
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
    fields.insert("agree".to_string(), FieldValue::CheckBox(true));
    fields.insert("level".to_string(), FieldValue::Text("Senior".to_string()));
    fields
}

fn filled_basic() -> Vec<u8> {
    let font_bytes = font();
    fill_and_flatten(
        &fixture("basic_form.pdf"),
        &basic_fields(),
        &options(&font_bytes),
    )
    .unwrap()
}

#[test]
fn fills_text_checkbox_and_choice_then_strips_the_form() {
    let pdf = filled_basic();
    assert!(pdf.starts_with(b"%PDF"));
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    let catalog = doc.catalog().unwrap();
    assert!(catalog.get(b"AcroForm").is_err());
    assert!(list_fields(&pdf).unwrap().is_empty());
    let text = pdftotext(&pdf);
    assert!(text.contains("Ada"), "{text:?}");
    assert!(text.contains("Senior"), "{text:?}");
    let content = page_text(&doc);
    assert!(content.contains(" m\n"), "{content}");
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
    assert!(!page_text(&doc).contains(" m\n"));
}

#[test]
fn unchecked_box_draws_no_mark() {
    let font_bytes = font();
    let mut fields = HashMap::new();
    fields.insert("agree".to_string(), FieldValue::CheckBox(false));
    let pdf = fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options(&font_bytes)).unwrap();
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    assert!(!page_text(&doc).contains(" m\n"));
}

#[test]
fn two_fills_match_byte_for_byte() {
    assert_eq!(filled_basic(), filled_basic());
}

#[test]
fn rejects_illegal_choice() {
    let font_bytes = font();
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
fn rejects_text_on_a_checkbox() {
    let font_bytes = font();
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
    assert_eq!(err.kind, ErrorKind::FontNotConfigured);
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn rejects_unusable_font_bytes() {
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("Ada".to_string()),
    );
    for bytes in [b"ttcf".as_slice(), b"not a font".as_slice()] {
        let options = FillOptions {
            font: Some(FontBytes { bytes, index: 0 }),
            ..FillOptions::default()
        };
        let err = fill_and_flatten(&fixture("basic_form.pdf"), &fields, &options).unwrap_err();
        assert_eq!(err.kind, ErrorKind::FontNotConfigured);
        assert!(std::error::Error::source(&err).is_some());
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
    let font_bytes = font();
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
    let font_bytes = font();
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
    fields.insert("missing".to_string(), FieldValue::CheckBox(true));
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
#[ignore = "CJK fill is deferred; this branch does not ship a CJK font"]
fn fills_chinese_text() {
    let font_bytes = font();
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
    let before = lopdf::Document::load_mem(&input).unwrap();
    assert!(before.trailer.get(b"Prev").is_ok());
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
    // A literal `Prev` is not an offset, so lopdf keeps `XRefStm` while loading.
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

fn page_text(doc: &lopdf::Document) -> String {
    let page_id = doc.page_iter().next().unwrap();
    let bytes = doc.get_page_content(page_id).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}
