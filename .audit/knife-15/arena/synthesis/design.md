# Knife 15 设计：正文文字外部 URI 链接（synthesis）

**Base.** Arena candidate-2. Grafts noted in `note.md`.

## 0. Ship order

1. Internal: `TextRun.link`, `prepare_inline` flush, `LinePiece` / `FragmentCommand::Link`, `with_offset` / `draw_fragment` (unit tests via crate-private constructors).
2. DSL: `Paragraph::link` + `active_link`, Band `left_link` / `right_link`.
3. Integration: `RecordingCanvas` matrix + `%PDF` smoke.

## 1. 调用方式

正文沿用段落闭包；`link` 只给闭包内追加的 `text` / `bold` 文字设置 URI，不自动改变颜色或增加下划线：

```rust
doc.paragraph(|p| {
    p.text("详情见 ");
    p.link("https://example.com/contracts/18", |linked| {
        linked.text("合同 ");
        linked.bold("C-18");
    });
    p.text("。");
});
```

页眉、页脚目前没有 `Paragraph` 槽位，因此增加整槽链接入口：

```rust
doc.header(|h| {
    h.left_link("https://example.com", "Example Ltd.");
    h.right(format!("第 {} 页", h.page().number));
});
```

建议的公开签名：

```rust
impl Paragraph {
    pub fn link(
        &mut self,
        uri: impl AsRef<str>,
        f: impl FnOnce(&mut Paragraph),
    );
}

impl Band {
    pub fn left_link(
        &mut self,
        uri: impl AsRef<str>,
        text: impl AsRef<str>,
    );
    pub fn right_link(
        &mut self,
        uri: impl AsRef<str>,
        text: impl AsRef<str>,
    );
}
```

`Paragraph::link` 进入一个临时 link scope，闭包返回后恢复先前状态。这样不增加公开 builder 类型，并允许现有 `text`、`bold` 原样使用。闭包中的 blank 仍按现有 API 追加，但 blank 自身不带链接，并会切断测量组；blank 后的文字重新形成同一 URI 的新链接片段。若 URI 为空，记录 `Error::EmptyLink`，不调用该 link 闭包，最终在组装结束时报错。

**签名必须返回 `()`，不能返回 `Result<()>`。** `Flow::paragraph` 与 `Band` hook 的闭包都是 `-> ()`。在闭包里对 `link` 使用 `?` 无法编译。空 URI 与 `RunAfterFillBlank` 一样记入 builder `error`，在 lower / paint 前失败，且早于 `start_page`。

`Band::{left,right}_link` 与现有 `left` / `right` 一样采用“同槽后一次设置覆盖前一次设置”；不过一旦构造 URI 失败，首个 builder 错误保留，`Band::lower` 返回该错误。

## 2. 类型与数据形状

复用现有公开 `LinkTarget`、`Error::EmptyLink`、`Canvas::annotate_link`，不增加另一种 URI 类型。

```text
Paragraph（公共 builder）
  items: Vec<InlineItem>
  active_link: Option<LinkTarget>       # 仅组装期临时状态
  error: Option<Error>

TextRun
  text: String
  face: Face
  link: Option<LinkTarget>

InlineItem
  Text(TextRun)
  Blank(InlineBlank)                    # blank 不增加 link 字段

MeasuredInline
  text: TaggedClusteredText<Face>
  glyphs: Vec<InlineGlyph>
  link: Option<LinkTarget>              # 整个测量组只有一个 target

LinePiece
  Glyphs(GlyphRun)
  Stroke(InlineStroke)
  Link {
    x0: f32,
    x1: f32,
    target: LinkTarget,
  }

FragmentCommand
  ...既有变体...
  Link {
    rect: Rect,                         # fragment-local 坐标
    target: LinkTarget,
  }

BandSlot
  text: String
  link: Option<LinkTarget>

Band
  page: Page
  left: Option<BandSlot>
  right: Option<BandSlot>
  error: Option<Error>
```

关键不变量：

