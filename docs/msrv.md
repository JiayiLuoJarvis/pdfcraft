# MSRV policy

Current MSRV: **1.98**. It equals the CI toolchain and `clippy.toml msrv`
(engineering standard TOOL-1).

krilla 0.8.2 needs 1.92, so the dependency floor is lower. The workspace runs
one toolchain for building, linting, and documenting, so the lint set in
`[workspace.lints]` is priced and enforced on exactly the version callers are
told to use. We don't promise stable-minus-two.

CI runs every Rust job on 1.98 (`basics`, `deny`, `test`, `minrust`,
`feature-matrix`); `minimal-versions` runs on nightly, `semver-checks` uses
its action's toolchain. `minrust` checks the workspace with `--locked` on the
floor.

## Bumping the toolchain

Bump in one PR (TOOL-2) and change every place the version appears:

- root `Cargo.toml` `[workspace.package] rust-version` (members inherit it)
- `clippy.toml` `msrv`
- `.github/workflows/ci.yml`: each `toolchain:` and the `minrust` job name
- `README.md`, `CONTRIBUTING.md`, this file
- `crates/*/README.md` (the MSRV row and the Rust line)

Find stragglers with `rg -n '1\.98' --glob '!target' --glob '!*CHANGELOG.md'`.

In the same PR:

- Re-price `[workspace.lints]` on the new toolchain with
  `python3 scripts/price_lints.py` (engineering standard LINT-3). Clear new
  lint and rustdoc hits. Drop `#[expect]` attributes that the new toolchain
  reports as unfulfilled.
- Re-run the coordinate and glyph tests:
  `cargo test -p pdfcraft-pdf --features backend-krilla,test-utils`,
  `cargo test -p pdfcraft-forms`, `cargo test -p pdfcraft-orgchart`.
- Record the bump under `Changed` in each crate's CHANGELOG; the next release
  of each crate bumps the `y` in `0.y.z` (VER-1).

Never fold a bump into a feature or dependency PR.
