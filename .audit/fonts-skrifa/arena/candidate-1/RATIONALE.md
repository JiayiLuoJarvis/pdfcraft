# Candidate 1 rationale. One skrifa store, shared by `Arc`, krilla fonts built in emit

## Problem

Knife 4 must measure text with skrifa and draw the same glyph ids through krilla `draw_glyphs`. Both halves must read one copy of the font bytes. Four constraints make the shape non-obvious. First, `fonts.rs`, `text/`, and `krilla_canvas.rs` may not import krilla, yet a krilla `Font` is needed to draw. Second, `KrillaCanvas` is a buffered plan, and only `emit.rs` builds `Document`, `Page`, and `Surface`. Third, measurement must build with `--no-default-features`. Fourth, header and footer text gets measured while pages are being drawn (提案书 §1, page closures run at each page draw), so the store must be readable by measurement and by the canvas at the same time. The design draft's `measure_text(&mut self)` with a lazy advance cache does not fit the fourth constraint once the canvas holds the store.

## Usage (caller's view)

The only callers this knife are crate-internal: tests now, `to_pdf` later. The public surface does not change. `FontId` stays the only font type exported.

```rust
let mut store = FontStore::new();
let body = store.register(FontSource { bytes: Arc::new(bytes), index: 0 })?;
let store = Arc::new(store);

let glyphs = measure(&store, body, "第1页")?;          // Vec<GlyphPlacement>
let width = glyphs.iter().map(|g| g.advance_norm).sum::<f32>() * size;

let mut canvas = KrillaCanvas::new(Arc::clone(&store));
canvas.start_page(A4)?;
canvas.draw_glyph_run(&GlyphRun { font: body, glyphs, text: "第1页".into(), .. })?;
canvas.finish_page()?;
let pdf = canvas.finish()?;
```

`measure` returns `GlyphPlacement`, the same type `GlyphRun` carries. The measure output becomes the draw input with no conversion step. The error a caller sees tells them which stage failed. `register` gives `Font`, `measure` gives `MissingGlyph`, `draw_glyph_run` gives `UnknownFont` or `InvalidGlyphRun`, and `finish` gives `FontMetricsMismatch`.

## Shape

`FontStore` is a `Vec<FontFace>` indexed by `FontId`. Each face holds its `FontSource` (`Arc<Vec<u8>>` plus index), `FaceMetrics`, and a dense `advance_norm: Box<[f32]>` table filled at `register`. After registration the store is frozen inside an `Arc`, and every method takes `&self`. `KrillaCanvas` holds that `Arc`. `Mark::Glyphs(GlyphRun)` stores the run as given. `emit::render` gets `&FontStore` and builds a stack-local `KrillaFonts` map that creates each `krilla::text::Font` on first draw from `Data::from(Arc::clone(bytes))` and runs `check_upem` right then.

Load-bearing decisions follow.

- **The advance quotient is computed once, at register.** The table stores `hmtx / upem`, so the division exists in one line of `fonts.rs`. `measure` copies values. The adapter passes them to `KrillaGlyph::new` as is. No layer can rescale, because no layer other than register ever sees font units (per type-system-discipline, construct the value in its final unit instead of checking it later).
- **The cache is eager and immutable, not lazy.** A dense table makes the store `Sync` with no `RefCell` or `Mutex`, so `Arc<FontStore>` is honest and measurement can run while the canvas holds the store. It also moves "glyph has no advance" to the register boundary, so `advance_norm` lookups after that can fail only on an out-of-range id (per boundary-discipline, validate when bytes enter, trust them after).
- **Validation lives at three boundaries, each with only the facts it has.** `register` validates bytes. `draw_glyph_run` validates the run against the store (`UnknownFont`) and against its own text (`InvalidGlyphRun`). I read the krilla 0.8.2 source and found it slices `text[glyph.text_range()]` in `content.rs:557` and `text/group.rs:115`, so a bad range panics if it reaches emit. `GlyphRun` has public fields, so this is a real boundary. Emit validates only what needs krilla, which is `Font::new` returning `None` and the upem comparison (per boundary-discipline).
- **Missing-glyph policy sits in `measure`, not `fonts`.** `FontFace::map_scalars` returns the raw charmap `Option`. `measure` turns `None` into `MissingGlyph`. `fonts.rs` stays a parser with no §4 policy in it.
- **`Mark::Glyphs` holds the whole `GlyphRun`.** The run already has every field emit needs. A parallel struct would duplicate a shape `canvas.rs` owns (per laziness-protocol, no duplicated shapes).

