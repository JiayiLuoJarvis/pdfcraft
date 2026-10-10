# Knife 18 Candidate 4 — Rationale

## Recommendation in One Sentence

Introduce **`MarginPreset` (`Inch`, `HalfInch`)** in **`src/geom.rs`** as an infallible, compile-time **`const`** sugar enum resolving via **`into_margins`** and **`From<MarginPreset> for Margins`** to exact **72.0 pt and 36.0 pt** uniform **`Margins`**, re-exported from **`src/lib.rs`**, while keeping **`Margins::default()`** strictly at four zeros, preserving exact **`margins(Margins)`** signatures, and leaving **`src/document.rs`** and all layout/backend modules completely untouched.

---

## 1. Adherence to Autonomy Defaults

Candidate 4 strictly follows all autonomy defaults from `arena/frame.md` with zero reverse-word divergences:

| Autonomy Call | Candidate 4 Stance | Reverse Word Avoided | Detailed Justification |
| --- | --- | --- | --- |
| **Shape** | `pub enum MarginPreset { Inch, HalfInch }` with `into_margins` and `From` trait | `assoc-consts` | Presets form an identifiable, discoverable catalog of choices. Attaching them as associated constants on `Margins` (`Margins::INCH`) blurs the distinction between a core geometric value and an optional recipe. |
| **Values** | Exact 72.0 pt and 36.0 pt | `mm-round` | In standard PDF coordinates, 1 point is defined as exactly 1/72 of an international inch. 72.0 pt is an exact integer representation, not an approximation. Rounding via millimeters (e.g. 25.4 mm * 2.8346457) introduces floating-point noise and violates precision invariants. |
| **Names** | `Inch` and `HalfInch` (documented as ≈1 in / 0.5 in and proposal §7 "约一寸" / "36pt") | `Normal`/`Narrow` | Semantic names like `Normal` and `Narrow` are ambiguous, context-dependent, and falsely imply that 72 pt is the library's "normal" (default) margin, directly contradicting §7. |
| **Catalog** | Exactly two presets (`Inch`, `HalfInch`) | `add-quarter` | Strictly observes the laziness rubric. §7 mentions 72 pt and 36 pt as concrete examples. Extra presets (such as 18 pt or 54 pt) are speculative and unneeded for this knife. |
| **Default** | Unchanged: `Margins::default()` is four zeros | `default-inch` | Proposal §7 explicitly mandates: "默认页边距 = 0（四边）". Presets are strictly opt-in sugar and must never alter the zero-margin default. |
| **Call Site** | `doc.margins(MarginPreset::Inch.into())` or `Margins::from(preset)` | `doc.margin_preset(...)` | Calling `doc.margins(...)` keeps the API surface small, maintains a single entry point, and reinforces that margins are stored as standard `Margins`. |

---

## 2. In-Depth Architectural Tradeoffs

### 2.1 Shape: `MarginPreset` Enum vs Associated Constants (`assoc-consts`)

- **Why Enum is Superior:**
  1. **Clear Mental Model:** A preset is a selection from a curated catalog. An enum models this catalog explicitly.
  2. **Discoverability:** Callers typing `MarginPreset::` in IDE autocomplete immediately see all supported presets without wading through geometric constructors (`new`, `uniform`, `top`, `left`, `channels`).
  3. **Documentation Locality:** The rustdoc for `MarginPreset` can thoroughly explain the conversion mechanics, the PDF unit contract (72 pt/inch), and the interaction with header/footer band capacities in one dedicated place.
  4. **Future Extensibility:** Marking the enum `#[non_exhaustive]` allows new presets to be added in future knives without breaking exhaustive `match` statements in user code.

- **Why Associated Constants (`Margins::INCH`) Were Rejected:**
  1. It pollutes `Margins`, which is a low-level geometric primitive representing four non-negative lengths.
  2. It creates pressure to add more associated constants (e.g. `Margins::ZERO`, `Margins::HALF_INCH`, `Margins::A4_MARGIN`, `Margins::NARROW`), blurring the boundary between basic geometry and document-level presets.

### 2.2 Numeric Precision: Exact 72/36 pt vs Millimeter Rounding (`mm-round`)

