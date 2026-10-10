# Knife 18 — optional margin presets

Baseline: `feat/margin-presets` from `feat/cell-rich` tip `9b939b8`. Public margins stay one `Margins` of four `Length<NonNegative>` in CSS order. This knife adds construction sugar only.

Autonomy defaults, all kept: `enum MarginPreset { Inch, HalfInch }` with `into_margins` and `From<MarginPreset> for Margins`, both arms `Margins::uniform` of exact 72 pt and 36 pt, rustdoc “≈1 in” / “≈0.5 in”, no third preset, `Margins::default()` stays four zeros, call site `doc.margins(MarginPreset::Inch.into())` or `Margins::from(preset)`. `Document::margins` and `Section::margins` keep the exact `Margins` argument.

## Usage sketch

```rust
use quire::{document, MarginPreset, Margins, NonNegative, PageSize};

document(PageSize::A4, |doc| {
    doc.font(bytes)?;
    // four sides of 72 pt; top and bottom are the header and footer band heights
    doc.margins(MarginPreset::Inch.into());
    doc.header(|h| {
        let m = h.page().margins;
        h.left("ACME"); // existing Band inset uses m.left(); this knife does not change that
        let _ = m;
    });
    doc.paragraph(|p| p.text("Body"));
    Ok(())
})?;

document(PageSize::A4, |doc| {
    doc.font(bytes)?;
    doc.section(|section| {
        // explicit sections still start at zero; the preset is applied here
        section.margins(Margins::from(MarginPreset::HalfInch));
        section.paragraph(|p| p.text("Half"));
    });
    Ok(())
})?;

// asymmetric margins stay Margins::new; there is no preset for them
let custom = Margins::new(
    NonNegative::pt(72.0)?,
    NonNegative::pt(36.0)?,
    NonNegative::pt(72.0)?,
    NonNegative::pt(36.0)?,
);
```

`MarginPreset::Inch.into()` and `Margins::from(MarginPreset::Inch)` are the same value as `Margins::uniform(NonNegative::pt(72.0)?)`. `HalfInch` is the 36 pt twin. A later `margins` call replaces the whole four-side value, whether the previous value came from a preset or from `new` / `uniform`.

Omitting `margins` leaves `Margins::default()`: four zeros. A preset is never implied by `document(...)` or `Section::new`.

`Inch` on a page whose width is ≤ 144 pt, or whose height is ≤ 144 pt, still fails `Error::InvalidGeometry` in `PaginateConfig::validate`. The preset only builds lengths; it does not bypass the positive-body check. The same rule applies to `HalfInch` at 72 pt.

## Types and signatures

New public type, next to `Margins` in `geom`. Re-export it from `lib.rs` beside `Margins`. No new method on `Document` or `Section`.

```rust
/// Uniform page-margin presets. Each variant is four equal sides.
///
/// Top and bottom are the header and footer band heights, the same as any
/// other [`Margins`]. [`Margins::default`] is four zeros. Choosing a preset
/// is an explicit `margins` call.
///
/// [`MarginPreset::Inch`] is 72 pt on every side: exactly one PDF inch
/// (1 pt = 1/72 in). The proposal’s「约一寸」names that PDF inch. It is not a
/// Chinese 寸. [`MarginPreset::HalfInch`] is 36 pt on every side: exactly
/// half a PDF inch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[must_use]
pub enum MarginPreset {
    /// Four sides of 72 pt (exactly 1 PDF inch; ≈1 in).
    Inch,
    /// Four sides of 36 pt (exactly 0.5 PDF inch; ≈0.5 in).
    HalfInch,
}

impl MarginPreset {
    /// Four equal sides. `Inch` is 72 pt; `HalfInch` is 36 pt.
    pub const fn into_margins(self) -> Margins {
        let side = match self {
            Self::Inch => Length::from_trusted_pt(72.0),
            Self::HalfInch => Length::from_trusted_pt(36.0),
        };
        Margins::uniform(side)
    }
}

impl From<MarginPreset> for Margins {
    fn from(preset: MarginPreset) -> Self {
        preset.into_margins()
    }
}
```

`Length::from_trusted_pt` stays private to `geom`. The literals are the same class as `LETTER` (612 pt by 792 pt): constants that already satisfy `NonNegative`. `NonNegative::pt` stays the runtime checked factory and is what tests compare against. `into_margins` is `const` because `uniform` is `const`. `From` is the `.into()` spelling at the call site; it is not a `const` impl.

`#[non_exhaustive]` matches `Paper`. This knife ships two variants. A later variant does not have to break an external `match`. Callers are expected to convert, not to match.

Unchanged:

```rust
impl Margins {
    pub const fn new(top: Length<NonNegative>, right: Length<NonNegative>, bottom: Length<NonNegative>, left: Length<NonNegative>) -> Self;
    pub const fn uniform(value: Length<NonNegative>) -> Self;
    pub const fn top(self) -> Length<NonNegative>;
    pub const fn right(self) -> Length<NonNegative>;
    pub const fn bottom(self) -> Length<NonNegative>;
    pub const fn left(self) -> Length<NonNegative>;
}

impl Default for Margins {
    fn default() -> Self {
        Self::uniform(NonNegative::ZERO) // still this; do not call a preset
    }
}

impl Section<'_> {
    pub fn margins(&mut self, margins: Margins);
}

impl Document<'_> {
    pub fn margins(&mut self, margins: Margins);
}
```

Do not widen those two methods to `impl Into<Margins>`. The caller converts. Preset state does not enter `Section`, `Document`, `PaginateConfig`, `PageInfo`, or `Page`. `Page.margins` remains a `Margins`. After `MarginPreset::Inch.into()`, `page.margins.top().as_pt()` is `72.0` and that top is the header band height.

