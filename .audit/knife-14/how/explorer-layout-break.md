# Explorer: paragraph measure, break, indent, UnbreakableWiderThanFrame

Baseline: `feat/style-builders` @ `5deb0df`. Authority: `docs/提案书.md` §2.2 (L241–268), with §2.1 wrap width (L192–206). Angle: `layout/paragraph.rs` measure → break → `LineBox` → fragment draw, plus `text/line_break.rs` and the measure/cluster tape it consumes. DSL builders and block `HLine` are out of scope except where the same error variant is constructed.

No inline blank exists. `TextRun` is `{ text: String, face: Face }` only (`src/layout/paragraph.rs` L21–34). `Face` is `Body | Bold` (L15–19). Every width in the break stream is a sum of skrifa advances. Every drawn paragraph command is `FragmentCommand::Glyphs`.

## Components Found

### Entry

`FlowNode::Text` splits by calling `TextNode::split` (`src/layout/flow.rs` L160). That always lays out, then splits the laid-out paragraph (`paragraph.rs` L64–67):

```text
TextNode::split
  → ParagraphNode::layout_runs(fonts, runs, style, avail.width)
  → ParagraphNode::split(avail)
```

Comment at L64: every outcome is a laid-out `Paragraph`, so a later page move never wraps again. `Split::MoveToNextPage` / `Partial` tails are `FlowNode::Paragraph`, not `Text` (`paragraph.rs` L161, L185–188).

`ParagraphNode::layout` (`L80–91`) is the single-string entry (tests). It builds one `Face::Body` run, or no runs when `text` is empty, then calls `layout_runs`. `TextNode::new` (`L50–57`) does the same. `TextNode::from_runs` (`L60–62`) keeps an existing run list.

`layout_runs` (`L94–144`) order:

1. Reject non-finite or `width <= 0` as `Error::InvalidGeometry` (L100–102).
2. `indent = style.first_line_indent.as_pt()` (`Style` field, `src/layout/style.rs` L24, default `NonNegative::ZERO` at L46). `first_width = width - indent`. Non-finite or `first_width <= 0` is `InvalidGeometry` (L104–108), **before** measure and **before** break. `indent >= W` never becomes `UnbreakableWiderThanFrame`.
3. `measure_inline` (glyphs).
4. `break_lines` with `LineBudget { first: first_width, rest: width }` and `cluster_width_pt`.
5. `line_metrics` of **`style.font` only** (body id). `line_height = metrics.line_height`. Bold face metrics are not consulted.
6. One `LineBox` per broken line. x is baked here.

`ParagraphNode` stores `lines`, `line_height`, `spacing_after`, and `width` = the full frame `W`, not `W'` (`L138–143`). `split` (`L147–149`) returns `InvalidGeometry` if a later frame is narrower than that stored `W`. It does not re-break and does not re-apply indent.

### Measure tape

`MeasuredInline` (`L270–282`): `TaggedClusteredText<Face>` plus `Vec<InlineGlyph>`. `InlineGlyph` is one Unicode scalar: `font`, `glyph_id`, `advance_norm`, `byte_range` into `glyph_text`.

`measure_inline` (`L284–327`):

1. `cluster_tagged` over `(face, text)` pairs (`src/text/cluster.rs` L131–158).
2. If any **non-empty** `Face::Bold` run exists, `fonts.bold()?` (`paragraph.rs` L286–292; `FontStore::bold` is `UnknownFont { id: 1 }` when the slot is empty, `src/fonts.rs` L134–136). Empty bold strings do not touch the slot. A bold run whose only scalar is BK still counts as non-empty (`"\n"`), so `bold()` runs even though that scalar gets no glyph tag.
3. Each **glyph scalar** tag (`Face`) resolves to `FontId`. Body → `style.font`. Bold → the slot id (`paragraph.rs` L294–301).
4. Contiguous equal `FontId`s are one `measure_text` call (`L307–324`). `debug_assert` requires one measured glyph per scalar in the slice (`L316`). Byte ranges are shifted back onto the full `glyph_text`.

`cluster_width_pt` (`L329–338`) is the break callback: sum of `advance_norm * font_size` over `cluster.glyph_range()`. Same `style.font_size` for body and bold. An empty range sums to `0.0`.

