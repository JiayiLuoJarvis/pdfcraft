# Arena frame: knife 5 line break shapes

## Artifact

One design package per candidate: usage call sites, type sketch + signatures, module map, rationale (rationale-template.md). Write under the assigned candidate dir as `design.md`.

## Constraints (fixed by提案书/设计稿; do not redesign the algorithm)

- Pin `unicode-segmentation = "=1.13.3"`.
- Modules: `src/text/{cluster,break_class,line_break}.rs` (stubs exist).
- Classes: BK/SP/ID/AL/OP/CL/Other; pair table in design §断行; not full UAX #14.
- Algorithm steps 1–9 in design; empty paragraph → one empty line.
- Add `Error::UnbreakableWiderThanFrame { width, avail }` when needed.
- Tests: monospace stub (BMP scalar = 10 fu, upem 1000). Do not paginate / DSL / table.
- Do not rewrite `measure.rs` to clusters unless the sketch proves line_break cannot stay pure without a thin atom type.
- Knife 4 measure stays per-scalar; cluster width for break tests comes from the stub.
- Alignment Start/Center/End may land later (flow); justify never in v1. Prefer leaving alignment out of knife 5 unless it is a pure post-break helper with zero layout coupling.

## Rubric

1. **Usage-first:** call sites for tests and future paragraph flow are clear before types.
2. **Table as data:** break pair matrix is a lookup, not scattered matches.
3. **Pure break:** `line_break` has no skrifa/krilla; widths enter as numbers on atoms.
4. **Minimal Error surface:** only variants the algorithm needs now.
5. **Testability:** every case in design 测试策略断行 bullet is expressible.
6. **Laziness:** no measure rewrite, no Flowable, no public DSL.

## Grounding

See `.audit/line-break/how.md` once present; also `docs/quire-design.md` 断行 + PR7, `docs/提案书.md` §3.
