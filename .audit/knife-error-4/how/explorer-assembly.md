组装期只留一个 `Error`。容器自己的 `error` 和 `Document.error` 在收口时会压过更早的子项错误；公开 `Result` 在 `document()` / `paint` 里交出这一个。

### Components Found

公开失败类型仍是单错。`src/error.rs` 第 590 行：`pub type Result<T> = std::result::Result<T, Error>`。`Error` 是 `kind` + `at: Option<Location>`（`src/error.rs` 第 18–25 行）。`in_section` 用 `get_or_insert`，已有节号不再改写（第 34–37 行）。没有 `Errors` 列表。

组装期的记录槽都是 `error: Option<Error>`，语义一律是「空才写入」：

| 类型 | 写入 | 收口 |
| --- | --- | --- |
| `Paragraph` | `note`（`src/document/paragraph.rs` 第 164–168 行） | `lower_paragraph` 先返回 `paragraph.error`（`src/document/flow.rs` 第 73–76 行） |
| `HLine` | `note`（`src/document/hline.rs` 第 90–94 行） | `lower` 第 96–98 行 |
| `ImageBox` | `note`（`src/document/image.rs` 第 78–82 行） | `lower` 第 84–87 行；其后才查 `UnknownImage`（第 88–91 行） |
| `TextProps` | `font_size` 里手写 `is_none`（`src/document/style_props.rs` 第 14–21 行） | `apply` 第 30–32 行 |
| `List` | `note`（`src/document/list.rs` 第 84–88 行），另有 `items: Vec<Result<ListItem<FlowNode>>>`（第 22 行） | `lower` 第 90–101 行 |
| `Table` | `note`（`src/document/table.rs` 第 167–170 行） | `lower` 第 173–176 行 |
| `Cell` | 只有 `spacer` 失败时把 `error: Some(...)` 放在格子上（第 245–250、349–356 行） | `Table::store` 立刻 `self.note(cell.error)`（第 109–112 行） |
| `Columns` | **没有** `error` 字段。`gap` / `col_fixed` / `col_ratio` 的 `Err` 推进 `cols: Vec<Result<...>>`（`src/document/columns.rs` 第 7–8、23–27、39–51 行） | `lower` 第 64–68 行 |
| `Flow` | **没有** `note`。`push` 和 `stack` 直接写 `error`（`src/document/flow.rs` 第 11、41–52 行；`src/document/flow_writer.rs` 第 221–224 行） | `into_nodes` 第 61–65 行 |
| `Document` | `section` / `fail_outside`（`src/document/document_ty.rs` 第 10–11、118–140、155–158 行） | `paint` 第 170–172 行最先返回它 |
| `Band` / `Layer` | 同样的 `note`（`src/document/band.rs` 第 93–96 行；`src/document/layer.rs` 第 157–160、170–173 行） | 不在构建闭包里收口，见 Boundaries |

`note` 的实现各处相同：`if self.error.is_none() { self.error = Some(error); }`。后到的错误丢掉，字段上的成功写入不回滚。例如 `Table::cell_padding` 失败之后，下一次成功的 `cell_padding` 仍会改 `self.padding`（`src/document/table.rs` 第 152–156 行），但 `lower` 先返回旧错误，改过的字段不会进节点。

`Flow::push`（`src/document/flow.rs` 第 41–52 行）顺序是：

1. `self.error` 已有则直接 `return`，新的 `Ok` 和 `Err` 都丢。
2. 否则若 `reject_pushes`，写成 `ErrorKind::ContentOutsideSection` 并 `return`。传入的 `Result`（包括另一种 `Err`）不看。
3. 否则 `Ok` 追加到 `nodes`，`Err` 写入 `error`。

`reject_pushes` 只在显式 `section` 之后的 `closed` 流上为真（`Document::section` 第 145–148 行；`Flow::rejecting` 第 26–30 行）。`Flow::child` 用 `Flow::new`，子流 `reject_pushes` 为 false（第 37–39 行）。

`List` / `Columns` 的 `Vec<Result>` 在 `lower` 里 `.collect::<Result<_>>()`（`list.rs` 第 99 行，`columns.rs` 第 65 行）。这是 `Result` 的 `FromIterator`：遇到第一个 `Err` 就停，后面的项不再看。

### Flow

**1. 构建闭包期间：错误记在槽里，多数 API 返回 `()`**

