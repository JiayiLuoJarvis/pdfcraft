# Knife 18 synthesis — margin presets

**Base.** candidate-2 (cross-judge 30/30).  
**Grafts.** c3 focused tests; c1 rustdoc note that presets set header/footer band heights.  
**Reject.** c4 oversized test matrix; `doc.margin_preset`; changing setter to `impl Into<Margins>`.

## Locked API

```rust
#[non_exhaustive]
pub enum MarginPreset {
    /// Exactly 72 PDF points (one international inch) on every side.
    Inch,
    /// Exactly 36 PDF points on every side.
    HalfInch,
}

impl MarginPreset {
    pub const fn into_margins(self) -> Margins;
}

impl From<MarginPreset> for Margins { … }
```

No `Default` on preset. Values via private `Length::from_trusted_pt` + `Margins::uniform`. Re-export from `lib.rs`.

Call: `doc.margins(MarginPreset::Inch.into())`.

## Tests

1. geom: `Inch`/`HalfInch` == `uniform(pt(72|36))`; `Margins::default()` zeros; preset has no Default implication.
2. document: `MarginPreset::Inch` body x equals hand-written `uniform(72)` (pt_near).
3. Optional: tiny page + Inch → `InvalidGeometry`.

## Files

`src/geom.rs`, `src/lib.rs`, `tests/document.rs` and/or geom unit tests.
