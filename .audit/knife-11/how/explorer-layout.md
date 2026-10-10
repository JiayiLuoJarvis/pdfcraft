### Components Found

Tip `9eda8da`（`feat/layer-path`）。behind / front 不进正文流。闭包在分页之后、`start_page` 之前按页执行，产出 `LayerItem`，再收成一个原点 `(0, 0)` 的 `PlacedFragment`。画的时候插进固定叠层，不另开 z 槽。

- **DSL：`Section` / `Document`**（`src/document.rs`）
  - `Section` 各持一份 `behind` / `front: Option<LayerHook>`（`69-76`）。`LayerHook = Box<dyn Fn(&mut Layer) + 'a>`（`56`）。后一次调用盖掉前一次（`128-136`）。
  - `Document::behind` / `front` 只写隐式 section；已经 `section()` 之后再写，记 `Error::ContentOutsideSection`，`paint` 开头就返回（`267-282`，`323-340`）。
  - `Section::lower` 把 hook 包成 `LayerContent`，放进 `Decorations`（`139-165`）。样式在 `lower` 时从该 section 的 `Flow` clone 进去。
  - 公共 `Layer`（`1013-1060`）只有 `page` / `is_last` / `text_at` / `image_at`。没有 path 方法。坐标注释写明是页坐标，不缩进 margin。
  - `text_at` 推 `LayerItem::Text`。`image_at` 先对 `DocToken`：外文 handle 把第一个 `Error::UnknownImage` 留在 `Layer.error` 并 return，不推 item（`1047-1055`）。
  - `layer_content`（`1063-1077`）每页新建 `Layer`，跑 hook，有 `error` 则 `Err`，否则 `Ok(items)`。
  - 公共 `Page`（`892-929`）从 `PageInfo` 来：全文 `number`/`count`，section 内 `section_number`/`section_count`，`size`，`margins`。`is_last` 比的是全文 `number == count`（`916-918`），不是 section 末页。

- **`LayerItem` / `LayerContent` / `Decorations`**（`src/layout/decorate.rs`）
  - `LayerText`：`text`、`at`（行框左上）、`style`。注释：不换行，对齐被忽略（`49-55`）。
  - `LayerItem` 两臂（`59-66`）：`Text(LayerText)`；`Image { image, rect }`。注释：页坐标矩形，不缩进、不对照页界检查。
  - `LayerContent<'a> = Box<dyn Fn(&PageInfo) -> Result<Vec<LayerItem>> + 'a>`（`68`）。
  - `Decorations` 四槽：`header`/`footer` 是 `BandContent`（流节点），`behind`/`front` 是 `LayerContent`（`70-76`）。缺省全 `None`。
  - `PageBands`（`84-90`）：四槽各 `Option<PlacedFragment>`。正文不在这里。

- **`PageInfo`**（`decorate.rs:36-45`）
  - `number` / `count`：全文，从 1 起，空白正文页算进 `count`。
  - `section_number` / `section_count`：本 span 内从 1 起。`measure_bands` 用 span 内 `offset` 填 section 序号，用全局 `index` 填全文序号（`119-132`）。

- **`measure_bands`**（`decorate.rs:96-161`）
  - 输入：每页 `Size`，以及 `SectionSpan`（起点、长度、margin、该 section 的 `Decorations`）（`paginate.rs:63-69`）。
  - span 必须首尾相接且盖满 `sizes`，否则 `Error::InvalidGeometry`（`101-113`）。
  - 每一页：header 用 `measure_band`，原点 `(0, 0)`，带高 = top margin；footer 原点 `(0, page_h - bottom)`，带高 = bottom margin（`133-151`）。behind / front 走 `measure_layer`，不走 `measure_band`。
  - 注释（`92-93`）：测量在绘制之前；失败时画布不动。

