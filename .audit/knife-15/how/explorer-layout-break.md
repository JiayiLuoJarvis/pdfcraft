# Explorer: paragraph break, LineBox, FragmentCommand, link hit-rect insertion

Baseline: `feat/inline-blank` @ `a232ce1`. Angle: `TextNode::split` / `layout_runs` → `break_spans` / `pack_lines` → `LineBox` / `LinePiece` → `fragment_from_lines` → `draw_fragment` → `paint_pages`. Question: where a per-visual-line link hit-rect must be computed and emitted so paint can call `Canvas::annotate_link`.

`FragmentCommand` has no `Link` variant. Layout never calls `annotate_link`. Blank is the existing non-glyph `LinePiece`: a horizontal `Stroke`.

Design-draft link geometry (`docs/quire-design.md` L1310–1316, PR 16 at L1661–1666) is **not** confirmed by `docs/提案书.md`. §2 lists `link` as an attribute (L158) and has no closed link subsection. Code facts below are what the tree does today. The draft is cited only as the stated target shape for the rect.

## Components Found

### Entry

`FlowNode::split` (`src/layout/flow.rs` L154–170):

- `FlowNode::Text` → `TextNode::split(ctx, avail)` (L160).
- `FlowNode::Paragraph` → `ParagraphNode::split(avail)` (L159). No fonts, no re-break.

`TextNode::split` (`src/layout/paragraph.rs` L116–118):

```text
ParagraphNode::layout_runs(ctx.fonts, &self.items, &self.style, avail.width)?.split(avail)
```

Comment L115: every outcome hands back a laid-out `Paragraph`, so a page move never wraps twice. `Partial` / `MoveToNextPage` tails are `FlowNode::Paragraph` (`paragraph.rs` L238–247, L220).

`TextNode` (`L91–94`) holds `Vec<InlineItem>` + `Style`. Constructors:

- `new` (`L97–105`): empty string → empty `items`; otherwise one `InlineItem::Text` / `Face::Body`.
- `from_runs` (`L107–109`): every run becomes `InlineItem::Text`.
- `from_items` (`L111–113`): caller-supplied items.

`ParagraphNode::layout` (`L131–143`) is the single-string test entry. Same empty-vs-one-Body-item rule, then `layout_runs`.

### Inline items (no link)

`InlineItem` (`paragraph.rs` L37–40): `Text(TextRun)` | `Blank(InlineBlank)`. No link arm.

`TextRun` (`L22–25`): `text: String`, `face: Face`. `Face` (`L16–19`) is `Body | Bold` only.

`BlankWidth` (`L43–47`): `Fixed(Length<NonNegative>)` | `Em(u16)` | `ToEndOfLine`.

`InlineBlank` (`L50–55`): `width`, `thickness` (default 1 pt, `L60`), `color: Option<Color>` (`None` means paragraph color), `baseline_offset` (default 1 pt, `L63`).

DSL assembly (`src/document.rs` `Paragraph`, L779–873) only pushes those two item kinds. `append` (`L828–843`) drops empty strings and coalesces adjacent same-`Face` text into one `TextRun`. Different faces stay separate runs. A blank does not coalesce. `sealed` (`L861–866`) is “last item is `BlankWidth::ToEndOfLine`”; a later non-empty run sets `Error::RunAfterFillBlank` (`L868–871`, variant `src/error.rs` L81). That check is assembly-time. `break_spans` repeats it (`line_break.rs` L166–177) if a `Span::Blank { advance: Advance::Fill }` is not the last span.

`LinkTarget` (`src/canvas.rs` L33–50) and `Error::EmptyLink` (`src/error.rs` L32, display L134) exist. Nothing in `document.rs` or `layout/` constructs a `LinkTarget`.

### Measure groups (glyph tape split only by blanks)

`prepare_inline` (`paragraph.rs` L371–393) walks items:

- Consecutive `Text` runs are buffered.
- A `Blank` flushes the buffer, then pushes `Span::Blank { advance, payload: BlankDraw }`.
- Trailing text is flushed.

`flush_text` (`L395–410`) calls `measure_inline` once per contiguous text group and pushes `Span::Group(groups.len())`. One `MeasuredInline` per group. Groups are separated only by blanks. Body and Bold inside one group stay one tape.

`BlankDraw` (`L360–364`) copies thickness, color, baseline_offset. It is `Copy` because `break_spans<T: Copy>` (`line_break.rs` L159).

