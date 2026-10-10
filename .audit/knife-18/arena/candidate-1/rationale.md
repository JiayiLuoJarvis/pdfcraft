# Knife 18 candidate 1 — rationale

## Recommendation in one sentence

Add a small public **`MarginPreset` enum (`Inch`, `HalfInch`)** in **`geom`**, resolve with **`const into_margins` / `From<MarginPreset> for Margins`** to **`Margins::uniform`** at exact **72 pt / 36 pt**, re-export from **`lib.rs`**, and prove with **unit equality tests plus one recording test**—without touching **`Document::margins` signatures** or defaults.

---

## Why autonomy defaults (no reverse words)

### Shape: enum + `From`, not associated constants on `Margins`

**Adopted (`MarginPreset` enum).** Presets are a **catalog of named choices**, not intrinsic fields of the margin value. Keeping `Margins` as “four lengths only” preserves the data-shape invariant: layout and pagination never branch on preset identity (`data-shape.md`, `how.md`).

**Rejected `assoc-consts`:** `Margins::INCH` blurs “value” vs “recipe” and invites future asymmetric constants on the same type. A separate enum makes “optional §7 sugar” obvious in docs and grep.

### Values: exact 72 pt and 36 pt

**Adopted.** §7 lists “四边 72pt / 约一寸” and “四边 36pt”. PDF points define the inch; 72.0 is exact, not a mm conversion (`how.md` gotcha on 约一寸).

**Rejected `mm-round`:** introduces a second numeric story (25.4 mm/in rounding) without proposal ask; violates rubric laziness.

### Names: `Inch` / `HalfInch` with rustdoc ≈ inch wording

**Adopted.** Names state the **PDF/international inch** contract; rustdoc can repeat proposal’s “约一寸” as descriptive copy without implying a different unit.

**Rejected `Normal`/`Narrow`:** product language with no fixed pt mapping; rubric warns against ambiguous semantic names (`data-shape.md` open call).

### Catalog: two presets only

**Adopted.** Matches proposal examples and frame laziness (“two or three presets enough”).

**Rejected `add-quarter`:** expands product surface before §7 “具体有哪几档” is needed for v1.

### Default: four zeros unchanged

**Adopted.** Contractual in §7 and implemented in `Margins::default()` + `Section::new` (`how.md` takeaway #2).

**Rejected `default-inch`:** directly violates proposal fidelity rubric #1.

### Call site: explicit conversion into existing `margins(Margins)`

**Adopted.** Smallest API change (`data-shape.md`: “exact argument type is the smaller API change”). Preset state never enters document types.

**Rejected `doc.margin_preset(...)`:** parallel entry point duplicates docs and suggests presets might be stored differently.

---

## Conversion mechanics: infallible `const` path

`NonNegative::pt` is fallible at runtime; preset literals are not. Use **`Length::from_trusted_pt(72.0)` / `36.0`** (same trusted-constant pattern as `NonNegative::ZERO` and paper sizes) so:

- `MarginPreset::Inch.into_margins()` is **`const`** and needs no `?`;
- `impl From<MarginPreset> for Margins` stays honest and ergonomic at `doc.margins(MarginPreset::Inch.into())`.

**Rejected `TryFrom` only:** would force `?` on every preset call though failure is impossible for defined variants—worse ergonomics for no safety gain.

---

## Alternatives considered

### 1. Associated functions on `Margins` (`Margins::one_inch()`)

**Rejected (same as `assoc-consts`).** Works technically but attaches catalog entries to the value type; harder to extend with a third preset without cluttering `Margins` impl block.

### 2. Free constants in `geom` (`pub const MARGIN_ONE_INCH: Margins`)

**Rejected.** No discoverable enum; callers pass bare `Margins` and lose preset naming in match/docs.

### 3. `impl Into<Margins>` on `Document::margins`

**Rejected for this knife.** Convenient but widens public signature and implies other `Into` sources later; frame default keeps explicit conversion.

### 4. Re-export only via `geom::MarginPreset` without `lib.rs`

**Rejected.** `Margins` is crate-root exported; presets are equally public DSL sugar and should appear beside it.

---

## Honesty and seam fit

- **Top/bottom = band heights:** choosing `Inch` sets 72 pt header/footer capacity even with no decoration installed—callers must understand preset side effects (`how.md`); rustdoc one-liner on `MarginPreset` should say all four sides including bands.
- **Page-size validation:** presets can still yield `InvalidGeometry` on tiny custom pages; no special-case preset bypass.
- **Band inset mismatch:** current `Band` lowering auto-insets by left/right margins while §2.10 prose differs; presets only change magnitude of existing behavior—**not** settled here (`data-shape.md` Band row).

---

## Risk register

| Risk | Mitigation |
| --- | --- |
| Callers confuse `Inch` with Chinese 寸 | Rustdoc: exact 72 pt; “≈1 in” wording only |
| Duplicate tests vs existing `Margins::uniform(36)` tests | New tests assert **preset ≡ manual uniform**, not re-prove all pagination |
| Future third preset | Add enum variant + trusted const; no layout change |

---

## Fidelity checklist (frame rubric)

1. **Proposal fidelity:** default four zeros; 72/36 uniform examples as sugar.
2. **Caller surface:** small enum; works with `doc.margins` / `section.margins`.
3. **Honesty:** one `Margins` type; top/bottom remain band heights.
4. **Fit to seams:** `geom` + `lib.rs` re-export only; no layout/backend.
5. **Testability:** preset equals `Margins::uniform(NonNegative::pt(N))`; default still zero.
6. **Laziness:** two presets; no mm sugar; no CHANGELOG requirement beyond rustdoc.
