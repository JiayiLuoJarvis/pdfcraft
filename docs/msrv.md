# MSRV policy

Current MSRV: **1.92**.

The floor follows krilla 0.8.2, which needs 1.92. That is higher than what
skrifa 0.48.0 and unicode-segmentation 1.13.3 ask for (1.85), so krilla sets
the bar while we depend on it. We don't promise stable-minus-two during that
time. See `docs/pdfcraft-design.md` Key Decisions for the original rationale.

CI enforces the floor in the `minrust` job, which runs `cargo check` on
toolchain 1.92. The `basics` gate (fmt / doc-links / readme identity)
runs first and the `test` job (fmt / clippy / test / doc) runs on the
stable toolchain; `minimal-versions`, `semver-checks`, and
`feature-matrix` cover the rest on main / manual runs.

## Bumping the floor

Bump MSRV in one PR, and touch exactly these 7 files:

- `README.md` (MSRV statement)
- `CONTRIBUTING.md` (MSRV statement)
- root `Cargo.toml` (`[workspace.package] rust-version`)
- `crates/pdfcraft-pdf/Cargo.toml`, `crates/pdfcraft-forms/Cargo.toml`,
  `crates/pdfcraft-orgchart/Cargo.toml`, `crates/pdfcraft-post/Cargo.toml`
  (each inherits `rust-version.workspace = true`, so confirm the inheritance
  still holds rather than editing a version string)

Bump only when krilla's own MSRV moves, or when the workspace agrees to drop
an older toolchain for a concrete reason. Never sneak a bump into a feature
or dependency PR.

## After a bump

Re-run the coordinate and glyph tests, since the same PR may move the
font/PDF stack. See the AGENTS.md dependency section for the rule that
krilla/skrifa pin changes ship in their own PR with those tests re-run:

- `cargo test -p pdfcraft-pdf --features backend-krilla,test-utils`
- `cargo test -p pdfcraft-forms`
- `cargo test -p pdfcraft-orgchart`