`blank_advance` (`paragraph.rs` L412–424):

- `Fixed` → `Advance::Fixed(length.as_pt())`.
- `Em(n)` → `Advance::Fixed(n as f32 * font_size)`. Non-finite or `< 0` → `InvalidGeometry`.
- `ToEndOfLine` → `Advance::Fill`. Width is not known until `pack_lines`.

`measure_inline` (`L491–534`):

1. `cluster_tagged` (`src/text/cluster.rs` L131–158) over `(face, text)` pairs. CRLF across runs becomes one `\n` (`normalize_tagged` L192–214; test L249–254). BK scalars are omitted from `glyph_text` and from `glyph_tags`.
2. Bold `FontId` is resolved only if some non-empty run has `Face::Bold` (`paragraph.rs` L493–500). Otherwise bold is `None`, and a Bold tag would be `Error::UnknownFont { id: 1 }` (`L506`).
3. Contiguous same-`FontId` slices of `glyph_text` go through `measure_text` (`src/text/measure.rs` L41–98). Missing cmap → `Error::MissingGlyph`. Advance is unscaled skrifa width / `units_per_em` (`advance_norm`). No kerning.
4. Flattened `InlineGlyph` tape (`paragraph.rs` L352–357): `font`, `glyph_id`, `advance_norm`, `byte_range` into that group’s `glyph_text`. Each group’s glyph indices start at 0. A blank is not a hole in this tape; it is the absence of a group boundary’s glyphs.

`cluster_width_pt` (`L536–545`) sums `advance_norm * font_size` over a glyph index range. The same tape is what `glyph_runs_for_line` draws. Measure and paint share glyph ids.

### Breaker: spans, not `BrokenLine`

Paragraph layout does **not** call `break_lines` / `BrokenLine`. It calls `break_spans` (`paragraph.rs` L167–177) and iterates `BrokenSpans<BlankDraw>` (`line_break.rs` L77–79, L499–514).

`break_lines` (`L123–157`) is a one-group facade: it builds `[Span::Group(0)]`, calls `break_spans`, then `facade_record` (`L471–497`) collapses each `SpanLine` to one `LineRecord { start, end, width }`. `BrokenLine::glyph_range` (`L562–564`) is one half-open glyph interval. A middle blank cannot be represented there: `facade_record` keeps only the first `Seg::Glyphs` and `debug_assert`s there is no second. `BrokenLine` is unused by `paragraph.rs`.

`Span<T>` (`L18–21`): `Group(usize)` | `Blank { advance: Advance, payload: T }`.

`Advance` (`L12–15`): `Fixed(f32)` | `Fill`.

`Seg<T>` (`L24–34`):

- `Glyphs { group, start: ClusterCursor, end: ClusterCursor }`.
- `Blank { width: f32, payload: T }`.

`SpanLine<T>` (`L82–86`): `segs: Vec<Seg<T>>`, `width: f32`, `empty_at: Option<(usize, ClusterCursor)>`. Accessors `segs` / `width` / `empty_at` (`L517–528`). `empty_at` is set only for an empty emit (`emit_line` L396–401) and is not read by `place_line`.

`ClusterCursor` (`cluster.rs` L36–40): `cluster`, `glyph`, `glyph_byte`. `Seg::Glyphs` stores cursors, not a `Range`. `place_line` uses `start.glyph..end.glyph` (`paragraph.rs` L442).

