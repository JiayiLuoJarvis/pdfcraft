# Knife 14 — paragraph run pipeline (DSL → TextNode → layout)

Tree: `feat/style-builders` tip `aaf14bf`, baseline `5deb0df`. Facts from the working tree. Scope: public `Paragraph` accumulation, `TextRun` / `Face`, `Flow::paragraph` → `TextNode`, and what cells / bands / layers do instead. Line-break packing and `HLine` stroke geometry are cited only where they constrain the plug-in.

Authority for the feature that is **not implemented**: `docs/提案书.md` §2.2, lines 241–268. No `blank`, fill-in, or inline underscore type exists under `src/`.

## Components Found

### Public surface

`src/lib.rs:21-24` re-exports `Paragraph` (with `document`, `Flow`, `Cell`, `Band`, `Layer`, `HLine`, …). `TextRun`, `Face`, `TextNode`, `ParagraphNode`, `Style` are `pub(crate)` (`src/layout/paragraph.rs:15-47`, `src/layout/style.rs:10-26`). Callers can name `Paragraph` only as the closure argument of `Flow::paragraph`.

`document()` (`src/document.rs:42-56`) runs the build closure once, then `Document::paint` (`:350-396`). The closure’s `Err` returns before paint. `Flow` stores the first `push` error and `into_nodes` (`:658-662`) returns it at `Section::lower` (`:161`), which is inside `paint`. So a `push(Err(_))` fails the document before pages; a `push(Ok(_))` whose problem shows up only in `layout_runs` also fails inside `paint`, with an empty canvas (tests at `src/document.rs:1426-1476`).

Doc comment `src/document.rs:40-41`: text uses the first `Document::font` at 12 pt, line height 1.2, black, start-aligned. That is `default_style()` (`:591-601`): `FontId::from_raw(0)`, `Positive::pt(12.0)`, `line_height_factor: 1.2`, `Color::rgb(0, 0, 0)`, `paragraph_spacing` 0, `first_line_indent` 0, `TextAlign::Start`. There is no public setter for font size, line height, text color, alignment, or paragraph spacing. `Style` (`src/layout/style.rs:16-26`) is paragraph-scoped, not per run. Every glyph run painted from a paragraph copies `style.color` and `style.font_size` (`src/layout/paragraph.rs:382-386`).

### Who can call `paragraph`

`Flow::paragraph` (`src/document.rs:678-686`) is the only constructor that turns a `Paragraph` into a node.

`Document` and `Section` `DerefMut` to `Flow` (`:571-588`, `:172-183`), so `doc.paragraph` / `section.paragraph` hit that method.

Nested `Flow`s that also have `paragraph`:

- `Columns::col` (`:827-829`) via `Flow::nested`
- `Flow::keep_together` (`:714-718`)
- `List::item` / `item_marked` (`:1020-1032`)

`Flow::child` (`:634-636`) clones `style` and `DocToken`. A paragraph inside a column or list item therefore still has 12 pt / black / start, plus whatever `first_line_indent` that paragraph sets.

Not on this path: `Cell`, `Band`, `Layer` (see Boundaries).

### `Paragraph` accumulation

`src/document.rs:774-817`.

```text
Paragraph { runs: Vec<TextRun>, first_line_indent: Length<NonNegative> }
```

`Default` (`:780-786`): empty `runs`, indent 0.

Methods:

- `text` (`:791-793`) → `append(Face::Body, …)`
- `bold` (`:796-798`) → `append(Face::Bold, …)`
- `first_line_indent` (`:801-803`) replaces the field. Last call wins. No check against the frame here.
- `append` (`:805-816`):
  - `text.is_empty()` returns immediately. `text("")` / `bold("")` do not create a run and do not break coalescing.
  - If the last run has the same `Face`, `last.text.push_str(text)`.
  - Otherwise `runs.push(TextRun::new(text, face))`.

Test `paragraph_coalesces_adjacent_faces_and_ignores_empty_appends` (`:1382-1406`): `text("A")`, `text("")`, `text("B")`, `bold("C")`, `bold("D")`, `text("E")` becomes three runs: Body `"AB"`, Bold `"CD"`, Body `"E"`.

