# Candidate 2 rationale

## Problem

This slice needs a backend-neutral drawing boundary that preserves page order, positioned geometry, glyph identity, image identity, and links without leaking krilla or pulling layout policy into the canvas. The existing geometry already encodes finite/positive point lengths and top-left, y-down coordinates; the design must reuse it. The non-obvious choice is whether the protocol should be spread across trait methods and then reconstructed as a separate recording enum, or represented once as owned domain data that both real and recording canvases interpret.

## Usage (caller's view)

The caller constructs an owned `CanvasCommand` and submits it through `Canvas::execute`. A page is visible as `StartPage`, drawing/annotation commands, and `FinishPage`; metadata is `SetDocumentInfo`. Layout can pass `&mut dyn Canvas`, while a custom backend implements one exhaustive command match. Tests call the same API and inspect `RecordingCanvas::commands()` in exact order, comparing point coordinates with absolute error `<= 0.01`.

Opaque `FontId` and `ImageId` values come only from crate-owned resource tables. `GlyphRun` and `DecodedImage` expose read access needed by backend implementers but keep construction crate-private, so outside callers cannot forge a run whose byte ranges or normalized advances bypass measurement.

## Shape

`CanvasCommand` is the single source of truth for the rendering protocol. It owns paths, strings, runs, and image descriptors, allowing either immediate consumption or retention without a second clone-and-translate layer. `Canvas` has one method, `execute`, and deliberately has no convenience methods: wrappers such as `draw_image` would be pass-through methods and would recreate the shallow interface this design removes.

The command enum makes document and page boundaries explicit in the same ordered value stream as paint operations. It does not pretend Rust typestate can guarantee balanced pages through a trait object; the trusted paginator owns ordering, while each stateful backend interprets the explicit boundaries. `RecordingCanvas` remains a pure recorder rather than a second lifecycle implementation. This keeps one source of truth per invariant.

Geometry reuses `Size`, `Rect`, `Path`, `Paint`, and `Stroke`. Font size becomes `Length<Positive>` rather than retaining the draft's bare `f32`. Pixel dimensions use `NonZeroU32`. Normalized glyph advance remains dimensionless `f32`, but its constructor is crate-private and the measurement boundary is responsible for validating it before creating a run. This avoids falsely branding a dimensionless ratio as a geometric length.

`DocumentInfo` lives in `document`, `FontId` in `fonts`, and the rendering protocol plus image handle in `canvas`. Those are knowledge/allocator boundaries, not execution stages. No flow node, document builder, decoder, measurement API, backend, or finish-to-bytes operation enters this slice.

The interface is deep: one object-safe method hides command retention, backend resource lookup, link queuing, and eventual krilla adaptation. The complexity intentionally exposed is only the stable rendering vocabulary that layout and backend must already share. Boundary validation is limited to what this slice can establish honestly: branded geometry, nonzero IDs/dimensions, opaque producer-only run construction, and rejection of an empty link before the command is accepted.

## Synthesis decision

## Tradeoffs accepted

- We accept public exposure of the command vocabulary in exchange for eliminating a duplicated trait-method protocol and recording representation.
- We accept one allocation-owning move per command in exchange for permitting canvases to retain commands safely without lifetime parameters or cloning borrowed payloads.
- We accept that balanced page order is a producer/backend contract rather than recorder typestate in exchange for object safety and a single trait usable as `&mut dyn Canvas`.
- We accept crate-private constructors for glyph and image payloads in exchange for preventing external code from forging resource handles and measurement invariants.
- We accept three ownership modules instead of putting every payload in `canvas.rs` in exchange for keeping document metadata and font identity at their long-term knowledge boundaries.

## Alternatives considered

- **Method-per-operation `Canvas` plus a separate `PaintCommand` recorder enum:** rejected because every new operation must be represented twice, and `RecordingCanvas` becomes a row of pass-through clone methods. It exposes a larger interface while hiding less.
- **GAT/typestate page handles (`start_page -> PageCanvas<'_>`):** rejected because it splits one drawing boundary into multiple public traits, prevents straightforward trait-object use, and still cannot force callers to consume a page with `finish_page` rather than drop it.
- **Closure transaction (`canvas.page(size, |page| ...)`):** rejected because callback error/lifetime machinery becomes public API, backends must hold a live page borrow across foreign callback code, and metadata plus finalization still require a second protocol.
- **Put `DocumentInfo`, `FontId`, and all constructors in `canvas.rs`:** rejected because it makes canvas the owner of document and font-table decisions merely because they happen to cross the drawing boundary; that is temporal decomposition and creates a later breaking move.

## Open questions and risks

- Will a future requirement need non-URI link targets, and if so should that become a new command variant rather than broadening the current `uri` field?
- Can the font measurement slice guarantee finite `advance_norm` and valid UTF-8 byte boundaries before calling the crate-private constructors, or will it need one narrowly scoped glyph-run construction error?
- Should whitespace-only URIs remain valid exactly as the current “empty only” rule implies?

## Next implementation step

Add `DocumentInfo` and opaque IDs, then implement `CanvasCommand`, `Canvas`, and `RecordingCanvas` with the ordered-command and empty-link unit tests before adding any backend.
