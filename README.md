# quire

Cargo workspace for PDF tools. Depend on the crate you need:

| Crate | Path | Role |
| --- | --- | --- |
| **quire-pdf** | [`crates/quire-pdf`](crates/quire-pdf) | Document layout → vector PDF (main line) |
| **quire-forms** | [`crates/quire-forms`](crates/quire-forms) | Fill forms in existing PDFs (shell) |
| **quire-orgchart** | [`crates/quire-orgchart`](crates/quire-orgchart) | Equity ownership charts. `FontBytes`, skrifa 0.48.0, krilla 0.8.2. Independent of quire-pdf |

## quire-pdf

Caller-facing docs live **with the crate** (same material crates.io / docs.rs show):

| Doc | Role |
| --- | --- |
| [`crates/quire-pdf/README.md`](crates/quire-pdf/README.md) | Install, quick start, features |
| [`crates/quire-pdf/docs/guide.md`](crates/quire-pdf/docs/guide.md) | Full usage guide |
| `cargo doc -p quire-pdf --open` | API reference |

```bash
cargo test -p quire-pdf --features test-utils
cargo run -p quire-pdf --example hello
```

**Status:** `0.x` — main path works; API not frozen; `publish = false` until the first release.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md).
Internal product decisions for maintainers: [`docs/提案书.md`](docs/提案书.md).

## License

MIT OR Apache-2.0
