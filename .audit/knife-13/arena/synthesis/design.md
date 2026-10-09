# Knife 13 synthesis: bold runs + first-line indent

**Base:** Candidate 3 (cross-judge 28/30; parent agrees).  
**Mandatory graft from C2:** tagged clustering / batch measure by contiguous font role (fix C3's per-cluster `measure_text` and "whole grapheme follows first scalar").  
**Grafts from C1:** coalesce adjacent same-face spans; after resolve, layout consumes `FontId` only.  
**Graft from C4:** name `LineBudget` fields clearly as first/rest (no trait).  
**Rejected:** C4 hierarchical `doc.style` / StylePatch; C2 public `LineHeight` + full paragraph style setter pack this knife; C1 Document-held FontRoles read from Flow::paragraph; returning public `FontId` from `font`.

Autonomy defaults: indent default 0 (`default-two-em` reverses); bold = `bold_font` slot (`bold-is-same-face` reverses); named styles out (`named-styles-in` reverses); line metrics from body/regular face (`max-run-metrics` reverses).

## Synthesis decision

Ship C3's caller surface and FontStore bold slot, with C2's measurement tape.

### Public API

```rust
impl Document<'_> {
    pub fn font(&mut self, bytes: Vec<u8>) -> Result<()>; // unchanged
    pub fn bold_font(&mut self, bytes: Vec<u8>) -> Result<()>;
}

impl Paragraph {
    pub fn text(&mut self, s: impl AsRef<str>);
    pub fn bold(&mut self, s: impl AsRef<str>);
    pub fn first_line_indent(&mut self, indent: Length<NonNegative>);
}
```

- `bold_font` requires a body face already registered; else `Error::Font { index: 0, message: "body font is not registered" }`.
- Re-`bold_font` rebinds the slot to the newly registered id (orphans prior face in store).
- Empty `text`/`bold` push nothing; adjacent same `Face` coalesce.
- Indent default `NonNegative::ZERO`. Later `first_line_indent` overwrites.

### Data shape

| Layer | Type | Notes |
| --- | --- | --- |
| DSL | `Paragraph { runs: Vec<TextRun>, indent }` | `TextRun { text, face: Body \| Bold }` |
| Lower | one `TextNode { runs, style }` | style clone + `first_line_indent`; do not mutate `Flow.style` |
| Fonts | `FontStore.bold: Option<FontId>` | `set_bold` / `bold()` → empty = `UnknownFont { id: 1 }` |
| Style | `first_line_indent: Length<NonNegative>` | default 0 |
| Break | `LineBudget { first, rest }` | `uniform(w)` for existing call sites |
| Line | `LineBox { runs: Vec<GlyphRun> }` | same baseline; x baked before paginate |

### Layout algorithm (C2 tape graft)

1. Concatenate run texts → one `ClusteredText::new` (single CRLF / BK pass).
2. Build a parallel per-scalar (glyph-text) role tape aligned with that normalization. Each scalar keeps the face of the run that contributed it. Combining marks keep their own run's face when the mark is in a bold run.
3. Resolve `Face::Bold` → `fonts.bold()?` once per layout; `Face::Body` → `style.font`.
4. Batch-measure contiguous same-`FontId` slices via existing `measure_text` (not once per cluster). Flatten to a measured glyph tape with advances in pt at `style.font_size`.
5. `break_lines(clustered, LineBudget { first: W-I, rest: W }, width_of)` where `width_of` sums the measured tape for the cluster's glyph range. Reject `I >= W` / non-positive `W'` as `InvalidGeometry` before break.
6. First visual line: `x = I + align_x(W-I, ink, align)`. Later lines: `align_x(W, ink, align)`. Split each line into homogeneous `GlyphRun`s; subsequent run `origin.x` accumulates advance.
7. Line height / baseline from `style.font` metrics only. `ParagraphNode.width` stores full `W`. Paginate only slices line boxes (no re-indent on continuation pages).

### Out of knife

Named styles, italics, decoration, §2.2 blank, flow canvas, justify, Cell/Band/Layer rich text, Flow-level `.style(|s|)`, synthetic bold, public `Style`.

### Tests

- §1 mixed `text`+`bold`: RecordingCanvas records `FontId(0)` then `FontId(1)` on one line when both faces registered (same fixture bytes OK for id/coords).
- First-line indent Start: first glyphs at `x ≈ I`; wrap uses `W'`; second line at `x ≈ 0`.
- Center/End first line uses box `[I, W]`.
- `p.bold` without `bold_font` → `UnknownFont { id: 1 }` before any page.
- `indent >= W` → `InvalidGeometry` before any page.
- Unbreakable wider than `W'` → `UnbreakableWiderThanFrame` with `avail == W'`.
- backend-krilla: mixed bold+indent doc bytes start with `%PDF`.
- Prefer a bold fixture with different advance if cheap; not a gate if unavailable.

## Usage (caller)

```rust
use quire_pdf::{document, NonNegative, PageSize};

let rendered = document(PageSize::A4, |doc| {
    doc.font(regular)?;
    doc.bold_font(bold)?;
    doc.paragraph(|p| {
        p.first_line_indent(NonNegative::pt(24.0)?);
        p.text("合同编号 ");
        p.bold(&contract.id);
    });
    Ok(())
})?;
```

## Next implementation step

Wire `LineBudget` + `Style.first_line_indent` + `FontStore` bold slot + `Paragraph` runs; then tagged measure tape and multi-run `LineBox`; parent re-runs seven gates.
