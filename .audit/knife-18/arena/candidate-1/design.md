# Knife 18 — optional margin presets (candidate 1)

**Scope.** Public construction sugar for §7 optional margin presets. Presets resolve to existing `Margins` and reuse unchanged `Document::margins` / `Section::margins` and all pagination/decoration paths.

**Baseline.** `feat/margin-presets` from `feat/cell-rich` tip `9b939b8`.

**Autonomy.** All frame defaults; no reverse words (`assoc-consts`, `mm-round`, `Normal`/`Narrow`, `add-quarter`, `default-inch`, `doc.margin_preset(...)`).

**Grounding.** `.audit/knife-18/how.md`, `data-shape.md`, `docs/提案书.md` §7.

---

## 1. Caller usage sketch

Default document and every new section stay at four zero margins until the caller sets them explicitly:

```rust
use quire::{document, MarginPreset, Margins, PageSize, NonNegative, Section, /* … */};

document(PageSize::a4(), |doc| {
    // Still zero until set:
    doc.margins(MarginPreset::Inch.into());
    // equivalent:
    doc.margins(Margins::from(MarginPreset::HalfInch));
    Ok(())
})?;
```

Explicit section (each section starts at zero; no inheritance from document):

```rust
document(PageSize::a4(), |doc| {
    doc.section(|section| {
        section.margins(MarginPreset::Inch.into());
        Ok(())
    })?;
    Ok(())
})?;
```

Custom asymmetric margins remain the primary API; presets are uniform shortcuts only:

```rust
doc.margins(Margins::new(
    NonNegative::pt(72.0)?,
    NonNegative::pt(36.0)?,
    NonNegative::pt(72.0)?,
    NonNegative::pt(36.0)?,
));
```

No new `Document::margin_preset` overload. Argument type stays `Margins`; conversion is explicit at the call site (`into()` / `From`).

Choosing `Inch` or `HalfInch` sets **all four** sides, including header/footer band heights (top/bottom). With default zero bands, decoration taller than zero still fails with `DecorationTallerThanMargin` until margins are raised.

---

## 2. Types and conversion

### Public enum

```rust
/// Common uniform page margins (§7 optional presets).
///
/// Each variant is four equal sides in PDF points. [`Inch`] is exactly 72 pt
/// (one international inch). [`HalfInch`] is exactly 36 pt (half inch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[must_use]
pub enum MarginPreset {
    /// Uniform 72 pt on all sides (≈1 in).
    Inch,
    /// Uniform 36 pt on all sides (≈0.5 in).
    HalfInch,
}
```

Rustdoc uses “≈1 in / ≈0.5 in” for human readability; the numeric contract is **exact** 72.0 and 36.0 pt, matching §7 examples and PDF units—not a separate “Chinese 寸” unit.

### Resolution to `Margins`

Presets must not be stored on `Document`, `Section`, or layout config. They terminate at the geometry boundary:

```rust
impl MarginPreset {
    /// Resolves this preset to a [`Margins`] value.
    #[must_use]
    pub const fn into_margins(self) -> Margins {
        match self {
            Self::Inch => Margins::uniform(PRESET_INCH),
            Self::HalfInch => Margins::uniform(PRESET_HALF_INCH),
        }
    }
}

impl From<MarginPreset> for Margins {
    fn from(preset: MarginPreset) -> Self {
        preset.into_margins()
    }
}
```

Trusted compile-time lengths (same pattern as `NonNegative::ZERO` / `A4`):

```rust
// private in src/geom.rs
const PRESET_INCH: Length<NonNegative> = Length::from_trusted_pt(72.0);
const PRESET_HALF_INCH: Length<NonNegative> = Length::from_trusted_pt(36.0);
```

`into_margins` is **infallible** and `const`, so callers need no `?` for preset selection. Runtime `NonNegative::pt(72.0)?` remains for arbitrary custom values.

### What does not change

