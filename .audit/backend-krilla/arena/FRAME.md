# Arena frame: knife 3 backend-krilla design sketch

## Artifact
One candidate design package (usage + shape + rationale) for implementing the private krilla Canvas adapter on branch feat/backend-krilla.

## Grounding (must honor)
- how explainer findings: Canvas trait already shipped; backend placeholders empty; branded geom (not design draft f32 fields); LinkTarget validates empty URI; Error is Copy today; self-referential Document/Page/Surface cannot live in one struct under forbid(unsafe_code); draw_glyph_run/draw_image need a non-panic behavior this knife; design PR4 content-stream + link experiments are mandatory tests.
- AGENTS.md: krilla only in backend/; identity geometry; no draw_text; no simple-text/rayon; pin krilla=0.8.2 default-features=false + raster-images; publish=false stays.
- 提案书 confirmed: krilla only backend; coords top-left y-down; measure≠draw_text. No document DSL this knife.
- Design docs/quire-design.md is technical facts source; unused draft geometry field access must be adapted to Length brands.

## Rubric (picker only; candidates do not see)
1. Honors Canvas trait + sealed krilla (no krilla types in lib re-exports).
2. Solves Document/Page/Surface borrow without unsafe / without changing Canvas trait.
3. Minimal Error/Cargo surface for this knife only; defers fonts/images properly without panic stubs.
4. Tests match design: %PDF, identity convert, observed path ops, observed Annot rect; no full PDF snapshot.
5. Interface depth: complexity inside KrillaCanvas; crate public API barely grows (feature + maybe nothing else).
6. Two candidates must be structurally distinct (not flavor variants).

## Models
- candidate-1: claude-opus-5-thinking-high
- candidate-2: gpt-5.6-sol-high
- candidate-3: grok-4.7-high
