# Knife 8 arena cross-judge — document DSL

输入：`frame.md`、`candidate-1/design.md`、`candidate-2/design.md`、`docs/提案书.md` §1 示例、§2.10–§2.12、确认总表。
刀 7 事实已在 `6dbf40c` 的源码上核对过：`decorate.rs:41` 的 `BandContent` 隐式 `'static`；`lower.rs:44` 的 `Columns | Table` 只允许出现在 `Container::Column` 里；`paginate.rs:220` 的绘制顺序是 header → body → footer；`canvas.rs` 里没有 replay 函数；`KrillaCanvas::new_with_fonts` 是 `pub(crate)`。

## 评分

| 维度 | C1（builder 直接产出 `FlowNode`） | C2（公开 `Block` AST + `lower`） |
| --- | --- | --- |
| 1. §1 闭包两时刻 | **5** | 4 |
| 2. 不发明未确认 builder | **4** | 2 |
| 3. 降到刀 7、不重写 split | 4 | **5** |
| 4. 可测性 | 4 | 4 |
| 5. 偷懒 / 接口深度 | **5** | 2 |
| 6. 刀 7 改动风险（越高越安全） | 2 | **5** |
| 合计 | **24** | 22 |

### 1. §1 闭包两时刻

- **C1 = 5。** 组装闭包跑一次，关闭时直接压成 `FlowNode`。header、footer、behind、front 都包成 `Decorations` 里的每页闭包。`is_last` 只在 `Layer` / `Page` 上存在。header 用 `'a` 生命周期借 `&company.name`，与 §1 示例原样一致。§1 完整模板里除 `bold`、`image_at` 外都能跑通，包括 behind/front。
- **C2 = 4。** 两时刻也分对了：`blocks()` 里不会出现页码，footer 的 `is_last` 在绘制时求值。扣分有三处：
  1. header 闭包要求 `'static`，调用方必须先 `clone` 再 `move`，偏离 §1 的借用写法。
  2. 签名变成 `|page, band|`，§1 写的是 `h.left` / `h.right`；C2 实际渲染成两段全宽文字，各占一行。
  3. 没有 behind/front。§2.12 已定，§1 完整模板里也有；frame 把它列为「可选」，所以只扣一分。

### 2. 不发明未确认 builder

- **C1 = 4。** 基本落在 frame 的 Defaults 范围内。新增的名字有 `h.page()`、`Rendered`、`item_marked`、`Numbering`、`Document: Deref<Target = Flow>`，都是把已确认语义接上去所需的最少东西。表格只有 `header` / `row` / `repeat_header` / `col_span` / `row_span`，严格守住 frame 的限制。扣一分：`col_span(u16)` 传 0 要等到运行时才报错，与 §2.0「约束优先类型化」相悖（C1 自己也承认了）。
- **C2 = 2。** 发明的 builder 很多，直接违反 frame「Do not invent: exact table builder method names beyond …」：
  - 表格：`t.widths` / `t.padding` / `t.border`。
  - 段落：`p.font` / `size` / `line_height` / `color` / `align` / `spacing_after`。
  - 栏：`row.gap` / `col_fixed` / `col_ratio` / `col_auto`。
  - 列表：`list.marker_width` / `nest_indent` / `start`。
  - 字体与入口：`register_face`、`Doc::new`。
  - 纸张：`Paper::{A3,A4,A5,Letter}`。A 系列清单在提案书里明确是「实现前可再钉」。
  - 整棵 `Block` / `Paragraph` / `Table` / `Cell` / `List` / `Track` 用 **pub 字段** 公开。等于把一棵内部树冻成了公共 API，以后改字段就是 breaking change。

### 3. 降到刀 7、不重写 split

- **C2 = 5。** `layout/` 一行不改，`lower_blocks` 只调用已有构造器，嵌套规则继续由 `check_content` 一处判定。
- **C1 = 4。** split 也没碰，但改了三处刀 7 代码：
  1. **放开 `Container::Band` 里的 `Columns`。** 按设计稿的写法（`matches!(container, Column | Band)`），`ColumnsNode::new` 会用 `Container::Column` 检查每一列，于是页眉里的 columns 可以嵌 table。这是规则矩阵的一个漏口。刀 8 的 DSL 只会产出 Text，碰不到，但不变量变松了。见下文嫁接 G1。
  2. `decorate.rs` 新增 layer 测量（`single_line_run` 加摆放），属于新代码，但不是 split。
  3. 改 `paint_pages` 的顺序。

### 4. 可测性

- **C1 = 4。** `tests/document.rs` 只用公开 API，断言跑在 `Rendered::pages()` 返回的 `RecordingCanvas` 记录上，`%PDF` 冒烟测试放在 feature 后面，符合 AGENTS「排版测试断言 RecordingCanvas」。扣分点：
  - 部分断言偏松，比如 `r[3].2 >= 36.0`、`y > 400.0`。
  - `to_pdf` 要把 `RecordingCanvas` 回放进 krilla，但现在没有 replay 函数，这条新路径只有 `%PDF` 覆盖。
- **C2 = 4。** 测试最细：`to_bits()` 精确坐标、`blocks()` 直接断言组装结果、`lower` 输出的是 lazy `FlowNode::Text`、失败时「命令日志为空」。扣分点：
  - 测试都放在 crate 内（`src/document/tests.rs`），公开 API 本身没有集成测试。
  - `to_bits` 全等在浮点路径上偏脆。
  - `blocks()` 可测，是拿暴露 AST 换来的（与第 5 项冲突）。

### 5. 偷懒 / 接口深度