`Paragraph` does not hold `Style`, font size, or color. `Flow::paragraph` clones `self.style` after the closure and overwrites only `first_line_indent` (`:682-684`).

### `Face` / `TextRun` / `TextNode` / `ParagraphNode`

`src/layout/paragraph.rs`.

`Face` (`:15-19`): `Body` | `Bold`. `Copy + Eq`. It is a cluster tag, not a font id.

`TextRun` (`:21-34`): `{ text: String, face: Face }`. No width, stroke, color, or size fields.

`TextNode` (`:42-68`): unwrapped `{ runs, style }`.

- `TextNode::new` (`:50-58`): empty string → `runs: []`; otherwise one `TextRun` with `Face::Body`.
- `TextNode::from_runs` (`:60-62`): stores the vec as given. Does not coalesce (coalescing already happened in `Paragraph::append`).
- `TextNode::split` (`:65-67`): `ParagraphNode::layout_runs(..., avail.width)` then `ParagraphNode::split(avail)`. Comment (`:64`): one wrap, so a page move never wraps twice.

`ParagraphNode` (`:70-77`): already-wrapped `{ lines: Vec<LineBox>, line_height, spacing_after, width }`. `width` is the frame width passed into `layout_runs`, not the ink width.

`LineBox` (`:36-40`): `{ runs: Vec<GlyphRun> }` only. No stroke, no non-glyph span.

`ParagraphNode::layout` (`:80-92`) is the `&str` helper used by layout tests. It synthesizes one Body `TextRun` (or empty). Production DSL does not call it. Production multi-run entry is `from_runs` → `layout_runs`.

`FlowNode` (`src/layout/flow.rs:140-152`): both `Text(TextNode)` and `Paragraph(ParagraphNode)`. `split` (`:159-160`): `Paragraph` does not re-enter `layout_runs`; `Text` does.

DSL always pushes `FlowNode::Text` (`src/document.rs:684`). After a partial split, the tail is `FlowNode::Paragraph` (`src/layout/paragraph.rs:179-188`).

### What `layout_runs` does with runs (boundary, not the breaker)

`ParagraphNode::layout_runs` (`src/layout/paragraph.rs:94-144`):

1. Rejects non-finite or `width <= 0` as `InvalidGeometry` (`:100-102`).
2. `indent = style.first_line_indent`. `first_width = width - indent`. Non-finite or `<= 0` → `InvalidGeometry` (`:104-108`). This is how an indent of the full frame width fails before any page (`src/document.rs:1452-1458`).
3. `measure_inline` (`:284-327`) then `break_lines` (`:111-118`) with `LineBudget { first: first_width, rest: width }`.
4. Line height and baseline come only from the **body** face: `fonts.metrics(style.font)` + `line_metrics` (`:119-121`, `src/text/measure.rs:100-120`). `baseline` is scaled ascent. `line_height = max(ascent - descent + leading, font_size * line_height_factor)`. Bold metrics are not consulted. Nothing in this function adds stroke thickness to `line_height`.
5. For each broken line (`:124-136`): `frame_width` is `first_width` on line 0 else `width`; `frame_x` is `indent` on line 0 else `0`; `line_x = frame_x + align_x(frame_width, line.width(), style.align)`; glyph origins are `(line_x, metrics.baseline)` in **line-local** coordinates.
6. Stores `spacing_after: style.paragraph_spacing` (public DSL leaves this at 0).

`align_x` (`:429-435`): Start → 0, Center → `(frame_width - line_width) / 2`, End → `frame_width - line_width`. Public paragraphs stay `TextAlign::Start`. Band text is the caller that sets `Start` or `End` (`src/document.rs:1118-1123`), and bands do not use `Paragraph`.

