# Overview

Knife 6 是把已经落地的“横向文字管线”和“定位后绘制管线”接成一个最小的纵向正文流：

```text
crate-internal FlowNode 树
  -> 段落测量与断行
  -> FlowNode::split（Fit / Split / MoveToNextPage）
  -> 页面上的 PlacedFragment
  -> draw_fragment
  -> Canvas
  -> RecordingCanvas 测试断言
```

它对应设计稿 PR8（paragraph flow）和 PR9（vertical paginate），范围是：

- 段落按当前 `Frame` 的宽度断行，按剩余高度切成页内 head 和下一页 tail。
- 根正文是内部 `FlowNode::Stack`。
- 支持内部 `Spacer`、`PageBreak`、`KeepTogether`。
- 实现段后距和 `PageCursor` 三态语义。
- 把定位后的字形交给 `Canvas::draw_glyph_run`，用 `RecordingCanvas` 验证坐标和页序。

Knife 6 不负责表格、columns、列表、图片、页眉页脚或最终公共 Rust DSL。尤其不能因为 `src/document.rs` 还是占位文件，就提前发明 `doc.paragraph(...)` 等 builder。测试应直接构造 crate-internal `FlowNode` / paragraph 数据。`.audit/handoff/knives-5-8.md` 也明确把公共 DSL 留给 knife 8。

当前仓库的状态很清楚：文字底座和画布底座已经可用，整个 `src/layout/` 仍是空骨架。Knife 6 不是改已有分页实现，而是第一次把这些底座接起来。

# Key Concepts

## 1. 已落地的横向文字管线

### `FontStore` 与 `measure_text`

`src/fonts.rs` 已有：

- `FontStore::register`：解析字体并保存 `FaceMetrics`。
- `FontStore::metrics`：提供 ascent、descent、leading、units-per-em。
- `FontStore::face_mut` 中的 advance cache。
- `FontStore::resolve`：供 krilla backend 取同一份字体。

`src/text/measure.rs` 已有：

- `measure_text(&mut FontStore, FontId, &str) -> MeasuredText`
- `line_metrics(&FaceMetrics, Length<Positive>, factor) -> LineMetrics`

`MeasuredText.glyphs` 对每个 Unicode scalar 保存 `glyph_id`、`advance_norm` 和源字符串 byte range。实际 pt 宽度是：

```text
advance_norm * font_size.as_pt()
```

缺字已经固定为 `Error::MissingGlyph`，未知字体为 `Error::UnknownFont`。布局不能画 `.notdef`，也不能另做 fallback。

### `ClusteredText`

`src/text/cluster.rs` 的 `ClusteredText::new` 已完成三件事：

1. 把 CRLF 和单独 CR 规范成 LF。
2. 用 UAX #29 把字符串切成扩展字素簇。
3. 生成不含硬换行 BK 的 `glyph_text()`。

每个 `Cluster` 的 `glyph_range()` 是进入 `glyph_text()` 的 scalar 下标范围。这个范围正好可以索引 `MeasuredText.glyphs`，因为当前测量也是“一 scalar 一 glyph”，没有 kerning、连字或 shaping。

### `break_lines`

`src/text/line_break.rs` 的：

```text
break_lines(&ClusteredText, avail, width_of_cluster)
```

已经负责所有横向断点选择。它消费每个非 BK cluster 的宽度回调，产出 `BrokenLine`：

- `text()`
- `glyph_range()`
- `glyph_byte_range()`
- `width()`

它已实现当前小分类表、硬换行、行首 U+0020 丢弃、向左找最近断点，以及 `Error::UnbreakableWiderThanFrame`。Knife 6 不应复制或改写断行规则；它只负责提供真实字体宽度，并按行组装 glyph run。

## 2. 已落地的绘制出口

`src/canvas.rs` 已有完整的 `Canvas` trait 和 `RecordingCanvas`。

段落最终要生成 `GlyphRun`。关键字段是：

- `font`
- `font_size`
- `color`
- `origin`：第一个 glyph 的基线左端，页面坐标系左上原点、y 向下
- `glyphs: Vec<GlyphPlacement>`
- `text`

`GlyphPlacement.advance_norm` 必须直接来自 `MeasuredGlyph.advance_norm`。测量和绘制必须是同一批 glyph id / advance，不能调用 krilla `draw_text`，也不能在 layout 中依赖 krilla 类型。

