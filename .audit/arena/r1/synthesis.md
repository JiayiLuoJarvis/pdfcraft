# R1 arena synthesis

## Rubric

1. Matches pinned public API (`inset`, `inset_left`/`inset_right` default 0).
2. Does not change `measure_band` origin or `layout/` semantics.
3. Minimal diff in `document.rs` only.
4. Without `inset`, left slot paints at x = 0 under non-zero side margins.
5. With `inset(l,r)`, left slot paints at x = l.

## Candidates

- candidate-1 (claude-opus-5-5-medium): track-spacer, read inset fields instead of margins. Passes all criteria.
- candidate-2 (grok-4.7-high): late. Content box `page_w − insets`, then a leading Fixed track for `inset_left`. Right inset is leftover page width, not a trailing empty track (except the one-slot `ColumnsNode` minimum of two tracks).

## Base

candidate-1. Implementation in `src/document.rs` matches it. Graft from rationale: rename local lower helper `inset` → `spacer` so it does not shadow `Band::inset`.

No graft from candidate-2. A trailing-spacer omission plus a one-slot extra track is a larger lowering surface than swapping the spacer width source. R1 is already committed on that smaller shape.

## Verification

`cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all green on `feat/element-repair-r1`.
