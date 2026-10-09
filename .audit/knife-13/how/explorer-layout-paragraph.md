# Paragraph measure, break, and glyph placement

Baseline: `HEAD` `f208329fc172aa9cd5d81ebaa38e2f20a7746e9e` (`feat/style-builders`). Repo `/mnt/data/demo/quire`.

Scope: single-font `TextNode` → `ParagraphNode::layout` → `break_lines` → one `GlyphRun` per line → `FragmentCommand::Glyphs`, and where first-line width `W' = W − indent` plus mixed-style (bold) runs would have to cut in. Authority for the change: `docs/提案书.md` §2.1 (first line only; later lines keep full `W`; page breaks do not change `W`) and §3 (no justify; Start/Center/End). This file does not design the DSL.

There is no first-line indent field, no bold flag, and no per-span style anywhere in this pipeline. One paragraph is one string, one `Style`, one `FontId`, one `GlyphRun` per visual line.

---

## Components Found

### Nodes and style

| Type | Role | Lines |
| --- | --- | --- |
| `TextNode` | Unwrapped paragraph. Private `text: String` + `style: Style`. Wraps once, on the first `split`. | `src/layout/paragraph.rs:23-39` |
| `ParagraphNode` | Already-broken lines. Fields: `lines`, `line_height`, `spacing_after`, private `width` (the frame `W` used to wrap, not ink width). | `44-49`, `86-91` |
| `LineBox` | One visual line. Exactly one `run: GlyphRun`. | `16-19` |
| `Style` | One face and one size for the whole paragraph: `font`, `font_size`, `line_height_factor`, `color`, `paragraph_spacing`, `align`. No indent, no bold, no run list. | `src/layout/style.rs:18-25` |
| `TextAlign` | `Start`, `Center`, `End`. No justify arm. | `style.rs:10-14` |
| `FlowNode::Text` / `::Paragraph` | DSL lowers to `Text`. After the first split the tail is `Paragraph`, never `Text` again. | `src/layout/flow.rs:141-142`, `159-160`; `paragraph.rs:109`, `133-136` |

Public builder (boundary only): `Paragraph` is a `String` accumulator (`src/document.rs:755-763`). `Flow::paragraph` pushes `FlowNode::Text(TextNode::new(paragraph.text, self.style.clone()))` (`659-664`, helper `646-648`). `Flow.style` is one style for the whole flow, initialized from `default_style()` (`573-581`: font id 0, 12 pt, factor 1.2, black, spacing 0, `Start`). `p.bold` in `docs/提案书.md:85-88` is not implemented.

### Text pipeline (font-agnostic break, one-font measure)

| Piece | Role | Lines |
| --- | --- | --- |
| `ClusteredText` | CRLF → `\n`, UAX #29 graphemes, BK omitted from `glyph_text`. | `src/text/cluster.rs:43-86`, `143-158` |
| `ClusterCursor` | Parallel indices: cluster / glyph-scalar / glyph-byte. | `cluster.rs:30-34` |
| `break_lines` | One `avail: f32` for every line. `width_of` is called once per non-BK cluster. | `src/text/line_break.rs:42-74` |
| `Atom` | `Hard` or `Ink { pair, width, cluster }`. | `line_break.rs:12-19` |
| `BrokenLine` | `glyph_range`, `glyph_byte_range`, `width` (committed ink). | `257-274` |
| `measure_text` | One `FontId`. cmap → unscaled advance → `advance_norm = font_units / upem`. Missing cmap is `MissingGlyph`. | `src/text/measure.rs:41-98` |
| `MeasuredGlyph` | `scalar`, `glyph_id`, `advance_font_units`, `advance_norm`, `byte_range` into that call's string. No font id. | `measure.rs:17-23` |
| `line_metrics` | One face. `baseline = ascent` (pt). `line_height = max(ascent − descent + leading, size * factor)`. | `100-120` |
| `FontStore` | `face` / `face_mut` / `metrics` / `resolve`. Advance cache is `FontFace.advances: HashMap<glyph_id, f32>` per face. | `src/fonts.rs:49-54`, `125-148` |

`break_lines` has one production caller: `ParagraphNode::layout` (`paragraph.rs:65`). `measure_text` production callers: that layout (`63`) and `single_line_run` (`149`).

### Drawing records

