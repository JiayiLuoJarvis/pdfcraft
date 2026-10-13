# Header / footer vs body fragment paint

Baseline: `feat/inline-blank` @ `a232ce1`. Authority: `docs/提案书.md` §8（约 616–619 行）：文字 URI 范围是「正文（及页眉页脚若同管线）」。

结论：页眉/页脚（含每个 section 自己的带）在**测量和绘制**上已经走正文同一条 `TextNode` → `ParagraphNode` → `FragmentCommand` → `draw_fragment` 管线。正文一旦把 URI 做进这条管线（挂在 `InlineItem`/`TextRun` 上，由 `fragment_from_lines` 产出，由 `draw_fragment` 调 `Canvas::annotate_link`，并在 `Fragment::with_offset` 里平移），带内的 `TextNode` 会一起画出链接注解。

调用方今天**挂不上**这个 URI。`Band` 只有整段字符串的 `left` / `right`，不经过 `Paragraph`。`behind` / `front` 的 `text_at` 是另一条单行路径，不在 §8 的「同管线」里。

## Components Found

- **文档 / section DSL**（`src/document.rs`）
  - `Document::header` / `footer`（270–286）只写隐式 section；已 `section()` 之后再调会 `ContentOutsideSection`。
  - `Section::header` / `footer`（114–126）各保存一个 `BandHook`。后一次调用替换前一次。`Section::lower`（142–168）把 hook 收成 `Decorations::{header,footer}: Option<BandContent>`。
  - `Band`（1184–1248）：`left` / `right` 收 `String`。`lower` 各做一个 `FlowNode::Text(TextNode::new(text, style))`，对齐分别是 `TextAlign::Start` / `End`。左右 margin 非 0 时插入空的定宽栏，把槽缩进到正文左右缘；多于一轨则包成 `FlowNode::Columns`（gap `NonNegative::ZERO`）。两槽都空则返回空 `Vec`。
  - `band_content`（1251–1260）：`BandContent = Fn(&PageInfo) -> Result<Vec<FlowNode>>`。每页调用 hook，再 `Band::lower`。
  - 正文段落是另一处构造：`Flow::paragraph`（679–689）→ `TextNode::from_items(paragraph.items, style)`。`Paragraph`（779–843）只有 `text` / `bold` / `blank*` / `first_line_indent`，没有 link。
  - `Layer::text_at`（1285–1291）→ `LayerItem::Text(LayerText { text, at, style })`。不是 `TextNode`。仓库里没有 `absolutePosition` 实现（只出现在 `AGENTS.md` 与提案书对照表）。

- **带测量**（`src/layout/decorate.rs`）
  - `measure_bands`（101–166）：正文分页之后，按 `SectionSpan` 逐物理页填 `PageInfo`（全文页码 + section 页码 + 纸张 + 四边 margin），再 `measure_band`。
  - `measure_band`（248–275）：`check_content(Container::Band, …)`，然后 `FlowNode::Stack(nodes).split`。框宽 = **整页宽**，框高 = **整页高**（不是 margin）。只要 `Split::Fit` 且 `height <= margin` 才留下 `Fragment`。原点稍后由调用方设成 `(0, 0)`（页眉）或 `(0, page_h - bottom)`（页脚）。
  - `measure_layer`（185–246）：`LayerItem::Text` 走 `single_line_run`，直接 `FragmentCommand::Glyphs`。不调用 `TextNode::split`。

- **正文分页与统一绘制**（`src/layout/paginate.rs`）
  - `plan_pages`（120–203）只拆 body，原点是 `body_origin`（left, top）。
  - `paginate_sections`（93–117）先 `plan_pages`，再 `measure_bands`，再 `paint_pages`。
  - `paint_pages`（266–278）顺序：`behind` → `header` → `page.placed`（正文）→ `footer` → `front`。每一块都是 `draw_fragment(&placed.fragment, canvas, placed.origin)`。

- **片段**（`src/layout/flow.rs`）
  - `FragmentCommand`（81–96）：`Glyphs` / `Fill` / `Stroke` / `Image`。没有链接。
  - `FlowNode::split`（155–169）：`Text` 与 `Paragraph` 都进段落实现。
  - `Fragment::with_offset`（63–77）平移 Glyphs origin、Fill/Stroke path、Image rect。
  - `draw_fragment`（347–375）把 origin 加到命令上，只调 `draw_glyph_run` / `fill_path` / `stroke_path` / `draw_image`。`layout/` 全树没有 `annotate_link`。

- **段落**（`src/layout/paragraph.rs`）
  - `InlineItem`（37–40）= `Text(TextRun)` | `Blank`。`TextRun` 只有 `text` + `Face`。
  - `TextNode::split`（116–118）→ `ParagraphNode::layout_runs` → `ParagraphNode::split`。
  - `fragment_from_lines`（308–343）把行盒收成 `FragmentCommand::Glyphs` 和填空 `Stroke`。
  - `single_line_run`（253–280）只给层用：不换行，超宽即 `UnbreakableWiderThanFrame`。

