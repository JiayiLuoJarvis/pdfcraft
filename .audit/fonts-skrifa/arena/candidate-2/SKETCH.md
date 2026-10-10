# Candidate 2 sketch

## Usage

Knife 4 adds an internal font registration and measurement path. It does not add a document DSL or another public font API. `FontId` remains the only public font type.

Measurement works without `backend-krilla`:

```rust
let mut fonts = FontStore::new();
let body = fonts.register(FontSource {
    bytes: Arc::new(include_bytes!("../../tests/fixtures/quire-subset.ttf").to_vec()),
    index: 0,
})?;

let measured = measure_text(&mut fonts, body, "A第")?;
assert_eq!(measured.glyphs.len(), 2);

let metrics = fonts.metrics(body)?;
let line = line_metrics(metrics, Positive::pt(12.0)?, 1.2)?;
assert!(line.descent < 0.0);
```

Layout converts the measured glyphs into the existing `GlyphRun`. `RecordingCanvas` keeps its current constructor and records the run without a font store:

```rust
let run = GlyphRun {
    font: body,
    font_size: Positive::pt(12.0)?,
    color: Color::rgb(0, 0, 0),
    origin: Point::try_pt(72.0, 100.0)?,
    glyphs: measured
        .glyphs
        .iter()
        .map(|glyph| GlyphPlacement {
            glyph_id: glyph.glyph_id,
            advance_norm: glyph.advance_norm,
            byte_range: glyph.byte_range.clone(),
        })
        .collect(),
    text: measured.text,
};

let mut recording = RecordingCanvas::new();
recording.start_page(A4)?;
recording.draw_glyph_run(&run)?;
recording.finish_page()?;
```

The private PDF adapter receives an immutable store after measurement has finished. `draw_glyph_run` resolves the `FontId` and copies the resolved font material into the buffered mark:

```rust
let fonts = Arc::new(fonts);
let mut canvas = KrillaCanvas::new(Arc::clone(&fonts));
canvas.start_page(A4)?;
canvas.draw_glyph_run(&run)?;
canvas.finish_page()?;
let pdf = canvas.finish()?;
assert!(pdf.starts_with(b"%PDF"));
```

An id that is absent from this store fails while the adapter records the run:

```rust
let mut unknown = run.clone();
unknown.font = FontId::from_raw(999);

let mut canvas = KrillaCanvas::new(Arc::clone(&fonts));
canvas.start_page(A4)?;
let error = canvas.draw_glyph_run(&unknown).unwrap_err();
assert!(matches!(error, Error::UnknownFont { id: 999 }));
```

## Data shape

`FontStore` owns skrifa-facing data only. It stores bytes, a face index, unscaled metrics, and an advance cache. It never stores `skrifa::FontRef`, because a `FontRef` borrows the same bytes that the face owns.

```rust
// src/fonts.rs
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct FontSource {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct FaceMetrics {
    pub(crate) units_per_em: f32,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) leading: f32,
}

pub(crate) struct FontFace {
    pub(crate) source: FontSource,
    pub(crate) metrics: FaceMetrics,
    pub(crate) advances: HashMap<u32, f32>,
}

pub(crate) struct FontStore {
    faces: Vec<FontFace>,
}

#[derive(Clone)]
pub(crate) struct ResolvedFont {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) skrifa_units_per_em: f32,
}

impl FontStore {
    pub(crate) fn new() -> Self {
        todo!("knife 4")
    }

    pub(crate) fn register(&mut self, source: FontSource) -> Result<FontId> {
        todo!("knife 4")
    }

    pub(crate) fn metrics(&self, id: FontId) -> Result<&FaceMetrics> {
        todo!("knife 4")
    }

    pub(crate) fn face_mut(&mut self, id: FontId) -> Result<&mut FontFace> {
        todo!("knife 4")
    }

    pub(crate) fn resolve(&self, id: FontId) -> Result<ResolvedFont> {
        todo!("knife 4")
    }
}
```

Registration assigns one-based ids only after skrifa accepts the face. Empty input, input above 32 MiB, an invalid face index, invalid bytes, or zero units per em returns `Error::Font`. `resolve` clones only the `Arc`, not the byte vector.

`text::measure` maps each Unicode scalar directly. Grapheme segmentation belongs to knife 5.

