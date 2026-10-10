# Knife 17 implement status

**Branch.** `feat/cell-rich`
**Baseline.** `a5ba3fb` (`a5ba3fb5ae701cb2047a0207b9001a504c194f3f`), `feat/cell-spacer` tip.
**Paragraph tip.** `7b82e3c` (`7b82e3cea3c0e134b8212719eef29a60139b2824`)
**Implementation tip.** `c3035df` (`c3035df27461213d4770b4f58eb4b518320f0277`)  
**Parent re-verify.** Gates re-run on `c3035df` after implementer return. Exit 0.  
**Date.** 2026-10-10  
**17b.** Shipped on this branch. Not deferred.

## Commits

- `7b82e3cea3c0e134b8212719eef29a60139b2824` `feat(document): Cell::paragraph via shared lower_paragraph`
- `c3035df27461213d4770b4f58eb4b518320f0277` `feat(document): deferred Cell::ul/ol`

## Gates

First run on `c3035df` by implementer. Parent re-ran the same three. Each command exited 0.

- `cargo fmt --all`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`. 198 library tests passed, 1 ignored. 76 tests in `tests/document.rs` passed. 0 doctests.

Also `cargo test --no-default-features` exited 0. 175 library tests passed, 0 ignored. 64 tests in `tests/document.rs` passed. The image arm of `cell_list_item_rejects_forbidden_kids` is not compiled in that run.

## Tests added

- `cell_paragraph_pins_bold_blank_and_link`
- `cell_paragraph_errors_record_no_pages`
- `cell_paragraph_indent_uses_cell_width`
- `cell_list_marker_at_cell_x`
- `cell_list_item_rejects_forbidden_kids`
- `cell_rich_taller_than_frame_does_not_continue`
- `cell_list_narrower_than_marker_is_invalid_geometry`

`paragraph_enters_the_flow_unwrapped` still passes. The shared helper still emits `FlowNode::Text`.

## Deviations

- `Eq` left `Cell` and `CellContent` on the paragraph commit. `Paragraph` stores `Error` and `InlineItem`, and neither is `Eq`, so the paragraph arm cannot keep `Eq`. `Clone` and `PartialEq` stayed until the list commit. That commit drops them, and also drops the temporary `Clone` and `PartialEq` on `Paragraph`. The shipped `Paragraph` is `Debug` only.
- The list-item image case is behind `feature = "backend-krilla"`. `Document::load_image` returns an error without that feature, so the rejection never reaches the list-item check. Default `cargo test` still runs the image case. Table, columns, page break, and keep-together run in both feature sets.
- The table and columns cases build a real node first. An empty table or an empty column row fails with `ColumnWidths` before `ListItem` can reject the node. The table row is one text cell. The columns row is two text columns.
- `cell_list_marker_at_cell_x` puts both the bullet list and the numbered list in column 1. Column 0 is the labels `L` and `M`. Marker x is 100, item text x is 118, the nested bullet is at 118, and `child` is at 136.
