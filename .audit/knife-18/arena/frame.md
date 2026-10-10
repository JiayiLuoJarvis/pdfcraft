# Knife 18 arena frame

**Artifact.** Design package for §7 optional margin presets. Usage, types, module map, tests, rationale. No production code in candidates.

**Baseline.** `feat/margin-presets` from `feat/cell-rich` tip `9b939b8`.

**Grounding.** `.audit/knife-18/how.md`, `data-shape.md`, `docs/提案书.md` §7.

**Out.** Changing default margins away from 0; asymmetric presets unless justified; header/footer band redesign; paper size changes (knife 19).

## Rubric

1. **Proposal fidelity.** Default stays four zeros. Presets are sugar over four `Length<NonNegative>`. Examples include ~72 pt and ~36 pt uniform sets.
2. **Caller surface.** Small public API: enum or associated consts that become `Margins`. Works with `doc.margins(...)` / `section.margins(...)`.
3. **Honesty.** Top/bottom remain header/footer band heights. Presets do not invent a second margin type.
4. **Fit to seams.** Live in `geom` (or document thin re-export). Reuse `Margins::uniform` / `new`. No layout/backend edits.
5. **Testability.** Unit or document test: preset equals `Margins::uniform(NonNegative::pt(N))`; default still zero.
6. **Laziness.** Two or three presets enough. No mm sugar unless trivial. No CHANGELOG requirement beyond rustdoc.

## Runners

| N | Model | Path |
| --- | --- | --- |
| 1 | composer-2.5-fast | `candidate-1/` |
| 2 | gpt-5.6-sol-high | `candidate-2/` |
| 3 | grok-4.7-high | `candidate-3/` |
| 4 | gemini-3.8-flash-high | `candidate-4/` |

Cross-judge: `gpt-5.6-sol-high`.

## Autonomy defaults

| Call | Default | Reverse |
| --- | --- | --- |
| Shape | `enum MarginPreset { Inch, HalfInch }` → `From`/`into_margins` → `Margins::uniform` | `assoc-consts` |
| Values | 72 pt and 36 pt exact | `mm-round` |
| Names | `Inch` / `HalfInch` (doc: ≈1 in / 0.5 in) | `Normal`/`Narrow` |
| Extra presets | none this knife | `add-quarter` |
| Default | unchanged `Margins::default()` zeros | `default-inch` |
| Call site | `doc.margins(MarginPreset::Inch.into())` or `Margins::from(preset)` | `doc.margin_preset(...)` |