- In digital typography and PDF specification (ISO 32000-1 §8.3.2.3), the default user space unit is 1/72 of an inch.
- Therefore:
  - 1 international inch = 72.0 pt (exact).
  - 0.5 international inch = 36.0 pt (exact).
- The proposal §7 refers to "四边 72pt / 约一寸". The phrase "约一寸" in Chinese reflects colloquial document drafting parlance, not a commitment to the Chinese market inch (市寸 ≈ 33.3 mm).
- Attempting to compute margins from millimeters (`25.4 * MM_TO_PT`) would evaluate to `71.999996` or `72.000004` due to single-precision float rounding in `f32`.
- Candidate 4 uses `Length::from_trusted_pt(72.0)` and `Length::from_trusted_pt(36.0)`, guaranteeing exact, integer-aligned point dimensions and total compile-time safety.

### 2.3 Product Naming: Physical Units vs Semantic Labels

- **The Danger of `Normal` / `Narrow`:**
  - In word processors (e.g. Microsoft Word), "Normal" margin is 1 inch (72 pt). However, in Word, "Normal" is also the default.
  - In `quire`, the default margin is **0 pt** (full bleed, caller-controlled). If `quire` provided `MarginPreset::Normal`, users would naturally assume that omitting `doc.margins(...)` applies `MarginPreset::Normal`.
  - Furthermore, "Narrow" has varying definitions across software suites (0.5 in, 12.7 mm, or 0.75 in).
- **The Clarity of `Inch` and `HalfInch`:**
  - `Inch` and `HalfInch` describe objective, physical quantities.
  - They set clear, unambiguous expectations for both Latin and CJK layout scenarios.

### 2.4 Call Site Ergonomics: Exact `Margins` vs `impl Into<Margins>` vs `margin_preset`

- **Preserving Exact `margins(&mut self, margins: Margins)`:**
  - `data-shape.md` notes: "Exact argument type is the smaller API change."
  - Using exact `Margins` requires zero signature changes in `src/document.rs`.
  - Because `Margins` implements `From<MarginPreset>`, callers can write either:
    - `doc.margins(MarginPreset::Inch.into())`
    - `doc.margins(Margins::from(MarginPreset::Inch))`
    - `doc.margins(MarginPreset::Inch.into_margins())`
  - All three options are idiomatic in Rust. Type inference in `doc.margins(preset.into())` works seamlessly without explicit type annotations because the parameter type is concretely `Margins`.

- **Why Not `doc.margin_preset(...)`:**
  - Adding a parallel builder method would duplicate documentation, increase API surface area, and suggest that presets might be stored or handled differently from custom margins inside `Document`.

- **Why Not `impl Into<Margins>` in this Knife:**
  - While harmless, changing `pub fn margins(&mut self, margins: impl Into<Margins>)` introduces generics/monomorphization to `Document` and `Section`. Following the principle of minimal diff and strict adherence to the autonomy defaults, Candidate 4 keeps the signature exact.

### 2.5 Intentional Omission of `Default` for `MarginPreset`

- `Margins` implements `Default`, returning four zeros (`Margins::uniform(NonNegative::ZERO)`).
- `MarginPreset` must **NOT** implement `Default`.
- If `MarginPreset::default()` were implemented, which variant would it return?
  - Returning `MarginPreset::Inch` would imply 1 inch is the default preset, conflicting with `Margins::default() == 0`.
  - Adding a `MarginPreset::Zero` variant just to satisfy `Default` would confuse the purpose of presets (which are shortcuts for common non-zero margins).
- Therefore, omitting `Default` enforces clarity: margins are zero unless the caller explicitly picks a preset or provides custom lengths.

---

## 3. Boundary Termination & Layout Invariants

### 3.1 Total Encapsulation within `geom`

The lowering pipeline maintains absolute architectural honesty:

```text
Preset selection (MarginPreset::Inch)
         │
         ▼ (pure const evaluation)
Margins { top: 72pt, right: 72pt, bottom: 72pt, left: 72pt }
         │
         ▼
Section::margins(margins)
         │
         ▼
PaginateConfig { margins, page_size }
```

