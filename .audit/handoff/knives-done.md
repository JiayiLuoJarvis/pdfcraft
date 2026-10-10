# Handoff: knives 7–19

**Date:** 2026-10-10  
**Repo:** /mnt/data/demo/quire  
**Exit predicate:** knives 16–19 green locally (no remote/PR).

| Knife | Branch | Tip (feature) |
| --- | --- | --- |
| 7 table/columns/list/decorate | feat/table-columns-list-decorate | 6dbf40c |
| 8 document DSL | feat/document-dsl | 7cbe823 |
| 9 image | feat/image | a12b610 |
| 10 section | feat/section | 9eda8da |
| 11 layer path | feat/layer-path | 7cb1654 |
| 12 flow hline | feat/hline | c89f2c4 |
| 13 style / bold / first-line indent | feat/style-builders | 5deb0df (feature tip aaf14bf) |
| 14 §2.2 fill-in underscore | feat/inline-blank | b83712e |
| 15 §8 text URI link | feat/text-uri-link | 16749da |
| 16 Cell::spacer | feat/cell-spacer | c9c2af7 (audit a5ba3fb) |
| 17 cell paragraph + list | feat/cell-rich | c3035df (audit 9b939b8) |
| 18 §7 margin presets | feat/margin-presets | 3c9e3da |
| 19 crate name / paper | feat/crate-rename | 7c594813c114fe1d9484dd496a27c33bebf0faca (verify-only) |

## What callers get (knives 16–18)

**16.** `Cell::spacer(height)` → empty vertical cell; too tall → `UnsplittableTallerThanFrame`.

**17.** `Cell::paragraph` (bold/blank/link at cell width). `Cell::ul` / `Cell::ol` with `'static` deferred closures. `Cell` drops `Clone`/`PartialEq`/`Eq`.

**18.** `MarginPreset::{Inch, HalfInch}` → uniform 72/36 pt via `.into()` into existing `margins(Margins)`. Default margins still four zeros.

## Knife 19 (verified, no code change)

Measured on `feat/margin-presets` tip after knife 18:

- `Cargo.toml` `name = "quire-pdf"`, `publish = false` already.
- README title and body already say `quire-pdf`.
- `Paper` / `PageSize` already expose A0–A6 (+ Letter/Legal). Geom unit tests pin mm sizes.
- No A-series gap to fill this knife. Nothing to rename.

## Gates (parent re-verified 2026-10-10)

On `feat/crate-rename` @ `7c59481` (includes knife 18 feature tip 3c9e3da): `cargo fmt --check`, `clippy -D warnings`, `cargo test` green (200 lib + 1 ignored, 78 `tests/document.rs`).

## Next

Empty for settled product work. Deferred / out (not blocking):

- Named styles / images dictionary
- Flow canvas; qr / svg / toc
- In-doc jumps; image-only links; `Layer::text_at` URI
- Electronic seal follows flow; per-page header/footer off
- Second PDF backend; `use krilla` in layout
- Open PR when remote and token exist
- Optional: write `Inch`/`HalfInch` into 提案书「可再钉」list as now fixed