`RecordingCanvas` 记录 `StartPage`、`DrawGlyphRun`、`FinishPage` 等命令，并可用 `pages()` 取出已关闭页面。Knife 6 的排版测试应断言这些命令里的页数、glyph 文本、x、基线 y 和 draw order；不应断言压缩 PDF 字节。

## 3. 设计中已定、但尚未实现的 flow 协议

`docs/quire-design.md` 的内部协议定义了这些概念，但当前源码中一个都还不存在：

- `LayoutCtx`
- `ResolvedStyles`
- `Frame`
- `Fragment`
- `FragmentCommand`
- `Split`
- `FlowNode`
- `FlowNode::split`
- `draw_fragment`
- `natural_height`
- `PlacedFragment`
- `PageBody`
- `PageCursor`

核心契约是 consume-self：

- `Fit { height, fragment }`：节点已经全部消耗，只画一次 `fragment`。
- `Split { head_height, head, tail }`：本页只画 `head`，以后只处理 `tail`。
- `MoveToNextPage(node)`：本页一点没画，完整未画节点被移到下一页。

分页器绝不能在拿到 `tail` 后又重画原节点。这是防止跨页段落重复的核心所有权规则。

`Fragment` 内坐标相对 fragment 原点；`PlacedFragment.origin` 才是页面坐标。分页期间只保存定位结果，`draw_fragment` 在绘制时加 origin。分页器不应在绘制阶段重新测量。

## 4. 三种“竖直空白”不能混淆

- 段落行盒：属于段落 `height`，必须参与切页。
- `spacing_after`：段后距；放不下时整段丢掉，不产生下一页。
- `Spacer`：用户正文节点；可拆，本页吃掉一部分后，余量作为更矮的 tail 去下一页。

`Spacer` 即使不产生 Canvas 命令，也是真正正文高度。`spacing_after` 则不是可见命令，也不是独立节点。

## 5. 当前框宽和默认 margin

提案书 §2.1 的 `W` 是“段落当前所在框的宽”，不是固定纸宽。Knife 6 只有根正文时：

```text
body_width  = page_width  - margin.left - margin.right
body_height = page_height - margin.top  - margin.bottom
body_origin = (margin.left, margin.top)
```

以后 columns 或 table cell 会传入更窄的 `Frame`，同一个 paragraph split 逻辑不应知道它来自页面、列还是单元格。

默认 margin 以提案书 §7 和现有 `Margins::default()` 为准：四边都是 **0**。设计稿旧文中“默认四边 72 pt”已经被提案书覆盖。提案书 §2.1 的 A4 + 72 pt 只是计算 `W` 的例子，不是默认值。

# How It Works

## 1. 段落从字符串变成可分页行

对于一个已经解析好内部样式的 paragraph，layout 至少需要拿到：

- 原文
- `FontId`
- 正字号
- line-height factor
- `Color`
- `Start` / `Center` / `End`
- 非负 paragraph spacing

这些是 layout 的输入事实；Knife 6 可以用 crate-internal 数据承载，但不能把它们包装成新的公共 DSL builder。

一次段落布局应按下面顺序组合现有模块：

1. `ClusteredText::new(source)`，得到规范化 cluster tape 和 BK-free `glyph_text()`。
2. `measure_text(fonts, font, clustered.glyph_text())`，一次得到所有非 BK scalar 的 glyph id 和 advance。
3. 从 `FontStore::metrics(font)` 取得 face metrics，再调用 `line_metrics(...)` 得到 `line_height` 和 `baseline`。
4. 调用 `break_lines(&clustered, frame_width, callback)`。
5. callback 用 `cluster.glyph_range()` 索引 `MeasuredText.glyphs`，求和：

   ```text
   cluster_width = Σ(glyph.advance_norm * font_size_pt)
   ```

6. 对每个 `BrokenLine`，用其 `glyph_range()` 取出同一批 measured glyph，构造一个 `GlyphRun`。

这样 charmap、advance、断行和绘制共享同一份 glyph 数据，不会出现“量的是一套字，画的是另一套字”。

`BrokenLine.width()` 已是实际 pt 行宽。断行后再计算水平位置，不得让对齐影响断点：

```text
Start:  x = 0
Center: x = (frame_width - line_width) / 2
End:    x = frame_width - line_width
```

这里的 x 是 fragment-local；页面上的最终 x 还要加 `PlacedFragment.origin.x`。

第 `i` 行的 local 基线是：