- **C1 = 5。** 只有一条 lower 路径：builder 关闭时直接产出 `FlowNode`。没有镜像树。13 个公开名字，`FlowNode`、`Style`、`TableSpec`、`Decorations` 全部藏住。
- **C2 = 2。** 多了一棵与 `FlowNode` 一一对应的 `Block` 镜像树，外加 `lower`，正好是 C1 在 Alternatives 里点名否决的形状。公开名字约 25 个，字段全公开。`render` 和 `to_pdf` 各排一次版。字体解析两遍。

### 6. 刀 7 改动风险

- **C2 = 5。** 零改动。
- **C1 = 2。** 改动点有五个：
  1. `Decorations<'a>`、`BandContent<'a>`，`paginate_root` 签名跟着改。
  2. 新增 `behind` / `front` 字段和 `PageBands` 的两层，现有测试里的 struct 字面量要补 `..Decorations::none()`。
  3. 改 `paint_pages` 顺序。
  4. 放开 Band 里的 Columns。
  5. 新增 RecordingCanvas → KrillaCanvas 的回放。

  其中第 1 到第 3 项是 §1 借用写法和 §2.12 叠层顺序的必然代价，不做就丢已定条目。第 4、5 项可以收窄，见嫁接。另外还有一个 C1 自己没验证的风险：`single_line_run(.., f32::INFINITY)`。`glyph_run_for_line` 会把 `max_width` 传给对齐计算，Start 对齐下大概率是 0，但必须验证。

## 推荐

**以 C1 为底。** 它更薄，与 §1 和 §2.12 的已定条目对得最齐，也没有越过 frame 的 builder 限制。C2 的优势集中在「不碰刀 7」和测试精度上，可以按条嫁接；它的公开 AST 和成批发明的 builder 不进本刀。

### 嫁接（按优先级）

- **G1. 收窄 Band 里的 Columns（降 C1 风险）。** 不要直接把 `Container::Band` 加进允许列表。改成：`FlowNode::Columns(c) if container == Band` 时，用 `Container::Band` 递归检查每一列的子节点。这样页眉里仍然不能有 table、list、keep_together。保留现有的 `band_rejects_tables`，再补一条「band 内 columns 嵌 table 被拒」。如果评审不同意改规则，就退回 C2 的方案：两段全宽、`End` 对齐（plan B），并把与 §1「左右同行」的差异写进 open questions。
- **G2. 回放路径可测（降 C1 风险）。** 写 `pub(crate) fn replay(&RecordingCanvas, &mut dyn Canvas)`，加一条 round-trip 单测：回放进第二个 `RecordingCanvas`，`commands()` 必须完全相同。这条不依赖 krilla，`%PDF` 冒烟测试保持不变。
- **G3. 失败时画布为空（来自 C2）。** 缺字、`DecorationTallerThanMargin`、`UnsplittableTallerThanFrame` 这几类测试，除了断言错误种类，还要断言没有任何 `StartPage`。C1 的 `document()` 失败时拿不到 `Rendered`，所以这条放 crate 内单测，直接对 `paginate_root` 断言。
- **G4. `Page` 带 section 页码（来自 C2，回应 C1 的 open question 6）。** §2.11 已定双页码。加 `section_number` / `section_count`，单节时等于全文页码。成本几乎为零，`#[non_exhaustive]` 也保留。
- **G5. span 改用 `NonZeroU16`（来自 C2）。** 对齐 §2.0。这样 C1 的 `first_builder_error_wins` 测试要换一个错误源，例如 columns 里只有一列 → `ColumnWidths`。如果坚持 `u16` 的易用性，就把这一点列成待用户拍板的项，不要默认放过。
- **G6. lazy 断言（来自 C2）。** crate 内加一条单测：根节点的第一个子节点是 `FlowNode::Text`，不是已经断过行的 `Paragraph`。
- **G7. 文档约定（来自 C2）。** 重复调用 `header` / `footer` / `behind` / `front` 时，后一次替换前一次。写进 rustdoc，并配一条测试。
- **G8. 收紧 C1 的松断言。** `>= 36.0`、`> 400.0` 改成 `pt_near(.., margin + baseline)`，baseline 用 `line_metrics` 算，但保留 `pt_near` 的容差，不用 `to_bits`。
- **G9. 先验证 `single_line_run(INFINITY)`。** 不成立就改传 `page_w - at.x`。

### 不嫁接

- 公开 `Block` AST、`blocks()`。C1 不这样做也能证明「`if` 分支不进流」，因为 `texts()` 断言已经覆盖。
- C2 在第 2 项列出的全部样式和宽度 builder、`Paper` 枚举、`register_face`、`Doc::new`。
- 「`render` 与 `to_pdf` 各排一次」。
- 「header 闭包必须 `'static`」。

### 仍待用户拍板（两份候选都没解决）

1. **页眉水平缩进。** §2.10 说调用方「用 margin 自行缩进」，但两个候选的页眉 API 都给不出缩进手段。这是 §2.10 与 DSL 之间的真实缺口，需要决定是让 slot 默认内缩（相当于修订 §2.10），还是给 Band 加一个缩进原语。
2. `p.bold` 和 rich run 是否属于刀 8。两边都省略了，§1 示例需要相应修订或延期。
3. 样式 builder（字号、对齐等）何时钉名。C2 已经先写了一套，正好可以当作讨论稿。

## 下一步（以 C1 为底）

按顺序推进，每一步单独提交：

1. 在 `layout/` 做三处改动：`Decorations<'a>`；layers 与 §2.12 叠层顺序；G1 的 Band 内 columns 递归检查。每一步都有 paginate / decorate 单测，现有测试全部保持绿。
2. 实现 `canvas` 的 replay 和 G2 的 round-trip 测试。
3. 写 `tests/document.rs`，以 C1 的测试为底，叠加 G4、G5、G7、G8。然后填 `document.rs` 直到测试通过；G3 和 G6 放 crate 内单测。
