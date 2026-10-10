# Knife 7 synthesis

## Pick

**Base:** candidate-1 (FlowNode-native + lazy `Text` + small `new`/`split` surface).

**Why.** Parent agrees with [cross-judge](cross-judge.md). `FlowNode::Text` makes §2.1 W = frame width a protocol property. C2 leaks width into every assemble site. Decorate keeps C1 closure + `PageInfo` shape for knife 8. C2 wins on paint isolation and typed cell bodies. Those graft cleanly.

**Cross-judge.** Base C1 (26 vs 20). Grafts below match the judge list.

## Grafts from candidate-2

1. Decorations never live on `PagePlan`. After `plan_pages`, `measure_bands` reads only page sizes + margins + `Decorations`. Local `Vec<PageBands>` zips into paint. Fail before any `start_page`.
2. `CellBody` / `ItemBody` newtypes. Only constructor is `check_content`. No C2 macro dual enum. No `Split<T>`.
3. List markers. `ListKind::Ordered { style, start }` / unordered. Per-item `Option` custom string. Marker run stored on first fragment only. No `head_started` bool as the sole resume signal. Prefer explicit `marker_drawn: bool` or `Option<GlyphRun>` remaining.
4. Typed error payloads where cheap (`DecorationTallerThanMargin { height, margin }`). Keep design `ColumnWidths { reason: &'static str }` static strings for the documented reason set. Add `InvalidCellSpan` for illegal merges.
5. Tests from C2. Body y bit-identical with/without decorations. Fail with zero `StartPage`. One test per `ColumnWidths` reason that matters.

## Rejected from both

- C2 `RuleNode` / separator this knife (handoff cells are Paragraph + Spacer; §2.3 later).
- C2 `Split<T>` generic.
- C1 storing header/footer on `PagePlan`.
- C2 pre-resolved widths + caller-laid paragraphs as the main path.
- Band may hold Table/Columns. Tighten to Text / Paragraph / Spacer / Stack of those.

## Defaults (operator reverse word in parentheses)

| Call | Default | Reverse |
| --- | --- | --- |
| Ratio meaning | Positive weight. Rest split by weight sum. Not forced to 1. | `ratio-is-fraction` |
| Illegal merge error | `InvalidCellSpan` | `merge-as-cell-content` |
| Footer vertical | From band top downward, same as header | `footer-pin-bottom` |
| Empty custom marker | `InvalidGeometry` | `allow-empty-marker` |
| Ordered marker text | Enum form plus `.` (e.g. `1.`, `A.`, `iv.`) left in marker column | `marker-no-dot` |
| keep_together in list item | Forbidden | `list-allow-kt` |
| nest_indent | Equals `marker_width` | (configure at assemble) |
| Section page numbers | Document totals only | knife 8 |
| Custom marker API | Per-item string only | knife 8 callback |

## Module map (ship)

```text
src/error.rs          + ColumnWidths, InvalidCellContent, InvalidCellSpan,
                        DecorationTallerThanMargin, DecorationWiderThanFrame
src/geom.rs           + Path::translated (pub(crate)) if shared; else private in flow
src/layout/
  flow.rs             FlowNode + Text, Columns, Table, List
                      FragmentCommand::Stroke; with_offset(dx,dy) pub(crate)
  paragraph.rs        TextNode; ParagraphNode.width + guard; single_line_run
  columns.rs          NEW ColumnWidth, resolve_column_widths, ColumnsNode
  table.rs            TableSpec, Cell, TableNode Open|Measured
  list.rs             NEW ListSpec, ListNode, MarkerKind
  lower.rs            Container, check_content, CellBody, ItemBody
  decorate.rs         PageInfo, Decorations, measure_bands, PageBands
  paginate.rs         paginate_root(..., &Decorations, ...); plan then measure_bands then paint
                      paint order header → body → footer
```

## Success criteria

RecordingCanvas tests for columns x/gap/split, table header repeat + rowspan block + strokes, list hanging + custom/roman, decorate full page width + PageNumber/PageCount + blank pages decorated + fail-zero-output + body coords unchanged with decorations.

Four gate pairs green. One commit on `feat/table-columns-list-decorate`. No public DSL.

## Next implementation step

Scaffold protocol (`Stroke`, 2D offset, errors, `TextNode`, `DecorationSpec::none` wired into `paginate_root`), then columns, table, list, decorate with tests. Verify gates once green.
