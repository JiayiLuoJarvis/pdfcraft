# Candidate 4 — Canvas slice 2 rationale

## Problem

Slice 2 establishes the boundary between layout and vector rendering. Layout must emit positioned text, rules, images, and link annotations across multiple pages without depending on `krilla`, while tests require inspecting paint commands and exact coordinates without generating PDF bytes.

The core tension lies in the lifecycle contract. In PDF generation, documents consist of metadata and pages, while pages consist of 2D drawing operations. When an API collapses document-level lifecycle (`set_document_info`, `start_page`, `finish_page`) and page drawing (`fill_path`, `draw_glyph_run`, `annotate_link`) into a single flat trait, invalid states become representable: drawing before `start_page`, calling `start_page` twice, or leaving a page dangling after an error. Furthermore, `krilla` strictly requires dropping its page surface before flushing link annotations and finishing the page. If the trait leaves page finalization to uncoordinated caller calls, any early return (`?`) leaks active surfaces and drops queued links.

## Usage (caller's view)

Callers interact with a clean two-tier hierarchy: `Canvas` for document-level lifecycle and `PageCanvas` for 2D page painting.

Paginator code calls `canvas.set_document_info(&info)` and iterates through planned pages using `canvas.page(size, |page| { ... })` or `canvas.with_page(size, &mut |page| { ... })`. Leaf layout code (paragraphs, table cells, rules, headers) receives `&mut dyn PageCanvas`. It has direct access to `fill_path`, `stroke_path`, `draw_image`, `draw_glyph_run`, and `annotate_link`, but has zero access to document metadata or page creation methods, making page corruption impossible.

In tests, `RecordingCanvas` captures both views:
1. `canvas.pages()` returns committed `RecordedPage` structs with `.size()` and `.ops()`, allowing tests to inspect page 2 without knowing or caring how many paths were drawn on page 1.
2. `canvas.commands()` provides an ordered `CanvasCommand` stream for exact full-lifecycle sequence assertions.
3. Leaf components can be tested against a standalone `RecordingPageCanvas` without spinning up a document.

All coordinate assertions use `test_util::assert_point_near` with an absolute `0.01 pt` tolerance.

## Shape

The architecture enforces page boundaries through scoped transactions rather than runtime state machines or untyped command buses (`per boundary-discipline` and `encode-lessons-in-structure`).

`Canvas` provides `set_document_info` and `with_page`. Drawing operations exist exclusively on `PageCanvas`. Because a `PageCanvas` instance only exists inside the closure passed to `with_page`, calling drawing operations outside an open page is unrepresentable at compile time.

This scoped transaction directly solves the `krilla` adapter lifecycle:
1. The adapter calls `krilla::Document::start_page_with` and opens a surface.
2. The user closure executes against `PageCanvas`.
3. When the closure returns `Ok`, the adapter drops the surface, drains and adds the queued link annotations to the `krilla::Page`, and calls `page.finish()`.
4. If the closure returns `Err`, the uncommitted page is cleanly torn down without corrupting the document.

Interface depth: `Canvas` hides surface allocation, surface borrow management, link annotation queuing, and page finalization behind a single scoped method. Callers do not manage pending link queues, match internal command enums, or balance `start_page`/`finish_page` pairs.

`RecordingCanvas` provides dual-view observability: it records both structured `RecordedPage` entries and a flat, chronological `CanvasCommand` log. This avoids brittle global index calculations in multi-page tests while preserving complete auditability.

Geometry reuse: all dimensions use slice 1 branded lengths (`Size`, `Rect`, `Stroke`, `Paint`, `Point`, `Color`). `GlyphRun.font_size` is branded as `Length<Positive>`. Dimensionless normalized glyph advances remain `f32`.

Error boundary: `src/error.rs` adds only `Error::EmptyLink`. Lifecycle phase errors (`CanvasNoPageOpen`, `CanvasPageOpen`) are completely eliminated because invalid states cannot be expressed. `annotate_link` validates the URI immediately and rejects empty or whitespace-only strings at the boundary.

## Synthesis decision

*(Arena picker fills this section.)*

## Tradeoffs accepted

- We accept closure-based page scoping (`canvas.page(size, |p| ...)`) in exchange for eliminating runtime phase tracking and guaranteeing automatic surface cleanup and link flushing.
- We accept passing `&mut dyn FnMut` in `with_page` in exchange for 100% object safety across `&mut dyn Canvas`.
- We accept maintaining both `RecordedPage` vectors and a chronological `CanvasCommand` stream in `RecordingCanvas` in exchange for resilient page-isolated test assertions and full sequence verifiability.
- We accept cloning paint payloads (`Path`, `GlyphRun`, `String`) into the test log in exchange for self-contained, lifetime-free test assertions.

## Alternatives considered

- **Flat 8-method trait with runtime phase checks (Candidate 1 shape):** Exposes `start_page`, `finish_page`, and paint methods on the same interface. Rejected because it makes invalid lifecycle states representable at runtime, requiring runtime phase tracking and new error variants (`CanvasNoPageOpen`, `CanvasPageOpen`). Furthermore, if a caller returns early via `?`, `finish_page` is skipped, leaving the backend in a poisoned state with dangling surface borrows and uncommitted link queues. It also leaks page-creation capabilities to leaf layout functions.
- **Single-method command bus `execute(CanvasCommand)` (Candidate 2 shape):** Pushes all operations into an untyped command enum. Rejected as a shallow module anti-pattern (`per design-red-flags`). It strips away method signatures, forces callers to allocate and move enum structs on every draw call, provides no compile-time page lifecycle enforcement, and hides no complexity behind its interface.
- **Linear RAII Page Guards (`start_page -> PageGuard<'_>`):** Avoids closures by returning an RAII guard struct. Rejected because Rust's `Drop` cannot return a `Result<()>`. If link annotation emission or page finalization fails during teardown, the error must either be silently swallowed or panic, violating deterministic error handling.

## Open questions and risks

- Will future streaming requirements ever demand interleaving drawing across multiple open pages simultaneously? (Proposal §1 commits to strictly sequential flowable page generation, so sequential scoped pages match current and planned requirements).
- Should `annotate_link` validate URI syntax (e.g., checking for valid URI schemes) or remain strictly non-empty as settled in 提案书 §8?
- Should `RecordingCanvas` record aborted pages when `with_page` returns `Err`, or is dropping uncommitted page data the preferred behavior for test inspection?

## Next implementation step

Add `Error::EmptyLink` to `src/error.rs`, define `DocumentInfo` in `src/document.rs` and `FontId` in `src/fonts.rs`, then implement `Canvas`, `PageCanvas`, `RecordingCanvas`, and `test_util` in `src/canvas.rs`.