- **`measure_layer`**（`decorate.rs:180-218`）
  - `content` 为 `None` → `Ok(None)`，闭包不跑。
  - 否则 `content(page)?`，按闭包 push 顺序 match：
    - `Text`：把 `align` 改成 `TextAlign::Start`，`single_line_run(..., f32::INFINITY)`（`paragraph.rs:141-169`）。再把 `at` 加到 run 原点上。run 在 `single_line_run` 里已被改成 `(0, baseline)`，所以画出的基线 y = `at.y + baseline`，x = `at.x`。宽度用 `INFINITY`，有限行宽不会触发 `UnbreakableWiderThanFrame`。缺字在 `measure_text` 失败。
    - `Image`：原样 `FragmentCommand::Image { image, rect }`。
  - `commands` 空 → `Ok(None)`（闭包跑了但没推 item）。
  - 否则一个 `PlacedFragment`：`origin = (0, 0)`，`Fragment { height: 0.0, spacing_after: 0.0, commands }`。高度 0 表示不占正文；`draw_fragment` 不读 `height`。

- **`Fragment` / `FragmentCommand` / `PlacedFragment`**（`src/layout/flow.rs`）
  - `FragmentCommand`（`78-89`）三臂：`Glyphs`、`Stroke { path, paint, stroke }`、`Image { image, rect }`。**没有 `Fill`。**
  - `with_offset`（`62-74`）只平移 Glyphs 原点、Stroke 的 `Path`、Image 的 `Rect`。
  - `draw_fragment`（`338-364`）把 `PlacedFragment.origin` 加到每条命令上：字形 `draw_glyph_run`，描边 `canvas.stroke_path`，图 `draw_image`。没有 `fill_path` 调用。
  - 层的原点是 `(0, 0)`，所以层命令里的页坐标原样落到画布。正文片段的原点才是 `(left, top + y)`（`paginate.rs:157-160`，`175-180`）。

- **`paginate_sections` / `paint_pages`**（`src/layout/paginate.rs`）
  - 顺序（`93-117`）：每个 section `plan_pages`（只排正文）→ 收齐 `sizes` 与 `SectionSpan` → `measure_bands` → `paint_pages`。注释（`71-73`）：带在正文计划之后测，装饰不能挪正文。
  - `paint_pages`（`266-279`）每页先 `start_page(page.size)`，再链式迭代：

    `bands.behind` → `bands.header` → `page.placed`（正文）→ `bands.footer` → `bands.front`

    每个 `PlacedFragment` 调 `draw_fragment`，然后 `finish_page`。`Option` 用 `.iter()`，空槽跳过。同一层里多条命令按 `commands` 顺序画，也就是闭包 push 顺序。
  - 空白正文页仍在 `pages` 里（`placed` 空），`measure_bands` 仍按该页跑层闭包。测试 `bands_span_the_page_and_number_every_page_including_blank_ones`（`decorate.rs:291-314`）证明空白页有页眉页脚；层走同一循环。

- **已有描边生产者（对照，不是层）**
  - 表格边框：`table.rs:236-266` 在格子内容之后 `push FragmentCommand::Stroke`。`border` 不是正数则不画。矩形由私有 `rect_path`（`393-399`）用 `MoveTo` + 三边 `LineTo` + `Close` 拼出。坐标相对表片段，经正文原点平移。
  - 列表 `first_baseline`（`list.rs:258-262`）对 `Stroke` 和 `Image` 返回 `None`。加 `Fill` 臂时这个 match 必须改，否则编不过。层路径本身不经过列表。