`LineBudget` (`line_break.rs` L89–92`): `first`, `rest`. `for_line` (`L102–108`): index 0 uses `first`, later lines use `rest`. `validate` (`L110–119`): both must be finite and `> 0`.

`layout_runs` passes (`paragraph.rs` L155–173):

```text
indent = style.first_line_indent.as_pt()
first_width = width - indent          // W'
LineBudget { first: first_width, rest: width }
width_of = cluster_width_pt(group, cluster.glyph_range(), font_size)
```

`width <= 0` or non-finite, or `first_width <= 0` / non-finite, is `InvalidGeometry` **before** measure and break (`L151–159`). `indent >= W` is not `UnbreakableWiderThanFrame`.

### `pack_lines` atom stream

`project_atoms` (`line_break.rs` L187–238):

- `Span::Group`: one atom per cluster. `BreakClass::Hard` → `Atom::Hard { group, cluster }` (no width). `BreakClass::Pair` → `Atom::Ink { pair, width, group, cluster }`. Non-finite or negative cluster width → `InvalidGeometry`.
- `Span::Blank`: one `Atom::Blank`. `Fixed` width validated finite and `>= 0`. `Fill` is stored with `width: 0.0` until pack time. Payload is copied onto the atom.

`pack_lines` (`L240–327`):

- At the start of a line (`line_start == i`), an `Atom::Ink { pair: PairClass::Sp }` is skipped (`L252–261`). Those glyphs never enter a `Seg`. Only ASCII space is `Sp` (`break_class.rs` L57–59`). Ideographic space U+3000 is `Id` and is kept (test `line_break.rs` L678–683).
- `Atom::Hard`: `emit_line(line_start, i)` **excluding** the hard atom, then start the next line (`L265–269`). Hard breaks are not inside a line (`emit_line` L408 `unreachable`).
- `Atom::Blank { Fill }`: `remain = avail - line_width`; if `remain > 0` write that into the atom’s `width`, else `0`; set `line_width = avail`; advance (`L271–286`). Does not close the line by itself. The following end-of-input `emit_line` includes it. Because fill must be the last span, nothing follows.
- Other ink or fixed blank: if `line_width + width <= avail`, consume it. Else `nearest_break` (`L294–310`). No legal split → `Error::UnbreakableWiderThanFrame { width: line_width + width, avail }` where `avail` is **this** line’s budget (`for_line`), so a first-line overflow reports `W'` (test L773–790, blank test L881–902).
- After the loop: if atoms remain, emit them; else emit one empty line (`L315–325`). An empty item list, a trailing hard break, and consecutive hard breaks therefore still produce a `SpanLine`. `break_lines("")` special-cases one empty record (`L130–143`); `break_spans` gets the same shape from the else branch.

`nearest_break` (`L329–344`), scanning backward from the overflowing atom:

- `is_fixed_blank` (`L365–373`): a `Advance::Fixed` blank is always a legal split **before** that blank. Fill is not `is_fixed_blank`.
- Otherwise the candidate must be ink, and `left_ink` (`L352–363`) walks back skipping blanks. `allows(prev, next)` (`break_class.rs` L27–40) or a line-start left side allows the break. A hard atom on the left is `LeftInk::Blocked` (the hard already closed the previous line, so this is defensive).
- Blanks are transparent to the pair table. Test `blank_is_transparent_to_a_forbidden_pair` (`line_break.rs` L1013–1028): `：` + fixed blank + `。` still refuses a break between `：` and `。` (CL forbids break before), so the blank moves with `。`.

`emit_line` (`L389–443`) is the segment tape. It merges adjacent `Atom::Ink` of the **same group** into one `Seg::Glyphs` from the first cluster’s `start_cursor` to the last cluster’s `end_cursor`. A different group, a blank, or end-of-line closes the glyph seg. Each blank atom becomes its own `Seg::Blank { width, payload }` with the packed width (fill already resolved). `SpanLine.width` is the committed sum passed in, not recomputed from segs. For a fill line that sum is the budget (`L285`).

There is no second side array. The `Seg` list **is** the tape that used to be missing from `BrokenLine::glyph_range`. Payload rides on `Seg::Blank`. `Seg::Glyphs` has no payload.

### Line geometry

`line_metrics` (`measure.rs` L100–121), called with `fonts.metrics(style.font)` — the **body** face only (`paragraph.rs` L178–180):

```text
scale = font_size / units_per_em
ascent = face.ascent * scale
descent = face.descent * scale          // fixture descent is negative (test L180–187)
leading = face.leading * scale
natural = ascent - descent + leading
line_height = natural.max(font_size * factor)
baseline = ascent                        // not vertically centered in the line box
```

`factor` non-finite or `<= 0` → `InvalidGeometry` (also rejected in `Style::new`, `style.rs` L37–38). Bold metrics do not affect `line_height` or `baseline`.

`layout_runs` then (`paragraph.rs` L182–195), for visual line `index`:

```text
frame_width = index == 0 ? first_width : width     // W' vs W
frame_x     = index == 0 ? indent      : 0
place_line(..., frame_width, frame_x, metrics.baseline, font_size)
```

`ParagraphNode` (`L123–128`) stores `lines`, `line_height`, `spacing_after = style.paragraph_spacing.as_pt()`, `width` = full `W` (not `W'`).

`place_line` (`L426–478`):

