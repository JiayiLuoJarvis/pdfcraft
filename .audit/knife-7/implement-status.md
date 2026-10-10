# Knife 7 implement status

Branch `feat/table-columns-list-decorate`, baseline `a6ff61e`. Uncommitted. Parent commits.

## Gates (all exit 0)

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --no-default-features -- -D warnings`
- `cargo clippy --all-targets --features backend-krilla -- -D warnings`
- `cargo test --no-default-features`: 100 passed (60 before, 40 new)
- `cargo test --features backend-krilla`: 118 passed, 1 ignored (pre-existing `emit.rs` probe)
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps` under both feature sets

Mutation spot checks. Dropping `dy` in `Path::translated` fails the table border test. Keeping the marker on a list continuation fails the hanging-indent test. Both reverted.

## Files

- `src/error.rs`. Adds `ColumnWidths { reason }`, `InvalidCellContent { container, node }`, `InvalidCellSpan`, `DecorationTallerThanMargin { height, margin }`, `DecorationWiderThanFrame { width, avail }`.
- `src/geom.rs`. Adds `Path::translated(dx, dy)`, `pub(crate)`.
- `src/layout/flow.rs`. Adds `FlowNode::{Text, Columns, Table, List}`, `FragmentCommand::Stroke`, and `with_offset(dx, dy)` as `pub(crate)`. `PlacedFragment` moved here from paginate. `draw_fragment` strokes.
- `src/layout/paragraph.rs`. Adds `TextNode`, which lays out lazily at `avail.width` and always hands back a `Paragraph`. `ParagraphNode.width` plus a guard: a frame narrower than the layout width is `InvalidGeometry`. Adds `single_line_run` for markers.
- `src/layout/lower.rs`. `Container`, `check_content` (one exhaustive match), and the `CellBody` / `ItemBody` newtypes. Their only constructors run `check_content`.
- `src/layout/columns.rs` (new). `ColumnWidth`, `Ratio`, `resolve_column_widths` (design draft algorithm and reason strings), `default_column_gap` (12 pt), and `ColumnsNode` (splittable, top aligned, spacing_after is the max over columns).
- `src/layout/table.rs`. `TableSpec`, `Border`, `Cell` with `NonZeroU16` spans, and `TableNode` with `Open` and `Measured` states. Spans are checked on a grid. Rows are measured once. Body rows are grouped into rowspan groups, and each group is atomic. The header rides on the tail, and `repeat_header` defaults to true. Each cell gets a closed rectangle stroke.
- `src/layout/list.rs` (new). `MarkerKind` (decimal, upper alpha, lower alpha, upper roman, lower roman), `ListKind::{Bullet, Ordered { style, start }}`, `ListItem::{new, custom}`, and `ListNode`. The marker is an `Option<String>`, and continuations carry `None`.
- `src/layout/decorate.rs`. `PageNumber`, `PageCount`, `PageInfo`, `Decorations` (boxed closures), `PageBands`, and `measure_bands`.
- `src/layout/paginate.rs`. `paginate_root(root, ctx, config, &Decorations, canvas)`. It runs plan, then `measure_bands`, then paint. Paint order is header, body, footer. The existing 14 tests now pass `&Decorations::none()`.
- `src/layout/test_util.rs` (new, `cfg(test)`). Shared fonts, style, render, runs, and strokes helpers.
- `src/layout/mod.rs`. Registers the new modules.

## Tests added (40)

- columns (16). Width solver literals: fixed+auto, ratio weights, ratio+auto half split, fixed slack. One test each for the reasons `more-than-one-auto`, `fixed-sum-exceeds`, `no-remaining-width`, `non-positive-ratio` / `non-finite`, `column-count`, `gap` (the `gap` one also checks for zero output). Column x and gap. Text wraps at the column width, not the body width. Columns split across pages. spacing_after is the max over columns. A stuck column errors with zero output. A pre-laid paragraph wider than its column errors.
- table (9). All header rows repeat on the continuation page. Header repeat off. A rowspan group moves as a whole. Header plus a group taller than the page errors with zero output. Border rects on two row groups use exact points, and text sits at the border/padding inset. Zero border draws nothing. A cell rejects columns, pageBreak, keep_together, nested table, and nested pageBreak. Overlapping spans, a span past the right edge, a header rowspan into the body, and an uncovered slot are all rejected.
- list (9). Hanging indent, with the marker on the first fragment only (exact bits across two pages). Nested indent. nest_indent. Roman numbering from `start`, with a custom marker that keeps its number. Labels for every kind. Labels past Z or past 3999 error. A custom marker that is too wide errors with zero output. An empty custom marker is rejected. An item rejects table and pageBreak.
- decorate (6). Bands span the full page width. Page numbers run across all pages, blank body page included. Paint order is header, body, footer, with exact bits. Body coordinates are bit-identical with and without decorations. A zero margin errors before any `StartPage`. A footer taller than its band errors. A header wider than the page errors. A band rejects a table.

## Defaults applied (from synthesis table)

- Ratio is a positive weight. With Auto present, Auto takes half the rest (design draft step 6).
- An illegal merge is `InvalidCellSpan`. Every grid slot must be covered.
- The footer lays out from the band top down.
- An empty custom marker is `InvalidGeometry`.
- An ordered marker is the label plus `.`, left aligned in the marker column.
- keep_together is forbidden in list items and table cells.
- `nest_indent` defaults to `marker_width`. Smaller is `InvalidGeometry`.
- Page numbers are document totals only.
- A custom marker still uses up its number.
- Alpha past 26 or roman past 3999 is `InvalidGeometry`.

## Deviations and scrap

- Band content is tightened to Text, Paragraph, Spacer, and Stack of those, as the synthesis asked. List is also refused in bands.
- `ColumnWidths.reason` is a `&'static str` per the synthesis and the design draft reason set. It is not an internal enum as cross-judge graft 4 suggested.
- Table defaults inside `TableSpec::new` are padding 0 and border 0. DSL defaults belong to knife 8.
- When band content cannot fit even a full page frame, `DecorationTallerThanMargin.height` reports the page height as a lower bound, because the real height is not measured past one page.
- `Fill`, RuleNode, `Split<T>`, Image, and the DSL were not added. `dump.rs` and `limits.rs` were not touched.
- AGENTS.md directory tree does not list `columns.rs` / `list.rs` yet. Left for the parent so the doc edit stays separate.