| Piece | Role | Lines |
| --- | --- | --- |
| `GlyphPlacement` | `glyph_id`, `advance_norm` (not multiplied by size), `byte_range` into **this** `GlyphRun.text`. No x. | `src/canvas.rs:102-112` |
| `GlyphRun` | One `font`, one `font_size`, one `color`, one `origin` (baseline of the first glyph, y down). | `114-132` |
| `FragmentCommand::Glyphs` | One run per command. | `src/layout/flow.rs:81-82` |
| `align_x` | `Start → 0`, `Center → (frame − ink) / 2`, `End → frame − ink`. | `paragraph.rs:260-266` |
| krilla emit | One `Font`, one size, one fill. Per glyph: `advance_norm`, x offset `0`, y offset `0`, y advance `0`. | `src/backend/emit.rs:131-169` |

User-space glyph width is `advance_norm * font_size` (`canvas.rs:107-108`). The backend does not re-measure. Pen advance across glyphs inside one run is krilla's; there is no pen carried from one `GlyphRun` to the next. A second run on the same line must set its own `origin.x`.

### Who supplies `W`

`TextNode::split` passes `avail.width` straight through (`paragraph.rs:38`). Callers of `FlowNode::split`:

| Frame | `W` | Lines |
| --- | --- | --- |
| Body | `page_width − margin_left − margin_right` | `src/layout/paginate.rs:21-23`, `131-153` |
| Column | Resolved column width (not body width) | `src/layout/columns.rs:146-151` |
| Table cell | Column span width minus `2 * inset` (`inset = border/2 + padding`), via `natural_height` (full cell height, same width) | `src/layout/table.rs:201-222`, `src/layout/flow.rs:336-344` |
| List item body | `avail.width − list.indent` (hanging indent). Fragment then shifted by `list.indent` | `src/layout/list.rs:173-187`, `215` |

§2.1 examples match these frames (`docs/提案书.md:194-206`). List hanging indent is a **fragment** dx applied to every line. It is not a first-line break width. A paragraph indent would add inside the run, on top of that column.

`single_line_run` (`paragraph.rs:142-170`) does **not** wrap. It measures the whole string, errors if ink `> max_width`, and builds one run aligned in `max_width`. Callers: list marker (`list.rs:208-211`, then **overwrites** origin to x = 0) and behind/front / band text (`src/layout/decorate.rs:196-206`, `max_width = ∞`, align forced to `Start`). Those paths must not pick up paragraph first-line indent.

---

## Flow

### 1. `TextNode::split` wraps once

```37:39:src/layout/paragraph.rs
pub(crate) fn split(self, ctx: &mut LayoutCtx<'_>, avail: Frame) -> Result<Split> {
    ParagraphNode::layout(ctx.fonts, &self.text, &self.style, avail.width)?.split(avail)
}
```

Comment at `36`: every outcome returns a laid-out `Paragraph`, so a page move never wraps twice. `LayoutCtx` only supplies `&mut FontStore` (`flow.rs:19-21`). Height is ignored until `ParagraphNode::split`.

`FlowNode::split` validates the frame first (`flow.rs:155-156`): non-finite or `width <= 0` is `InvalidGeometry` before layout.

### 2. `ParagraphNode::layout` — measure, one width, one run per line

`paragraph.rs:52-92`.

1. Reject non-finite or `width <= 0` → `InvalidGeometry` (`58-60`). This `width` is `W`.
2. `ClusteredText::new(text)` (`62`).
3. `measure_text(fonts, style.font, clustered.glyph_text())` (`63`). The string is BK-stripped. Hard breaks are not measured and have empty glyph ranges (`cluster.rs:61-66`, `171`).
4. `break_lines(&clustered, width, cluster_width_pt)` (`65-67`). The closure sums `advance_norm * font_size` over that cluster's glyph range (`216-225`). One `font_size` for every glyph. The same `width` is the only avail.
5. `fonts.metrics(style.font)` + `line_metrics` (`68-70`). One baseline, one `line_height` for every line, including empty lines. Factor `<= 0` or non-finite → `InvalidGeometry` (`measure.rs:105-107`), already checked in `Style::new` (`style.rs:36-37`).
6. For **each** broken line, `glyph_run_for_line(..., line.width(), width, metrics.baseline)` (`73-83`). Third width argument is ink; fourth is the alignment frame, and it is full `W` for every line, including line 0.
7. Store `width` (full `W`) on the node (`90`). Not `W'`. Not ink width.

