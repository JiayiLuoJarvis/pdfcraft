# Arena frame: knife 6 flow/paginate

## Artifact

One design package per candidate under `.audit/flow-paginate/arena/candidate-N/design.md`:
usage sketch (crate-internal call sites for tests), type sketch + signatures with `not implemented` / TODO bodies, module map, rationale (alternatives considered and rejected).

## Constraints (authority)

- `docs/提案书.md` 确认总表 + §2.1 / §2.5.1 / §2.9 / §2.13. Design `quire-design.md` PR8+PR9 for tech shape only.
- Internal only. No public Document builders. No table / columns / list / decorate.
- Wire `ClusteredText` + `break_lines` + `measure_text(glyph_text)` + `line_metrics`. Align Start/Center/End. No Justify.
- PageBreak three-state cursor. Drop `spacing_after` when it does not fit. Spacer splittable. KeepTogether atomic vs frame.
- Tests assert `RecordingCanvas` glyph origins / page count. Prefer fixture font.
- Add `Error::UnsplittableTallerThanFrame` when needed. Do not invent LimitExceeded / InvalidCellContent unless a test needs them this knife.

## Rubric

1. **Domain fit.** PageCursor is a state machine; Split is a consuming enum; illegal states unrepresentable.
2. **Boundary.** layout never `use krilla`; styles/lengths validated at construction; measure/break reused not forked.
3. **Knife scope.** Ships paragraph + stack + spacer + pageBreak + keepTogether + paginate; stops before table/columns/list/decorate/DSL.
4. **Testability.** RecordingCanvas assertions for baseline y and Start/Center/End x; four PageBreak trajectories named in design.
5. **Laziness.** Smallest crate-internal surface; no public builders; no one-caller wrappers.

## Runners

- candidate-1: `composer-2.5-fast` → `.audit/flow-paginate/arena/candidate-1/`
- candidate-2: `grok-4.7-high` → `.audit/flow-paginate/arena/candidate-2/`

Forced structural divergence (not point fixes):

- **candidate-1:** Design-faithful. `FlowNode::split(self, …) -> Split` with `Fragment` + `FragmentCommand::Glyphs`, `draw_fragment`, `PageCursor` inside `paginate`.
- **candidate-2:** Two-phase. Paragraph lays out into owned `LineBox` / height units first; paginator consumes a flatter `FlowItem` list without FragmentCommand indirection (paint happens at page emit).
