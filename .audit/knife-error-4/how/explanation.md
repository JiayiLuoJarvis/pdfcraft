# quire 错误的记录与交出，以及刀 4 多错收集要改什么

### Overview

今天 quire 从头到尾只有**单错**：公共 `Result<T> = std::result::Result<T, Error>`（`src/error.rs` 590），仓库里没有 `Errors`、`Vec<Error>` 这类类型。错误分三个阶段产生，每个阶段都是「先到的赢，后面的丢」，只有一处例外：容器自己的槽会压过更早的子项错误。

1. **组装期**（`document()` 的 `build` 闭包里）。builder 方法大多返回 `()`，错误写进各节点的 `error: Option<Error>` 槽，或推进 `Vec<Result<_>>`。元素在 `add_to` 时就 `lower`，`Flow::push` 只留第一条 `Err`。
2. **测量 / 分页期**（`paint` → `paginate_sections`）。全程 `?` 硬停：第一个缺字、第一个缺 bold、第一个超宽就结束这一跑。
3. **后端期**（`to_pdf`）。`KrillaCanvas.failure` 是被毒化的单错槽。

`paint` 交出错误的顺序是：`Document.error` → 拒收流 `closed` → 各节 `flow.error`（加 `in_section`）→ `plan_pages` → `measure_bands`。只要有错，就不会产出 `Rendered`，`RecordingCanvas` 也是空的。

刀 4（`docs/error-diagnosability.md` 46–88，方向已确认、未实现）要做的事：新增非空 `Errors`，把公共 `Result` 改成 `Result<T, Errors>`；组装期所有 `note` / `push` / `collect` 都写进同一张错误表；测量期至少让 `MissingGlyph`、`BoldFaceRequired`、`UnknownFont` 记下后继续；最多收 64 条，并标明截断；顺序按遍历发现序，纠正今天「容器槽压过子项」的优先级扭曲；backend 仍然只报一条。

### Key Concepts

- **`Error`**（`src/error.rs` 12–105）：公开字段 `kind: ErrorKind`、`at: Option<Location>`，私有 `source: Arc<dyn StdError + Send + Sync>`。手写的 `PartialEq` 只比 `kind` 和 `at`。实现了 `Clone`，因为文档侧要克隆延迟下来的首错。
- **定位方法**（`pub(crate)`）：`in_section`、`on_page_index`、`in_region`、`in_cell`，都用 `get_or_insert`，规则是**内层先写、外层不覆盖**。`From<ErrorKind>` / `From<GeometryKind>` 构造出来的错误 `at` 为 `None`。
- **`note`**：共 7 处实现，语义完全一样：`if self.error.is_none() { self.error = Some(e) }`。所在位置：`paragraph.rs` 164、`table.rs` 167、`list.rs` 84、`image.rs` 78、`hline.rs` 90、`band.rs` 93、`layer.rs` 157。`TextProps::font_size`（`style_props.rs` 14–21）是手写的同一种逻辑。
- **`Vec<Result<_>>` + `collect::<Result<_>>()`**：`List.items`、`Columns.cols`、`Table` 的 `Track::into_width`。这些向量里**可以同时存着多条 `Err`**，但 `collect` 遇到第一条就停。这是今天唯一已经「收了一半」的地方。
- **`Flow`**（`document/flow.rs`）：有 `error` 字段，但没有 `note`，由 `push` 直接写。另有 `reject_pushes`，只在显式 `section` 之后的 `closed` 流上为真。
- **拒收流 / `fail_outside`**：节外内容一律记成 `ContentOutsideSection`，`at: None`。
- **整份失败**：只要有错就没有部分页，这条刀 4 保持不变（第 4 点）。

### How It Works

#### 1. 组装期：一条正文节点怎么走

1. 元素闭包里出错，就 `note` 到元素自己的槽（`Columns` 例外：它把 `Err` 推进 `cols`）。闭包本身照常跑完。
2. `IntoFlowItem::add_to`（`flow_writer.rs` 22–92）**先 `lower()`，再 `flow.push(...)`**。`lower` 发生在构建期，不在 `paint`。
3. `Flow::push`（`flow.rs` 41–52）：
   - 已有 `self.error`：直接 `return`，新来的 `Ok` 和 `Err` 一起丢掉；
   - 否则如果是 `reject_pushes`：写成 `ContentOutsideSection`，**不看**传进来的 `Result`，传入的 `Err` 被替换掉；
   - 否则 `Ok` 追加到 `nodes`，`Err` 记进 `error`。
