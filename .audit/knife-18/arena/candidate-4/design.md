# Knife 18 — Optional Margin Presets (Candidate 4 Design)

**Authority.** `docs/提案书.md` §7 (纸张与默认边距：默认页边距 = 0，预置枚举可选) & §2.10 (header / footer 带高 = 上下 margin).  
**Baseline.** `feat/margin-presets` from `feat/cell-rich` tip `9b939b8`.  
**Grounding.** `.audit/knife-18/frame.md`, `.audit/knife-18/how.md`, `.audit/knife-18/data-shape.md`.  
**Autonomy.** Strictly follows all autonomy defaults; zero reverse words adopted (`assoc-consts`, `mm-round`, `Normal`/`Narrow`, `add-quarter`, `default-inch`, `doc.margin_preset(...)`).

---

## 1. Caller Usage Sketch

Presets are pure construction-time sugar that resolve immediately into standard `Margins` values with four non-negative lengths. They operate through the existing `Document::margins` and `Section::margins` methods without introducing parallel API entry points.

### 1.1 Implicit Single-Section Document

A newly initialized document starts with four zero margins (`Margins::default()`). A caller can opt into a preset using `.into()` or explicit `Margins::from()`:

```rust
use quire::{document, MarginPreset, Margins, PageSize};

// Using .into() conversion:
document(PageSize::a4(), |doc| {
    doc.margins(MarginPreset::Inch.into());
    doc.paragraph(|p| p.text("Body text inset by 72 pt on all sides."));
    Ok(())
})?;

// Using explicit Margins::from:
document(PageSize::a4(), |doc| {
    doc.margins(Margins::from(MarginPreset::HalfInch));
    doc.paragraph(|p| p.text("Body text inset by 36 pt on all sides."));
    Ok(())
})?;

// Using direct into_margins() const method:
document(PageSize::a4(), |doc| {
    doc.margins(MarginPreset::Inch.into_margins());
    doc.paragraph(|p| p.text("Const-resolved preset margins."));
    Ok(())
})?;
```

### 1.2 Explicit Sections and Isolation

Each explicit section begins with `Margins::default()` (four zeros). Sections do not inherit margins from earlier sections or the implicit document lead. Presets and custom margins compose orthogonally across sections:

```rust
use quire::{document, MarginPreset, Margins, NonNegative, PageSize};

document(PageSize::a4(), |doc| {
    // Section 1: 1-inch preset (72 pt uniform)
    doc.section(|s| {
        s.margins(MarginPreset::Inch.into());
        s.paragraph(|p| p.text("Section 1: 1-inch uniform margins."));
        Ok(())
    })?;

    // Section 2: Half-inch preset (36 pt uniform)
    doc.section(|s| {
        s.margins(MarginPreset::HalfInch.into());
        s.paragraph(|p| p.text("Section 2: 0.5-inch uniform margins."));
        Ok(())
    })?;

    // Section 3: Custom asymmetric margins (primary API remains unchanged)
    doc.section(|s| {
        s.margins(Margins::new(
            NonNegative::pt(54.0)?,
            NonNegative::pt(36.0)?,
            NonNegative::pt(54.0)?,
            NonNegative::pt(36.0)?,
        ));
        s.paragraph(|p| p.text("Section 3: Custom asymmetric margins."));
        Ok(())
    })?;

    // Section 4: Left unset -> remains four zeros (full bleed)
    doc.section(|s| {
        s.paragraph(|p| p.text("Section 4: Zero margins default."));
        Ok(())
    })?;

    Ok(())
})?;
```

### 1.3 Interaction with Headers, Footers, and Layers

Top and bottom margins define fixed header and footer band heights (§2.10). Choosing a preset sets all four sides equally:

```rust
use quire::{document, MarginPreset, PageSize};

document(PageSize::a4(), |doc| {
    // Top = 72 pt, Bottom = 72 pt, Left = 72 pt, Right = 72 pt
    doc.margins(MarginPreset::Inch.into());

    // Header band capacity is exactly 72 pt (y = 0..72 pt)
    doc.header(|band| {
        band.left(|p| p.text("Document Title"));
    });

    // Footer band capacity is exactly 72 pt (y = page_height - 72 pt .. page_height)
    doc.footer(|band| {
        band.right(|p| p.text("Confidential"));
    });

    // Body flow frame: x = 72..page_width - 72, y = 72..page_height - 72
    doc.paragraph(|p| p.text("Main document flow."));

    // Page-coordinate escapes: behind/front closures receive effective Margins
    doc.behind(|page| {
        // page.margins().top().as_pt() == 72.0
        // Drawing coordinates remain un-inset relative to physical page origin (0, 0)
    });

    Ok(())
})?;
```

