# Arena cross-judge verdict

## 评分

| 候选 | 1. Usage-first | 2. Table as data | 3. Pure break | 4. Minimal Error | 5. Testability | 6. Laziness | 合计 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 — BreakTape + seams + stamp | 3 | 5 | 4 | 5 | 4 | 2 | 23 |
| 2 — ClusteredText + width callback | **5** | **5** | **5** | **5** | **5** | **5** | **30** |
| 3 — sticky Pieces | 5 | 5 | 5 | 5 | 5 | 4 | 29 |
| 4 — immutable Cluster + backtrack | 2 | 5 | 4 | 5 | 4 | 4 | 24 |

评分要点：

- 四案都把 6×6 规则写成数据表，而不是把断行决定散落在 `match` 中。
- 候选 2 的 `ClusteredText` 先规范换行并生成无 `BK` 的 `glyph_text`；段落只测量这串文字，再由回调把 `advance_norm * font_size` 作为簇宽送入纯断行器。它同时解决了换行字符不应进 cmap、字形下标连续、pt 单位换算和测试替身四件事。
- 候选 3 的测试覆盖最完整，算法也基本等价；扣分在于调用方要维护与 `scalars()` 平行的 scalar 宽度切片，而 `Pieces` 提前融合不可断片段，隐藏了编号算法第 6 步要审计的原始相邻边界。
- 候选 1 的未测量 `BreakTape` 以 `0.0` 占位再原地 stamp，是明确的时间耦合和半有效状态。其 `break_measured` 还把 `advance_norm` 与 `avail_width_pt` 直接比较，缺少字号换算；若原文含换行，先测原文也可能在 cmap 阶段失败。
- 候选 4 的第一个 stub 示例本身不成立：`"好（世界"` 共 4 个 scalar，宽度 `0.04` 小于 `W=0.05`，不应断成两行。其 future call site 先测 `user_text`，随后才规范 CRLF；这会让 `BK` 进入 cmap，并使 measured byte ranges 与规范化文本错位。它也同样把归一化 advance 与 pt 宽度直接比较。

## 推荐 BASE：candidate 2

候选 2 最适合作为基线。它的调用顺序清楚，但没有暴露可变的“准备一半”对象：

1. `ClusteredText::new` 完成算法第 1 步、UAX #29 分簇、`BK` 剔除和私有游标建立。
2. `measure_text(..., clustered.glyph_text())` 只测真正会绘制的 scalar。
3. `break_lines` 把每个非 `BK` 簇投影成一次性的数值宽度，再按步骤 2–9 扫描、查表和回退。
4. `BrokenLine` 用不可伪造的游标给出连续 glyph/byte 范围。

这套形状没有 skrifa/krilla 泄漏进 `line_break`，不要求改写 knife 4，也没有 stamp、版本号或同步维护的 seams。私有 `ClusterRecord`、`ClusterCursor` 和 `LineRecord` 比候选 4 可由 crate 内任意代码拼出的 `Cluster { class, width, ranges }` 更能守住边界一致性。它也最容易逐项对应设计稿的编号算法，而不需要先证明“融合片段”与回退等价。

## 值得移植的部分

- 从候选 3 移植完整的验收表：ASCII 空格、超宽 `AL`、`ID|ID`、`好（世`、`（世界`、`字。下`、CRLF、两类 U+0020、U+3000、组合簇、`Other|Other`、`ID|Other`、空段落；再保留对错误 `width` 的精确断言。
- 从候选 3 移植 `PairClass`：让 6×6 lookup 的参数类型根本不能表示 `BK`，比依赖“调用者不要传 BK”的注释更强。
- 从候选 3 移植分类边界测试，尤其是 U+00D7、U+00F7、U+F900、U+FF21、扩展 B 和 OP/CL 清单。
- 从候选 1 移植“`BK` 是控制原子、没有绘制宽度”的显式建模思想，但不要移植可变 tape、stamp 或 seams 生命周期。
- 从候选 4 移植 `LineSpan` 式紧凑结果命名和 `W` 入口校验；具体范围仍应由候选 2 的私有游标产生。

## Rejects

### Candidate 1 作为基线

拒绝。主要问题不是 seams 的空间成本，而是 stamp 造成的时序协议：类型允许未测量 tape 进入断行器。`line_break::break_measured` 又跨进 `measure` 层，却没有解决 BK-free 测量和字号单位。设计中的 `text/mod.rs` 还同时重导出两个同名 `break_tape`，表面会冲突。`head_class`、`ink_cell` 等入口也把内部 tape 形状泄漏给调用方。

### Candidate 3 作为基线

拒绝作为 BASE，但它是最有价值的备选。融合 `Piece` 在当前固定布尔表下可与最近断点回退等价，然而它丢掉了 packer 中的原始边界和类别。未来若断点从 bool 扩成优先级、惩罚或上下文规则，维护者必须先拆掉这层编译表示。调用方自行构造平行 scalar 宽度数组也比候选 2 的簇回调更容易发生坐标系错配。`Lines::len` / `lines` 等方法还有少量浅包装。

### Candidate 4 作为基线

拒绝。它最像编号伪代码，但调用示例和 measured 桥接存在实质错误，不能用“以后补 BK-free scalars”掩盖。`break_stub` 丢弃规范化后的字符串，而 `LineSpan::byte_range` 又以该字符串为坐标；CRLF 输入时调用方没有正确的源可切。stub cluster 的 glyph ranges 全是 `0..0`，`LineSpan::text` 的 `_clusters` 参数是无作用的透传。公开 crate 内字段和 `break_lines(&[Cluster])` 也允许构造 class、width、byte range、glyph range 彼此矛盾的 atom，削弱了其宣称的不可变完整性。

## 结论

以 candidate 2 为 BASE；吸收 candidate 3 的类型约束和测试清单。不要采用 candidate 1 的 stamp，也不要以 candidate 4 当前的 measured/stub convenience API 为实现起点。