`ParagraphNode::split` (`:146-190`): if `self.width > avail.width` → `InvalidGeometry` (no reflow). Empty `lines` → `Fit` height 0. Otherwise vertical slice by `line_height`. `spacing_after` is attached only when the whole remainder fits (`:171`); a partial head gets `spacing_after: 0` (`:178`). Tail keeps `spacing_after` and the same `width`.

`fragment_from_lines` (`:249-268`) emits only `FragmentCommand::Glyphs`. For line `index`, glyph `y` becomes `index * line_height + origin.y`. Fragment `height` is `line_count * line_height` and does not include `spacing_after` (that field is separate). No clip.

`Fragment::with_offset` (`src/layout/flow.rs:63-76`) and `draw_fragment` (`:347-375`) already translate `FragmentCommand::Stroke` the same way as glyph origins. Paragraph layout never builds a `Stroke`. `Stroke` (`src/geom.rs:520-536`) requires `Length<Positive>` width.

### How runs become glyphs

`measure_inline` (`src/layout/paragraph.rs:284-327`):

- `cluster_tagged(runs.iter().map(|run| (run.face, run.text.as_str())))` (`:285`).
- `fonts.bold()` runs only if some run has `face == Bold` **and** non-empty text (`:286-293`). Missing bold → `Error::UnknownFont { id: 1 }` (`src/fonts.rs:134-136`). That `id: 1` is the conventional empty-slot id, not “the second registered font.” Test: `src/document.rs:1436-1439`.
- Tags map to font ids: `Body → style.font`, `Bold → bold id` (`:294-301`).
- Consecutive scalars with the same font id are measured as one `measure_text` slice (`:308-324`). Glyph byte ranges are shifted back onto the concatenated `glyph_text`.

`cluster_tagged` (`src/text/cluster.rs:131-158`) flattens runs to `(scalar, tag)` pairs. An empty string contributes no scalars (`:134-139`). Hard breaks stay clusters but are omitted from `glyph_text` (`ClusteredText::from_normalized`, `:72-76`). Tags exist per glyph scalar, not per original `TextRun`. There is no atom that is not a Unicode scalar.

`glyph_runs_for_line` (`src/layout/paragraph.rs:340-394`) walks `measured.glyphs[glyph_range]` and cuts a new `GlyphRun` only when `font` changes. Same-font glyphs are one run. `x` advances by `advance_norm * font_size` only. An empty glyph range still pushes one empty `GlyphRun` at `(line_x, baseline)` (`:347-355`).

`break_lines` (`src/text/line_break.rs:77-106`) takes a `ClusteredText` plus a per-cluster width callback. Empty text returns one line of width 0 (`:84-97`; test `:452-457`). `BrokenLine::width` (`:305-307`) is the packed cluster-width sum. Atoms are `Hard` or `Ink { width, … }` (`:12-19`, `:108-130`). No third atom kind.

`single_line_run` (`src/layout/paragraph.rs:193-222`) is a different function: one string, body font only, no `TextRun`, no `break_lines`. If `advance * font_size > max_width` → `UnbreakableWiderThanFrame`. Used by list markers and layers, not by `Flow::paragraph`.

### Error already used for “too wide to break”

`Error::UnbreakableWiderThanFrame { width, avail }` (`src/error.rs:73-79`). Display text: unbreakable content width exceeds available width (`:157-160`).

Paragraph text hits it from `break_lines`, not from `single_line_run`. Test `bold_and_indent_failures_happen_before_any_page` (`src/document.rs:1460-1476`): custom page width 24 pt, indent 12, text `"AA"` → `avail` near 12. So the budget that overflows is the **first-line** budget `W - indent`, and the failure is at `paint`, before any canvas command.

`single_line_run` raises the same variant for markers (`src/layout/list.rs:207-209`, `marker_width` 18 pt from `src/document.rs:1038`) and would for layers, except layers pass `max_width: f32::INFINITY` (`src/layout/decorate.rs:201`).

## Flow

