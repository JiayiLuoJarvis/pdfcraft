# Knife 15 implement status

**Branch.** `feat/text-uri-link`
**Baseline.** `feat/inline-blank` at `a232ce1`
**Verified tip.** `16749da`
**Date.** 2026-10-10

The status commit sits on `16749da`. Gates below ran on `16749da` before this file was committed.

## Commits

- `2ee1e96` emits one link rectangle per visual line. A link change flushes the measured group, and `place_line` records that group's advance.
- `a42e764` adds `Paragraph::link` and `Band::left_link` / `right_link`. An empty URI is stored on the builder.
- `36b1ca4` rustfmts the layout tests.
- `16749da` pins the recording-canvas cases from the synthesis test plan, including the grafted cases.

## Gates

Run on `16749da`. Each command exited 0.

- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`. 197 library tests passed, 1 ignored. 65 tests in `tests/document.rs` passed. 0 doctests.
- `cargo test --features backend-krilla`. Same counts. `backend-krilla` is already a default feature, so this command runs the same suite.

`text_link_writes_a_pdf` asserts the bytes start with `%PDF`.

`use krilla` appears only under `src/backend/`.

## Behavior pinned

- `A` + linked `BC` + `D` draws one `AnnotateLink`. The rectangle starts at the right edge of `A`, matches the `BC` advance, sits on the line-box top, and is `LINE` tall.
- Linked `A A` on a frame one `A` wide draws two annotations. The second rectangle's `y` is `LINE`.
- Body `A`, bold `B`, and body `C` inside one `link` draw three glyph runs and one rectangle whose width is the sum of the three advances.
- Adjacent URIs `a`, `b`, unlinked, `a` stay three rectangles and do not overlap.
- Two `link` calls with the same URI and face draw one rectangle. A face change inside that URI still draws one rectangle.
- A blank inside a link draws two rectangles and one stroke. The gap between the rectangles is 30 pt.
- A 10 pt first-line indent, a second column, and a list item each put the rectangle on the glyph `x`. The list body is at `x` = 18.
- Two linked lines on a page that fits one line put both rectangles at the body top, including a 20 pt top margin. Page 2 does not continue page 1's `y`.
- Header `left_link` / `right_link` and footer `left_link` match the slot glyphs, the side margins, end alignment, and the band `y`.
- A header URI that includes `page().number` is `https://example.com/1` on page 1 and `https://example.com/2` on page 2.
- `p.link("", ...)` and `Band::right_link("", ...)` return `Error::EmptyLink`. The link closure does not run. The recording has no commands, so there is no `StartPage`.
- `" "`, `mailto:a@b.c`, and `not a uri` are stored unchanged.
- Empty linked text draws no `AnnotateLink`.
- A linked `ABC` matches the unlinked command list once `AnnotateLink` is removed. Font, size, color, and position stay the same, and no stroke is added.
- A cell string and `behind` `text_at` in the same document as `p.link` produce one annotation, the paragraph one.
- Center and end alignment, checked inside the crate, put the rectangle on the placed glyph `x`.

## Remaining gaps

- Cell strings, `Layer::text_at`, list markers, and images do not take a URI.
- A link does not change color and does not draw an underline.
- In-document page and anchor targets are not implemented. `LinkTarget` remains an external URI.
- Paragraph alignment is not on the public DSL. The center and end checks set `Style.align` inside the crate.
- `place_line` skips a non-positive advance. The fixture has no zero-advance Latin glyph, so a document test does not paint that branch. Empty text is covered.
- `src/canvas.rs`, `src/error.rs`, and `src/backend/` were not changed.
- No pull request. This branch has no remote to open one against.
