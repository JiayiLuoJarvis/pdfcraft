# 已确认元素补接线

对调用方：把提案书已确认、引擎已实现、DSL 未接出的能力接到 `document`。合同表、公文标题居中、落款右对齐、页眉全页宽，这些现在写不出来或写出来和提案书相反。

对维护者：只接线，不改 `layout/` 的分页与绘制语义（R4 的 Auto 求解除外）。`docs/提案书.md` 仍是产品权威。本文不改提案书。本文是实现工单。

审查来源：2026-10-10 元素三轴审查。模块名保持 `layout`，不改成 `elements`。

## 怎么读

按工作项顺序做。一项一个可测状态。每个工作项先写失败的 `RecordingCanvas` 集成测试，再改 `src/document.rs` 的 lowering，最后只在引擎行为与提案书冲突时改 `src/layout/`。

公共签名未在提案书钉死。本文钉第一版签名，使各工作项不互相发明。实现时不要另起一套名字。

不做：流内 canvas、qr、svg、toc、Justify、奇偶 `page_break`、具名 styles 或 images、`Layer::text_at` URI、仅图片可点、第二 PDF 后端、把 `layout` 改名为 `elements`。

## 现状与目标

| 项 | 提案书 | 现在 | 目标 |
| --- | --- | --- | --- |
| 页眉水平 | §2.10 横跨 `0 … page_width`，调用方用传入的 margin 自己缩进 | `Band::lower` 用左右 margin 铺空轨，槽被强制对齐正文 | 去掉自动缩进。`left` / `right` 贴页边。调用方用 `inset` 自己缩 |
| 段落对齐 | §3 Start / Center / End | 引擎有，段落 DSL 不改 `align` | `Paragraph::center` / `end`，默认 Start |
| 表列宽与外观 | §2.6 `Fixed` / `Ratio` / 至多一列 `Auto`，`cell_padding`，边框宽 0 不描边 | 等分、padding 0、边框 0，调用方设不了 | `Table` 接出三项。默认保持现在的等分无边框 |
| 栏列宽与间距 | §2.5 同上列宽，`column_gap` 默认 12 pt | `col` 全是 `Ratio(1)`，gap 写死 12 pt | `col` 仍等分。另加定宽、比例、Auto、`gap` |
| Auto 与 Ratio 同列 | §2.5 Auto 吃剩余 | `AUTO_WITH_RATIO_SHARE = 0.5`，DSL 走不到 | Auto 吃尽 Fixed 之后的剩余。有 Ratio 时先禁止混用，见 R4 |
| 列表 start 与标记列 | §2.8 `start` 默认 1，`marker_width` 可配，`nest_indent` 默认同标记列 | start 写死 1，标记列 18 pt，`nest_indent: None` | `ol` 可带 start。`List` 可设标记列宽与嵌套缩进 |
| 单元格列表闭包 | 无单独条。与 `Cell::paragraph` 应对齐 | `Cell::ul` / `ol` 要求 `'static`，降低时才跑 | 与段落一样当场跑。无生命周期 |

## 公共数据形

这些类型进 `src/document.rs`，由 `lib.rs` 重导出。不要把 `layout::ColumnWidth` 直接公开。

```rust
pub struct Ratio(f32); // Ratio::new 拒绝非有限或 ≤ 0，错误仍是 ErrorKind::ColumnWidths

pub enum Track {
    Fixed(Length<Positive>),
    Ratio(Ratio),
    Auto,
}
```

边框不导出 `Border`。调用方用 `Table::border(width, color)`。宽 0 不描边，默认黑。

`Track` 只在 `Table::tracks` 与 `Columns` 的分栏方法之间用。lowering 时映到内部 `ColumnWidth`。`Ratio` 的权重语义与现在的 `layout::column_width::Ratio` 相同。

`TextAlign` 继续 `pub(crate)`。段落、图、线、页眉槽继续用方法设对齐，不把该枚举交给调用方。

## 工作项顺序

### R1. 页眉页脚按 §2.10 画满页宽

**改谁。** `Band`、`Band::lower`（`src/document.rs`）。测试：`header_slots_inset_by_side_margins_and_body_sits_below_the_band` 及同类。不改 `measure_band` 的 `x = 0` 原点（已经对）。

**数据。** `Band` 增加一对非负 `inset_left` / `inset_right`，默认 0。不要再从 `page.margins` 偷宽度。

**调用方。**

```rust
doc.header(|b| {
    let m = b.page().margins;
    b.inset(m.left(), m.right()); // 想跟正文齐才写
    b.left("文号");
    b.right(format!("第 {} 页", b.page().number));
});
```

`left` / `right` / `left_link` / `right_link` 保留。槽画在「页宽减去 inset」里，Start / End。inset 为 0 时文字可贴到 `x = 0` 与 `x = page_width`。

