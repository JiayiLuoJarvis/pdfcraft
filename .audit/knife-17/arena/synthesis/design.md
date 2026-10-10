# Knife 17 synthesis — cell paragraph + list

**Base.** candidate-3 (cross-judge 30/30; parent agree).  
**Grafts.** c2 derive layering note (paragraph commit may keep Clone/PartialEq until list lands); c1/c4 conditional 17b fallback if list slips.  
**Reject.** c2 `Table<'a>` / `Cell<'a>`; c1 `FnMut`.

## Locked API

```rust
Cell::paragraph(f: impl FnOnce(&mut Paragraph)) -> Cell
Cell::ul(f: impl FnOnce(&mut List) + 'static) -> Cell
Cell::ol(numbering: Numbering, f: impl FnOnce(&mut List) + 'static) -> Cell
```

Storage: `CellContent::Paragraph(Paragraph)` and `CellContent::List(DeferredList)` with `Box<dyn FnOnce(&mut List) + 'static>`.

Helpers (private, shared with Flow): `lower_paragraph`, `assemble_list`, `numbering_kind`.

Paragraph lower emits `FlowNode::Text(TextNode::from_items(...))`, never `FlowNode::Paragraph`.

List runs at `Table::lower` via `assemble_list(flow.child(), …)`.

## Commits (one knife)

1. **17a:** `lower_paragraph` + `Cell::paragraph` + paragraph tests. Derive: add `Debug` on `Paragraph`; `Cell` may drop `Eq` only if needed.
2. **17b:** deferred list + `ul`/`ol` + drop `Clone`/`PartialEq`/`Eq` on `Cell` + list tests.

If 17b slips the evening, ship 17a alone and note 17b in handoff.

## Tests (from c3 §Test plan)

Required: `cell_paragraph_pins_bold_blank_and_link`, errors, indent-vs-cell-width, `cell_list_marker_at_cell_x`, item forbidden kids, taller-than-frame, narrow marker geometry.

## Out

Multi-kid cell; mid-cell Partial; columns/pageBreak/keep_together/nested table in cell; named styles; canvas/qr/svg/toc.
