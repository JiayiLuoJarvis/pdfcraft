# Contributing

Thank you for considering a contribution.

## Before you code

1. For **product scope**, read [`docs/提案书.md`](docs/提案书.md). Confirmed
   bullets there override [`docs/pdfcraft-design.md`](docs/pdfcraft-design.md).
   Callers should never need those files — their docs are
   [`crates/pdfcraft-pdf/README.md`](crates/pdfcraft-pdf/README.md) and
   [`crates/pdfcraft-pdf/docs/guide.md`](crates/pdfcraft-pdf/docs/guide.md).
2. For **how to write code**, follow
   [`docs/rust-engineering-standard.md`](docs/rust-engineering-standard.md).
   It is the single source for style, panics, errors, API shape, architecture,
   determinism, dependencies, semver, tests, docs, and lints. Each rule names
   its source (Rust Style Guide, Rust API Guidelines, Microsoft Pragmatic Rust
   Guidelines, Google Rust style guide) and how it is enforced.
3. [`AGENTS.md`](AGENTS.md) covers product constraints, layout, and what not to
   touch.

## Standards in one paragraph

Lints live in the root `Cargo.toml` `[workspace.lints]`; override one only at
the site with `#[expect(lint, reason = "...")]`, never `#[allow]`. Production
code has no `unwrap`; a panic is allowed only for a proven internal invariant
and is registered with `#[expect(clippy::expect_used, reason = "...")]`.
Rustdoc, comments, and error messages are in English. The font/PDF stack is
`=`-pinned; leaf utilities use caret requirements and `Cargo.lock` keeps builds
reproducible. One concern per PR.

## Before opening a PR

Run the gate in section TOOL-5 of the engineering standard. CI runs the same
commands with `RUSTFLAGS=-D warnings` and `RUSTDOCFLAGS=-D warnings`. Forms
fill tests call `pdftotext`, so have it installed.

The toolchain and MSRV are **1.98**; see [`docs/msrv.md`](docs/msrv.md).
Package layout: [`docs/workspace-crates-spec.md`](docs/workspace-crates-spec.md).

Integration tests compare coordinates with `pdfcraft_pdf::canvas::pt_near`.
That function is always public. Feature `test-utils` only re-exports it
at the crate root.

User-visible behavior belongs in the crate's `CHANGELOG.md`. A breaking change
bumps the `y` in `0.y.z` first.