- **画布已经能填和描，布局命令还不能填**
  - `Canvas::fill_path` / `stroke_path`（`canvas.rs:193-197`）。`CanvasCommand::FillPath` 与 `StrokePath`（`145-160`）。
  - `RecordingCanvas` 两者都记（`canvas.rs:322-330` 一带）。
  - `KrillaCanvas` 把 `Mark::Fill` / `Mark::Stroke` 推进当前页（`krilla_canvas.rs:58-62`，`252-268`）。`emit.rs:115-130`：fill 时 `set_fill(Some)` + `set_stroke(None)`；stroke 时反过来。
  - `convert.rs:52-73`：`to_fill` 用颜色 alpha 做 opacity，`FillRule::NonZero`；`to_stroke` 同样用 alpha，线宽取 `Stroke::width`，cap/join/miter/dash 留 krilla 默认。`to_krilla_path`（`36-49`）只转发 `MoveTo` / `LineTo` / `Close`。
  - 公共几何已导出（`lib.rs:12-14`）：`Path`、`PathBuilder`、`PathVerb`、`Paint`、`Stroke`、`Color`。`PathVerb`（`geom.rs:541-548`）只有 `MoveTo`、`LineTo`、`Close`。`Path::line`（`573-575`）是单段直线。没有曲线动词，没有椭圆，没有旋转矩阵。`Point` 允许负的有限坐标（`geom.rs:171`）。`Stroke` 宽度必须是 `Length<Positive>`（`520-536`），表达不了 0 宽。`Color::rgba` 已有 alpha（`489-491`）。
  - `Canvas` 没有 `push_transform`。`emit.rs` 里的 `push_transform` 只用于图片平移（`172-182`），不在公共 trait 上。

- **`lower.rs` 不碰层。** `Container` 只有 Column / TableCell / ListItem / Band（`lower.rs:11-16`）。`measure_band` 才 `check_content(Container::Band, …)`（`decorate.rs:230`）。层 item 不是 `FlowNode`。

### Flow

1. **组装（还没测量）**  
   `document` 闭包跑一遍（`document.rs:39-52`）。`doc.behind` / `doc.front` 或 `section.behind` / `section.front` 只存 hook。`text_at` / `image_at` 此时不执行。

2. **`Document::paint`**（`329-369`）  
   先抛已记下的 `self.error`。每个 section `lower` 出 `(PaginateConfig, FlowNode::Stack, Decorations)`。`paginate_sections`。

3. **正文计划** `plan_pages`（`paginate.rs:120-202`）  
   只消耗 `FlowNode`。层闭包不跑。失败（例如 `UnsplittableTallerThanFrame`）在 `measure_bands` 之前返回，画布无命令（`paginate.rs:653-675`）。

4. **每页测量层** `measure_bands` → `measure_layer`  
   对该 section 的每一物理页（含空白页）调用 behind 闭包，再调用 front 闭包。闭包里 `Layer::page()` 已是最终页码。`is_last()` 为真只在全文最后一页。  
   文本 → `Glyphs`；图 → `Image`。同一闭包内的 item 进同一 `commands` 向量。  
   页眉页脚另走 `measure_band`：流式 `split`，高度超过 margin 则 `DecorationTallerThanMargin`（`decorate.rs:221-247`）。层没有这道高度检查，也不查是否画出页外（`layer_ignores_alignment_and_page_bounds`，`decorate.rs:594-618`：`at = (190, 395)` 在 200×400 页上仍画）。

5. **绘制** `paint_pages`  
   这是这条链上唯一的 `canvas.start_page`（`paginate.rs:268`）。叠层：

   `behind` → `header` → body → `footer` → `front`

   集成测试 `header_numbers_every_page_and_layers_stack_behind_header_body_footer_front`（`tests/document.rs:165-183`）第 1 页文字顺序 `DRAFT, ACME, 页码, 正文`；第 2 页末尾才有 `SEAL`（`front` 里 `is_last`）。布局测试期望坐标（`decorate.rs:564-590`，页 200×400，margin 上 30 / 左右 20 / 下 30）：`W` 在 `(50, 120+baseline)`，`H` 在 `(0, baseline)`，正文 `A` 在 `(20, 30+baseline)`，`F` 在 `(0, 370+baseline)`，`S` 在 `(150, 300+baseline)`。

