# Contributing

Thank you for considering a contribution.

## Before you code

1. Read [`docs/提案书.md`](docs/提案书.md). Confirmed bullets there override
   [`docs/quire-design.md`](docs/quire-design.md).
2. Follow [`AGENTS.md`](AGENTS.md) for modules, layering, coding standards, and
   what not to touch. That file is the detailed source of truth for agents and
   humans alike.

## Standards we follow

- Formatting and lints: [Rust Style Guide](https://doc.rust-lang.org/style-guide/),
  plus this repo’s `rustfmt.toml` / `clippy.toml` / `Cargo.toml` `[lints]`.
- Public API shape (when the DSL lands):
  [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Small, single-concern PRs; no drive-by refactors.
- Exact dependency pins (`=x.y.z`); add `krilla` / `skrifa` /
  `unicode-segmentation` only in the PR that first needs them.

## Before opening a PR

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
```

MSRV is **1.92** (driven by krilla 0.8.2 once that dependency lands). CI uses
the same toolchain and treats Rust/rustdoc warnings as errors.

User-visible behavior belongs in `CHANGELOG.md`.
