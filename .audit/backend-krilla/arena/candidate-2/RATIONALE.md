# Buffered page plans at the krilla boundary

## Problem

The shipped `Canvas` trait opens and closes pages across separate calls. Krilla returns `Page<'_>` from `Document` and `Surface<'_>` from `Page`. A struct cannot own all three without self-reference, and this crate forbids `unsafe`. The adapter must also defer page annotations until the surface borrow ends. This knife has no font or image registry, but every `Canvas` method still needs defined behavior. The design must keep krilla private, preserve top-left coordinates, and produce testable PDF bytes without changing the trait.

## Usage (caller's view)

The crate-internal caller uses `KrillaCanvas` as an ordinary `Canvas`. It finishes each page, then consumes the adapter for bytes.

```rust
let mut pdf = KrillaCanvas::new();
pdf.set_document_info(&info)?;
pdf.start_page(A4)?;
pdf.stroke_path(&path, &paint, &stroke)?;
pdf.annotate_link(rect, target)?;
pdf.finish_page()?;
let bytes = pdf.finish()?;
```

Multiple pages use the same sequence. The page size may change between calls.

```rust
for planned in pages {
    pdf.start_page(planned.size)?;
    planned.paint(&mut pdf)?;
    pdf.finish_page()?;
}
let bytes = pdf.finish()?;
```

Text and images fail clearly in this knife. The first failure poisons the build, so a caller cannot ignore the error and get incomplete output.

```rust
assert!(matches!(
    pdf.draw_glyph_run(&run),
    Err(Error::UnsupportedCanvasOperation {
        operation: "draw_glyph_run"
    })
));
assert!(pdf.finish().is_err());
```

## Shape

`KrillaCanvas` stores `DocumentInfo`, closed `PagePlan` values, one optional `OpenPage`, and the first error. A page stores painted `Mark` values separately from `Link` values. `finish_page` converts `OpenPage` into `PagePlan`. No closed page can remain mutable.

The consuming `finish` method creates the krilla document and replays each page. It opens a page, creates one surface in a nested lexical scope, draws all marks, and drops the surface. It then adds links and finishes the page. The type shape matches krilla's ownership rules instead of trying to retain borrowed krilla values. This follows `principle-model-the-domain` and `principle-type-system-discipline`.

The adapter records a private page plan rather than public `CanvasCommand` values. `CanvasCommand` includes metadata and page lifecycle events. Replaying it would require a second protocol parser and would admit malformed command streams. `PagePlan` stores only renderable closed pages and partitions annotations at the exact borrow boundary. This follows `principle-encode-lessons-in-structure`.

The geometry boundary trusts `Length` brands. Conversion copies point values into krilla's top-left coordinate system without a y flip. A failed krilla constructor means that the backend contract changed, so the adapter returns `Error::Backend`. It does not repeat finite and positive checks throughout rendering. This follows `principle-boundary-discipline`.

The public API grows only through three backend-neutral `Error` variants. Krilla types remain under `src/backend/`. The private adapter hides page planning, metadata timing, surface lifetime, pen reset, annotation timing, and serialization settings behind the existing `Canvas` interface plus `finish`. The interface is deep because callers do not coordinate any of those rules.

`draw_glyph_run` and `draw_image` return `UnsupportedCanvasOperation` after they verify that a page is open. They do not panic, fake output, or retain unusable identifiers. Later knives add font and image registries and replace those errors with new private `Mark` variants. This follows `principle-subtract-before-you-add`.

`Error::Backend(String)` removes the current `Copy` derive. The string keeps krilla's diagnostic while avoiding a public krilla error type. The other new errors cover lifecycle misuse and deliberately deferred operations. This knife does not add speculative font, image, or generic geometry variants.

The default feature becomes `backend-krilla`. Krilla is pinned to `=0.8.2` with its defaults off and only `raster-images` on. CI checks both `--no-default-features` and `--features backend-krilla`. This keeps the canvas baseline usable while making the only production backend the normal build.

Tests call the adapter and inspect observed output. They assert `%PDF`, identity conversion, literal path operators, and the literal `/Annot` rectangle produced by krilla 0.8.2. They do not snapshot the full PDF. This follows `principle-test-behavior-not-implementation`.

## Synthesis decision

TBD by the arena orchestrator.

## Tradeoffs accepted

- We accept buffering page plans in memory in exchange for lexical borrows that need no self-reference or `unsafe`.
- We accept cloning paths, paints, links, and metadata in exchange for preserving the existing stateful `Canvas` trait.
- We accept losing `Error: Copy` in exchange for retaining the backend failure message.
- We accept that text and image documents fail in this knife in exchange for never producing a PDF with missing content.
- We accept separate mark and link collections in exchange for making krilla's surface-before-annotation rule structural.
- We accept a second CI feature configuration in exchange for testing both the backend and the canvas-only crate.

## Alternatives considered

- A live `Document`, `Page`, and `Surface` inside `KrillaCanvas` lost because the latter two borrow their owners. Making that shape compile requires self-reference, leaked storage, or `unsafe`. It also exposes lifetime mechanics to every `Canvas` method.
- A closure such as `with_page(|surface| ...)` would give krilla ideal lexical borrows. It lost because it changes the shipped `Canvas` interface and forces layout callers to coordinate a backend-specific page scope.
- A raw `Vec<CanvasCommand>` replay log would compile. It lost because the renderer must parse lifecycle events, reject malformed streams, and rediscover that links cannot run while a surface exists. It hides less complexity than the typed page plan.
- Rendering a temporary one-page document at every `finish_page` lost because krilla owns document-wide metadata, resources, and final serialization. Combining those independent PDFs would require another PDF writer, which the repository forbids.

## Open questions and risks

- Which literal content-stream coordinates and annotation rectangle does the krilla 0.8.2 experiment produce on the 200 pt probe page?
- Does krilla preserve enough readable metadata in the observable profile for the late `set_document_info` test, or should that test inspect only successful serialization?
- Will the font knife need `PagePlan` to hold a shared font registry for the whole document, or can the renderer own that registry beside the plans without changing the `Canvas` interface?

## Next implementation step

Add the Cargo feature and exact krilla pin, then implement identity conversion tests before adding `KrillaCanvas`.
