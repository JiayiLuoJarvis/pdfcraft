# Canvas slice 2 rationale

## Problem

Slice 2 is the drawing surface layout and tests share, with krilla still sealed out. Slice 1 already brands geometry: `Size`, `Rect`, `Path`, `Paint`, and `Stroke` cannot represent empty pages, empty paths, or zero-width strokes, and `Error` only reports length construction. `src/canvas.rs` is a stub. The design draft's `Canvas` is a flat list of `Result`-returning methods plus a `Vec<PaintCommand>` that can record a fill before `start_page`. That protocol is a caller convention. It also puts `DocumentInfo` next to the DSL, but the crate arrow is `document → layout → canvas`; a canvas method that names a document-module type becomes a cycle the moment the DSL calls the trait. This slice still has to record `set_document_info` and `annotate_link` in order, compare coordinates to 0.01 pt, and add `EmptyLink` without the rest of the draft's error enum. The flow-inline canvas element stays deferred.

## Usage (caller's view)

```rust
use quire_pdf::{
    pt_near, Canvas, Color, DecodedImage, DocumentInfo, FontId, GlyphRun, GlyphRunError, ImageId,
    LinkUri, PageOp, PagePaint, Paint, Path, Point, Positive, Record, RecordingCanvas, Rect, Size,
    Stroke, A4, LETTER,
};

fn draw_sample(canvas: &mut RecordingCanvas) -> quire_pdf::Result<()> {
    canvas.set_document_info(DocumentInfo::new().with_title("Letter"));

    let origin = Point::try_pt(72.0, 96.0)?;
    let run = sample_run(Positive::pt(12.0)?, origin).expect("spans sit on char boundaries");

    let mut page = canvas.start_page(A4);
    let rule = Path::line(origin, Point::try_pt(400.0, 96.0)?);
    page.stroke_path(
        &rule,
        Paint::solid(Color::rgb(0, 0, 0)),
        Stroke::new(Positive::pt(0.75)?),
    );
    let image = DecodedImage::new(ImageId::from_raw(7), 200, 100).expect("nonzero pixels");
    page.draw_image(image, Rect::try_pt(72.0, 140.0, 100.0, 50.0)?);
    page.draw_glyph_run(&run);

    let uri = LinkUri::new("https://example.com/a")?;
    page.annotate_link(Rect::try_pt(72.0, 86.0, 80.0, 14.0)?, &uri);
    page.finish();
    Ok(())
}

fn sample_run(
    size: quire_pdf::Length<Positive>,
    origin: Point,
) -> std::result::Result<GlyphRun, GlyphRunError> {
    let mut run = GlyphRun::build(FontId::from_raw(1), size, Color::rgb(0, 0, 0), origin, "Ab");
    run.place(11, 0.5, 0..1)?;
    run.place(12, 0.55, 1..2)?;
    run.finish()
}

/// Layout already holds trusted `Path` / `Stroke` values. Painting them cannot fail.
fn paint_rule<C: Canvas>(canvas: &mut C, size: Size, path: &Path, stroke: Stroke) {
    let mut page = canvas.start_page(size);
    page.stroke_path(path, Paint::solid(Color::rgb(0, 0, 0)), stroke);
    page.finish();
}

/// One checked URI, two line boxes. `finish` ends the page borrow, so the next
/// page starts in the same scope.
fn link_lines<C: Canvas>(canvas: &mut C, uri: &str) -> quire_pdf::Result<()> {
    let uri = LinkUri::new(uri)?;
    let mut page = canvas.start_page(A4);
    page.annotate_link(Rect::try_pt(72.0, 100.0, 120.0, 14.0)?, &uri);
    page.annotate_link(Rect::try_pt(72.0, 116.0, 40.0, 14.0)?, &uri);
    page.finish();
    let mut page = canvas.start_page(LETTER);
    page.finish();
    Ok(())
}

fn assert_sample_log(canvas: &RecordingCanvas) {
    let records = canvas.records();
    let Record::Info(info) = &records[0] else {
        panic!("info");
    };
    assert_eq!(info.title(), Some("Letter"));

    let Record::Page(page) = &records[1] else {
        panic!("page");
    };
    assert_eq!(page.size(), A4);
    assert!(matches!(page.ops()[0], PageOp::StrokePath { .. }));
    assert!(matches!(page.ops()[1], PageOp::Image { .. }));
    match &page.ops()[2] {
        PageOp::GlyphRun(run) => {
            let placed = &run.placements()[0];
            let user_advance = placed.advance().get() * run.font_size().as_pt();
            assert!(pt_near(run.origin().x().as_pt(), 72.0));
            assert!(pt_near(user_advance, 6.0));
            assert_eq!(placed.advance().get().to_bits(), 0.5f32.to_bits());
        }
        _ => panic!("glyphs"),
    }
}
```

