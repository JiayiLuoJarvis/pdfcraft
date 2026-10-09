# Rationale — Candidate 1 (RunTape + ParagraphStyle)

## Problem

Knife 13 要在现有「一段一字一 `Style`、一行一 `GlyphRun`」管线上接入提案书 §1 的 `p.bold` 与 §2.1 首行缩进，且粗体必须是第二个 `FontId` 的真实测量，不能合成加粗。当前 `Paragraph` 在 DSL 期就把 run 边界拼平成 `String`，layout 只在已知框宽 `W` 时断行一次。若只在 `Style` 上增加 `bold: bool` 和 `first_line_indent: Option<…>`，会把段级几何与 run 级字体绑在同一快照里，lower 时要么整段换 font（破坏混排），要么 draw 期再分支（断行宽度错误）。设计必须在 **不引入具名样式表** 的前提下，让同一 `TextNode` 内多 face 统一断行，并在 `start_page` 之前报告 bold 未配置、缺字与 `W' <= 0`。

## Usage (caller's view)

见 [`design.md`](design.md) Quickstart 与三个调用点。调用方心智模型：**段落闭包里写 run（`text` / `bold`）和段属性（`first_line_indent`）**；文档级 **`bold_font` 一次绑定**；默认无缩进。与 §1 模板一致，公共面刻意小于 pdfmake 的全套 style 键。

## Shape

RunTape（已解析 `FontId` 的 span 列表 + 拼接全文）与 ParagraphStyle（现有 `Style` + 非负 `first_line_indent`）分离。DSL 的 `Paragraph` 只缓冲 `PendingRun` 和 indent override；`Flow::paragraph` lower 时解析 bold 角色、合并 Flow 继承样式、产出单个 `TextNode`。Layout 构建 `MeasuredTape`，在扩展后的 `break_lines` 上对首条 visual line 用 `W' = W − indent`，后续用 `W`；每 `LineBox` 持多个同 baseline 的 `GlyphRun`，首 run 的 x 含 indent 与对齐。文档级 `FontRoles.bold` 仅在 lower 读取，layout 只信任 tape 内的 id（per separate-before-serializing-shared-state）。行高暂用段默认 regular face 的 metrics（autonomy default；reverse word `max-run-metrics`）。

公共面隐藏：cluster 投影、变 avail 断行、按 font 切 run、首行几何——调用方只碰 `Paragraph` 与 `bold_font`（interface depth）。

## Synthesis decision

Filled by arena.

## Tradeoffs accepted

- 我们接受 **行高一律按 regular face metrics** 换取实现简单与测试稳定；极端情况下 bold ascent 更大可能贴线，换 `max-run-metrics` 需另开决策。
- 我们接受 **`bold_font` 文档级绑定**（第二注册 face）换取 DSL 里 `p.bold` 零参数；若需多套 bold，须 future API，不在本刀。
- 我们接受 **lower 时 coalesce 相邻同 FontId span** 换取 layout 热路径更短，代价是 lower 多一步扫描。
- 我们接受 **Band/Layer/Cell 不获混排/缩进 builder** 换取 scope 可控；`Style.first_line_indent` 对它们恒为 0。
- 我们接受 **扩展 `break_lines` 签名**（首行 indent 参数）换取不在 paragraph 层做「假断行再合并」的第二套算法。

## Alternatives considered

- **在 `Style` 上堆 `Option` 字段 + 单字符串内联 markup**：caller 仍写 `p.text` 一种方法，但 run 边界丢失或需二次解析；layout 要么整段一种 font，要么在 draw 换 font——违反测量一致性；对调用方暴露的复杂度低，对 layout 正确性隐藏灾难（information leakage）。拒绝。
- **每个 run lower 成多个相邻 `FlowNode::Text`**：实现复用现有单 font 路径，但块级分页与 `paragraph_spacing` 重复、无法跨 run 断行——与 how.md 硬约束冲突。拒绝。
- **保留 `String`，并行 `Vec<(Range, FontId)>` 而不命名 RunTape**：与 tape 模型等价但易与 `TextNode.text` 双源；RunTape 强制单拼接源 derive cluster（per single source of truth）。tape 形胜出。
- **公共 `Document::font` 返回 id，调用方自行 `set_bold(FontId)`**：更灵活，但 §1 模板不要求；`bold_font` 更浅。本 candidate 选 `bold_font`；若 arena 要 graft 返回 id，可并存。

## Open questions and risks

- 首行缩进 + **Center/End** 对齐时，`align_x` 的 content box 是否一律用「该行实际 avail」（首行 `W'`，续行 `W`）？本设计按 how.md 公式实现，需 RecordingCanvas 固定期望，避免与 pdfmake 默认歧义。
- Run 边界落在 **grapheme 中间**（UTF-8 切在组合字符中间）时，DSL 应否在 `push_str` 边界拒绝？当前假设调用方按 Rust `str` 合法边界传入；是否要 debug 断言？
- **`UnknownFont { id: 1 }` vs 专用 `BoldFontUnconfigured`**：哪种更利于调用方？与现有 error 枚举风格需统一。
- 测试字体仅 ExtraLight  duplicate：能否充分覆盖「bold 缺字而 regular 有字」？需 fixture 子集或 synthetic cmap 差分。

## Next implementation step

在 `document.rs` 实现 `Paragraph` 的 run 缓冲与 `Flow::paragraph` lower 到 `RunTape` + 单 span 回归测试，确保未启用 bold 时行为与 f208329 逐字节一致。