**错误。** 带内装不下仍报错，不撑开正文。`header_with_zero_margin` 行为不变（带高 0 仍拒内容）。

**不在本工作项。** 带内自由 path / 图。左右双槽够用。不要把 `Band` 做成第二套 `Flow`。

**验收。** 左右 margin 非 0、调用方不写 `inset` 时，左槽原点 `x = 0`（测 `RecordingCanvas`）。写了 `inset(left, right)` 后，左槽原点等于 `left`。正文框坐标与有无页眉无关，现有测试继续过。

### R2. 段落 Center / End

**改谁。** `Paragraph`、`lower_paragraph`。`Cell::paragraph` 走同一条 lower，会自动获得对齐。

**数据。** `Paragraph` 增加 `align: TextAlign`，默认 `Start`。

**调用方。** 与 `ImageBox` / `HLine` 同形：

```rust
p.center();
p.end();
```

不要 `start()`。默认已是 Start。后写覆盖先写。

**验收。** 单行短于行宽时，Center 的字形原点在框中线附近，End 贴右缘。多行每行各自对齐（引擎 `align_x` 已如此）。`first_line_indent` 与 Center / End 同时出现时，按现有 `LineBudget.first`：首行可用宽先减缩进再对齐。不要为本组合发明新规则。

### R3. 表的列宽、padding、边框

**改谁。** `Table`、`Table::lower`。`TableSpec` 字段已有，只填值。

**默认。** 与现在相同：列数取自首行 span 和，每列 `Track::Ratio` 权重 1，`cell_padding` 0，边框宽 0。未调这三项的旧测试坐标不变。

**调用方。**

```rust
t.tracks([
    Track::Fixed(mm(28.0)),
    Track::Ratio(Ratio::new(1.0)?),
    Track::Auto,
]);
t.cell_padding(pt(4.0));
t.border(pt(0.5), Color::rgb(0, 0, 0));
```

`tracks` 长度必须等于首行列数（span 和）。对不上则 `Error::ColumnWidths { reason: "column-count" }`，组装期失败。只允许一列 `Auto`。多于一列 Auto 用已有 `"more-than-one-auto"`。

未调用 `tracks` 时不要要求调用方写列宽。

**格内宽。** 已有公式 `列宽 - 2*padding - 边框占位`。接上 padding / 边框后用现有 `table.rs` 测量，不要在 `document.rs` 重算。

**验收。**

- 三列 Fixed + Ratio + Auto，表宽吃满当前框。Fixed 列为给定 pt。
- padding 非 0 时，格内段落在更窄的 `W` 处断行（比对照表更早折行）。
- 边框宽 0 不产生描边命令。宽 `> 0` 时 `RecordingCanvas` 上有对应 `Stroke`。
- `repeat_header`、合并、格内禁 columns 的现有测试不改语义。

### R4. 栏的列宽、gap，以及 Auto 吃剩余

**改谁。** `Columns`、`Columns::lower`。`src/layout/columns.rs` 的 `resolve_column_widths`。

**默认。** `col(|f| …)` 仍是 `Ratio(1)`。`gap` 默认 12 pt。至少两列。未调新方法的旧测试坐标不变。

**调用方。**

```rust
c.gap(pt(8.0));
c.col_fixed(mm(40.0), |f| { … });
c.col_ratio(2.0, |f| { … });
c.col_auto(|f| { … });
c.col(|f| { … }); // Ratio(1)，保留
```

**Auto 与 Ratio。** 提案书写 Auto 吃剩余，没有写与 Ratio 如何分。现在的 `0.5` 没有提案书依据。本工作项规则：

- 只有 Auto、没有 Ratio：Auto = `inner - sum(Fixed)`。已是这样。
- Auto 与 Ratio 同时出现：组装期 `Error::ColumnWidths { reason: "auto-with-ratio" }`。调用方用纯 Ratio 或 Fixed+Auto。
- 删掉 `AUTO_WITH_RATIO_SHARE`。

这样公开面不会出现「Auto 只拿一半」的静默行为。引擎求解器与 DSL 同一条规则。

**验收。** 两列 Fixed + Auto，Auto 列宽 = 框宽 − gap − Fixed。`gap(0)` 时两列紧贴。`col` 两个等分仍距 12 pt。Auto+Ratio 的 `columns` 与 `table.tracks` 都失败。

### R5. 列表 start、标记列、嵌套缩进

**改谁。** `Flow::ol`、`Cell::ol`、`List`、`List::lower`。`ListSpec` 字段已有。

**调用方。**

```rust
doc.ol(Numbering::Decimal, |l| {
    l.start(NonZeroU32::new(4).unwrap());
    l.marker_width(pt(24.0));
    l.nest_indent(pt(24.0));
    l.item(|f| { … });
});
```

