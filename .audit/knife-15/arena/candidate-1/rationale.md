# Knife 15 candidate-1 — rationale

## 与提案 / frame 的对齐

- **仅文字外链 URI**：数据挂在 `TextRun` / 测量组，PDF 侧只写 `LinkAction` 字符串；不碰 destination、图链、Layer。
- **空 URI**：沿用 `LinkTarget::uri`，不在 layout 重复校验。
- **每视觉行一片段一个矩形**：靠 `emit_line` 切组 + `place_line` 按 seg 记录 x，换行自然产生多个 `AnnotateLink`。
- **页眉页脚**：layout 已与正文同管线；DSL 缺口用 `Band::*_link` 整槽补全，避免本刀引入 Band 内富文本 `Paragraph`。

**Autonomy defaults：** 全部接受 frame 表中的默认项；**无 reverse word**（未采用 `chain-last`、`band-paragraph`、`blank-no-link` 的反向含义等）。

---

## 采纳的主线

### 闭包式 `p.link(uri, |inner| …)`（默认 DSL）

**理由：** URI 是 run 级属性，闭包明确作用域，避免「链到最后一个 run」在 `blank` / 嵌套 face 时语义模糊；与 §1 流式闭包风格一致。嵌套用 `active_link` 栈一层即可。

### 组级 `MeasuredInline.link` + link 边界 flush（非 `Span` payload）

**理由：** `Seg::Glyphs` 与 `cluster_tagged` 标签均为 `Copy` 约束；`LinkTarget` 含 `String` 不能进 span payload。在 blank 同款 flush 点按 link 变化切组，改动面小，且保证每个 seg 对应唯一 URI。

### `LinePiece::Link` → `FragmentCommand::Link`（非 paint 时才算 rect）

**理由：** x 在 `place_line`（含 align、缩进）才确定；y 在 `fragment_from_lines` 按行 index 叠加。与 `InlineStroke` 模式一致。在 `place_line` 直接 emit 完整 `Rect` 也可，但 y 仍要在 fragment 阶段填，故 x0/x1 延迟成 rect 更清晰。

### 一条 link piece 覆盖整 seg（不按 `glyph_runs_for_line` 切分）

**理由：** §8 要求「落在该段文字上的矩形」；bold/body 同链应为一个 hit box。若按 `GlyphRun` 切分会产生多个窄 rect，且与「链接段 ≠ font 段」explorer 结论相反。

### `Band::left_link` / `right_link` 整槽（默认 band-slot-link）

**理由：** 页眉页脚真实需求是整行公司名/URL；让 Band 接受完整 `Paragraph` 会扩大 API 与测量失败面（band 溢出错误路径）。单 run + `Option<LinkTarget>` 足够覆盖 §8「同管线」。

### `skip-empty` / `no-decoration` / `line-box-height`

**理由：** 与现有 `append` 空串忽略一致；提案未要求链接样式；行高用 body metrics 的 `line_height` 与 explorer 对 y0 的定义一致，bold 不改变行盒高。

---

## 拒绝的备选方案

### `chain-last`：`p.text("…").link(uri)` 或 mut last run

**拒绝原因：** 需要可变的「上一 run」句柄或 builder 返回链，与当前 `Paragraph` 消费式 API 不一致；`text` 后接 `blank` 时「last run」边界易错；测试与 coalesce 交互更难说明。

### `band-paragraph`：Band 槽位接受 `impl FnOnce(&mut Paragraph)`

**拒绝原因：** 页眉带宽固定、溢出即错；富文本 band 超出 knife 15 范围；与 frame laziness 和「cell-link-in 反向 = 本刀不做 Cell」一致。

### 在 `cluster_tagged` 增加 `Copy` 标签索引 + 侧表 URI

**拒绝原因：** 可行但 indirection 更高；`place_line` 仍要在 seg 级聚 x，最终仍要 link 边界 flush；不如组上挂 `Option<LinkTarget>` 直观。

### 在 `pack_lines` / `break_spans` 产出 link rect

**拒绝原因：** 该阶段无 frame align、无 `line_x`、无 `y0`；与 explorer-layout-break 结论一致。

### 事后从 `GlyphRun.origin` 反推 link 范围

**拒绝原因：** font 切分 ≠ 链接切分；会错误拆分 bold/body 同链，违反 rubric「Bold/body inside one link stays one rect per line」。

### 并行注解路径（layout 直接队列 krilla / 新 trait 方法）

**拒绝原因：** 违反 AGENTS.md 与 frame「Fit to seams」；`Canvas::annotate_link` 与 `RecordingCanvas` 已完备。

### `LinePiece` 省略，仅在 `draw_fragment` 扫 `Glyphs` 猜链

**拒绝原因：** `GlyphRun` 无 URI 字段；要在 draw 阶段重建需额外平行结构，等价于 `FragmentCommand::Link` 但更隐蔽。

### 链接内 blank 仍可点（`blank-no-link` 的反向）

**拒绝原因：** blank 无 `TextRun`、无 ink；给 blank 加 URI 无 §8 依据，且与「blank 结束测量组」模型冲突。

### 本刀做 Cell / `Flow::text` 字符串链

**拒绝原因：** frame 默认 `cell-link-in` 为 out；表格单元格走 `TextNode::new` 无 `Paragraph`；避免 scope creep。

### `Layer::text_at` 外链

**拒绝原因：** `single_line_run` 独立路径；§8 第一版范围未覆盖；frame `layer-out`。

### 链接仅 URI 无文字时报错

**拒绝原因：** 提案未要求；与现有空 `text` 忽略一致；`skip-empty` 默认更懒、更稳。

### `Paragraph::link` 失败只 panic

**拒绝原因：** 与 `LinkTarget::uri` 的 `Result` 和 §2.0 构造期拒绝精神不一致；应 `EmptyLink` 可测。

---

## `link` 返回 `Result` vs `Paragraph::error` 的取舍

设计正文推荐 **`link -> Result<()>`** 并在文档说明与 `Flow::paragraph` 非 Result 闭包的衔接；备选 **`Paragraph::error`** 与 fill-blank 一致。实现 PR 应择一并在测试中覆盖 `EmptyLink`；本 candidate **略倾向显式 `Result`**，因 URI 错误来自调用方输入而非排版状态，与 `RunAfterFillBlank` 性质不同。

---

## 风险与已知非目标

- **行尾空格**：explorer 指出链接 rect 可能含 trailing space 宽度；本设计接受（与 seg advance 一致），不 trim。
- **front 层遮挡页眉**：PDF 阅读器 hit-test 行为不在库控制范围；本刀不特殊处理。
- **跨页 tail**：`fragment_from_lines` 对 tail 从 `index = 0` 重算 y 正确；link rect 必须在片段局部坐标，已通过 `with_offset` + `draw_fragment` 保证。

---

## 小结

candidate-1 走 **最小 seam**：两层 coalesce 修 key + `prepare_inline` link flush + `place_line`/`fragment_from_lines` 产出 `FragmentCommand::Link`，DSL 用 **闭包 link** 与 **Band 整槽 link**，全部 autonomy 默认，无 parallel backend、无 decoration、无 cell/layer 扩展。
