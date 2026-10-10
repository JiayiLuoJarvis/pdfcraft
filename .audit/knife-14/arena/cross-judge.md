# Knife 14 Arena Cross-Judge

## 1. 分项评分

### Candidate 1

| 准则 | 分数 | 一句证据 |
| --- | ---: | --- |
| Proposal fidelity | 5/5 | 方案完整落实 Fixed XOR fill、定宽不可拆、fill 组装期封口、blank 不测字形、单 Stroke、不增行高及随行分页。 |
| Caller surface | 2/5 | 五个 blank 入口加 `BlankStroke` 偏大，且自定义示例依赖当前不可公开的 `Style` 经 `from_style` 构造并使用了基线中不存在的 `Length::pt`。 |
| Layout honesty | 4/5 | `W'`、pack 内 fill、行内推进 x、baseline 加 offset 与提前失败均正确，但把所有 Text 片段送入单一 `cluster_tagged` 未明确保留 blank 的聚类边界，可能让 CR/LF 或组合字符跨 blank 合并。 |
| Fit to seams | 5/5 | 它扩展 `InlineItem`、breaker atom、`LineBox`/fragment，并复用现有 `FragmentCommand::Stroke`、`Path`、`Paint` 与 `Stroke`，没有另起块级节点或样式系统。 |
| Testability | 5/5 | 测试计划覆盖 stroke 端点、blank 后 glyph x、行高、fill 后 run、首行过宽、分页、对齐与 `%PDF` 冒烟。 |
| Laziness | 5/5 | 没有引入下划线字形、具名样式、Cell/Band/Layer blank、虚线双线或线上叠字。 |

### Candidate 2

| 准则 | 分数 | 一句证据 |
| --- | ---: | --- |
| Proposal fidelity | 5/5 | 两种宽度、不可拆定宽、terminal fill、非字形测量、单 Stroke、固定行高和随行分页均有明确数据与算法落点。 |
| Caller surface | 4/5 | 三个 branded-length 方法加一个属性 builder 足够小且非法宽度不可表示，但默认 blank 也必须写 `|_| {}`，比相邻的 `p.text` / `p.bold` 更嘈杂。 |
| Layout honesty | 5/5 | 它按 blank 分隔连续文本组，显式携带 cursor/span，在 pack 中用视觉行 `avail - line_width` 解析 fill，并正确推进后续 glyph 与 baseline stroke。 |
| Fit to seams | 5/5 | 方案沿现有 paragraph、line breaker、`LineBox` 与 fragment 接缝扩展，并只复用已有 stroke 几何和命令。 |
| Testability | 5/5 | 单元、布局、RecordingCanvas、空画布失败和 backend 冒烟测试覆盖了 rubric 指定的全部观测点。 |
| Laziness | 5/5 | 范围严格停在 Paragraph inline blank，明确排除 Cell/Band/Layer、underline decoration、字形占位和花式线。 |

### Candidate 3

| 准则 | 分数 | 一句证据 |
| --- | ---: | --- |
| Proposal fidelity | 5/5 | 它逐项满足 §2.2 和 frame 的固定/填满语义、错误时机、非字形测量、单线绘制、固定行高与分页归属。 |
| Caller surface | 5/5 | 仅三个直观构造方法，默认调用无需空闭包，可选样式通过短生命周期 `Blank<'_>` 链式句柄设置，宽度互斥且长度约束类型化。 |
| Layout honesty | 5/5 | `break_spans` 保留文本组和 blank 的真实顺序，Hard 自带 group/cluster，fill 在 pack 解析，`LinePiece` 走笔推进 x，并严格使用首行 `W'`。 |
| Fit to seams | 5/5 | 它保留纯文本 `break_lines` 门面，同时把 inline item、breaker segment、有序 `LinePiece` 和现有 stroke fragment 接成一条管线。 |
| Testability | 5/5 | 计划既锁定路径、glyph x、行高和错误，也覆盖 hard break、空 run、列表/列偏移、分页和现有纯文本回归。 |
| Laziness | 5/5 | 公共面和实现范围都没有越过本 knife，且没有借机增加 Cell blank、文字装饰、花式描边或新后端能力。 |

### Candidate 4

| 准则 | 分数 | 一句证据 |
| --- | ---: | --- |
| Proposal fidelity | 5/5 | 设计明确实现两种互斥宽度、fill 封口、不可拆 blank、非字形测量、单 Stroke、不增行高和随行分页。 |
| Caller surface | 2/5 | 六个公开入口加 `BlankBuilder` 明显偏大，且 usage 与默认实现反复调用基线并不存在的 `Length::pt`，示例闭包也缺少 `Ok(())`。 |
| Layout honesty | 4/5 | `W'`、pack-time fill、x 游标、baseline offset 与提前失败都正确，但单一 glyph range/tape 的说明没有像 Candidate 2/3 那样保证 blank 两侧分组聚类，存在跨 blank 规范化风险。 |
| Fit to seams | 5/5 | 它正确扩展 paragraph、breaker、`LineBox` 和 fragment，并复用现有 stroke 几何、画布与 backend 通路。 |
| Testability | 5/5 | 测试覆盖 rubric 要求的路径端点、后续 glyph 位移、行高、组装错误、首行过宽、分页和 `%PDF` 冒烟。 |
| Laziness | 5/5 | 明确不做 Cell/Band/Layer blank、字形下划线、线上叠字、虚线或双线。 |

## 2. 总分

| Candidate | 总分 |
| --- | ---: |
| Candidate 1 | 26/30 |
| Candidate 2 | 29/30 |
| Candidate 3 | **30/30** |
| Candidate 4 | 26/30 |

## 3. 推荐基础方案

推荐 **Candidate 3**：它同时给出最小且自然的三方法调用面、最诚实的分组文本/blank 断行数据形状、保持兼容的纯文本 breaker 门面，以及按阅读顺序保存 glyph/stroke 的 `LinePiece`，因此在可扩展性与小 API 上都优于其余候选。

按 Laziness Protocol，应以 Candidate 3 原样作为底座，不合入 Candidate 1/4 的额外 convenience overload，也不提前扩展 Cell/Band/Layer、具名样式或更多线型。

## 4. 可从落选方案移植的部分

- Candidate 1：移植其清晰的分阶段实现顺序（items/API → breaker → line pieces → 集成测试），以及 atom 含 blank 时专门锁定 Hard/eof cursor 的回归测试。
- Candidate 2：移植 `cursor_x - line_x ≈ broken.width` 的 debug assertion，并保留其对 margin/column/list 外层平移后的页面坐标断言。
- Candidate 4：移植 `blank_em` 在 12 pt 与 16 pt 两组字号下的参数化测试，直接证明糖是在布局期读取最终字号。

## 5. Reverse-word autonomy defaults

没有候选实际采用任何 reverse word；Candidate 1/3 明确讨论后拒绝了相关反转，Candidate 2/4 声明采用 frame 默认。

补充：Candidate 2 允许 `Length<Finite>` 的负 baseline offset，这不属于 frame 列出的 reverse word，但作为底座时应保留 Candidate 3 的 `Length<NonNegative>`，以维持“线在基线下方”的较小能力面。
