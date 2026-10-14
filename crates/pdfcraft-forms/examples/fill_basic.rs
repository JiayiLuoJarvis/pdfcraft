//! Fill `tests/fixtures/basic_form.pdf` and write `out/fill_basic.pdf`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use pdfcraft_forms::{fill_and_flatten, FieldValue, FillOptions, FontBytes};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf = fs::read(root.join("tests/fixtures/basic_form.pdf")).unwrap();
    let font = fs::read(root.join("tests/fixtures/DejaVuSans.ttf")).unwrap();
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("Ada".to_string()),
    );
    fields.insert("agree".to_string(), FieldValue::CheckBox(true));
    fields.insert("level".to_string(), FieldValue::Text("Senior".to_string()));
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font,
            index: 0,
        }),
        ..FillOptions::default()
    };
    let filled = fill_and_flatten(&pdf, &fields, &options).unwrap();
    fs::create_dir_all("out").unwrap();
    fs::write("out/fill_basic.pdf", filled).unwrap();
}
