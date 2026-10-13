# Knife 15 throughput checkpoint

**After how.** Grounding in `.audit/knife-15/how.md` and `data-shape.md`.

## Blocking first steps

1. Arena design package (DSL + TextRun link + prepare_inline flush + LinePiece/FragmentCommand::Link + Band slot URI) must land before any production code.
2. Autonomy defaults below bind open product calls so runners do not invent divergent defaults.

## Independent workstreams

After synthesis, one implementer owns the knife. Internal serial order:

1. Types + DSL assemble (`TextRun.link`, Paragraph link API, coalesce key, Band slot helpers, EmptyLink at construct)
2. `prepare_inline` flush-on-link + group-side `Option<LinkTarget>`
3. `place_line` / `fragment_from_lines` + `FragmentCommand::Link` + `with_offset` / `draw_fragment` / `first_baseline`
4. RecordingCanvas tests (wrap → two rects, empty URI, band slot if in scope) + `%PDF` smoke

No parallel writers on `src/layout/paragraph.rs` or `src/document.rs`.

## Shared mutable state

| Shared | Rule |
| --- | --- |
| `feat/inline-blank` branch (or knife-15 feature branch cut from it) | Single implementer after arena |
| Arena candidate dirs | Each runner writes only its `candidate-N/` |
| Synthesis | Parent grafts into `arena/synthesis/` after pick |

## Smallest safe decomposition

One worker for implementation. The URI rides the same group/seg model as blanks. Splitting DSL and layout across two writers would force a fake intermediate API. Arena explores the shape first.
