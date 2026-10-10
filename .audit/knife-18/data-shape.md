# Knife 18 data shape (optional margin presets)

**Authority.** `docs/提案书.md` §7 and §2.10.  
**Baseline.** `Margins` is four `Length<NonNegative>` values; `Default` is four zeros.

## Domain shape

```text
preset choice
  -> Margins::new(top, right, bottom, left)
  -> Document::margins / Section::margins
  -> existing PaginateConfig
```

Presets are public construction sugar only. They must resolve to `Margins`; no preset enum/value should survive into `Document`, `Section`, pagination, decoration, or canvas. `Margins::default()` remains zero.

Proposed example values from §7:

| Example | Resolved value |
| --- | --- |
| 72 pt / “约一寸” | `Margins::uniform(NonNegative::pt(72.0)?)` |
| 36 pt | `Margins::uniform(NonNegative::pt(36.0)?)` |

Top/bottom remain header/footer band heights. Presets do not introduce separate decoration dimensions.

## Arena open calls

| Call | Options / constraint |
| --- | --- |
| Public shape | `MarginPreset` enum with `From<MarginPreset> for Margins`; `Margins` associated constants/functions; or named free constants. All must yield `Margins`. |
| Call spelling | Keep `margins(Margins)` exact and require explicit conversion, or accept `impl Into<Margins>`. Exact argument type is the smaller API change. |
| Names | Product decision: numeric (`Pt72`, `Pt36`), semantic (`OneInch`, `HalfInch`), or document-language names. Avoid ambiguous `Normal`/`Wide` without a stated contract. |
| Catalog | Only uniform 72/36 initially, or additional/asymmetric conventions. Every entry still resolves to four lengths. |
| Unit promise | 72 pt is exactly one PDF inch; decide whether “约一寸” is documentation copy or a public identifier. |
| Export | Re-export a new preset type from `src/lib.rs`, or keep the namespace on already-exported `Margins`. |
| Band mismatch | Separate call: current `Band` slots auto-inset by side margins, while proposal §2.10 says full-width with caller-controlled inset. Do not hide or settle this through preset design. |

## Non-negotiable checks

1. `Margins::default()` and a fresh `Section` stay four zeros.
2. Preset and equivalent custom `Margins` compare equal and render identically.
3. Both document and explicit-section paths accept the produced `Margins`.
4. Custom pages still reject presets that leave non-positive body width/height.
5. 72/36 presets set all four sides, including header/footer band heights.
