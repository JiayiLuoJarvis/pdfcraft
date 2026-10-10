# Canvas slice 2 — cross-judge verdict

## 结论

推荐 **Candidate 1** 作为 BASE。

它最贴近已确认的落地路径：对象安全的 `Canvas`、逐操作方法、平坦且有序的记录日志、品牌化几何，以及不进入 trait 的文档终结。相较其余方案，它对后续 `layout` 和私有 krilla 适配器施加的额外架构假设最少，改成可实现版本所需的修正也最局部。

Candidate 1 不能原样照抄：示例中的 glyph 命令索引错误（该序列中 glyph 在索引 1，不是 2），`Length` 导入不完整，`set_document_info` 的注释与实际 phase 检查不一致；两个公开 page-phase 错误也应重新审视，避免超出本 slice 的最小错误面。

## 评分

每项 1–5，5 为最好。

| Candidate | Boundary purity | Domain model | Geom reuse | Minimal surface | Testability | Laziness | Total |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | 5 | 4 | 5 | 4 | 4 | 5 | **27** |
| 2 | 5 | 3 | 5 | 5 | 5 | 3 | **26** |
| 3 | 5 | 5 | 5 | 2 | 5 | 2 | **24** |
| 4 | 5 | 5 | 5 | 2 | 4 | 1 | **22** |

### Candidate 1

- 边界纯净：无 krilla，Canvas 不依赖 layout；`DocumentInfo` 只是数据，不是 DSL builder。
- 生命周期集中在 `RecordingPhase`，日志也显式记录 start/finish；虽不是静态 typestate，但没有散落 bool。
- 完整复用 `Size`、`Rect`、`Path`、`Paint`、`Stroke` 和正长度字号。
- 单一 `canvas.rs` 很克制；主要赘余是两个公开 phase 错误。
- 平坦日志适合精确顺序断言，但给出的 glyph 测试索引本身有误，需在实现前修正。

### Candidate 2

- 几何、资源封装和测试日志都很干净，错误面也最小。
- 但 `execute(CanvasCommand)` 只把完整协议从 trait 方法搬到调用方。非法生命周期序列仍可构造，空 URI 仍要求每个实现重复验证。
- 后端实现成为一层 exhaustive dispatch；模块没有隐藏多少复杂度。这是典型的浅模块与协议信息泄漏，故不作为 BASE。

### Candidate 3

- page guard 对生命周期的类型编码最强；单一嵌套 transcript 也没有双写。
- 代价是明显超出 slice：非对象安全 GAT trait、两个 writer trait、大量公共构造器/错误/oracle 类型，以及 metadata 清洗策略。
- 最大问题是 paint 方法不可失败，并建议把实际绘制失败 latch 到 adapter-only 的文档 `finish`。这把错误从发生点推迟到文档结束，属于危险的 temporal split，也偏离技术参考中逐调用返回 `Result` 的边界。

### Candidate 4

- closure page scope 与 leaf-only `PageCanvas` 的隔离思路很好，生命周期也静态受控。
- 但同时保存 `info`、`pages`、`commands`，再用 `PaintOp -> CanvasCommand` 转换，形成多个事实源和明显 pass-through。
- `with_page` 先写 `StartPage`；closure 失败时却丢掉 page/ops 且不写 `FinishPage`，留下畸形的“完整 chronological log”。
- 把 whitespace-only URI 当空 URI 拒绝，超出了已确认的“空字符串拒绝”规则。

## 建议吸收的 grafts

1. **Candidate 2 的 payload 封装**：保留 crate-private 构造与只读 accessor；图片边长用 `NonZeroU32`，避免调用方伪造零尺寸资源。glyph 的有限 advance/合法 byte range 可在后续测量边界验证，但不要搬入 Candidate 3 那整套公共 builder/error 面。
2. **Candidate 3 的依赖方向处理**：让 `DocumentInfo` 由 canvas 边界或更低层的中立 metadata 模块拥有，避免长期形成 `canvas -> document`，再由 `document -> layout -> canvas` 回指的概念环。
3. **Candidate 4 的按页测试视图**：多页测试确实需要 page-oriented 访问；应从 Candidate 1 的 canonical flat log 派生 iterator/view，而不是同时持久化 `pages` 与 `commands` 两份日志。

## Red flags

| Candidate | Shallow module | Info leak | Temporal split | Pass-through |
|---|---|---|---|---|
| 1 | 无实质问题 | 低；公开命令仅作 test oracle | 无 | recorder 的 clone-and-push 是必要 test double，不算额外层 |
| 2 | **是**：单方法 command sink | **是**：layout 构造完整 wire/lifecycle 协议 | 无 | **是**：backend 主要是 command exhaustive dispatch |
| 3 | 否，page guard 较深 | 中：公共 oracle/builder 面过大 | **是**：绘制错误延迟到 document finish | 无 |
| 4 | 中：两套协议没有隐藏对应复杂度 | 中：同时公开 `PaintOp` 与 `CanvasCommand` | **是**：失败页只留下 `StartPage`；记录与提交时点分裂 | **是**：`PaintOp -> CanvasCommand` 双写转换 |

## Hard rejects

- **Candidate 3 as-is**：不可失败的 paint API 加文档结束时延迟报错，不适合作为后端边界。
- **Candidate 4 as-is**：双日志事实源、失败 closure 产生不平衡 command stream，且 URI 规则与已确认语义不一致。

Candidate 2 不是 hard reject；其 opaque payload 设计值得采用，但 command-bus 形态不应成为公共 Canvas 主接口。
