# Knife 13 implement status

**Branch:** `feat/style-builders`  
**Baseline:** `f208329` (`feat/hline` tip)  
**Feature tip:** `aaf14bf`  
**Date:** 2026-10-09

## Shipped

- `Document::bold_font` registers and rebinds the bold face after a body font exists.
- `Paragraph::bold` and `Paragraph::first_line_indent` preserve one paragraph with ordered, coalesced text runs.
- `FontStore` owns the bold slot. An empty slot returns `UnknownFont { id: 1 }`.
- Tagged clustering keeps each normalized glyph scalar's face across run boundaries and CRLF normalization.
- Measurement batches contiguous `FontId` ranges. Line boxes draw one `GlyphRun` per contiguous font range.
- `LineBudget { first, rest }` gives the first visual line `W - I` and later lines `W`.
- First-line Start, Center, and End alignment use the inset box `[I, W]`.
- Layout errors, missing bold faces, and bold missing glyphs fail before `start_page`.
- RecordingCanvas tests pin font ids, origins, wrapping, pagination, and failure timing.
- The backend smoke test confirms that a mixed bold and indented document starts with `%PDF`.

## Gates

Parent re-ran all seven on feature tip `aaf14bf` (2026-10-09). Exit code 0.

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --no-default-features -- -D warnings`
- `cargo clippy --all-targets --features backend-krilla -- -D warnings`
- `cargo test --no-default-features`: 146 library and 30 integration tests passed.
- `cargo test --features backend-krilla`: 169 library and 40 integration tests passed. One probe test remains ignored.
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --no-default-features`
- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --features backend-krilla`

## Autonomy defaults

- First-line indent defaults to 0. Reverse with `default-two-em`.
- Bold uses the registered bold face. Reverse with `bold-is-same-face`.
- Named styles remain out of scope. Reverse with `named-styles-in`.
- Line metrics come from the body face. Reverse with `max-run-metrics`.

## Deviations

No behavioral deviation from the synthesis design shipped.

The font-id and coordinate tests register the existing fixture twice. A separate bold fixture with different advances was optional, so this change does not add one.

## Deferred

- Named styles and a public `Style`.
- Flow-level style builders.
- Italics and synthetic bold.
- Fill-in blanks and flow canvas.
- Rich text in cells, bands, and page layers.
- A distinct bold fixture with different advances.