```text
baseline_y = i * line_metrics.line_height + line_metrics.baseline
```

这正是设计 PR8 要测试的“行顶 + baseline”。不能拿 ascent、字号或 line height 本身冒充基线。

## 2. 从 `BrokenLine` 组装 `GlyphRun` 的细节

`GlyphRun.text` 应只包含该行文本，而 `GlyphPlacement.byte_range` 必须相对这个 run 的 `text`。`MeasuredGlyph.byte_range` 原本相对整段 BK-free `MeasuredText.text`，所以抽出某一行时必须重新基于该行起始 byte 偏移做 rebasing。

行首被丢弃的普通空格不会出现在 `BrokenLine.glyph_range()` 中；硬换行也不在 `glyph_text()` 中。因此必须以 landed 的 line ranges 为准，不能根据原始字符串重新猜切片。

空段落由 `break_lines` 产出一个宽 0 的空行，仍占一个 `line_height`。连续硬换行产生的空行也不能被 layout 随意过滤。

## 3. 段落的竖向 `split`

横向断行完成后，段落按当前 `Frame` 的剩余高度决定能放几行：

- 所有行都放得下：返回 `Fit`，height 是所有行盒高度；终结段落的 fragment 携带 `spacing_after`。
- 至少一行、但不是全部行放得下：返回 `Split`；head 只含本页行，tail 只含余下行，head 不应产生段后距。
- 当前剩余高度一行也放不下：返回 `MoveToNextPage`，节点完全未画。
- 在完整正文框中仍放不下一行：最终必须成为 `UnsplittableTallerThanFrame`，而不是不断换页重试。

换页只改变可用高度，不改变 §2.1 的水平 `W`。同一根正文跨页后仍按相同 body width 断行；未来列或单元格则按各自 frame width 断行。

设计要求 `split(self, ...)` 消耗节点，因此 tail 必须携带继续布局所需的数据。它可以保存未画行/字形或保存足以继续生成它们的内部状态；具体存储形状尚未由 landed API 固定，但语义上不能重新加入已经画过的 head。

## 4. `Stack` 把节点串成一条正文流

根正文是内部 `FlowNode::Stack(Vec<FlowNode>)`。它按顺序处理孩子：

- child `Fit`：把 child fragment 合并进本页 head，y 向下推进 `height`。
- child `Split`：合并 child head，本页结束；child tail 加到未访问孩子之前，成为 stack tail。
- child `MoveToNextPage`：
  - stack 本页还没有内容时，整个剩余 stack 返回 `MoveToNextPage`；
  - 已有内容时，本页返回 `Split`，未画 child 和后续孩子组成 tail。
- 空 stack：`Fit`，高度 0，空 fragment。

合并 fragment 时，child 的 local 命令需要增加当前 stack y offset；不能把多个 child 的 glyph 都留在 `(0, baseline)`。

段后距在 child 与下一个 child 之间处理：

- 剩余高度足够，就消耗完整 `spacing_after`。
- 不足就把整段 spacing 丢掉。
- 最后一个 child 的 spacing 不算 stack 自然高度；它继续挂在 stack fragment 上，交给 stack 外层按相同规则处理。

## 5. `Spacer`

提案书 §2.13 固定了：

- 高度 0：`Fit`，height 0，空 fragment。
- 剩余高度足够：`Fit`，消耗全部 spacer 高度。
- 剩余高度不足：本页 head 消耗能吃下的高度，tail 是余下高度的 spacer。
- spacer 可以成为下一页唯一正文内容。

它没有 `FragmentCommand`，但 pagination 的 y 和页状态仍需反映它消耗的正文高度。不要把它按“没有 Canvas op”优化掉。

## 6. `KeepTogether`

`KeepTogether` 的目标不是禁止所有 flow 元素跨页，而是显式把一组孩子变成页内原子：

1. 用完整正文框计算组的自然高度。
2. 自然高度超过完整正文框：`Error::UnsplittableTallerThanFrame`。
3. 能装进整框，也能装进当前剩余高度：`Fit`。
4. 能装进整框，但当前页剩余高度不足：`MoveToNextPage`，整组不画。

自然高度包含孩子之间能保留的 paragraph spacing，不包含组外最后一个孩子的 spacing。最后 spacing 应挂回组 fragment，供组之后处理。