`cluster_width_pt` indexes `measured.glyphs[cluster.glyph_range()]`. That range is valid only because `measure_text` walks `glyph_text` scalars in order and clusters number those same scalars (`cluster.rs:62-65`, `measure.rs:63-90`). A second font breaks that single-tape assumption (see Boundaries).

### 3. `break_lines` / `pack_lines` — one `avail` for every line

`line_break.rs:43-74`, `101-161`.

Early exits:

- `avail` non-finite or `<= 0` → `InvalidGeometry` (`48-50`).
- Empty source → **one** zero-width line at cursor 0. Does not enter `pack_lines` (`52-65`). Test `empty_paragraph_one_empty_line` (`419-424`).

Otherwise `project_atoms` (`76-99`):

- `BreakClass::Hard` (`\n` after CRLF normalize; `break_class.rs:54-56`) → `Atom::Hard`. `width_of` is not called.
- Pair class → `width_of(cluster)`. Non-finite or `< 0` → `InvalidGeometry`.

`pack_lines` state: `line_start`, `i`, `line_width`, output `records`.

**Leading spaces, only at the start of a line** (`108-117`): if `line_start == i` and the atom is `PairClass::Sp` (U+0020 only, `break_class.rs:58-59`), skip it and move `line_start` forward. Not a width. U+3000 is `Id`, not `Sp` (`break_class.rs:80`, test `ideographic_space_kept` `381-386`), so it is ink and can occupy its own line.

**Hard break** (`121-126`): emit the open line (possibly empty if `line_start == i`), consume the BK, reset `line_width` to 0. CRLF is one cluster (`cluster.rs:165-171`, test `crlf_is_one_hard_break` `357-362`). Consecutive BK yields an empty line (`436-441`). A trailing BK also yields a following empty line because after the loop `line_start == atoms.len()` hits the else branch (`154-158`). `"A\n"` is two records: `"A"` and `""`. `"A"` with no BK is one record (`445-448`).

**Ink fits** (`127-131`): `line_width + width <= avail` (exact fit allowed; no epsilon). Add width, advance.

**Ink overflows** (`133-148`): `nearest_break` scans from `overflow_at` down to `line_start + 1` for a pair the 6×6 table allows (`163-176`, table `break_class.rs:27-40`). On a hit, committed width is the sum of ink atoms in `[line_start, split)` — the break atom itself is **not** included (`135-138`). Then `line_start = i = split`, `line_width = 0`, and the loop retries. The atom that was the break opportunity is often a space; the next iteration drops it as leading SP. That is why `"hello world"` at avail 5 becomes `"hello"` / `"world"` with the space in neither line (`297-301`, `365-370`, `373-378`). `"hello "` at avail 5 is `"hello"` plus an **empty** second line (overflow SP discarded, then EOF empty record).

On no legal break: `UnbreakableWiderThanFrame { width: line_width + overflowing_cluster, avail }` (`143-147`). `width` includes the cluster that did not fit. `avail` is the single function argument, the same value for line 0 and later lines. Docs: `src/error.rs:73-78`. Examples: a wide AL word (`305-314`), `字。` because CL may not start a line (`338-347`, `ALLOW[*][Cl] = false`), a combining cluster (`388-397`; class is the first scalar, `cluster.rs:54-55`).

**EOF** (`154-159`): leftover ink is emitted with the accumulated `line_width`. If `line_start == len` (just consumed a BK, or skipped a trailing SP), push one more empty record, width 0.

`emit_line` (`192-221`): empty range → one cursor, width forced to 0 (argument ignored). Non-empty → cursor at first cluster's start through last cluster's end. Hard atoms are not inside a line (`210`, `214`).

`BrokenLine::width` is that committed sum (`272-274`), not a re-sum of glyphs at draw time.

There is no per-line avail. After a wrap or a BK, `line_width` resets to 0 but `avail` does not change (`124-125`, `140-141`).

### 4. Glyph run and alignment x

`glyph_run_for_line` (`227-258`):

- `text` = `measured.text[byte_range]` (the line slice of `glyph_text`).
- Each placement's `byte_range` is rebased by `byte_range.start` so it indexes **that slice** (`243-244`).
- `origin = (align_x(frame_width, line_width, align), baseline)`.
- `font` / `font_size` / `color` copied from the one `Style`.

`align_x` (`260-266`) does not look at line index. `Start` ignores both widths and returns `0.0`. Center/End use the frame passed in, which today is full `W` for every line.