Interface depth: three crate-internal entry points (`register`, `measure`, `KrillaCanvas::new(Arc<FontStore>)`) hide skrifa parsing, the advance table, charmap policy, krilla font construction, its cache, and the upem cross-check. The public surface grows by zero types. `FontSource` stays `pub(crate)` until the DSL `FontTable` lands in knife 8.

Out of scope on purpose: grapheme clusters, break classes, `LimitKind` (32 MiB stays a `fonts.rs` constant mapped to `Error::Font`), variable fonts (`LocationRef::default()` on both sides), and images.

## Synthesis decision

Filled by arena later.

## Tradeoffs accepted

- We accept that `FontMetricsMismatch` surfaces at `finish` rather than at `register`, in exchange for keeping every krilla call in `emit.rs` and `fonts.rs` free of any `cfg` hook. A font that is registered but never drawn is never cross-checked. That font never reaches the PDF, so the mismatch cannot cause harm.
- We accept that the upem test calls the pure `check_upem` with literal numbers, not a real mismatched font, in exchange for a deterministic test. No real font makes skrifa and krilla disagree, so a fixture-driven test is impossible.
- We accept an O(glyph_count) loop at register (about 65k entries and 256 KiB for a full Source Han face, inferred from its glyph count), in exchange for an immutable `Sync` store. The fixture is tiny. This can look like premature optimization. It is not one. It exists so the store has no interior mutability.
- We accept a stricter register. A face where any glyph lacks an advance is rejected, even if the document never uses that glyph. In exchange, measure has no "advance missing" path for valid glyphs.
- We accept re-parsing the `FontRef` table directory once per `measure` call, in exchange for not holding a self-referential `FontRef` beside its bytes. Register already proved the parse succeeds. The re-parse still returns `Error::Font` rather than `unwrap`.
- We accept that `KrillaFonts` lives for one `render` call and rebuilds fonts if `finish` ran twice. `finish(self)` consumes the canvas, so it cannot run twice.
- We accept one new error variant beyond the four in how.md (`InvalidGlyphRun`), in exchange for no panic path from public `GlyphRun` fields into krilla.

## Alternatives considered

- **Lazy advance cache in a `RefCell` inside the shared store.** It matches the design draft's `&mut self` intent. Behind `Arc` it is `!Sync`. I recall that clippy's `arc_with_non_send_sync` lint warns on it under `-D warnings`, but I did not run clippy to check (guess). Switching to `Rc` or a `Mutex` to hide the mutation leaks a concurrency choice into every caller. It lost because it adds hidden state to buy a speedup that a dense table gets with none.
- **No advance cache, query `hmtx` per measure call.** This is the smallest diff. It lost because "no advance for glyph" would then be a per-call error path in `measure`, and the quotient would be recomputed at every call site rather than owned by register.
- **Build the krilla `Font` at `register` behind `#[cfg(feature = "backend-krilla")]` inside `fonts.rs`.** This catches mismatch earliest, and it is the design draft's step 4. It lost because it puts krilla into `fonts.rs`, which AGENTS.md forbids, and it adds a feature split to the measure layer.
- **Build a krilla `Font` at every glyph draw with no per-render map.** Reading krilla source shows `Font` hashes by `FontInfo` (checksum and metrics), so the PDF would not carry duplicate font objects. Each `Font::new` still runs a full `FontInfo::new` parse, though, and the upem check would run per run instead of per font. The per-render map costs one `HashMap` and removes both costs.

## Open questions and risks

- Is `InvalidGlyphRun` acceptable as a new public error variant this knife? Or should a bad byte range map to an existing variant such as `InvalidGeometry`, which reads wrong but adds nothing to the error surface?
- Should `measure` live as a free function in `text/measure.rs` (sketched), or as `FontStore::measure` like the design draft? The free function keeps the §4 policy out of `fonts.rs`. The method saves one import.
- `FontId::from_raw` is still public, so a caller can forge an id that names a font in the wrong store. Today that gives `UnknownFont` only when the index is out of range. Two stores with the same length would accept each other's ids. Is that acceptable until knife 8 brands ids per `FontTable`?
- Risk. The TJ number is a prediction (about `-250` for a 0.25 em surplus at 10pt) and still needs a real run. The test must pin what the uncompressed stream actually shows.
- Risk. The register-time loop assumes skrifa's `advance_width` returns `Some` for every id below `glyph_count` on well-formed fonts. If a real CJK font breaks this, register rejects a usable font. That would push us back to a sparse table.

## Next implementation step

Add `skrifa = "=0.47.0"` and the fixture subset, then write `FontStore::register` with its fixture-metrics and bad-bytes tests under `--no-default-features`, before touching the backend.