```text
doc.paragraph(|p| { p.text / p.bold / p.first_line_indent })
  Flow::paragraph                          document.rs:678
    Paragraph::append                      :805   coalesce same Face; drop empty
    style = flow.style.clone()
    style.first_line_indent = paragraph.first_line_indent
    push FlowNode::Text(TextNode::from_runs(runs, style))
  Section::lower → FlowNode::Stack(nodes)  :161
  paint → paginate_sections                :389
    FlowNode::split                        flow.rs:155
      TextNode::split                      paragraph.rs:65
        layout_runs(fonts, runs, style, frame.width)
          measure_inline → cluster_tagged → measure_text per font slice
          break_lines(LineBudget { first: width-indent, rest: width })
          LineBox { GlyphRuns } per visual line
        ParagraphNode::split(avail)         vertical only
          Fit | Partial tail=FlowNode::Paragraph | MoveToNextPage
      later pages: ParagraphNode::split     does not call layout_runs again
    fragment_from_lines → FragmentCommand::Glyphs
    draw_fragment → canvas.draw_glyph_run
```

Frame width at the first `TextNode::split` is whoever is splitting:

- Body / `keep_together`: the paginator’s body frame (not re-derived here).
- Column: `ColumnsNode::split` builds `Frame { width: *column_width, … }` (`src/layout/columns.rs:145-151`).
- List item body: `body_w = avail.width - indent` where `indent` is the marker column, default 18 pt (`src/layout/list.rs:172-187`, `src/document.rs:1038`). Marker glyphs are a separate `single_line_run` and are not part of the item’s `TextRun`s (`list.rs:207-215`).
- Table cell: `inner_w = col_span width - 2*inset`, then `natural_height` (`src/layout/table.rs:211-222`, `src/layout/flow.rs:335-344`). `natural_height` requires `Split::Fit`; a partial cell is `UnsplittableTallerThanFrame`. Cell strings still go through `TextNode` (below), so this width rule applies to cell **text**, not to a `Paragraph` builder.

`layout_runs` bakes `x`, including indent and alignment, into glyph origins. `ParagraphNode::split` only adds `line_index * line_height` to `y`. A fill-to-EOL width has to be chosen inside `layout_runs` (the only time `avail.width` is applied). The continuation node does not know `W_remain`.

## Files Read

- `docs/提案书.md` §2.2 lines 241–268 (also §2.0 208–239 and §2.1 width rules 200–206 for the `W` the blank consumes)
- `src/lib.rs` (re-exports)
- `src/document.rs`: `document`, `default_style`, `Flow`, `Paragraph`, `HLine`, `Table::lower`, `Cell`, `Band`, `Layer::text_at`, `straight_path`, `paint`, paragraph tests 1368–1477
- `src/layout/paragraph.rs` (full)
- `src/layout/style.rs` (full)
- `src/layout/flow.rs`: `Frame`, `Fragment`, `FragmentCommand`, `FlowNode`, `draw_fragment`, `natural_height`
- `src/layout/lower.rs`: `check_content` / `node_name`
- `src/text/cluster.rs`: `cluster_tagged`, `ClusteredText::from_normalized`
- `src/text/line_break.rs`: `Atom`, `break_lines` empty path, `BrokenLine::width` (not `pack_lines` math)
- `src/text/measure.rs`: `line_metrics`
- `src/layout/decorate.rs`: `LayerText`, `measure_layer`, `measure_band`
- `src/layout/list.rs`: item frame and `single_line_run` marker
- `src/layout/columns.rs`: column `Frame` width
- `src/layout/table.rs`: cell `inner_w` and `natural_height`
- `src/layout/hline.rs`: `HLineNode` fields (block rule, not the stroke rasterizer)
- `src/error.rs`: `Error` variants used above
- `src/geom.rs`: `Stroke`, `Path::line`
- `src/fonts.rs`: `FontStore::bold`
- `src/canvas.rs`: `GlyphRun` (`origin` is baseline, y down)

## Boundaries

### In scope for a `Paragraph` blank

