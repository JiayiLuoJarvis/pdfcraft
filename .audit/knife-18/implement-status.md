# Knife 18 implement status

**Branch.** `feat/margin-presets`
**Baseline.** `9b939b8` (`9b939b8d6394efd330e3006946b3e99e6aaeb7da`)
**Geom tip.** `e080a8e` (`e080a8e74c9511d40678b5cae3a6071747048d86`)
**Implementation tip.** `3c9e3da` (`3c9e3da695bca2b3ebabef6911cad354dc65afd1`)  
**Parent re-verify.** Gates re-run on `3c9e3da` after implementer return. Exit 0.  
**Date.** 2026-10-10

## Commits

- `e080a8e74c9511d40678b5cae3a6071747048d86` `feat(geom): MarginPreset Inch and HalfInch`
- `3c9e3da695bca2b3ebabef6911cad354dc65afd1` `test(document): margin presets match uniform insets`

No push.

## Gates

First run on `3c9e3da` by implementer. Parent re-ran the same three. Each command exited 0.

- `cargo fmt --all`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`. 200 library tests passed, 1 ignored. 78 tests in `tests/document.rs` passed. 0 doctests.

## Tests added

- `inch_margin_preset_is_uniform_72pt`
- `half_inch_margin_preset_is_uniform_36pt`
- `default_margins_are_zero` still passes. `Margins::default()` is four zeros.
- `margin_presets_match_uniform_insets`
- `inch_preset_on_a_narrow_page_is_invalid_geometry`

## What shipped

`MarginPreset` lives in `src/geom.rs` and is re-exported next to `Margins`. `Inch` is uniform 72 pt. `HalfInch` is uniform 36 pt. Both use private `Length::from_trusted_pt` and `Margins::uniform`. `From<MarginPreset> for Margins` calls `into_margins`. The enum has no `Default`. `Document::margins` and `Section::margins` still take `Margins`.

## Deviations

- The document test also pins `HalfInch` and the body y (`72 + BASELINE`, `36 + BASELINE`). The locked pin was body glyph x near 72 for `Inch`.
- The narrow-page test is the optional `InvalidGeometry` case. Page size is 100 by 200 pt, so 72 plus 72 is not a positive body width.
- Section-scoped presets, full canvas equality, and a header/footer band recording were left out. The synthesis rejected that larger matrix.
- This status file is untracked, with the rest of `.audit/knife-18/`.
