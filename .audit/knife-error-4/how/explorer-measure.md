测量和排版失败都收成一个 `Error`。`?` 立刻停掉当前这条路径，后面的字形、节点和页都不再量。公共入口 `document()` 只返回这一个错误，失败时不会交出 `Rendered`。

### Components Found

- **错误类型**（`src/error.rs`）。`Result<T>` 就是 `std::result::Result<T, Error>`（590）。`Error` 带 `kind`、可选 `Location`（`section` / `page` / `region` / `cell`），以及可选 `source`。`From<ErrorKind>` 和 `From<GeometryKind>` 建出来的错误 `at` 都是 `None`（67–80）。外层用 `get_or_insert` 补位置，已经写过的字段不覆盖（34–59，516–518）。没有 `Vec<Error>`。
- **测量**（`src/text/measure.rs`）。`measure_text`（41–102）先 `fonts.face`，再 `parse_face`，再按 `char_indices` 逐个标量查 cmap。`charmap.map` 得到 `None` 就返回 `MissingGlyph`（61–66）。注释写明不画 `.notdef`、不回退（1–3）。advance 缺失是 `ErrorKind::Font`，没有 `source`（74–80）。`parse_face` 失败是 `Font`，`source` 是 skrifa 的错误（`src/fonts.rs` 69–77）。`line_metrics` 在 factor 非有限或 `<= 0` 时返回 `GeometryKind::LineHeightFactor`（104–111）。
- **字体查找**（`src/fonts.rs`）。`face` / `face_mut` 下标越界是 `UnknownFont { id }`（145–155）。`bold()` 在 `bold: None` 时是 `BoldFaceRequired`（141–143）。`set_bold` 不检查 id 是否已注册（137–139）。`Document::bold_font` 只有在 `register` 成功之后才 `set_bold`（`src/document/document_ty.rs` 52–64）。正文样式固定用 `FontId(0)`（223–232）。
- **行内测量**（`src/layout/paragraph.rs`）。`measure_inline`（539–587）先看有没有非空 bold run，有则 `fonts.bold()?`；再按连续相同 `FontId` 切段，每段 `measure_text`。`prepare_inline`（405–431）从左到右 flush。`layout_runs`（161–218）在测量之前就拒绝非法行宽和首行缩进。`TextNode::split`（128–134）先 `layout_runs` 再 `ParagraphNode::split`，所以换页之前已经量过字。
- **断行硬停**（`src/text/line_break.rs`）。`break_spans`（157–182）先校验 `LineBudget`，再拒绝「Fill 不是最后一个 span」的 `RunAfterFillBlank`。`pack_lines` 在当前行找不到断点时返回 `UnbreakableWiderThanFrame`，不继续排后面的 atom（286–307）。行首空格会跳过（250–259），那是继续，不是错误。
- **分页**（`src/layout/paginate.rs`）。`plan_pages` 逐个 `split`，错误补上 `Region::Body` 和当前页（157–161）。`y == 0` 却得到 `MoveToNextPage` 时是 `StalledOnFreshPage`（195–201）。全部 section 的正文计划完，才 `measure_bands`（116–118），然后才 `paint_pages`（278–295）。
- **页眉页脚 / 层**（`src/layout/decorate.rs`）。`measure_band` 把 `UnbreakableWiderThanFrame` 改写成 `DecorationWiderThanFrame`，把过高改写成 `DecorationTallerThanMargin`；其它错误原样传出（294–309）。改写用 `Error::from`，丢掉内层 `Location`（包括 `cell`）。`MissingGlyph` 走 `Err(other)`，位置保留。层文字走 `single_line_run(..., f32::INFINITY)`（230），宽度检查不会触发。
- **公共入口**（`src/lib.rs` 45–70，`src/document/document_ty.rs` 161–214）。`document()` 先 `build(&mut doc)?`，再 `doc.render()`。`paint` 先返回 `Document.error`，再 `section.lower`，再 `paginate_sections`。`render` 画进局部 `RecordingCanvas`；`Err` 时这块 canvas 被丢掉，调用方拿不到 `Rendered`。

### Flow

公共路径只有一个 `Result`。先返回的那个错误就是最终错误。