`start` 只对有序枚举编号有意义。`ul` 上调用 `start` 忽略或组装失败。选忽略，与「无序号」一致，并在 rustdoc 写明。`item_marked` 仍覆盖该项标记，不消耗序号（保持现有 layout 行为，不要改计数规则）。

默认：`start = 1`，`marker_width = 18 pt`，`nest_indent = None`（内部已等于标记列宽）。未调这三项的旧测试坐标不变。

`Cell::ol` 与 `Flow::ol` 同一套 `List` 方法。

**验收。** 从 4 起编的十进制标记是 `4.` `5.`。自定义 `marker_width` 过窄且标记更宽时，现有 `custom_marker_wider_than_marker_width_errors` 同类错误。嵌套列表在设置 `nest_indent` 后，内层左缘再右移该长度。

### R6. 单元格列表当场组装

**改谁。** `Cell::ul` / `ol`、`CellContent`、`Table::lower`。依赖 R5 已把 `List` 配置放在 builder 上。

**数据。** 删 `DeferredList` 与 `Box<dyn FnOnce + 'static>`。`CellContent::List` 改为已 lower 的 `FlowNode`，或改为已跑完的 `List` 再在 `Table::lower` 里 `list.lower(kind)`。选后者若 `List` 能在构造时拿到 `Flow::child()` 的 `Style` 与 `DocToken`。

`Table` 构造时已有外层 `Flow`（`Flow::table` 里的 `self`）。`Cell::ul` 必须能读到同一套 style / token。做法：`Cell::ul` 不再是纯关联函数能在表外凭空造列表。改为 `Table` 上的方法，或 `Cell::ul` 在 `Table::row` 写入时再跑。为少动调用方，保持 `Cell::ul(f)` 的外形，在 `Table` 上暂存闭包会回到 `'static`。

选定：`Table` 在 `header` / `row` 入格时，对 `Cell` 里未跑的列表闭包立即 `assemble_list`。这仍要求闭包活过 `row(...)` 调用，不要求 `'static`。`Cell::ul` 的签名改为 `impl FnOnce(&mut List)`，去掉 `+ 'static`。

实现时若发现 `Cell` 在 `row` 之外先造好再 `collect`，闭包仍要活到 `row`。这与现在「move 进 Cell」兼容，且允许借用 `table` 闭包里的局部（局部活过 `t.row`）。

**验收。** `tests/document.rs` 里现有「必须 move 图像进 `Cell::ul`」的用例改为可以借用 table 闭包局部。文档注释删掉 `'static` 说明。`Cell` 继续不 `Clone`。

### R7. 内部可见性与死代码门（规范，不改行为）

**改谁。** `src/lib.rs` 的 `layout` / `text` 改为 `pub(crate) mod`。各已完成模块去掉 `#![allow(dead_code)]`。删或 `pub(crate)` 空的 `layout/dump.rs`。

**验收。** `cargo clippy --all-targets -- -D warnings` 与 `cargo test` 全绿。对外 rustdoc 不再出现 `pdfcraft_pdf::layout`。无公共签名变化。

本工作项可与产品工作项并行，但不要和 R1–R6 挤在同一 diff。

## 刻意不做（审查里提过、本轮不修）

- `Document` / `Section` 对 `Flow` 的 `Deref`。DSL 可用，不换。
- `Error::InvalidGeometry` 拆变体、`ColumnWidths` 的 `&'static str` 改枚举。行为不变时再开工作项，避免测试大面积改匹配。
- `paginate` / `split_stack` 的 `Vec::remove(0)`。行为正确，是性能债。
- `Style` 补 `Copy`、`Positive` 的 const 工厂。顺手可以，不单开产品工作项。
- 过宽错误统一成一种变体。先保持图 / 线走 `UnbreakableWiderThanFrame`，表与缩进走过宽几何。若 R3/R4 新路径能沿用已有变体就沿用，不要为统一而改旧测试。
- 列表项放图片、栏内 `page_break`。提案书白名单没有它们。保持拒绝。栏内 `page_break` 的权威句在提案书 §2.5。公开 `stack` 是样式作用域，不是粘页分组。`Page::is_last` 是全文末页。§2.12 旋转只覆盖 path（`line` / `rect` / `ellipse`），不含 `text_at` / `image_at`。

## 每个工作项的门禁

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
# 有 PDF 断言时再加：
# cargo test --features backend-krilla
```

排版断言只打 `RecordingCanvas` 坐标与命令。`to_pdf` 只冒烟 `%PDF`。不要在 `tests/document.rs` 再数 `/Subtype`。

`document.rs` 继续放 lowering。不要为这七个工作项把 DSL 拆文件。

## 原则对照

- 提案书 > 本文 > 设计稿未确认部分。
- 未调新方法的调用方，坐标与错误与改前相同。
- 引擎已有的字段先接线。没有的求解规则（Auto+Ratio）按提案书收成可报错的组合，不保留无依据的 `0.5`。