### 1.4 Geometry Error Handling on Small Custom Pages

Setting a preset that exceeds the dimensions of a small custom page is rejected during layout validation via `Error::InvalidGeometry`:

```rust
use quire::{document, MarginPreset, PageSize, NonNegative, Error};

// Custom tiny page: 100 pt x 100 pt
let tiny_page = PageSize::custom(
    NonNegative::pt(100.0)?,
    NonNegative::pt(100.0)?,
);

let result = document(tiny_page, |doc| {
    // 72 pt left + 72 pt right = 144 pt > 100 pt width!
    doc.margins(MarginPreset::Inch.into());
    doc.paragraph(|p| p.text("Cannot fit"));
    Ok(())
});

// Fails with Error::InvalidGeometry { width, height }
assert!(matches!(result, Err(Error::InvalidGeometry { .. })));
```

---

## 2. Types, Signatures & Resolution Pipeline

### 2.1 Public Enum Definition in `src/geom.rs`

```rust
/// Common uniform page margin presets (§7 optional presets).
///
/// Each variant defines four identical non-negative side margins in PDF points.
///
/// Under standard PDF coordinate conventions, 1 point is exactly 1/72 of an
/// international inch (25.4 mm). Presets convert immediately into [`Margins`]
/// via [`From`] or [`MarginPreset::into_margins`].
///
/// Note: Default document and section margins remain four zeros ([`Margins::default`]).
/// `MarginPreset` intentionally does not implement [`Default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[must_use]
pub enum MarginPreset {
    /// Uniform 72 pt on all four sides (exactly 1 international inch, ≈25.4 mm).
    ///
    /// Corresponds to the §7 proposal example "四边 72pt / 约一寸".
    /// Sets top, right, bottom, and left to 72.0 pt. Header and footer
    /// band heights are also set to 72.0 pt.
    Inch,

    /// Uniform 36 pt on all four sides (exactly 0.5 international inch, ≈12.7 mm).
    ///
    /// Corresponds to the §7 proposal example "四边 36pt".
    /// Sets top, right, bottom, and left to 36.0 pt. Header and footer
    /// band heights are also set to 36.0 pt.
    HalfInch,
}
```

Key attributes:
- `#[non_exhaustive]`: Guarantees future preset additions (e.g., if a third standard preset is confirmed later) will not break downstream match patterns.
- `#[must_use]`: Prevents accidental unconsumed preset expressions.
- No `Default` implementation: Precludes ambiguity regarding whether `Inch` or zero is the default margin.

### 2.2 Compile-Time Trusted Length Constants

Inside `src/geom.rs`, define private trusted constants reusing `Length::from_trusted_pt`:

```rust
// In src/geom.rs
const PRESET_INCH_PT: Length<NonNegative> = Length::from_trusted_pt(72.0);
const PRESET_HALF_INCH_PT: Length<NonNegative> = Length::from_trusted_pt(36.0);
```

Because `Length::from_trusted_pt` is already `const fn` within `src/geom.rs`, these constants require no runtime float checks or fallible unwrapping.

### 2.3 Value Resolution and Conversion Traits

```rust
impl MarginPreset {
    /// Resolves this preset into a concrete [`Margins`] value with four equal sides.
    ///
    /// This method is `const` and infallible.
    #[must_use]
    pub const fn into_margins(self) -> Margins {
        match self {
            Self::Inch => Margins::uniform(PRESET_INCH_PT),
            Self::HalfInch => Margins::uniform(PRESET_HALF_INCH_PT),
        }
    }
}

impl From<MarginPreset> for Margins {
    #[inline]
    fn from(preset: MarginPreset) -> Self {
        preset.into_margins()
    }
}
```

### 2.4 Crate Root Re-Export in `src/lib.rs`

Export `MarginPreset` alongside `Margins` at the crate root:

```rust
// In src/lib.rs
pub use geom::{
    Color, Finite, Ink, Length, MarginPreset, Margins, NonNegative, Orientation, PageSize,
    Paint, Paper, Path, PathBuilder, PathVerb, Pen, Point, Positive, Rect, Size, Stroke, A4,
    LETTER, MM_TO_PT,
};
```

