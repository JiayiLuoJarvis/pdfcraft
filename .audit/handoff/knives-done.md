# Handoff: knives 7–17

**Date:** 2026-10-10  
**Repo:** /mnt/data/demo/quire  
**Exit predicate (tonight):** knives 16–19 green locally (no remote/PR).

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
| 16 Cell::spacer | feat/cell-spacer | c9c2af7 (status/audit a5ba3fb) |
| 17 cell paragraph + list | feat/cell-rich | c3035df |

Knife 17 stacks on `feat/cell-spacer` @ `a5ba3fb`. Feature tip `c3035df`. Design trail under `.audit/knife-17/`.

## What callers get (knife 17)

`Cell::paragraph(|p| …)` runs immediately and reuses body `Paragraph` (text/bold/blank/link). Shared `lower_paragraph` emits unwrapped `FlowNode::Text` so wrap width is the cell inner width.

`Cell::ul` / `Cell::ol` store a `'static` `FnOnce` and run it at `Table::lower` with `flow.child()`. Nested item lists work. Move owned data into the closure.

`Cell` no longer implements `Clone` / `PartialEq` / `Eq` (keeps `Debug`). Tall cell content still fails with `UnsplittableTallerThanFrame`. Forbidden kids unchanged.

## Gates (parent re-verified 2026-10-10)

On `feat/cell-rich` @ `c3035df`: `cargo fmt --check`, `clippy -D warnings`, `cargo test` green (198 lib + 1 ignored, 76 `tests/document.rs`).

## Next

- Knife 18: §7 margin presets (optional sugar; default still 0)
- Knife 19: confirm `quire-pdf` naming leftovers; A-series gaps only if small
- Named styles / flow canvas still out
- Open PR when remote and token are available