Layout imports `Canvas` and `PagePaint`, holds `&mut impl Canvas`, and paints with `()`. `Result` appears where raw inputs enter: `LinkUri::new`, geom factories, and `GlyphRunBuilder`. Tests import `Record` / `PageOp` and read `records()` after the guard is gone. They do not build commands, and they do not compare PDF bytes.

## Shape

`Canvas::start_page` returns `Page<'a>: PagePaint` that exclusively borrows the canvas. Drawing methods live only on that guard. `finish(self)` seals and releases the borrow so another page can start in the same scope; `Drop` seals through the same function, and a second seal does not append a second page (per make-operations-idempotent). The open page's ops sit on the guard and move into the log at seal, so a reader never sees a half-built page (per separate-before-serializing-shared-state). `&mut` is the only writer.

The log is `Vec<Record>`: `Info(DocumentInfo)` or `Page(RecordedPage { size, ops })`. `PageOp` is fill, stroke, image, glyph run, or link, in call order. Links are not a side queue inside the recorder. A later adapter filters `PageOp::Link` out when it writes annotations; the test oracle keeps the call order. Sequential append and index are the access path. An image-id map is a backend table, not a second structure on this log.

Validation sits on values, and paint trusts them (per boundary-discipline):

- `LinkUri::new` is the only new `crate::Error` path (`EmptyLink`). `annotate_link` clones a `LinkUri` and returns `()`.
- `GlyphRunBuilder::place` is the only constructor of `AdvanceNorm` and `GlyphPlacement`. Non-finite advances and byte ranges that cannot slice the UTF-8 text become `GlyphRunError`. `finish` rejects zero glyphs. Fields are private, so a finished run cannot grow a dangling range.
- `DocumentInfo` builders turn `""` into unset and drop empty keywords. The log stores that value, so empty-string metadata policy has one implementation.
- `DecodedImage::new` returns `Option` and stores `NonZeroU32` edges. No image-byte or limit error in this slice.
- `Path`, `Stroke`, and `Size` are trusted as geom built them. Paint methods do not re-check them.
- `font_size` is `Length<Positive>`. The placement stores `AdvanceNorm` (font units / upem), not `advance * font_size`. One stored pair; the user-unit width is derived in the test or the future adapter (per single source of truth).

`FontId` is a `u32` newtype in `fonts.rs`. `canvas` depends on `fonts`; `fonts` does not depend on `canvas`. `DocumentInfo` lives in `canvas.rs` so the DSL can later own one and pass it down. `document.rs` gains no builders.

`Canvas` is generic, not `dyn`. The page associated type is the point of the trait; object safety would force the flat method list this shape exists to retire. Call chains stay in one file: `start_page` → `PagePaint` → private `seal`.

