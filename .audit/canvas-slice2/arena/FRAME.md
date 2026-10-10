# Arena frame: Canvas / RecordingCanvas (slice 2)

## Artifact

Each candidate writes a design package under its output path:

- `SKETCH.md` — types, signatures, module map, `not implemented` bodies
- `RATIONALE.md` — shaped per architect rationale-template (Problem, Usage, Shape, Tradeoffs, Alternatives, Open questions, Next step). Leave Synthesis decision empty.

## Task (identical for every runner)

Design slice 2 of quire-pdf at `/mnt/data/demo/quire` (baseline branch `feat/geom-length-types`, HEAD has geom+error done).

Implementable shape for:

1. Public `Canvas` trait (no krilla)
2. `RecordingCanvas` + ordered command log for tests
3. Supporting types needed by the trait (`GlyphRun`, `GlyphPlacement`, `ImageId`, `DecodedImage`, and whatever `DocumentInfo` / `FontId` placement you choose)
4. Minimal `Error` additions only if RecordingCanvas must return them
5. Unit tests that assert command order and coordinates (0.01 pt abs tolerance), not PDF bytes

Hard constraints (do not reopen):

- Top-left origin, y down, pt
- Depend only on existing geom + error
- No backend, fonts measurement, line break, layout, document DSL builders
- Flow-inline canvas element is deferred (提案 §2.14)
- `publish = false`, crate name `quire-pdf`
- Proposal > AGENTS > design draft; design Canvas section is technical fact, but length brands from slice 1 replace design's bare f32

Grounding: `/mnt/data/demo/quire/.audit/canvas-slice2/how.md`
Design Canvas excerpt lives in `docs/quire-design.md` around the `### Canvas` heading.
Existing geom API: `src/geom.rs`, `src/error.rs`, `src/lib.rs`, stub `src/canvas.rs`.

Produce structurally opinionated designs. Do not converge on a safe middle. Name what you rejected.

## Rubric (picker only; candidates do not see scores)

1. **Boundary purity** — no krilla leakage; layout can depend on canvas; canvas does not depend on layout/document builders
2. **Domain model** — paint/page lifecycle encoded in types (command enum or equivalent), not scattered bools
3. **Geom reuse** — uses branded `Size`/`Rect`/`Path`/`Paint`/`Stroke`; no parallel f32 geometry
4. **Minimal surface** — no DSL builders, no unused Error flood, no flow-canvas element
5. **Testability** — RecordingCanvas supports ordered assertions with 0.01 pt tolerance
6. **Laziness** — smallest module split that keeps ownership clear; no pass-through layers
