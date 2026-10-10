# Candidate 1 sketch. Skrifa-only `FontStore`, canvas holds `Arc<FontStore>`, emit builds krilla fonts

Knife 4 only. No line break, paginate, DSL, images, unicode-segmentation, or `LimitKind`.

## Usage first

### Crate-internal caller (the future `to_pdf`, and knife 4 tests)

```rust
use std::sync::Arc;

use crate::fonts::{FontSource, FontStore};
use crate::text::measure::{line_metrics, measure};

// 1. Registration phase. The store is mutable only here.
let mut store = FontStore::new();
let body = store.register(FontSource {
    bytes: Arc::new(std::fs::read("tests/fixtures/QuireSubset.otf")?),
    index: 0,
})?;                                     // Error::Font on empty, oversize, or unreadable bytes
let store = Arc::new(store);             // frozen; measure and draw share it

// 2. Measure. Output is already the draw input.
let glyphs = measure(&store, body, "第1页")?;  // Error::MissingGlyph { font, scalar } on any unmapped scalar
let width_pt: f32 = glyphs.iter().map(|g| g.advance_norm).sum::<f32>() * 12.0;
let line = line_metrics(store.metrics(body)?, 12.0, 1.2);

// 3. Draw. Same `Arc`, same glyph ids, same advance_norm.
let mut canvas = KrillaCanvas::new(Arc::clone(&store));
canvas.start_page(A4)?;
canvas.draw_glyph_run(&GlyphRun {
    font: body,
    font_size: Positive::pt(12.0)?,
    color: Color::rgb(0, 0, 0),
    origin: Point::try_pt(72.0, 72.0 + line.baseline)?,
    glyphs,
    text: "第1页".to_owned(),
})?;                                     // Error::UnknownFont { id } if `body` is not in this store
canvas.finish_page()?;
let pdf = canvas.finish()?;              // Error::FontMetricsMismatch surfaces here, at first draw of a font
```

### Public caller (unchanged this knife)

```rust
use quire_pdf::{FontId, GlyphRun, RecordingCanvas, Canvas};
// FontId::from_raw stays for tests. No FontStore, FontSource, or measure in the public surface.
```

### RecordingCanvas test (PR5 + PR6 test 2)

```rust
let mut store = FontStore::new();
let id = store.register(fixture())?;
let glyphs = measure(&store, id, "AB")?;
let mut rec = RecordingCanvas::new();
rec.start_page(A4)?;
rec.draw_glyph_run(&run(id, 10.0, (72.0, 100.0), glyphs, "AB"))?;
// second glyph x = 72 + advance_norm[0] * 10, against a literal from the fixture's hmtx
```

## Module map

| File | Change | May import |
| --- | --- | --- |
| `Cargo.toml` | `skrifa = "=0.47.0"`, not optional | |
| `src/fonts.rs` | `FontSource`, `FontStore`, `FontFace`, `FaceMetrics`, `MAX_FONT_BYTES` | skrifa, crate types |
| `src/text/measure.rs` | `measure`, `LineMetrics`, `line_metrics` | crate types only (calls `FontStore`) |
| `src/error.rs` | `Font`, `MissingGlyph`, `UnknownFont`, `FontMetricsMismatch`, `InvalidGlyphRun` | |
| `src/backend/krilla_canvas.rs` | holds `Arc<FontStore>`; `Mark::Glyphs(GlyphRun)`; validates run at record time | crate types only, no krilla |
| `src/backend/emit.rs` | `KrillaFonts` per render; `check_upem`; draw `Mark::Glyphs` | krilla |
| `src/lib.rs` | no change (`FontId` already exported) | |
| `tests/fixtures/` | small OFL subset: ASCII, 0-9, U+2022, 第 页 共 | |

`fonts.rs` and `text/measure.rs` never name krilla. Only `emit.rs` names `krilla::text::Font`.

## Types and signatures

### `src/fonts.rs`

