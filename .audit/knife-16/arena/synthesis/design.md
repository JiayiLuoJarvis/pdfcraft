# Knife 16 synthesis — `Cell::spacer`

**Base.** candidate-2.  
**Grafts.** candidate-3 alone-no-ops (+ optional short-beside-text); candidate-1 rustdoc body/cell contrast.  
**Authority.** `docs/提案书.md` §2.6 / §2.13; `.audit/knife-16/how.md`; `arena/frame.md` defaults (no reverse words).

## Locked API

```rust
enum CellContent {
    Text(String),
    Image(Image, ImageBox),
    HLine(HLine),
    Spacer(Length<NonNegative>),
}

impl Cell {
    /// A one-by-one cell holding vertical space that paints nothing.
    ///
    /// Height may be zero. Taller than the cell's inner full frame is
    /// [`Error::UnsplittableTallerThanFrame`]. Unlike [`Flow::spacer`], the blank
    /// does not split across pages inside the cell.
    pub fn spacer(height: Length<NonNegative>) -> Self { … }
}
```

`Table::lower`:

```rust
CellContent::Spacer(height) => vec![FlowNode::Spacer(SpacerNode { height })],
```

No `From<Length>`, no cell Flow closure, no layout allowlist or `natural_height` edits.

## Files

| File | Change |
| --- | --- |
| `src/document.rs` | enum arm, constructor, lower arm, rustdoc |
| `tests/document.rs` | geometry + error tests below |
| `src/layout/table.rs` | optional `cell_allows_spacer` unit test only |

## Required tests (`tests/document.rs`)

1. **Shift.** Single-column table: spacer-only row height `H`, then `"B"`. Compare `H=0` vs `H=30`. Assert `y(30)-y(0) ≈ 30` via `pt_near`. One page. No stroke/fill/image/link ops when asserting the positive-H page (glyph for `"B"` only).
2. **Alone no-ops (graft).** Table with only `Cell::spacer(24)`. One page, `ops()` empty. No font required.
3. **Too tall.** Custom page height 20; spacer 21. `Err(UnsplittableTallerThanFrame)` before any page.

## Optional tests

- Zero alone already covered by shift control; keep if useful.
- `short_cell_spacer_does_not_grow_a_text_row` (8 pt spacer beside text; next row at `LINE + BASELINE`).
- `cell_allows_spacer` in `src/layout/table.rs`.
- Whole-row move to next page when spacer fits full body but not remaining height.

## Out of knife

Cell paragraph / blank / link / list; multi-kid cell; `split-in-cell`; body spacer changes; canvas/qr/svg/toc; krilla outside backend.
