# Knife 17 candidate 1 — rationale

## Recommendation in one sentence

Add **`Cell::paragraph` + shared `lower_paragraph`**, then **`Cell::ul`/`Cell::ol` with list closure deferred to `Table::lower` via `ListCellSeed`**, reusing existing `Paragraph`/`List` and **`FlowNode::Text`** (not `Paragraph` node); drop **`Cell: Clone/Eq`** if needed; prove with **`document().pages()`** recording tests.

---

## Why autonomy defaults (no reverse words)

### Paragraph: `Cell::paragraph(|p| …)` like `Flow::paragraph`

**Adopted.** §2.6 allows 段落（含填空）; layout already accepts `Text`/`Paragraph`. Public rich text is **`TextNode::from_items`** (`how.md`, explorer). A second cell builder would duplicate coalesce/blank/link rules and drift from body.

**Rejected `items-only`:** storing raw `Vec<InlineItem>` bypasses `Paragraph` validation and duplicates `Flow::paragraph` field handling (`error`, indent).

**Rejected `rich-new`:** `Cell::new` must remain plain `TextNode::new` shorthand per frame and handoff continuity (knife 16).

**Rejected `text-only`:** would ship half of §2.6 cell paragraph without link/blank/bold that proposal already bundles with 段落.

### List: `Cell::ul` / `Cell::ol` + defer to lower

**Adopted.** `List::item` requires live `Flow::nested` at item build time; free `Cell::ul` has no `Flow` until lower (`explorer-document-dsl.md`). Deferring matches **`Flow::list`** sequence with `flow.child()` at lower — same style/token path as `CellContent::Text` today.

**Rejected `table-child-flow`:** correct context but breaks **`t.row([Cell::ul(...)])`** ergonomics and expands `Table` struct like `Columns` without removing the closure problem for cells built outside `Table` method chain.

**Rejected `list-closure-on-row`:** would pin list construction to row API; rubric asks for mirror of body `ul`/`ol` beside `Cell::hline`.

### Pagination: `UnsplittableTallerThanFrame`

**Adopted.** §2.6 row-unit split vs §2.8 item split is already encoded in `natural_height` for cells (`how.md`). Changing it is **`split-in-cell`**, out of frame.

### Multi-kid cell

**Rejected `stack-cell`.** One payload per public `Cell` matches knife 16 handoff; `CellBody(Vec<_>)` stays internal.

---

## Alternatives considered (list storage)

### 1. Eager `Cell::ul` with synthetic `Flow::new(default_style(), DocToken::next())`

**Rejected.** Wrong document token boundary; breaks the day item bodies gain image; style diverges from `flow.child()` when inheritance lands. Explorer explicitly flags this.

### 2. Generic `Cell<'a>` carrying `FnOnce(&mut List) + 'a`

**Rejected.** Propagates lifetime into `Table`, `row` iterators, and `Into<Cell>`; public API pollution for a single arm.

### 3. `Box<dyn FnMut(&mut List) + 'static>`

**Chosen.** Not identical to `FnOnce` ergonomics but **one shot at lower**; table DSL captures are typically owned (`'static`). Document that non-`'static` captures need copying into owned data before `Cell::ul`.

### 4. Declarative item tree (no closure)

**Rejected.** Largest diff; second list-item API surface; forces multi-node cell modeling.

---

## Alternatives considered (paragraph storage)

### 1. Store only `items` + indent, discard `Paragraph::error`

**Rejected.** Silently drops `EmptyLink` / `RunAfterFillBlank` — violates layout honesty rubric.

### 2. Lower to `FlowNode::Paragraph`

**Rejected.** Pre-wraps at wrong width; fill-to-end blank and link rects would not use final cell `inner_w`.

### 3. Keep `Cell: Eq` via error code enum snapshot

**Possible but not required.** No in-tree `Cell` equality. Simpler to drop `Eq`/`PartialEq`/`Clone` for the whole struct when any arm holds `Paragraph` or list seed. Paragraph arm alone already blocks `Clone`.

---

## Split 17a / 17b

**Paragraph is independently valuable and low-risk** (helper extraction + one enum arm + tests).

**List adds** `ListCellSeed`, trait object storage, and derive policy — orthogonal to paragraph lowering.

If timeboxed: **17a merge first**; 17b is additive with zero layout changes. If both fit one evening: **two commits, one knife** (frame default unless measured slip).

**Rejected `force-both` single commit** without list tests — rubric prefers shippable slices.

---

## Risk register

| Risk | Mitigation |
| --- | --- |
| `'static` on list closures | Document; table closures usually own strings; copy captures if needed |
| Narrow columns vs 18 pt marker | Existing `InvalidGeometry`; test optional |
| Public `Cell` derive removal | Note in CHANGELOG; grep shows no `Clone`/`Eq` reliance |
| Semantic drift vs `Flow::paragraph` | Single `lower_paragraph` helper |

---

## Fidelity checklist (rubric)

1. **Proposal:** paragraph + list in cell; forbidden kids unchanged.
2. **Caller surface:** reuses `Paragraph` / `List`; no parallel rich builder.
3. **Layout honesty:** shared allowlist; `natural_height` documented; list-item errors keep `"list item"` container name.
4. **Seams:** deferred lower; no krilla; no second backend.
5. **Tests:** font ids, blank strokes, link rects, marker x, reject forbidden, over-height error.
6. **Laziness:** minimal new types (`ListCellSeed` only); split only if schedule slips.
