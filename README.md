# pdfcraft

Cargo workspace for PDF tools. Depend on the crate you need:

| Crate | Path | Role |
| --- | --- | --- |
| **pdfcraft-pdf** | [`crates/pdfcraft-pdf`](crates/pdfcraft-pdf) | Document layout → vector PDF (main line) |
| **pdfcraft-forms** | [`crates/pdfcraft-forms`](crates/pdfcraft-forms) | Fill AcroForm fields in an existing PDF and flatten the result |
| **pdfcraft-orgchart** | [`crates/pdfcraft-orgchart`](crates/pdfcraft-orgchart) | Equity and outbound-investment charts. `FontBytes`, skrifa 0.48.0, krilla 0.8.2. Independent of pdfcraft-pdf |

## pdfcraft-pdf

Caller-facing docs live **with the crate** (same material crates.io / docs.rs show):

| Doc | Role |
| --- | --- |
| [`crates/pdfcraft-pdf/README.md`](crates/pdfcraft-pdf/README.md) | Install, quick start, features |
| [`crates/pdfcraft-pdf/docs/guide.md`](crates/pdfcraft-pdf/docs/guide.md) | Full usage guide |
| `cargo doc -p pdfcraft-pdf --open` | API reference |

```bash
cargo test -p pdfcraft-pdf --features test-utils
cargo run -p pdfcraft-pdf --example hello
```

**Status:** `0.1.0`. The main path works. The API can break before `1.0`.

## pdfcraft-forms

`list_fields` reads the widgets in an existing PDF. `fill_and_flatten` fills text, checkbox, and choice fields, then flattens the form on every success. The same call accepts alignment, per-field alignment overrides, `font_size_overrides`, `line_below`, and `absolute_lines`. `FontBytes.index` selects one face in a TTC. Pass font bytes on `FillOptions`. The crate does not depend on `pdfcraft-pdf`.

```bash
cargo test -p pdfcraft-forms
cargo run -p pdfcraft-forms --example fill_basic
```

This crate does not implement OTF fonts or font subsetting. CJK fill works when you pass a TrueType face that contains `glyf`. The repository does not vendor that font. The CJK tests run when `PDFCRAFT_FORMS_FONT` is set.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md).
Internal product decisions for maintainers: [`docs/提案书.md`](docs/提案书.md).

## License

MIT OR Apache-2.0
