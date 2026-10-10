# Explorer: block HLine stroke vs inline blank

Baseline: `feat/style-builders` @ `5deb0df`. Authority: `docs/提案书.md` §2.2 (L241–268) and §2.3 (L270–285). Angle: stroke drawing precedent only. Paragraph run accumulation and `break_lines` internals not traced.

No inline blank / underscore type exists. `TextRun` is `{ text, face }` only (`src/layout/paragraph.rs` L21–34). Paragraph fragments emit `FragmentCommand::Glyphs` only.

## Components Found

### Public builder (`src/document.rs`)

`HLine` (L494–569) is `Copy`. Fields:

| Field | Type | Default (`Default`, L510–520) |
| --- | --- | --- |
| `width` | `Option<Length<Positive>>` | `None` = full frame at split |
| `align` | `TextAlign` | `Center` |
| `thickness` | `Length<Positive>` | `Positive::pt(1.0)` |
| `color` | `Color` | `Color::rgb(0, 0, 0)` (alpha 255) |
| `space_before` | `Length<NonNegative>` | `NonNegative::ZERO` |
| `space_after` | `Length<NonNegative>` | `NonNegative::ZERO` |

Mutators (all `&mut self`, no `Result`): `width` (L525), `thickness` (L530), `color` (L535, doc says alpha included), `space_before` (L540), `space_after` (L545), `start` (L550), `end` (L555). No `center()` setter; Center is only the default. No dash, double-line, or vertical-line field.

`HLine::lower` (L559–568) is infallible: `FlowNode::HLine(HLineNode::new(...))`. Width/height errors are deferred to `split`.

Entry points, all `HLine::default()` then closure then `lower`:

- `Flow::hline` (L726–733) → `self.push(Ok(line.lower()))`.
- `Cell::hline` (L951–959) → `CellContent::HLine`. `Table::lower` (L896) maps that to `vec![line.lower()]` and does **not** apply `Flow.style` (text cells do, L892).
- List items are a nested `Flow` (`List::item`, L1020–1022), so `item.hline` is `Flow::hline`. Marker column is 18 pt (`List::lower`, L1038).

Document root wraps the flow as `FlowNode::Stack` (`Section::lower`, L161).

`HLine` does not read `Flow.style` (`default_style` L591–600: font 12 pt, `line_height_factor` 1.2, `color` black, `align` **Start**). Black matches the style default by coincidence; the line stores its own `Color`.

### Layout node (`src/layout/hline.rs`)

`HLineNode` (L10–18) mirrors the builder fields. `split(self, avail: Frame) -> Result<Split>` (L43–77) takes **no** `LayoutCtx` / fonts. Dispatched from `FlowNode::split` (`src/layout/flow.rs` L168) after `avail.validate()`.

Resolution, in order:

1. **Width.** `self.width.map_or(avail.width, Length::as_pt)` (L44). `None` → `Frame.width` of whoever is splitting (body, column, list body, cell inner). `Some` → stored pt. `width > avail.width` (strict `>`, equal allowed) → `Error::UnbreakableWiderThanFrame { width, avail }` (L46–50). This check runs **before** the height checks (`wider_beats_taller_and_short_frame_moves`, L172–180).
2. **Height.** `space_before + thickness + space_after` in pt (L39–41). `height > avail.full_height` → `Error::UnsplittableTallerThanFrame` (L52–54). `height > avail.height` → `Split::MoveToNextPage(FlowNode::HLine(self))` (L55–57). Never `Split::Partial`. No shrink.
3. **X align** (L58–62), same arithmetic as private `align_x` in `paragraph.rs` L429–435 and `image.rs` L45–47, not a shared function:
   - Start → `0`
   - Center → `(avail.width - width) / 2`
   - End → `avail.width - width`
   Full width ⇒ Center x is `0`. Align only matters when the line is shorter than the frame.
4. **Y.** Centerline `y = space_before + thickness/2` (L63). Not a baseline, not glyph descent.
5. **Path.** `Path::line(Point::try_pt(x, y)?, Point::try_pt(x + width, y)?)` (L64). One `MoveTo` + one `LineTo`, both y equal. `Path::line` (`geom.rs` L625–628) does not reject zero length; HLine segment length is `width`, and both a `Positive` width and a validated frame width are `> 0`.
6. **Fit fragment** (L65–76): `height` as above, `spacing_after: 0.0` (not `Style.paragraph_spacing`), one command:

```text
FragmentCommand::Stroke {
    path,
    paint: Paint::solid(self.color),
    stroke: Stroke::new(self.thickness),
}
```

`f32` overflow: `space_before = f32::MAX` makes height non-finite and returns `UnsplittableTallerThanFrame` (`overflowing_spaces_are_taller_than_frame`, L196–208).

### Fragment → canvas → backend

`FragmentCommand` (`flow.rs` L81–96): `Glyphs(GlyphRun)` | `Fill { path, paint }` | `Stroke { path, paint, stroke }` | `Image { ... }`.

`Fragment::with_offset` (L63–77) translates a stroke `Path` (and a glyph `origin`, a fill path, an image rect). Callers:

- Columns: `with_offset(column_x, 0)` after the child split on a frame of that column’s width (`columns.rs` L145–164). HLine `None` width is the column width, then the path is shifted. Unit test (`hline.rs` L212–234): page width 200, gap 0, two strokes `(0, 0.5)–(100, 0.5)` and `(100, 0.5)–(200, 0.5)`.
- List: body frame width is `avail.width - indent`; commands then `with_offset(indent, y)` (`list.rs` L172–215). Default indent = marker width 18 pt. Integration test (`tests/document.rs` L1193–1230): start-aligned 40 pt line at `(18, 0.5)–(58, 0.5)` (default thickness 1 ⇒ y = 0.5).
- Table: cell content is `natural_height` on the inner frame (`table.rs` L217–222), then `with_offset(x + inset, y + inset)` (L259). Cell hline `None` width is the cell **content** width. Table borders are a **second** `FragmentCommand::Stroke` on a rect path (L260–266), separate from the hline command.

`draw_fragment` (`flow.rs` L347–375), called from `paint_pages` (`paginate.rs` L266–278) after `start_page`, in order behind → header → body `placed` → footer → front:

- `Glyphs`: clone run, add fragment origin to `run.origin`, `canvas.draw_glyph_run`.
- `Stroke`: `canvas.stroke_path(&path.translated(ox, oy)?, paint, stroke)` (L365–369).
- Fill and image are the other arms.

Body origin is `(body_origin.x, body_origin.y + cursor_y)` (`paginate.rs` L155–160). Cursor advances by fragment `height`, then optionally `spacing_after` if it fits (L161–167). HLine `spacing_after` is 0, so only `height` (which already includes `space_before` / `space_after`) moves the cursor. `MoveToNextPage` at `y == 0` is upgraded to `UnsplittableTallerThanFrame` (`paginate.rs` L187–189). Otherwise the node is retried on a fresh page. Integration: page 100×40, spacer 31, thickness 2 + gaps 4+4 = height 10 → stroke only on page 2, path `(0, 5)–(100, 5)`, paint black, `Stroke::new(2pt)` (`tests/document.rs` L1135–1173). Taller than the body fails before any page (L1177–1189).

`Canvas::stroke_path` (`canvas.rs` L196–197). `RecordingCanvas` logs `CanvasCommand::StrokePath { path, paint, stroke }` (L152–160, L330–337). `GlyphRun` (`canvas.rs` L114–132) is a fill-colored baseline run: `font`, `font_size`, `color`, `origin` = baseline of the first glyph, y down. No stroke field.

Backend (`feature backend-krilla`), layout never names krilla:

- `KrillaCanvas::stroke_path` queues `Mark::Stroke { path, paint, stroke }` (`krilla_canvas.rs` L261–268).
- `draw_glyph_run` queues `Mark::Glyphs` with `run.color` (`L286–302`).
- `draw_mark` (`emit.rs` L122–129): stroke sets fill `None`, stroke `Some(to_stroke)`, `surface.draw_path`. Glyphs (`L131–163`): fill `Some(to_fill(Paint::solid(color)))`, stroke `None`, `surface.draw_glyphs`. Text is never a path stroke.
- `to_stroke` (`convert.rs` L73–83): krilla paint from RGB channels, `width: stroke.width().as_pt()`, opacity `a/255`, **`..KStroke::default()`** for cap, join, miter, dash. Comment: those stay krilla defaults. No dash field exists on quire `Stroke`. Point mapping is identity, top-left y-down (`convert.rs` L1–5, L16–18).

### Paint types (`src/geom.rs`)

