# pdfcraft

Cargo workspace for PDF tools. Depend on the crate you need. Each package ships
its own README, usage guide, and rustdoc.

| Crate | Path | Role |
| --- | --- | --- |
| **pdfcraft-pdf** | [`crates/pdfcraft-pdf`](crates/pdfcraft-pdf) | Document layout → vector PDF (main line) |
| **pdfcraft-forms** | [`crates/pdfcraft-forms`](crates/pdfcraft-forms) | Fill AcroForm fields in an existing PDF and flatten |
| **pdfcraft-orgchart** | [`crates/pdfcraft-orgchart`](crates/pdfcraft-orgchart) | Equity and outbound-investment charts |

## pdfcraft-pdf

| Doc | Role |
| --- | --- |
| [`crates/pdfcraft-pdf/README.md`](crates/pdfcraft-pdf/README.md) | Install, quick start, features |
| [`crates/pdfcraft-pdf/docs/guide.md`](crates/pdfcraft-pdf/docs/guide.md) | Full usage guide |
| [docs.rs/pdfcraft-pdf](https://docs.rs/pdfcraft-pdf) | API reference |

```bash
cargo test -p pdfcraft-pdf --features test-utils
cargo run -p pdfcraft-pdf --example hello
```

**Status:** `0.1.0`. The API can break before `1.0`.

## pdfcraft-forms

| Doc | Role |
| --- | --- |
| [`crates/pdfcraft-forms/README.md`](crates/pdfcraft-forms/README.md) | Install, quick start |
| [`crates/pdfcraft-forms/docs/guide.md`](crates/pdfcraft-forms/docs/guide.md) | Full usage guide |
| [docs.rs/pdfcraft-forms](https://docs.rs/pdfcraft-forms) | API reference |

```bash
cargo test -p pdfcraft-forms
cargo run -p pdfcraft-forms --example fill_basic
```

Needs `PDFCRAFT_FORMS_FONT` for text-fill examples and CJK tests. No OTF or font
subsetting in v1.

**Status:** `0.1.0`. The API can break before `1.0`.

## pdfcraft-orgchart

| Doc | Role |
| --- | --- |
| [`crates/pdfcraft-orgchart/README.md`](crates/pdfcraft-orgchart/README.md) | Install, quick start |
| [`crates/pdfcraft-orgchart/docs/guide.md`](crates/pdfcraft-orgchart/docs/guide.md) | Full usage guide |
| [docs.rs/pdfcraft-orgchart](https://docs.rs/pdfcraft-orgchart) | API reference |

```bash
cargo test -p pdfcraft-orgchart
cargo run -p pdfcraft-orgchart --example equity_chart
```

Needs `PDFCRAFT_ORGCHART_FONT` or `PDFCRAFT_CJK_FONT` (single-face).

**Status:** `0.1.0`. The API can break before `1.0`.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md).
Internal product decisions for maintainers: [`docs/提案书.md`](docs/提案书.md).

## License

MIT OR Apache-2.0
