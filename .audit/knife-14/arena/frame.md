# Knife 14 arena frame

**Artifact.** Design package for §2.2 fill-in underscore (inline blank). Deliver usage sketch, types, module map, layout algorithm, tests, rationale. No production code in candidates.

**Baseline.** `feat/style-builders` @ `5deb0df` (feature tip `aaf14bf`).

**Grounding.**

- `docs/提案书.md` §2.2 (and §2.1 for `W` / `W'`, §2.0 for lengths/colors)
- `.audit/knife-14/how.md`
- `.audit/knife-14/data-shape.md`
- Explorers under `.audit/knife-14/how/`

**Out of knife.** canvas / qr / svg / toc / named styles / §8 URI links / `Cell::spacer` / text `decoration: underline` / block `HLine` redesign / Cell·Band·Layer rich runs.

## Rubric (picker grades these)

1. **Proposal fidelity.** Fixed XOR fill-to-EOL; unbreakable fixed blank; fill forbids later runs at assemble; blank not glyph-measured; one Stroke; no line-height growth; rides its line across pages. Not a `FlowNode::HLine`.
2. **Caller surface.** Reads like §1-style DSL beside `p.text` / `p.bold`. Illegal widths unrepresentable or fail at assemble/layout with existing error variants where they fit. Public API small.
3. **Layout honesty.** Blank width in the break stream; middle blank shifts following glyphs; fill width = `avail - line_width` inside pack; first-line budget is `W'` unless the design explicitly argues otherwise against §2.1. Stroke y = baseline + offset. Failures before `start_page` when possible.
4. **Fit to seams.** Extends paragraph run list + breaker + `LineBox` / fragment. Reuses `FragmentCommand::Stroke` / geom Stroke types. Does not invent a parallel style system.
5. **Testability.** RecordingCanvas can pin `StrokePath` endpoints, glyph x after a blank, line height unchanged, fill-after-run assemble error, fixed-too-wide `UnbreakableWiderThanFrame`. backend-krilla stays `%PDF` smoke.
6. **Laziness.** No underscore glyphs, no named styles, no Cell blank this knife, no dash/double line, no overlay text on blank.

## Runners

Paths under `.audit/knife-14/arena/`:

| Candidate | Model | Output |
| --- | --- | --- |
| 1 | composer-2.5-fast | `candidate-1/` |
| 2 | gpt-5.6-sol-high | `candidate-2/` |
| 3 | grok-4.7-high | `candidate-3/` |
| 4 | gemini-3.8-flash-high | `candidate-4/` |

Cross-judge: pick from pool, prefer family ≠ parent (composer). Default seat: `gpt-5.6-sol-high`.

## Autonomy defaults (if proposal silent)

| Call | Default | Reverse word |
| --- | --- | --- |
| Stroke thickness | `1pt` (`Length<Positive>`) | `thickness-half` |
| Baseline offset | `1pt` below baseline (y-down) | `offset-half-thickness` |
| Stroke color default | `style.color` | `always-black` |
| Fixed span type | `Length<NonNegative>` (0 allowed; omit zero-length stroke) | `positive-span` |
| Overflow budget | visual-line budget (`W'` on line 0) | `full-frame-W` |
| Fixed blank wrap | break allowed immediately before blank (whole blank moves); never `PairClass::Sp` | `glue-to-prev` |
| `W_remain == 0` | omit stroke, still consume 0 width | `error-on-zero-fill` |
| Han sugar | `blank_em(n)` → `n * style.font_size` at layout | `close-over-builder-size` |
| Fill API | dedicated method (e.g. `blank_to_end`) distinct from fixed | `one-builder-enum` |
| Cell / Band blank | out | `cell-blank-in` |