```text
line_x = frame_x + align_x(frame_width, line.width(), style.align)
x = line_x
```

`align_x` (`L636–642`):

| `TextAlign` | added x |
| --- | --- |
| `Start` | `0` |
| `Center` | `(frame_width - line_width) / 2` |
| `End` | `frame_width - line_width` |

First line, end-aligned: `indent + (W' - line_width) = W - line_width`. Indent cancels. Pinned by `first_line_indent_uses_the_inset_alignment_box` (`paginate.rs` L356–397): Start → `indent`, Center → `indent + (W' - line_width) / 2`, End → `W - line_width`. Glyph `origin.y` on the `LineBox` is `baseline` only (page margins not yet applied; that test’s margins are zero).

Per seg (`place_line` L439–464):

- `Seg::Glyphs`: skip empty `start.glyph..end.glyph`. Else `cluster_width_pt` for the whole range, `glyph_runs_for_line` at current `x`, push `LinePiece::Glyphs` for each run, then `x += advance`.
- `Seg::Blank`: if `width > 0`, push `LinePiece::Stroke(InlineStroke { x0: x, x1: x + width, y: baseline + payload.baseline_offset, color: payload.color.unwrap_or(style.color), thickness })`. Always `x += width`, including zero. A zero-width fill (line already full; test `line_break.rs` L948–964) produces **no** stroke.

If no glyph seg was emitted, insert `LinePiece::Glyphs(empty_glyph_run)` at index 0 (`paragraph.rs` L466–470, `L480–489`): body font, empty glyphs, empty text, origin `(line_x, baseline)`. A blank-only line still has a glyph command so later baseline probes see a y. Strokes stay after that inserted run.

`debug_assert` (`L472–476`): final `x - line_x` equals `line.width()`.

`glyph_runs_for_line` (`L547–601`) splits a glyph range only where `InlineGlyph.font` changes. Each piece is a `GlyphRun` (`canvas.rs` L119–130): `origin` is the baseline of the **first** glyph of that piece (`L116`), `advance_norm` is **not** multiplied by font size (`L104–108`), `text` is the group’s `glyph_text` byte slice, `GlyphPlacement.byte_range` is rebased to that slice (`paragraph.rs` L576–584`). User-space width is `advance_norm * font_size`. Color is `style.color` for every run; face does not change color.

`LineBox` (`L70–72`) is `pieces: Vec<LinePiece>` only. No line rect, no baseline field, no link. Coordinates are relative to the paragraph fragment origin, but **y is not yet stacked**: every line’s glyph origin y is the same `baseline`, and stroke y is `baseline + offset`. Line index is applied later.

`LinePiece` (`L75–78`): `Glyphs(GlyphRun)` | `Stroke(InlineStroke)`. `InlineStroke` (`L81–87`) is private fields `x0`, `x1`, `y`, `color`, `thickness`. No `Link` piece.

`single_line_run` (`L253–281`) is a separate path (list markers, decorations). One `measure_text`, reject if `advance_norm * font_size > max_width`, then `glyph_run_for_line` which **does** call `align_x` (`L624`) and then overwrites origin x to 0 (`L279`). Callers place it themselves. It does not wrap, has no blanks, and does not go through `LineBox`.

### Fragment commands and paint

`fragment_from_lines` (`paragraph.rs` L308–343`), for line `index`:

```text
y0 = index * line_height          // lines_height, L303–306; count clamped via u16
```

- `Glyphs`: clone run, `origin.y = y0 + run.origin.y` (baseline + stacked line). x unchanged. `FragmentCommand::Glyphs`.
- `Stroke`: path is a two-point line `(x0, y0 + stroke.y)` → `(x1, y0 + stroke.y)`, `Paint::solid(color)`, `Stroke::new(thickness)`. `FragmentCommand::Stroke`.

`Fragment` (`flow.rs` L48–52`): `height` (sum of line heights, **not** including `spacing_after`), `spacing_after`, `commands`.

`FragmentCommand` (`L81–96`) variants, exhaustive:

1. `Glyphs(GlyphRun)`
2. `Fill { path, paint }`
3. `Stroke { path, paint, stroke }`
4. `Image { image, rect }`

No `Link`. `with_offset` (`L63–77`) translates glyph origins, fill/stroke paths, and image rects. A fifth variant would not compile until this match grows. Same for `draw_fragment` (`L347–376`) and `list.rs` `first_baseline` (`L258–265`), which returns the first `Glyphs` origin y and maps Fill/Stroke/Image to `None`.

`ParagraphNode::split` (`paragraph.rs` L206–249`):