这里有一个需要实现时正面解决的设计接口缺口：文档示例的 `FlowNode::split` 只收到“当前剩余 `Frame`”，但 `KeepTogether` 同时需要“完整正文 `Frame`”；而 `natural_height` 会消费 node。landed 源码没有相关接口可沿用。Knife 6 必须在 crate-internal plumbing 中同时保留 current/full frame 或让 paginator做等价预检，但不能改变上述语义，也不能因此新增公共 API。

## 7. `PageBreak` 与 `PageCursor`

`FlowNode::PageBreak` 不调用普通 `split`。如果误调，设计要求返回 `Error::InvalidGeometry`。

分页器使用三态，不可简化成 `has_content: bool`：

- `Fresh`：尚未开出正文页。
- `HasBody`：当前页已有正文。
- `PendingBlank`：上一页刚被 pageBreak 结束，下一页只挂起、尚未提交。

必须满足：

| 输入 | 输出 |
| --- | --- |
| 空 content | 1 页空正文 |
| 开头一个或多个 pageBreak | 仍是 1 页空正文 |
| 内容 + break | 1 页内容；末尾挂起页丢弃 |
| 内容 + break + 内容 | 2 页内容 |
| 内容 + break + break | 内容页 + 1 页空白正文 |
| 内容 + break + break + 内容 | 内容页 + 空白正文页 + 内容页 |

空白正文页仍然是正式物理页。Knife 6 还没有页眉页脚，所以在 `RecordingCanvas` 上它应表现为一对 `StartPage` / `FinishPage`，中间没有正文 draw op；以后 PR10 会在这类页上也画装饰并计入页码。

## 8. 分页先完成，再绘制

设计中的分页产物是 `Vec<PageBody>`；每个 `PageBody.fragments` 是若干 `PlacedFragment { origin, fragment }`。

分页阶段：

1. 根据 page size 和 margins 算 body frame。
2. 反复消费根 `FlowNode::Stack`。
3. 对 `Fit` / `Split` 保存 fragment，并把页面 y 向下推进。
4. 对 `MoveToNextPage` 关闭当前逻辑页；若已经处于完整空框仍不能放，则报 `UnsplittableTallerThanFrame`，防止死循环。
5. 根据 `PageCursor` 提交普通页或显式空白页。

绘制阶段：

1. `Canvas::start_page(page_size)`。
2. 按顺序对每个 `PlacedFragment` 调用 `draw_fragment`。
3. `draw_fragment` 把 fragment-local `GlyphRun.origin` 平移到页面 origin，再调用 `Canvas::draw_glyph_run`。
4. `Canvas::finish_page()`。

Knife 6 不产出 PDF bytes，也不接触 `krilla::*`。同样的定位结果既可画到 `RecordingCanvas`，也可在开启 `backend-krilla` 时交给适配器。

# Where Things Live

## 已有实现

| 文件 | 当前可直接消费的内容 |
| --- | --- |
| `src/fonts.rs` | `FontStore`、字体注册、face metrics、advance cache、`FontId` |
| `src/text/measure.rs` | `measure_text`、`MeasuredText`、`MeasuredGlyph`、`line_metrics`、`LineMetrics` |
| `src/text/cluster.rs` | `ClusteredText`、`Cluster`、cluster/glyph/byte cursor 映射 |
| `src/text/break_class.rs` | `BreakClass`、`PairClass`、分类表 |
| `src/text/line_break.rs` | `break_lines`、`BrokenLines`、`BrokenLine` |
| `src/canvas.rs` | `GlyphRun`、`GlyphPlacement`、`Canvas`、`RecordingCanvas` |
| `src/geom.rs` | pt 长度、`Point`、`Size`、`Margins`；`Margins::default()` 为四边 0 |
| `src/error.rs` | 字体、缺字、frame 横向不可断、geometry、canvas/backend 等现有错误 |

这些模块目前有不少 `#![allow(dead_code)]`，原因正是 layout 尚未接线，不代表它们是占位实现。

## 当前为空的 layout 骨架

| 文件 | 当前状态 | Knife 6 应承担的职责 |
| --- | --- | --- |
| `src/layout/flow.rs` | 只有模块注释 | 内部 `FlowNode`、`Fragment`、`Split`、consume-self split、fragment 绘制/平移、stack/spacer/keep 语义 |
| `src/layout/paragraph.rs` | 只有模块注释 | cluster → measure → break → line glyph runs；按高度切 paragraph |
| `src/layout/paginate.rs` | 只有模块注释 | body frame、页面组装、`PageCursor`、整框失败检测、Canvas 页生命周期 |
| `src/layout/lower.rs` | 只有模块注释 | 将来公共 document 节点到 `FlowNode` 的边界；Knife 6 不得为填满此文件而发明 builder |
| `src/layout/decorate.rs` | 只有模块注释 | PR10 页眉页脚；不是 knife 6 |
| `src/layout/table.rs` | 只有模块注释 | 后续 table；不是 knife 6 |
| `src/layout/dump.rs` | 只有模块注释 | 后续可选调试输出；不是 knife 6 |

