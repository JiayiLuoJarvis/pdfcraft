# pdfcraft-forms

**Fill AcroForm fields in an existing PDF and flatten the result.**

Pass PDF bytes to `Form::open`. Compile a `FillRequest` with `Fill::new`, then
write bytes with `Fill::to_pdf`. The library overlays text, checks, and
choices, then removes every widget annotation and the catalog `AcroForm`
entry. An empty request still strips the form. Reading uses
[lopdf](https://crates.io/crates/lopdf). Measurement uses
[skrifa](https://crates.io/crates/skrifa). This crate does not depend on
`pdfcraft-pdf` and does not use krilla.

| | |
| --- | --- |
| Guide | [docs/guide.md](docs/guide.md) |
| API | [docs.rs/pdfcraft-forms](https://docs.rs/pdfcraft-forms) |
| MSRV | 1.98 |
| License | MIT OR Apache-2.0 |
| Status | `0.3.0`. The API can break before `1.0` |

---

## Install

```toml
[dependencies]
pdfcraft-forms = "0.3.0"
```

---

## Quick start

```rust
use std::collections::BTreeMap;

use pdfcraft_forms::{
    FieldFill, FieldRule, FieldStyle, Fill, FillRequest, FillStyle, Font, Form, HAlign, Result,
};

fn main() -> Result<()> {
    let pdf = std::fs::read("form.pdf").expect("pdf");
    let form = Form::open(&pdf)?;
    let _widgets = form.fields();

    let font_bytes = std::fs::read("face.ttf").expect("font");
    let font = Font::new(font_bytes)?;
    let name = FieldFill {
        style: FieldStyle {
            h_align: Some(HAlign::Center),
            ..FieldStyle::default()
        },
        ..FieldFill::text("Ada")
    };
    let request = FillRequest {
        style: FillStyle {
            font_size: 14.0,
            ..FillStyle::default()
        },
        rules: BTreeMap::from([("employee_name".to_owned(), FieldRule::new(2.0))]),
        ..FillRequest::new(BTreeMap::from([
            ("employee_name".to_owned(), name),
            ("agree".to_owned(), FieldFill::check(true)),
        ]))
    };
    let filled = Fill::new(&form, request, Some(&font))?.to_pdf()?;
    assert!(filled.starts_with(b"%PDF"));
    std::fs::write("out/filled.pdf", filled).expect("write");
    Ok(())
}
```

Runnable demo (needs `PDFCRAFT_FORMS_FONT` pointing at a `.ttf` with `glyf`):

```bash
cargo run -p pdfcraft-forms --example fill_basic
```

---

## How it works

```text
PDF bytes ──Form::open──► FieldInfo…
Form + FillRequest + optional Font ──Fill::new──► Fill::to_pdf ──► PDF bytes
```

1. `Form::open` returns one `FieldInfo` per widget annotation.
2. `Fill::new` checks the request and lays out text. `to_pdf` draws, then strips widgets and `/AcroForm`.
3. `FillRequest` carries `FillStyle`, per-field `FieldStyle`, `FieldRule`, and `AbsoluteLine`.

Coordinates: PDF user space, **bottom-left origin, y upward**.
Fonts: `Font::new` for one `sfnt`, `Font::from_collection` for a `ttcf` face.
`FontFile2` embeds a `glyf` subset of the glyphs that were drawn.

Deep dive: **[docs/guide.md](docs/guide.md)** (values, fit/wrap, errors, testing).

---

## Capsule API

| Item | Role |
| --- | --- |
| `Form` | Open a PDF and list widgets |
| `Fill` | Check a request and write PDF bytes |
| `Font` | One TrueType face (`new` or `from_collection`) |
| `FillRequest` / `FieldFill` / `FieldRule` | Values, rules, and placement |
| `FieldValue` | `Text(String)` or `CheckBox(bool)` |
| `HAlign` / `VAlign` | Text placement inside the widget |
| `MissingName` | Unknown names: error (default) or ignore |
| `AbsoluteLine` | Page-absolute stroke |
| `Error` / `ErrorKind` | Open, fill, and flatten failures |

---

## Design constraints (v1)

- No OTF fonts
- Signature, radio, and push-button widgets are not writable
- Choice strings must match a widget `/Opt` export or display value
- No dependency on `pdfcraft-pdf`

---

## Compatibility

- **Rust**: MSRV 1.98 (edition 2021)
- **`unsafe`**: forbidden in this crate
- **SemVer.** `0.x`. A breaking change bumps the minor version. Read
  `CHANGELOG.md`.

---

## License

Licensed under either of

- Apache License, Version 2.0, [LICENSE-APACHE](LICENSE-APACHE)
- MIT license, [LICENSE-MIT](LICENSE-MIT)

at your option.

[tests/fixtures/README.md](tests/fixtures/README.md) records which fixtures the
package ships.
