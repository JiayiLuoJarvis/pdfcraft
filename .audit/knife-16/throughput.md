# Knife 16 throughput checkpoint

**After how.** Grounding in `.audit/knife-16/how.md` and `data-shape.md`.

## Blocking first steps

1. Arena design package must land before production code.
2. Autonomy defaults in `arena/frame.md` bind open product calls.

## Independent workstreams

After synthesis, one implementer owns the knife. Internal serial order:

1. `CellContent::Spacer` + `Cell::spacer` + `Table::lower` arm
2. Layout unit allow unwrap (optional) + public `document` test pinning neighbor y
3. Too-tall cell spacer error test if cheap

No parallel writers on `src/document.rs`.

## Shared mutable state

| Shared | Rule |
| --- | --- |
| Feature branch cut from `16749da` | Single implementer after arena |
| Arena candidate dirs | Each runner writes only its `candidate-N/` |
| Synthesis | Parent grafts into `arena/synthesis/` after pick |

## Smallest safe decomposition

One worker for implementation. Layout already allows Spacer. Splitting DSL and tests across writers adds merge cost without parallelism.
