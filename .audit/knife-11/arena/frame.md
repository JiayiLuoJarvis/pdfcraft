# Knife 11 arena frame

## Artifact

One design package per candidate under `.audit/knife-11/arena/candidate-N/` (`design.md` with usage + type sketch + rationale sections per architect rationale template). Synthesized result at `.audit/knife-11/arena/synthesis/design.md`.

## Task (shared prompt body)

Design knife 11 for quire-pdf at `/mnt/data/demo/quire` on branch `feat/layer-path` (tip `9eda8da`).

Authority: `docs/提案书.md` §2.12 (path on behind/front), plus §2.10 stack/full-width, §2.11 section scope, §2.0 length/color, §2.7 image_at sources. AGENTS.md. Design doc only for krilla Surface transform/path/opacity facts.

Goal: expose path primitives on `Layer` (behind/front closures): at least `line` / `rect` / `ellipse` with stroke and/or fill; support rotation and semi-transparent colors; run per page; keep section primary + doc sugar; RecordingCanvas asserts paths/order; backend-krilla smoke `%PDF` only.

Do not: flow hline, flow canvas, parallel watermark API, style builders, fit/cover, header full-width rewrite, overturn knife 8/10 choices.

Grounding: read `.audit/knife-11/how.md` (and explorers if needed), plus current `Layer` / `LayerItem` / `measure_layer` / `FragmentCommand` / `Path` / Canvas / `to_fill`/`to_stroke`.

Must decide structurally (not point tweaks):

1. How ellipse enters `Path` (no CurveTo today; krilla PathBuilder has cubic/quad).
2. How rotation is applied (bake into path vertices vs Canvas `push_transform`/`pop`).
3. How fill joins the paint pipeline (`FragmentCommand::Fill` vs other).
4. Public Layer method shapes (builder vs stroke_/fill_ pairs vs paint enum).

Write usage call sites first, then types. Screen against architect design-red-flags. Structurally distinct from the other candidate.

## Rubric

1. **Proposal fidelity.** §2.12 primitives + rotation + opacity; section/doc sugar preserved; stack order unchanged.
2. **Seam fit.** Extends `LayerItem` → `measure_layer` → `FragmentCommand` → existing Canvas fill/stroke; opacity via Color alpha already in backend.
3. **Interface depth.** Small Layer surface; complexity (ellipse construction, rotation) hidden in geom/layout.
4. **Diff cost / laziness.** Avoid Canvas trait growth unless rotation bake is worse; no parallel watermark or flow canvas.
5. **Testability.** RecordingCanvas can assert concrete paths/rects/ellipses and layer order; failures before `start_page`.
6. **Illegal states.** Degenerate geometry rejected at branded constructors where §2.0 applies; no silent clip.

## Runners

- candidate-1: `claude-opus-5-5-medium` → `.audit/knife-11/arena/candidate-1/design.md`
- candidate-2: `grok-4.7-high` → `.audit/knife-11/arena/candidate-2/design.md`

(Configured pstack xhigh/max slugs unavailable in this session; closest listed family defaults.)

## Cross-judge

After candidates land, one readonly judge (prefer other family than parent) scores rubric and recommends a base.