No `Margins::INCH` constant. No `margin_preset` method. No millimeter preset constructor.

## Module map

| Location | This knife |
| --- | --- |
| `src/geom.rs` | `MarginPreset`, `into_margins`, `From<MarginPreset> for Margins`. Reuse `Margins::uniform`. `Default` stays `uniform(NonNegative::ZERO)`. Unit tests below |
| `src/lib.rs` | add `MarginPreset` to the existing `geom` re-export |
| `src/document.rs` | no edit. `Section::new` still stores `Margins::default()`. `Document::margins` still delegates to the implicit section and still records `ContentOutsideSection` after explicit sections. `Section::margins` still replaces the four sides. `Page.margins` still copies the resolved `Margins` |
| `src/layout/paginate.rs` | no edit. Body origin `(left, top)`, body size `page − horizontal/vertical margins`, `InvalidGeometry` when the body is not positive |
| `src/layout/decorate.rs` | no edit. Header capacity is `top`, footer origin is `page_height - bottom`. `Band` left/right inset stays as it is |
| `src/backend/` | no edit |
| `tests/document.rs` | the two recording tests below |
| `docs/提案书.md` | no edit. §7 already allows an optional preset enum and leaves the names to implementation |

Dependency direction stays `document → layout → geom`. No `krilla` outside `backend/`.

Resolve at the geometry boundary:

```text
MarginPreset::Inch
  -> into_margins / From
  -> Margins::uniform(72 pt)     // four Length<NonNegative>
  -> Document::margins / Section::margins
  -> PaginateConfig              // unchanged
```

`HalfInch` is the same path at 36 pt.

## Test plan

`cargo test` (geom unit tests do not need `backend-krilla`). Recording tests use `document(...).pages()`, `pt_near`, `FONT`, `BASELINE`. No PDF content-stream asserts. No CHANGELOG entry; the enum rustdoc is the note.

Existing `default_margins_are_zero` in `src/geom.rs` stays the lock that `Margins::default()` is four `NonNegative::ZERO`. Do not retarget it at a preset.

1. **`margin_presets_are_uniform_72_and_36`** (`src/geom.rs`)  
   `MarginPreset::Inch.into_margins()` equals `Margins::uniform(NonNegative::pt(72.0).unwrap())` on `top`, `right`, `bottom`, and `left`. `Margins::from(MarginPreset::Inch)` equals `into_margins()`. `HalfInch` equals `uniform(NonNegative::pt(36.0).unwrap())` on all four sides. `assert_ne!` against `Margins::default()`.

2. **`margin_preset_matches_uniform_on_document_and_section`** (`tests/document.rs`)  
   Page `PageSize::A4`, body font, one glyph `"A"`.  
   Implicit document: `doc.margins(MarginPreset::Inch.into())` plus a header `h.left("H")`. Body glyph `x = 72`, `y = 72 + BASELINE`. Header glyph `x = 72` (today’s side inset), `y = BASELINE`. Inside the header closure, `h.page().margins == Margins::from(MarginPreset::Inch)`.  
   A second build uses `Margins::uniform(NonNegative::pt(72.0).unwrap())` and the same header text. Glyph positions match the preset build.  
   Explicit section: `section.margins(Margins::from(MarginPreset::HalfInch))`, body `"B"`, no header. Body glyph `x = 36`, `y = 36 + BASELINE`. A sibling section that never calls `margins` keeps body `x = 0` (sections do not inherit). That sibling is the zero-default witness on the section path; `default_margins_are_zero` remains the value witness.

3. **`inch_preset_on_a_narrow_page_is_invalid_geometry`** (`tests/document.rs`)  
   `PageSize::custom` of width 100 pt and height 200 pt. `doc.margins(MarginPreset::Inch.into())` and a one-glyph paragraph. `Error::InvalidGeometry`, no pages. 72 + 72 ≥ 100, so the failure is the existing body-width check. A page of 200×200 with the same preset succeeds; this test does not need to paint it if test 2 already paints 72 pt on A4.

No new layout test. Band capacity, zero-band `DecorationTallerThanMargin`, and `Band` side inset stay on the tests that already pass a `Margins` (`header_with_zero_margin_is_rejected`, `header_slots_inset_by_side_margins_and_body_sits_below_the_band`). Test 2 only checks that a preset is that `Margins`.

## Out of knife

- `Margins::default()` becoming 72 pt (`default-inch`). `Section::new` stays zeros. The design draft’s 72 pt default is overridden by §7.
- Associated constants or free functions on `Margins` (`assoc-consts`): `Margins::INCH`, `Margins::inch()`, `const INCH: Margins`.
- Names `Normal` / `Narrow`, `Wide`, `Pt72` / `Pt36`, or a public identifier `约一寸` (`Normal`/`Narrow`).
- A third preset, including 18 pt (`add-quarter`).
- Values taken from `NonNegative::mm(25.4)` or any rounded millimeter (`mm-round`). `NonNegative::mm` already exists for custom lengths.
- `doc.margin_preset(...)` / `section.margin_preset(...)` (`doc.margin_preset(...)`).
- `impl Into<Margins>` on `Document::margins` and `Section::margins`.
- Storing `MarginPreset` in `Document`, `Section`, `PaginateConfig`, or `Page`.
- Asymmetric presets. Callers use `Margins::new`.
- A second margin type for decoration. Top and bottom of the resolved `Margins` are the band heights.
- Header/footer band contract: full-width versus the current `Band` left/right auto-inset. Presets scale whatever inset already exists. They do not settle §2.10.
- Paper-size catalog (`knife 19`).
- `behind` / `front` auto-inset. They stay page coordinates; callers read `Page.margins`.
- CHANGELOG. Rustdoc on `MarginPreset` is enough.
- Second PDF backend, or `krilla` outside `backend/`.
