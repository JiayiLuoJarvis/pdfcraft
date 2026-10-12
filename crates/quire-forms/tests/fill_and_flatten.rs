//! `fill_and_flatten` against the copied `AcroForm` fixtures.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use quire_forms::{fill_and_flatten, list_fields, ErrorKind, FieldValue, FillOptions, FontBytes};

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
