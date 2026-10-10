# Knife 14 取舍说明

## 为什么是 Paragraph inline item

blank 的关键语义是“占据当前文本行中的一段水平 advance”。只有把它放进 `Paragraph` 的有序行内项、交给同一个 breaker，才能同时满足：

- blank 前后的文字共享一条基线；
- fixed blank 会影响断点；
- 中间 blank 会推进后续 glyph 的 x；
- fill 能读取它实际落入的视觉行预算与已用宽度；
- 分页时 stroke 随整行移动。

因此选择 `InlineItem::Text | Blank`，而不是增加 flow block、绘制层对象或第二套段落系统。

## 公共 API 取舍

### 选择三个明确方法

采用：

- `blank(width, configure)`；
- `blank_em(n, configure)`；
- `blank_to_end(configure)`。

fixed 和 fill 的入口分开，调用处一眼能看到动态行为，且无法同时设置两种 width。`Blank` 只配置 thickness、color、baseline offset，不再提供 width setter。

考虑过公开 `BlankWidth` 枚举并只留一个 `blank(width, ...)`，但拒绝：它会把内部布局判别暴露给调用方，弱化 DSL 的可读性，也背离 arena 对 dedicated fill method 的默认。

考虑过让 `blank` 接收裸 `f32`，但拒绝：负值、NaN、∞ 应继续由 §2.0 的 branded length 在边界处理，不应在布局热路径重新验证。

fixed span 使用 `Length<NonNegative>` 而非 `Length<Positive>`。零宽虽然不画线，却是稳定、无害且可组合的值；为它另设错误没有收益。汉字数使用 `u16` 而非浮点“字数”，避免半个字、NaN 与平台相关 `usize`，同时保留 `0`。

### 配置闭包而不是参数爆炸

考虑过：

```text
blank(width, thickness, color, offset)
```

但拒绝：默认值常用，位置参数不易读，也与现有 `hline(|h| ...)` 风格不一致。小型 `Blank` builder 是元素自身属性，不是 named style 或平行文字样式系统。

### 精确的 terminal 错误

fill 后有 run 是 DSL 关系错误，不是几何错误，也不是 cell nesting 错误。现有错误变体都不准确，所以建议新增 `InlineAfterFillBlank`。单值长度错误和过宽错误则分别复用 branded length 错误与 `UnbreakableWiderThanFrame`。

让 `Paragraph` 收集首错、由 `Flow::paragraph` 接入已有 deferred error 通道，可以保持现有闭包签名 `FnOnce(&mut Paragraph)`，也能保证失败早于 `start_page`。没有把所有 paragraph builder 改成 `Result`，避免无关 API 扩散。

## 默认值取舍

按 arena 默认采用 1 pt thickness、基线下 1 pt、继承 `style.color`。

- 没采用 `thickness / 2` offset：行内线以文字 baseline 为参照，块级 HLine 的中心线公式不适用。
- 没采用固定黑色：段落改色后出现黑色 blank 会显得割裂；`Option<Color>` 能自然表达“继承，或显式覆盖”。
- offset 采用 signed finite length：默认仍在下方，但“相对基线”本身是有方向的；限制成 nonnegative 会无必要地禁止调用方轻微上调。它仍在构造期排除 NaN/∞。
- thickness 和 offset 都不进入 line metrics。若调用方选择极粗或极低的线，视觉重叠是显式样式选择，而不是扩行高的理由。

## 为什么 em 在布局时解析

`blank_em(n)` 的定义是 `n * 当前字号`，不是字体中某个汉字 glyph 的 advance。

考虑过在 `Paragraph` builder 调用时直接闭包捕获字号，但当前 `Paragraph` 并不拥有 `Style`，这样会扩大 builder 耦合，并可能与未来样式作用域不一致。保留一个固定宽度的 `Em(n)` 表达，在 `TextNode` 已持有最终 `Style` 且开始布局时解析，既符合语义，也不触发 glyph 测量。

## breaker 取舍

### 不用字符占位

拒绝以下表示：

- `"____"`：长度依赖字体、kerning/advance，且得到多条字形轮廓而不是一条 stroke；
- 普通空格：会进入 cmap，使用 `PairClass::Sp`，还可能在行首被丢弃；
- 全角空格：仍是 glyph/cluster，宽度依赖字体；
- 私用区 sentinel：引入缺字和文本提取污染。

