# Handoff: knives 7–13 complete

**Date:** 2026-10-09  
**Repo:** /mnt/data/demo/quire  
**Exit predicate:** met locally (no remote/PR).

| Knife | Branch | Commit |
| --- | --- | --- |
| 7 table/columns/list/decorate | feat/table-columns-list-decorate | 6dbf40c |
| 8 document DSL | feat/document-dsl | 7cbe823 |
| 9 image | feat/image | a12b610 |
| 10 section | feat/section | 9eda8da |
| 11 layer path | feat/layer-path | 7cb1654 |
| 12 flow hline | feat/hline | c89f2c4 |
| 13 style / bold / first-line indent | feat/style-builders | 45c9a07 |

Knife 13 tip stacks on `f208329` (feat/hline tip with handoff hash note). Audit under `.audit/knife-13/` (how / arena / implement-status). Feature tip `aaf14bf`; full tip recorded in the next commit.

## What callers get (knife 13)

`Document::bold_font` binds a real second face after body `font`. `Paragraph::text` / `Paragraph::bold` keep ordered runs inside one paragraph. `Paragraph::first_line_indent(Length<NonNegative>)` defaults to 0. First visual line breaks at `W' = W − indent` and paints in box `[I, W]`. Continuation lines use full `W`. Empty bold slot is `UnknownFont { id: 1 }`. Missing bold glyphs fail the whole document before `start_page`. Cell, band, and layer text stay plain strings.

## Gates (parent re-verified 2026-10-09)

Seven commands green on `feat/style-builders` @ `aaf14bf`. With `backend-krilla`: 169 lib + 1 ignored, 40 integration. Without: 146 lib, 30 integration.

## Next (optional)

- §2.2 fill-in underscore
- Flow-level style builders / named styles
- flow canvas (§2.14)
- Distinct bold fixture with different advances
- Stack/rebase onto master if desired
- Open PR when remote/token available
