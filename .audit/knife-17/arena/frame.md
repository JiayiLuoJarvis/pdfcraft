# Knife 17 arena frame

**Artifact.** Design package for public cell rich paragraph + list (§2.6 / §2.8). Usage sketch, types, module map, lower notes, tests, rationale. No production code in candidates.

**Baseline.** Stack on `feat/cell-spacer` tip (see git log; include knife 16).

**Grounding.**

- `docs/提案书.md` §2.6 cell kids, §2.8 list (格内允许)
- `.audit/knife-17/how.md` (explainer in flight; explorers under `how/`)
- `.audit/knife-17/data-shape.md`

**Out of knife.** Cell columns / pageBreak / keep_together / nested table; multi-kid public cell unless required; named styles; flow canvas; qr/svg/toc; changing `natural_height` to allow mid-cell Partial.

## Rubric

1. **Proposal fidelity.** Cell paragraph (incl. blank; link if same pipeline); cell list if matrix allows; forbidden kids stay forbidden.
2. **Caller surface.** Reuses `Paragraph` and `List` APIs. Reads like body `paragraph` / `ul`/`ol` beside `Cell::hline`. No parallel rich-text builder.
3. **Layout honesty.** Reuses allowlist. Documents `natural_height` fit-only. List item invalid content still `InvalidCellContent` with list-item container name when nested.
4. **Fit to seams.** Prefer deferred lower or smallest Flow bridge. No krilla outside backend. No second PDF backend.
5. **Testability.** `document().pages()` pins bold font ids, blank strokes (cell-width aware), link rects, list marker x; reject forbidden if exposed. Prefer RecordingCanvas over PDF bytes.
6. **Laziness.** If list Flow wiring is large, propose 17a paragraph / 17b list split with clear commits. Minimal new types.

## Runners

| Candidate | Model | Output |
| --- | --- | --- |
| 1 | composer-2.5-fast | `candidate-1/` |
| 2 | gpt-5.6-sol-high | `candidate-2/` |
| 3 | grok-4.7-high | `candidate-3/` |
| 4 | gemini-3.8-flash-high | `candidate-4/` |

Cross-judge default: `gpt-5.6-sol-high`.

## Autonomy defaults

| Call | Default | Reverse |
| --- | --- | --- |
| Paragraph API | `Cell::paragraph(\|p\| { … })` like Flow | `items-only` |
| Keep `Cell::new(str)` | plain text path unchanged | `rich-new` |
| Link / blank / bold | same Paragraph methods | `text-only` |
| List API | `Cell::ul` / `Cell::ol` mirroring Flow | `list-closure-on-row` |
| List Flow seam | defer list builder closure until `Table::lower(&Flow)` | `table-child-flow` |
| Split knife | ship both if design ≤ ~one evening; else 17a then 17b | `force-both` |
| Multi-kid cell | out | `stack-cell` |
| Too tall | `UnsplittableTallerThanFrame` | `split-in-cell` |
