# Knife 6 cross-judge

## 逐项评分

| 标准 | candidate-1 | candidate-2 | 判断 |
| --- | ---: | ---: | --- |
| 1. Domain fit | 4/5 | 2/5 | candidate-1 的 consume-self `Split` 明确区分已画 head、未画 tail 和完全未画节点，且递归 `FlowNode` 与既定协议一致；但 `PageCursor` 外另放 `page_open`/`y`，以及 `Split` 与 `Fragment` 重复保存高度，仍能组成不一致状态。candidate-2 有三态游标，却用借用的 `BlockBox` 输入生成拥有 `Vec<LineBox>` 的输出，未交代移动/克隆，且没有 consuming split 等价物来约束“只画一次”。 |
| 2. Boundary | 4/5 | 4/5 | 两者都不让 layout 接触 krilla，也都复用 `ClusteredText → measure_text → break_lines → line_metrics`。candidate-1 对样式边界写得更完整，但 `BrokenLines<'static>` 只是不可直接落地的占位；candidate-2 的文字管线较干净，但宽度仍是裸 `f32`，并未完整说明嵌套 `KeepTogether` 的非法内容校验边界。 |
| 3. Knife scope | 3/5 | 5/5 | candidate-1 提前放入 `Fill`/`Stroke`/`Image`/`Link`、`ImageTable`，还设计了无实际用途的 `lower_test_blocks`，超过本刀。candidate-2 严格停在 paragraph、stack、spacer、pageBreak、keepTogether 与 paginate。 |
| 4. Testability | 5/5 | 4/5 | candidate-1 明确列出 RecordingCanvas 的 x、baseline、页数及四条命名 PageBreak 轨迹。candidate-2 也覆盖这些点，且阶段可单测；但 usage 中 50pt 页高、100pt spacer 后再接段落却断言总计三页，按其自身规则至少还会产生额外页，示例预期不可信。 |
| 5. Laziness | 2/5 | 4/5 | candidate-1 的未来命令空壳、图片上下文和 lower 包装不是本刀所需。candidate-2 虽有 `FlowItem → BlockBox → PlacedBlock` 三层，但都服务于其两阶段实现，且没有公共 builder。 |
| **合计** | **18/25** | **19/25** | candidate-2 略胜本刀局部最小化；下刀扩展成本改变最终取舍。 |

## 推荐基底

选择 **candidate-1**。它的递归 `FlowNode`、consume-self `Split`、当前 `Frame.width` 和 fragment-local 坐标可以直接承接 knife 7 的表格整行切分、重复表头和 columns 各列 tail。candidate-2 已自行承认表格会迫使它重新引入 split/fragment；此外，一次按单一宽度全量展平 `BlockBox` 无法自然表达 columns 的不同子框宽度与并行 tail，届时不是增量扩展，而是替换分页核心。

采用 candidate-1 前应收紧三处：让 `PageCursor` 连同当前页构建状态一起封装，去掉可矛盾的 `page_open` 组合；用私有构造器保证 `Split` 高度与 `Fragment.height` 一致；本刀只保留 `Glyphs`，删除 `ImageTable`、未来 command 变体和无调用价值的 lower 占位。

## 从 candidate-2 移植

1. 移植 owned `LineBox`，但只作为 `ParagraphNode` 的已布局缓存/续排状态；tail 持有剩余行，避免 candidate-1 的 `BrokenLines<'static>` 自引用问题和每页从全文重测。
2. 移植“先分页、后绘制”：paginate 产出 `Vec<PageBody>` / placed fragments，再统一画到 `Canvas`，不要在状态机推进时直接产生 Canvas 副作用。
3. 移植预计算原子组的思路解决 `KeepTogether` 所有权缺口：完整框布局一次后持有 owned fragment、总高和 trailing spacing；当前页放不下时移动这个 prepared group，而不是 clone 已被 `natural_height` 消耗的 children。
4. 不移植全局 `FlowItem → BlockBox` 展平层，也不采用借用 boxes 再复制成 owned pages 的接口。

## 结论

candidate-2 是更整洁的 knife 6 局部解，按五项 rubric 以 19:18 小胜；但它把复杂度推迟到确定紧随其后的 table/columns，并会改写核心数据流。以已确认的递归 flow 协议和 knife 7 成本为决胜项，应以 candidate-1 为基底，削掉超范围空壳，再吸收 candidate-2 的 owned 行盒、分页与绘制分离、以及 prepared keep-together，这样既保持本刀边界，也避免下一刀推倒重来。