- Stored `width > avail.width` → `InvalidGeometry` (L207–209). Does not re-wrap.
- `lines.is_empty()` → `Split::Fit` height 0, empty fragment (L210–215). Unreachable from `pack_lines`, which always emits ≥ 1 line. Reachable only if a `ParagraphNode` is built with an empty vec.
- `line_height > avail.full_height` → `UnsplittableTallerThanFrame` (L216–218).
- `avail.height < line_height` → `MoveToNextPage(FlowNode::Paragraph(self))` (L219–221). The whole laid-out node moves. Indent and alignment stay baked in piece x.
- `lines_fitting` (`L283–301`): largest `n` with `n * line_height <= avail.height`, at least 1 if `avail.height >= line_height`, capped at `u16::MAX`.
- All lines fit: `fragment_from_lines(&lines, line_height, spacing_after)`, `Split::Fit { height: n * line_height }` (L228–231). `height` excludes spacing.
- Else `split_off`: head fragment has `spacing_after: 0.0` (L237); tail `ParagraphNode` keeps `line_height`, `spacing_after`, `width`, and the remaining `LineBox`es (L238–247). Tail lines are reindexed from 0 by the next `fragment_from_lines`, so their fragment-local y starts over. Their x (indent on original line 0 only) is already in the pieces.

`draw_fragment` (`flow.rs` L347–376`) adds `PlacedFragment.origin` (`L99–103`) to glyph origins and path translations, then:

- `canvas.draw_glyph_run`
- `canvas.fill_path`
- `canvas.stroke_path`
- `canvas.draw_image`

No `canvas.annotate_link`.

`paint_pages` (`paginate.rs` L266–279`): per page, `start_page`, then `draw_fragment` for `behind`, `header`, `page.placed`, `footer`, `front`, then `finish_page`. It does not match `FragmentCommand`. A new command reaches the canvas only if `draw_fragment` handles it.

Page placement of body fragments (`plan_pages` L155–160, L175–181): origin is `(body_origin.x, body_origin.y + y)` where `body_origin` is `(margin.left, margin.top)` (`L29–30`). `y` advances by fit `height`, then by `spacing_after` only when the remainder of the body height can hold it (`L161–166`). Partial heads are placed at the current `y` and the page is committed (`L170–185`). `MoveToNextPage` at `y == 0` is `UnsplittableTallerThanFrame` (`L187–189`); otherwise the node is retried on a fresh page.

`with_offset` call sites that shift fragment-local coordinates before that page origin:

- Stack (`flow.rs` L271, L289): `dy = y` accumulated in the stack.
- Columns (`columns.rs` L164): `dx = x` of the column.
- List body (`list.rs` L215): `(marker indent, y)`.
- Table cell content (`table.rs` L259): `(x + inset, y + inset)`; block stack inside the cell (`L365`): `(0, y)`.

`single_line_run` consumers (`list.rs` L207–212 markers, `decorate.rs` L201) never build a `LineBox` and never call `fragment_from_lines`.

### Canvas / backend (already able to record a rect; layout never feeds one)

`Canvas::annotate_link(&mut self, rect: Rect, target: LinkTarget)` (`canvas.rs` L207–210). `LinkTarget::uri` rejects only `""` (`L39–43`); spaces are kept. `Rect::try_pt` (`geom.rs` L379–385`) requires a finite origin and a **strictly positive** width and height (`Size`, L209–215). A zero-width segment cannot be a `Rect`.

`RecordingCanvas` stores `CanvasCommand::AnnotateLink { rect, target }` (`canvas.rs` L173–179, impl L353–355`) in call order.

`KrillaCanvas::annotate_link` (`krilla_canvas.rs` L306–309`) only pushes `QueuedLink { rect, target }` (`L88–93`) onto the open page. It does not call krilla. `finish_page` (`L236–248`) moves `links` onto `PagePlan` beside `marks`. `emit_page` (`emit.rs` L89–112`) draws every mark inside the single `surface()` borrow, `surface.finish()`, **then** `page.add_annotation` for each link (`L108–110`). `to_annotation` (`L199–207`) maps the rect through `to_krilla_rect` and `Target::Action(LinkAction::new(uri))`. The observable test (`L254–278`) shows the public top-left y-down rect `(10, 30, 80, 44)` on a 200-pt page becomes PDF `/Rect [10 126 90 170]` because krilla’s page transform flips y. Layout must pass the same top-left y-down space as glyph origins. It must not pre-flip.