Any `Flow::paragraph` closure: document body, section body, column, `keep_together`, list item. The run list is `Paragraph.runs` → `TextNode.runs` → `layout_runs`. A blank is another element of that list, sibling of `text` / `bold`, not a new `FlowNode`. `check_content` (`src/layout/lower.rs:33-36`) already allows `Text` and `Paragraph` in column, table cell, list item, and band. `node_name` calls both `"paragraph"` (`:72`). No new container permission is required if the blank stays inside `TextNode`.

### Cells — do not see `Paragraph`

`CellContent` (`src/document.rs:923-928`): `Text(String)` | `Image` | `HLine`. `Cell::new` / `From<&str>` / `From<String>` (`:930-995`). No closure, no `bold`, no indent.

`Table::lower` (`:891-897`) maps `CellContent::Text(text)` to `vec![flow.text(text)]`. `Flow::text` (`:665-667`) is private and always `TextNode::new(text, self.style.clone())`: one Body run, or empty runs if the string is empty. Cell text **does** wrap through `layout_runs`, at `inner_w`, but only as that single string. A method on `Paragraph` does not appear in cells.

### Bands — strings, then the same `TextNode::new`

`Band` (`src/document.rs:1094-1153`): `left` / `right` store `Option<String>`. `lower` builds `TextNode::new` with `align` overwritten to `Start` or `End` (`:1117-1123`), packed into a `Columns` row with margin insets. No `Paragraph`.

`measure_band` (`src/layout/decorate.rs:248-275`) splits that stack against a frame as wide as the **page** and as tall as the page, then:

- `Fit` with `height <= margin` ok
- `Fit` taller than the margin → `DecorationTallerThanMargin`
- `Partial` / `MoveToNextPage` / `UnsplittableTallerThanFrame` → `DecorationTallerThanMargin { height: page_h, margin }`
- `UnbreakableWiderThanFrame` → `DecorationWiderThanFrame`

Band strings therefore share `layout_runs`, but a `Paragraph::blank` would not be callable. If a blank were stuffed into those `TextNode`s later, an over-wide fixed blank would surface as `DecorationWiderThanFrame`, not `UnbreakableWiderThanFrame`.

### Layers — not the run pipeline

`Layer::text_at` (`src/document.rs:1190-1197`) pushes `LayerItem::Text(LayerText { text, at, style })`. `measure_layer` (`src/layout/decorate.rs:196-206`) forces `TextAlign::Start`, calls `single_line_run(..., f32::INFINITY)`, then sets origin to `at + run.origin`. No wrap, no `TextRun`, no `break_lines`. Width never errors.

`Layer::line` (`src/document.rs:1217-1220`, `straight_path` `:1299-1305`) is a page-coordinate segment. `from == to` → `InvalidGeometry` before paint. `Path::line` itself (`src/geom.rs:625-628`) does **not** reject equal endpoints; only `straight_path` does. That is a different stroke from an inline blank.

### List markers — not item runs

The marker label is `single_line_run` into the fragment at x = 0 (`src/layout/list.rs:207-213`). Item body fragments are offset by `self.indent` (`:215`). A blank inside `item(|flow| flow.paragraph(...))` lays out at `body_w` and is shifted right by the marker column. It does not extend under the marker. A blank cannot be added to the marker string through `Paragraph`.

### Block `hline` — different node

`Flow::hline` / `Cell::hline` (`src/document.rs:726-733`, `:951-959`) lower to `FlowNode::HLine(HLineNode)` (`:559-568`, `src/layout/hline.rs:10-18`).

| | Inline blank (§2.2) | `HLine` (implemented) |
| --- | --- | --- |
| Node | inside `TextNode` runs | `FlowNode::HLine` |
| Width | fixed pt **xor** rest of the current line | `Option<Positive>`: `None` = full **frame** at split (`hline.rs:11`) |
| Height | must not add to `line_height` | `space_before + thickness + space_after` (builder docs `document.rs:496-498`) |
| Split | rides the line inside `ParagraphNode` | whole node, `MoveToNextPage` (docs `:497-498`) |
| Paint | one `Stroke` under the baseline | `FragmentCommand::Stroke` from the hline module |