`document`（`src/lib.rs` 第 53–63 行）先 `build(&mut doc)?`，再 `doc.render()`。`render` 只调 `paint`（`src/document/document_ty.rs` 第 207–209 行）。

`FlowWriter` 的 `paragraph` / `columns` / `table` / `ul` / `ol` / `image` / `hline` / `spacer` / `page_break` / `add` 都返回 `()`（`src/document/flow_writer.rs` 第 112–195 行）。组装错不会从这些调用返回。文档注释写明 builder 失误推迟到 `document()` 返回（`src/lib.rs` 第 47–48 行）。

立刻返回 `Err`、且不写入 `Document.error` 的只有 `Document::font`、`bold_font`、`load_image`（`document_ty.rs` 第 31–36、40–46、52–57 行）。调用方若 `?`，`build` 失败，`render` 不跑。调用方若丢掉这个 `Err`，这次失败就没了，后面的延迟错误仍可成为公开结果。

一条正文节点的路径：

1. 元素闭包里 `note`（或 `Columns` 把 `Err` 推进 `cols`）。
2. `IntoFlowItem::add_to` 先 `lower()`，再 `flow.push(...)`（`flow_writer.rs` 第 24–26、32、39、46、72、91 行）。`lower` 发生在 **构建期**，不是 `paint`。
3. 父 `Flow.error` 还空时，`push` 留下这一个 `Err`。已有错误时，`lower` 仍然会跑完，但 `push` 把结果丢掉。
4. `paint` 时 `Section::lower` 只做 `flow.into_nodes()?`（`src/document/section.rs` 第 107 行），不再拆子节点。成功节点若排在错误之前，会留在 `nodes` 里，但 `into_nodes` 在 `error` 为 `Some` 时整份 `nodes` 丢弃（`flow.rs` 第 61–64 行）。

`FlowWriter` 在 `Document` 上的目标流（`flow_writer.rs` 第 245–251 行）：还没有显式 `section` 时是隐式节的 `flow`；已经有显式节时是 `closed`（拒收流）。

`stack`（`flow_writer.rs` 第 216–229 行）：`TextProps::apply` 失败时，仅当 `flow.error` 为空才写入，然后恢复样式并 **不跑** `body`。样式成功则 `body` 跑在同一条流上；即使这条流已经有错误，`body` 仍会执行，其 `push` 变成空操作。`Layer::stack` 同样，失败走 `note` 并跳过 `body`（`layer.rs` 第 30–38 行）。

`keep_together`（`flow_writer.rs` 第 201–206 行）：`nested` 建一条新子流（不继承父错误，也不继承 `reject_pushes`），跑完再 `push`。父流已有错误时，子流的 `Err` 被丢掉。

**2. 容器 `lower` 的内部优先级（构建期就定死一个 `Err`）**

`List::lower`（`list.rs` 第 90–101 行）：

1. `self.error`（`marker_width` / `nest_indent` 的 `note`）有则立刻返回，`items` 不看。
2. 否则 `items` collect，第一个 `Err`。
3. 否则 `ListNode::new`：先 `GeometryKind::ListIndent`（`src/layout/list.rs` 第 140–148 行），再按项 `MarkerKind::label`，越界是 `MarkerOutOfRange`（第 151–156、26–27 行）。

`item` / `item_marked` 总是 `push` 进 `items`，不看 `list.error`（`list.rs` 第 67–81 行）。子流错误来自 `parent.nested(f)`；`ListItem::new` / `custom` 只在子流 `Ok` 之后才跑。空 marker 在 `ListItem::custom` 变成 `GeometryKind::EmptyListMarker`（`src/layout/list.rs` 第 113–116 行）。非法子节点在 `ItemBody::new` → `check_content`（`src/layout/nesting.rs` 第 103–105、30–64 行），第一个不允许的节点就返回。因此子流自己的 `error` 会让 `into_nodes` 失败，`check_content` 根本不会看到那些节点。

`Columns::lower`（`columns.rs` 第 64–68 行）没有单独的错误槽，只按 `cols` 的调用顺序 collect。然后 `reject_auto_with_ratio`（第 72–80 行），然后 `ColumnsNode::new`：少于两列是 `ColumnWidthsReason::ColumnCount`，再对每一列 `check_content(Container::Column)`（`src/layout/columns.rs` 第 97–111 行）。这两步只在 collect 全 `Ok` 之后。列内容错误会挡住 `AutoWithRatio` 和列数检查。`col_fixed` / `col_ratio` 宽度非法时 **不调用** 内容闭包，只 `push(Err)`（`columns.rs` 第 39–51 行）。`gap` 失败则在当前位置 `push(Err)`，后面的 `col` 仍会跑。

