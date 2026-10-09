# Knife 13 arena frame

**Artifact:** design package for style builders + `p.bold` + first-line indent (architect sketch: usage, types, module map, rationale).

**Baseline:** `feat/style-builders` @ `f208329` (from `feat/hline`).

**Grounding (Phase A how):** `.audit/knife-13/how/` (explorers + `how.md`).

## Rubric (picker grades these)

1. **Proposal fidelity.** Matches 提案书 §1 `p.bold`, §2.1 `W' = W − indent`, §2.0 typed lengths; does not invent named-styles / italics / underline / §2.2 blank this knife unless clearly justified as required scaffolding.
2. **Caller surface.** README-style usage feels like the §1 template; public API small; illegal lengths unrepresentable at the boundary.
3. **Layout honesty.** First-line indent only affects first line break width and glyph x; mixed bold runs measure with correct FontId; failures before `start_page`.
4. **Fit to existing seams.** Extends `Style` / `TextNode` / `Paragraph` / `break_lines` without a parallel style system; Band/Layer/Cell text stay coherent or explicitly deferred with reason.
5. **Testability.** RecordingCanvas can pin glyph FontId / origin x for bold + indent; backend-krilla smoke stays `%PDF`.
6. **Laziness.** No synthetic bold, no full StyleOverride+named map unless needed for this knife's caller story.

## Runners

Paths under `.audit/knife-13/arena/`:

| Candidate | Model | Output |
| --- | --- | --- |
| 1 | composer-2.5-fast | `candidate-1/` |
| 2 | gpt-5.6-sol-high | `candidate-2/` |
| 3 | grok-4.7-high | `candidate-3/` |
| 4 | gemini-3.8-flash-high | `candidate-4/` |

Cross-judge: gpt-5.6-sol-high (different family from parent if parent is composer; pool pick).

## Autonomy defaults (if proposal silent)

- Bold face = second registered font (`FontId(1)`), exposed as `Document::bold_font` or `font` returns ids / second `font` call binds bold. Reverse: `bold-is-same-face`.
- First-line indent default = 0 (no automatic 二字). Reverse: `default-two-em`.
- Named styles / defaultStyle map **out of knife** unless a candidate proves they are required for `p.bold`. Reverse: `named-styles-in`.
