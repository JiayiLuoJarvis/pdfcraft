# Knife 16 取舍说明

## 选择摘要

选择最窄的公共桥接：`Cell::spacer(Length<NonNegative>)` 存入新的私有 `CellContent::Spacer` 分支，`Table::lower` 将其转换为既有 `FlowNode::Spacer`。布局节点、cell allowlist、测量和分页逻辑全部复用。

该选择遵循 frame 的全部 autonomy defaults，没有偏离项，也没有启用 reverse word。

## 采纳理由

### 与提案一致

提案 §2.6 允许 spacer 作为单元格孩子，同时规定表格以整行作为分页单位；§2.13 又规定 spacer 是非负、可为零、不绘制可见内容的竖直空白。`Length<NonNegative>` 已精确表达长度约束，`FlowNode::Spacer` 已精确表达布局对象。

### 与现有调用面一致

公共 `Cell` 当前是一个 payload 加跨度，图片和水平线使用 `Cell::image`、`Cell::hline` 命名构造函数。`Cell::spacer(height)` 因而是最自然的相邻 API：一个高度参数，不引入 cell 专用 Flow 闭包。

### 与现有 seam 一致

`CellBody` 已持有 `Vec<FlowNode>`，`Container::TableCell` 已允许 `FlowNode::Spacer`，`SpacerNode` 已完成 split 和空 fragment 行为。缺口只在公共存储枚举和 `Table::lower` 的穷尽匹配。复用该 seam 能把生产改动限制在 `src/document.rs`。

### 对分页行为保持诚实

表格在分页前用 `natural_height` 测量 cell，而该函数拒绝 `Partial`。保留此行为可同时维持“物理行不横切”和既有表格测量模型。过高 cell spacer 返回 `UnsplittableTallerThanFrame`；本设计不声称它像正文 spacer 一样跨页。

## 考虑后拒绝的方案

### `Cell::space(height)`

拒绝。它触发 reverse word `space`，却没有测量或可读性收益；现有正文 API 已叫 `Flow::spacer`，相同领域对象应使用相同名词。

### 拒绝零高度

拒绝。它触发 `reject-zero`，并直接违背 §2.13“可为 0”。`NonNegative::ZERO` 已是公开且有用的合法值；零 spacer 应自然 lower 为高度 0 的空 fragment。

### 为 cell 建立多内容 closure 或 stack

拒绝。它触发 `stack-cell`，会把本刀从“补一个已有孩子类型”扩大成新的公共组合 DSL，还会提前牵涉段落、列表、blank、link 以及错误传播方式。现有 cell 单 payload 足以交付 spacer-only cell。

### 修改 `natural_height` 以拆分格内 spacer

拒绝。它触发 `split-in-cell`，并非局部 spacer 改动：表格必须保存 cell tail、重新计算跨页行高、边框、rowspan 和重复表头，且会与 §2.6 的整行分页单位发生冲突。当前明确报 `UnsplittableTallerThanFrame` 更可预测。

### 只用页数证明 spacer 生效

拒绝。它触发 `page-count-only`。页数容易被字体行高、页面尺寸或其他节点间接影响，不能证明实际位移量，也不能证明 spacer 无 paint。比较相邻文字 glyph 的 y 差并检查 canvas ops 更直接。

### 新建 `CellSpacer` / `CellSpacerNode`

拒绝。同一个领域对象出现第二种布局类型会复制非负约束、split 和 fragment 逻辑，并让正文与 cell 行为更容易漂移。差异来自 table 的 `natural_height` 测量上下文，不来自 spacer 本身。

### 修改 table-cell allowlist

拒绝。`FlowNode::Spacer` 已明确在无条件允许分支中。再次加规则没有行为变化，只会制造两处真相来源。增加一个 seam 单元测试即可锁住现状。

### 接受裸 `f32` 并在 `Cell::spacer` 返回 `Result`

拒绝。它绕过 §2.0 的类型化边界，重复 `NonNegative::pt/mm` 已有的有限值和非负校验，还会使其签名与 `Flow::spacer` 不一致。

### `From<Length<NonNegative>> for Cell`

拒绝。裸长度在 `t.row([...])` 中意图不如 `Cell::spacer(...)` 清楚，且现有 image/hline 先例使用命名构造函数。隐式转换也没有减少必要的品牌长度构造。

### 在 lower 时把 spacer 改成空字符串或透明线

拒绝。空字符串没有稳定的竖直自然高；透明线仍是绘制命令，并把不可见布局对象错误地耦合到 paint。既有 `SpacerNode` 已提供正确的纯几何语义。

### 仅做内部 layout 测试

拒绝。内部测试只能证明 allowlist 接受 spacer，不能证明公共 `Cell::spacer`、`CellContent` 分支和 `Table::lower` 接线完整。主证明必须经过 `document(...).pages()`；内部测试只作为 seam 回归。

## 风险与约束

- spacer 行与同行其他 cell 的最终高度取最大值；因此几何测试必须使用 spacer-only 行，避免文字行高遮蔽 spacer 高度。
- 未来公开 padding/border 后，过高阈值应继续以 `inner_full` 为准，而不是硬编码页面正文全高。
- 在重复表头中使用 spacer 会重复该空白高度，这是现有“重复全部表头行”的自然结果，不应添加例外。
- 公开 API 不能构造 cell 内 forbidden node；内部拒绝测试仍需保留，防止未来扩展绕过矩阵。

