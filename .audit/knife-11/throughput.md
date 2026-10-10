# Knife 11 throughput checkpoint

**Branch:** `feat/layer-path` from `9eda8da`  
**Date:** 2026-10-09

## Blocking first steps

- Tip `9eda8da` confirmed; branch cut.
- Seven gates green on baseline before design fan-out (measured 2026-10-09).

## Independent workstreams

1. **how** explorers (readonly, parallel): document DSL / layout decorate+flow / geom+canvas+krilla → explainer `how.md`.
2. **arena** design candidates (write only under `.audit/knife-11/arena/candidate-*`).
3. **implement** after synthesis (single owner on `src/` + `tests/`).

## Shared mutable state

- `src/` and `tests/` have one writer after synthesis.
- Audit artifacts are per-candidate directories; merge only at synthesis/handoff.
- No concurrent commits to the same branch tip during arena.

## Smallest safe decomposition

One implement owner after arena synthesis. Path/rotation forks need a single coherent pick before touching `FragmentCommand` and `Layer` together. Splitting geom helpers vs DSL wiring mid-wave risks a Fill-less Stroke-only half state.
