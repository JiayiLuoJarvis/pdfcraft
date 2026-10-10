# Knife 17 throughput checkpoint

**After how.** Grounding in `.audit/knife-17/how.md` and `data-shape.md`.

## Blocking first steps

1. Arena must lock the list↔Flow seam (defer-to-lower vs Table child Flow) and whether to split 17a/17b before code.
2. Autonomy defaults in `arena/frame.md` bind open calls.

## Independent workstreams

After synthesis, one implementer. Internal serial order:

1. `Cell::paragraph` + `CellContent` arm + `Table::lower` (reuse `Paragraph` → `TextNode::from_items`)
2. List seam from arena + `Cell::ul` / `Cell::ol` (or 17b if split)
3. RecordingCanvas tests: bold, blank (cell width), link, list marker; forbidden kids stay covered by existing layout tests

No parallel writers on `src/document.rs`.

## Shared mutable state

| Shared | Rule |
| --- | --- |
| Branch cut from `feat/cell-spacer` tip | Single implementer after arena |
| Arena candidate dirs | Each runner writes only `candidate-N/` |
| Synthesis | Parent grafts into `arena/synthesis/` |

## Smallest safe decomposition

One implementer. Paragraph and list share `Table::lower` / `CellContent`. If arena chooses 17a/17b, two sequential commits on one branch still beat two writers.
