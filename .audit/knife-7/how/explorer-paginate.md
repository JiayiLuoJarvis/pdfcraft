# Explorer: paginate / decorate hooks

Source: how explorer task 21379032-ec65-46ad-b308-a0320cb21773

## Components Found

- `paginate_root` = `plan_pages` then `paint_pages`. `PaginateConfig` is page_size + margins; knife 6 does not paint header/footer.
- Body frame from margins; decorate must not shrink it.
- `PageCursor` Fresh / HasBody / PendingBlank. `PagePlan` has size + placed fragments only.
- `draw_fragment` only Glyphs today. Canvas already has `stroke_path` / `StrokePath`.
- `decorate.rs` / `table.rs` stubs. Missing errors: Decoration*, ColumnWidths, InvalidCellContent.

## Flow

1. plan_pages → Vec<PagePlan>
2. PageBreak bypasses split; content uses Fit / Partial / MoveToNextPage
3. finalize_eof sets final page count
4. paint_pages: start_page → draw_fragment each → finish_page

Decorate must run after plan (page count known), before or inside paint, without rewriting body height / y / placed origins.

## Boundaries

- Default margins 0. Proposal: decorate full page width; band height = top/bottom margin.
- Paint order proposal: behind → header → body → footer → front. Design sequence paints body then decorations.
- Table borders need FragmentCommand::Stroke (or paint-time stroke) + draw_fragment support.

## Open Questions (for synthesis)

- Header vs body paint order (proposal vs design sequence) for RecordingCanvas order.
- Decorate as PagePlan fields vs paint-time side path.
- Stroke in FragmentCommand vs direct canvas stroke during paint.