`Table::lower`（`table.rs` 第 173–238 行）在 `add_to` → `push` 时运行：

1. `self.error` 有则返回。这个槽在 `store` / `cell_padding` / `border` 时按调用顺序第一次 `note`。
2. 有 `tracks` 时先比列数，再 `Track::into_width` collect（第一个坏 track），再 `reject_auto_with_ratio`。
3. 先 `lower_rows(header)`，再 `lower_rows(body)`。行内从左到右，`?` / `collect` 停在第一个内容 `Err`（段落、列表 `Result`、图片、横线、`Cell::new` 的 `check_content`）。
4. 最后 `TableNode::new`。跨度错误在这里带 `.in_cell(row, col)`（`src/layout/table.rs` 第 153–159、164–168、185–189 行）。内容错误不带单元格位置。

格子里的列表在 `store` 时就 `assemble_list` → `list.lower`（`table.rs` 第 119–131 行，`flow.rs` 第 99–120 行），得到 `StoredContent::List(Result<FlowNode>)`。这个 `Result` **不会** `note` 到表上。`Cell::spacer` 的失败会。所以后一格的 spacer 错误可以盖住前一格已经算好的列表/段落错误：`lower` 在第 174–176 行直接返回 `self.error`，不再解内容。

`header` 全部降完才降 `body`。track 校验在内容之前，所以「表自己的 `error`」压过 track，track 压过单元格内容，单元格内容压过 `TableNode::new` 的跨度错误。

**3. `Document.error` 与节流，在 `paint` 才比**

`Document::section`（`document_ty.rs` 第 118–152 行）：

- 已有 `self.error`：克隆并立刻 `return Err`，新闭包不跑。
- 隐式节 `written()` 为真：写入并返回 `ContentOutsideSection`。`written` 含「流里已有错误」（`flow.rs` 第 33–35 行，`section.rs` 第 41–47 行）。此时 **不会** 改成 `Explicit`，原节还在。
- 闭包返回 `Err`：`error.in_section(index)` 后写入并返回。失败的 `Section` 不推进 `sections`。`index` 在第一次调用时是 0，之后是已提交节的 `len`（第 132–140 行）。
- 闭包返回 `Ok(())`：**不看** `section.flow.error`。流上的错误留到 `paint`。

`margins` / `header` / `footer` / `behind` / `front` 在已经是 `Explicit` 时走 `fail_outside`（第 155–158 行）：仅当 `Document.error` 为空才写成 `ContentOutsideSection`，方法本身返回 `()`。钩子闭包不保存。隐式节上这些方法直接改节，不记错。

`paint`（第 161–202 行）的顺序：

1. `Document.error` 为 `Some` 则立刻 `return`。不 `lower` 任何节，不读 `closed`。
2. `Explicit` 时 `closed.into_nodes()?`。这是节后正文 `push` 产生的 `ContentOutsideSection`（或下面说的 `stack` 样式错误）。**不** 包 `in_section`。
3. 按节下标 `section.lower`，失败则 `map_err(|error| error.in_section(index))`。第一节的流错误挡住后面的节。`lower` 成功之后才把页眉/页脚/层包成闭包，钩子此时不跑（`section.rs` 第 107–113 行）。
4. 全部节都 `lower` 成功才 `paginate_sections`（`document_ty.rs` 第 202 行）。

因此：更早写在节流里的组装错，只要后来 `Document.error` 或拒收流被写上，公开结果就不是那条流错误。`section()` 的 `Err` 若被 `build` 里的 `?` 传出，`paint` 根本不跑，公开结果就是这次 `section()` 的返回值（已带 `in_section` 的那份）。

隐式节、构建返回 `Ok(())`、且 `Document.error` 为空时，`paint` 对第 0 节 `into_nodes`，再 `in_section(0)`。测试对得上：构建刚结束时 `section.flow.error == Some(Error::from(RunAfterFillBlank))`，`at` 为空（`src/document/tests.rs` 第 137–141 行）；`paint` 后是同一错误 `.in_section(0)`，且画布无命令（第 153–156、208–212 行）。`ContentOutsideSection` 的测试期望值不带 `Location`（第 468–482、486–494、533–541 行）。

