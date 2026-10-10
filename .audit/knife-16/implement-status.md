# Knife 16 implement status

**Branch.** `feat/cell-spacer`
**Baseline.** `16749da` (`16749da0474b091e18d9c19a14833e8f779afe18`)
**Implementation tip.** `c9c2af7` (`c9c2af7b6dee17c0a6f745c8e6589f81be1b664e`)
**Date.** 2026-10-10

## Commits

- `7d327875b8af1f66d289736d61910136d1cd477e` `feat(document): Cell::spacer + lower arm`
- `40d55509b5d21634e1489fe87e0df50ddb264d3a` `test(document): cell spacer geometry and errors`
- `c9c2af7b6dee17c0a6f745c8e6589f81be1b664e` `test(layout): cell_allows_spacer`

## Gates

Run on the working tree that became `c9c2af7`, before this status commit. Each command exited 0.

- `cargo fmt --all`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`. 198 library tests passed, 1 ignored. 69 tests in `tests/document.rs` passed. 0 doctests.

## Tests added

- `cell_spacer_shifts_the_following_row`
- `cell_spacer_alone_records_no_ops`
- `cell_spacer_taller_than_body_fails_before_any_page`
- `short_cell_spacer_does_not_grow_a_text_row`
- `layout::table::tests::cell_allows_spacer`

## Deviations

None. `CellContent::Spacer(Length<NonNegative>)`, `Cell::spacer`, and the `Table::lower` arm match the synthesis. Layout allowlist, `natural_height`, body `Flow::spacer`, and the backend are unchanged.
