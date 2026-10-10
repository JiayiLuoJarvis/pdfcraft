# Knife 14 — 设计 rationale（candidate-1）

本文记录本方案在关键分叉上的取舍，以及被拒绝的替代路线。默认项与 `.audit/knife-14/arena/frame.md` autonomy 一致；未写 **reverse word** 的条目表示接受默认。

---

## 1. 行内联合 vs 其他建模

| 方案 | 结论 |
| --- | --- |
| **A. `Vec<InlineItem>`（Text \| Blank）** | **采用。** 与提案「与 text/bold 并列的行内占位」、§2.2 互斥宽度、组装期 fill 终止一致；blank 天然打断 text 合并。 |
| B. 文本 `decoration: underline` | **拒绝。** 提案明确不是给已有字加 decoration；且无法表达「纯空白宽度」。 |
| C. 新 `FlowNode`（块级 blank） | **拒绝。** 与 §2.2 行内语义冲突；会 duplicate 分页/框宽规则。 |
| D. Unicode 下划线 / 全角空格 / U+0020 占位 | **拒绝。** 进入 cmap 与 `PairClass::Sp`（行首丢弃）；违背「blank 不进字形测量」与 rubric laziness。 |
| E. 独立 parallel run 类型但不改断行结果 | **拒绝。** `glyph_range` 连续区间无法表达 middle blank 后 glyph 右移；必须在 broken line 上保留 atom/segment 回放。 |

---

## 2. 定宽 vs 吃到行尾 API

| 方案 | 结论 |
| --- | --- |
| **A. 分方法：`blank` / `blank_em` / `blank_to_end`** | **采用。** frame 默认；互斥在类型层（`BlankWidth` enum）+ 组装 flag，调用方误用面小。 |
| B. 单一 `blank(BlankSpec)` enum builder | **拒绝（reverse：`one-builder-enum`）。** 合法组合少，但 DSL 可读性不如动词方法；与现有 `text`/`bold` 并列风格弱。 |
| C. 定宽与 fill 共用一个 `Option<Width>` | **拒绝。** 无法表达「互斥」不变量；易双设或双空。 |

---

## 3. fill 宽度解析时机

| 方案 | 结论 |
| --- | --- |
| **A. 在 `pack_lines` 接纳 `BlankFill` 时：`avail - line_width`** | **采用。** 唯一已知 `W_remain` 的时刻；与首行 `W'`、硬换行后行预算自动一致。 |
| B. 在 `project_atoms` 预计算 fill 宽 | **拒绝。** 投影期不知道行内已用宽度，也不知道 atom 落在第几条 visual line。 |
| C. 仅在 `fragment_from_lines` 画线时用 frame 宽减 glyph 宽 | **拒绝。** `BrokenLine.width` 不含 fill 时 Center/End 错位（how.md gotcha #4）。 |

---

## 4. 断行器扩展方式

| 方案 | 结论 |
| --- | --- |
| **A. 扩展 `Atom` + 共用 `pack_lines` 逻辑** | **采用。** 最小侵入；定宽 blank 与 `Ink` 同属「占宽 atom」；fill 多一步解析宽度。 |
| B. 段落内二次 pass：先 break 纯文本再插入 blank | **拒绝。** fill 与定宽参与断点决策；后插入会破坏 overflow / `UnbreakableWiderThanFrame` 语义。 |
| C. 完全重写 UAX 级 breaker | **拒绝。** 超出 knife；AGENTS.md 禁止完整 UAX #14。 |
| D. blank 伪装为超宽单 cluster | **拒绝。** 污染 cluster tape；破坏 cursor / BK 映射；仍无法 fill 动态宽。 |

**`Hard` 携带 `after_cluster`：** 拒绝继续假设 atom index == cluster index（how.md gotcha #3）。显式 cluster 引用成本低于后续 knife 再踩坑。

---

## 5. 定宽 blank 的断行 pair 策略

| 方案 | 结论 |
| --- | --- |
| **A. 新 `PairClass::InlineBlank`；允许在 blank 前断行，整颗挪下一行** | **采用。** 满足 frame「break allowed immediately before blank；never Sp」。 |
| B. 使用 `PairClass::Sp` | **拒绝。** 行首丢弃 + 空格 cmap 语义；rubric 明确禁止。 |
| C. **glue-to-prev**（与左侧不可断） | **拒绝（reverse word 未启用）。** 定宽大于剩余宽时更易无辜 `UnbreakableWiderThanFrame`；中文公文中「签字：____」更常希望整段 blank 换行。 |
| D. 定宽可拆成两段 stroke | **拒绝。** 违反提案「不可拆占位」。 |

