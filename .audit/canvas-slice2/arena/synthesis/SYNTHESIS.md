# Canvas slice 2 — synthesized sketch

## Synthesis decision

**Base:** Candidate 1 (multi-method object-safe `Canvas`, flat `CanvasCommand` log, branded geom).

**Agreed with** [Canvas arena cross-judge](fb46379d-8f87-4e5f-b974-c3e446b2d368): C1 highest total; C3/C4 hard-rejected as-is; C2 not base (shallow command bus).

**Grafts**

| From | Take | Skip |
| --- | --- | --- |
| C2 | `DocumentInfo` field set (`keywords: Vec`, `language`); `EmptyLink` unit variant only | `execute(CanvasCommand)` as the public trait; sealed GlyphRun constructors (tests need pub fields this slice; measurement can tighten later) |
| C3 | `DocumentInfo` lives in `canvas` (avoids `canvas → document` cycle when DSL later imports canvas) | GAT `PagePaint`, paint returns `()`, GlyphRunBuilder / `GlyphRunError` / `AdvanceNorm` public surface |
| C4 | Optional derived page view over the flat log (iterator grouping `StartPage`..`FinishPage`) | Dual persisted `pages`+`commands`; `with_page` closure API; whitespace-trim on URI |

**Rejects**

- C1 page-phase public errors (`CanvasNoPageOpen` / `CanvasPageOpen`): RecordingCanvas is append-only like the design draft. Producer owns ordering.
- C1 sample glyph command index bug (index 1 after `start_page`, not 2).
- C1 DocumentInfo `creator`/`producer` fields (adapter hardcodes creator `"quire"` later).

## Shape (implement this)

### Modules

```text
src/error.rs     + EmptyLink
src/fonts.rs     FontId(pub(crate) u32) + from_raw / as_u32
src/canvas.rs    DocumentInfo, Canvas, CanvasCommand, LinkTarget,
                 GlyphRun, GlyphPlacement, ImageId, DecodedImage,
                 RecordingCanvas, pt_near, derived pages() helper
src/lib.rs       re-export public canvas + FontId
src/document.rs  untouched placeholder
```

### `Error`

Add only:

```rust
EmptyLink,
```

Display: `"link URI is empty"`.

### `DocumentInfo` (in `canvas.rs`)

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
    pub language: Option<String>,
}
```

### `LinkTarget`

`LinkTarget::uri(&str) -> Result<Self>` rejects empty (not trim). `as_str()`.

### `Canvas` trait

Eight methods returning `Result<()>`, matching design + branded geom:

- `set_document_info(&DocumentInfo)`
- `start_page(Size)` / `finish_page()`
- `fill_path(&Path, &Paint)` / `stroke_path(&Path, &Paint, &Stroke)`
- `draw_image(&DecodedImage, Rect)`
- `draw_glyph_run(&GlyphRun)`
- `annotate_link(Rect, LinkTarget)` — already validated

### `CanvasCommand`

One variant per method; owned payloads; `#[non_exhaustive]`. Flat chronological log is the single source of truth.

### `GlyphRun`

Pub fields. `font_size: Length<Positive>`. `advance_norm: f32` (font_units/upem). Origin is baseline of first glyph, y down.

### `RecordingCanvas`

Append-only `Vec<CanvasCommand>`. No phase machine. `commands() -> &[CanvasCommand]`.

`pages()` returns a derived `Vec<RecordedPage>` by scanning the log (groups between StartPage and FinishPage; ops are paint/link commands only). Do not store a second write path.

`pt_near(a, b) -> bool` with `PT_ABS_TOLERANCE = 0.01`.

### Tests (behavior)

1. Order: info → start → fill → stroke → image → glyphs → link → finish.
2. Empty URI: `LinkTarget::uri("")` is `Err(EmptyLink)`; not recorded.
3. Glyph advances stored without `* font_size`; second glyph expected x computed in test as `origin.x + advance_norm * font_size`.
4. Coordinate assert with `pt_near`.
5. `pages()` derives one page with correct size and op count from the flat log.

### Out of scope

backend, fonts measure, layout, DSL builders, flow-inline canvas, `finish(self) -> Vec<u8>`.