Interface depth: the writer surface is two traits. `Canvas` has two methods. `PagePaint` has the six draw operations plus `finish`. Callers do not sequence a legal command stream, do not match a backend error on a rectangle fill, and do not learn krilla's link queue. The complexity pulled inside is well-formed transcripts, URI emptiness, glyph-span slicing, metadata blank-stripping, and seal idempotence. What stays outside is deliberate: page size, geometry, glyph ids, pixel size, and URI text are caller data. The oracle enums are public because integration tests sit outside the crate; they are read-only products of the guard, with no public `push`.

Invariants in the types: no draw without a live page, no second open page, no empty URI, no zero pixel edge, no non-finite advance, no unsealed page inside `records()`. Invariants in tests and docs, because a trait cannot require `Drop`: every `PagePaint` impl seals on drop the way `RecordingPage` does.

## Synthesis decision

## Tradeoffs accepted

- We accept a non-object-safe `Canvas` in exchange for an open page that the borrow checker enforces.
- We accept `GlyphRunError` beside `crate::Error` in exchange for keeping span failures out of the enum this slice's recorder never returns. `paint_rule` stays `()`.
- We accept cloning `Path`, `GlyphRun`, and `LinkUri` into the log in exchange for a trait that borrows them, so a later adapter can paint from the borrow.
- We accept public oracle types (`Record`, `PageOp`) in exchange for tests that match a typed transcript at 0.01 pt.
- We accept that backend I/O errors are invisible on `PagePaint` in exchange for a trait recording can implement without fake `Ok(())`. A later `KrillaCanvas` latches a mid-page failure and returns it from its own `finish(self) -> Result<Vec<u8>, Error>`.
- We accept Drop-seals as a tested contract on `RecordingPage` in exchange for blank pages and forgotten `finish` still showing up as one sealed page. A trait cannot encode `Drop`.
- We accept `FontId::from_raw` on any `u32` in exchange for not inventing a font table in this slice.

## Alternatives considered

Flat `Canvas` with `start_page` / `finish_page` / paint all on one trait, each returning `Result<(), Error>`, stored as a flat `Vec<PaintCommand>` that includes `StartPage` and `FinishPage`. This is the design draft. The public surface is eight fallible methods whose legal order is prose. Every impl repeats the empty-URI check. The log can represent a fill outside a page, so tests can record states layout must never produce. Interface depth is low: the trait lists the stages, and the caller runs the state machine. The nested guard hides that machine.

Single `fn apply(&mut self, op: PageOp) -> Result<(), Error>`. The trait is one method, which looks deep and is shallow. The caller builds the wire enum, including lifecycle variants, and a private `page_open: bool` rejects bad order at runtime. The representation of a page is also the caller's protocol, so recording and krilla and layout all match on the same variants to do different jobs. Illegal transcripts are representable up to the check. The guard keeps `PageOp` as an output of recording, not an input.

Detached `PageScript` built off the canvas, then `commit(script)`. Also well-formed, and worse on depth. Layout would fill a script type that is a twin of `RecordedPage`, then the canvas would store or replay that twin. The krilla surface wants to draw during the page, while the script wants the page finished first. The guard already yields one owned page at seal without a second script type or a public command builder.

## Open questions and risks

- Is Drop-seals the right lost-guard policy, or should dropping without `finish` discard the page so a forgotten seal cannot emit a blank page?
- When measurement lands, should `GlyphRunError` stay a constructor error, or fold into `crate::Error` so `?` crosses from span building into `write_to`?
- Krilla can fail inside `surface` drawing. Is latching that until a non-trait `finish(self) -> Result<Vec<u8>>` acceptable, or does `PagePaint::finish` need to return `Result` in this slice so the signature survives the adapter?
- `DocumentInfo` blank-stripping copies a krilla metadata quirk into the domain value. Should the raw strings be recorded and the empty-skip happen only in the adapter?

## Next implementation step

Add `Error::EmptyLink` and `FontId`, then implement `LinkUri`, `GlyphRunBuilder`, and `RecordingCanvas::seal` in `src/canvas.rs` and land the order, tolerance, and drop-once tests.