```rust
//! Font registration and nominal metrics via skrifa.

use std::sync::Arc;

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::error::{Error, Result};

/// Fonts larger than this are `Error::Font`. Moves to `limits` with the limits PR.
pub(crate) const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub struct FontId(pub(crate) u32);           // unchanged, from_raw / as_u32 unchanged

/// Font bytes and face index. Single-face files use index 0.
#[derive(Debug, Clone)]
pub(crate) struct FontSource {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
}

/// Face metrics in font units, signs as skrifa reports them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FaceMetrics {
    pub(crate) units_per_em: f32,             // > 0, checked at register
    pub(crate) ascent: f32,
    pub(crate) descent: f32,                  // negative on the fixture
    pub(crate) leading: f32,
}

/// One registered face. Immutable after `register`.
#[derive(Debug)]
pub(crate) struct FontFace {
    source: FontSource,
    metrics: FaceMetrics,
    /// Indexed by glyph id. Already `hmtx / units_per_em`. Filled once at register.
    advance_norm: Box<[f32]>,
}

/// All faces for one render. `FontId(n)` is `faces[n]`.
#[derive(Debug, Default)]
pub(crate) struct FontStore {
    faces: Vec<FontFace>,
}

impl FontStore {
    pub(crate) fn new() -> Self { Self::default() }

    /// Parses once with skrifa and fills the advance table.
    ///
    /// Returns `Error::Font { index, message }` for empty bytes, bytes over `MAX_FONT_BYTES`,
    /// a `FontRef::from_index` failure, `units_per_em == 0`, or any glyph in
    /// `0..glyph_count` with no advance.
    pub(crate) fn register(&mut self, source: FontSource) -> Result<FontId> {
        // reject empty / oversize
        // let font = FontRef::from_index(&source.bytes, source.index).map_err(to_font_error)?;
        // let m = font.metrics(Size::unscaled(), LocationRef::default());
        // upem = f32::from(m.units_per_em); 0 -> Error::Font
        // let gm = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        // advance_norm = (0..gm.glyph_count()).map(|g| gm.advance_width(GlyphId::new(g)) / upem)
        //     None -> Error::Font { message: "no advance for glyph {g}" }
        // id = FontId(self.faces.len() as u32) via u32::try_from, overflow -> Error::Font
        unimplemented!()
    }

    pub(crate) fn face(&self, id: FontId) -> Result<&FontFace> {
        // self.faces.get(id.0 as usize).ok_or(Error::UnknownFont { id })
        unimplemented!()
    }

    pub(crate) fn metrics(&self, id: FontId) -> Result<&FaceMetrics> {
        Ok(&self.face(id)?.metrics)
    }
}

impl FontFace {
    pub(crate) fn bytes(&self) -> &Arc<Vec<u8>> { &self.source.bytes }
    pub(crate) fn index(&self) -> u32 { self.source.index }
    pub(crate) fn metrics(&self) -> &FaceMetrics { &self.metrics }

    /// Charmap lookup. `None` covers unmapped and `.notdef`. No glyph-0 branch.
    ///
    /// Re-parses the table directory each call; register already proved it parses.
    pub(crate) fn map_scalars(&self, text: &str) -> Result<Vec<(usize, char, Option<u32>)>> {
        // let font = FontRef::from_index(...).map_err(to_font_error)?;   // no unwrap
        // let cmap = font.charmap();
        // text.char_indices().map(|(at, ch)| (at, ch, cmap.map(ch).map(|g| g.to_u32()))).collect()
        unimplemented!()
    }

    /// `hmtx / units_per_em`. Out of range is `Error::Font`, not `MissingGlyph`.
    pub(crate) fn advance_norm(&self, glyph_id: u32) -> Result<f32> {
        unimplemented!()
    }
}
```

`map_scalars` takes the whole string so one `FontRef` parse serves a run. It returns the raw `Option` so the missing-glyph policy stays in `measure`.

### `src/text/measure.rs`