Pinned by `paginate::start_center_end_x_and_baseline_y` (`src/layout/paginate.rs:356-393`): for `"AB"` in a 200 pt body, x is `0`, `(body_w − ink) / 2`, or `body_w − ink`, and y is `metrics.baseline`. Margins in that test are default 0, so page x equals run x.

`fragment_from_lines` (`197-214`) clones each run and sets

```text
origin.y = index * line_height + run.origin.y
```

`origin.x` is left as `align_x` wrote it. `origin.y` from layout is the baseline, so line 0 stays at `baseline`, line 1 at `line_height + baseline`. One `FragmentCommand::Glyphs` per line. Fragment `height` is `line_count * line_height` and does **not** include `spacing_after` (`209-212`).

`draw_fragment` (`flow.rs:347-360`) adds the placed fragment origin (body origin + cursor y, plus any column/list dx) onto both x and y, then `Canvas::draw_glyph_run`. It does not re-align and does not walk glyph advances.

Emit (`emit.rs:147-169`) draws the whole run from that origin in one face. Glyph x offsets stay 0. Indent cannot be a per-glyph offset unless emit changes; the existing lever is `GlyphRun.origin.x`.

### 5. Vertical split — wrap width is frozen

`ParagraphNode::split` (`95-138`):

| Condition | Result |
| --- | --- |
| `self.width > avail.width` | `InvalidGeometry` (`96-98`). Not `UnbreakableWiderThanFrame`. Comment `94`: a narrower frame would overflow silently, so it is rejected. |
| `lines.is_empty()` | `Fit` height 0, empty fragment (`99-103`). `layout` never produces this: `break_lines` always returns ≥ 1 record. |
| `line_height > avail.full_height` | `UnsplittableTallerThanFrame` (`105-107`). |
| `avail.height < line_height` | `MoveToNextPage(FlowNode::Paragraph(self))` (`108-110`). Same node, same `width`, same already-aligned runs. |
| `lines_fitting == 0` | Same `MoveToNextPage` (`113-115`). `lines_fitting` (`172-189`) returns 0 when `avail < line_height`, so this is the same predicate. |
| all lines fit | `Fit`. Fragment gets `spacing_after`. Height is lines only (`117-120`). |
| some lines fit | `Partial`. Head fragment `spacing_after = 0`. Tail is `ParagraphNode` with `width: self.width` copied (`123-136`). |

`plan_pages` (`paginate.rs:187-195`): `MoveToNextPage` while `y == 0` becomes `UnsplittableTallerThanFrame`. While `y > 0`, the page is committed and the **same** node is pushed back. Next frame has the same body/column width and full height. `break_lines` is not called again.

`spacing_after` is applied by the paginator only on `Fit`, and only if the remainder of the page is ≥ spacing (`paginate.rs:162-167`). A `Partial` head does not carry it; the tail still has `spacing_after` and applies it when that tail eventually `Fit`s. Spacer is a different node (§2.13); paragraph spacing is droppable.

Empty lines still consume `line_height`. An empty paragraph is one blank line, not zero height. `"A\n\nB"` is three line boxes.

### 6. Where `W' = W − indent` applies

Proposal (`docs/提案书.md:204-206`): first line's available width is `W' = W − indent`; every later line still uses `W`. Page breaks do not change horizontal `W`. Justify is out of scope (`156`, `586`).

Two sites. Changing only one of them does not implement 首行缩进.

**Site A — break avail, line 0 only.** `pack_lines` compares and errors against a single `avail` (`line_break.rs:128`, `145-146`). That value is the `width` argument of `ParagraphNode::layout` (`paragraph.rs:65`). Today it is `W` for line 0 and for every line after a wrap or a BK.

The switch belongs where a line is packed, not in `width_of`. `width_of` only prices a cluster. Practical test that this line is the paragraph's first line: no `LineRecord` has been pushed yet (`records.is_empty()`). After the first emit — wrap **or** hard break — later lines keep `W`. Do not pass `W'` as the only `avail` to `break_lines`; that would narrow every line.

Use `W'` in both the fit test and the `UnbreakableWiderThanFrame.avail` reported for that line. A cluster wider than `W'` but narrower than `W` fails on line 0. The packer does not skip ahead to a wider next line.

`break_lines` also rejects `avail <= 0` (`48-50`). `indent >= W` makes `W' <= 0` and must fail before packing. That is still inside `plan_pages`, before `start_page`. Existing variant for a bad frame width is `InvalidGeometry` (`paragraph.rs:58-60`), not `UnbreakableWiderThanFrame`.