Shared types already exist (`Color`, `Length<Positive>`, `Stroke`, `FragmentCommand::Stroke`). The block node is the wrong place to hang the inline blank: `check_content` treats `HLine` as its own kind (`lower.rs:80`), and `fragment_from_lines` would not see it.

### Hard breaks stay inside text runs

`\n` / `\r\n` are clusters inside a `TextRun`’s string (`cluster.rs` normalize + `BreakClass::Hard`). They are not run boundaries. `Paragraph::append` does not split on newlines. A fill-blank that follows `text("a\n")` is still the next **run**, placed after the hard break in the flattened scalar tape. The “no run after a fill blank” rule is about later `text` / `bold` / blank calls, not about newlines already inside earlier strings.

## Non-Obvious Things

1. **Do not encode a blank as `Face` or as `TextRun.text`.** `cluster_tagged` only keeps Unicode scalars. Empty text is dropped (`cluster.rs:134-139` and `Paragraph::append` `:806-808`). A placeholder space is a real glyph, measured by `measure_text`, and is an `Ink` atom the breaker may legally break. §2.2 says the blank is not measured and is unbreakable. `Face` is `Copy` and stored once per glyph scalar (`measure_inline` `:294-301`); a faceless gap cannot live there.

2. **Stripping blanks and then clustering the leftover text deletes the gap.** `text("A")`, blank, `text("B")` must stay three runs because `append` only coalesces when the **last** run has the same face (`document.rs:809-814`). If `measure_inline` concatenates the two Body strings, `glyph_runs_for_line` sees one font and emits one `GlyphRun` for `"AB"` with no hole (`paragraph.rs:359-391`). The blank width is gone and the glyphs touch. The hole has to survive as structure beside the glyph tape: separate glyph runs (or an explicit x jump) plus a stroke, not a font-change cut. Font-change cuts are the only splits today, and Body/Body does not change font.

3. **`append` cannot grow a blank.** Empty input returns before the last-run check. A blank method that called `append(some_face, "")` would no-op. Coalescing must stay text-only. Two adjacent Body runs merge; a blank between them must not merge them. Adjacent fixed blanks are not merged today because that code does not exist; §2.2 does not require merging. Merging would be wrong if each blank can carry its own thickness or color.

4. **Fill-to-EOL cannot be a pt stored at `paragraph()` return.** `Paragraph` does not know the frame. `TextNode` stays unwrapped until the first `split`, and that width depends on body vs column vs list-item `body_w` vs (if ever) cell `inner_w`. Indent applies only to line 0: budget `first_width = width - indent`, `frame_x = indent` (`paragraph.rs:104-127`). A fill on line 0 consumes remainder of `first_width`, not of the raw frame. A fill that starts after a hard break or a wrap on a later line consumes remainder of `width`. `align_x` uses `BrokenLine::width`, which is cluster ink only (`line_break.rs:305-307`). If the fill is included in that width so the line equals `frame_width`, Start/Center/End all yield x offset 0. If it is painted after `align_x` without being part of `line.width()`, Center/End will shift the glyphs and leave the stroke in the wrong place. Public paragraphs are Start, so this is latent, but `layout_runs` always calls `align_x`.

5. **“No run after fill” has nowhere to live yet.** `text` / `bold` return `()`. `Flow::paragraph` always `push(Ok(node))` (`:685`). Assembly errors elsewhere use `Flow::push(Err)` (images, tables, lists) and surface at `into_nodes` during `paint`, still before canvas commands. A fill followed by `text("…")` / `bold("…")` / another blank needs that channel (flag on `Paragraph`, checked in `Flow::paragraph`). `text("")` after a fill does **not** create a run under current `append`; treating that as “a following run” would contradict `paragraph_coalesces_adjacent_faces_and_ignores_empty_appends`. A second fill **is** a run. Fixed blank then fill is allowed by §2.2 (the prohibition is only after the fill-to-EOL form). Fill then fixed is not.

