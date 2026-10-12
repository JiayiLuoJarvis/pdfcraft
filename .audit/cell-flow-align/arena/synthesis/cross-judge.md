# Cross-judge: Cell 入口对齐 FlowWriter / add

只读仓库现状（`feat/flow-writer` 上的 `table.rs`、`flow_writer.rs`、`layout/nesting.rs`，以及提案书 §2.6）。不改设计稿。

姿态权重：0.x 可以破坏调用点；主题内部收口优先于为旧签名打补丁；单元格要共享 FlowWriter / `add` 的入流模型（密封的窄准入也算，前提是复用同一批元素值）；§2.6 的禁止项必须留下来。下一轮实现的人扩展正文糖时，不能把 `columns` / pageBreak / `keep_together` / 嵌套 table 变成单元格里的合法调用。

## 1. 分项

| 准则 | C1 | C2 |
| --- | ---: | ---: |
| 1. 用法贴合 | 4 | 3 |
| 2. 共享构建结果 | 5 | 4 |
| 3. 产品禁止面 | 3 | 4 |
| 4. 父流 / 列表生命周期 | 4 | 5 |
| 5. 接口深度 | 4 | 3 |
| 6. 迁移 | 4 | 4 |
| **合计** | **24** | **23** |

### 证据（各一句）

1. **用法。** C1 的日常格是 `t.cell(|c| …)`，`c.add(Paragraph::build(…))` 和正文工厂同一条，但写格的助手必须拿着 `&Table`，禁止方法也挂在这个被教成「就是一个流」的接收者上。C2 的 `Cell::paragraph` / `ul` 仍是今天的形状，`empty().add` 能接 `Paragraph`、`HLine`、`PlacedImage`、`Spacer`、`ListBlock`，可是设计稿开头的 `t.row(["Qty", Cell::new(…), Cell::paragraph(…)])` 元素类型不一致，这段主示例在 Rust 里编不过。
2. **共享构建。** C1 删掉六个 `Cell::*` 内容构造，段落、线、图、列表都进现有 `FlowWriter` 默认方法里的 `build` / `build_on`。C2 的糖是 `empty().add(共享值)`，正文 `ul` / `spacer` 也改道到 `ListBlock` / `Spacer`，但 `IntoCellItem` 加上 `lower_piece` 仍是 `IntoFlowItem::add_to` 旁边的第二份准入和降低。
3. **禁止面。** C1 把 `c.columns` / `page_break` / `keep_together` / `table` 留成真方法，拒绝本应落在 `check_content(TableCell)`，草图却写了子流已有错误就跳过检查，而且 `c.stack` 会成功（§2.6 单元格允许列表里没有 stack）。C2 的 `Cell` 上没有这四个方法，`Table` 和 `Columns` 也不实现 `IntoCellItem`，所以 `cell.add(table)` 是缺 trait；单元格列表项里的 `item.table()` 仍能编译，只靠现有 list-item 矩阵以 `container: "list item"` 拒绝。
4. **父流与借用。** C1 在 `table.parent.child()` 上当场跑闭包，样式和 `DocToken` 是对的，借用比「活到 `row`」更短，但作者自己把 `t.row([t.cell(…)])` 标成还要用真 `Table` 钉死的借用，段落错误也会从 `Table::lower` 提前到 `cell`，跨行发现顺序会变。C2 把 `ListBlock` 留到 `store`，对 `table.parent` 做一次 `List::build_on`（里面一次 `child()`），这就是今天的 `assemble_list`，`cell_list_can_borrow_a_table_local` 的借用和「后一行的列表错误先于前一行的段落错误」都还在。
5. **接口深度。** C1 的公开增量是一个方法、去掉一个生命周期、删掉 `CellContent` / `StoredContent` / `DeferredList`，矩阵仍只在 `nesting.rs`，但单元格槽把整个 `FlowWriter`（含 `flow_mut`）交出去了。C2 用 `Piece<L>` 把生命周期缝收在列表载荷上，避免了第二套存储枚举，公开面却多出 `IntoCellItem`、`ListBlock`、`Spacer`，以及 `map_list` 和 `lower_piece` 两处要手写同步的匹配。
6. **迁移。** C1 对 `Cell::new` / `From<&str>` / `col_span` / `fill` 有一张机械对照表，仓库里每处 `Cell::paragraph` 和 `Cell::ul`（`tests/document.rs`、`examples/hello.rs`、`examples/bill.rs`）都要改写，禁止项从今天的编译失败变成 `InvalidCellContent`。C2 的现有调用点可以不改，故事最清楚；「测试一行不动」仍取决于两件还没证明的等价：非法 spacer 不再塞零高度节点，以及 `assemble_list` 的 `start` 合并收进 `BuiltList::lower` 之后和今天一致，同时正文 `ul` / `ol` / `spacer` 也要改道。

## 2. 合计

- Candidate 1：24 / 30
- Candidate 2：23 / 30

不是平局。C1 多的那一分在「删掉平行构造、直接复用 `FlowWriter`」。

## 3. 推荐底稿

**以 Candidate 2 为底。**