| Item | Stays |
| --- | --- |
| `Margins` fields / accessors | Four `Length<NonNegative>`; `new`, `uniform`, `Default` → four zeros |
| `Document::margins(&mut self, margins: Margins)` | Signature unchanged |
| `Section::margins(&mut self, margins: Margins)` | Signature unchanged |
| `PaginateConfig`, `PageInfo`, decoration | Only see `Margins` |

**Rejected for this candidate:** `impl Into<Margins> for Document::margins` (widens API without need); associated constants on `Margins` (`Margins::INCH`, …) — see rationale (`assoc-consts`).

---

## 3. Module map

| Location | Change |
| --- | --- |
| `src/geom.rs` | Add `MarginPreset`, `PRESET_*` trusted lengths, `into_margins`, `From<MarginPreset> for Margins`; extend module docs for §7 sugar |
| `src/lib.rs` | Re-export `MarginPreset` alongside `Margins`, `NonNegative`, `Length` |
| `src/document.rs` | **No change** (already accepts `Margins`) |
| `src/layout/paginate.rs` | **No change** |
| `src/layout/decorate.rs` | **No change** |
| `tests/document.rs` | Add preset-focused integration tests (below); optional duplicate in `geom` unit tests |
| `src/geom.rs` tests | Optional: `margin_preset_inch_equals_uniform_72` |

Dependency direction unchanged: no `krilla`; layout never references `MarginPreset`.

---

## 4. Test plan

### Unit (`src/geom.rs`)

| Test | Assert |
| --- | --- |
| `margin_preset_inch_is_uniform_72_pt` | `MarginPreset::Inch.into_margins()` equals `Margins::uniform(NonNegative::pt(72.0).unwrap())` on all four accessors |
| `margin_preset_half_inch_is_uniform_36_pt` | Same for 36 pt |
| `default_margins_are_zero` | **Existing** — must remain green; presets must not touch `Default` |

### Integration (`tests/document.rs`)

Reuse existing patterns: `document().pages()`, `RecordingCanvas`, `pt_near`, header/footer band tests.

| Test | Assert |
| --- | --- |
| `document_margin_preset_inch_body_origin_and_width` | Body/content x ≈ 72; wrap width ≈ `page_width - 144` on A4 (mirror existing uniform-margin tests) |
| `section_margin_preset_half_inch_scoped` | Second section with `HalfInch` does not inherit document zero; page metadata / paint matches 36 pt inset |
| `margin_preset_equals_manual_uniform` | Same recording output for `MarginPreset::Inch.into()` vs `Margins::uniform(NonNegative::pt(72.0)?)` |
| `fresh_document_default_margins_zero_without_preset` | No `doc.margins` → body still full width at y=0 origin (existing behavior) |

Optional regression: preset + non-empty header still respects band height and side inset (existing `header_slots_inset_by_side_margins_*` style); no new decoration logic.

Do **not** assert compressed PDF bytes; recording coordinates only per AGENTS.md.

---

## 5. Out of knife

Per frame + proposal:

- Changing **default** margins away from four zeros (`default-inch`).
- **Asymmetric** or extra presets (quarter-inch, mm-rounded values, `Normal`/`Wide` names) — `add-quarter`, `mm-round`, semantic names without numeric contract.
- **`doc.margin_preset(...)`** dedicated DSL method — conversion stays at argument.
- **`impl Into<Margins>`** on `Document::margins` unless a later knife explicitly widens the API.
- Header/footer **band width contract** rework (§2.10 full-width vs current `Band` auto-inset); presets only scale existing inset.
- Paper size enum changes, section page-size knife (**knife 19**).
- Layout/backend edits “for symmetry”; CHANGELOG beyond rustdoc on new enum.

---

## 6. Implementation checklist (implement PR, not this artifact)

1. Add `MarginPreset` + constants + `From` in `geom.rs`.
2. Re-export from `lib.rs`.
3. Unit tests for 72/36 equality and default unchanged.
4. One or two document recording tests proving preset ≡ manual uniform.
5. `cargo fmt`, `cargo clippy -D warnings`, `cargo test`.