6. **Fixed width vs `W` is a layout error, same variant as an unbreakable cluster.** §2.2: wider than the current line `W` → `UnbreakableWiderThanFrame`, no split, no clip. Existing text uses the line budget, which is `W - indent` on line 0 (test avail 12 when frame is 24 and indent is 12). A fixed blank narrower than `W` but wider than the leftover on the current line is a wrap of the whole blank, not that error — the error is “does not fit a line’s budget”. `layout_runs` will not see the blank unless the width is presented to the breaker as an unbreakable atom or the run list is segmented around blanks. `break_lines` has only `Hard` and `Ink` (`line_break.rs:12-19`).

7. **Han-count sugar needs `style.font_size`, which `Paragraph` does not hold.** §2.2: about N han → `N * 当前字号`, square approximation, not the measured advance of `字`. The only size in the pipeline is `Style.font_size`, currently always 12 pt (`default_style`). Bold does not change it. Resolve either by passing `font_size` into `Paragraph` before the closure (`Flow::paragraph` has `self.style`) or by storing N on the run and multiplying in `layout_runs`, where `style` is present (`paragraph.rs:110`). Both match today’s constant 12 pt. Layout-time stays correct if a font-size setter appears later; the closure-time value is what “当前” means at the call.

8. **Line color “当前文字色或黑” is one color.** All runs share `style.color`, default `Color::rgb(0, 0, 0)`. There is no per-run color, and bold does not change it. Defaulting the stroke to `style.color` and defaulting it to black are the same until a text-color API exists. `HLine::color` (`document.rs:535-537`) is independent and also defaults to black (`:515`). Thickness on `HLine` defaults to `Positive::pt(1.0)` (`:514`). `Stroke::new` rejects non-positive width (`geom.rs:529`). §2.2 does not pick a default thickness or a numeric baseline offset.

9. **Stroke y is baseline-local, then line-index shifted.** Glyph origin y is `metrics.baseline` (ascent), then `fragment_from_lines` adds `index * line_height` (`paragraph.rs:256-260`). A stroke path has to go through that same shift. `LineBox` cannot store a path today. Emitting `FragmentCommand::Stroke` from `fragment_from_lines` is enough for parents: columns, lists, and tables already `with_offset` stroke paths (`flow.rs:70-71`). Do not add the stroke thickness into `lines_height` / fragment `height` (§2.2 不增加行高). Fragments are not clipped, so an offset that drops below `line_height - baseline` paints into the next line without changing layout height. `spacing_after` is 0 in the public DSL, so the next paragraph’s first baseline is the next place ink sits.

10. **Zero remaining width can build a degenerate segment.** `Path::line` allows `from == to`. `Layer::line` rejects that via `straight_path`. A fill whose preceding ink already equals the line budget, or a fixed width of 0, produces that segment if the stroke is a two-point line. `Length<NonNegative>` allows 0; `Length<Positive>` does not. `HLine` widths are `Positive`. §2.2 says the length goes through §2.0 and names 非负 vs 正 only in general (提案书 226).

11. **Empty `paragraph(|| {})` is not zero lines.** No runs → `cluster_tagged` empty → `break_lines` one line of width 0 → `glyph_runs_for_line` one empty `GlyphRun` → fragment height is one `line_height`. `ParagraphNode::split`’s `lines.is_empty()` → height 0 path (`paragraph.rs:151-155`) is not what `layout_runs` produces for empty input. A blank-only paragraph must not be implemented by leaving `runs` empty.

12. **Cross-page is “the line moves”, because wrap is frozen.** `TextNode::split` comment (`paragraph.rs:64-66`) and `ParagraphNode::split` (`:145-149`): later frames that are narrower than `self.width` are `InvalidGeometry`, not a second wrap. §2.2 “跨页随所在行走” matches baking the stroke into the `LineBox` / fragment commands at `layout_runs`. A fill width computed against the first frame must not be recomputed on the tail.

13. **Bold lookup must ignore blanks.** `measure_inline` calls `fonts.bold()` only for a non-empty Bold run (`:286-289`). A paragraph that is only a blank, or Body text plus a blank, must not require `Document::bold_font`. A missing glyph can still fail the document if real text is present (`:1436-1450`); the blank itself has no scalar to miss.

