# Knife 14 throughput checkpoint

**After how.** Grounding in `.audit/knife-14/how.md` and `data-shape.md`.

## Blocking first steps

1. Arena design package (API + InlineItem + break segment tape + LineBox stroke) must land before any code.
2. Autonomy defaults below bind open product calls so runners do not invent divergent defaults.

## Independent workstreams

After synthesis, one implementer owns the knife. Internal serial order:

1. Types + DSL assemble (`InlineItem`, blank builders, fill-after error)
2. Breaker atoms + segment-aware `BrokenLine` / side tape
3. `layout_runs` + `LineBox` stroke geometry + `fragment_from_lines`
4. RecordingCanvas tests

No parallel writers on `src/layout/paragraph.rs` or `src/text/line_break.rs`.

## Shared mutable state

| Shared | Rule |
| --- | --- |
| `feat/style-builders` branch | Single implementer after arena |
| Arena candidate dirs | Each runner writes only its `candidate-N/` |
| Synthesis | Parent grafts into `arena/synthesis/` after pick |

## Smallest safe decomposition

One worker for implementation. The breaker and paragraph layout share one atom/segment model. Splitting would force a fake intermediate API. Arena already explores the shape before that worker starts.
