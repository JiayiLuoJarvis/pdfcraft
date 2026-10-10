# Handoff: knives 7–16

**Date:** 2026-10-10  
**Repo:** /mnt/data/demo/quire  
**Exit predicate (tonight):** knives 16–19 green locally (no remote/PR; `gh` token invalid, no `git remote`).

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
| 16 Cell::spacer | feat/cell-spacer | c9c2af7 (status 1d343c3) |

Knife 16 stacks on `feat/text-uri-link` @ `16749da`. Feature tip `c9c2af7`. Status commit `1d343c3`. Design trail under `.audit/knife-16/` (how / arena / synthesis / implement-status).

## What callers get (knife 16)

`Cell::spacer(height: Length<NonNegative>)` stores one spacer payload. `Table::lower` emits `FlowNode::Spacer`. Zero height is legal. The cell paints nothing. A spacer taller than the cell inner full frame fails with `UnsplittableTallerThanFrame` (unlike body `Flow::spacer`, which can split across pages). Still one payload per public cell. No cell paragraph or list yet.

## Gates (parent re-verified 2026-10-10)

On `feat/cell-spacer` @ `1d343c3`: `cargo fmt --check`, `clippy -D warnings`, `cargo test` green (198 lib + 1 ignored, 69 `tests/document.rs`).

## Next

- Knife 17: cell paragraph (blank/link if same pipeline) and cell list (§2.6); still forbid columns / pageBreak / keep_together / nested table
- Knife 18: §7 margin presets (optional sugar; default still 0)
- Knife 19: crate name `quire-pdf` (Cargo.toml already `quire-pdf`; verify remaining rename gaps) + small A-series gaps if any
- Named styles / flow canvas still out
- Open PR when remote and token are available
