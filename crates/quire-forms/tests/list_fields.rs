//! `list_fields` against the copied `AcroForm` fixtures.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use quire_forms::{list_fields, ErrorKind, FieldType};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

#[test]
fn lists_basic_names_and_types() {
    let fields = list_fields(&fixture("basic_form.pdf")).unwrap();
    let by_name: BTreeMap<_, _> = fields
        .into_iter()
        .map(|field| (field.name.clone(), field))
        .collect();
    assert_eq!(by_name.len(), 3);
    assert_eq!(by_name["employee_name"].field_type, FieldType::Text);
    assert_eq!(by_name["employee_name"].page_index, 0);
    assert_eq!(by_name["employee_name"].options, None);
    assert_eq!(by_name["agree"].field_type, FieldType::CheckBox);
    assert_eq!(by_name["agree"].options, None);
    assert_eq!(by_name["level"].field_type, FieldType::ListBox);
    assert_eq!(
        by_name["level"].options,
        Some(vec![
            "Junior".to_string(),
            "Senior".to_string(),
            "Lead".to_string()
        ])
    );
}

#[test]
fn lists_signature_field() {
    let fields = list_fields(&fixture("signature_form.pdf")).unwrap();
    let signature = fields
        .iter()
        .find(|field| field.name == "signature")
        .unwrap();
    assert_eq!(signature.field_type, FieldType::Signature);
}

#[test]
fn rejects_xfa() {
    let err = list_fields(&fixture("xfa_form.pdf")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::HasXfa);
}

#[test]
fn lists_chinese_field_name() {
    let fields = list_fields(&fixture("chinese_form.pdf")).unwrap();
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["employee_name_zh"]
    );
    assert_eq!(fields[0].field_type, FieldType::Text);
}

#[test]
fn rejects_bytes_that_are_not_a_pdf() {
    let err = list_fields(b"not a pdf").unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotAPdf);
}

#[test]
fn inherits_name_and_type_from_parent() {
    let fields = list_fields(&parent_field_pdf()).unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "employee_name");
    assert_eq!(fields[0].field_type, FieldType::Text);
    assert_eq!(fields[0].page_index, 0);
}

fn parent_field_pdf() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.4");
    let page_tree = doc.new_object_id();
    let page = doc.new_object_id();
    let field_id = doc.add_object(lopdf::Dictionary::from_iter([
        ("T", lopdf::Object::string_literal("employee_name")),
        ("FT", lopdf::Object::Name(b"Tx".to_vec())),
    ]));
    let widget_id = doc.add_object(lopdf::Dictionary::from_iter([
        ("Type", lopdf::Object::Name(b"Annot".to_vec())),
        ("Subtype", lopdf::Object::Name(b"Widget".to_vec())),
        ("Parent", lopdf::Object::Reference(field_id)),
        (
            "Rect",
            lopdf::Object::Array(vec![
                lopdf::Object::Integer(10),
                lopdf::Object::Integer(10),
                lopdf::Object::Integer(80),
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