1. **闭包阶段**（`document()` 62 行的 `build()?`）。`Document::font` / `bold_font` / `load_image` 直接返回 `Result`。闭包用 `?` 的话，`render` 不会跑，错误没有 `Location`。`bold_font` 在没有 body font 时是 `Font { message: "body font is not registered" }`（`document_ty.rs` 52–57），不是 `BoldFaceRequired`。`paragraph`、`link`、`blank` 这类方法返回 `()`，把第一个错误 `note` 进节点；`Paragraph::note`（164–168）和 `Flow::push`（41–52）都是「已有错误就不再记」。`Flow::push` 在 `error` 已有值时直接 `return`，后面的节点连 `Err` 一起丢掉。闭包本身还是会继续执行。`lower_paragraph`（`flow.rs` 73–76）一看到 `paragraph.error` 就 `Err`，那段文字不会变成 `TextNode`，也就不会被测量。所以同一段里如果先记下了 `EmptyLink` 或 `RunAfterFillBlank`，后面的缺字不会被看到。`RunAfterFillBlank` 在公共 DSL 上是组装错误：`paint` → `lower` → `in_section`，没有 page/region（`document/tests.rs` 146–156）。`break_spans` 里还有一道同样的检查（`line_break.rs` 164–175），给不经过 `Paragraph` 的内部节点用。

2. **一次正文 `split` 里的顺序**（先失败的赢）：
   - `FlowNode::split` 先 `avail.validate()`（`frame.rs` 25–40，`flow.rs` 51–52）。非法 frame 是 `GeometryKind::LayoutFrame`。
   - `layout_runs`：`LineWidth`（167–169）→ `FirstLineIndent`（171–175）→ `prepare_inline`。缩进不合法时，缺字和超宽都不会查。
   - `measure_inline`：非空 bold run 先 `fonts.bold()?`。同一组里缺 bold face 时，`BoldFaceRequired` 先于任何 `measure_text`，也先于正文的 `UnknownFont` / `MissingGlyph`。空的 `bold("")` 在 `append` 里被丢掉（`paragraph.rs` 122–124），不会要 bold face。
   - `measure_text`：`face` 失败即 `UnknownFont`（还没解析、还没看字符）→ `parse_face` 失败即 `Font` + `source` → 标量循环。第一个 `map` 为 `None` 的标量变成 `MissingGlyph`，函数立刻 `return`（61–66）。后面的标量不再查。同一段里这个标量之前的 advance 已经写进 `FontFace.advances`（70–82），但 `paint` 失败时 `FontStore` 随 `Document` 一起丢掉，调用方看不到缓存。换行不进 `glyph_text`（`cluster.rs` 70–75、215–221），所以不会对 `\n` 做 cmap 查找。只含换行、没有 bold 的段落不会调用 `measure_text`；随后 `fonts.metrics(style.font)`（`paragraph.rs` 194）仍可能给出 `UnknownFont`。
   - 同一 flush 组按字体切片从左到右量（`measure_inline` 566–584）。前一段失败，后一段（比如后面的 bold）不会量。`prepare_inline` 也是从左到右，前一个 group 失败则后面的 blank/text 不再量。
   - 测量成功之后才 `break_spans`：`LineWidth`（预算再查一次，110–116）→ `RunAfterFillBlank` → `AdvanceWidth`（204–206、219–222）→ `UnbreakableWiderThanFrame`。然后 `line_metrics` 可能给出 `LineHeightFactor`。`place_line` 里 `Point::try_pt` 失败是 `NonFiniteLength`（533），前面已经放好的行随 `Err` 丢掉。
   - `ParagraphNode::split`：`LaidOutWiderThanFrame`（222–228）或行高大于整帧的 `UnsplittableTallerThanFrame`（236–241）。否则 `MoveToNextPage` / `Partial` / `Fit`，分页继续。`TextNode::split` 在判断能不能放下之前就已经 `layout_runs`，所以「这段其实会挪到下一页」的缺字，记在**当前被提供的那一页**上，不是下一页。

3. **节点之间**。`split_stack`（`flow.rs` 192）和 `plan_pages` 的 `?` 让第一个 `split` 错误结束循环。后面的段落、页、section 正文都不再量。列从左到右（`columns.rs` 141–147），表按格子顺序，`natural_height(...).map_err(in_cell)`（`table.rs` 258–259）后立即返回。列表先 `body.split`，成功才 `single_line_run` 量标记（`list.rs` 196–217）。条目标记用 `parent.style` 的正文字体（`document/list.rs` 94–98）。项目符号是 U+2022。正文缺字会盖住标记缺字。`Location` 没有条或列的下标。