### 2.5 Invariant Signatures That Do Not Change

1. `Document::margins`:
   ```rust
   pub fn margins(&mut self, margins: Margins)
   ```
   Signature stays strictly exact. Callers provide `MarginPreset::Inch.into()` or `Margins::from(preset)`. No `impl Into<Margins>` widening or parallel `doc.margin_preset(...)` method is added.
2. `Section::margins`:
   ```rust
   pub fn margins(&mut self, margins: Margins)
   ```
   Signature stays strictly exact.
3. `Margins::default()`:
   ```rust
   impl Default for Margins {
       fn default() -> Self {
           Self::uniform(NonNegative::ZERO)
       }
   }
   ```
   Remains strictly four zeros (0.0 pt).
4. `Section::new`:
   Initializes `margins: Margins::default()`.

---

## 3. Module Map

| File | Status | Description of Changes |
| --- | --- | --- |
| `src/geom.rs` | **MODIFIED** | Define `pub enum MarginPreset { Inch, HalfInch }`<br>Add private compile-time constants `PRESET_INCH_PT` (72 pt) and `PRESET_HALF_INCH_PT` (36 pt)<br>Implement `pub const fn into_margins(self) -> Margins`<br>Implement `From<MarginPreset> for Margins`<br>Add unit tests verifying exact resolution, zero default, and const resolution |
| `src/lib.rs` | **MODIFIED** | Re-export `MarginPreset` from `geom` |
| `src/document.rs` | **UNTOUCHED** | Prod signatures for `Document::margins` and `Section::margins` remain exact `Margins` |
| `src/layout/lower.rs` | **UNTOUCHED** | Lowers section into `PaginateConfig` using existing `Margins` |
| `src/layout/paginate.rs` | **UNTOUCHED** | Derives body frame `(left, top, pw - left - right, ph - top - bottom)` and validates positive frame |
| `src/layout/decorate.rs` | **UNTOUCHED** | Top/bottom continue to serve as header/footer band capacities; copies margins into `PageInfo` |
| `src/backend/*` | **UNTOUCHED** | Translates canvas recording commands; completely oblivious to margin presets |
| `src/error.rs` | **UNTOUCHED** | Existing `Error::InvalidGeometry` covers too-large margins; preset conversion is infallible |
| `tests/document.rs` | **MODIFIED (tests only)** | Add integration tests covering preset body insetting, band height allocation, explicit section isolation, tiny page geometry errors, and parity with manual `Margins::uniform` |

---

## 4. Lowering, Measurement & Invariant Notes

### 4.1 Boundary Termination

Presets terminate completely at the caller boundary:

```text
MarginPreset
    │
    ▼ (into_margins / From)
Margins { top: 72pt, right: 72pt, bottom: 72pt, left: 72pt }
    │
    ▼
Document::margins / Section::margins
    │
    ▼ (Section::lower)
PaginateConfig { margins: Margins, page_size: PageSize }
    │
    ▼
layout / decorate / recording canvas
```

No preset discriminant or enum variant penetrates into `Section`, `PaginateConfig`, `PageInfo`, or `Canvas`.

### 4.2 Page Geometry and Decoration Invariants

1. **Body Frame Coordinates:**
   For a page of dimensions `(pw, ph)` with `MarginPreset::Inch`:
   ```text
   body_x = 72.0 pt
   body_y = 72.0 pt
   body_width = pw - 144.0 pt
   body_height = ph - 144.0 pt
   ```
2. **Decoration Band Capacity:**
   - Header band: `y = 0.0 .. 72.0 pt`, available height = 72.0 pt.
   - Footer band: `y = (ph - 72.0 pt) .. ph`, available height = 72.0 pt.
   - If decoration content exceeds 72.0 pt, `decorate.rs` returns `Error::DecorationTallerThanMargin`.
3. **Band Auto-Inset Seam Acknowledgment:**
   In the current codebase (`src/document.rs:Band::left/right`), slots automatically inset text by `margins.left()` and `margins.right()`. As documented in `.audit/knife-18/how.md`, this differs from the §2.10 confirmed prose (which envisions full-width decoration with manual caller insetting). Preset design respects this seam: choosing `MarginPreset::Inch` causes `Band::left` to inset by 72.0 pt rather than 0.0 pt. Knife 18 does NOT attempt to alter this existing Band layout behavior.

---

## 5. Test Strategy & Test Matrix

### 5.1 Unit Tests in `src/geom.rs`