- `LinkTarget::uri` 是字符串进入内部数据的唯一入口；因此 `TextRun`、`MeasuredInline`、`FragmentCommand` 中出现的 target 必然非空。
- 不访问 URI，也不解析或检查 scheme；空格等非空字符串照原样保留。
- `TextRun` 的组装合并键是 `(face, link)`，而不是只有 `face`。
- 一个 `MeasuredInline` 内可以混合 body/bold face，但不能混合不同 link 状态或不同 URI。
- `Rect` 只为严格正宽的链接片段构造；空文字和零 advance 不发注解。

## 3. 模块改动图

| 文件 | 改动 |
| --- | --- |
| `src/document.rs` | 增加 `Paragraph::link` 和临时 `active_link`；`append` 把 link 写入 `TextRun` 并按 face+link 合并；Band 槽改为 `BandSlot`，增加 `left_link` / `right_link` 和错误累计；lower 时把整槽 link 写入 `TextNode`。 |
| `src/layout/paragraph.rs` | `TextRun`、`MeasuredInline` 携带 `Option<LinkTarget>`；`prepare_inline` 在 link 变化和 blank 处 flush；`place_line` 记录链接 x 范围；`fragment_from_lines` 形成每行 `Rect` 和 `FragmentCommand::Link`。 |
| `src/layout/flow.rs` | 增加 `FragmentCommand::Link`；`Fragment::with_offset` 平移 rect；`draw_fragment` 调用 `Canvas::annotate_link`。 |
| `src/layout/list.rs` | `first_baseline` 的穷尽 match 把 `Link` 视为无 baseline。列表项中的段落链接因此自然随正文列缩进平移；列表 marker 本身仍不可链接。 |
| `tests/document.rs` | 增加公开 DSL 到 `RecordingCanvas` 的行为测试，以及 backend feature 下的 `%PDF` 冒烟。 |
| `src/layout/paragraph.rs`、`src/layout/decorate.rs` 的测试模块 | 钉住组边界、矩形局部坐标、Band 失败早于 `start_page` 等内部不变量。 |

不改 `src/canvas.rs`、`src/error.rs`、`src/backend/*` 和 `src/lib.rs`：所需 target、错误、记录命令、krilla 注解队列和公开重导出已经存在。

## 4. 排版算法

### 4.1 Assemble：按 face 和 link 合并

`Paragraph::link(uri, f)` 先调用 `LinkTarget::uri`。成功后暂存原 `active_link`，把新 target 设为当前值，运行 `f(self)`，然后恢复原值。`text` / `bold` 最终仍只经过 `Paragraph::append`。

`append(face, text)` 保留现有顺序：

1. 空文字直接忽略，不产生 run 或注解。
2. 若前一个 item 是 `blank_to_end`，记录 `RunAfterFillBlank`。
3. 读取当前 `active_link`。
4. 只有最后一个 item 是文字且 `last.face == face && last.link == active_link` 时才拼接字符串。
5. 否则追加新的 `TextRun { text, face, link }`。

因此 URI 的 `None → Some`、`Some → None`、`Some(a) → Some(b)` 都不会在 assemble 阶段被抹平；相同 face、相同 URI 的相邻调用仍合并。

Band 的 whole-slot helper 同样先构造 `LinkTarget`。`Band::lower` 使用带 link 的单个 `TextRun` 创建 `TextNode`，随后与正文共用完全相同的 paragraph → fragment → paint 管线。

### 4.2 `prepare_inline`：link 变化时 flush

现有 `pending: Vec<TextRun>` 不能继续只在 blank 处 flush。遍历 item 时：

1. 遇到 `Text(run)`，若 pending 非空且 pending 首项的 link 与 `run.link` 不同，先 `flush_text`。
2. 把 run 加入 pending。face 改变但 link 相同不 flush，所以一个链接中的 body/bold 仍属于同一组。
3. 遇到 `Blank`，先 flush pending，再按现有逻辑追加 `Span::Blank`。blank 没有 target。
4. 遍历结束后再 flush。

`flush_text` 调用 `measure_inline`，后者从该批 run 的首项复制 link 到 `MeasuredInline`。调试断言该批所有 run 的 link 相等。`Span::Group(index)` 和 `Seg::Glyphs { group, ... }` 不需要扩充 payload；target 可由 group index 查回。

增加组边界不会改变允许断行的位置。`break_spans` 仍按 cluster 分类和宽度决定断点；一个链接组被换行切开后，每个视觉行得到引用同一 group 的独立 `Seg::Glyphs`。