`measure_text` (`src/text/measure.rs` L41–98`) maps every scalar through skrifa charmap. Missing cmap → `Error::MissingGlyph` (L64–66). No `.notdef`, no fallback. Advance is unscaled width / units-per-em, cached on the face (`L68–81`). This runs for spaces that the breaker will later drop. A blank that entered `glyph_text` would be a cmap lookup.

`line_metrics` (`measure.rs` L100–121): scale ascent/descent/leading by `font_size / units_per_em`. `line_height = max(ascent - descent + leading, font_size * factor)` with descent negative in the fixture. `baseline = ascent`. Non-positive or non-finite `factor` → `InvalidGeometry`. Paragraph height is this number times line count (`paragraph.rs` L244–247, L264). Nothing in the line's ink, and no stroke thickness, is added.

### Cluster tape

`ClusteredText` (`cluster.rs` L13–17, built at L56–96): CRLF-normalized string, BK-free `glyph_text`, one `ClusterRecord` per grapheme. Class comes from the **first** scalar only (`classify`, `break_class.rs` L54–75).

BK (`\n` after CRLF fold) is a cluster with an **empty** `glyph_range`: it does not advance `glyph` / `glyph_byte` (`cluster.rs` L71–76). Non-BK clusters append every scalar of the grapheme to `glyph_text`. Combining marks are one cluster and N glyphs (`cluster.rs` test L231–237; break test treats that cluster as unbreakable, `line_break.rs` L421–429).

`TaggedClusteredText<T>` (`cluster.rs` L20–23) stores one tag **per glyph scalar**, not per cluster. `cluster_tagged` (`L131–158`) flattens runs to scalars, folds CRLF (the `\n` tag is dropped; the `\r` tag is kept on the single `\n`, `L199–205` and test `L249–254`), then copies tags only for clusters whose `glyph_range` is non-empty. A cluster split across runs keeps each scalar's tag (`test` L241–246: `"e"` + `"\u{0301}X"` → tags `[0, 1, 1]`).

`cluster_tagged` only accepts `(T, impl AsRef<str>)`. There is no non-string atom.

`project_atoms` (`line_break.rs` L108–130) emits **exactly one** `Atom` per cluster, in order. Atom index equals cluster index today.

```text
Atom::Hard
Atom::Ink { pair: PairClass, width: f32, cluster: usize }
```

`width_of` is called here, once per non-BK cluster, **before** packing (`L117–118`). Non-finite or negative width → `InvalidGeometry` (`L119–121`). BK is not measured.

### Breaker

`LineBudget` (`line_break.rs` L42–74`): `{ first, rest }`. `uniform(w)` sets both. `for_line(index)` is `first` iff `index == 0`, else `rest` (`L56–61`). `validate` rejects non-finite or `<= 0` on either (`L64–72`).

`break_lines` (`L77–106`): validate budget, then either the empty-text short circuit or `project_atoms` + `pack_lines`.

Empty `ClusteredText` (`L84–97`): one `LineRecord`, cursors at 0, `width: 0.0`. No atoms.

`pack_lines` (`L133–194`) live state: `records`, `line_start` (atom index), `i`, `line_width`. Each iteration reads `avail = budget.for_line(records.len())` (`L140`). The line **being built** is index `records.len()`; emitting a record (soft wrap, hard break, or trailing empty line) bumps later lines onto `rest`.

Leading `PairClass::Sp` while `line_start == i` is discarded (`L141–150`): `i` and `line_start` advance, width stays 0, no record is pushed. This does **not** consume the `first` budget. Only U+0020 is `Sp` (`break_class.rs` L58–59). U+3000 is `Id` (`L80`, test `line_break.rs` L414–418) and is kept.

Fit test (`L160–164`): `line_width + width <= avail` (raw `f32`, equality fits). On overflow, `nearest_break(atoms, line_start, i)` (`L166–184`, search `L196–208`):

