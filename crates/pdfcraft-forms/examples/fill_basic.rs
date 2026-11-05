//! Fill `tests/fixtures/basic_form.pdf` and write `out/fill_basic.pdf`.
//!
//! Requires `PDFCRAFT_FORMS_FONT` pointing at a TrueType face with `glyf`.
//! A `ttcf` collection uses face 0.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use pdfcraft_forms::{FieldFill, Fill, FillRequest, Font, Form};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf = fs::read(root.join("tests/fixtures/basic_form.pdf"))?;
    let font_path =
        env::var_os("PDFCRAFT_FORMS_FONT").ok_or("set PDFCRAFT_FORMS_FONT to a .ttf path")?;
    let font_bytes = fs::read(&font_path)
        .map_err(|error| format!("read font {}: {error}", PathBuf::from(&font_path).display()))?;
    let font = open_font(font_bytes)?;
    let request = FillRequest::new(BTreeMap::from([
        ("employee_name".to_owned(), FieldFill::text("Ada")),
        ("agree".to_owned(), FieldFill::check(true)),
        ("level".to_owned(), FieldFill::text("Senior")),
    ]));
    let form = Form::open(&pdf)?;
    let filled = Fill::new(&form, request, Some(&font))?.to_pdf()?;
    fs::create_dir_all("out")?;
    fs::write("out/fill_basic.pdf", filled)?;
    Ok(())
}

fn open_font(bytes: Vec<u8>) -> pdfcraft_forms::Result<Font> {
    if bytes.starts_with(b"ttcf") {
        Font::from_collection(bytes, 0)
    } else {
        Font::new(bytes)
    }
}
