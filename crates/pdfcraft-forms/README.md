# pdfcraft-forms

**Fill AcroForm fields in an existing PDF and flatten the result.**

Pass PDF bytes, a map of field names to values, and optional draw settings.
`list_fields` returns one entry per widget. `fill_and_flatten` overlays text,
checks, and choices, then removes every widget annotation and the catalog
`AcroForm` entry. An empty map still strips the form. Reading uses
[lopdf](https://crates.io/crates/lopdf). Measurement uses
[skrifa](https://crates.io/crates/skrifa). This crate does not depend on
`pdfcraft-pdf` and does not use krilla.

| | |
| --- | --- |
| Guide | [docs/guide.md](docs/guide.md) |
| API | [docs.rs/pdfcraft-forms](https://docs.rs/pdfcraft-forms) |
| MSRV | 1.92 |
| License | MIT OR Apache-2.0 |
| Status | `0.1.0`. The API can break before `1.0` |

---

## Install

```toml
[dependencies]
pdfcraft-forms = "0.1.0"
```

---

## Quick start

```rust
use std::collections::HashMap;

use pdfcraft_forms::{
    fill_and_flatten, list_fields, FieldValue, FillOptions, FontBytes, Result,
};

fn main() -> Result<()> {
    let pdf = std::fs::read("form.pdf").expect("pdf");
    let _widgets = list_fields(&pdf)?;

    let font = std::fs::read("face.ttf").expect("font");
    let mut fields = HashMap::new();
    fields.insert("employee_name".into(), FieldValue::Text("Ada".into()));
    fields.insert("agree".into(), FieldValue::CheckBox(true));

    let filled = fill_and_flatten(
        &pdf,
        &fields,
        &FillOptions {
            font: Some(FontBytes {
                bytes: &font,
                index: 0,
            }),
            ..FillOptions::default()
        },
    )?;
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
PDF bytes ──list_fields──► FieldInfo…
PDF bytes + field map + FillOptions ──fill_and_flatten──► PDF bytes
```

1. **Inspect** — one `FieldInfo` per widget annotation.
2. **Fill** — overlay draws, then strip widgets and `/AcroForm`.
3. **Options** — alignment, per-field sizes, `line_below`, `absolute_lines`.

Coordinates: PDF user space, **bottom-left origin, y upward**.  
Fonts: caller TrueType with `glyf`. `FontBytes.index` selects a TTC face.

Deep dive: **[docs/guide.md](docs/guide.md)** (values, fit/wrap, errors, testing).

---

## Capsule API

| Item | Role |
| --- | --- |
| `list_fields` | One `FieldInfo` per widget |
| `fill_and_flatten` | Draw values and strip the form |
| `FieldValue` | `Text(String)` or `CheckBox(bool)` |
| `FillOptions` | Font, size, alignment, decorative lines |
| `FontBytes` | Face bytes plus collection index |
| `HAlign` / `VAlign` | Text placement inside the widget |
| `MissingName` | Unknown names: error (default) or ignore |
| `AbsoluteLine` | Page-absolute stroke |
| `Error` / `ErrorKind` | Open, fill, and flatten failures |

---

## Design constraints (v1)

- No OTF fonts and no font subsetting
- Signature, radio, and push-button widgets are not writable
- Choice strings must match a widget `/Opt` export or display value
- No dependency on `pdfcraft-pdf`

---

## Compatibility

- **Rust**: MSRV 1.92 (edition 2021)
- **`unsafe`**: forbidden in this crate
- **SemVer**: `0.x` — breaking changes may land without a `1.0` bump; read
  `CHANGELOG.md`

---

## License

Licensed under either of

- Apache License, Version 2.0, [LICENSE-APACHE](LICENSE-APACHE)
- MIT license, [LICENSE-MIT](LICENSE-MIT)

at your option.

[tests/fixtures/README.md](tests/fixtures/README.md) records which fixtures the
package ships.