```rust
// src/text/measure.rs
pub(crate) struct MeasuredGlyph {
    pub(crate) scalar: char,
    pub(crate) glyph_id: u32,
    pub(crate) advance_font_units: f32,
    pub(crate) advance_norm: f32,
    pub(crate) byte_range: Range<usize>,
}

pub(crate) struct MeasuredText {
    pub(crate) text: String,
    pub(crate) glyphs: Vec<MeasuredGlyph>,
    pub(crate) advance_norm: f32,
}

pub(crate) struct LineMetrics {
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) leading: f32,
    pub(crate) line_height: f32,
    pub(crate) baseline: f32,
}

pub(crate) fn measure_text(
    fonts: &mut FontStore,
    font: FontId,
    text: &str,
) -> Result<MeasuredText> {
    todo!("knife 4")
}

pub(crate) fn line_metrics(
    face: &FaceMetrics,
    font_size: Length<Positive>,
    factor: f32,
) -> Result<LineMetrics> {
    todo!("knife 4")
}
```

`measure_text` uses `FontRef::from_index`, `charmap().map`, and unscaled `advance_width`. A missing cmap entry returns `Error::MissingGlyph`. The code has no glyph-zero branch. An absent advance returns `Error::Font`. It calculates `advance_norm` once as `advance_font_units / units_per_em`.

`line_metrics` uses one formula:

```text
scale       = font_size / units_per_em
ascent      = face.ascent * scale
descent     = face.descent * scale
leading     = face.leading * scale
natural     = ascent - descent + leading
line_height = max(natural, font_size * factor)
baseline    = ascent
```

The backend mark owns everything that emission needs:

```rust
// src/backend/krilla_canvas.rs
pub(super) enum Mark {
    Fill {
        path: Path,
        paint: Paint,
    },
    Stroke {
        path: Path,
        paint: Paint,
        stroke: Stroke,
    },
    Glyphs {
        font_bytes: Arc<Vec<u8>>,
        font_index: u32,
        skrifa_units_per_em: f32,
        font_size: Length<Positive>,
        color: Color,
        origin: Point,
        glyphs: Box<[GlyphPlacement]>,
        text: Box<str>,
    },
}

pub(crate) struct KrillaCanvas {
    font_store: Arc<FontStore>,
    info: Option<DocumentInfo>,
    closed_pages: Vec<PagePlan>,
    open_page: Option<OpenPage>,
    failure: Option<Error>,
    serialization: Serialization,
}

impl KrillaCanvas {
    pub(crate) fn new(font_store: Arc<FontStore>) -> Self {
        todo!("knife 4")
    }

    #[cfg(test)]
    pub(crate) fn new_observable_for_test(font_store: Arc<FontStore>) -> Self {
        todo!("knife 4")
    }

    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        todo!("validate state, drop font_store, then call emit::render")
    }
}
```

`draw_glyph_run` performs one store lookup. It poisons the canvas with `UnknownFont` if the lookup fails. On success, it clones the byte `Arc`, the placements, and the text into `Mark::Glyphs`. `finish` drops `font_store` before calling `emit::render`. The renderer receives only metadata, closed page plans, and serialization settings.

## Data flow

1. `FontStore::register` parses the source with skrifa and records unscaled metrics.
2. `measure_text` maps each scalar through the charmap and reads or caches its unscaled advance.
3. Layout copies the measured glyph ids, byte ranges, and normalized advances into `GlyphRun`.
4. `KrillaCanvas::draw_glyph_run` resolves `run.font` and records a self-contained `Mark::Glyphs`.
5. `KrillaCanvas::finish` removes the store from the emission path.
6. `emit.rs` creates a krilla font from the mark bytes, compares units per em, creates `KrillaGlyph` values, and calls `Surface::draw_glyphs`.

Neither `fonts.rs` nor `krilla_canvas.rs` imports krilla. `emit.rs` is the only file that constructs `Document`, `Page`, `Surface`, or `krilla::text::Font`.

## Emission