- **栏偏移**（`src/layout/columns.rs` 164）：每栏 `fragment.with_offset(x, 0.0)` 后并进父片段。页眉左右槽走这里。

- **内容矩阵**（`src/layout/lower.rs` 30–67）：`Container::Band` 允许 `Text` / `Paragraph` / `Spacer` / `HLine` / `Stack`（递归）/ `Columns`（子节点再按 Band 查）。拒绝 `List`、`Table`、`Image`、`PageBreak`、`KeepTogether`。公共 `Band` 实际只发出 `Text` 和（有缩进或双槽时）`Columns`。

- **注解底座已有，layout 未接**
  - `canvas::LinkTarget::uri`（`src/canvas.rs` 30–43）拒绝空串（`Error::EmptyLink`）。`Canvas::annotate_link`（207–210）。
  - `backend/krilla_canvas.rs` 把注解排队；`backend/emit.rs` `to_annotation`（199–206）写成 krilla `LinkAnnotation` + `LinkAction`。只存 URI 字符串。

## Flow

1. `document()` 闭包只组装 body 与 hook，不画页眉（`src/document.rs` 1–5、42–49）。
2. `Section::lower` 把 body 收成 `FlowNode::Stack`，把 header/footer hook 收成 `BandContent`（142–168）。每个 section 一份 `Decorations`。
3. `plan_pages` 在正文框（宽 = 页宽 − left − right，高 = 页高 − top − bottom）里 `split`。页眉不参与，也不推动正文（`paginate.rs` 72–73 的注释；`decorate.rs` 测试 `body_coordinates_are_identical_with_and_without_decorations`）。
4. 分页完成后 `measure_bands` 才跑 hook。`PageInfo` 里全文 `number`/`count` 与 section `section_number`/`section_count` 都从 1 起（`decorate.rs` 130–137；`Page` 1142–1181）。页码是闭包里 `format!` 进字符串，再当普通 `TextNode`。没有单独的页码绘制器。
5. `Band::lower` → `TextNode::new`（或 Columns 包若干 `TextNode`）→ `measure_band` 的 `Stack::split` → `TextNode::split` → `layout_runs` / `fragment_from_lines`。与 `Flow::paragraph` 的 `TextNode::from_items` 在 `split` 之后汇合。
6. 带高检查用 `Split::Fit` 的 `height`（行盒高度，不含 `spacing_after`）。默认 style 的 `paragraph_spacing` 是 0（`document.rs` 591–600）。超高 → `DecorationTallerThanMargin`；超宽（栏内不可断）→ `DecorationWiderThanFrame`（`decorate.rs` 264–272）。失败发生在 `start_page` 之前，canvas 仍空（`document.rs` 1754–1764）。
7. `paint_pages` 用同一个 `draw_fragment` 画带和正文。页眉片段原点 y=0、x=0；页脚 y = `page_h - margin.bottom`。栏内 x 已在 `with_offset` 里加过。

`FlowNode::Paragraph` 是换行之后的剩余行盒（`paragraph.rs` 205–247），不是公共 `Paragraph` builder。带不会从 DSL 直接造出它；多行 `TextNode` 在 `split` 内部会变成它。带的框高是整页，所以「比 margin 高、比页矮」走 `Fit` 然后被高度检查拒绝，而不是 `Partial`。比整页还高才变成 `Partial` / `MoveToNextPage`，再被映射成 `DecorationTallerThanMargin { height: page_h }`（`decorate.rs` 266–269）——报出的 `height` 是页高，不是内容高。

## Files Read

- `docs/提案书.md` §2.10–2.12、§5、§7、§8（约 486–544、596–619）
- `src/document.rs`：模块说明、`Section`/`Document` header/footer、`Flow::paragraph`、`Paragraph`、`Band`、`band_content`、`Layer::text_at`、`default_style`、零 margin 测试
- `src/layout/decorate.rs`：`measure_bands` / `measure_band` / `measure_layer`
- `src/layout/paginate.rs`：`PaginateConfig`、`paginate_sections`、`plan_pages`、`paint_pages`
- `src/layout/flow.rs`：`Fragment`、`FragmentCommand`、`FlowNode::split`、`with_offset`、`draw_fragment`
- `src/layout/paragraph.rs`：`TextNode`、`ParagraphNode::split`、`fragment_from_lines`、`single_line_run`
- `src/layout/lower.rs`：`check_content(Container::Band)`
- `src/layout/columns.rs`：`with_offset(x, 0)`
- `src/layout/style.rs`、`src/canvas.rs`（`LinkTarget`、`GlyphRun`、`annotate_link`）、`src/backend/emit.rs` `to_annotation`
- `src/geom.rs` `Margins::default` = 四边 0（467–470）
- `tests/document.rs` `full_template`（137–148）：页码是 hook 里格式化的字符串

