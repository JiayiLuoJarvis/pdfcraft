# How explorer: layout flow seams for image

Source agent: `94392bf1-39d3-4305-ba81-06188c7dc137` (2026-10-09).

### Components Found

`FlowNode`（`src/layout/flow.rs` 127–137）是内部消费协议。变体只有：`Paragraph`、`Text`、`Spacer`、`PageBreak`、`KeepTogether`、`Stack`、`Columns`、`Table`、`List`。`src/layout/` 里没有任何 image 类型。

`FragmentCommand`（76–83）只有 `Glyphs`、`Stroke`。`with_offset` / `draw_fragment` 不调用 `draw_image`。画布侧已有 `Canvas::draw_image`；`KrillaCanvas` 仍 `UnsupportedCanvasOperation`。

`KeepTogether` 是现成原子块模式：只 `Fit` 或 `MoveToNextPage(Prepared)`，高于 `full_height` → `UnsplittableTallerThanFrame`，从不 `Partial`。页顶 `MoveToNextPage` 会被分页器当成超高（`paginate.rs` 137–140）。

`check_content`（`lower.rs`）矩阵：TableCell/ListItem 允许 Text/Paragraph/Spacer/Stack/List；拒绝 Columns/Table/PageBreak/KeepTogether。Band 允许 Columns（再查子栈），拒绝 Table/List/PageBreak/KeepTogether。

`Decorations`：band 是 `Vec<FlowNode>`；layer 只有 `LayerText`（文字）。behind/front 不是流。

公共 DSL 单元格只降成单段 Text；无 image builder。

### Flow (hooks for image)

1. 新 `FlowNode` 变体 + `split` 一臂。
2. 新 `FragmentCommand` 必须改 `with_offset`、`draw_fragment`、`list::first_baseline`。
3. 原子语义对齐 KeepTogether；单元格经 `natural_height` 必须一次 Fit。
4. Layer 图像要另加载荷，不走 FlowNode。
5. Band 整页 frame；原子图若 MoveToNextPage 会被改写成 DecorationTallerThanMargin。

### Boundaries / Gotchas

- 坐标两段：`with_offset` 再加 `PlacedFragment.origin`；漏一边栏/表错位。
- KeepTogether 不能用来在单元格里包原子图（矩阵拒绝）。
- 列表 Partial 会去标记；图像不能 Partial。
- RecordingCanvas 可测；to_pdf 在 backend 未实现时失败。

### Open Questions

尺寸来源、容器允许表、Band vs layer、过宽错误变体、spacing_after、backend 实现顺序。