### 4.3 `place_line`：在最终水平位置取矩形宽度

`place_line` 已经知道：

```text
line_x = frame_x + align_x(frame_width, line.width, align)
```

因此它是计算链接横向范围的唯一位置。对每个非空 `Seg::Glyphs`：

1. 先计算整个 seg 的 `advance`。
2. 从 `x` 生成现有 glyph runs；字体切换可生成多个 `GlyphRun`。
3. 若 `groups[group].link` 为 `Some(target)` 且 `advance > 0`，追加一个 `LinePiece::Link { x0: x, x1: x + advance, target }`。
4. 最后执行 `x += advance`。

不能从 `GlyphRun.origin` 事后重建链接范围，因为 face/font 的切分与链接边界不是同一回事。上述做法使 link 内的 body+bold 只有一个矩形，同时自动包含首行缩进、Start/Center/End 对齐结果。

### 4.4 `fragment_from_lines`：叠加 y 并形成每视觉行矩形

对片段中的第 `index` 行：

```text
y0 = index * line_height
rect = Rect(x0, y0, x1 - x0, line_height)
```

`y0` 是行盒顶边；不能使用 glyph baseline。高度固定为正文 style 算出的完整 `line_height`，bold 不另行改变命中高度。宽度不大于零时省略，避免构造非法 `Rect`。

矩形保持 fragment-local。段落跨页时，tail 再次从 `index = 0` 开始生成片段，所以链接会在新页正文 origin 下正确落点，不携带上一页 y。

### 4.5 `FragmentCommand::Link`、平移与 paint

新增：

```text
FragmentCommand::Link { rect, target }
```

并补齐两个关键 match：

- `Fragment::with_offset(dx, dy)`：与 Image 一样执行 `rect.translated(dx, dy)`。这覆盖 stack 的 y、columns 的 x、列表正文缩进、表格 padding 以及 Band 槽位偏移。
- `draw_fragment(fragment, canvas, origin)`：再次用 `PlacedFragment.origin` 平移到页坐标，然后调用 `canvas.annotate_link(rect, target.clone())`。

`layout/list.rs::first_baseline` 对 Link 返回 `None`；注解命令不参与 marker baseline 搜索。

`paint_pages` 不需要改。它仍按 `behind → header → body → footer → front` 绘制，header/footer 的 link command 会和正文一样到达 `draw_fragment`。现有 `KrillaCanvas` 把注解加入当前页的 `QueuedLink`，`emit_page` 在 `surface.finish()` 后写 `LinkAnnotation + LinkAction`；不建立平行后端路径。

## 5. 错误与边界情况

| 情况 | 结果 |
| --- | --- |
| `p.link("", ...)` | `LinkTarget::uri` 返回 `Error::EmptyLink`；Paragraph 保存首个错误，文档组装失败，不进入分页或 `start_page`。 |
| `Band::{left,right}_link("", ...)` | Band 在页钩子执行时保存 `EmptyLink`；`measure_bands` 返回错误。所有页和 band 都在 paint 前测量，因此 canvas 仍无 `StartPage`。 |
| `" "`、自定义 scheme、相对串等非空 URI | 原样接受和写入；不访问，不做 scheme 合法性判断。 |
| 有效 URI + 空文字 | 沿用现状忽略空文字；无 glyph、无注解、无错误。 |
| 零 advance 的文字片段 | 不构造 Rect，不调用 `annotate_link`。 |
| link 内 body/bold 切换 | 字形可切成多个 run，链接仍是该视觉行上的一个连续矩形。 |
| URI 变化 | assemble 不合并；`prepare_inline` flush；每个 URI 得到自己的矩形。 |
| link 中的 blank | blank 自身不可点击并切断组；blank 两侧各自形成矩形，不跨过空白合并。 |
| `blank_to_end` 后追加链接文字 | 与普通文字相同，报现有 `RunAfterFillBlank`。 |
| 换行或分页 | 每个视觉行片段一个矩形；分页后的 y 从该页 fragment 顶部重新开始。 |
| Rect 构造/平移出现非法几何 | 返回现有 `InvalidGeometry`；这些步骤发生在全页规划/装饰测量期间，正常情况下早于首个 `start_page`。 |
| Canvas page lifecycle 错误 | 沿用 `Canvas::annotate_link` / backend 的 `InvalidCanvasState`；正常 `paint_pages` 总在已打开页面内调用。 |