4. `Flow::into_nodes`（61–65）：只要 `error` 是 `Some`，就把整份 `nodes` 丢掉。

「核对」：一份 explorer 说「`columns` 失败后的 `table` 根本不会 lower」，这不准确。`add_to` 无条件先 `lower`，所以 table 的 `lower` 会跑，内部也会收口成一个单错，只是随后在 `push` 时被丢弃。`tests/document.rs` 568–584 的 `first_builder_error_wins_and_later_calls_are_ignored` 锁住的正是这个行为：表的行列不一致确实被算出来了，但没有交出去。

#### 2. 容器 `lower` 内部的优先级（在构建期就选定一条）

- `List::lower`（`list.rs` 90–101）：先看 `self.error`（来自 `marker_width` / `nest_indent`），再 collect `items`，最后才是 `ListNode::new` 的 `ListIndent` / `MarkerOutOfRange`。`item()` 会无条件 push 进 `items`。
- `Columns::lower`（`columns.rs` 64–80）：没有槽，按调用顺序 collect `cols`；之后才做 `reject_auto_with_ratio` 和 `ColumnsNode::new` 的列数检查、`check_content`。
- `Table::lower`（`table.rs` 173–238）：`self.error` → track 列数 / `into_width` / `AutoWithRatio` → `header` 行 → `body` 行（从左到右，`?`）→ `TableNode::new` 的跨度检查，跨度错误带 `in_cell`。`Cell::spacer` 失败时会经 `Table::store` `note` 到表上（109–112）；格子里的列表 / 段落错误存在 `StoredContent` 里，**不会** `note`。
- `ImageBox::lower`：宽度错误优先于 `UnknownImage`（`image.rs` 84–91）。

结果是**后写入的容器槽会压过更早的子项错误**。比如先 `item` 失败、后 `marker_width` 失败，交出去的是 `marker_width` 的错；后一格的 `spacer` 错误会盖住前一格的段落错误。结构检查（列数、跨度、嵌套）排在所有子项之后，只要有子项错误，它们就跑不到。

#### 3. `Document` 层与 `paint`

- `Document::section`（`document_ty.rs` 118–152）：已有 `self.error` 时克隆一份返回 `Err`，**不跑**新闭包。如果隐式节已经 `written()`（流里有错误也算 written，见 `flow.rs` 33–35），就记成 `ContentOutsideSection`。闭包返回 `Err` 时先 `in_section(index)` 再写入，失败的节不入列；闭包返回 `Ok` 时**不检查** `section.flow.error`，留到 `paint` 再处理。
- 节后再调 `margins` / `header` / `footer` / `behind` / `front`，走 `fail_outside`（155–158）。
- `FlowWriter::stack` 如果样式失败，会把 `TextProps` 的错误**直接**写进目标流（包括 `closed`），绕过 `reject_pushes`，而且不跑 `body`（`flow_writer.rs` 216–229）。
- `Document::font` / `bold_font` / `load_image` 直接返回 `Result`，**不进**任何槽。
- `paint`（161–202）依次检查：`Document.error` → `closed.into_nodes()?`（不加节号）→ 每节 `section.lower` 加 `in_section(index)` → `paginate_sections`。因此 `Document.error` 会压过节流里更早的错误。

#### 4. 测量 / 分页期：全是 `?`

