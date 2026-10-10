# Handoff: knives 7–14

**Date:** 2026-10-10  
**Repo:** /mnt/data/demo/quire  
**Exit predicate:** knife 14 green locally (no remote/PR; `gh` token invalid, no `git remote`).

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

Knife 14 stacks on `feat/style-builders` @ `5deb0df`. Design trail under `.audit/knife-14/` (how / arena / synthesis / implement-status).

## What callers get (knife 14)

`Paragraph::blank(width)`, `blank_em(n)`, and `blank_to_end()` return a short-lived `Blank` for optional `thickness` / `color` / `baseline_offset`. Defaults are 1 pt thick, 1 pt below baseline, and the paragraph text color. Fixed blanks are unbreakable atoms; if the visual-line budget cannot hold them, layout returns `UnbreakableWiderThanFrame` (first line uses `W'`). Fill blanks resolve to `avail - line_width` in the breaker and must be the last non-empty run, or assembly fails with `RunAfterFillBlank` before `start_page`. Blanks do not enter glyph measurement. Each positive-width blank paints one horizontal `Stroke`. Line height stays the body face metrics. Cell, Band, and Layer stay without blank APIs.

## Gates (parent re-verified 2026-10-10)

On `feat/inline-blank` @ `b83712e`: `cargo fmt --check`, `clippy -D warnings`, `cargo test` green (183 lib + 1 ignored, 49 `tests/document.rs`). `backend-krilla` is a default feature; same suite. `inline_blank_writes_a_pdf` asserts `%PDF`.

## Next

- §8 external URI links on text
- `Cell::spacer` (flow `spacer` already exists)
- Named styles / flow canvas still out
- Open PR when remote and token are available (`feat/inline-blank` → stack base as desired)