**overflow 比较宽度：** 采用 **visual line 的 avail（首行 `W'`）**，与现有 `UnbreakableWiderThanFrame` 文本行为一致；不把提案里的「W」强行读成未扣 indent 的全框宽（how.md §5）。

---

## 6. 行布局：LineBox 形状

| 方案 | 结论 |
| --- | --- |
| **A. 有序 `LineSegment`（Glyphs \| Stroke）** | **采用。** 回放 atom 顺序一次 walk；middle blank 自然推进 x；fragment 生成顺序清晰。 |
| B. 仅增加 `blank_strokes: Vec<...>` 与 `GlyphRun` 并行 | **拒绝。** 同一条线多 blank / text 交错时，绘制顺序与 x 依赖隐式排序。 |
| C. 第二遍扫描 glyph tape 按总宽插 stroke | **拒绝。** 无法恢复 blank 在 ink 之间的位置（tape 中 ink 相邻）。 |

---

## 7. Stroke 默认与属性

| 方案 | 结论 |
| --- | --- |
| **A. 默认 thickness 1 pt、offset 1 pt、`color = style.color`** | **采用。** frame autonomy；与 HLine 默认分离（HLine 线心在盒内，baseline 语义不同）。 |
| B. 照搬 HLine：`offset = thickness/2` | **拒绝（reverse：`offset-half-thickness`）。** 原点是文字 baseline 而非线盒；会切字或浮太高。 |
| C. 默认 always-black | **拒绝（reverse：`always-black`）。** 与 §2.2「默认跟当前文字色或黑」在默认样式下等价，但显式色样式时更一致。 |
| D. v1 不允许覆盖 thickness/color/offset | **拒绝。** 提案属性表含线粗/线色/偏移；用 `BlankStroke` + `blank_styled` 控制面，避免 10 个 `blank_*` 方法。 |

**零宽定宽：** 采用 **omit stroke**（`NonNegative` 0）；拒绝 **positive-span** 强制正宽（frame 默认允许 0）。

---

## 8. `blank_em` 解析时机

| 方案 | 结论 |
| --- | --- |
| **A. layout 期 `n * style.font_size`** | **采用。** 未来段落级字号样式时自动跟随；组装期 `Paragraph` 无 `Style` 引用也可保持 today 的 `Flow::paragraph` 形状。 |
| B. builder 闭包内乘字号 | **拒绝（reverse：`close-over-builder-size`）。** 需在 `Paragraph` 存 Style 快照或改 `Flow::paragraph` 传 style 进闭包，耦合更大。 |

---

## 9. fill 终止与错误类型

| 方案 | 结论 |
| --- | --- |
| **A. 组装期 `fill_sealed` + 首个错误存 `Paragraph`** | **采用。** 与 `Flow::push(Err)` 模式一致；空 `text` 不触发终止后违规。 |
| B. layout 期才发现 fill 后有 text | **拒绝。** 违背提案「组装期报错」；浪费 measure/break。 |
| C. 新错误变体 vs `InvalidGeometry` | **弱偏好 `InvalidGeometry` 或小型 `BlankAfterFill`。** 二者皆可；rubric 允许复用现有 variant。不在设计阶段钉死 Display 文案。 |

---

## 10. `W_remain == 0` 的 fill

| 方案 | 结论 |
| --- | --- |
| **A. 占宽 0，不画 stroke** | **采用（frame 默认）。** 与 `Fixed(0)` 一致；避免零长 path 与 Layer 分歧。 |
| B. `error-on-zero-fill` | **拒绝。** 硬换行后空行尾 fill 等场景过严。 |

---

## 11. 范围外刻意不做

- **Cell / Band / Layer blank：** frame 默认 out；table cell 仍走 `TextNode::new` 字符串，本 knife 不扩 cell DSL。
- **underscore 字形、虚线/双线、线上叠字：** 提案 v1 不做 / rubric laziness。
- **krilla cap 外扩建模：** 布局只保证 path 端点；与 HLine 测试先例一致，不为本 knife 增 backend 几何。

---

## 12. 与 candidate 池的差异化预期

本方案核心赌注是：**一次 atom 流扩展 + 有序 LineSegment**，把 blank 严格塞进现有 paragraph 管线，而不是绕开 breaker 或后处理 glyph x。若评审更偏好「最小 diff 只改 paragraph 不改 line_break」，会看到另一 candidate 把 pack 复制到 `layout/paragraph.rs`——本方案认为 duplicate pack 长期更贵，故选择扩展 `Atom` 并修正 `Hard` cursor。
