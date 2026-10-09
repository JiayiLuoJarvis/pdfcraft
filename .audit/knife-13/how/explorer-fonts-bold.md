# Knife 13 — fonts subsystem and `p.bold` options

Baseline: `feat/style-builders` @ `f208329fc172aa9cd5d81ebaa38e2f20a7746e9e`.
Crate name `quire-pdf`. Scope: font registration, measurement, glyph-run consumption of `Style.font`, fixture faces, krilla embedding boundary, and what that implies for `p.bold`. First-line indent is not a font feature; it is noted only where it meets this pipeline.

## Components Found

### `FontId` (`src/fonts.rs:13-31`, re-exported `src/lib.rs:30`)

- Newtype `FontId(pub(crate) u32)`. Field is crate-private. `from_raw` and `as_u32` are public `const`.
- `#[must_use]`. Comment: a store numbers faces from 0 in registration order; the first face is 0 (`src/fonts.rs:13-16`).
- Callers can forge any id with `FontId::from_raw`. Nothing on the public DSL accepts a `FontId`. Design draft said callers could not construct `FontId(1)` (`docs/quire-design.md:485`); `from_raw` makes that untrue, but there is still no public setter.
- `RecordingCanvas` stores the id as given and does not resolve a store. A unit test records `FontId::from_raw(1)` with invented glyph ids (`src/canvas.rs:424-429`, `347-350`).

### `FontSource` / `FontFace` / `FontStore` (`src/fonts.rs:33-149`)

- `FontSource { bytes: Arc<Vec<u8>>, index: u32 }` (`src/fonts.rs:35-39`). `index` is the collection face index for `FontRef::from_index`, not a weight.
- `FontFace` holds `source`, `FaceMetrics` (`units_per_em`, `ascent`, `descent`, `leading`), and `advances: HashMap<u32, f32>` keyed by glyph id (`src/fonts.rs:41-54`).
- `FontStore { faces: Vec<FontFace> }` (`src/fonts.rs:63-66`). Methods are `pub(crate)`.
- `MAX_FONT_BYTES` is a module const, 32 MiB (`src/fonts.rs:33`). `src/limits.rs` has no font-byte limit. Design draft put the cap on `Limits::max_font_bytes` (`docs/quire-design.md:489`); the code does not.
- `register` (`src/fonts.rs:76-123`):
  1. Empty bytes → `Error::Font { index, message: "font bytes are empty" }`.
  2. Longer than 32 MiB → `Error::Font` with the source index.
  3. `skrifa::FontRef::from_index(bytes, index)`; `ReadError` Display → `Error::Font`.
  4. `metrics(Size::unscaled(), LocationRef::default())`. `units_per_em == 0` → `Error::Font`.
  5. Id = `faces.len()` as `u32` before push. Overflow → `Error::Font { message: "too many registered fonts" }`.
  6. Push face with an empty advance map. Return that id.
- Failed `register` does not push. A later successful call still gets the next index.
- Lookup: `face` / `face_mut` / `metrics` / `resolve` use `id as usize` into `faces`. Out of range → `Error::UnknownFont { id }` (`src/fonts.rs:125-148`). Empty store + `FontId(0)` is `UnknownFont`, not `MissingGlyph` (`src/fonts.rs:217-221`).
- `resolve` returns `ResolvedFont { bytes, index, skrifa_units_per_em }` (`src/fonts.rs:57-61`, `141-148`). It does not return the advance cache. Embedding does not read cached advances.
- No dedup by bytes. Registering the same `Vec` twice yields two faces, two ids, two caches.
- No weight, style, family, or bold flag on the face. No variable-font location. Only `LocationRef::default()`.

### `Document::font` and the single default (`src/document.rs:198-241`, `573-582`)

- `Document` owns one `FontStore`, created empty (`src/document.rs:190`, `204`).
- `font(&mut self, bytes: Vec<u8>) -> Result<()>` (`src/document.rs:233-241`):
  - Rustdoc: "The first font is the one text uses."
  - Always `FontSource { bytes: Arc::new(bytes), index: 0 }`. Public API cannot select a TTC face.
  - `.map(drop)` discards the `FontId`. `#[must_use]` on `FontId` is why the method returns `()` (knife 8 status: `doc.font(bytes)?` would warn if the `Ok` were a `FontId`).
