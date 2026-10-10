# Candidate 1 — Canvas slice 2 rationale

## Problem

Slice 2 must introduce the layout-facing drawing boundary: a krilla-free `Canvas` trait, a test double that records paint in order, and the glyph/image/link payloads the adapter will later consume. The shape is constrained by slice-1 branded geometry (no bare f32 sizes on the public surface), top-left / y-down pt space, and a minimal error enum that today only knows length construction failures. Flow-inline canvas is deferred; this is the imperative draw API paginate will call. The non-obvious part is how much lifecycle and validation to encode in types versus leaving to prose and backend checks.

## Usage (caller's view)

Callers hold `&mut dyn Canvas`. They set document metadata once, loop `start_page` → draw → `finish_page`, and never call a trait-level document finish. Tests construct `RecordingCanvas`, run the same calls, and assert on `commands()` with 0.01 pt tolerance—not PDF bytes. Hyperlinks are written as URIs only (提案 §8); empty URIs must fail before anything is queued. See the three call sites in `SKETCH.md` (README loop, paginate body, glyph placement test).

## Shape

**Single deep module (`canvas.rs`)** owns the trait, command enum, recording implementation, and test helpers. `DocumentInfo` and `FontId` live in their placeholder modules as inert data/newtypes so `lib.rs` can re-export without pulling document builders into this slice.

**`CanvasCommand` is the test-facing domain log**: each trait method appends one variant with owned snapshots (`Path` clone, `GlyphRun` clone). Tests match on the enum instead of a parallel stringy protocol. The enum is `#[non_exhaustive]` so backend-only commands can appear later without breaking callers who only pattern-match in tests.

**`LinkTarget` validates at the boundary** (`LinkTarget::uri`). `annotate_link` takes `LinkTarget`, not `&str`, so `EmptyLink` is enforced once and both `RecordingCanvas` and `KrillaCanvas` stay thin—no duplicated empty-string checks (boundary-discipline).

**`GlyphRun.font_size_pt` uses `Length<Positive>`** instead of design-draft bare `f32`, aligning with slice-1 invariants; the adapter reads `.as_pt()` when calling krilla.

**`RecordingCanvas` enforces page phase in the recorder** via a private `RecordingPhase` and two error variants (`CanvasNoPageOpen`, `CanvasPageOpen`). That encodes “draw only inside an open page” without a type-state `Canvas` that would break object-safe `dyn Canvas`. The krilla adapter will use its own surface/lifetime rules; it does not need to surface these two errors if its internal state makes them unreachable.

**Interface depth:** The public trait stays the eight methods from the design Canvas section—no extra staging hooks, no expose of pending link queues, no wire types. Complexity hidden: command serialization for tests, page-phase policy in the recorder, URI validation in `LinkTarget`. Complexity exposed: ordered lifecycle (start/finish page), normalized glyph advances, and the fact that links are queued until page end (callers only see `annotate_link`; flush is inside `finish_page` on the adapter).

Load-bearing invariant encoding: geometry validity remains in `Path` / `Stroke` / `Size` construction; canvas does not re-validate non-empty paths or positive stroke width in the recorder.

## Synthesis decision

*(Arena picker fills this section.)*

## Tradeoffs accepted

- We accept cloning `Path` and `GlyphRun` on every record call in exchange for a log that is fully self-contained and equality-comparable in tests without lifetimes on `CanvasCommand`.
- We accept two canvas-specific `Error` variants for phase misuse in exchange for deterministic test failures instead of silent log corruption or panics.
- We accept keeping the trait procedural (not type-state pages) in exchange for a stable `dyn Canvas` hook for paginate, pushing lifecycle enforcement into `RecordingCanvas` and the adapter separately.
- We accept `LinkTarget` allocating an owned `String` in exchange for passing validated targets by value into `annotate_link`, so the adapter never sees raw caller strings again.

## Alternatives considered

- **Type-state `Canvas` (`DocumentOpen` / `PageOpen`)** — Encodes lifecycle in the type system but prevents a single `&mut dyn Canvas` across paginate without generic associated types or erasure tricks. Loses on interface depth for the main caller (paginate would carry generic state). Rejected.

- **Flat `annotate_link(rect, &str)` with validation inside each impl** — Matches the design draft literally but duplicates empty-URI policy and tempts backend-only validation, leaking the rule into tests and adapter. Rejected in favor of `LinkTarget`.

- **Hierarchical log (`Vec<PageRecord { size, ops }>`) built only on `finish_page`** — Hides invocation order between pages and complicates asserting that `annotate_link` happened before `finish_page` on the same page without re-deriving order. Rejected; flat `CanvasCommand` list is the single source of truth for ordering.

- **Split `canvas/command.rs` + `canvas/recording.rs` now** — Temporal decomposition by file without distinct ownership of invariants. Rejected until the module exceeds a readable size; one deep module for slice 2.

## Open questions and risks

- Should `set_document_info` be callable after pages have started (design: metadata before first page on adapter)? This candidate forbids it in `RecordingCanvas` when a page is open; should paginate be allowed to call it only once ever, or only before the first `start_page`?
- Are `CanvasNoPageOpen` / `CanvasPageOpen` acceptable on the public `Error` enum, or should phase checks be `debug_assert` in tests only with `EmptyLink` as the sole new variant for slice 2?
- `FontId::for_test` — keep `#[doc(hidden)]` in crate root tests, or expose only under `cfg(test)`?

## Next implementation step

Implement `DocumentInfo`, `FontId`, `CanvasCommand`, `LinkTarget`, the `Canvas` trait, and `RecordingCanvas` with the lifecycle unit tests in `src/canvas.rs`, adding `EmptyLink` (and optionally phase errors) to `error.rs`.