blank 必须是专用非字形 atom。

### fill 在 pack 时解析

考虑过在组装期、测量期或 fragment 绘制期解析 fill，均拒绝：

- 组装期不知道容器实际 `W`；
- 测量投影时不知道 blank 会落在首行 `W'` 还是后续 `W`，也不知道本行已用宽；
- 绘制期才补线会让 breaker、`BrokenLine.width`、Center/End 对齐和后续 x 全部看不到这段宽度。

pack 接纳 fill 的瞬间同时知道 `avail` 与 `line_width`，因此唯一正确的公式是 `avail - line_width`。fill 计入最终 line width 后，该行的对齐偏移自然为零。

### 首行使用 W'

没有把 blank 特判成使用全框 `W`。§2.1 已明确首行预算是 `W'`，现有 breaker 也会在首行不可拆内容超过 `W'` 时报告该预算。fixed blank 和 fill 应遵守同一视觉行预算，否则下划线会越过首行缩进后的右边界模型。

### fixed blank 的断行边界

fixed blank 自身始终是一个不可拆 atom。它不是 `Sp`，也不会在行首消失。当前行剩余不足时允许在它之前断行，使整个 blank 移到下一行；没有把它粘在前文。

blank 后仍可能有文本。为避免把 blank 无意中粘成后续单词的一部分，breaker 使用显式 inline boundary predicate：blank 后通常允许断；若下一个 text atom 是 `PairClass::Cl`，则禁止让闭口标点独占行首。blank 前则按本刀默认始终是机会。这个规则不冒充 Unicode pair class，也不修改现有 6×6 文本矩阵。

fill 不需要 after 规则，因为组装期保证它终止段落。

## 为什么断行结果必须有 parts

考虑过只给现有 `BrokenLine` 增加总宽和 blank 列表，但拒绝：现有单个连续 `glyph_range` 无法说明 blank 插在 glyph tape 的哪个位置。若只知道总宽，`glyph_runs_for_line` 仍会连续累加 advance，blank 后文字不会右移。

`LinePart::Text | Blank` 保留顺序，既能按 blank 推进 x，也能把真实 text span 继续交给现有按字体切分的 GlyphRun 逻辑。显式 span/cursor 还顺便消除 atom 下标等于 cluster 下标的脆弱假设；这是接入非 cluster atom 所必需的变化，不是无关重构。

文本必须按 blank 分隔的连续组做 clustering。若先移除 blank 再串接文本，blank 两侧可能被错误合成一个 grapheme cluster，尤其是后一侧以 combining mark 开头时。

## 为什么 stroke 存在 LineBox

考虑过在 fragment 阶段根据原始 items 重跑 horizontal layout，但拒绝：分页后的 `ParagraphNode` 已不应重新断行，重新解析 fill 还可能使用错误的页/行状态。

把 resolved stroke geometry 与 GlyphRun 一起烘焙到 `LineBox`，分页只需切行。fragment 阶段仅增加行 y offset 并翻译成已有 `FragmentCommand::Stroke`，外层 column/list/page offset 则继续由 `Fragment::with_offset` 统一处理。

每个正宽 blank 发一条 Path 和一个 Stroke，而不是逐段画 underscore glyph。零宽省略命令，避免退化路径，同时保留 0 advance 语义。

## 为什么不复用 HLineNode

`HLineNode` 的 width 相对整个 frame，拥有自己的垂直盒、space before/after 和不可拆 flow 分页语义。inline blank 的起点是当前行笔位置，高度完全来自字体，并随所在文本行分页。

两者只能复用底层 `Path::line`、`Paint::solid`、`Stroke::new` 和 `FragmentCommand::Stroke`。复用 `HLineNode::split` 会错误增加高度、丢失左右文本，并把行内项变成独立 flow block。

## 测试策略取舍

RecordingCanvas 是路径端点、glyph x、颜色、stroke width、分页归属与绘制顺序的权威测试面。它能直接证明：

- blank 后文字确实右移；
- fill 终点使用局部 frame；
- stroke y 为 baseline + offset；
- blank 不增行高；
- stroke 与行一起跨页。

backend-krilla 只保留 `%PDF` 冒烟。压缩内容流、浮点序列和后端默认 line cap 都不应成为排版断言；几何端点以 RecordingCanvas 的 `StrokePath` 为准。
