//! Fill `tests/fixtures/tax_refund_guarantee_id.pdf` with ASCII sample values.
//!
//! Source template: hcsw-pdfcraft `assets/forms/tax_refund_guarantee_id.pdf`
//! (偿还税款保证书 · 身份证号). The passport and BR variants are the same
//! family; only this id template is checked in. It alone has `idTypeName`.
//!
//! Production fill in hcsw-pdfcraft also uses:
//! - Source Han Serif HK for Chinese field values
//! - center horizontal align, with `fileNumber2` left-aligned
//! - `line_below` on `idTypeName` only for a China ID card
//!
//! This example fills ASCII fields. Set `PDFCRAFT_FORMS_FONT` to a CJK `glyf`
//! font to also fill `companyName`, `recipientName`, and `address`.
//!
//! Font resolution order:
//! 1. `PDFCRAFT_FORMS_FONT` — path to a TrueType (`glyf`) `.ttf`
//! 2. `tests/fixtures/DejaVuSans.ttf` — enough for the ASCII fields below
//!
//! ```text
//! cargo run -p pdfcraft-forms --example fill_tax_refund_guarantee
//! PDFCRAFT_FORMS_FONT=/path/to/SourceHanSerifHK-Regular.ttf \
//!   cargo run -p pdfcraft-forms --example fill_tax_refund_guarantee
//! ```

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use pdfcraft_forms::{fill_and_flatten, FieldValue, FillOptions, FontBytes, HAlign, VAlign};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pdf =
        fs::read(root.join("tests/fixtures/tax_refund_guarantee_id.pdf")).unwrap_or_else(|err| {
            eprintln!("read template: {err}");
            process::exit(1);
        });

    let font_path = env::var_os("PDFCRAFT_FORMS_FONT")
        .map_or_else(|| root.join("tests/fixtures/DejaVuSans.ttf"), PathBuf::from);
    let font = fs::read(&font_path).unwrap_or_else(|err| {
        eprintln!("read font {}: {err}", font_path.display());
        process::exit(1);
    });

    // ASCII-only sample map. Production also sets companyName / recipientName /
    // address (CJK) and, for China ID, idTypeName = 「中國」.
    // Same-name widgets (money ×3, recipientName ×2, and page-1 repeats of
    // fileNumber* / year) all receive the same FieldValue.
    let mut fields = HashMap::new();
    for (name, value) in [
        ("idNumber", "A123456(7)"),
        ("date", "13/10/2026"),
        ("year", "2025/26"),
        ("money", "12,345.67"),
        ("RefundNumber", "R-999"),
        ("checkDate", "01/09/2026"),
        ("checkNumber", "C-1001"),
        ("fileNumber1", "12"),
        ("fileNumber2", "34567890"),
    ] {
        fields.insert(name.to_string(), FieldValue::Text(value.to_string()));
    }
    if env::var_os("PDFCRAFT_FORMS_FONT").is_some() {
        for (name, value) in [
            ("companyName", "香港恒盛國際貿易投資顧問有限公司"),
            ("recipientName", "新懋企業管理顧問及公司秘書服務有限公司"),
            ("address", "香港中環"),
            ("idTypeName", "中國"),
        ] {
            fields.insert(name.to_string(), FieldValue::Text(value.to_string()));
        }
    }

    let mut h_overrides = HashMap::new();
    h_overrides.insert("fileNumber2".to_string(), HAlign::Left);
    let options = FillOptions {
        font: Some(FontBytes {
            bytes: &font,
            index: 0,
        }),
        h_align: HAlign::Center,
        v_align: VAlign::Bottom,
        h_align_overrides: Some(&h_overrides),
        ..FillOptions::default()
    };

    let filled = fill_and_flatten(&pdf, &fields, &options).unwrap_or_else(|err| {
        eprintln!("fill_and_flatten: {err}");
        process::exit(1);
    });

    let out_dir = PathBuf::from("out");
    fs::create_dir_all(&out_dir).unwrap_or_else(|err| {
        eprintln!("create out/: {err}");
        process::exit(1);
    });
    let out_path = out_dir.join("fill_tax_refund_guarantee.pdf");
    fs::write(&out_path, filled).unwrap_or_else(|err| {
        eprintln!("write {}: {err}", out_path.display());
        process::exit(1);
    });
    println!("wrote {}", out_path.display());
}