- `default_style()` hardcodes `font: FontId::from_raw(0)` plus 12 pt, line height 1.2, black, paragraph spacing 0, `TextAlign::Start` (`src/document.rs:573-581`). This `Style` is built in `Document::new` before any `font()` call (`src/document.rs:205`).
- Function docs repeat the same default (`src/document.rs:40-41`).
- `Flow.style` is set only at `Flow::new` (`src/document.rs:586-601`). No assignment updates it later. `child()` clones it (`src/document.rs:615-617`). Sections clone that same style (`src/document.rs:304-308`, `83-87`).
- A second `font()` call does register face 1. The DSL never reads that id.

### `Style` (`src/layout/style.rs:16-48`)

`pub(crate) struct Style` fields: `font: FontId`, `font_size`, `line_height_factor`, `color`, `paragraph_spacing`, `align`.

No `bold`, `italic`, decoration, opacity separate from `Color`, named-style name, or first-line indent.

`Style::new` only rejects a non-finite or non-positive line-height factor. It does not check that `font` is registered. Unknown ids fail later in `measure_text` / `metrics`.

### Measurement (`src/text/measure.rs`)

- `measure_text(fonts: &mut FontStore, font: FontId, text: &str) -> Result<MeasuredText>` (`src/text/measure.rs:41-98`).
- Looks up the face first, even for `""`, and re-parses `FontRef::from_index` on every call. The `FontRef` is not stored on the face.
- Per Unicode scalar in `text` (not per grapheme cluster inside this function):
  - `charmap.map(scalar)` `None` → `Error::MissingGlyph { font, scalar }` and return immediately (`src/text/measure.rs:63-66`). No `.notdef` glyph, no glyph 0 branch, no fallback face.
  - Advance: cache hit on `face.advances`, else `glyph_metrics.advance_width` at unscaled size and default location. `None` → `Error::Font { message: "no unscaled advance for glyph …" }`, which is not a missing glyph (`src/text/measure.rs:68-79`).
  - `advance_norm = advance_font_units / units_per_em`. Summed into `MeasuredText.advance_norm`.
- `line_metrics(face, font_size, factor)` (`src/text/measure.rs:100-121`): scale = `font_size / upem`; `line_height = max(ascent - descent + leading, size * factor)`; `baseline = ascent`. Descent sign is preserved (fixture descent stays negative). Factor must be finite and `> 0`.
- Pins on the fixture (`src/text/measure.rs:154-176`): `'A'` advance 707 font units, norm `0.707`; `'第'` 1000 font units, norm `1.0`; upem 1000. Unmapped `U+FFFF` is `MissingGlyph` (`src/text/measure.rs:141-150`).

### Who passes `Style.font` into measure / `GlyphRun`

One font per string. There is no per-span font.

| Site | What it does with `style.font` |
| --- | --- |
| `ParagraphNode::layout` | One `measure_text(fonts, style.font, clustered.glyph_text())`, then `fonts.metrics(style.font)` for the whole paragraph's line box (`src/layout/paragraph.rs:62-69`). Each line becomes one `GlyphRun` whose `font` is `style.font` (`src/layout/paragraph.rs:227-257`). |
| `single_line_run` | Same single-font measure, used by list markers and layer text (`src/layout/paragraph.rs:141-169`). |
| `fragment_from_lines` | One `FragmentCommand::Glyphs` per line (`src/layout/paragraph.rs:197-212`). |
| `Flow::text` | `TextNode::new(text, self.style.clone())` (`src/document.rs:646-648`). |
| `Flow::paragraph` | Concatenates every `Paragraph::text` into one `String`, then one `TextNode` (`src/document.rs:659-664`, `753-763`). |
| `Band::lower` | Clones style and overrides only `align` (`src/document.rs:1064-1070`). Header/footer text uses font 0. |
| `Layer::text_at` | Stores `self.style.clone()` (`src/document.rs:1138-1143`). `measure_layer` forces `TextAlign::Start` and keeps `item.style.font` (`src/layout/decorate.rs:196-201`). |
| `List::lower` | `marker_style: self.parent.style` (`src/document.rs:982-988`). Markers measure with the body font (`src/layout/list.rs:209`). |
| `Table::lower` | Cell strings go through `flow.text` (`src/document.rs:839`). |
| `draw_fragment` | Clones the run, translates origin, `canvas.draw_glyph_run` (`src/layout/flow.rs:356-360`). Does not remap the font. |