明确不做：文内页码/锚点跳转、图片链接、`Layer::text_at` 链接、Cell 字符串链接、自动下划线/颜色、URI 抓取或 scheme 检查。

## 6. 测试计划

测试以 `RecordingCanvas` 的 `CanvasCommand::AnnotateLink { rect, target }` 为行为事实；坐标比较用 `pt_near`。

1. **单行部分链接。** 段落为 unlinked `"A"` + linked `"BC"` + unlinked `"D"`。断言恰好一个 `AnnotateLink`，target 字符串完全相等；rect.x 等于 A 的右缘，rect.width 等于 BC glyph advance，rect.y 为行盒顶，rect.height 为 `LINE`。
2. **自动换行。** 在 12pt 宽 frame 放 linked `"第第"`，让两个字自动分成两视觉行。断言两个注解，分别为 `(x=0, y=0, w=12, h=LINE)` 和 `(x=0, y=LINE, w=12, h=LINE)`，target 相同。
3. **body + bold 不拆注解。** 注册 body/bold 字体，在一个 `p.link` 内追加 `"A"`、bold `"B"`、`"C"`。断言三个 glyph runs 但只有一个链接矩形，宽度等于三段 advance 之和。
4. **URI 边界不丢失。** 同 face 相邻写 URI A、URI B、unlinked、URI A；断言 target 和 x 区间分别正确且互不覆盖。该用例同时防止 `append` 和 `prepare_inline` 两层错误合并。
5. **相同 URI 合并。** 两个相邻 `p.link("u", ...)` 写同 face，断言单行只有一个注解，覆盖两段总宽；face 改变时也仍只有一个注解。
6. **blank 不可点击。** 同一个 link scope 中写文字、定宽 blank、文字。断言有两个链接矩形和一个 StrokePath；两个 rect 不覆盖 blank 的 x 区间。
7. **对齐与嵌套偏移。** Center/End 段落的 rect.x 与最终 glyph x 一致；第二 column 或列表项中的链接包含 column/list 偏移，证明 `with_offset` 的 Link 分支生效。
8. **分页。** 两行链接放入只能容纳一行的页面。断言两页各一个注解，两个 rect.y 都从各页正文 margin 顶开始，而不是第二页延续上一页 y。
9. **页眉/页脚 whole-slot。** `left_link` 和 `right_link` 分别产生注解；断言左右 margin inset、End 对齐及 header/footer y origin 都反映在 rect 中。
10. **空 URI。** 正文 DSL 返回 `Error::EmptyLink`。Band 的内部 render harness 进一步断言失败时 `RecordingCanvas::commands()` 为空，即没有 `StartPage`。
11. **空文字/零宽。** 有效 URI 包裹空字符串时没有 `AnnotateLink`，也不产生非法 Rect。
12. **无视觉装饰副作用。** 链接用例中除已有文字命令和 `AnnotateLink` 外，不因 link 新增 FillPath/StrokePath；glyph 的 font、size、color 与无链接对照完全一致。
13. **backend 冒烟。** `backend-krilla` 下生成含链接文档，仅断言输出以 `%PDF` 开头。URI 注解内容的具体编码继续由已有 backend emit probe 覆盖，不对压缩 PDF 字节做布局断言。
14. **逐页 Band URI。** `page_break` 两页，`left_link` 的 URI 含 `page().number()`。两页各一个注解，字符串分别为 `…/1` 与 `…/2`。
15. **跨页 y 重置。** 页高只够一行的两行链接。两页 `rect.y` 都等于该页正文顶，不延续上一页 y。
16. **原样 URI。** `" "`、`"mailto:a@b.c"`、`"not a uri"` 成功写入，`target.as_str()` 完全相等。
17. **刀外不注解。** 同一文档含单元格字符串、`behind` 的 `text_at`、以及一段真正的 `p.link`。`AnnotateLink` 只有段落那一条。
18. **flush 同构。** `prepare_inline` flush 后对组内 run 的 `link` 做 `debug_assert` 同构（仅 debug）。