The empty-source short circuit (`52-65`) never sees avail except the `<= 0` check. Nothing to pack. Line 0 is still a zero-width run; width does not change.

`ParagraphNode.width` must stay **full `W`**. The tail copies it (`131`). The guard `self.width > avail.width` (`96-98`) is what stops a later narrower frame from clipping lines that were broken at `W`. Storing `W'` would let a frame with `W' < avail.width < W` through, and lines 1..N would overflow.

Leading SP is dropped before the fit test and does not consume `W'`. Indent is not a fake SP cluster and not extra advance on the first glyph (that would be tracking; §3 forbids kerning and justify, and emit hardcodes glyph offsets to 0).

**Site B — line 0 origin x.** `align_x` + the call that passes `frame_width = width` for every line (`paragraph.rs:73-83`, `248`, `260-266`).

`Start` returns 0. If site A is done and site B is not, the first line wraps earlier and still draws at x = 0. That is not a 二字 indent. The ink's right edge would sit at `line_width <= W'`, leaving `indent` of unused space on the **right**.

§2.1 says the first line's **available width** shrinks. It does not say how Center/End use the leftover. The alignment box that matches "content must lie in `[indent, W]`" is:

```text
x = align_x(W - indent, line_width, align) + indent
```

which is:

| Align | Line 0 | Later lines (unchanged) |
| --- | --- | --- |
| Start | `indent` | `0` |
| Center | `indent + (W' − ink) / 2` | `(W − ink) / 2` |
| End | `W − ink` | `W − ink` |

End's right edge stays the frame's right edge. Start's left edge moves by `indent`. Center is centered in the narrowed measure, not in the full `W`.

If Center/End keep aligning to full `W` while only Start adds `indent`, that is a different reading. The proposal text only forces the break width. `start_center_end_x_and_baseline_y` pins today's full-`W` formulas.

Apply site B only when building line index 0 in the `layout` loop. Do not apply it in `fragment_from_lines` via `with_offset`: that dx hits every line (list hanging indent works that way on purpose, `list.rs:215`). Do not re-apply it when the tail is split: line 0's x is already baked into `GlyphRun.origin`. If the whole paragraph `MoveToNextPage`s, line 0 moves with its origin intact. If line 0 was already emitted in a `Partial` head, the tail's first line is a later line and must keep full-`W` x.

`single_line_run` calls the same `glyph_run_for_line` with `frame_width = max_width` (`159-166`). Markers and layer text go through it. A `Style` indent field must not move those origins. List markers overwrite x to 0 anyway (`list.rs:211`).

Outer x (margin, column, list indent) is added later in `draw_fragment` / `with_offset` and applies to all lines. First-line indent is inside the run, before that.

Vertical layout does not change: indent does not affect `line_height`, `lines_fitting`, or `spacing_after`.

### 7. Failures happen before `start_page`

`paginate_sections` (`paginate.rs:93-117`) plans every section, then `measure_bands`, then `paint_pages`. `canvas.start_page` is only in `paint_pages` (`266-268`).

`node.split` → `TextNode::split` → `layout` runs inside `plan_pages` (`154`) and uses `?`. These abort the document with no page opened:

| Error | Where |
| --- | --- |
| `MissingGlyph` | `measure_text` cmap miss (`measure.rs:64-66`). §4: no `.notdef`, no fallback (`docs/提案书.md:590-594`). |
| `InvalidGeometry` | Bad `W`, bad cluster width, `W' <= 0` if indent is checked the same way, `self.width > avail.width` on a later split. |
| `UnbreakableWiderThanFrame` | `pack_lines` (`line_break.rs:143-147`) or `single_line_run` (`paragraph.rs:151-156`). |
| `UnsplittableTallerThanFrame` | Line taller than `full_height` (`paragraph.rs:105-107`), or `MoveToNextPage` at y = 0 (`paginate.rs:187-189`). |

A bold face that lacks a scalar fails the same way, during measure, still before `start_page`, even if the regular face has the scalar.

---

## Files Read

| File | Why |
| --- | --- |
| `src/layout/paragraph.rs` | Entire file. Layout, split, `single_line_run`, fragment y, `align_x`. No `#[cfg(test)]` in this file. |
| `src/text/line_break.rs` | Entire file, including pack tests. |
| `src/text/measure.rs` | `measure_text`, `line_metrics`, missing-glyph and fixture advances. |
| `src/text/cluster.rs` | Cluster tape, BK omitted from `glyph_text`, CRLF, combining cluster. |
| `src/text/break_class.rs` | Pair table, SP vs U+3000, class of the first scalar. |
| `src/text/mod.rs` | Module list only. |
| `src/layout/style.rs` | `Style` / `TextAlign` fields. |
| `src/layout/flow.rs` | `Frame`, `Fragment`, `FragmentCommand::Glyphs`, `Split`, `FlowNode::split`, `draw_fragment`, `natural_height`. |
| `src/layout/paginate.rs` | `plan_pages` vs `paint_pages` / `start_page`; align pin test `356-393`. |
| `src/canvas.rs` | `GlyphPlacement`, `GlyphRun` (`99-132`). |
| `src/fonts.rs` | `FontFace.advances`, `metrics` / `face_mut` / `resolve` (`41-148`). |
| `src/error.rs` | `MissingGlyph`, `UnbreakableWiderThanFrame`, `UnsplittableTallerThanFrame` (`54-81`, display `141-163`). |
| `src/backend/emit.rs` | `draw_glyphs` offsets (`131-169`). |
| `src/backend/krilla_canvas.rs` | `draw_glyph_run` resolves one `FontId` (`286-303`). |
| `src/document.rs` | `Paragraph`, `Flow::paragraph` / `text`, `default_style` (`573-581`, `646-664`, `755-763`). |
| `src/layout/list.rs` | Body `W`, fragment dx, marker `single_line_run` (`172-215`). |
| `src/layout/columns.rs` | Column frame width (`130-151`). |
| `src/layout/table.rs` | Cell inner width (`191-222`). |
| `src/layout/decorate.rs` | Layer text uses `single_line_run` (`185-206`). |
| `docs/提案书.md` | §2.1 `194-206`, justify `156` and `586`, §3 pipeline `580-588`, §4 `590-594`, example `p.bold` `85-88`. |
| `docs/quire-design.md` | Measure sketch `496-528` (not what the code does; see Boundaries). |

Grep of production `break_lines(` / `measure_text(`: only `paragraph.rs` (plus unit tests and `emit.rs` tests for `measure_text`).

---

## Boundaries

### FontStore vs this pipeline

`measure_text` borrows the face twice (`measure.rs:46-53`, `69`): first to clone bytes, index, and `units_per_em`; then `face_mut` to read or fill `advances`. Cache key is glyph id **within that face**. Two `FontId`s do not share a cache. `LocationRef::default()`, `Size::unscaled()`. No kerning, no GSUB, no synthetic bold. A scalar with no cmap entry is `MissingGlyph` and the function returns immediately — glyphs already measured in that call are discarded.

`line_metrics` takes `&FaceMetrics` from `fonts.metrics(style.font)` after `measure_text` returns. It does not look at which glyphs were used. Bold with a different ascent/descent does not affect today's line box, because today's line box never sees a second face.

`FontStore::resolve` is for the backend (`krilla_canvas.rs:288`), not for layout. Layout does not call krilla. Drawing uses the glyph ids and `advance_norm` layout stored. `check_upem` compares skrifa vs krilla units (`emit.rs:146`).

Design draft `quire-design.md:505-516` shows `MeasuredCluster { glyphs, advance, break_class }` and `measure_text(..., font_size)`. The code is a flat `MeasuredText { text, glyphs, advance_norm }` with no `font_size` and no break class (`measure.rs:26-30`). Cluster classes live only on `ClusteredText`. Point size is applied later in `cluster_width_pt`. Do not treat the draft struct as the current API.

### What mixed-style runs break

The breaker can stay font-agnostic **if** `width_of` returns each cluster's true pt width. `pack_lines` never reads `FontId`. These pieces cannot:

1. **Input.** `TextNode` is one `String` + one `Style` (`paragraph.rs:23-26`). `Style.font` is one id (`style.rs:19`). Bold in this pipeline is another registered face, not a flag. Nothing in measure or emit emboldens outlines (`emit.rs:147-169`).

2. **One `measure_text` call** (`paragraph.rs:63`) maps every scalar of `glyph_text` through one charmap. A bold span must be measured with the bold `FontId`. The glyph tape still has to stay index-aligned with `ClusterCursor.glyph` (non-BK scalars in order). Each `MeasuredGlyph` needs a face (and size and color, if those can change). `cluster_width_pt` (`216-225`) multiplies every glyph by one `font_size`.

3. **One `GlyphRun` per line.** `LineBox` (`16-19`), `glyph_run_for_line` (`250-257`), and `fragment_from_lines` (`203-207`) all assume one font, one size, one color, one origin. `GlyphRun` cannot mix them (`canvas.rs:119-132`). A line that changes face, size, or color needs several `FragmentCommand::Glyphs` on the **same** baseline y.

   Pen x of run *k* is `line_origin_x + sum of previous runs' ink`. Ink is `Σ advance_norm * that_run.font_size`. The line's `align_x` uses the breaker's total `line.width()` once; later runs on that line must not call `align_x` again. Those totals match only if `width_of` and the stored advances are the same numbers.

4. **Byte ranges.** Rebase is to the whole line slice (`243-244`). Each run's `GlyphPlacement.byte_range` indexes that run's `text` (`canvas.rs:110-111`). A bold slice needs its own string (or a shared line string with ranges into that string), not ranges into the full paragraph.

5. **Line box metrics.** One `baseline` and one `line_height` from `style.font` (`68-70`, `measure.rs:114-119`). A second face is drawn at the first face's baseline. Different ascents will not share a visual baseline unless layout picks one line box for the whole line (max height, one baseline). §2.1 / §3 do not say whose metrics win. Same nominal size is not the same `hmtx` advance: bold changes `cluster_width_pt` and therefore wrap points. That part is required, not optional.

6. **Cluster vs style boundary.** Clusters are graphemes of the **concatenated** paragraph (`cluster.rs:52`). `classify` uses the first scalar only (`54-55`). `"e" + bold(combining acute)` is one cluster (`175-181`) and one unbreakable atom (`388-397`). That atom would contain two faces. `width_of` can sum both advances; the line painter must still emit two runs with no break between them. CRLF normalization changes byte offsets (`\r\n` → `\n`, `143-156`) before glyph bytes are counted, so a style range taken on the raw DSL string is not a `glyph_text` range until it is remapped.

7. **`single_line_run`.** Same one-font measure and one run (`142-170`). List markers and layer text do not need mixed runs unless bold is allowed there. The width check compares total `advance_norm * font_size` to `max_width` and does not call `break_lines`.

8. **Pagination.** Once lines exist, `ParagraphNode::split` never remeasures. Mixed-run glyph commands have to be finished inside `layout`, before the first `MoveToNextPage` / `Partial`. The tail copies `lines` and `width` only (`127-132`).

`FragmentCommand` has no "span" variant. Multiple `Glyphs` commands are enough: `draw_fragment` already walks commands in order (`flow.rs:354-360`). No backend change is required for a second face if each command is a homogeneous `GlyphRun`.

### First-line indent vs other indents

| Mechanism | What moves | Which lines |
| --- | --- | --- |
| First-line indent (not implemented) | Break avail and line-0 `origin.x` | First visual line of the paragraph only |
| `ParagraphNode.width` | Guard against a narrower **later** frame | All lines already broken at `W` |
| List `indent` | Body frame width, then `with_offset` dx | Every line of the item (`list.rs:173`, `215`) |
| Column / cell | The `W` passed into `TextNode::split` | All lines, at break time |
| `align_x` | Origin x inside the frame | Each line independently, against full `W` today |

Indent does not change `W` stored on the node and does not change `W` on the next page. §2.1 line 206: page breaks are vertical. The code already matches that by freezing line boxes.

---

## Non-Obvious Things

- **`layout` always yields ≥ 1 line.** Empty string, trailing BK, and a discarded overflow space all produce a zero-width record (`line_break.rs:52-65`, `154-158`, `373-378`, `419-424`). `ParagraphNode::split`'s empty-lines branch (`99-103`) is not how `layout` behaves. Those empty records still become a `GlyphRun` (empty `glyphs`, `text == ""`) and still take one `line_height`.

- **`"A\n"` is two line boxes**, not one. The BK emits `"A"`; EOF then emits an empty line (`154-158`). `"A\n\nB"` is three. Indent-on-`records.is_empty()` therefore indents only the first of those, not the line after each BK. `"\n正文"` would indent an empty line 0 and put `正文` at full `W`, x = 0.

- **Leading U+0020 is dropped only when it is the first atom of a line** (`108-117`). Mid-line spaces that fit stay. A space that causes the overflow is excluded from the committed width and then dropped as the next line's leading SP. U+3000 is not dropped.

- **Exact fit stays on the line** (`line_width + width <= avail`). The next cluster is what triggers `nearest_break`.

- **`UnbreakableWiderThanFrame.width` includes the cluster that did not fit**, measured from the current line start (`143-146`). It is not the width of the line that was successfully emitted. On line 0 that `avail` must be `W'` or the error text lies (`error.rs:157-159`).

- **`MoveToNextPage` keeps the wrapped width and the baked x.** Tail construction copies `width` and the remaining `LineBox`es (`127-132`). `y == 0` never paints that node; it errors (`paginate.rs:187-189`). There is no second call to `break_lines`. Changing indent at draw time would also shift a continuation page's first remaining line if that code used "first line of this fragment" instead of "line 0 at layout".

- **`self.width > avail.width` is `InvalidGeometry`**, a different variant from a cluster that cannot break. It fires on the already-wrapped node, still inside `plan_pages`.

- **`Start` x is 0 even when the line is shorter than `W`.** Center/End are the only arms that read `frame_width`. Narrowing the break without changing `align_x` does not indent.

- **Alignment uses `line.width()` from the breaker, not a fresh sum of placements** (`79`, `248`). They match today because both come from `advance_norm * font_size`. A bold path that prices clusters one way and stores advances another way will shift Center/End.

- **Glyphs have no x.** Indent and mixed-run pen positions are `GlyphRun.origin` only. Emit writes x offset, y offset, and y advance as `0.0` (`emit.rs:150-154`).

- **`fragment_from_lines` adds baseline on top of `index * line_height`.** Layout already stored y = baseline (`249`). Line 0's drawn y is the baseline, not 0. The paginate test expects that (`377-378`, `392`).

- **Paragraph spacing is not part of fragment height** and is stripped on a `Partial` head (`126`). It is not an indent and not a line-gap inside the paragraph.

- **List marker `single_line_run` alignment is thrown away** (`list.rs:208-211` sets x to 0). An indent field on the shared `Style` would still affect the max-width check only if `single_line_run` subtracted it. It should not.

- **No justify means extra space stays at the line's start, center, or end.** Do not implement indent by stretching `advance_norm` inside the run. That would also desynchronize layout from krilla's hmtx adjustment (`src/backend/emit.rs:451`: the TJ number is the 1000-unit hmtx delta, not `advance_norm`).

- **Combining cluster is one atom and two glyphs** (`cluster.rs:175-181`). Style ranges that split it are one break atom with two faces.

- **Hard-break cursors do not advance `glyph` / `glyph_byte`** (`cluster.rs:61-71`). A BK between two style runs does not consume a measured glyph. Mapping runs by raw byte index without going through the cluster tape will drift after each `\n` or `\r\n`.

---

## Open Questions

1. **Center/End alignment box on line 0.** §2.1 only defines break width `W'`. `align_x(W', ink) + indent` keeps ink inside `[indent, W]` (End stays flush right). Aligning Center/End to full `W` while only shifting Start is also possible and matches the current pin test's frame more closely. The code has no third signal.

2. **Does a hard break inside one `TextNode` start a new indented line?** The proposal says the first line only; later lines use `W`. The implementation treats `\n` as intra-paragraph BK, and `spacing_after` is per `TextNode`, not per BK. `records.is_empty()` implements "first line of this paragraph". Indenting after every BK (or only after a blank line) would be a different rule and is not what §2.1 writes.

3. **`indent >= W`.** Fail with `InvalidGeometry` (same as `W <= 0`) or `UnbreakableWiderThanFrame`? Either way it is before `start_page`. `indent == 0` must be today's behavior.

4. **Whose line metrics when faces differ?** One baseline and one `line_height` are paragraph-global. A bold face with a larger ascent, or a run with a different `font_size`, needs a rule the current `line_metrics(style.font)` call does not have. §3 does not mention mixed metrics.

5. **Style run that splits a grapheme.** One cluster, two fonts, still unbreakable. Is that allowed at the DSL boundary, or must `bold` ranges snap to cluster edges before `width_of`?

6. **Empty line 0** (`""`, or a leading `\n`). It still emits a `GlyphRun`. Shifting its x is invisible. The following line is not line 0 if the empty record was pushed first.

These are unspecified by §2.1 / §3. The pipeline facts above do not depend on the answers, except that site A and site B stay "line index 0 of the paragraph that `layout` just built", and mixed faces cannot share one `GlyphRun`.