```rust
// src/backend/emit.rs
fn draw_mark(surface: &mut Surface<'_>, mark: &Mark) -> Result<()> {
    todo!("dispatch Fill, Stroke, or Glyphs")
}

fn draw_glyphs(
    surface: &mut Surface<'_>,
    font_bytes: &Arc<Vec<u8>>,
    font_index: u32,
    skrifa_units_per_em: f32,
    font_size: Length<Positive>,
    color: Color,
    origin: Point,
    glyphs: &[GlyphPlacement],
    text: &str,
) -> Result<()> {
    todo!("construct krilla Font and KrillaGlyph values, then draw_glyphs")
}
```

`draw_glyphs` performs these operations:

1. Construct `krilla::text::Font` from `Data::from(Arc::clone(font_bytes))` and `font_index`. Map `None` to `Error::Font`.
2. Compare the krilla units per em with `skrifa_units_per_em`. A delta at or above `0.01` returns `Error::FontMetricsMismatch`.
3. Convert each placement to `KrillaGlyph::new`. Pass `advance_norm` unchanged. Set both offsets and `y_advance` to zero.
4. Set the fill from `color` and clear the stroke.
5. Call `surface.draw_glyphs(origin, glyphs, font, text, font_size, false)`.

The renderer does not multiply `advance_norm` by the font size and does not divide it by units per em again. It does not call `draw_text`.

## Errors

Knife 4 adds these variants to `Error`:

```rust
Font {
    index: u32,
    message: String,
},
MissingGlyph {
    font: FontId,
    scalar: char,
},
FontMetricsMismatch {
    skrifa_upem: f32,
    krilla_upem: f32,
},
UnknownFont {
    id: u32,
},
```

The error boundary is fixed:

- `Font` covers empty, oversized, unreadable, malformed, or face-index-invalid bytes and a missing unscaled advance.
- `MissingGlyph` covers only `Charmap::map` returning `None`.
- `UnknownFont` occurs in `KrillaCanvas::draw_glyph_run`, before the mark is recorded.
- `FontMetricsMismatch` occurs in `emit.rs`, where both libraries' metrics are available.

## Module map

| File | Knife 4 change |
| --- | --- |
| `Cargo.toml` | Add unconditional `skrifa = "=0.47.0"`. |
| `src/error.rs` | Add the four font errors and their display text. |
| `src/fonts.rs` | Add `FontSource`, `FaceMetrics`, `FontFace`, `FontStore`, registration, and immutable resolution. |
| `src/text/measure.rs` | Add scalar mapping, unscaled advances, normalization, and line metrics. |
| `src/backend/krilla_canvas.rs` | Add `Mark::Glyphs`, take `Arc<FontStore>` in constructors, and resolve ids while recording. Keep this file krilla-free. |
| `src/backend/emit.rs` | Construct krilla fonts, check units per em, and draw glyphs. |
| `tests/fixtures/` | Add one small redistributable subset with ASCII, digits, U+2022, `第`, `页`, `共`, and at least one CJK advance fixture. |

`src/canvas.rs` keeps the existing `Canvas` and `GlyphRun` signatures. `src/lib.rs` continues to re-export only `FontId`. No `FontStore`, `FontSource`, backend type, or krilla type becomes public.

## Verification shape

- A no-default-features test registers the fixture, asserts `descent < 0`, pins one ASCII advance and one CJK advance, and checks bad and empty bytes.
- A missing scalar test matches `Error::MissingGlyph` and has no glyph-zero case.
- The existing `RecordingCanvas` test keeps the two-glyph `advance_norm * font_size` assertion.
- A backend test passes an unknown `FontId` and observes `Error::UnknownFont` from `draw_glyph_run`.
- A backend test submits `hmtx / upem + 0.25`. The observable PDF test pins the first observed `TJ` adjustment number and does not search for `Tj`.
- A backend test covers the units-per-em mismatch path separately.
- `unsupported_glyph_run_poisons_finish` is removed. The image poison test remains.

## Rejected shapes

- Emission does not receive `FontStore` or resolve `FontId`. That shape delays unknown-id errors and makes the page plan depend on external state.
- The backend does not maintain a second `FontId` registry of krilla fonts. That shape adds synchronization between skrifa registration and backend registration.
- `Canvas::draw_glyph_run` does not gain a resolver parameter. That shape would change every canvas implementation for a private adapter concern.
- `fonts.rs` does not call a feature-gated krilla hook during registration. Font registration and measurement must compile without the backend feature.
