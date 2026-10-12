# quire

Cargo workspace for PDF tools. Depend on the crate you need:

| Crate | Path | Role |
| --- | --- | --- |
| **quire-pdf** | [`crates/quire-pdf`](crates/quire-pdf) | Document layout → vector PDF (main line) |
| **quire-forms** | [`crates/quire-forms`](crates/quire-forms) | Fill forms in existing PDFs (shell) |
| **quire-orgchart** | [`crates/quire-orgchart`](crates/quire-orgchart) | Equity ownership charts (shell) |

## quire-pdf

You describe the document as a `DocumentSpec` (flow content + page-anchored marks) and lay it out with `layout_spec`.
Put font bytes and encoded images on `Resources`. `Resources::add_image` returns an `ImageRef`. `DocumentSpec::limits` holds the caps. `layout_spec` returns `Rendered`, and `to_pdf` writes the PDF bytes.
The library measures text, breaks lines and pages, lays out tables, then draws through
an internal canvas. The only PDF backend is a private [krilla](https://crates.io/crates/krilla)
adapter.

**Status:** scaffolding. Public API is not frozen. See `docs/提案书.md`.

```bash
cargo test -p quire-pdf --features test-utils
cargo run -p quire-pdf --example hello
```

## Docs

| Document | Role |
| --- | --- |
| [`docs/提案书.md`](docs/提案书.md) | Confirmed product decisions (source of truth) |
| [`docs/workspace-crates-spec.md`](docs/workspace-crates-spec.md) | Workspace / crate boundaries |
| [`docs/quire-design.md`](docs/quire-design.md) | Design draft; unconfirmed items do not bind |
| [`AGENTS.md`](AGENTS.md) | Conventions for humans and coding agents |

## License

MIT OR Apache-2.0