裸分更高的是 C1，按「下一轮扩展不能破坏 §2.6」这一条，底稿仍是 C2。C1 的单元格就是 `&mut Flow`。之后给 `FlowWriter` 加一个糖，单元格闭包上立刻就有这个方法；它只要降低成段落、列表、图、线、spacer 或 stack，矩阵就会放行。四个禁止方法今天也能通过类型检查，只靠 `document()` 里的 `InvalidCellContent`，而且草图允许子流已经记错时跳过 `check_content`。这和列表项的现状一样，但列表项本来就是流；单元格若走同一条路，§2.6 的禁止面会跟着正文 API 一起变宽。

C2 的允许集是 `table.rs` 里密封 trait 的五条实现（`Paragraph`、`HLine`、`PlacedImage`、`Spacer`、`ListBlock`）。`IntoFlowItem` 的印章在另一个模块，给 `Table` 或 `Columns` 补单元格 impl 必须有人进 `table.rs` 改印章。`page_break`、`keep_together`、`stack` 不是值，单元格类型上也不会长出这些方法。0.x 允许把 `Cell::paragraph` 改成 `t.cell(|c| …)`，但 FRAME 写明窄密封准入可以成立，只要构建结果仍是 `Paragraph::build` 那一批。C2 把旧糖收成 `empty().add(…)` 的一行，满足这一条；它没有为了兼容留下 `CellContent` / `StoredContent` / `DeferredList`。

选 C2 的代价要写在实现约束里：`fn emit(w: &mut impl FlowWriter)` 这种泛型写入器喂不进单元格，共享的是元素值，不是写入器本身。这是 Pole B 的边界，不要在实现时补一个 `Cell: FlowWriter` 把两极捏成中间态。

## 4. 从 C1 值得嫁接的（3 条）

1. **非列表片段的降低走现有 `IntoFlowItem::add_to`。** 在 `Table::lower` 里对 `table.parent.child()` 重放 `Paragraph` / `HLine` / `PlacedImage` / `Spacer`，删掉手写的 `lower_piece` 分支。列表仍在 `store` 时 `ListBlock::resolve` → `BuiltList::lower`。这样正文和单元格不会各改各的 `lower_paragraph`。
2. **把 C1 的 `cell_add_matches_sugar` 收成硬断言。** `Cell::paragraph(f)` 与 `Cell::empty().add(Paragraph::build(f))`（线、图、spacer、`ListBlock::bullet` 同样）必须录出同一页。六条旧糖只许是这一行 `add`，不许长回自己的 builder。
3. **内容失败不能改列跨度和。** C1 用失败占位保住该格的 `col_span`，避免内容错误再冒出一轮 `ColumnWidths` 或 `EmptySlot`。C2 应沿用今天的算法：列数按存储行的 span 合计，降低失败只让该行组变成 `None`，不要先把失败格从行里拿掉再去对轨道。

## 5. 不修则否决

### 对 C2（不修则不要按这份底稿落地）

- 主示例里 `&str` 和 `Cell` 混在同一个数组。实现时不许为此加新的 `From`、异构行宏或第二套字符串入口；把示例改成同质数组（全是 `Cell::new` / `Cell::paragraph`，或全是 `&str`）。
- `lower_piece` 若发展出自己的禁止匹配，或对段落、线、图再写一套降低，而不是调用已有的 `lower_paragraph` / `HLine::lower` / `PlacedImage::lower`，共享构建就不成立。
- `BuiltList` 必须保持不可被单元格准入。`ListBlock` 在已经绑过父流的列表上再 `build_on` 一次（双次 `child()`）会拿错样式或 `DocToken`。
- 正文 `ul` / `ol` / `spacer` 改道之后，拒绝流上的非法高度仍须是 `ContentOutsideSection`，有序列表的 `start` 覆盖必须和今天的 `assemble_list` 一致。`tests/document.rs` 现有断言要原样通过，不许为了新类型把断言改弱。
- 给 `Table`、`Columns` 或 `BuiltList` 实现 `IntoCellItem`，或在 `Cell` 上增加 `page_break` / `keep_together` / `stack` / `columns` / `table`，这份设计的禁止面就没了。

### 对 C1（不修则不能拿来换掉 C2）

- `t.row([t.cell(…)])` 以及 `t.header([head(t, …), …])` 必须在真实 `Table`（`&self` 的 `cell`、`&mut self` 的 `row`、闭包借用表闭包里的局部）上编译通过。不过则 Pole A 没有入口。
- `check_content(TableCell)` 在子流错误袋非空时也要跑。按草图 TODO 跳过的话，同一格里的坏段落会把 `columns` / `page_break` 从错误袋里抹掉。
- `FlowWriter::stack` 在单元格里会成功，§2.6 没有把 stack 列为格内孩子。要么从单元格槽拿掉这个方法，要么先改提案；不能靠「矩阵会递归进 stack」把它算进这刀。
- 之后任何新的 `FlowWriter` 方法都会出现在单元格上。在这个方法集合被收窄之前，C1 不符合「扩展时禁止项保持类型上不可写或只有一条拒绝路径」；收窄之后它就不再是 Pole A，应重开评审，而不是在 C2 上悄悄加一个内部 `Flow`。
