# Knife 16 arena frame

**Artifact.** Design package for public `Cell::spacer` (§2.6 / §2.13). Deliver usage sketch, types, module map (DSL + lower only if layout unchanged), tests, rationale. No production code in candidates.

**Baseline.** `feat/text-uri-link` feature tip `16749da`.

**Grounding.**

- `docs/提案书.md` §2.6 cell kids, §2.13 spacer
- `.audit/knife-16/how.md` (explainer in flight; explorers under `how/`)
- `.audit/knife-16/data-shape.md`

**Out of knife.** Cell rich paragraph / blank / link, cell list, multi-content cell stack API, canvas / qr / svg / toc / named styles, changing `natural_height` so cell spacers split across pages.

## Rubric (picker grades these)

1. **Proposal fidelity.** Cell may hold spacer; height non-negative (0 ok); no paint; columns / pageBreak / keep_together / nested table stay forbidden.
2. **Caller surface.** Reads like `Flow::spacer` beside existing `Cell::hline` / `Cell::image`. One height argument. No inventing a cell Flow closure unless required.
3. **Layout honesty.** Reuses `FlowNode::Spacer` / existing allowlist. Does not claim mid-cell page split. Documents that `natural_height` rejects Partial (taller than inner frame → `UnsplittableTallerThanFrame`).
4. **Fit to seams.** Only `CellContent` + `Table::lower` + `Cell::spacer` (+ tests). No second spacer type. No krilla outside backend.
5. **Testability.** Public `document().pages()` pins neighbor glyph y (or row gap) via `pt_near`; optional layout `cell_allows_spacer` unwrap; too-tall cell spacer errors; no fake paint ops from spacer alone.
6. **Laziness.** No Cell paragraph/list this knife. No multi-kid public cell. No change to body spacer split semantics.

## Runners

Paths under `.audit/knife-16/arena/`:

| Candidate | Model | Output |
| --- | --- | --- |
| 1 | composer-2.5-fast | `candidate-1/` |
| 2 | gpt-5.6-sol-high | `candidate-2/` |
| 3 | grok-4.7-high | `candidate-3/` |
| 4 | gemini-3.8-flash-high | `candidate-4/` |

Cross-judge: prefer family ≠ parent. Default seat `gpt-5.6-sol-high`.

## Autonomy defaults (if proposal silent)

| Call | Default | Reverse word |
| --- | --- | --- |
| API | `Cell::spacer(height: Length<NonNegative>)` | `space` |
| Zero height | allow | `reject-zero` |
| Multi-content cell | one content per Cell | `stack-cell` |
| Too tall for page body | `UnsplittableTallerThanFrame` (keep natural_height) | `split-in-cell` |
| Proof | neighbor text y shifted by spacer height | `page-count-only` |
