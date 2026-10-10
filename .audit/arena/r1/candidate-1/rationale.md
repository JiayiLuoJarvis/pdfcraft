# R1 候选 1：取舍理由

## 为什么是轨道垫片

现状的 bug 只在一处：`Band::lower` 把空 `Fixed` 轨的宽度从 `page.margins` 读。垫片这个机制本身没错，它就是在全页宽的带框里做水平缩进的现成办法。错的是宽度来源：工单要求「不要再从 `page.margins` 偷宽度」。

所以改法是换来源，不换机制：

- diff 只有三处：结构体加两个字段，构造处填两个 0，`lower` 换一行来源，外加一个 `inset` 方法和注释。
- 删掉的是 margin 耦合（`let margins = self.page.margins;`），没有新增分支。`lower` 不再读 `self.page`。
- `ColumnsNode`、`resolve_column_widths`、`measure_band` 都已被现有测试覆盖。链接矩形、Start / End 对齐、gap 0 等行为照旧，不用再验一遍。
- `layout/` 一行不动，符合「只接线，不改 layout 语义」。

## 考虑过、没选的

### A. 在 `measure_band` 里缩框（把带框改成 `x = l, width = W − l − r`）

需要把 inset 传进 `BandContent` 的返回类型或 `PageInfo`，要改 `layout/decorate.rs`，还会碰到 `x = 0` 原点。工单明说不改 `measure_band`，AGENTS 要求 lowering 留在 `document.rs`。否决。

### B. 给 `TextNode` / `Style` 加左右 padding

需要在 `layout` 的文字节点里加新语义，影响段落、单元格，范围远超 R1。否决。

### C. 保留 margin 默认，`inset` 只做覆盖（`Option<Length>`，`None` 用 margin）

和 §2.10「画 header/footer 时无视左右 margin」正面冲突，验收要求「不写 inset 时左槽 x = 0」。否决。

### D. 一个槽一个 inset（`left_inset` / `right_inset` 挂在 `BandSlot` 上）

名字不在工单里，而且双槽共享一排 Columns 时语义更绕。工单钉的是 `Band` 上的一对字段加 `b.inset(left, right)`。否决。

### E. `inset` 接 `f32` 或 `Length<Finite>`，返回 `Result` 或走 `note`

`Margins::left()` 返回 `Length<NonNegative>`。用同一品牌，工单示例零转换，非负由类型保证（§2.0「构造期拒绝」）。不需要运行期校验，也就没有新的错误路径。否决。

### F. 在 `lower` 里预检 `l + r ≥ page_width`，映射成 `DecorationWiderThanFrame`

报错名字对调用方更贴切，但要多写约五行，并且要在 `document.rs` 里复制 `resolve_column_widths` 的边界判断（`fixed > inner`、`rest ≤ 0`）。本刀先不做：现有的 `ColumnWidths` 已经能在开画前失败，满足「装不下仍报错」。如果评审认为错误名比 diff 大小更重要，这是唯一值得追加的改动，而且不影响其余结构。

### G. 双槽改成 `Start` 槽 + `Auto` + `End` 槽，或单轨内左右叠放

会改变现有槽宽分配（各占一半），属于行为变化，超出 R1。保持 `R1 R1`。

## 风险

- **调用方行为变化**：有页眉且左右 margin 非 0 的旧调用方，页眉会从对齐正文变成贴页边。这正是 §2.10 的语义，也是工单目的。仓库仍是 `0.x`，`publish = false`，CHANGELOG 记一条即可。
- **inset 超宽的报错名**：见 F。
- **同名冲突**：`lower` 里的局部闭包叫 `inset`，和新方法同名。闭包作用域不冲突，可以编译。仍建议改名 `spacer`，读起来不混淆。