- `Length<C>` (L14–63): pt brand. `Positive::pt` rejects `<= 0` and non-finite (`L129–137`). `NonNegative::pt` rejects `< 0` (`L102–112`). `as_pt` is the raw `f32`.
- `Color` (L473–499): 8-bit RGBA. `rgb` forces alpha 255.
- `Paint` (L501–518): solid color only. `Paint::solid`.
- `Stroke` (L520–537): **width only**, `Length<Positive>`. `Stroke::new`. No cap, join, dash, or double-line.
- `Pen` (L539–565): `{ color, stroke: Stroke }`. **Not used by HLine.** Used by page-layer shapes.

`TextAlign` (`style.rs` L8–14): `Start | Center | End`. Crate-private. Paragraph `Style.align` defaults to Start; HLine defaults to Center. They are different fields.

### Other stroke producers (same command, different job)

`Layer::line(from, to, pen)` (`document.rs` L1217–1220) and `Shapes::line` (L1278–1283) build `Ink::Stroke(pen)`. `straight_path` (L1299–1304) rejects `from == to` with `InvalidGeometry`. `decorate::measure_layer` (L211–230) splits ink into `FragmentCommand::Stroke { paint: Paint::solid(pen.color()), stroke: Stroke::new(pen.width()) }` — same command shape HLine builds directly, without `Pen`. Layer coordinates are page coordinates, not the flow frame; height of that fragment is 0 (L238–244). This is the `behind` / `front` path §2.3 says the separator is not.

Table border stroke (`table.rs` L235–266) is a rectangle outline, width from the table border length, not an HLine.

### Paragraph glyph draw (contrast only)

`line_metrics` (`text/measure.rs` L100–120): `line_height = max(ascent - descent + leading, font_size * factor)`. `baseline = ascent`. Default factor 1.2 (`document.rs` L595). No stroke thickness in the formula.

`fragment_from_lines` (`paragraph.rs` L249–267): for each line, push `FragmentCommand::Glyphs` with `origin.y = line_index * line_height + run.origin.y`. Fragment `height = line_count * line_height`. Command list and height are independent: a `Stroke` pushed here would not change height unless `lines_height` is also changed. Today nothing pushes a stroke.

`glyph_run_for_line` (L396–427): `color: style.color`, `origin` at `(align_x(...), baseline)`. `align_x` shifts the **whole line** inside the frame. That is paragraph alignment, not HLine alignment.

`draw_fragment` glyph arm never calls `stroke_path`. Backend glyph arm clears stroke.

`first_baseline` (`list.rs` L258–265) reads the first `Glyphs` origin y and **skips** `Stroke` / `Fill` / `Image`. An item whose only command is an HLine stroke does not move the marker to the stroke y; the marker keeps `single_line_run`’s own baseline (`paragraph.rs` L193–221, origin `(0, metrics.baseline)`), then `unwrap_or(run.origin.y())` (`list.rs` L207–212).

## Flow

```text
Flow::hline / Cell::hline / list item Flow::hline
  HLine::default + closure
  HLine::lower → FlowNode::HLine(HLineNode)     // no style, no font, no error
  container split builds Frame { width, height, full_height }
    column: width = column track; later with_offset(x, 0)
    list:   width = frame - indent; later with_offset(indent, y)
    cell:   width = inner content width; later with_offset(cell + inset)
    body:   width = body width; paginate origin = body_origin + cursor y
  HLineNode::split(avail)                       // no LayoutCtx
    width  = option or avail.width;  > frame → UnbreakableWiderThanFrame
    height = before + thickness + after
            > full_height → UnsplittableTallerThanFrame
            > avail.height → MoveToNextPage (whole node)
    x from TextAlign; y = before + thickness/2
    Path::line horizontal
    Split::Fit { height, Fragment { spacing_after: 0,
        [Stroke { path, Paint::solid(color), Stroke::new(thickness) }] } }
  paginate: y += height; MoveToNextPage at y==0 → UnsplittableTallerThanFrame
  draw_fragment: path.translated(origin) → Canvas::stroke_path
  RecordingCanvas: CanvasCommand::StrokePath
  backend: Mark::Stroke → set_fill(None), set_stroke(to_stroke), draw_path
            to_stroke width + rgb + alpha; cap/join/dash = krilla default (solid)

Paragraph (separate):
  measure → line_height from font metrics × factor
  FragmentCommand::Glyphs only; height = n * line_height
  draw_glyph_run → set_fill(color), set_stroke(None), draw_glyphs
```

## Files Read