`ClusteredText::glyph_text` is the BK-stripped scalar string that `measure_text` sees (`src/text/cluster.rs:92-94`). Cluster widths are sums of that one `MeasuredText`'s `advance_norm * font_size` (`src/layout/paragraph.rs:216-224`). `break_lines` does not know about fonts.

`LayoutCtx` is only `&mut FontStore` (`src/layout/flow.rs:19-20`).

### Errors (`src/error.rs:47-72`, `140-155`)

- `Error::Font { index, message }`: parse, empty, oversize, upem 0, missing advance, krilla `Font::new` returned `None`.
- `Error::MissingGlyph { font: FontId, scalar: char }`: cmap miss. Display is `font {id} has no glyph for U+{scalar:04X}`.
- `Error::UnknownFont { id: u32 }`: id not in this store. Display `font {id} is not registered`.
- `Error::FontMetricsMismatch { skrifa_upem, krilla_upem }`: compared at draw time, threshold `0.01` (`src/backend/emit.rs:188-196`).

### Canvas glyph run (`src/canvas.rs:99-132`)

`GlyphPlacement`: `glyph_id`, `advance_norm` (font units / upem, not multiplied by size), `byte_range` into `GlyphRun::text`.

`GlyphRun`: one `font: FontId`, one `font_size`, one `color`, `origin` = baseline of the first glyph, `glyphs`, `text`. No second font, no synthetic offset, no embolden flag.

### Backend embedding (feature `backend-krilla`)

