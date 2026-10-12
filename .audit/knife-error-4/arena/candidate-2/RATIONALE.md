# Rationale

## Problem

Knife 4 collects many faults in one run without changing what a fault is. `Error` stays one `kind` plus `at` plus an optional `source`. The public failure is a non-empty `Errors`, and `quire::Result<T>` is `Result<T, Errors>`. `document` / `paint` / `render` return that list, capped at 64, in discovery order. Any item blocks `Rendered` and PDF. Assembly must keep lowering siblings after a fault, and a container must stop returning `self.error` / `Document.error` ahead of child faults already discovered. Measure must keep going for `MissingGlyph`, `BoldFaceRequired`, and `UnknownFont`. Geometry may be recorded and then stop the walk. Backend stays one `Error`. `error` cannot depend on `document` or `layout`.

The non-obvious part is the carrier. A single accumulator borrowed through `LayoutCtx` makes every layout helper depend on that borrow, and a container can still write its own fault before it folds children. Returning the list from the function that already sees the children keeps one owner at a time and makes "children before this `lower`'s new faults" a property of the return, not of who holds a mutable slot.

## Usage (caller's view)

The caller writes the same closure DSL and matches one `Result`. `Err` is the list (`first`, `as_slice`, `iter`, `truncated`, multi-line `Display`). `Error::from(kind)` converts with `From<Error> for Errors`, so `font(...)?` inside the build closure is a one-item list at the `?`. `to_pdf` runs only on `Ok(Rendered)` and adds at most one wrapped backend fault.

`section` returns `()`. Its inner closure is still `Result`, so a `?` aborts that section only and the next `section` still runs. Deferred builders (`paragraph`, `table`, `columns`) do not return errors; ignoring `font` / `load_image` / `pt` still drops that single `Result`.

A paragraph with a missing scalar followed by `spacer(-3.0)` yields `NegativeLength` then `MissingGlyph`: assembly in document order, then measure. A ragged table after a bad column width yields both faults, not only the container's.

## Shape

`Errors` is the seal: private `Vec` of length `1..=64`, plus `truncated`, `Clone`, per-item `PartialEq` (`kind` + `at`) including the flag, numbered `Display`, `source()` = `None` (`per boundary-discipline`). `ErrorList` is `pub(crate)`, may be empty, and is the only type that enforces the cap. `Fallible<T>` is `Result<T, Error>` for leaves. `lower` / `Flow::finish` return `Result<T, ErrorList>` so a parent can `append` without unsealing.

Each builder owns one `ErrorList`. Setter faults `push` at the call. Attaching a child `append`s that child's list at the call, so a note taken before the child stays in front of it. `lower` appends only whole-container checks (spans, column skeleton) and then returns `Err(list)` without the node. Siblings still lower. `Document.error` is removed; `section` drains the section list, appends the inner closure's `Err`, `fill_missing`s the section, and either stores the job or appends the list (`per encode-lessons-in-structure`: the node and the list are not both returned).

`measure_text` returns `Result<(MeasuredText, ErrorList), Error>`. `UnknownFont` and `MissingGlyph` are on the `Ok` list; the value uses `.notdef` advances so later scalars in the string are still seen. `BoldFaceRequired` is pushed in `measure_paragraph` the same way. `measure_paragraph` does not call `fit`. `layout_pages` owns a **local** `ErrorList`, `append`s each paragraph's list, `fill_missing`s page and region, then matches `fit`. `fit`'s `Err(Error)` is `push`ed and the walk returns `(pages_so_far, found)` without `?`. `render` is the seal: caller `Err` from the build closure appends and skips layout; otherwise it appends `layout_pages`'s list and, if anything was recorded, returns `Err(Errors)` without building a canvas. Empty list: `paint_pages(...).map_err(Errors::from)`. `to_pdf` is the same `From` on the backend's single `Error`.

`LayoutCtx` does not carry a list. Soft versus hard is which `Result` variant the site returns, not a flag on `ErrorKind`. Public depth is one list in `Err`. Callers do not learn `ErrorList`, the cap's storage, or which sites continue (`per minimize-reader-load`).

