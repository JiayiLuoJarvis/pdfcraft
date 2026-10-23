//! Fill `tests/fixtures/basic_form.pdf` and write `out/fill_basic.pdf`.
//!
//! Requires `PDFCRAFT_FORMS_FONT` pointing at a TrueType face with `glyf`.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use pdfcraft_forms::{fill_and_flatten, FieldValue, FillOptions, FontBytes};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf = fs::read(root.join("tests/fixtures/basic_form.pdf")).unwrap();
    let font_path = env::var_os("PDFCRAFT_FORMS_FONT")
        .unwrap_or_else(|| panic!("set PDFCRAFT_FORMS_FONT to a .ttf path"));
    let font = fs::read(&font_path).unwrap_or_else(|error| {
        panic!("read font {}: {error}", PathBuf::from(font_path).display())
    });
    let mut fields = HashMap::new();
    fields.insert(
        "employee_name".to_string(),
        FieldValue::Text("Ada".to_string()),
    );
    fields.insert("agree".to_string(), FieldValue::CheckBox(true)); // checked
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
