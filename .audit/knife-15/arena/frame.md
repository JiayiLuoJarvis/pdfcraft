# Knife 15 arena frame

**Artifact.** Design package for §8 text external URI links. Deliver usage sketch, types, module map, layout algorithm (group flush → per-line rect → annotate), tests, rationale. No production code in candidates.

**Baseline.** `feat/inline-blank` @ `a232ce1`.

**Grounding.**

- `docs/提案书.md` §8 (and §2.0 empty-URI newtype spirit)
- `.audit/knife-15/how.md`
- `.audit/knife-15/data-shape.md`
- Explorers under `.audit/knife-15/how/`

**Out of knife.** canvas / qr / svg / toc / named styles / `Cell::spacer` / in-doc jumps / image-only links / Layer `text_at` links / link visual decoration (underline/color).

## Rubric (picker grades these)

1. **Proposal fidelity.** Text URI only; write string, no fetch, no scheme check; empty rejected at construct via `LinkTarget`; one hit rect per visual-line slice of linked text; body + header/footer if same pipeline; no `linkToPage` / destination.
2. **Caller surface.** Reads like §1-style DSL beside `p.text` / `p.bold`. Public API small. Illegal empty URI unrepresentable after construct. Band gets a URI entry if body Paragraph alone cannot reach slots.
3. **Layout honesty.** Assemble coalesce keys on face **and** link. `prepare_inline` flushes on link change (not only blank). Rect x from `place_line` after align; y from line-box top (`y0`), height = `line_height`; omit zero width. Bold/body inside one link stays one rect per line. Failures before `start_page` when possible.
4. **Fit to seams.** Reuses `LinkTarget`, `Canvas::annotate_link`, backend queue. Adds `FragmentCommand::Link` (or equivalent) with `with_offset` / `draw_fragment` arms. Does not invent a parallel annotation path or krilla use outside `backend/`.
5. **Testability.** `RecordingCanvas` can pin `AnnotateLink` rects (single line, wrap → two rects), empty URI `EmptyLink`, no decoration side effects. backend-krilla stays `%PDF` smoke (URI string presence optional if already covered by emit probe).
6. **Laziness.** No new PDF backend; no image link; no Layer link; no named styles; no underline auto-style; no Cell rich paragraph unless required for a stated Band/body gap.

## Runners

Paths under `.audit/knife-15/arena/`:

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
| DSL shape | `p.link(uri, \|inner\| { … })` sets URI on text/bold pushed inside | `chain-last` |
| Band URI | `Band::{left,right}_link(uri, text)` whole-slot | `band-paragraph` |
| Blank under link | blanks stay unlinkable; blank ends a linked group | `blank-no-link` |
| Link visual style | no underline/color change | `no-decoration` |
| Hit rect height | full `line_height` (body metrics) | `line-box-height` |
| Adjacent same-URI runs | coalesce when face and URI match | `coalesce-same-uri` |
| Zero-width / empty text | omit annotate; empty text ignored as today | `skip-empty` |
| Layer `text_at` / image / Cell string link | out | `layer-out` |
| Cell / table-string URI | out this knife (paragraph + band only) | `cell-link-in` |