Deliberately absent: a context-threaded bag, a public `push`, `ErrorKind::Truncated`, partial PDF, parallel collect, source line numbers, and dependency error types on the public surface.

## Synthesis decision

TBD.

## Tradeoffs accepted

- We accept `section` → `()` (call sites drop `?`) in exchange for storing the inner `Err` once and still running the next section.
- We accept add-time `append` of a child's list in exchange for call order. Whole-container checks stay at `lower` because that is when they are discovered; they cannot jump ahead of lists already appended.
- We accept `layout_pages` turning a leaf `Err(Error)` into `(pages, found)` after `push`, in exchange for not dropping soft faults from earlier siblings and not putting `&mut ErrorList` on `LayoutCtx`. `render` discards those pages. Leaves (`fit`, `measure_text`'s face/advance `Err`, `paint_pages`, `write_pdf`) still return `Error`.
- We accept `.notdef` advances inside a `MeasuredText` that is never painted, in exchange for one walk over the string that records every missing scalar.
- We accept that a face/advance `Err` inside `measure_text` drops `MissingGlyph`s already seen in **that** string, in exchange for the leaf staying `Result<_, Error>`. The paragraph boundary appends the `Ok` list before `fit`, so glyphs and a later geometry fault on the same paragraph are both kept.
- We accept laying out kept nodes even when assembly already failed, then dropping the plans, in exchange for real page/region stamps and no second glyph scanner. Nodes whose `lower` returned `Err` are not in that walk.
- We accept a hard geometry or canvas fault hiding later siblings, in exchange for the confirmed hard stop. Bands run only after an empty layout list, because hook page identity needs a finished body plan.
- We accept one `ContentOutsideSection` per rejecting flow (a latch) in exchange for not repeating the marker. Later rejected pushes still contribute their own child lists.
- We accept `truncated` in `PartialEq`, in exchange for cap tests that do not parse `Display`. Per-item equality stays `kind` + `at`.
- We accept an ignored `font` / `load_image` / `pt` `Result` as gone, in exchange for not copying that channel into the builder list.
- We accept a crate-private `append_errors` in exchange for a public `Errors` with no `push`.

## Alternatives considered

- **One `&mut ErrorList` on `LayoutCtx`, passed into `lower` and `measure_text`.** The caller-facing `Errors` is the same, but every helper's signature grows the bag, and a container can `push` its own fault before it folds children — the override bug, with a longer argument list. Rejected.
- **At `lower`, concatenate every child list, then the container's notes, as two blocks.** Simple, and it does put children before the container's slot. It also moves a `cell_padding` recorded *before* `row` to after that row, which breaks discovery order. Add-time `append` plus lower-time checks at the end keeps both. Rejected.
- **`measure_text` returns `Err` for a missing glyph and no `MeasuredText`.** The paragraph cannot see later scalars without a second scan, and layout grows a walk that exists only on the dirty path. Returning `(MeasuredText, ErrorList)` on `Ok` makes "keep scanning, do not paint" the caller's merge plus `render`'s empty check. Rejected.
- **Seal `Errors` inside every `lower`.** Parents would unseal to append the next sibling, so the non-empty public type leaks into assembly and the cap gains two writers. `ErrorList` until `render` / `to_pdf` keeps one sealer. Rejected.

## Open questions and risks

- The build closure's `Err` skips layout. Should kept nodes from the part that did run still be measured?
- A cell that fails `into_stored` is omitted from the grid before `check_spans`. Should a follow-on `EmptySlot` be suppressed, or is the extra item acceptable?
- `Errors::source()` is `None`. Should it forward `first().source()` so `anyhow` / `{:#}` still show one underlying cause?
- Is the extra Display line `further errors omitted`, including after a single kept item when `truncated` is set?
- Laying out a tree with holes can itself `fit`-fail (stall, short page) in a way the full tree would not. Is that extra hard fault acceptable, or should any assembly fault skip `fit` and only run `measure_text`?

## Next implementation step

Add `Errors`, `ErrorList`, `From<Error>`, cap/`append` order, and Display tests in `src/error.rs`, and point `quire::Result` at `Errors`, before changing `Table::row` / `Table::lower`.