1. `preset_inch_resolves_to_exact_72pt`:
   - `MarginPreset::Inch.into_margins()` produces `Margins` where top, right, bottom, left are all `NonNegative::pt(72.0).unwrap()`.
   - `Margins::from(MarginPreset::Inch) == Margins::uniform(NonNegative::pt(72.0).unwrap())`.
2. `preset_half_inch_resolves_to_exact_36pt`:
   - `MarginPreset::HalfInch.into_margins()` produces `Margins` where top, right, bottom, left are all `NonNegative::pt(36.0).unwrap()`.
   - `Margins::from(MarginPreset::HalfInch) == Margins::uniform(NonNegative::pt(36.0).unwrap())`.
3. `preset_into_margins_is_const`:
   - Assert `const M: Margins = MarginPreset::Inch.into_margins();` compiles in a const context.
4. `margins_default_remains_four_zeros`:
   - Assert `Margins::default().top().as_pt() == 0.0`.
   - Assert `Margins::default().bottom().as_pt() == 0.0`.
   - Assert `Margins::default().left().as_pt() == 0.0`.
   - Assert `Margins::default().right().as_pt() == 0.0`.
5. `preset_trait_implementations`:
   - Assert `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash` work as expected.

### 5.2 Integration Tests in `tests/document.rs`

1. `document_preset_inch_insets_body_flow`:
   - Construct a document with `MarginPreset::Inch.into()`. Add a paragraph.
   - Render to `RecordingCanvas`.
   - Assert glyph placements have `x >= 72.0` and `y >= 72.0` (offset by body origin).
2. `document_preset_half_inch_insets_body_flow`:
   - Construct a document with `MarginPreset::HalfInch.into()`.
   - Assert glyph placements have `x >= 36.0` and `y >= 36.0`.
3. `document_preset_parity_with_manual_uniform`:
   - Render document A with `doc.margins(MarginPreset::Inch.into())`.
   - Render document B with `doc.margins(Margins::uniform(NonNegative::pt(72.0).unwrap()))`.
   - Assert recorded command streams are 100% identical.
4. `document_preset_header_footer_band_allocation`:
   - Add header with a 50 pt line box.
   - With `MarginPreset::Inch` (72 pt band), layout succeeds and header renders in `0..72 pt`.
   - With default zero margins, layout fails with `DecorationTallerThanMargin`.
5. `section_presets_isolated`:
   - Section 1 uses `MarginPreset::Inch.into()`.
   - Section 2 uses `MarginPreset::HalfInch.into()`.
   - Section 3 does not call `margins` (remains zero).
   - Assert page 1 body starts at `(72.0, 72.0)`, page 2 body starts at `(36.0, 36.0)`, page 3 body starts at `(0.0, 0.0)`.
6. `preset_on_tiny_page_fails_invalid_geometry`:
   - Create a page with width = 100 pt, height = 100 pt.
   - Apply `MarginPreset::Inch.into()`.
   - Layout returns `Err(Error::InvalidGeometry { .. })`.

---

## 6. Out-of-Knife Boundaries

The following items are explicitly out of scope for Knife 18:

| Out-of-Knife Item | Status / Reason |
| --- | --- |
| `assoc-consts` (`Margins::INCH`) | Rejected reverse word. Presets belong in a distinct, discoverable enum rather than bloating the `Margins` value type. |
| `mm-round` | Rejected reverse word. 72 pt is exactly 1 inch in PDF points; rounding via mm introduces floating point noise and breaks exact contracts. |
| `Normal` / `Narrow` names | Rejected reverse word. Vague semantic labels imply default behavior and lack precise numeric contracts. |
| `add-quarter` (Quarter-inch / 18 pt) | Rejected reverse word. §7 proposal explicitly lists 72 pt and 36 pt. Additional variants deferred until needed. |
| `default-inch` | Rejected reverse word. §7 contractual mandate: default page margins are strictly four zeros. |
| `doc.margin_preset(...)` | Rejected reverse word. Duplicates entry points; `doc.margins(preset.into())` keeps API surface minimal. |
| Asymmetric presets (e.g., Word 1.25" sides, gutter margins) | Out of scope. Callers use `Margins::new(...)` for asymmetric margins. |
| Paper size enum / presets | Knife 19 boundary. |
| Changing `Band` horizontal insetting behavior | Out of scope. Presets alter values, not band insetting algorithms. |
| Serde / JSON serialization | Out of scope. Presets are pure Rust DSL construction sugar. |