- Walk `k` from `overflow_at` down to `line_start + 1`.
- Both `atoms[k - 1]` and `atoms[k]` must be `Ink`. `allows(prev.pair, next.pair)` (`break_class.rs` L27–40, L103–105) means a break is allowed **before** `next`.
- Hit: emit `[line_start, split)` with width = sum of ink widths in that half-open range (`line_break.rs` L167–174), then restart at `split` with `line_width = 0`. The overflowing atom is not included. A `Sp` that lands at the new `line_start` is then dropped, which is why `"hello "` at width 5 yields `"hello"` plus an **extra empty line** (`test` L406–411, EOF branch L187–192).
- Miss: `Error::UnbreakableWiderThanFrame { width: line_width + width, avail }` (`L176–180`). `width` is the unbreakable prefix from the current line start **through** the overflowing atom, not the atom alone. `avail` is the budget of the line being built.

Line-start overflow: if `i == line_start` and the atom is not dropped `Sp`, the search range `line_start+1 ..= line_start` is empty. `nearest_break` returns `None`. The atom does **not** move to the next line to try `rest`. It errors with `avail == for_line(records.len())`. Test `first_line_overflow_reports_the_first_budget` (`L507–526`): one cluster of width 1 against `{ first: 0.5, rest: 10 }` → `{ width: 1.0, avail: 0.5 }`.

`Hard` (`L154–158`) emits `[line_start, i)` **excluding** the BK, then starts a new line after it. Consecutive BK yield an empty record (`test` L529–534). A leading BK emits an empty line 0 while `records` is still empty, so that empty line **is** the `first` budget slot; following text uses `rest` (`test` L488–504: `"你\n你好"` with both budgets 1 → `["你", "你", "好"]`).

EOF (`L187–192`): leftover atoms become a line; if `line_start == len` (trailing BK, or a dropped overflow space), push one more empty line of width 0.

`emit_line` (`L225–254`): empty atom range → cursor from `eof_cursor`, width forced to 0. Non-empty: start cursor of `atoms[start]`'s **stored cluster index**, end cursor of `atoms[end - 1]`'s cluster. `Hard` inside a line is `unreachable!`.

`eof_cursor` (`L256–267`): for a preceding `Atom::Hard`, calls `text.cluster(atom_index - 1)`. That treats atom index as cluster index. The `Ink` arm uses the stored `cluster` field. They coincide only because `project_atoms` is 1:1.

`BrokenLine` (`L290–307`) exposes `text()` (slice of `glyph_text`), `glyph_range()`, `glyph_byte_range()`, `width()`. Atoms, pair classes, and per-atom widths are dropped. A gap that is not a glyph index cannot be recovered from `width` alone once two gaps exist, and a single gap's **position** cannot be recovered from `glyph_range` because glyph indices stay contiguous.

Pair table consequences that a blank must not inherit (`break_class.rs` L27–40):