Call order inside `draw_fragment` would show up on `RecordingCanvas`. On the krilla backend, annotations are always written after the content stream, regardless of interleaving with `draw_glyph_run`.

## Flow

```text
Document::Paragraph items
  InlineItem::Text(TextRun { text, face }) | InlineItem::Blank(InlineBlank)
  (same-face text already coalesced; fill-blank seals the paragraph)
        │
        ▼
TextNode::split(ctx, frame)
  layout_runs(fonts, items, style, W = frame.width)
        │
        ├─ reject W <= 0 or W' = W - indent <= 0 → InvalidGeometry
        ├─ prepare_inline
        │     text groups between blanks → cluster_tagged → measure_text
        │     → MeasuredInline glyph tape (indices local to the group)
        │     blanks → Span::Blank { Advance::Fixed | Fill, BlankDraw }
        │     text   → Span::Group(i)
        ├─ break_spans(groups, spans, LineBudget { first: W', rest: W }, cluster_width_pt)
        │     project_atoms: cluster → Hard | Ink; blank → Blank atom
        │     pack_lines:
        │       drop leading Sp
        │       hard closes a line (BK not in the seg list)
        │       Fill writes remain into the atom and sets line width = budget
        │       overflow → nearest_break (fixed blank, or pair table; blanks transparent)
        │       else UnbreakableWiderThanFrame { avail: this line's W or W' }
        │     emit_line → SpanLine { segs: Glyphs cursors | Blank { width, BlankDraw }, width }
        ├─ line_metrics(body face) → line_height, baseline = ascent
        └─ for each visual line:
              place_line
                line_x = frame_x + align_x(frame_width, line.width)
                walk segs, x cursor
                  Glyphs → one or more LinePiece::Glyphs (split on FontId only)
                  Blank width > 0 → LinePiece::Stroke at baseline + offset
                blank-only / empty → empty GlyphRun inserted at front
        │
        ▼
ParagraphNode { lines: Vec<LineBox>, line_height, spacing_after, width: W }
  split(avail) slices whole lines; does not re-break
  fragment_from_lines
    y0 = index * line_height
    Glyphs origin.y += y0
    Stroke path y = y0 + stroke.y
    → Fragment { commands: Glyphs | Stroke, height: n * line_height, spacing_after }
        │
        ▼
FlowNode parents with_offset(dx, dy)     // stack, columns, list, table
paginate plan_pages places origin at (margin.left, margin.top + y)
paint_pages → draw_fragment(origin)
  Glyphs → Canvas::draw_glyph_run
  Stroke → Canvas::stroke_path
  (no annotate_link)
        │
        ▼
KrillaCanvas: glyphs/strokes are marks; annotate_link would queue QueuedLink
emit_page: surface marks, drop surface, then add_annotation
```

Empty `items` still become one empty `SpanLine` (`pack_lines` L323–325) and one `LineBox` with an empty `GlyphRun`. `fragment_from_lines` gives that paragraph `height = line_height`, not 0.

## Files Read

- `src/layout/paragraph.rs` (full): items, `layout_runs`, `place_line`, `fragment_from_lines`, `align_x`, blank stroke, tests L644–689.
- `src/layout/flow.rs`: `Fragment`, `FragmentCommand`, `with_offset`, `Split`, `draw_fragment`, stack offset.
- `src/layout/paginate.rs` L1–280 and test L356–397: page origin, `paint_pages`, first-line indent x.
- `src/text/line_break.rs` L1–573 and blank tests L809–1028: `break_spans`, `pack_lines`, `emit_line`, `BrokenLine` facade.
- `src/text/cluster.rs` L1–214: `ClusterCursor`, `cluster_tagged`, BK omitted from glyph tape.
- `src/text/measure.rs` L1–121: `measure_text`, `line_metrics`.
- `src/text/break_class.rs` L1–75: `Sp` / pair table.
- `src/layout/style.rs` L1–50: `TextAlign`, `first_line_indent`.
- `src/canvas.rs` L30–50, L99–210: `GlyphRun`, `LinkTarget`, `AnnotateLink`, `annotate_link`.
- `src/geom.rs` L171–215, L368–410: `Point` may be negative; `Rect` / `Size` require positive extent.
- `src/backend/krilla_canvas.rs` L80–93, L236–310: `QueuedLink`, `annotate_link`.
- `src/backend/emit.rs` L89–112, L199–207, L254–278: annotation after surface; y flip is backend-only.
- `src/document.rs` L779–912: paragraph items and fill-blank seal. No link builder.
- `src/error.rs` L31–32, L81: `EmptyLink`, `RunAfterFillBlank`.
- `src/layout/list.rs` L207–265: marker `single_line_run`, `with_offset(indent, y)`, `first_baseline` match.
- `src/layout/columns.rs` L145–166: `with_offset(x, 0)`.
- `src/layout/table.rs` L259, L365: cell `with_offset`.
- `docs/提案书.md` L192–206 (`W` / `W'`), L241–268 (blank rules). Link is listed at L158 and not closed.
- `docs/quire-design.md` L1310–1316 and L1661–1666 (unconfirmed target: one rect per visual line segment, `FragmentCommand::Link`, `annotate_link`).