4. **页和装饰的先后**。`paginate_sections`（97–118）：每个 section 先 `plan_pages`，错误再 `in_section`。正文错误带 `Body` + 文档全局页码（1-based，`on_page_index`）+ section。全部正文成功之后才量装饰。因此 section 1 正文的缺字会盖住 section 0 页眉的缺字。装饰按页、按 section 走；每一页的测量顺序是 header、footer、behind、front（176–185）。绘制顺序不同（`paint_page` 287–291）。某一页的 header 缺字会挡住该页 footer/层以及后面所有页的装饰。层闭包在 `measure_layer` 里才跑（`layer.rs` 272–286）；`layer.error` 变成 `Err`，然后 `single_line_run` 才可能缺字。`is_last()` 上的缺字记在最后一页的 `Front`，前面的页只存在于内存里的 `PagePlan`，还没画。

5. **装饰改写**。页眉/页脚里的 `UnbreakableWiderThanFrame` 和 `UnsplittableTallerThanFrame` 换成装饰专用 kind，内层 `cell` 丢失，再由 `at()` 补上 region、page、section（154–160）。`MissingGlyph`、`BoldFaceRequired`、`UnknownFont`、`FirstLineIndent` 不改写。`single_line_run` 在层上用 `f32::INFINITY` 做最大宽度（`paragraph.rs` 277–290），有限宽度不会触发超宽；缺字仍然会。

6. **变成公共的那一个 `Error`**。`paint`（202）`?` 到 `render`（209）`?` 到 `document()`（63）。测试期望：正文 `BoldFaceRequired` / `MissingGlyph` / `FirstLineIndent` 都是 `section 0, page 1, body`，且 `RecordingCanvas` 为空（`document/tests.rs` 200–213、294–365；`tests/document.rs` 599–607、674–681）。没有字体时正文是 `UnknownFont { id: 0 }`（`tests/document.rs` 599–607），因为样式字体是 0，`face` 在看字符之前失败。布局错误发生在 `paint_pages` 之前，所以命令列表是空的。成功排出来的前面几页也不会变成 `Rendered`。

7. **`to_pdf`（测量失败到不了这里）**。`to_pdf`（`lib.rs` 68–69）只接受已经成功的 `Rendered`。`write_pdf`（`backend/mod.rs` 24–36）`replay` 用 `?`（`canvas.rs` 381–398）。`KrillaCanvas::draw_glyph_run` 对 `font_store.resolve` 失败调用 `fail`（`krilla_canvas.rs` 187–198、286–291）：只保留第一个错误，之后的 canvas 调用返回这份克隆。`replay` 停住，`finish` 不会被调用。`finish` 若被调用且 `failure` 已有值，返回的还是那一个（162–176）。`emit::render`（30–47）按页 `emit_page`，第一个错误 `on_page_index`，后面的页和 `document.finish()` 都不跑。这里才会出现 `Font`（krilla `Font::new` 为 `None`）、`FontMetricsMismatch`、`Backend`、`GeometryKind::DegeneratePath`（`convert.rs` 35–59）。这些错误没有 section/region，除非原来就带了 page（`get_or_insert` 不覆盖）。

### Files Read

- `src/lib.rs`、`src/error.rs`、`src/fonts.rs`、`src/text/measure.rs`、`src/text/cluster.rs`、`src/text/line_break.rs`（预算、Fill、超宽）
- `src/layout/paragraph.rs`、`frame.rs`、`flow.rs`、`paginate.rs`、`decorate.rs`、`columns.rs`、`table.rs`（`measure`）、`list.rs`、`image.rs`、`hline.rs`、`nesting.rs`、`style.rs`
- `src/document/document_ty.rs`、`flow.rs`、`flow_writer.rs`、`paragraph.rs`、`section.rs`、`layer.rs`、`band.rs`、`list.rs`、`columns.rs`、`table.rs`（`lower`）、`style_props.rs`
- `src/geom.rs`（`Point::try_pt` / `Size::try_pt` / 长度品牌）
- `src/canvas.rs`（`replay`）、`src/backend/mod.rs`、`krilla_canvas.rs`、`emit.rs`、`convert.rs`（`DegeneratePath`）
- `src/document/tests.rs`（位置与空 canvas）、`tests/document.rs`（`UnknownFont`、`BoldFaceRequired`、首个 builder 错误）

