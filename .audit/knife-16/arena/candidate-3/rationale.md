# Knife 16 rationale

Chosen shape: `Cell::spacer(height: Length<NonNegative>)` stores `CellContent::Spacer(Length<NonNegative>)` and lowers to the existing `FlowNode::Spacer(SpacerNode { height })`. One payload per cell. `natural_height` stays, so a spacer taller than the cell's inner full frame is `UnsplittableTallerThanFrame`. Proof is the next row's glyph y, plus an empty op list and the too-tall error.

Frame reverse words that were available and not taken: `space`, `reject-zero`, `stack-cell`, `split-in-cell`, `page-count-only`.

## API name `space` (`space`)

`Flow::spacer` and §2.13 already call this object a spacer. `Cell::hline` is a named constructor, but the verb next to it should match the body method callers already have, not a shorter alias. Two names for one branded height would make the DSL look like two features.

## Reject zero (`reject-zero`)

§2.13 says the height is a non-negative length and may be 0. `NonNegative::ZERO` is already a valid `Flow::spacer` argument, and `SpacerNode::split` returns `Fit` at height 0 with an empty fragment. Switching the cell method to `Length<Positive>`, or returning an error for zero inside `Cell::spacer`, would split one domain object into two contracts. Negative values never reach the method: `NonNegative::pt` / `mm` already return `NegativeLength`.

## Configure closure, or `From<Length<NonNegative>> for Cell`

`Cell::hline` and `Cell::image` take closures because they have fields. A spacer has one field and no paint (§2.13 色块背景不做), so a closure would only forward the height. `From<&str/String/&String>` is the text shortcut used by `t.row(["a", "b"])`. Implementing `From<Length<NonNegative>>` would let a bare length become a cell inside `IntoIterator` and would not match image or hline, which also require named constructors.

## Multi-child cell (`stack-cell`)

`CellBody` is already a `Vec<FlowNode>`, and `check_content` allows a spacer next to text inside that vector. The public `Cell` does not. §2.6 leaves DSL signatures unpinned and lists paragraph and list as later cell children, not as this change. A `Cell` flow closure, or `Cell::stack`, would be a second public cell model and would drag paragraph, blank, link, and list into the same knife. Callers who need blank between two paragraphs still put the spacer in its own cell, or use `Flow::spacer` in the body. One `vec![FlowNode::Spacer(...)]` in `Table::lower` matches every existing arm.

## New layout node or a new allowlist line

A cell spacer is the same object as a flow spacer: non-negative vertical blank, no commands. Adding `FlowNode::CellSpacer` or a second height field would fork split behavior. `check_content` already returns true for `FlowNode::Spacer` in a table cell. A new allow branch would duplicate that and would not change `natural_height`.

## Split inside the cell (`split-in-cell`)

§2.13 says a flow spacer splits: this page takes what fits, the remainder is a shorter spacer on the next page. §2.6 says a table splits by whole physical row, not through the middle of a row. The current code already joins those rules. `measure` calls `natural_height` on the full inner frame; `natural_height` turns `Partial` into `UnsplittableTallerThanFrame`; `split_measured` only moves finished row groups.

Teaching `natural_height` to keep `Partial` would change every unsplittable cell child (text line, image, hline), not just spacer. Teaching only the spacer arm to ignore `natural_height` would special-case one child and still cut a row on a page boundary, which §2.6 forbids. Body split stays covered by `spacer_remainder_leads_next_page` and is not retuned.

A fitting spacer row can still move to the next page when the remaining body is shorter than the row. That is `split_measured`, not a mid-cell split. The design records it with `cell_spacer_row_moves_whole` (next glyph at `BASELINE` on page 2, not at a leftover spacer offset).

## Page-count proof (`page-count-only`)

A spacer paints nothing. A one-cell spacer document is one page whether the height is 0 or 24, so page count does not pin the height. Neighbor glyph y does: with inset 0, a spacer-only row of 24 pt puts the next row's baseline at `24 + BASELINE`. A same-row text sibling stays at `BASELINE` because content is top-aligned; a spacer shorter than `LINE` does not change the next row either, because row height is a max. Those two facts are why the plan has both a spacer-only shift test and a short-beside-text test, instead of one page-count assertion.

## Header-only path, or a new too-tall error

Header and body both go through `lower_rows`. A separate header factory would drift. `UnsplittableTallerThanFrame` is already what an oversized hline, image, and keep-together group report. A spacer-specific error would force callers to match a variant that means the same thing: this cell cannot fit the full inner frame.

## Public padding or border in the same change

Inset is already `border/2 + padding` inside `measure`. Public tables still pass `TableSpec::new` defaults (both zero), so today `inner_full` is the page body height. Shipping padding now would change every existing table's glyph positions and is a §2.6 appearance item, not the spacer arm.