- `docs/提案书.md` L230–285 (§2.0 color/length note, §2.2, §2.3)
- `src/document.rs` L142–169, L494–569, L591–600, L726–733, L878–909, L930–960, L1012–1042, L1217–1220, L1278–1304
- `src/layout/hline.rs` (entire, L1–272)
- `src/layout/flow.rs` L23–170, L347–376
- `src/layout/paginate.rs` L140–199, L266–279
- `src/layout/columns.rs` L130–167
- `src/layout/list.rs` L172–265
- `src/layout/table.rs` L210–266
- `src/layout/lower.rs` L29–82
- `src/layout/paragraph.rs` L21–45, L99–136, L193–267, L396–435
- `src/layout/style.rs` L8–26
- `src/layout/decorate.rs` L185–246
- `src/text/measure.rs` L100–120
- `src/geom.rs` L14–151, L473–628, L690–698
- `src/canvas.rs` L99–211, L305–357
- `src/backend/krilla_canvas.rs` L252–302
- `src/backend/emit.rs` L115–169
- `src/backend/convert.rs` L1–83
- `src/error.rs` L73–81
- `tests/document.rs` L1135–1244

## Boundaries

§2.3 as implemented vs §2.2 (not implemented). “Reuse” means the stroke pipe a blank can call. “Separate” means behavior that must not be obtained by embedding `HLineNode`.

| Topic | Block HLine today | Inline blank (§2.2) |
| --- | --- | --- |
| Identity | `FlowNode::HLine`, own `split` | Inside a paragraph run list, beside `text` / `bold`. Not a flow node. `TextRun` has no width or stroke. |
| Width | `None` = **this frame’s** `W` at split; or a `Positive` length, error if `> W` | Fixed length **or** `W_remain` (mutually exclusive). Eat-to-end forbids a following run at assemble time. Fixed wider than line `W` → same error variant `UnbreakableWiderThanFrame`, but the avail width is the line budget, not a second flow frame. |
| Align | `TextAlign` on the **rule** inside the frame. Default Center. Full width ⇒ x = 0. | No rule-in-frame align. X is the run cursor after previous glyphs. Paragraph `Style.align` may still shift the **whole line** via `align_x`. Center-on-frame would place the underscore in the middle of the column, not next to neighboring text. |
| Thickness / color types | `Length<Positive>`, `Color`, `Paint::solid`, `Stroke::new` | Same types. Do not copy `HLine::default` (1 pt, black, Center, zero spaces) as the blank’s bundle. |
| Color source | Own field, default black. Ignores `Style.color`. | §2.2: default current text color **or** black. Glyph color today is `style.color` (`paragraph.rs` L422). HLine never sees that style. |
| Vertical position | Centerline at `space_before + thickness/2` inside its own box | Slightly below the glyph baseline. `space_before` is not that offset. |
| Height | `before + thickness + after` added to the flow cursor | Must not change `line_height`. Height today is only `line_metrics`. A stroke command does not affect height unless layout adds it. |
| `spacing_after` | Always 0. Vertical gaps are inside `height`, so they move with the unbreakable node and are not dropped the way trailing fragment spacing can be (`paginate.rs` L162–167). | Not paragraph spacing and not blank padding. |
| Pagination | Atomic. `MoveToNextPage` or hard taller-than-frame. Never `Partial`. | Crosses pages only with its line (paragraph already splits by whole lines). |
| Fonts | `split` has no `LayoutCtx`. No “N × font size”. | §2.2 sugar is `N * current font size`. That value exists on `Style.font_size` during paragraph measure, not on `HLineNode`. |
| Draw | One horizontal `FragmentCommand::Stroke` → `stroke_path` | Same command and the same canvas/backend stroke functions. One stroke per placed blank. |
| Dash / double / vertical | Not representable. `Stroke` is width-only; backend keeps krilla dash default (solid). | §2.2 does not ask for dash. Do not extend `Stroke` for the blank. |
| Layer `Pen` | Same final `Stroke` command after `Ink::Stroke` is unpacked (`decorate.rs` L216–219), but page-absolute, zero-length rejected, rotatable, fragment height 0. | Not the inline model. §2.3 also says the separator is not `behind`. |

Shared, safe to call from a future blank draw:

- `Color`, `Length<Positive>`, `Paint::solid`, `Stroke::new`, `Path::line`, `Point::try_pt`
- `FragmentCommand::Stroke` and `Fragment::with_offset` (path translate)
- `draw_fragment` stroke arm, `Canvas::stroke_path`, `CanvasCommand::StrokePath`
- Backend `Mark::Stroke` / `to_stroke` (solid, width, rgb, alpha opacity, krilla default cap/join/miter/dash)
- `Error::UnbreakableWiderThanFrame` as the named overflow error (definition comment at `error.rs` L73–78 still talks about “current line start through the overflowing cluster”; HLine already reuses the variant for a rule wider than the frame)

Must stay off the HLine node:

- `HLineNode::split`, `Flow::hline`, `space_before` / `space_after`, and `TextAlign` on the rule
- Treating thickness as line-box growth
- Reading color from `HLine`’s independent default instead of the paragraph style
- Using column/cell/list frame width as “rest of this line”

`check_content` (`lower.rs` L33–36) allows `HLine` in column, cell, list item, and band the same way as `Spacer`. A blank must not become a new `FlowNode` variant to get into those containers; it rides inside `Text` / `Paragraph`, which those containers already allow.

## Non-Obvious Things

- Default thickness is **1 pt**, hardcoded in `HLine::default` (`document.rs` L515), not a `geom` constant. Default color is opaque black, not a lookup of `Style.color`, even though `default_style` is also black (`L596`).
- Default align is Center, but a default (full-width) line has x = 0, so Center is invisible until `width` is set shorter. There is no public `center()` method.
- Stroke y is the **centerline**, so half the stroke width lies above it and half below, assuming a centered stroke. With the height formula, the ink band is `[space_before, space_before + thickness]` only if the stroke is centered on the path and caps do not extend the ends. Cap/join/dash are whatever `KStroke::default()` is (`convert.rs` L73–82); quire does not set them. Layout tests assert the centerline (thickness 2, before 5 → y = 6; thickness 1, before 0 → y = 0.5) (`hline.rs` L136–141, L167).
- `space_before` / `space_after` **increase node height and the flow cursor**. They are not `Fragment.spacing_after`. A blank that “does not increase line height” cannot reuse these fields. Paragraph line height ignores stroke width entirely (`measure.rs` L114–119, `paragraph.rs` L264).
- Width is compared with `>` not bit-equality. Equal to the frame is legal and draws edge to edge.
- Wider-than-frame is reported even when the node is also taller than the frame (`hline.rs` L172–180).
- `HLine::lower` cannot fail. `Positive::pt(0)` fails at the call site, before the builder, via `Error::NonPositiveLength`.
- List `first_baseline` ignores strokes. A glyph-less HLine item does not pin the marker to the rule.
- Column “only to the column width” (§2.3) is not a clip. The child frame’s `width` **is** the column width, so `None` resolves to that, and `with_offset` moves the path. A fixed width still uses `TextAlign` inside that column frame.
- `Path::line` on the HLine path is not the same guard as `Layer::line`: the layer rejects a zero-length segment; HLine relies on positive width.
- Paragraph `align_x` and HLine’s match are duplicated, not shared. Reusing the function would still be the wrong **input**: frame width and rule length, rather than “x already equals the pen position and the blank width is the advance”.
- Backend glyph draw explicitly clears stroke; backend stroke draw explicitly clears fill. A blank must not be a `GlyphRun` of underscore characters and must not set both fill and stroke.

## Open Questions

- Krilla’s concrete default line cap is not in this repo (`..KStroke::default()`). If the default cap is not butt, ink extends past the segment endpoints by about half the thickness. Layout tests do not assert caps. Matters if a blank must stay inside its advance and not collide with the next glyph.
- §2.2 “线色默认跟当前文字色或黑” is still an or. HLine always uses its own color (default black) and never `Style.color`. Which arm a blank uses is not decided by this code.
- §2.2 “相对基线偏移：线在基线略下方” gives no pt value. HLine’s `thickness/2` centerline is a different origin (top of the rule box, not the baseline). Nothing in geom encodes a baseline gap.
- `Error::UnbreakableWiderThanFrame`’s doc comment still describes a cluster overflow (`error.rs` L73–76). HLine already uses it for a rule. A blank wider than `W` would be a third caller; whether the comment should mention all three is open.
- A line whose only content is a blank has no `Glyphs` command. `first_baseline` would skip the stroke. Paragraph `fragment_from_lines` would still size the line from `line_height` if the blank does not change that number. Marker alignment for a blank-only list line is unspecified here.
- Eat-to-end “no run after this blank” is an assemble-time rule. HLine has no run list and cannot express it. Where that check sits (paragraph builder vs `TextNode` measure) is outside this slice.
