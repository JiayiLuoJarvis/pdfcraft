# Knife 13 arena cross-judge

## 评分

| 评分项（每项 0–5） | Candidate 1 | Candidate 2 | Candidate 3 | Candidate 4 |
| --- | ---: | ---: | ---: | ---: |
| 1. Proposal fidelity | 5 | 4 | 5 | 3 |
| 2. Caller surface | 4 | 3 | 5 | 1 |
| 3. Layout honesty | 3 | 5 | 3 | 1 |
| 4. Fit to existing seams | 3 | 4 | 5 | 2 |
| 5. Testability | 4 | 5 | 5 | 3 |
| 6. Laziness | 5 | 3 | 5 | 1 |
| **总分 /30** | **24** | **24** | **28** | **11** |

### 评分依据

- **Candidate 1** 的外部面和范围控制很好，`W' = W - indent`、首行 x、真实 `FontId` 和开页前失败也都写对了；但它要求 `Flow::paragraph` lower 时读取只由 `Document` 持有的 `FontRoles`，与当前 `Document: DerefMut<Target = Flow>`、`Flow` 不持有字体状态的结构不闭合。`RunTape` 的原始字节范围如何穿过 CRLF 规约和 BK 从 `glyph_text` 删除也没有给出可执行映射，按 span 直接 `measure_text` 还可能把换行送入 cmap。
- **Candidate 2** 的 tagged cluster → measured inline tape → 多 `GlyphRun` 是四案里最完整、最能保证量画同源的内部设计；不同 advance 的粗体 fixture 也能真正钉住断行。但它同时加入整套段落样式 setter、公共 `LineHeight`、新字体配置错误和 metrics envelope，并移除 `Style.font`，使本刀公共面与改动半径明显偏大。
- **Candidate 3** 最贴合现有 seam：保留 `Document::font -> Result<()>`，只增加 `bold_font`、`p.bold`、`p.first_line_indent`；字体槽在布局可见的 `FontStore` 中，段落仍 lower 成一个节点，Band/Layer/Cell 明确不扩展。主要扣分在内部测量：逐簇调用现有 `measure_text` 会反复构造 skrifa `FontRef`；“跨 run 字素簇全部跟随第一个标量”还会让某些 `p.bold` 标量实际使用 regular face。这两点必须在实现前修正。
- **Candidate 4** 同时扩张到 Document/Flow 分层样式、`StyleBuilder`、`StylePatch` 和泛型预算 trait，违反本刀的最小面目标；其示例与 `style` 的 `Result` 闭包签名也不一致。更严重的是“grapheme 不会跨 span”这一前提错误，逐 span 测量无法自然对齐统一规约后的 cluster/glyph 坐标，CRLF 和跨 run combining mark 都会破坏方案。

## 推荐基线

推荐 **Candidate 3**。

它提供最小且与提案书 §1 一致的调用面，保留现有 `Style` / `TextNode` / `Paragraph` / `break_lines` 的所有权边界，字体角色也放在排版阶段实际可访问的 `FontStore` 中。对维护者而言，正文段落富文本、固定位置纯文本和段落样式三者仍是清楚分开的；将来确认更多样式时可以继续扩 Paragraph，而不用先背负一套公共层叠系统。

基线采用前有一项强制修正：不要实现 Candidate 3 的“逐簇 `measure_text` + 首标量决定整簇字体”。应移植 Candidate 2 的 tagged clustering 与按连续字体角色批量测量，让规约只发生一次，并让同一 grapheme 内每个 scalar 仍使用调用方指定的真实 `FontId`。

## 可移植思路

### From Candidate 1

1. `RunTape` 的“拼接文本与字体 span 由一个对象共同拥有”、相邻同字体 span coalesce，适合作为 measured-inline 输入的不变量；但范围必须建立在规约后的 glyph 坐标上，不能直接沿用原始字符串范围。
2. 字体角色一旦解析为 `FontId`，后续 layout 只消费 id、不再查询文档角色表，这个阶段边界值得保留；解析时机应放在 plan/layout，而不是当前 `Flow::paragraph` 调用现场。

### From Candidate 2

1. `cluster_tagged` / `TaggedClusteredText`：在同一次 CRLF 规约、grapheme segmentation 和 BK 删除中保留每个 glyph scalar 的字体角色。这是对 Candidate 3 的必需补丁。
2. 使用 advance 与 regular 不同的可复现 bold fixture，除 RecordingCanvas 的 `FontId`/origin 外，再钉住粗体宽度确实参与断行。

### From Candidate 4

1. `DualWidth { first, rest }` 这个命名能清楚表达断行预算；本刀用具体 crate-private struct 即可，不需要开放 `LineWidthBudget` trait。
2. Flow 级样式继承可留作提案确认后的独立 knife；届时可复用当前 clone-by-value 的 `Flow.style`，不应在本刀提前引入 `StyleBuilder`/`StylePatch`。

## Red flags / reject reasons

### Candidate 1

- **拒绝直接作为基线**：`Flow::paragraph` 无路径读取 `Document::FontRoles`，lowering 数据依赖没有闭合。
- 原始 `RunTape.byte_range` 与规约后的 `ClusteredText::glyph_text` 之间缺少正式映射；CRLF、BK 和跨 run grapheme 都可能错位。
- 把现有 `Document::font` 扩成返回公共 `FontId` 并非本刀所需，额外增加了第二条字体选择路径。

### Candidate 2

- **不直接作为基线的原因是范围，不是内部正确性**：公共段落样式面、`LineHeight`、字体配置错误与 metrics envelope 同时落地，超过 `p.bold` 和首行缩进的必要脚手架。
- 从 `Style` 移除 `font` 会迫使 `single_line_run`、列表 marker、Band、Layer、Cell 一起改走 `FontSlots`，改动半径大于需求。
- “bold 必须恰好是紧随 regular 的第二次注册，否则新增配置错误”是额外政策；显式槽已经足以保证真实 face，无需再冻结注册序列。

### Candidate 3

- 跨 run grapheme 不能静默全部采用首标量 face；否则调用方写入 bold run 的 combining mark 可能以 regular 绘制。
- 不可按 grapheme 高频调用当前 `measure_text`；该函数每次都会重新构造 skrifa `FontRef`/charmap/metrics。应按规约后的连续字体角色批量测量。
- 重复 `bold_font` 的“最后一次绑定”以及缺槽时报约定 id 1，需要测试和文档固定；不要让实际 bold id 与错误语义混淆。

### Candidate 4

- **拒绝**：其核心假设“grapheme 不跨 span”不成立；统一断行 tape 与逐 span 测量结果无法可靠对齐。
- `doc.style` / `Flow::style` / `StyleBuilder` / `StylePatch` 是未经本刀证明必要的平行公共样式系统。
- 示例闭包返回值与声明的 `FnOnce(...) -> Result<()>` 不匹配，README 风格调用不能按草图直接编译。
- 泛型 `LineWidthBudget` 为当前仅有的 uniform/first-rest 两种预算增加了不必要抽象。
