# Knife 12 synthesis: flow hline

**Base:** candidate-1 (cross-judge 28–21; parent agrees).  
**Grafts from candidate-2:** real `document(PageSize, …)` / `quire_pdf` usage shape in examples; concrete split body (drop C2's `InvalidGeometry` height path); merge richer test checklist.  
**Rejected grafts:** `Cell::flow` / `CellContent::Flow`; DSL name `rule`; `Pen`/`From<Pen>` public bundling; co-locate node in `flow.rs`; height overflow → `InvalidGeometry`.

Autonomy defaults for open §2.3 questions: **fixed default black + 1 pt** (not follow text color). Reverse word: `follow-text-color`. Cell line width uses content-frame `W` after padding (existing cell geometry).

## Synthesis decision

Ship C1's ImageNode-mirror shape.

1. `src/layout/hline.rs` with `HLineNode` + `split` (width deferred: `None` → `avail.width`).
2. `FlowNode::HLine`; `check_content` treats HLine like Spacer.
3. Public `HLine` builder + `Flow::hline` + `Cell::hline` / `CellContent::HLine`.
4. Defaults: full width, Center when short, 1 pt, black, zero spaces.
5. Stroke on geometric centre line; `spacing_after = 0`; spaces inside node height.
6. `lib.rs` re-exports `HLine`.

## Usage (caller)

```rust
use quire_pdf::{document, Cell, Color, NonNegative, Numbering, PageSize, Positive, MM_TO_PT};

let short = Positive::pt(60.0 * MM_TO_PT)?;
let gap = NonNegative::pt(12.0)?;

let rendered = document(PageSize::A4, |doc| {
    doc.paragraph(|p| p.text("标题"));
    doc.hline(|_| {});
    doc.hline(|h| {
        h.width(short);
        h.space_before(gap);
        h.space_after(gap);
    });
    doc.ol(Numbering::Decimal, |list| {
        list.item(|item| {
            item.paragraph(|p| p.text("一项"));
            item.hline(|h| h.thickness(Positive::pt(0.5)?));
        });
    });
    doc.table(|t| {
        t.row([Cell::hline(|_| {}), Cell::hline(|_| {})]);
    });
    Ok(())
})?;
```

## Shape (contract)

| Rule | Where |
| --- | --- |
| thickness `Positive`, spaces `NonNegative`, optional width `Positive` | `HLine` / `HLineNode` fields |
| height = before + thickness + after; never stored | `HLineNode::height` |
| wider → `UnbreakableWiderThanFrame`; taller than full → `UnsplittableTallerThanFrame`; taller than remain → `MoveToNextPage` | `HLineNode::split` (width before height) |
| y = before + thickness/2; x from TextAlign | Stroke path |
| Fail before `start_page` | existing paginate |
| List/column via nested Flow; cell via `Cell::hline` only | document.rs |
| No dashed/double/vertical; no Layer; no blank; no flow canvas | out of scope |

## Graft details

1. Examples use `quire_pdf::document(PageSize::…, |doc| { …; Ok(()) })`.
2. Split follows ImageNode order; overflow sum → taller-than-frame error (not `InvalidGeometry`).
3. Unit tests from C1 + C2 checklist: default centre line, align trio, wider/taller/move, column offset, paint/width bits, list/cell allow, integration page-move + tall error + `%PDF`.

## Tradeoffs accepted

- Closure builder (`hline(|_| {})`) to match `image`.
- No `h.center()` (default is Center); only `start` / `end`.
- Separate `thickness` / `color` setters, not `Pen`.
- No general `Cell::flow` this knife.

## Next implementation step

Add `hline.rs` + wire `FlowNode` / `lower` / DSL / tests; parent re-runs seven gates.
