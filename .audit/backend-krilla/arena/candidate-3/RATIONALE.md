# Candidate 3 rationale

## Problem

Knife 3 turns a finished `Canvas` sequence into PDF bytes through krilla 0.8.2, and only through krilla. `Document::start_page_with` returns `Page<'_>`, and `Page::surface` returns `Surface<'_>`. Those three values cannot live in one struct while `unsafe` is forbidden, and `Page::surface` wipes the page if it is called twice. Annotations have to wait until that surface borrow ends. `quire::Error` is `Copy` today and only covers length construction plus `EmptyLink`. The full design-draft error enum, glyph drawing, and image decode are later knives. `Canvas` is already shipped and stays as it is. Geometry is branded `Length`, so the design draft's `p.x` and `r.width` field reads do not compile.

## Usage (caller's view)

The caller is an in-crate test or the future in-crate writer. They construct `KrillaCanvas`, use the existing `Canvas` methods, then call `finish(self) -> Result<Vec<u8>, EmitError>`. A public user of the crate keeps using `RecordingCanvas` and never sees krilla.

The quickstart strokes the probe path `(10, 20) -> (40, 20) -> (40, 50)` on a 200 by 200 page, adds the link rect `(10, 30, 80, 44)`, finishes the page, and checks that `finish` starts with `%PDF`. An empty `start_page` plus `finish_page` is also a PDF. `draw_glyph_run` and `draw_image` return `Ok(())` from the trait and make `finish` return `EmitError::Deferred { glyphs, images }` with no bytes. Identity compares `to_krilla_point` with `krilla::geom::Point::from_xy`.

## Shape

`KrillaCanvas` holds a `Session`. `Open` stores `DocumentInfo`, closed `PageBuf` values, an optional open `PageBuf`, and two deferred counters. `Fault` stores the first sequencing `EmitError`. A `PageBuf` has a `Size`, a `Vec<PaintOp>`, and a `Vec<QueuedLink>`. `PaintOp` is fill or stroke. There is no glyph arm and no image arm.

`finish` consumes the canvas. It returns the stored fault, else `Deferred` when either counter is non-zero, else `PageStillOpen` when a page is still open, else `NoPage` when nothing closed, else `emit`. `emit` is the only function that calls `Document::new_with`, `set_metadata`, `start_page_with`, `surface`, and `Document::finish`. The surface is taken once and consumed with `surface.finish()` before the link loop. Metadata is applied before that loop. `creation_date` stays unset. Serialize settings are pretty, uncompressed, and `enable_tagging: false`, because a struct update over `Default` would leave tagging on.

Convert functions read `as_pt()` through `x()`, `y()`, `origin()`, `size()`, `width()`, and `height()`. They do not negate y. krilla `Option` failures become `RejectedPage`, `RejectedRect`, `RejectedPath`, or `RejectedOpacity`. `Document::finish`'s `Display` becomes `EmitError::Finish(String)`.

Per model-the-domain, the link-after-surface rule is two vectors plus the consume of `Surface`. Per type-system-discipline, deferred drawing is a counter, so the emitter has no match arm that could call `draw_glyphs` or `draw_image`, and `quire::Error` stays the `Copy` enum it is. Per boundary-discipline, brand and URI checks stay in `geom` and `LinkTarget`. `emit` trusts `PageBuf` and only handles krilla's `Option` and `KrillaResult`. Per encode-lessons-in-structure, `session.rs` imports no krilla, so a `Document` field cannot appear there without a new dependency. Per make-operations-idempotent, `finish` consumes `self`, `Drop` does not call `finish`, and a failed emit returns no `Vec<u8>`. Per experience-first, the draw loop matches `RecordingCanvas` method for method, and the PDF-only decision sits on `finish`. Per laziness-protocol and minimize-reader-load, three backend files cover the question, and the crate's public type list does not grow. Per foundational-thinking, `PageBuf` is the structure later glyph work extends. Per test-behavior-not-implementation, tests assert `%PDF`, the URI, bit-identical convert results, `EmitError::Deferred`, and the operands actually present in the file. Per separate-before-serializing-shared-state, one `emit` call owns the only `Document`. Pages are a loop of locals, not a shared surface.

Interface depth. The crate's public surface gains the feature dependency and `default = ["backend-krilla"]`. It gains no type. Callers do not choose serialize settings, pen order, annotation timing, or krilla error types. That policy sits inside `emit` and `finish`.

## Synthesis decision

TBD. Left for the arena synthesizer.

## Tradeoffs accepted

- We accept `Ok(())` from glyph, image, and illegal canvas order, in exchange for leaving `Canvas` and `quire::Error` untouched. The failure is visible on `finish`.
- We accept refusing the whole document when a glyph or image was recorded, even if the page also had a path, in exchange for never writing a PDF that dropped those draws.
- We accept pretty, uncompressed output from the only `finish` path, in exchange for a content stream the probe can read. There is no production caller on this knife.
- We accept `EmitError` as a second error type inside `backend`, in exchange for keeping `quire::Error` `Copy` and out of the PDF layer.
- We accept a guessed operand list (`10 180`, `40 180`, `40 150`, annot `10 126 90 170`) as the first assertion, in exchange for a rule that a mismatch edits the literal and not `convert.rs`. The guess comes from `page_root_transform`. It is not a measurement yet.

## Alternatives considered

Replay of `CanvasCommand` inside `finish`. The public shape is the same size, and the emitter must special-case links and skip glyphs inside one enum. Those states stay representable in the log. The split tape removes them from `emit`'s input.

Storing `krilla::Document` on `KrillaCanvas` and drawing live, which is the design draft. `Page<'_>` and `Surface<'_>` cannot sit in that struct under `forbid(unsafe_code)`, and a second `surface()` call clears the page.

Extending `quire::Error` with `Backend(String)` and `InvalidGeometry` on this knife. That drops `Copy`, publishes PDF failures next to length errors, and pulls the draft enum forward. The private `EmitError` keeps that draft where it is.

## Open questions and risks

- If krilla still writes a varying id when `creation_date` is unset, do we keep substring asserts only, or is there a supported way to pin the id? The sketch assumes substring asserts and does not snapshot the file.
- Should `default = ["backend-krilla"]` land in this knife? The sketch says yes, because design PR4 turns the feature on by default and otherwise `cargo test` skips the probe.
- When a glyph is drawn and the page is also left unclosed, `finish` reports `Deferred` and hides `PageStillOpen`. Is deferred-first the order later `to_pdf` should keep?

## Next implementation step

Pin krilla `0.8.2` in `Cargo.toml`, replace the backend stub with `convert.rs` identity functions, and land the bit-compare test before writing `Session` or `emit`.