- 单段的检查顺序：`avail.validate()`（`LayoutFrame`）→ `layout_runs` 的 `LineWidth` / `FirstLineIndent` → `measure_inline`：先 `fonts.bold()?`（`BoldFaceRequired`，`paragraph.rs` 550），再按字体切片调 `measure_text`：`face`（`UnknownFont`）→ `parse_face`（`Font` + `source`）→ 逐个标量查 cmap，**第一个**缺字就 `return MissingGlyph`（`measure.rs` 61–66）。测量完成后才 `break_spans`：`RunAfterFillBlank` / `AdvanceWidth` / `UnbreakableWiderThanFrame`，然后是 `line_metrics`、`place_line`，最后 `ParagraphNode::split` 的超宽 / 超高检查。
- 节点之间：`split_stack` / `plan_pages` 用 `?`，第一个 `split` 错误就结束整跑。列从左往右；表按格子 `natural_height(...).map_err(in_cell)`；列表先 `body.split`，成功后才量标记。
- `paginate_sections`（`paginate.rs` 97–118）：所有节的正文 `plan_pages` 完成后才 `measure_bands`。装饰按页依次 header → footer → behind → front（`decorate.rs` 176–185），出错时补 `in_region` / `on_page_index` / `in_section`。`measure_band` 会把超宽 / 超高改写成 `Decoration*`，改写时用 `Error::from`，**丢掉**内层 `cell`。`Band` / `Layer` 的 `note` 要到这一步钩子执行时才会发生。
- 都通过后才 `paint_pages`。

#### 5. Backend

`to_pdf` 只接收已经成功的 `Rendered`。`KrillaCanvas::fail`（`krilla_canvas.rs` 187–198）只存第一条，之后每次调用都返回这一条的克隆；`replay` 用 `?`；`emit::render` 按页 `on_page_index`。

#### 6. 刀 4 要改什么（对照上面的机制）

| 刀 4 条目 | 今天 | 要改 |
| --- | --- | --- |
| 3. API | 公共 `Result<T, Error>` | `error.rs` 新增非空 `Errors`（`Clone`、逐条 `PartialEq` 忽略 `source`、可迭代、多行 `Display`）；`Result<T> = Result<T, Errors>`；`From<Error> for Errors`。`error` 模块依旧不依赖 document/layout |
| 2. 组装范围 | 7 处 `note` + `TextProps` + `Flow.error` + `Document.error` + `Cell.error` 都是 `Option<Error>` | 都改成追加到同一张表（或者上交到一个共享收集器）；`List` / `Columns` / `Table` 的 `collect` 改成收集全部 `Err` |
| 6. 继续 | `Flow::push` 有错就丢掉后来的 `Err` | 已有错时也要收下兄弟节点的 `Err`，`Ok` 节点不进成功树；`Document::section` 已有错时是否仍跑后续闭包，文档没写（见 Gotchas） |
| 5. 顺序 | 容器槽 / `Document.error` 在 `lower` / `paint` 开头就抢先返回 | 改成发现序：子项错误排在后写的容器错误前面；`paint` 不再让 `Document.error` 抢先，节流、`closed` 流也要并进来 |
| 5. 上限 | 无 | 64 条，满了就停，并标明截断 |
| 2. 测量范围 | `measure_text` 第一个缺字就 return；`bold()?`；`face()?` | 至少 `MissingGlyph` / `BoldFaceRequired` / `UnknownFont` 记下后继续（需要占位 advance 之类的策略让后续断行能跑），并且和组装错误进同一个桶 |
| 2/6. 硬停 | 全是 `?` | 几何 / 分页（`StalledOnFreshPage`、`LayoutFrame`、超宽超高）可以先记入表再硬停 |
| 2. backend | 单错 | 保持单错；类型随别名变成 `Errors`，但只有一条 |
| 6. 跳过闭包 | 空 link、非法 `col_fixed` / `col_ratio` / `gap`、`stack` 样式失败、`rotated` 非有限 | 可以保留 |

### Where Things Live