## Boundaries

Layout must not `use krilla`. `annotate_link` is already the canvas boundary. `draw_fragment` is the only paint function `paint_pages` uses for body, header, footer, behind, and front. Emitting a fragment-local rect from `fragment_from_lines` and translating it like an image rect is enough for pagination to call `annotate_link` in page space: `with_offset` first (parent dx/dy), then `draw_fragment` adds `PlacedFragment.origin`.

Do not compute the rect in `pack_lines`. That layer knows the visual-line split and the seg width in the line’s content order, and it does not know `frame_x`, `align_x`, baseline, or `y0`.

Do not compute it in `paint_pages` or the backend. `GlyphRun` has no URI. `glyph_runs_for_line` may split one `Seg::Glyphs` into several commands when the font changes, so one link segment is not one `Glyphs` command. `KrillaCanvas::annotate_link` only queues a finished `Rect`.

`Rect` cannot represent a zero advance. `place_line` already skips empty glyph ranges (`L443–445`) and skips zero-width strokes (`L453`). A hit-rect of width 0 must be omitted, not forced through `Rect::try_pt`.

`single_line_run` (markers, decoration text) is outside this break path. A link implemented only in `place_line` / `fragment_from_lines` will not cover those runs.

Image blocks have no link field in `src/layout/image.rs`. The design draft’s image `link` (`quire-design.md` L1022, L1314) is a different rect (the image rect), not a line-segment rect.

## Non-Obvious Things

1. **The segment tape replaced `BrokenLine` for paragraphs.** `Seg::Blank` carries `BlankDraw` by value. `Seg::Glyphs` carries only `(group, start cursor, end cursor)`. Link identity has nowhere to sit unless groups are split on link boundaries and the URI is stored on `MeasuredInline`, or glyph atoms grow a payload the way blanks did. Coalescing in `Paragraph::append` (`document.rs` L836–840) and again in `prepare_inline` (`paragraph.rs` L377) merges same-face text, and `prepare_inline` also merges different faces into one group. A URI on a `TextRun` would be erased by either merge. `Face` survives only as `glyph_tags` inside the group, and those tags are consumed when choosing `FontId`; they are not on `LinePiece`.

2. **One rect per visual segment, not per `GlyphRun` and not per paragraph.** `emit_line` cuts a group at line breaks, so one link group that wraps becomes one `Seg::Glyphs` per line (partial cursors, same `group`). `place_line` then advances `x` by `cluster_width_pt` of that whole seg before the next seg (`L446–449`), even when `glyph_runs_for_line` emits two runs for a font change. The hit width is that seg advance. The hit x starts at the cursor **after** `line_x = frame_x + align_x(...)`. A bold/body change inside the link must not split the rect. A link next to unlinked text in the same group cannot be recovered after font-only splitting.

3. **Horizontal position is baked in `place_line`; vertical line-box position is baked in `fragment_from_lines`.** Glyph and stroke y on the `LineBox` are baseline-relative and identical for every line. Stacked top of line `i` is `y0 = i * line_height`. Design-draft hit rect (`quire-design.md` L1312): height = line box height, width = that segment’s advance on that line. That height is `metrics.line_height`, not the ink bbox and not “baseline to next baseline” measured from the glyph origin. Glyph origin y is `y0 + ascent`, so a rect whose top is the glyph origin misses the ascent and starts at the baseline. `baseline` stays `ascent` even when `line_height` is raised by `font_size * factor` (`measure.rs` L114–119); the extra space is the part of the line box below the natural em box. Body metrics only; a bold run does not change the rect height.

