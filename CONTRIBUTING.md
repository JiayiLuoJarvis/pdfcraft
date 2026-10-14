# Contributing

Thank you for considering a contribution.

## Before you code

1. For **product scope**, read [`docs/提案书.md`](docs/提案书.md). Confirmed
   bullets there override [`docs/pdfcraft-design.md`](docs/pdfcraft-design.md).
   Callers should never need those files — their docs are
   [`crates/pdfcraft-pdf/README.md`](crates/pdfcraft-pdf/README.md) and
   [`crates/pdfcraft-pdf/docs/guide.md`](crates/pdfcraft-pdf/docs/guide.md).
2. Follow [`AGENTS.md`](AGENTS.md) for modules, layering, coding standards, and
   what not to touch.

## Standards we follow

- Formatting and lints: [Rust Style Guide](https://doc.rust-lang.org/style-guide/),
  plus this repo’s `rustfmt.toml` / `clippy.toml` / `Cargo.toml` `[lints]`.
- Public API shape (`spec` types):
  [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Small, single-concern PRs; no drive-by refactors.
- Exact dependency pins (`=x.y.z`); add `krilla` / `skrifa` /
  `unicode-segmentation` only in the PR that first needs them.

## Before opening a PR

```bash
cargo fmt --all -- --check
cargo clippy -p pdfcraft-pdf --all-targets --features test-utils -- -D warnings
cargo test -p pdfcraft-pdf --features test-utils
RUSTDOCFLAGS='-D warnings' cargo doc -p pdfcraft-pdf --no-deps
cargo clippy -p pdfcraft-forms -p pdfcraft-orgchart --all-targets -- -D warnings
```

MSRV is **1.92** (driven by krilla 0.8.2). CI uses the same toolchain and
treats Rust/rustdoc warnings as errors. Package layout:
[`docs/workspace-crates-spec.md`](docs/workspace-crates-spec.md).

Integration tests compare coordinates with `pdfcraft_pdf::canvas::pt_near`.
That function is always public. Feature `test-utils` only re-exports it
at the crate root.

User-visible behavior belongs in `CHANGELOG.md`.