6. **失败发生在 `start_page` 之前**  
   `plan_pages` 与 `measure_bands` 都返回 `Result`，`paint_pages` 在两者都成功之后才调用（`paginate.rs:115-117`）。层闭包、缺字、`UnknownImage`、非法 span 都从 `measure_layer` / `measure_bands` 冒泡，此时 `RecordingCanvas` 仍空。  
   - 布局：`missing_glyph_in_a_layer_fails_before_any_page_starts`（`decorate.rs:622-641`），behind 里缺字，`rec.commands()` 空。  
   - 文档：`missing_glyph_in_front_fails_before_any_page`（`document.rs:1158-1172`），`front` 仅在 `is_last` 时 `text_at` 缺字，整份文档失败且画布空。页眉缺字、零 margin 页眉、超高 `keep_together` 同样在任何页开始前失败（`1143-1197`）。  
   - `Layer.error`（外文 image）在 `layer_content` 变成 `Err`，同样停在测量阶段。  
   测量阶段把所有页的 `PageBands` 收齐再画。第 2 页层失败时，第 1 页也还没 `start_page`。  
   例外：`draw_fragment` 里的 `Point::try_pt` / `Path::translated` 若失败，发生在该页 `start_page` 之后（`flow.rs:349-357`）。今天层原点是 `(0,0)`，点在测量时已经是有限 `Point`，这条平移是恒等。新路径若把非法几何留到绘制时才 `try_pt`，就会打破「失败则画布空白」。

### Files Read

- `src/document.rs`（`Layer`、`layer_content`、`Section::lower`、`Document::paint`、缺字测试）
- `src/layout/decorate.rs`（`LayerItem`、`measure_bands`、`measure_layer`、叠层测试）
- `src/layout/paginate.rs`（`paginate_sections`、`plan_pages`、`paint_pages`）
- `src/layout/flow.rs`（`FragmentCommand`、`with_offset`、`draw_fragment`）
- `src/layout/paragraph.rs`（`single_line_run`）
- `src/layout/table.rs`（`FragmentCommand::Stroke`、`rect_path`）
- `src/layout/list.rs`（`first_baseline` 对 `Stroke`/`Image` 的 match）
- `src/layout/lower.rs`（`Container` 不含层）
- `src/layout/mod.rs`、`src/layout/test_util.rs`（`strokes` 只收集 `StrokePath`）
- `src/canvas.rs`（`fill_path` / `FillPath`）
- `src/geom.rs`（`Path`、`PathVerb`、`Paint`、`Stroke`、`Color`）
- `src/backend/convert.rs`、`src/backend/krilla_canvas.rs`、`src/backend/emit.rs`（fill/stroke 已接到 krilla）
- `src/lib.rs`（公共导出）
- `tests/document.rs`（叠层与 `image_at`）
- `docs/提案书.md` §2.12（`527-543`：`line` / `rect` / `ellipse`、stroke/fill、旋转、半透明；不进流式断行）
- `src/layout/dump.rs`（空占位，不匹配 `FragmentCommand`）

### Boundaries

Knife 11 要接上的缝，按数据流方向：

1. **`Layer` 方法**（`document.rs:1022-1060`）  
   今天只有 `text_at` 与 `image_at`。`line` / `rect` / `ellipse` 的 stroke/fill 要在这里 push 新 item。外文资源错误的先例是把 `Error` 留在 `Layer.error`，由 `layer_content` 在测量时返回，从而赶在 `start_page` 前失败。`text_at` 不返回 `Result`。

2. **`LayerItem`**（`decorate.rs:59-66`）  
   只有 `Text` 与 `Image`。路径原语需要新臂（或一个携带 `Path` + paint 的臂）。闭包顺序 = 向量顺序 = 绘制顺序。不要做成 `FlowNode`，不要进 `check_content`。

3. **`measure_layer` 的 match**（`decorate.rs:189-206`）  
   新臂在这里变成 `FragmentCommand`。保持 `origin (0,0)`、`height 0`、空命令 → `None`。几何应在这里（或更早的 `try_pt` / `Path` 构造）失败，才能沿用「测量完才 `paint_pages`」。

4. **`FragmentCommand` 缺 `Fill`**（`flow.rs:78-89`）  
   `Stroke` 已存在，表格在用。`Canvas::fill_path`、`CanvasCommand::FillPath`、`Mark::Fill`、`to_fill` 都已存在。层的填充要新臂，例如 `Fill { path, paint }`，并改两处穷尽 match：`Fragment::with_offset`（`64-71`）与 `draw_fragment`（`346-361`，应调 `canvas.fill_path`）。`list.rs:260-261` 同样穷尽。`test_util::strokes`（`91-99`）只认 `StrokePath`，不断言 `FillPath`。  
   只加 `Stroke`、不加重填，填不满 §2.12 的 fill，也用不到已经接好的 `fill_path`。