14. **Tests that lock the run vec.** `paragraph_enters_the_flow_unwrapped` (`document.rs:1369-1378`) expects `TextNode::new("A B", default_style())`, i.e. one Body run. `paragraph_coalesces_…` (`:1382-1406`) expects `from_runs` of three `TextRun`s. Changing `TextRun` into an enum, or stopping coalescing, breaks those equalities. `TextNode::new` / `ParagraphNode::layout(&str)` should keep building a single Body run so cell, band, and layout-test callers stay valid.

## Plug-in (where §2.2 attaches)

Not a design; the seams the current types force:

| §2.2 | Attach |
| --- | --- |
| Sibling of `text` / `bold` | New method(s) on `Paragraph` (`document.rs:789-816`), not on `Cell` / `Band` / `Layer` / `HLine` |
| Width xor (fixed pt, or N × font size, or fill-to-EOL) | A run enum beside `TextRun`, stored in `Paragraph.runs` and `TextNode.runs`. `Face` stays `Body \| Bold`. Fill is a variant, not an `f32`, until `layout_runs` sees `width` / `first_width` |
| Fill then another run is an assemble error | Check when a non-empty run is appended after a fill variant; `Flow::paragraph` `push(Err(_))` so `into_nodes` fails before pages. Do not wait for `break_lines` |
| Fixed wider than the line budget | Inside `layout_runs` / the breaker, `UnbreakableWiderThanFrame`. Budget is `first_width` on line 0 |
| Not measured, one `Stroke`, no extra line height | Skip `cluster_tagged` / `measure_text` for that run. Emit `FragmentCommand::Stroke` from the line’s fragment (`fragment_from_lines`). Leave `line_height` on body `line_metrics` |
| Unbreakable, rides the line across pages | Resolve geometry in `layout_runs` before `ParagraphNode` is stored. `ParagraphNode::split` only slices `lines` |
| Default stroke color | `style.color` (black today). Thickness and baseline offset have no fields; `Stroke` needs `Positive` |
| Where it must not go | `Flow::text`, `Band::lower`, `Layer::text_at`, `single_line_run`, `HLineNode` |

`Vec<TextRun>` (`Paragraph` `:776`, `TextNode` `:45`) is the vec that has to become “text run or blank”. `measure_inline`’s `runs.iter().map(|run| (run.face, run.text))` (`paragraph.rs:285`) is the line that will silently drop or glue a blank if the new variant is forced through it.

## Open Questions

- Numeric default for stroke thickness and for “略下方” baseline offset. Not in §2.2. `HLine` uses 1 pt and no baseline concept. Offset is not on `Style`.
- Whether fixed width is `Length<NonNegative>` (0 allowed) or `Length<Positive>` (matches `HLine` / `Stroke`). Zero width vs `Path::line` equal endpoints is unspecified.
- Whether a fill whose remainder is 0 is an empty stroke, omitted, or `InvalidGeometry` (layer lines reject zero-length; `Path::line` does not).
- Whether N in the han sugar is `u32`, and whether N = 0 is legal. “当前字号” is 12 pt with no public setter; confirm the sugar may close over `Flow.style.font_size` at `paragraph()` time.
- Per-blank thickness / color parameters vs one paragraph default. §2.2 lists them as blank attributes. The text DSL has no per-run color API to copy from a neighboring `bold` run.
- Interaction of fill-to-EOL with `TextAlign::Center` / `End`. Public paragraphs cannot set align; `layout_runs` still applies it. Spec does not say.
- A fixed blank that fits `width` but not `first_width` because of indent: the existing cluster path errors against `first_width`. §2.2 says “当前行 W”. §2.1 defines first-line `W' = W − indent`. The text test uses `W'`. Confirm blanks use `W'`.
- Hard break immediately before a trailing fill (`text("a\n"); blank_fill()`): the fill occupies the next line’s full budget. Spec does not mention this; it follows from newline-in-string plus “fill is the last run”.