**4. 分页之后才出现的 `note`（不是构建期）**

`paginate_sections` 先按节 `plan_pages`，错误 `in_section(section)`（`src/layout/paginate.rs` 第 97–100 行）。正文排完才 `measure_bands`（第 117 行）。某一节的正文测量错误会让页眉钩子不跑。

页眉钩子里的 `Band::note` 要到 `band_content` 被调用时才发生（`band.rs` 第 148–159 行）。`measure_bands` 对每一页的顺序是 header、footer、behind、front（`src/layout/decorate.rs` 第 176–185 行），再用 `in_region` + `on_page_index` + `in_section`（第 154–160 行）。层的 `note` 在 `layer_content` 里变成该页的 `Err`（`layer.rs` 第 272–285 行），由 `measure_layer` 的 `content(page)?` 抛出（`decorate.rs` 第 223 行）。

### Files Read

- `src/lib.rs`（`document` / `to_pdf`）
- `src/error.rs`（`Error`、`in_section` / `in_cell`、`Result` 别名、`ContentOutsideSection`）
- `src/document/document_ty.rs`
- `src/document/flow.rs`
- `src/document/flow_writer.rs`
- `src/document/section.rs`
- `src/document/list.rs`
- `src/document/columns.rs`
- `src/document/table.rs`
- `src/document/paragraph.rs`
- `src/document/style_props.rs`
- `src/document/hline.rs`
- `src/document/image.rs`
- `src/document/band.rs`
- `src/document/layer.rs`
- `src/document/mod.rs`
- `src/document/tests.rs`（流错误 vs `paint` 后的 `in_section`；节外内容）
- `src/layout/list.rs`（`ListItem::custom`、`ListNode::new`、`label`）
- `src/layout/columns.rs`（`ColumnsNode::new`）
- `src/layout/table.rs`（`TableNode::new` 的 `in_cell`）
- `src/layout/nesting.rs`（`check_content`、`ItemBody`、`CellBody`）
- `src/layout/image.rs`（`ImageNode::new` / `resolve_size`，`lower` 时失败，仍算组装）
- `src/layout/paginate.rs`（`paginate_sections` 在组装成功之后）
- `src/layout/decorate.rs`（页钩子错误的区域/页/节）
- `docs/error-diagnosability.md` 第 46–74 行（刀 4 未实现；写明今日是容器 `self.error` / `Document.error` 在 lower 时压过更早子项）

### Boundaries

- 组装记录止于「一个 `Option<Error>` 或 `Vec<Result>` 收成一个 `Err`」。公开面到 `document` / `paint` / `render` 仍是 `Result<_, Error>`。
- 子元素 `lower` 在 `push` 之前、构建闭包内完成。`paint` 不再在子项之间重选，只在 `Document.error`、拒收流、各节 `flow.error` 之间按上面的顺序取第一个。
- `Band` / `Layer` 的 `note` 不在 `document()` 的构建闭包里。钩子要等正文 `plan_pages` 成功、进入 `measure_bands` 才执行。组装失败时这些闭包不跑。
- `font` / `bold_font` / `load_image` 不进错误槽。`to_pdf` 在 `Rendered` 已经成功之后，不参与组装收集。
- 缺字、`UnbreakableWiderThanFrame`、`UnsplittableTallerThanFrame` 等来自 `split` / 测量（例如 `src/document/tests.rs` 第 160–180、437–464 行），不是 `note`。它们在 `plan_pages` 里失败，并且已经带 `in_region(Body)` 和页码；`paint` 的 `in_section` 不会覆盖已有节号，但这些错误在 `plan_pages` 里还没有节号，节号是 `paginate_sections` 另加的。
- 单元格内容错误没有 `in_cell`。只有 `TableNode::new` 的跨度错误有。随后 `paint` 再补节号。
- 内层会跳过闭包、因此根本不会产生后续错误的路径：`Paragraph::link` 空 URI（`paragraph.rs` 第 109–115 行）、`Columns::col_fixed` / `col_ratio` 宽度非法、`Flow::stack` / `Layer::stack` 样式失败、`Layer::rotated` 非有限角度（`layer.rs` 第 138–140 行）、`Band` 空 URI（`band.rs` 第 74–79 行）。`Document::section` 在已有 `Document.error` 时也不跑新闭包。

### Non-Obvious Things

