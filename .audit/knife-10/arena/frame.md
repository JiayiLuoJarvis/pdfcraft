# Arena frame — knife 10 section

**Branch:** `feat/section` @ `a12b610`  
**Artifact:** candidate design package (usage + types + module map + rationale)  
**Authority:** `docs/提案书.md` §2.10 / §2.11 / §2.12 / §7 + 确认总表 > AGENTS.md > design draft tech facts only.

## Task (identical for every runner)

Design multi-`section` for quire-pdf so that:

1. Public name is `section` (never `page` for a layout segment).
2. A document may contain multiple sections. Starting a new section forces a new physical page even if the previous section's last page is not full.
3. `document` keeps a default paper; a section may override paper (wins over document default).
4. Paper API: enum of common sizes (at least A-series A3/A4/A5… list them in the design) + `Custom(w,h)`. Orientation enum pairs only with size enum. Custom is final size and does not take orientation.
5. `margin` / `header` / `footer` / `behind` / `front` live on the section. Single-section docs keep today's sugar: `doc.header` etc. apply to the implicit sole section.
6. Header/footer closures see two page-number pairs (all 1-based), filled after pagination: document current/total and section current/total.
7. Do **not** change knife-8's header left/right slot inset-by-side-margins behavior unless a hard conflict with tests/proposal forces it. Path primitives, style builders, fit/cover, named images, odd/even break, per-page header off stay out of scope.
8. Preserve dependency direction: document → layout → (fonts, text, canvas); krilla only in backend.

Grounding will land in `.audit/knife-10/how.md` (and explorers under `how/`). Read those plus `src/document.rs`, `src/layout/paginate.rs`, `src/layout/decorate.rs`, `src/geom.rs`.

Write under your assigned candidate directory:
- `design.md` shaped per architect rationale template (Usage first, then Shape, Module map, Tradeoffs, Open questions).
- Include a concrete named data shape table.

## Rubric (picker only)

| # | Criterion | What a strong answer shows |
| --- | --- | --- |
| 1 | Proposal fidelity | §2.11/§7 paper+orientation+Custom, force new page, dual counters, section-scoped decorations, sugar |
| 2 | Compatibility | `doc.header/footer/behind/front/margins` still work for one-section docs; existing tests stay meaningful |
| 3 | Domain model | One clear structure for "list of sections each with config + flow"; illegal states hard (Custom+Orientation) |
| 4 | Pagination seam | Minimal, correct change to plan/measure/paint so sizes/margins/decorations can vary per section; section counts filled post-plan |
| 5 | Public surface depth | Small caller API; complexity inside paint/paginate; no krilla leak; no Flowable exposure |
| 6 | Deferral discipline | No style builders, no header full-width rewrite, no odd/even, no path layer unless forced |

## Runners

| Seat | Model | Output |
| --- | --- | --- |
| C1 | claude-opus-5-5-medium (architect runner default family) | `.audit/knife-10/arena/candidate-1/` |
| C2 | grok-4.7-high (architect runner default family) | `.audit/knife-10/arena/candidate-2/` |

## Cross-judge

After candidates land: one readonly judge on a different family from the parent when possible, scoring the rubric and recommending a base.