## Boundaries

| 路径 | 是否 `TextNode` → `fragment_from_lines` → `draw_fragment` | §8 外链若做进该管线 |
| --- | --- | --- |
| 正文 `Flow::paragraph` | 是（`from_items`） | 这就是「正文接线」 |
| section / 文档 header、footer | 是（`TextNode::new`，常外包 `Columns`） | 绘制会自动发生 |
| `behind` / `front` `text_at` | 否。`single_line_run` 直接 `Glyphs` | 不会自动有 |
| 流上的 `absolutePosition` | 代码里不存在。页坐标逃逸是 `Layer` | 不在本条 |

带和正文的差别在**框和失败策略**，不在画文字的函数：

- 正文可 `Partial` 跨页。带必须单次 `Fit` 且行盒高度 ≤ 对应 margin，否则整份文档失败，不裁切、不回流正文（提案书 §2.10；`measure_band`）。
- 带的外框宽是整页宽。与正文左右对齐是 `Band::lower` 用空栏缩进，不是把 `TextNode` 的框设成 `body_width`。
- 带的 style 在 `Section::lower` 时从 `flow.style` 克隆，再按槽改 `align`。公共 API 不能在带里 `bold`、缩进或 `blank`。
- `Container::Band` 拒绝 image / list / table / page break。图上链接、单元格链接不经过带。

## Non-Obvious Things

- **默认 margin 0 = 没有带高。** 非空 `left`/`right` 且 top 或 bottom 为 0 → `DecorationTallerThanMargin { margin: 0 }`，且不 `start_page`。hook 存在但两个槽都是 `None` 时，`lower` 得到空 vec，`measure_band` 在高度检查之前返回 `None`（`decorate.rs` 254–256），不报错。`left("")` 不同：会造出空 `TextNode`（`paragraph.rs` 97–104），空行 `split` 为 height 0 的 `Fit`（209–214），0 ≤ margin 0，留下一个空片段。这是读代码的推论，没有对应测试。
- **页码没有专用绘制。** `Band::page()` 在每页 hook 里可读；测试与 `full_template` 都是 `format!` 后 `left`/`right`。链接若要打在「第 n 页」上，和打在任意页眉字符串上是同一件事，而且每页可以不同（hook 在 `measure_bands` 里按页跑）。
- **左右槽的链接矩形要过两段平移。** 栏内 `with_offset(x, 0)`，再在 `draw_fragment` 里加 `PlacedFragment.origin`（页眉 y=0，页脚 y=`page_h-bottom`，x 都是 0）。今天 `with_offset` 没有链接臂。新的 `FragmentCommand` 若只在 `draw_fragment` 里处理、不在 `with_offset` 里平移，双槽或带左右 margin 的链接矩形会停在栏局部坐标。
- **注解与字形 z 序无关。** 绘制顺序是 behind → header → body → footer → front，但链接是 `annotate_link` 侧通道，不是 `GlyphRun`。backend 已能在当前页上排队。layout 还没有任何调用。
- **缺字在带里同样整份失败**，且在画页之前（`document.rs` 1693–1704，`MissingGlyph`）。与正文同一测量（`prepare_inline` / charmap）。
- **空 section 仍画该 section 的 footer**（`tests/document.rs` 中 `empty_section_is_one_page_and_still_draws_its_footer`）。带不依赖 body 里有节点。

## Open Questions

- §8「文字上可挂 URI」若是**行内一段**而不是整段：`Band::left`/`right` 是整槽一个 `TextNode::new`。即使 `TextRun` 上有 URI 字段，还要改 `Band` 的构造（或让槽接受和 `Paragraph` 相同的 `InlineItem`）。只给 `Paragraph::text` 加 link，页眉调用方到不了。
- 链接矩形是字形 ink 还是行盒，代码里还没有。`GlyphRun` 有 `origin`、`glyphs`、`text`，没有现成的 link rect（`canvas.rs` 119–132）。算矩形的位置若放在 `fragment_from_lines`，带与正文共用；若只放在 `Flow::paragraph`，带用的 `TextNode::new` 绕开它。
- `front` 盖住页眉时，页眉链接注解是否仍可点：提案书叠层是绘制顺序，没说注解。当前没有实现可对照。
- 层文字（水印、电子章）要不要外链：§8 写的是正文及同管线的页眉页脚。`text_at` 不走 `fragment_from_lines`。若也要，需在 `measure_layer` 另接，不能靠正文接线自动出现。