5. **`paint_pages` 叠层不要改**（`paginate.rs:269-273`）  
   behind → header → body → footer → front 已是提案顺序。路径命令跟着所属层的 `PlacedFragment` 走。同一层内先 push 的先画。

6. **直线 / 矩形可以落在现有 `Path` 上。椭圆与旋转不行。**  
   `Path::line` 与表格 `rect_path` 的动词集够 `line` 和 `rect`。`PathVerb` 没有曲线；`to_krilla_path` 没有对应臂。椭圆要扩动词并改 `convert.rs`，或在测量时折成折线再走现有 `LineTo`。  
   旋转：公共 `Canvas` 无变换。图片平移是 `emit` 私有。把旋转烘焙进顶点则 `draw_fragment` 不用新方法；若要 `push_transform`，得扩 `Canvas`、`CanvasCommand`、`RecordingCanvas`、`KrillaCanvas`、`replay`。半透明不必新管线：`Color` alpha 已进 `to_fill` / `to_stroke` 的 opacity。

7. **不要接的地方**  
   `measure_band`、正文 `split`、`plan_pages` 的 `y`、header 的 margin 限高。层不占正文高度，不换行（`single_line_run` + `INFINITY` 只服务于 `text_at`）。流内分隔线与流内 canvas 是别的条目（提案 §2.14），不是这条缝。

### Non-Obvious Things

- 层闭包的「每页绘制时」在代码里是 `measure_bands`，不是 `paint_pages`。绘制只重放已经测好的命令。
- 一个 behind（或 front）是**一个**片段，不是每个 item 一个片段。空 item 列表变成 `None`，该槽在链上消失，但闭包已经跑过。
- `measure_layer` 把对齐改成 `Start`；`single_line_run` 又把原点 x 改成 0。两处都在忽略对齐。路径 item 若复用 `Style`，不要假定 `TextAlign` 会影响矢量。
- 层片段 `height` 恒为 0。分页不用它。页外坐标照画（测试钉了 395 靠近页底）。
- `doc.behind` 不是第二套装饰存储。它是隐式 section 的糖。多 section 时每段自己的 hook，经 `SectionSpan.decorations` 只覆盖该段页（`paginate.rs:108-113`，`decorate.rs:115-157`）。
- `is_last` 是全文最后一页。section 末页要自己比 `section_number` 与 `section_count`。
- 表格 `Stroke` 与层将来的 `Stroke` 共用 `draw_fragment`。层路径若用页坐标且原点 `(0,0)`，不会再被 body margin 平移。若误放进正文片段，会被 `with_offset` / body origin 再加一次。
- `FragmentCommand` 是 `pub(crate)`。公共面是 `Layer` 方法加上已导出的 `Path` / `Paint` / `Stroke`。
- `draw_fragment` 的失败点在 `start_page` 之后。现有层文本和图在测量时就 `try_pt`，所以测试能断言画布仍空。新几何应同样在 `measure_layer` 失败。

### Open Questions

- 椭圆进 `Path` 的方式：新曲线动词（并改 `to_krilla_path`）还是折线。代码里两种都不存在。
- 旋转烘焙进顶点，还是给 `Canvas` 加成对的 transform。trait 上目前没有变换。
- `Fill` 与 `Stroke` 是两条 `FragmentCommand`，还是一个 item 在 `measure_layer` 里按「先填后描」展开。`draw_fragment` 只保证命令顺序。
- 同一形状既填又描时，公共 `Layer` 方法是 builder、`stroke_*`/`fill_*` 成对，还是一个 paint 枚举。今天没有任何 path 方法可对照。
- 退化几何（零面积矩形、零长线、非正 stroke 宽）放在哪一层拒绝。`Stroke` 已经拒绝非正宽度；`Path` 不拒绝零长线段（`Path::line` 不看两点是否重合）。层今天不裁剪页外图形。