| 关注点 | 位置 |
| --- | --- |
| `Error` / `ErrorKind` / `Location` / `Result` | `src/error.rs`（`Result` 590；定位方法 29–59；`PartialEq` 83–87；`Display` 89–96）；`src/lib.rs` 10–13 再导出 |
| 公共入口 | `src/lib.rs` 45–70（`document`、`to_pdf`）；`src/document/document_ty.rs` `paint` 161–202、`render` 207–209 |
| 文档级槽 | `document_ty.rs` 11、`section` 118–152、`fail_outside` 155–158 |
| 流 | `src/document/flow.rs`（`push` 41–52、`written` 33–35、`into_nodes` 61–65、`lower_paragraph` 73–76） |
| 写入入口 | `src/document/flow_writer.rs`（`add_to` 22–92、`keep_together` 201–206、`stack` 216–229、目标流选择 245–251） |
| 元素槽 | `document/{paragraph,table,list,columns,image,hline,band,layer,style_props}.rs` |
| 节 lower | `src/document/section.rs` 88–114 |
| 结构校验 | `src/layout/{list,columns,table,nesting}.rs` |
| 测量 | `src/text/measure.rs` 41–102；`src/fonts.rs` 69–155；`src/layout/paragraph.rs` `measure_inline` 538–587、`layout_runs` 161–218 |
| 断行 | `src/text/line_break.rs` 157–182、286–307 |
| 分页 / 装饰 | `src/layout/paginate.rs` 97–118、157–161、195–201；`src/layout/decorate.rs` 154–185、223、294–309 |
| Backend | `src/backend/krilla_canvas.rs` 103、162–198；`emit.rs` 30–47；`convert.rs` 35–59 |
| 锁死单错的测试 | `tests/document.rs` 568–584；`src/document/tests.rs` 92–142、208–212（`assert_fails_with_empty_canvas`，约 13 个调用点）；`src/layout/decorate.rs` 817–837；以及大量 `.kind` / `unwrap_err()` 断言 |

### Gotchas

- **优先级扭曲没有测试覆盖。** 「容器槽压过子项」「`Document.error` 压过节流」「拒收流替换传入的 `Err`」「`stack` 在拒收流上直接写样式错误」这几条都只能从控制流推出来，没有断言。刀 4 改了顺序，现有测试也不会因此变红，需要另补对打测试。
- **会变红的测试。** `first_builder_error_wins_and_later_calls_are_ignored` 改成兄弟收集之后，会同时报出 `ColumnCount` 和表的行列不一致。`fill_blank_coalesces...` 里 `blank_to_end` 调了两次，逐次追加会变成两条相同 kind 的错误，要决定是否合并。凡是 `result.err() == Some(&Error)`、`unwrap_err().kind` 的断言，类型都要改。
- **改 `Result` 别名会波及全仓库。** `geom` 的 `try_*`、`fonts`、`canvas`、backend、`LinkTarget::uri` 都用 `crate::error::Result`。`return Err(Error::from(..))` 不会自动变成 `Errors`。可以考虑内部保留 `Result<T, Error>`，只在公共边界包一层。这一点文档没有定。
- **`lower` 在构建期发生。** 收集器必须在 `add_to` 时就拿到子树里的全部错误，`paint` 时已经没有子项可以重新拆。
- **`Document::section` 早退不是 `note`。** 已有错时它不跑新闭包。刀 4 第 2 点没有点名这条路径，要不要跑完后续闭包需要先定下来。
- **测量期要继续，就得有占位。** `measure_text` 现在是第一个缺字就 return；要继续，需要给缺失的标量一个占位 advance，否则后面的断行 / 分页没法跑。缺字的段落如果会挪到下一页，它的 `page` 记的是**当前**页（`TextNode::split` 先测量，再判断放不放得下）。
- **阶段之间互相挡住。** 组装失败时 `paginate_sections` 不跑，`Band` / `Layer` 钩子的 `note` 也不会发生。任何一节正文测量失败，所有装饰都不会被测量。刀 4 的「组装序，再测量序」意味着组装有错时是否还进测量，需要明确。按「几何硬停」的口径，可能只到组装为止。
- **定位缺口。** 单元格**内容**错误没有 `in_cell`，只有跨度错误有。`measure_band` 的改写会丢掉 `cell`。列表项和列没有下标。`ContentOutsideSection` 一直是 `at: None`。这些不在刀 4 的承诺内，但多错之后会更显眼。
- **同一调用里两个非法参数只记一条。** `Band::inset`、`Layer::line` / `Shapes::line` 用 `(Err(e), _) | (_, Err(e))` 取左边那个。刀 4 没说这里要不要拆成两条。
- **`measure_inline` 里按 glyph tag 做的那次 `collect`**（`paragraph.rs` 554–561）实际跑不到 `BoldFaceRequired`：bold tag 只来自非空 bold run，而这种 run 在 550 行的 `bold()?` 就已经失败了。改成多错时别把这一处当成第二个错误来源。
- **`charmap.map` 返回 `Some(0)`**（`.notdef`）时，今天当作普通字形处理，不报缺字。还没对照 skrifa 核实这种情况会不会出现。