- `Al`/`Other` do not break against each other. `"AA"` is one unbreakable run. Document test (`tests/document.rs` L520–535): frame 24 pt, indent 12, text `"AA"` → `UnbreakableWiderThanFrame` with `avail` near 12 (`W'`), not 24.
- `Id` breaks against `Id`. `"第第第"`, indent 12, frame 24 (`tests/document.rs` L463–480`): first visual line is one `第` at x = 12; the next page's first line is `第第` at x = 0. Pagination did not re-indent.
- `Op` row is all `false`: no break after an opening delimiter.
- `Id`/`Al`/`Other`/`Sp`/`Op` may break before `Cl` is false; break **after** `Cl` is mostly true. `"字。"` at width 1 errors (`line_break.rs` L371–379).
- `Sp` column is a legal break before a space from almost everything except `Op`. That plus the line-start drop is why spaces vanish instead of painting.

### LineBox and draw

`LineBox` (`paragraph.rs` L36–40) is `runs: Vec<GlyphRun>` only.

For broken line `index` (`L124–136`):

```text
frame_width = index == 0 ? first_width : width
frame_x     = index == 0 ? indent       : 0
line_x      = frame_x + align_x(frame_width, line.width(), style.align)
```

`align_x` (`L429–435`): Start `0`, Center `(frame_width - line_width) / 2`, End `frame_width - line_width`. First-line End therefore pins to the **original** right edge `W`, not to `W'`. Paginate test (`src/layout/paginate.rs` L356–395): Start x = indent, Center x = `indent + (W - indent - ink) / 2`, End x = `W - ink`, y = baseline. `ParagraphNode.width` stays `W`.

`glyph_runs_for_line` (`paragraph.rs` L340–394`):

- Empty `glyph_range` → **one** `GlyphRun` with `font: style.font` (body, even if the paragraph is bold), empty `glyphs`, empty `text`, origin `(line_x, baseline)` (`L347–355`). Used for empty source, hard-break holes, and the extra line after a dropped overflow space.
- Non-empty: walk `measured.glyphs[glyph_range]` and split where `FontId` changes. Each run's `origin.x` starts at `line_x` and then accumulates `advance_norm * font_size` (`L359–391`). `GlyphPlacement.byte_range` is rebased to **that run's** `text` substring of `glyph_text`. No extra x is inserted. A cluster with mixed face tags can become two runs; the breaker still treated it as one atom.

`fragment_from_lines` (`L249–268`) is the only paragraph command emitter. For each run, `y = index * line_height + run.origin.y` (the stored y is the line-local baseline). Pushes `FragmentCommand::Glyphs` only. Fragment `height` is `lines.len() * line_height`. `spacing_after` is the style value when the whole paragraph fits (`L171`); a partial head passes `0.0` (`L178`) and the tail keeps `spacing_after`.

`ParagraphNode::split` (`L147–190`): vertical only. `line_height > full_height` → `UnsplittableTallerThanFrame`. Not enough height for one line → `MoveToNextPage` of the same node (baked x included). Otherwise slice with `lines.split_off`. Tail lines keep their origins. The original line 0 stays indented if it already fitted on a previous page; the continuation page's first line is whatever line index it already was.

`draw_fragment` (`flow.rs` L347–375`) adds the fragment origin and dispatches `Glyphs` → `Canvas::draw_glyph_run`, `Stroke` → `Canvas::stroke_path`. `Fragment::with_offset` (`L63–77`) translates glyph origins **and** stroke/fill paths. Paragraphs never produce a stroke, but the offset/draw path already would if a `Stroke` were in `commands`.

`GlyphRun` (`canvas.rs` L114–132`): origin is the first glyph's baseline, y down. `advance_norm` is **not** multiplied by font size; user-space width is `advance_norm * font_size`. `RecordingCanvas::draw_glyph_run` records `DrawGlyphRun` even when `glyphs` is empty (`L347–350`). krilla `draw_glyph_run` (`src/backend/krilla_canvas.rs` L286–303`) resolves `FontId` to bytes and pushes a glyph mark; it does not remeasure.

`single_line_run` (`paragraph.rs` L194–221`) is a **different** constructor of the same error. It does not call `break_lines`, does not read `first_line_indent`, and forces `origin.x = 0` after `glyph_run_for_line` computed alignment (`L220`). Width is the whole string's `advance_norm * font_size`. If `width > max_width`, `UnbreakableWiderThanFrame { width, avail: max_width }`. Callers: list markers with `marker_width` (`src/layout/list.rs` L207–209`; test L451–468 expects `avail` 10 pt) and layer text with `max_width = f32::INFINITY` (`src/layout/decorate.rs` L201), so layers do not trip it.

### Shared error variant

`Error::UnbreakableWiderThanFrame { width, avail }` (`src/error.rs` L73–79). Display (`L157–159`): `"unbreakable content width {width} exceeds available width {avail}"`. Doc comment says the width is "from the current line start through the overflowing cluster". That sentence matches `pack_lines` only.

| Site | `width` | `avail` | Notes |
| --- | --- | --- | --- |
| `pack_lines` `line_break.rs` L177–180 | prefix through overflowing atom | `LineBudget::for_line(records.len())` (`W'` or `W`) | Paragraph wrap. No `StartPage` yet if this fires inside the first `TextNode::split`. |
| `single_line_run` `paragraph.rs` L204–207 | full measured advance | caller `max_width` | No clusters. List marker / layer. |
| `HLineNode::split` `src/layout/hline.rs` L47–50 | rule width | `avail.width` (frame) | Not a cluster. Height is separate (`UnsplittableTallerThanFrame`). |
| `ImageNode::split` `src/layout/image.rs` L33–36 | image width | `avail.width` | Not a cluster. |
| `measure_band` `decorate.rs` L270–272 | *(same fields)* | *(same)* | **Remaps** this error to `DecorationWiderThanFrame`. A paragraph overflow inside a header/footer does not surface as `UnbreakableWiderThanFrame`. |

`InvalidGeometry` is the other paragraph failure: bad frame, `indent >= W`, bad cluster width, bad line-height factor. It is not an overflow.

## Flow

```text
TextNode { runs: [TextRun { text, Face }], Style { font, font_size, first_line_indent, align, ... } }
  layout_runs(W):
    reject W <= 0 or W' = W - indent <= 0     → InvalidGeometry
    cluster_tagged(runs)                      → ClusteredText + per-scalar Face tags
    measure_text per contiguous FontId        → InlineGlyph tape (BK omitted)
    line_metrics(style.font)                  → one line_height, one baseline for every line
    break_lines(clustered, LineBudget { first: W', rest: W }, cluster_width_pt):
        project_atoms: 1 cluster = 1 Atom; width = Σ glyph advances (BK has no width)
        pack_lines: drop leading U+0020; greedy <= avail; pair-table retreat; else UnbreakableWiderThanFrame
    per BrokenLine index:
        line_x from indent/align using line.width()
        glyph_runs_for_line: contiguous glyph_range → Vec<GlyphRun> (empty range → one empty body run)
        LineBox { runs }                      x,y line-local; y = baseline
  ParagraphNode { lines, line_height, width: W }
  split(frame): slice lines vertically; do not re-break; do not re-indent
  fragment_from_lines: y += index * line_height; FragmentCommand::Glyphs only
  draw_fragment → Canvas::draw_glyph_run
```

§2.2 requirements against this pipe:

| Requirement | What exists | Seam |
| --- | --- | --- |
| Blank is not measured as glyphs | `measure_text` cmaps every scalar in `glyph_text`. Tags and `InlineGlyph` are 1:1 with those scalars. BK is the only cluster omitted from `glyph_text`, and its width is not queried. | Keep the blank out of `glyph_text` / `glyph_tags` / `measure_text`. An empty `glyph_range` currently contributes width **0** via `cluster_width_pt`. |
| Fixed blank is unbreakable | Atoms do not split internally. Overflow with no legal break before the atom errors. Line-start overflow does not wrap onto `rest`. | Model the blank as one atom whose width is the fixed pt length, **not** `PairClass::Sp` (line-start `Sp` is deleted, `line_break.rs` L141–150). Edges need an explicit break policy; see Non-Obvious. |
| Fill-to-EOL consumes `W_remain` | No remainder consumer. `width_of` runs before packing and returns a constant per cluster. `avail` is the full line budget, not `avail - line_width`. | Width must be computed inside `pack_lines` as `budget.for_line(records.len()) - line_width` (after leading-`Sp` drops). Include that width **in** the line (unlike `Hard`, which is excluded). Assembly already forbids a following run; the breaker does not check that. |
| Blank does not increase line height | `line_height` is body `line_metrics` only. Fragment height is `n * line_height`. | Emit `FragmentCommand::Stroke` without adding thickness, `space_before`, or `space_after` into `line_height` / `Fragment.height`. Block `HLine` **does** add `space_before + thickness + space_after` (`hline.rs` L39–41, L70–74). Do not reuse that height formula. |
| Draw one horizontal stroke | Paragraph emitter is glyphs only (`paragraph.rs` L260). `FragmentCommand::Stroke { path, paint, stroke }` already exists (`flow.rs` L87–91). `Path::line` (`geom.rs` L626–628`) and `Stroke::new(Length<Positive>)` (`geom.rs` L523–530`) are what `HLine` uses. | Store a line-local segment on `LineBox` and shift y by `index * line_height` in `fragment_from_lines`, the same way glyph baselines move (`paragraph.rs` L255–260`). `with_offset` / `draw_fragment` already translate and stroke. |
| Wider than current line `W` → `UnbreakableWiderThanFrame`, no split, no clip | `pack_lines` already returns that variant and does not clip. `avail` is `W'` on visual line 0 and `W` after any record exists. | A fixed blank at the **start** of the first line, with `W' < blank ≤ W`, errors with `avail = W'` (`line_break.rs` L176–180 and L507–526). It is not moved onto a continuation line. |
| Cross-page follows the line | `split` slices `LineBox`es. x is already baked. | Bake the stroke's x and width into the line before `split`. A fill's `W_remain` is a horizontal decision and must not be recomputed on the next page. |

## Files Read

- `docs/提案书.md` L192–206 (§2.1 `W` / `W'`), L241–268 (§2.2).
- `src/layout/paragraph.rs` (full).
- `src/text/line_break.rs` (full, including tests L310–543).
- `src/text/measure.rs` L1–121 (measurement and `line_metrics`; tests not behavior).
- `src/text/cluster.rs` (full).
- `src/text/break_class.rs` L1–105.
- `src/text/mod.rs`.
- `src/layout/style.rs` (full).
- `src/layout/flow.rs` L46–96 (`Fragment`, `FragmentCommand`, `with_offset`), L154–170 (`FlowNode::split` dispatch), L347–375 (`draw_fragment`).
- `src/error.rs` L73–79, L157–159.
- `src/canvas.rs` L99–132 (`GlyphPlacement`, `GlyphRun`), L330–350 (recording stroke + glyph run).
- `src/fonts.rs` L126–136 (`metrics`, `bold`).
- `src/geom.rs` L520–530 (`Stroke`), L626–628 (`Path::line`).
- `src/backend/krilla_canvas.rs` L286–303 (`draw_glyph_run` resolves font bytes, no measure).
- `src/layout/hline.rs` L39–76 (shared error + stroke command; height formula contrast only).
- `src/layout/image.rs` L29–36 (shared error).
- `src/layout/list.rs` L207–212, L451–468 (`single_line_run` overflow).
- `src/layout/decorate.rs` L196–206, L270–272 (layer text; band remaps the error).
- `src/layout/paginate.rs` L356–395 (indent alignment box).
- `tests/document.rs` L463–536 (indent wrap, `InvalidGeometry` when indent equals frame, overflow `avail ≈ W'`).

## Boundaries

- **Fonts.** `measure_inline` / `measure_text` are the only skrifa calls on this path. They need a real `FontId` and a cmap entry. Line height reads `FontStore::metrics(style.font)` only. Bold is a second id on `InlineGlyph.font` / `GlyphRun.font`. A blank has no `FontId`, no glyph id, and must not call `measure_text`.
- **Canvas.** Layout stops at `FragmentCommand`. `draw_glyph_run` paints `GlyphRun`s. A blank underline is `stroke_path` of a `Path::line`. krilla stays behind `Canvas`. Empty glyph runs are still recorded.
- **Breaker vs paragraph.** `break_lines` does not know indent, alignment, fonts, or strokes. It sees `ClusteredText`, a `LineBudget`, and a width callback. Indent is applied twice, consistently, in `layout_runs`: once as `LineBudget.first = W - indent`, again as `frame_x` / `frame_width` when placing line 0. `pack_lines` uses `records.len()`; placement uses the emitted line's `enumerate` index. Those match as long as every emitted record becomes one `LineBox`, including empty ones.
- **Vertical split.** `ParagraphNode::split` and pagination do not call `break_lines`. `W` is fixed at the first `TextNode::split`.
- **Not this path.** `single_line_run` (list marker, `behind`/`front` layer text) never breaks and never indents. Cell/band plain strings still become `TextNode` runs; a blank only participates if it is in the run list that `layout_runs` receives. Block `HLine` shares the error variant and the stroke command type, then diverges on height.
- **Error identity.** Paragraph overflow, marker overflow, image overflow, and `HLine` overflow are the same enum variant with different field meanings. Header/footer measurement rewrites it.

## Non-Obvious Things

1. **Atom index is cluster index.** `project_atoms` is 1:1 (`line_break.rs` L113–128). `eof_cursor`'s `Hard` arm indexes `text.cluster(atom_index - 1)` (`L264–265`). Inserting a blank atom between clusters desynchronizes that arm unless `Hard` also stores a cluster cursor. `emit_line` already uses stored cluster indices for `Ink` (`L241–251`), so a blank that is not a cluster cannot be an endpoint of a `LineRecord` cursor span without a new representation.

2. **Glyph ranges have no holes.** Omitting the blank from `glyph_text` makes the scalars on either side adjacent. `BrokenLine::glyph_range` is one half-open interval (`L297–299`). `glyph_runs_for_line` places that interval as a solid advance sum (`paragraph.rs` L359–391). A middle blank (`金额：` + fixed gap + `元`) is invisible to that walk: `元` is drawn immediately after `：`. The gap has to be applied while walking **atoms / segments**, by splitting the glyph range and adding `blank_width` to `x` between the two `glyph_runs_for_line` calls. `LineRecord` does not store those segments; they die in `pack_lines`. `line.width()` can hide one gap's size (ink sum vs recorded width) but not its position, and not two gaps.

3. **`PairClass::Sp` is the wrong token.** Line-start spaces are deleted and contribute neither width nor glyphs (`line_break.rs` L141–150, test L398–403). A trailing space that does not fit is broken off, then deleted, and **an extra empty visual line is emitted** (`L187–192`, test L406–411). That empty line still has full `line_height` and an empty `GlyphRun`. A fixed or fill blank must not take this class. Ideographic space is `Id` and is kept, which is also the wrong model (it is a real glyph and a legal break on both sides).

4. **Unbreakable at line start never tries the next budget.** `nearest_break` cannot return a split at `line_start` (`L197`). A fixed blank wider than `W'` on visual line 0 errors even when `rest` (`W`) would hold it. `avail` in the error is `W'`. After a legal break **before** the blank, the blank is retested at the start of the next line against `rest`. "不可拆" is already true of every atom; "may wrap before it" depends entirely on `allows(prev, blank_class)`. There is no pair class that means "always a break opportunity on both sides, but do not drop me, and do not look me up in the cmap." `Op` forbids a break after it (`break_class.rs` L35). A blank with no pair class is skipped by `ink_pair` (`line_break.rs` L211–215`), which **glues** the neighboring inks to the blank so you also cannot break immediately before or after it.

5. **Fill width is not knowable in `project_atoms`.** `width_of` is a pure function of the cluster (`line_break.rs` L80, L118). `W_remain = avail - line_width` depends on which visual line the blank reached (`first` vs `rest`), on ink already accepted, and on spaces already discarded. A precomputed giant width would overflow, retreat, and then error at the start of the next line instead of stretching. The fit test is `line_width + width <= avail` (`L161`). Setting the fill's width to `avail - line_width` makes it fit exactly and, if it is last, the EOF emit includes it. `Hard` is not a template: `Hard` is excluded from the line's cursor range (`L155`). The fill must be inside the line. If `line_width` already equals `avail`, `W_remain` is 0; `Path::line` of a zero-length segment is not rejected.

6. **Fill interacts with alignment only through `line.width()`.** Alignment runs **after** break (`paragraph.rs` L125–127). If the recorded width includes the fill and equals `frame_width`, `align_x` is 0 for Start, Center, and End. Text before the fill sits at `frame_x` (indent on line 0); the stroke covers the rest of `[frame_x, frame_x + frame_width]`. On line 0 that right edge is `W`. Center/End do not keep a centered ink block with a fill in the leftover visual gap; the leftover was already consumed as break width. First-line End **without** a fill still ends at `W` (`paginate.rs` L377).

7. **Indent is "first emitted record," not "first record that has ink."** A leading `\n` emits an empty line 0 while `records` is empty, so the empty line consumes `W'` and is placed at `frame_x = indent`. The following text line is index 1: full `W`, `frame_x = 0` (`line_break.rs` L154–158 together with `paragraph.rs` L125–126). Leading U+0020 does **not** do this, because those spaces are skipped without emitting. `tests/document.rs` covers indent on a normal first ink line, not a leading BK.

8. **Empty runs are real draw commands.** `break_lines("")` is one zero-width line (`line_break.rs` L84–97, test L452–457). `glyph_runs_for_line` still builds a body `GlyphRun` (`paragraph.rs` L347–355`). With indent, that empty run's x is `indent + align_x(W', 0, align)` (End → x = `W`). `layout_runs` never returns `lines.is_empty()`; the `split` branch at `paragraph.rs` L151–156 is unreachable for this constructor. A blank-only line also has an empty glyph range, so it would hit that same empty-run branch **and** need a stroke; the empty run does not reserve width.

9. **Measure failures happen before overflow, and they see dropped characters.** `measure_inline` finishes before `break_lines` (`paragraph.rs` L109–118). A missing space glyph is `MissingGlyph`, not a successful drop. Bold slot lookup happens before the glyph loop (`L286–292`). Line height does not depend on which faces the line actually used, so a bold-only paragraph is still the body face's `line_height` / baseline. An underline "略下方" of that baseline is not clamped to `line_height - baseline`. A stroke past the line box paints into the next line; fragment height does not grow; nothing clips.

10. **`width` on the error is the prefix, not the blank.** `{ width: line_width + overflowing_atom, avail }` (`line_break.rs` L177–179). For a blank at line start these are equal. For `金额：` + over-wide blank, if the pair table cannot break between the colon (`Cl`) and the blank, `width` includes the glued text. `Cl` → next is allowed for several classes (`break_class.rs` L38) and forbidden before another `Cl`. The variant cannot say "this was a blank."

11. **Baked geometry survives pagination.** Document test L463–480: indented first line stays on page 1 at x = 12; page 2 starts at x = 0 with the already-broken tail. `split` copies `LineBox` (`paragraph.rs` L175–184). Fill width and stroke x have to be in that struct. Re-running `break_lines` on the tail would reapply `LineBudget.first` and indent the continuation.

12. **Tests that only scrape `DrawGlyphRun` will not see a blank.** `RecordingCanvas` stores strokes as `CanvasCommand::StrokePath` (`canvas.rs` L330–336). `draw_fragment` already emits them (`flow.rs` L365–369). Glyph-only assertions stay green if the stroke is missing.

## Open Questions

- **§2.2 "宽于当前行 `W`" vs `W'`.** Implemented `avail` for line 0 is `W - indent`. A fixed blank with `W' < width ≤ W` at the start of the paragraph errors and does not wrap (`line_break.rs` L507–526 pattern). Data-shape note "wider than current frame `W`" (`.audit/knife-14/data-shape.md` L29) can be read as the full frame. The code path that matches today's overflow is: compare against the budget of the line the atom actually starts on.
- **Break opportunities beside a fixed blank.** Not specified. Needed so `nearest_break` can place the blank on the next line when the remainder is short but `W` would fit it, without gluing it to the previous `Op`/`Cl` run and without using `Sp`.
- **Where the segment list lives.** `BrokenLine` would have to grow non-glyph spans, or `layout_runs` would keep a side tape and replay it against each line's atom range. `glyph_range` + one `f32` is not enough for a middle blank (item 2).
- **`W_remain == 0` and a zero fixed length.** Fit allows width 0 (`L161`). Zero-length `Path::line` is not `InvalidGeometry`. §2.0 non-negative lengths allow 0; stroke **width** is `Length<Positive>` and cannot be 0 (`geom.rs` L520–530). The horizontal **span** of the blank is a different number from the stroke thickness.
- **Fill after a line that is already full**, and fill as the only item on an empty line produced by a leading BK (that empty line already consumed `first`). Spec does not say whether the fill then uses `rest`.
- **Underline y.** Baseline is `ascent` (`measure.rs` L119). Offset is "略下方" of that, in a y-down system. No code clamps the stroke to the line box. Thickness must stay out of `line_height` or the "does not increase line height" rule breaks.
- **Empty `GlyphRun` next to a blank-only stroke.** The empty-range branch always emits a body run (`paragraph.rs` L347–355`). Harmless for hit-testing today (no hit testing), but record dumps will show a zero-glyph run beside the stroke.
- **Following-run check.** "吃到行尾的 blank 后面禁止再有 run" is specified as assemble-time. `pack_lines` will still place later atoms if they appear, and may error or wrap them. Layout does not re-check.