- **容器槽压过更早的子项，而且是在 `lower` 一开始。** `List.error` 盖住 `items` 里已经记下的第一个 `Err`，即使 `note` 发生在 `item()` 之后。`Table.error` 盖住更早格子里的段落/列表 `Result` 和尚未做的 track 校验。后写的 `cell_padding` / `border` / 后格 `spacer` 可以成为公开错误，前格的内容错误被丢掉。`Columns` 没有这个槽：谁先被 `push` 进 `cols`，collect 就取谁。结构检查（列数、`AutoWithRatio`、`check_content`、列表缩进、marker 越界、表跨度）都在「槽和 `Vec<Result>` 都干净」之后，所以它们压不过更早的子项错误，反而会被子项错误挡住。
- **`Document.error` 压过节流里更早的错误。** `paint` 先返回它，不调用 `into_nodes`。隐式节上先有段落组装错（`written()` 为真），再调用 `section()`，公开结果变成 `ContentOutsideSection`，原来的流错误还在 `section.flow.error` 上，但不会被读。节后 `margins` / 页钩子走 `fail_outside`，同样压过已经写在各节流里的错误，也压过 `closed` 流。失败节闭包返回的任意 `Err` 会丢掉该节对象上的 `flow.error`（节没被 push）。
- **拒收流用 `ContentOutsideSection` 替换即将推入的 `Err`。** `push` 在匹配 `Result` 之前就返回（`flow.rs` 第 45–47 行）。显式节之后再 `paragraph` 一个本身就会 `note` 失败的段落，公开错误是 `ContentOutsideSection`，不是段落的错误。`spacer` 的非法长度同样被换掉。`keep_together` / `columns` / `table` 的子流不是拒收流，内部错误会算出来，再在外层 `push` 时被换成 `ContentOutsideSection`（外层 `error` 仍为空时）。
- **`stack` 写拒收流时不经过 `reject_pushes`。** 样式 `apply` 失败会把 `TextProps` 的错误直接放进 `closed.error`（`flow_writer.rs` 第 221–226 行），`body` 不跑。这条错误没有 `in_section`。样式成功时，`body` 里的 `push` 才变成 `ContentOutsideSection`。
- **`Flow::push` 是唯一在记录后就拒绝后续节点的收集器。** `List::item`、`Columns` 的 `cols.push`、`Table::row` 在已有错误后仍继续收录；要到各自的 `lower` 才丢掉。但 `add_to` 先 `lower` 再 `push`：父流已有错误时，兄弟容器仍会 `lower`（内部仍是单错收口），然后整份 `Result` 被 `push` 丢掉，不会替换父错误。
- **两边都失败时取左边。** `Band::inset` 与 `Layer::line` / `Shapes::line` 的匹配是 `(Err(error), _) | (_, Err(error))`（`band.rs` 第 40 行，`layer.rs` 第 95–96、221–222 行）。两个转换都失败时只 `note` 左边那个。
- **延迟错误在构建结束时通常还没有 `Location`。** 节号是 `paint` 第 186 行才 `in_section`。`section()` 闭包自己返回的 `Err` 在写入 `Document.error` 时就已经 `in_section`，`paint` 不再包一层。拒收流和 `fail_outside` 的 `ContentOutsideSection` 一直 `at: None`。
- **有错即整份失败，没有部分页。** `paint` 在 `paginate_sections` 之前返回时，测试要求 `RecordingCanvas` 命令为空。前面节已经 `lower` 成功、后面节失败时，那些成功节点也被 `?` 丢掉，不会画。

### Open Questions

- 仓库里没有测试把「子项已记下的 `Err`」和「随后的 `List.error` / `Table.error` / `Document.error`」对打，来锁死谁赢。上面的覆盖关系来自 `lower` / `paint` 的控制流，不是来自断言。现有节测试用的是合法段落，没有覆盖「拒收 `push` 丢掉传入 `Err`」和「`stack` 在拒收流上记下样式错误」。
- `Cell` 内容失败不打 `in_cell`，是当前代码事实；刀 4 文档只说要纠正优先级，没有单独写单元格定位是否要补。
- 页钩子 `note` 与正文组装不在同一次收集里竞争（组装失败则钩子不跑）。若把它们算进「今日的首错」，顺序是：`build` 已返回的 `Err`，否则 `paint` 的 `Document.error`，否则拒收流，否则节流，否则 `plan_pages`，否则 `measure_bands` 的 header → footer → behind → front。测量期缺字是否还能在组装错之后继续收集，不在这条组装路径里。