4. **First-line indent and alignment move the x cursor before any seg is placed.** `frame_x` is `indent` only when `index == 0` of the **original** wrap. Later visual lines use `frame_x = 0` and `frame_width = W`. End alignment on line 0 lands at `W - line_width` (indent cancels). Center uses `W'`, not `W`. A fill blank sets `SpanLine.width` to the budget (`line_break.rs` L285, test L929–944), so `align_x` is 0 for Start, Center, and End. The centered fill test (`paragraph.rs` L652–689) pins glyph x at 0 and stroke end at the frame width 120. A link on a fill line is not centered in the ink; it sits at `frame_x` plus preceding advances. `place_line` must record x0/x1 while walking segs. Re-deriving x from `GlyphRun.origin` after the fact misses any span that is not its own glyph run, and misses the fill interaction unless the URI is already on the command.

5. **Blank stroke is not a line-height contributor and is not a glyph.** y = `baseline + baseline_offset` (default +1 pt, downward), then `+ y0`. Width 0 omits the stroke but still consumed advance at pack time (the line width is already the budget). Color defaults to `style.color`. Fixed blanks are atomic (`UnbreakableWiderThanFrame` against the current line budget, not clipped). They are also break opportunities **before** the blank, so a fixed blank can sit alone on the next line (test `line_break.rs` L853–877) and that line’s stroke is the whole blank width, shifted by that line’s `align_x`. Leading spaces are dropped only at `line_start` and never become segs; a link made only of those spaces has no seg on that line.

6. **Page splits do not re-run `place_line`.** Head/tail reuse `LineBox`es. `fragment_from_lines` on the tail starts `index` at 0, so fragment-local y resets, then `plan_pages` adds the next page’s body origin. Hit rects must be fragment-local, same as strokes. Baking page coordinates in `place_line` would double-add margins and would ignore `with_offset` from stack, columns (`dx`), list (`indent`), and table insets. `Split::Fit.height` and `Fragment.height` exclude `paragraph_spacing`; spacing only moves the next block’s y in `plan_pages` / stack when it fits. A link rect uses the line box, not the spacing gap.

7. **Empty glyph run on a line with no ink.** Inserted at piece index 0 (`paragraph.rs` L466–470). `first_baseline` in lists finds that y and ignores strokes. A link command that is not `Glyphs` will not move marker alignment. Match arms that must grow for any new variant: `Fragment::with_offset`, `draw_fragment`, `list.rs` `first_baseline`.

8. **Backend y-flip is not a layout concern.** `emit.rs` L254–260: annotation rects are stored in canvas space and written already transformed. `draw_fragment` should pass the same coordinates it uses for glyph origins.

## Open Questions

- 提案书 has not closed link (no §2 subsection; design draft L1312 is unconfirmed). Whether v1 is URI-only (`LinkTarget::uri`, no `linkToPage` / destination) is a product choice. The canvas and krilla adapter already implement URI annotations only (`emit.rs` L203–206).
- Whether a link is a run boundary (its own `MeasuredInline` / `Span::Group`, URI beside the group) or a per-scalar tag inside a group. The blank implementation forces the first for non-glyph items. A link is glyph-bearing, but merging it into a neighboring text group loses the x range once `glyph_runs_for_line` cuts only on `FontId`.
- A blank **inside** a linked span: today a blank always ends a text group. That would emit two glyph segs and a stroke. Unspecified whether the hit rect covers the stroke’s width, splits into two rects, or is illegal.
- Zero-width cases: dropped leading spaces, empty glyph range, fill blank of width 0. `Rect` cannot be built. Omit versus error is unspecified. `LinkTarget::uri("")` is already `EmptyLink` at the canvas boundary; layout has no call site.
- Command order: stroke and glyphs are emitted in seg order. `annotate_link` does not paint ink. `RecordingCanvas` would record it where `draw_fragment` calls it; krilla writes every link after the content stream anyway.
- `FragmentCommand::Link` is the design-draft name (L1312) and does not exist. Image rects are the only command that already stores a `Rect` and is translated by `with_offset` (`flow.rs` L73). A link rect wants that same translation, plus `draw_fragment` calling `annotate_link` instead of `draw_image`.
