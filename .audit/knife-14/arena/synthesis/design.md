# Knife 14 synthesis: §2.2 inline blank

**Base.** Candidate 3 (cross-judge 30/30; parent agrees).
**Grafts.**

| From | Keep |
| --- | --- |
| C1 | Phased ship order (items/API → breaker → line pieces → integration). Hard/eof-cursor regression when atoms include blanks. |
| C2 | `debug_assert!(cursor_x - line_x ≈ broken.width)`. Page-coordinate asserts after list/column `with_offset`. |
| C4 | Parameterized `blank_em` at 12 pt and 16 pt once a font-size path exists; until then assert `n * style.font_size` via measured stroke length at default 12 pt. |

**Rejected.** C1/C4 six-method / `BlankStroke` / `blank_styled` surfaces. C2 required `\|_\| {}` and `Length<Finite>` negative offsets. `InvalidGeometry` for fill-after-run (C4). Single tape that can normalize across blanks (C1/C4 risk). Cell/Band/Layer blanks. Reverse-word defaults (none taken).

Autonomy defaults from `frame.md` (all kept): thickness 1 pt, offset 1 pt below baseline, color `style.color`, NonNegative span, visual-line `W'`, break-before-blank, omit zero stroke, layout-time em, dedicated `blank_to_end`, Cell blank out.

## Public API

```rust
impl Paragraph {
    pub fn blank(&mut self, width: Length<NonNegative>) -> Blank<'_>;
    pub fn blank_em(&mut self, ems: u32) -> Blank<'_>;
    pub fn blank_to_end(&mut self) -> Blank<'_>;
}

pub struct Blank<'a> { /* paragraph + item index */ }

impl Blank<'_> {
    pub fn thickness(&mut self, thickness: Length<Positive>) -> &mut Self;
    pub fn color(&mut self, color: Color) -> &mut Self;
    pub fn baseline_offset(&mut self, offset: Length<NonNegative>) -> &mut Self;
}
```

Ignore the return value for defaults. Re-export `Blank` next to `Paragraph` from `lib.rs`.

Assemble error after a fill-to-end blank with a later non-empty item: `Error::RunAfterFillBlank` (display: `a fill-to-end blank must be the last run`). Empty `text`/`bold` stay no-ops. `Flow::paragraph` `push(Err)` before any `TextNode`.

## Data shape

```text
InlineItem = Text(TextRun) | Blank(InlineBlank)
BlankWidth = Fixed(Length<NonNegative>) | Em(u32) | ToEndOfLine
InlineBlank { width, thickness, color: Option<Color>, baseline_offset }
LineBox { pieces: Vec<LinePiece> }
LinePiece = Glyphs(GlyphRun) | Stroke(InlineStroke { x0, x1, y, color, thickness })
```

`Em` resolves at layout to `n as f32 * style.font_size.as_pt()` before the breaker sees Fixed. Color resolves at place as `color.unwrap_or(style.color)`.

## Layout algorithm

1. **Assemble.** Coalesce adjacent same-face Text. Blank breaks coalesce. Terminal fill seals later non-empty items.
2. **Measure.** Split items into text groups separated by blanks. Each group gets its own `cluster_tagged` / measure tape. Blanks never enter cmap. Bold lookup only if a group has non-empty Bold text. Body `line_metrics` only for height/baseline.
3. **Break.** `break_spans` projects Group clusters to Hard/Ink and blanks to Blank atoms. `Hard` stores `(group, cluster)`. Break always allowed immediately before a fixed blank. Blank is not `PairClass::Sp`. After blank, pair table looks through blank between neighboring Ink when needed. Fill resolves in `pack` as `avail - line_width`, sets line width to `avail`. Zero remain omits stroke later. Overflow uses `UnbreakableWiderThanFrame` with visual-line `avail` (`W'` on first record). Keep pure-text `break_lines` facade for existing tests.
4. **Place.** `line_x` from indent/align. Walk `LinePiece` order, advancing `cursor_x` through glyph advances and blank widths. Record strokes at `y = baseline + offset` for `x1 > x0`.
5. **Fragment.** `fragment_from_lines` emits Glyphs and Stroke. Height stays `n * line_height`. Paginate slices whole `LineBox`es only.

## Out of knife

Cell/Band/Layer blanks, named styles, text decoration underline, overlay on blank, dash/double, §8 links, `Cell::spacer`, flow canvas.

## Tests (behavior)

RecordingCanvas literals for stroke endpoints and glyph x after a middle blank. Line height equals a no-blank control. Assemble fill-after-run → `RunAfterFillBlank`, no `StartPage`. Fixed blank `W' < w ≤ W` at line-0 start → `UnbreakableWiderThanFrame` with `avail ≈ W'`. Fill last line to `W`. List/column page coords after offset. backend-krilla `%PDF` only. Preserve existing `break_lines` expectations via facade.

## Ship order

1. Types + DSL + `RunAfterFillBlank`
2. `break_spans` + Hard cursor + blank atoms
3. `layout_runs` / `LineBox` pieces / fragment strokes
4. Integration tests + gates