### Boundaries

- **Builder 的 `note` / `Flow.error` 对测量。** 组装错误在 `section.lower` → `into_nodes` 就返回，发生在 `plan_pages` 之前。公共路径上的 `RunAfterFillBlank`、`EmptyLink`、列宽/跨格/嵌套不合法，都还没量字形。`Document.error`（`ContentOutsideSection`、section 闭包错误）在 `paint` 开头就返回（170–172），Flow 上更早的错误被盖住。
- **`font()` / `bold_font()` 对布局期的 `UnknownFont` / `BoldFaceRequired`。** 注册失败是闭包里的 `Font`，没有 `Location`。注册被忽略且闭包仍 `Ok` 时，布局才报 `UnknownFont { id: 0 }` 或 `BoldFaceRequired`，并带上 page/region/section。
- **正文计划对装饰测量。** 装饰只在所有 section 的正文都成功之后运行。层/页眉闭包也在这时才执行。
- **布局对 `paint_pages` / `to_pdf`。** 测量和几何硬停发生在 `start_page` 之前。`to_pdf` 只处理成功的 `Rendered`。后端再单独失败一次，不回头做测量。
- **位置字段。** 表的 `in_cell` 最先写上，外层只填空着的 section/page/region。装饰尺寸改写会丢掉 `cell`。列表项、列号不进 `Location`。
- **`RecordingCanvas` 对 `KrillaCanvas`。** 布局用的是 recording，`draw_glyph_run` 不查字体（`canvas.rs` 367–370）。`UnknownFont` 在后端 replay 时才会从 `resolve` 再出现。

### Non-Obvious Things

- 同一段落里的优先级是：非法行宽 / 首行缩进，然后 `BoldFaceRequired`，然后该字体切片的 `UnknownFont`，然后**第一个**缺字，然后超宽。后面的缺字不会被收集。
- 缺字报的是当前 `FontId`。bold 切片失败时 `font` 是 bold 的 id（测试里 body 为 0、bold 为 1：`document/tests.rs` 302–314）。
- `measure_text` 在失败标量之前已经缓存了 advance，但失败的 `paint` 会丢掉整个 `FontStore`。
- 会挪到下一页的段落，仍在当前页的 frame 上测量；缺字的 `page` 是这一页。
- 页眉缺字不会阻止「正文已经在内存里排完」；它会阻止 `paint_pages`。canvas 仍然是空的。
- `measure_band` 的尺寸改写用新的 `Error::from`，内层 `cell` 消失。`MissingGlyph` 不会走这条改写。
- 装饰测量顺序是 header → footer → behind → front，不是绘制顺序。
- 列表标记在**该项正文 `split` 成功之后**才测量。标记缺字没有 item 下标。
- `default_style` 直接写 `line_height_factor: 1.2`（`document_ty.rs` 223–232），不经过 `Style::new`。`line_metrics` 仍会拒绝非法 factor，但公共 DSL 没有设置入口。`Style::new` 只出现在测试辅助里。
- `finalize_eof` 在一页都没有时会推一个空白页（`paginate.rs` 260–267）。`PageCount { pages: 0 }` 不是空文档的那条路径。
- `StalledOnFreshPage` 只在 `y == 0` 且 `split` 返回 `MoveToNextPage` 时出现（195–201）。段落、图片、横线在高于整帧时自己返回 `UnsplittableTallerThanFrame`，而不是这种 stall。
- 继续排下去的是成功变体：`Split::Partial`、`Split::MoveToNextPage`（且 `y != 0`）、断行换行、`stacked_height` 为了填写「过高」的高度而接着拆（`frame.rs` 187–211）。`stacked_height` 里若后续 `split` 返回 `Err`（例如后半段缺字），传出去的是那个 `Err`，不会再包成 `UnsplittableTallerThanFrame`。

### Open Questions

- `charmap.map` 若对未映射标量返回 `Some(0)` 而不是 `None`，当前代码会把它当成普通字形。仓库里只把 `None` 定义成 `MissingGlyph`，没有对 glyph id 0 的分支，也没有对照 skrifa 的 cmap 实现核实。
- 空文档以外，`GeometryKind::PageCount { pages: 0 }` 有没有公共路径能碰到，只看到 `finalize_eof` 会补一页；`u32` 溢出那条仍在（`paginate.rs` 101–107）。