```rust
//! Nominal glyph placement and line boxes.
//!
//! Missing cmap entries fail the whole document. v1 has no `.notdef` and no fallback.

use crate::canvas::GlyphPlacement;
use crate::error::{Error, Result};
use crate::fonts::{FaceMetrics, FontId, FontStore};

/// One placement per Unicode scalar. Clusters wait for knife 5.
///
/// An unmapped scalar is `Error::MissingGlyph { font, scalar }`. An unknown id is `Error::UnknownFont`.
pub(crate) fn measure(store: &FontStore, font: FontId, text: &str) -> Result<Vec<GlyphPlacement>> {
    // let face = store.face(font)?;
    // for (at, ch, gid) in face.map_scalars(text)? {
    //     let glyph_id = gid.ok_or(Error::MissingGlyph { font, scalar: ch })?;
    //     push GlyphPlacement { glyph_id, advance_norm: face.advance_norm(glyph_id)?, byte_range: at..at + ch.len_utf8() }
    // }
    unimplemented!()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LineMetrics {
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) leading: f32,
    pub(crate) line_height: f32,
    pub(crate) baseline: f32,
}

/// The one line-box formula (design "字体与测量"). No second branch for negative descent.
pub(crate) fn line_metrics(face: &FaceMetrics, font_size: f32, factor: f32) -> LineMetrics {
    unimplemented!()
}
```

### `src/error.rs` additions

```rust
/// A font could not be read or is unusable. `message` is the parser's display text.
Font { index: u32, message: String },
/// `scalar` has no glyph in `font`. The document is not produced.
MissingGlyph { font: FontId, scalar: char },
/// `id` is not registered in the store this canvas or measurement uses.
UnknownFont { id: FontId },
/// skrifa and krilla read different units per em from the same bytes.
FontMetricsMismatch { font: FontId, skrifa_upem: f32, krilla_upem: f32 },
/// A glyph run whose byte ranges do not slice its text on char boundaries.
InvalidGlyphRun,
```

`Error` derives `Clone, PartialEq`. `FontId` is `Copy + Eq`, so it fits. No krilla or skrifa type appears in any variant.

### `src/backend/krilla_canvas.rs`

```rust
use std::sync::Arc;

use crate::fonts::FontStore;

pub(super) enum Mark {
    Fill { .. },
    Stroke { .. },
    /// Validated against the canvas's store and text at record time.
    Glyphs(GlyphRun),
}

pub(crate) struct KrillaCanvas {
    fonts: Arc<FontStore>,
    info: Option<DocumentInfo>,
    closed_pages: Vec<PagePlan>,
    open_page: Option<OpenPage>,
    failure: Option<Error>,
    serialization: Serialization,
}

impl KrillaCanvas {
    pub(crate) fn new(fonts: Arc<FontStore>) -> Self { .. }
    #[cfg(test)]
    pub(crate) fn new_observable_for_test(fonts: Arc<FontStore>) -> Self { .. }

    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        // same guards as today, then
        // emit::render(self.info.as_ref(), &self.closed_pages, &self.fonts, self.serialization)
        unimplemented!()
    }
}

impl Canvas for KrillaCanvas {
    fn draw_glyph_run(&mut self, run: &GlyphRun) -> Result<()> {
        // self.open_page_mut("draw_glyph_run")?;
        // if let Err(e) = self.fonts.face(run.font) { return self.fail(e); }        // UnknownFont
        // if run.glyphs.iter().any(|g| run.text.get(g.byte_range.clone()).is_none()) {
        //     return self.fail(Error::InvalidGlyphRun);                          // krilla slices text[range]
        // }
        // page.marks.push(Mark::Glyphs(run.clone()));
        unimplemented!()
    }
}
```

The canvas checks only what the store and the run can answer without krilla. It does not compare upem.

### `src/backend/emit.rs`