- `Cargo.toml:14-19`: `krilla =0.8.2`, `default-features = false`, feature `raster-images` only. `simple-text` and `rayon` are off. `skrifa =0.47.0` is a direct dependency (a second copy from krilla's own skrifa, per design).
- `layout/`, `text/`, `fonts/`, `document/` do not `use krilla`.
- `KrillaCanvas` holds `Arc<FontStore>` and does not import krilla (`src/backend/krilla_canvas.rs:1-4`, `97-98`).
- `draw_glyph_run` (`src/backend/krilla_canvas.rs:286-303`): `font_store.resolve(run.font)` or fail (poison). Pushes `Mark::Glyphs` with cloned bytes, `font_index`, skrifa upem, size, color, origin, placements, text (`src/backend/krilla_canvas.rs:74-83`).
- `emit::draw_mark` is the only `Font::new` / `Surface::draw_glyphs` site (`src/backend/emit.rs:131-170`):
  - `Font::new(Data::from(Arc::clone(font_bytes)), font_index)`. `None` → `Error::Font`.
  - `check_upem`.
  - Each placement → `KrillaGlyph::new(GlyphId, advance_norm, 0.0, 0.0, 0.0, byte_range, None)`. Offsets and y-advance are 0. Last `None` is no span decoration from this call.
  - `surface.draw_glyphs(origin, glyphs, font, text, font_size, false)`. The `false` is `outlined`.
- No `Font::new_variable`. No `Surface::draw_text` anywhere under `src/`.
- `Rendered::to_pdf` (`src/document.rs:1294-1298`) moves the post-layout `FontStore` into `KrillaCanvas::new_with_resources`. Replay then emit. Advance cache is irrelevant by then; glyph ids and `advance_norm` already sit on the recorded run.
- Unknown font at draw time: `emit.rs` test `unknown_font_poisons_finish` with an empty store and `FontId(0)` (`src/backend/emit.rs:355-373`).
- Same measured glyphs are what get drawn: `measured_glyphs_emit_pdf` (`src/backend/emit.rs:377-408`) asserts `%PDF` only.

### Fixture fonts (`tests/fixtures/`)

Directory listing at this commit:

| File | Role |
| --- | --- |
| `quire-measure.ttf` (14384 bytes) | The only font. |
| `LICENSE-OFL.txt` | OFL for that subset. |
| `one-pixel.png`, `bad-idat.png` | Images, not fonts. |
| `README.md` | Describes the face. |

`tests/fixtures/README.md`: OFL subset of Adobe Source Han Serif **ExtraLight**. Covers printable ASCII, digits, U+2022, and 第 / 页 / 共. Family name **QuireMeasure**. Rebuild via `.audit/fonts-skrifa/subset-fixture.sh` plus a local rename. Do not vendor full CJK.

`subset-fixture.sh` takes one source TTF, subsets `U+0020..=U+007E` plus `第页共` and U+2022, layout features off, writes one file. It does not emit a bold or regular companion. The script's default output name is `source-han-serif-quire.ttf`; the committed file is the renamed `quire-measure.ttf`.

Load paths:

- `fonts::fixture_bytes()` is `#[cfg(test)]` `include_bytes!` of that ttf (`src/fonts.rs:151-160`).
- Integration tests: `const FONT: &[u8] = include_bytes!("fixtures/quire-measure.ttf")` (`tests/document.rs:13`).

There is no bold, regular, or italic fixture. ExtraLight is the only outline. Re-registering those same bytes would be a second `FontId` with the same advances (A=707, 第=1000, upem=1000), not a heavier face.

Missing-glyph tests use U+6F22 `漢`, which the subset does not contain (`src/document.rs:1335-1348`).

## Flow

### Register

```text
Document::font(bytes)
  → FontStore::register(FontSource { Arc bytes, index: 0 })
      → reject empty / >32MiB
      → FontRef::from_index + unscaled default-location metrics
      → FontId = faces.len()   // first success is 0
      → push FontFace { empty advance map }
  → drop FontId
```

`default_style.font` is already `FontId(0)` and is copied onto every `TextNode`, band slot, layer `text_at`, and list marker. Registration order does not restamp existing or future nodes. Calling `font()` after `paragraph()` still paints with id 0, as long as the first successful register is face 0 by the time `paint` measures.

### Measure (paint time, before any `start_page`)

```text
document() → Document::paint
  → section.lower (style already on nodes)
  → LayoutCtx { fonts: &mut store }
  → paginate_sections
      → TextNode::split → ParagraphNode::layout
          → ClusteredText
          → measure_text(store, style.font, glyph_text)
              → face() or UnknownFont
              → charmap.map or MissingGlyph (whole call aborts)
              → advance cache or Error::Font
          → line_metrics(that same face)
          → one GlyphRun per line, run.font = style.font
      → measure_bands / measure_layer (headers, behind, front)
          → single_line_run → same measure_text
```

`paint_pages` runs only after pagination and band measurement succeed. Failures leave `RecordingCanvas` empty:

- Header missing glyph: `src/document.rs:1338-1349`, expected `MissingGlyph { font: FontId(0), scalar: U+6F22 }`.
- Front `text_at` missing glyph, including when the hook only draws on `is_last()`: `src/document.rs:1381-1395`. Measure still runs the hook on every page.
- Layer unit test: `src/layout/decorate.rs:651-670`, `rec.commands()` empty.

Proposal §4 and `document` rustdoc: a missing glyph anywhere, including headers and layers, fails the whole document (`src/document.rs:37-38`, `docs/提案书.md:588-594`).

### Draw

```text
RecordingCanvas::draw_glyph_run  → store GlyphRun (includes FontId)
to_pdf:
  KrillaCanvas::draw_glyph_run → resolve(FontId) → Mark::Glyphs { bytes, index }
  emit → Font::new(bytes, index)
       → upem check
       → draw_glyphs(..., outlined: false)
```

Unused registered faces are parsed at `register` and never passed to `Font::new` unless some run's `font` resolves to them. Design draft checked krilla upem at register time (`docs/quire-design.md:492`). The code checks at emit, once per glyph mark (`src/backend/emit.rs:141-146`).

## Files Read

- `src/fonts.rs` (full)
- `src/text/measure.rs` (full)
- `src/text/mod.rs`, `src/text/cluster.rs` (glyph_text / cluster tape)
- `src/layout/style.rs` (full)
- `src/layout/paragraph.rs` (layout, `single_line_run`, `glyph_run_for_line`)
- `src/layout/flow.rs` (`LayoutCtx`, `draw_fragment`)
- `src/layout/decorate.rs` (`measure_layer`, missing-glyph test)
- `src/layout/list.rs` (marker uses `marker_style`)
- `src/layout/test_util.rs` (`fonts_and_style` registers one fixture face)
- `src/document.rs` (`font`, `default_style`, `Paragraph`, `Band`, `Layer`, `paint`, `to_pdf`, missing-glyph tests)
- `src/error.rs` (font variants)
- `src/canvas.rs` (`GlyphRun`, recording)
- `src/backend/krilla_canvas.rs` (`Mark::Glyphs`, `draw_glyph_run`)
- `src/backend/emit.rs` (`Font::new`, `draw_glyphs`, unknown-font and `%PDF` tests)
- `src/lib.rs`, `Cargo.toml`
- `tests/fixtures/README.md`, `tests/document.rs` fixture constants
- `.audit/fonts-skrifa/subset-fixture.sh`
- `docs/提案书.md` §1 example (`p.bold` at lines 85-88), §2 style inventory (154-155, 204), §2.2 blank-vs-bold (243), §3 pipeline (580-588), §4 (590-594), confirmation table (632-647)
- `docs/quire-design.md` font/measure (446-535), `Style` / `StyleOverride` (873-925), `Span` (982-991), `draw_text` rejection (18, 99, 1380-1381)
- `AGENTS.md` glyph pipeline and `Surface::draw_text` ban
- `.audit/knife-13/arena/frame.md` autonomy line: bold face = second registered font, no synthetic bold

## Boundaries

### What the store already allows

- Any number of faces, ids 0, 1, 2, … in register order, until `u32` overflows.
- Each face has its own bytes, collection `index`, metrics, and advance cache.
- `measure_text` and `GlyphRun.font` already take an arbitrary `FontId`.
- Backend embeds whichever face a run resolves, via that face's bytes and `index`.
- A second `Document::font` call already parses a second face. The id is thrown away and no node points at it.

### What the public DSL does not allow

- `Document::font` always uses collection index 0 and returns `()`.
- Every text path clones `default_style`, whose font is the constant 0.
- `Paragraph` is one `String`. `p.text` only `push_str`s (`src/document.rs:761-763`). There is no `p.bold`.
- `TextNode` is one string plus one `Style`. A line is one `GlyphRun`. A mixed-weight line cannot be represented.
- `Style` has no weight bit. Bold cannot be "same face, flag set" without a new field that nothing reads.

### Synthetic bold does not exist

Searched `src/` for `draw_text`, `new_variable`, embolden/faux/synthetic: no matches except `LocationRef::default()` in `fonts.rs` and `measure.rs`.

- Draw path hardcodes glyph offsets to 0 and `outlined: false` (`src/backend/emit.rs:150-169`). Outlines are not stroked to fake weight.
- `simple-text` is off, so krilla's `draw_text` (shaping via rustybuzz, not a bold synthesizer) is not enabled. AGENTS.md and 提案书 §3 forbid calling it anyway. Design: widths would diverge because this pipeline has no kerning or ligatures (`docs/quire-design.md:18`, `1380`).
- Variable-font bold instance is closed: skrifa always `LocationRef::default()`, krilla only `Font::new` (default instance). Design explicitly skips `Font::new_variable` (`docs/quire-design.md:54`, `493`).
- Knife 13 frame: "No synthetic bold" (`.audit/knife-13/arena/frame.md:16`). Autonomy default is a second registered face (`FontId(1)`), with reverse option `bold-is-same-face` (`frame.md:33`).

### Design draft vs this tree (unconfirmed design is not authority)

| Topic | Design draft | This code |
| --- | --- | --- |
| Id allocation | `FontTable::insert` assigns from 1; 0 unused (`docs/quire-design.md:1067-1068`) | Store assigns from 0. First face is 0. Knife frame's "second font is `FontId(1)`" matches this code, not the draft's 1-based table. |
| When bytes are parsed | `insert` only allocates; `FontRef` at `write_to` | `Document::font` parses immediately. |
| krilla `Font::new` | At register, plus upem compare | At emit, per `Mark::Glyphs`. |
| Bold | No `bold` field. `Style.font: FontId`. `StyleOverride.font: Option<FontId>` (`docs/quire-design.md:876-886`). `Span` is `Text` or `Link` only (`docs/quire-design.md:988-991`) | No `StyleOverride`, no named map, no spans. |
| `measure_text` signature in the draft | Method on `FontStore` taking `font_size` and returning clusters | Free function; size applied later; one flat glyph vec. |

### Proposal text about bold

- §1 template calls `p.text("合同编号 "); p.bold(&contract.id);` inside one paragraph (`docs/提案书.md:85-88`). That is inline, not a second paragraph.
- §2 attribute inventory lists `bold` and `italics` next to `font` as pdfmake-style properties (`docs/提案书.md:154`). The section header still says the inventory was not yet selected; the later confirmation table does not add a sentence that defines how bold picks a face (`docs/提案书.md:632-647`).
- §2.1 first-line indent is `W' = W − indent` on the first line only (`docs/提案书.md:204`). It does not mention fonts. `Style` has no indent field; `paragraph.rs` has no indent.
- §2.2 blank underscore is "inline, beside `text` / `bold`" (`docs/提案书.md:243`). Confirms `bold` is an inline run in the proposal's language, not a document-level setting.
- §3: one pipeline, no kerning/ligatures, no `draw_text` (`docs/提案书.md:586-588`).
- §4: missing glyph fails the whole document; no `.notdef`, no fallback chain (`docs/提案书.md:590-594`). A bold face that lacks a scalar the regular face has still fails the document. The error carries the `FontId` that was queried, so a miss on face 1 is distinguishable from a miss on face 0.

### Options that fit the current store

1. **Second registered face (store-ready, DSL-not-ready).** Call `register` again (new method or stop dropping the id). Stamp `FontId(1)` on bold spans. `measure_text` and `draw_glyphs` already follow `GlyphRun.font`. Recording tests can assert `run.font.as_u32() == 1` without a heavier outline by registering the ExtraLight bytes twice; the PDF will not look bold. A real weight change needs a second file that is not in `tests/fixtures/`. Line breaking must sum advances from the face that owns each span. Today one `MeasuredText` covers the paragraph. A line that mixes faces needs more than one `GlyphRun` (current `fragment_from_lines` emits one run per line). Line-box metrics are taken from the single `style.font`; two faces with different ascents have no rule in code or in the confirmed proposal.

2. **Same face (`bold-is-same-face`).** `p.bold` is only a span boundary that still measures and draws `FontId(0)`. No second file, no visual weight. `GlyphRun.font` cannot show the distinction. Frame lists this as the reverse of the default.

3. **Whole-paragraph font switch.** Possible only by constructing a `TextNode` with a different `Style.font` inside the crate (tests already do this via `Style::new`). It does not implement §1 `p.bold`, which mixes runs in one paragraph. `Flow.style` is not publicly mutable, so the DSL cannot do even the block-level switch today.

4. **TTC index.** `FontSource.index` is the hook. `Document::font` forces 0. A bold face inside a collection is unreachable without a new argument. No test registers `index != 0`.

5. **Synthetic bold / wght axis / `draw_text`.** Not implemented, and the frame, AGENTS.md, §3, and the draw path reject them. Do not treat `outlined: true` or a stroke as an existing feature; `outlined` is the literal `false`.

6. **Named styles + `StyleOverride`.** Design-only. Frame says leave them out unless `p.bold` cannot be expressed without them. A span-local `FontId` is enough for the store; the missing piece is the paragraph run model, not a style map.

## Non-Obvious Things

- The default font is not "whatever `register` returned first" stored on the document. It is the constant `FontId(0)` copied at `Document::new`. That coincides with the first successful `register` only because ids are `faces.len()` before push.
- Extra `font()` calls are not ignored by the store. They parse, count toward the `u32` cap, and sit unused. They are not embedded until a run references them.
- `Document::font` after body text still works: nodes do not capture bytes, only id 0, and measurement is in `paint`.
- Zero fonts plus any text: `UnknownFont { id: 0 }` at measure time, before pages. Empty string still calls `face()` first (`src/text/measure.rs:46-53`), so `""` also requires face 0.
- `MissingGlyph` vs `UnknownFont` vs `Error::Font`: unmapped scalar; id not in the vec; parse / empty / oversize / upem 0 / `advance_width` `None` / krilla `Font::new` `None`.
- Cmap `None` is the whole-document failure. The implementation does not draw glyph 0. Design claims this skrifa pin returns `None` for both unmapped and `.notdef` (`docs/quire-design.md:115`, `521`). The code does not have a separate glyph-0 check.
- Advance cache is per face and per glyph id, filled lazily, and is not what PDF uses. PDF uses the `advance_norm` copied onto `GlyphPlacement` at layout. Two faces do not share cache entries even if glyph ids collide.
- `measure_text` re-parses the font on every call. Registration already parsed it once for metrics only.
- Upem cross-check is not at `Document::font`. A face skrifa accepts and krilla rejects fails at `to_pdf`, after layout has already produced a recording. `document()` itself uses `RecordingCanvas` and returns `Rendered` without calling krilla (`src/document.rs:42-55`).
- List markers, table cell text, header/footer slots, and `text_at` all inherit font 0. A bold API that only changes `Paragraph` leaves those on the regular face unless they grow the same span type.
- Fixture family is ExtraLight, not Regular. Using it as "the regular face" and expecting a bold sibling in-repo will fail. Coverage is tiny: page-number characters and ASCII, not a general CJK bold pair. `漢` (U+6F22) is the intentional miss.
- `FontId` `#[must_use]` plus `map(drop)` is why the public method hides ids. Returning `FontId` from `font()` is a one-line API change; teaching paragraphs to carry more than one id is not.
- Design draft 1-based `FontTable` must not be copied on top of this store. Existing tests and `default_style` assume face 0.
- First-line indent does not change cmap, advance cache, or `GlyphRun.font`. It would change the width passed into `break_lines` for line 0 and the x of that line's run (`align_x` is the only x adjuster today, `src/layout/paragraph.rs:260-265`).

## Open Questions

- Proposal never says bold is "the second `font()` call" versus a same-face span versus a named style. The §1 example only shows the call shape. Frame autonomy picks the second face and forbids synthetic bold; that is arena guidance, not 提案书 text.
- No rule for line height when the bold face's ascent/descent differ from face 0. Current code has one `line_metrics` per paragraph.
- No rule for a bold span that crosses a line break: how many `GlyphRun`s per line, and whether `GlyphRun::text` byte ranges are per span or per line. Today's model is one run, one font, one string per line.
- Whether `p.bold` without a prior second `font()` is `UnknownFont { id: 1 }`, a builder error at `document()` time, or silently uses face 0. Nothing in the code chooses.
- Whether a missing glyph in the bold face fails the document (consistent with §4, and with `MissingGlyph.font` already being able to name face 1) or falls back to face 0 (forbidden by §4's no-fallback sentence if read strictly).
- Public collection index: needed only if bold lives in a TTC. Not required if bold is a second file with index 0.
- Fixture strategy is unresolved in-tree: second id over `quire-measure.ttf` proves id plumbing and not weight; a real bold subset is not built by `subset-fixture.sh`.
- `Document::font` returning `FontId` interacts with `#[must_use]` and with "first font is implicit id 0". Keeping `()` and adding `bold_font(bytes)` matches the frame's wording without making every existing `doc.font(bytes)?` handle an id.
- Design's register-time krilla parse is not how embedding works. A bold face that skrifa accepts and krilla rejects still lays out, then fails in `to_pdf`.
