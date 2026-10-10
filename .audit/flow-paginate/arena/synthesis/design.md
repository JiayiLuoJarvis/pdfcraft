# Knife 6 synthesis — base candidate-1

## Pick

**Base:** candidate-1 (`FlowNode::split` + `Fragment` + `PageCursor` in `paginate`).

**Why.** Matches `quire-design.md` PR8/PR9 and how.md grounding. Knife 7 table/columns need the same consume-self `Split` protocol; candidate-2’s two-phase `BlockBox` list would be rewritten then. Domain fit: `PageCursor` enum, not bools. Scope stays internal (no public DSL).

**Cross-judge.** See `cross-judge.md` when present; parent provisional pick agrees with how.md and design authority.

## Grafts from candidate-2 (cross-judge)

- Owned `LineBox` / remaining-line resume inside `ParagraphNode` (avoid `BrokenLines<'static>` self-ref; do not re-measure full text each page).
- Paginate first into placed fragments / page bodies, then paint (`draw_fragment`); avoid Canvas side effects inside the cursor transition helpers when practical.
- Prepared `KeepTogether`: layout once on full frame into owned fragment + height + trailing spacing; move that prepared value on `MoveToNextPage` (children already consumed).
- Keep the named PageBreak / spacer / spacing_after acceptance tests.
- Do **not** adopt global `FlowItem → BlockBox` flattening.

## Corrections vs raw cand1 sketch

1. **`Frame` must allow height 0.** Landed `Size` requires `Positive` sides, so remaining body height of 0 cannot be `Size`. Use:

   ```rust
   pub(crate) struct Frame {
       pub width: f32,  // must be finite and > 0 at split entry
       pub height: f32, // finite and >= 0
   }
   ```

2. **`FragmentCommand` knife-6:** only `Glyphs(GlyphRun)`. Do not stub Fill/Stroke/Image/Link variants until a consumer needs them (Laziness).

3. **`LayoutCtx`:** omit `images` until image PR. Styles: crate-internal `Style` + `TextAlign` only; `resolve_style` / named map can be minimal or skip if tests construct resolved `Style` directly.

4. **Spacer rest height:** use `NonNegative::pt(rest)?` (0 allowed). No `from_trusted_pt` unless it already exists.

5. **`KeepTogether`:** pass full body frame height into split (paginate/stack injects `full_height`), per design `natural_height` on full frame.

6. **No `lower` / public `Block` this knife.** Tests build `FlowNode` trees.

## Module map (ship)

| File | Owns |
| --- | --- |
| `src/error.rs` | `UnsplittableTallerThanFrame` |
| `src/layout/style.rs` | `TextAlign`, `Style` (internal) |
| `src/layout/flow.rs` | `Frame`, `Fragment`, `FragmentCommand`, `Split`, `FlowNode`, `draw_fragment`, `natural_height`, Stack/KT/Spacer split |
| `src/layout/paragraph.rs` | `ParagraphNode`, measure+break+align+glyph runs |
| `src/layout/paginate.rs` | `PaginateConfig`, `PageCursor`, `paginate_root` |
| `src/layout/mod.rs` | modules |
| stubs stay empty | `lower`, `decorate`, `table`, `dump`, `document` |

## Success criteria (gates + behavior)

RecordingCanvas tests for:

- Start / Center / End x; baseline y = line_top + `line_metrics.baseline`
- Paragraph splits across pages; head has no `spacing_after`
- `spacing_after` dropped when it does not fit; next page y at body top
- Spacer splits; remainder can lead next page
- KeepTogether moves whole; taller than full frame → `UnsplittableTallerThanFrame`
- PageBreak: leading no-op → 1 blank page at EOF; leading×2 → 1 page; content+1 break → 1 content page; content+2 breaks → content + blank; content+break+content → 2 content pages; content+break+break+content → 3 pages

Four gate pairs green (fmt / clippy-D / test / doc) for `--no-default-features` and `--features backend-krilla`.