- Zero bytes of preset metadata are passed downstream.
- Neither `Section`, `Document`, `PaginateConfig`, `PageInfo`, nor `Canvas` contains any knowledge of `MarginPreset`.
- Consequently, layout regressions are mathematically impossible: custom margins and preset margins follow the exact same code paths.

### 3.2 Dual Responsibility: Body Origin and Band Capacity

- In `quire`, top and bottom margins serve two distinct, synchronized roles (§2.10, `how.md`):
  1. They shrink the vertical body flow: `body_height = page_height - top - bottom`.
  2. They establish the header and footer band capacities:
     - Header band capacity = `top` (located at `y = 0 .. top`).
     - Footer band capacity = `bottom` (located at `y = page_height - bottom .. page_height`).
- When a caller chooses `MarginPreset::Inch`, they are simultaneously:
  - Insetting the body text 72 pt from the top and bottom of the page.
  - Allocating a 72 pt vertical band for headers and a 72 pt vertical band for footers.
- Any header or footer closure exceeding 72 pt will be rejected by `decorate.rs` with `Error::DecorationTallerThanMargin`.
- Rustdoc on `MarginPreset::Inch` and `MarginPreset::HalfInch` explicitly highlights this dual behavior so callers are not surprised.

### 3.3 Seam Acknowledgment: Band Auto-Insetting

- As identified in `.audit/knife-18/how.md`, `Band::left` and `Band::right` in the current implementation automatically inset content horizontally by `margins.left()` and `margins.right()`, whereas §2.10 confirmed prose describes full-width decoration with manual caller-controlled insets.
- Presets naturally scale this existing behavior: using `MarginPreset::Inch` causes `Band::left` to inset by 72 pt instead of 0 pt.
- Knife 18 does not alter or attempt to resolve this underlying Band behavior; it cleanly integrates with the existing layout seam.

---

## 4. Risk Register & Mitigations

| Risk | Impact | Mitigation Strategy |
| --- | --- | --- |
| Caller confuses `Inch` with Chinese 市寸 | Caller expects ~33.3 mm (1/30 meter) instead of 25.4 mm (72 pt) | Comprehensive rustdoc explicitly stating: `Inch` is exactly 72.0 PDF points (25.4 mm). Document "约一寸" only as descriptive copy. |
| Preset applied to tiny page breaks pagination | `PaginateConfig::validate` rejects non-positive body frame | Let existing `Error::InvalidGeometry` handle this failure mode naturally; document in rustdoc and add an explicit integration test. |
| Confusion over default margins | Caller assumes choosing `quire` automatically sets 1-inch margins | Reiterate in rustdoc that `Margins::default()` remains four zeros; `MarginPreset` does not implement `Default`. |
| Unintended layout drift | Presets behave differently from manual `Margins::uniform` | Unit and integration tests prove that preset and manual `Margins::uniform` produce identical values and identical `RecordingCanvas` output. |

---

## 5. Fidelity Checklist (Arena Rubric Compliance)

1. **Proposal Fidelity:**
   - Default page margins remain strictly four zeros (§7).
   - Presets resolve directly to four `Length<NonNegative>` values (§7).
   - Exact 72 pt and 36 pt uniform options provided (§7).
2. **Caller Surface:**
   - Minimal, idiomatic public API: `pub enum MarginPreset { Inch, HalfInch }`.
   - Works directly with existing `doc.margins(...)` and `section.margins(...)`.
3. **Honesty:**
   - Presets do not create a secondary margin type.
   - Top and bottom margins accurately reflect header/footer band capacities.
4. **Fit to Seams:**
   - Defined cleanly in `src/geom.rs` and re-exported in `src/lib.rs`.
   - Reuses existing `Margins::uniform` and `Length::from_trusted_pt`.
   - Zero production code edits required in `src/document.rs`, `src/layout/`, or `src/backend/`.
5. **Testability:**
   - Direct unit tests verifying `MarginPreset::Inch.into_margins() == Margins::uniform(NonNegative::pt(72.0).unwrap())`.
   - Integration tests verifying canvas coordinate offsets, band heights, section isolation, and geometry validation errors.
6. **Laziness:**
   - Exactly two variants (`Inch`, `HalfInch`).
   - No unnecessary dependencies or complex conversions.
   - Minimal diff across the entire repository.
