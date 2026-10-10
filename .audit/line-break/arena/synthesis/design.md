# Knife 5 synthesis (reconciled)

## Synthesis decision

**Base: candidate 2** (`ClusteredText` + width callback + BK-free `glyph_text`), per [Arena cross-judge](cross-judge.md). Parent initially favored cand4; judge caught real defects in cand4 stub/measured bridges and weak atom invariants. Agreement after re-read: ship cand2.

**Grafts from candidate 3**

- Full acceptance test matrix (ASCII space, wide AL, ID|ID, 好（世, （世界, 字。下, CRLF, leading/overflow SP, U+3000, combining, Other|Other, ID|Other, empty line).
- `PairClass` so the 6×6 lookup cannot take `Bk`.
- OP/CL literal tables and classify edge tests (× ÷ U+F900 U+FF21 ext B).

**From candidate 4:** `W` entry check → `InvalidGeometry`; compact line result naming is fine if backed by cand2 cursors.

**Rejected:** cand1 stamp/seams; cand3 sticky pieces as base; cand4 as base.

## Usage

```rust
let text = ClusteredText::new("好（世");
let lines = break_lines(&text, 20.0, |cluster| {
    10.0 * cluster.scalar_count() as f32
})?;
// lines → ["好", "（世"]

let text = ClusteredText::new(paragraph);
let measured = measure_text(fonts, font, text.glyph_text())?; // no BK in cmap
let size = font_size.as_pt();
let lines = break_lines(&text, avail, |cluster| {
    measured.glyphs[cluster.glyph_range()]
        .iter()
        .map(|g| g.advance_norm * size)
        .sum()
})?;
```

Stub tests use scalar_count × fixed width (10 fu / upem 1000 → 0.01 norm, or 10.0 font-unit style with matching W). Prefer one consistent unit in tests (e.g. width 10 per BMP scalar, W in same units).

## Shape to implement

| Module | Contents |
| --- | --- |
| `break_class` | `BreakClass { Hard, Pair(PairClass) }` or Bk + `PairClass`; `classify`; `allows(PairClass, PairClass)` over const `ALLOW` |
| `cluster` | `ClusteredText::new` (normalize CRLF, graphemes, build glyph_text without BK, private cursors); `Cluster` view with `scalar_count`, `glyph_range`, `as_str` |
| `line_break` | `break_lines(text, avail, width_of) -> Result<BrokenLines>`; steps 2–9; private atom tape with class+width; leading SP drop; BK hard break; left backtrack via `allows` |
| `error` | already has `UnbreakableWiderThanFrame` |
| `Cargo.toml` | already pinned `unicode-segmentation = "=1.13.3"` |
| `measure` | unchanged |

## Non-goals this knife

Alignment, justify, kerning, bidi, pagination, DSL, rewriting measure.rs.

## Next step

Implement modules + tests from cand3 matrix; run four gates both feature configs; one commit on `feat/line-break`.