```rust
use std::collections::HashMap;

use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::Data;

use crate::fonts::{FontId, FontStore};

/// krilla fonts built on first draw within one `render`. Lives on the render stack.
struct KrillaFonts<'a> {
    store: &'a FontStore,
    built: HashMap<FontId, Font>,
}

impl<'a> KrillaFonts<'a> {
    fn new(store: &'a FontStore) -> Self { .. }

    /// First call per id: `Font::new(Data::from(Arc::clone(face.bytes())), face.index())`.
    /// `None` is `Error::Font { index, message: "krilla Font::new returned None" }`.
    /// Then `check_upem`. Later calls clone the cached `Font`.
    fn get(&mut self, id: FontId) -> Result<Font> {
        unimplemented!()
    }
}

/// `|skrifa - krilla| >= 0.01` is `Error::FontMetricsMismatch`.
fn check_upem(font: FontId, skrifa_upem: f32, krilla_upem: f32) -> Result<()> {
    unimplemented!()
}

pub(super) fn render(
    info: Option<&DocumentInfo>,
    pages: &[PagePlan],
    fonts: &FontStore,
    serialization: Serialization,
) -> Result<Vec<u8>> {
    // let mut krilla_fonts = KrillaFonts::new(fonts);
    // for page in pages { emit_page(&mut document, page, &mut krilla_fonts)?; }
    unimplemented!()
}

fn draw_mark(surface: &mut Surface<'_>, mark: &Mark, fonts: &mut KrillaFonts<'_>) -> Result<()> {
    // Mark::Glyphs(run) => {
    //     let font = fonts.get(run.font)?;
    //     surface.set_fill(Some(to_fill(Paint::solid(run.color))?));
    //     surface.set_stroke(None);
    //     let glyphs: Vec<KrillaGlyph> = run.glyphs.iter().map(|g| KrillaGlyph::new(
    //         GlyphId::new(g.glyph_id), g.advance_norm, 0.0, 0.0, 0.0, g.byte_range.clone(), None,
    //     )).collect();
    //     surface.draw_glyphs(to_krilla_point(run.origin), &glyphs, font, &run.text,
    //                         run.font_size.as_pt(), false);
    // }
    unimplemented!()
}
```

`g.advance_norm` goes to krilla untouched. No `* font_size`, no `/ upem` in the adapter.

## Tests

| Test | Where | Config | Asserts |
| --- | --- | --- | --- |
| fixture metrics | `fonts.rs` | both | `descent < 0`; literal upem; literal `advance_norm` for `A` and `第` |
| missing glyph | `measure.rs` | both | `measure(.., "A\u{4E00}")` is `MissingGlyph { scalar: '\u{4E00}' }` (one literal scalar the subset lacks) |
| bad bytes | `fonts.rs` | both | empty, `b"nope"`, and an oversize length check each give `Error::Font` |
| unknown id in measure | `measure.rs` | both | `FontId::from_raw(9)` gives `UnknownFont` |
| line metrics | `measure.rs` | both | literal `line_height` and `baseline` for the fixture at 12pt, factor 1.2 |
| recording x | `canvas.rs` or `measure.rs` | both | measured "AB", second x equals literal `72 + hmtx_A/upem * 10` |
| TJ delta | `emit.rs` | backend | observable canvas, first glyph `advance_norm + 0.25` at 10pt. Pin the observed TJ number (about `-250`, 1000-unit hmtx delta). Comment says it is not `advance_norm`. No `Tj` search |
| upem check | `emit.rs` | backend | `check_upem(id, 1000.0, 1000.0)` ok; `(1000.0, 2048.0)` is `FontMetricsMismatch` with those literals |
| unknown id on canvas | `emit.rs` | backend | `draw_glyph_run` with id 9 is `UnknownFont`; `finish` is the same error |
| bad byte range | `emit.rs` | backend | range `0..2` over `"第"` (3 bytes) is `InvalidGlyphRun`; `finish` fails without panic |
| glyph smoke | `emit.rs` | backend | measured "第1页" draws; bytes start with `%PDF` |

`unsupported_glyph_run_poisons_finish` is deleted. Existing backend tests change only `KrillaCanvas::new()` to `KrillaCanvas::new(Arc::default())`.

CI runs `cargo test` and `cargo test --no-default-features`. The second proves measurement builds without krilla.