`src/document.rs` 也只有说明性注释和“no public builders yet”。这是有意的边界，不是 knife 6 必须补掉的遗漏。

## 仍需补齐的错误与内部数据

当前 `src/error.rs` 没有设计协议使用的 `UnsplittableTallerThanFrame`。Knife 6 若实现完整的 MoveToNextPage / KeepTogether 防死循环语义，就需要补上该已设计错误；这不是自由发明错误语义。

当前也没有 `Style`、`TextAlign`、`ResolvedStyles`、`Frame`、`FragmentCommand`、`PageBody` 等源码类型。设计稿给出的是内部协议方向；在公共 DSL 延后到 knife 8 的约束下，knife 6 只应增加排版所需的 crate-internal 形状和测试入口。

# Gotchas

1. **不要提前写公共 DSL。** Knife 6 测试直接构造内部 flow 数据；`src/document.rs` 保持占位是允许的。

2. **默认 margin 是 0，不是 72 pt。** 提案书 §7 高于设计稿旧段落；现有 `Margins::default()` 也已经用测试钉死四边 0。

3. **`W` 永远来自当前 frame。** 不要在 paragraph 中读取 A4 宽度或减固定边距。这样未来 columns 和 table cell 才能复用同一逻辑。

4. **一次测量、同一批 glyph 绘制。** cluster width 来自 `MeasuredText.glyphs`；`GlyphRun` 复用这些 glyph id 和 normalized advance。禁止 `draw_text`。

5. **byte range 要重基。** `MeasuredGlyph.byte_range` 相对整段，`GlyphPlacement.byte_range` 必须相对每个 `GlyphRun.text`。

6. **BK 不测量、不绘制。** `ClusteredText::glyph_text()` 已去掉 BK。不要把原字符串直接交给 `measure_text` 后再试图跳过 newline glyph。

7. **不要过滤空行。** 空段落、连续 BK、以及 landed `break_lines` 产生的空行都有既定行盒语义。

8. **段后距不是 spacer。** spacing 放不下要整段丢弃；spacer 放不下要产生 tail。

9. **只在段落结束处带 `spacing_after`。** 跨页 paragraph 的 head 后面不能出现“段后距”，因为段落尚未结束。

10. **`MoveToNextPage` 必须有整框终止条件。** 在完整 body frame 上仍返回 move 时，应报 `UnsplittableTallerThanFrame`；否则会无限开空页。

11. **`PageBreak` 必须由 paginator 特判。** 把它实现成 height 0 的普通节点无法表达连续 break 产生一张空白正文页的语义。

12. **三态不能压成二态。** `PendingBlank` 区分“下一页只是挂起”与“空白页已经提交”，决定 trailing break 是否多一页。

13. **不可见内容仍可能占页。** Spacer 没有 Canvas op，但消耗正文高度；pageBreak 产生的空白页也必须有完整 start/finish 生命周期。

14. **fragment 坐标先局部、后平移。** Stack 合并时加 child y offset，page paint 时再加 body/page origin；不要在测量阶段直接写死页面坐标。

15. **Canvas 不是测量器。** 分页和断点必须在绘制前确定；不能画完再从 `RecordingCanvas` 或 krilla 反推高度/宽度。

16. **Knife 6 没有页眉页脚。** PR9 明确“仍然没有页眉”。正文框只由 page size 与 margins 决定；decorate 留给 PR10。

17. **当前设计的 KeepTogether plumbing 不完整。** 它需要 current frame 和 full frame，且 `natural_height` consume node；实现可以调整 crate-internal 数据流，但必须保持 consume-self、整组不拆和过高报错三项契约。

18. **不要顺手实现后续节点。** `Table`、`Columns`、`List`、`Image` 虽出现在完整设计的 `FlowNode` 枚举里，但 knife 6 的 PR8/PR9 最小集合是 Paragraph、Spacer、PageBreak、KeepTogether、Stack。
