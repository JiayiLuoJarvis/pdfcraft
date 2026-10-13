# Handoff: knives 7–15

**Date:** 2026-10-10  
**Repo:** /mnt/data/demo/quire  
**Exit predicate:** knife 15 green locally (no remote/PR; `gh` token invalid, no `git remote`).

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

Knife 15 stacks on `feat/inline-blank` @ `a232ce1`. Feature tip `16749da`. Status commit `2269a11`. Design trail under `.audit/knife-15/` (how / arena / synthesis / implement-status).

## What callers get (knife 15)

`Paragraph::link(uri, |p| { … })` stamps `text` / `bold` inside the closure with one `LinkTarget`. Empty URI records `Error::EmptyLink` on the builder and skips the closure. `Band::left_link` / `right_link` attach a whole-slot URI. After break, each visual-line slice of linked text paints one `AnnotateLink` (line-box height, fragment-local then offset). No fetch, no scheme check, no auto underline/color. Cell strings, `Layer::text_at`, and images stay unlinkable.

## Gates (parent re-verified 2026-10-10)

On `feat/text-uri-link` @ `16749da`: `cargo fmt --check`, `clippy -D warnings`, `cargo test` green (197 lib + 1 ignored, 65 `tests/document.rs`). `backend-krilla` default; same suite. `text_link_writes_a_pdf` asserts `%PDF`.

## Next

- `Cell::spacer` (flow `spacer` already exists)
- Named styles / flow canvas still out
- Open PR when remote and token are available (`feat/text-uri-link` → stack base as desired)
