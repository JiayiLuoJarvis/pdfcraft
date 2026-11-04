# 表格用法指南（调用方）

本文只讲正文流里的 `Block::Table` / `TableSpec`、表头、行与单元格。
字段对照见 [migrate-0.6.md](migrate-0.6.md)。完整字段与错误面见
[guide.md](guide.md) 与 [docs.rs](https://docs.rs/pdfcraft-pdf)。

目标：用 `TableStyle` 定列宽 / 边框 / 内边距，用 `TableHeader` 管表头是否跨页重复，
单元格里直接放普通 `Block`（通常是段落），而不是已经删除的 `CellBlock`。

下文代码里字体键用 `()`，和 guide 一样。正式项目里换成你的 `K`。

---

## 强烈建议

| 做 | 不要 |
| --- | --- |
| 短文本用 `Block::paragraph(font, text)` | 找已经删除的 `CellBlock::Text` |
| 表头写 `header: Some(TableHeader { .. })`，无表头写 `None` | 用空 `rows` 表达「有开关无内容」还指望画出表头（空 `rows` ≡ `None`） |
| 跨页重复表头靠 `TableHeader.repeat`（默认 `true`） | 每页手复制一行表头进 `body` |
| 列宽用 `TableStyle.tracks` | 指望库按内容自动猜比例还静默挤窄超宽表 |
| 格内统一加粗用表外层 `Block::Styled` | 每个单元格段落重复写 `bold` |
| 格内条款用**合法**列表（见下文禁止项） | 在单元格里放 columns / 嵌套表 / `KeepTogether` / `PageBreak` |
| 字面量写 `..TableSpec::default()` / `..CellSpec::default()` | 漏 `col_span` / `row_span` / `style` |

---

## 心智模型

```text
TableSpec
├─ style: TableStyle          ← tracks / cell_padding / border
├─ header: Option<TableHeader>
│     repeat: bool            ← 跨页后是否重画非空表头（默认 true）
│     rows: Vec<TableRow>
└─ body: Vec<TableRow>
      children: Vec<CellSpec>
        style.fill
        children: Vec<Block>  ← 普通块，不是 CellBlock
        col_span / row_span
```

```text
页框
│
├─ （可选）表头行 ─── 分页后续页可按 repeat 再画
│
└─ 表体行 ─────────── 可在行缝分页；合并单元格不能被缝切开
```

- 单元格孩子自上而下排。
- `Block::Styled` / `Block::Inset` / 合法列表可以进格。
- 栏、嵌套表、`KeepTogether`、`PageBreak` 在 `layout_spec` 失败：
  `InvalidCellContent { container: "table cell" }`。
- 格内列表项仍受单元格规则约束：项里再放 columns，报错容器仍是 `"table cell"`；
  项里放 `PageBreak`，容器是 `"list item"`。

---

## 最小表

```rust
use pdfcraft_pdf::spec::{
    Block, CellSpec, TableBorder, TableHeader, TableRow, TableSpec, TableStyle,
};
use pdfcraft_pdf::{Color, NonNegative};

fn simple_table() -> TableSpec<()> {
    TableSpec {
        style: TableStyle {
            cell_padding: NonNegative::pt(4.0).unwrap(),
            border: TableBorder {
                width: NonNegative::pt(0.5).unwrap(),
                color: Color::rgb(0, 0, 0),
            },
            ..TableStyle::default()
        },
        header: Some(TableHeader {
            rows: vec![TableRow {
                children: vec![
                    CellSpec {
                        children: vec![Block::paragraph((), "名称")],
                        ..CellSpec::default()
                    },
                    CellSpec {
                        children: vec![Block::paragraph((), "数量")],
                        ..CellSpec::default()
                    },
                ],
            }],
            ..TableHeader::default() // repeat: true
        }),
        body: vec![TableRow {
            children: vec![
                CellSpec {
                    children: vec![Block::paragraph((), "零件 A")],
                    ..CellSpec::default()
                },
                CellSpec {
                    children: vec![Block::paragraph((), "3")],
                    ..CellSpec::default()
                },
            ],
        }],
    }
}
```

无表头：

```rust
TableSpec {
    header: None,
    body: rows,
    ..TableSpec::default()
}
```

`header: Some(TableHeader { rows: vec![], .. })` 与 `None` 相同，不会画出表头，
也不会按「有表头」去重复。

---

## TableStyle

| 字段 | 含义 | 默认 |
| --- | --- | --- |
| `tracks` | 每列 `Track`；`None` 时由布局按规则分配 | `None` |
| `cell_padding` | 每个单元格四边内边距 | `0` |
| `border` | 网格线；`width == 0` 不画线 | 零宽、黑 |

`Track` 与分栏相同：`Fixed` / `Ratio` / `Auto`。至多一个 `Auto`，且不要与
`Ratio` 混用（`ErrorKind::ColumnWidths`）。`Ratio::new` 拒绝非有限或非正权重。

表比正文框更宽，或不可拆片段高于整页正文框时，会以明确 `ErrorKind` 失败，
而不是静默缩小。

```rust
use pdfcraft_pdf::{Positive, Ratio, Track};

TableStyle {
    tracks: Some(vec![
        Track::Fixed(Positive::pt(72.0).unwrap()),
        Track::Ratio(Ratio::new(1.0).unwrap()),
        Track::Ratio(Ratio::new(2.0).unwrap()),
    ]),
    ..TableStyle::default()
}
```

---

## 表头重复

```rust
use pdfcraft_pdf::spec::TableHeader;

// 跨页后续页重画表头（默认）
TableHeader {
    repeat: true,
    rows: header_rows,
}

// 只在表开始画一次
TableHeader {
    repeat: false,
    rows: header_rows,
}
```

`repeat` 只对**非空** `rows` 有意义。空 `rows` 整段表头被当成不存在。

---

## 单元格：普通 Block

`CellSpec`：

| 字段 | 含义 |
| --- | --- |
| `style.fill` | 可选底色（在 `CellStyle`） |
| `children` | 自上而下的 `Block` |
| `col_span` / `row_span` | `NonZeroU16`，默认 `1` |

```rust
use std::num::NonZeroU16;
use pdfcraft_pdf::spec::{Block, CellSpec, CellStyle, RunStyle};
use pdfcraft_pdf::Color;

CellSpec {
    style: CellStyle {
        fill: Some(Color::rgb(240, 240, 240)),
    },
    children: vec![
        Block::Styled {
            style: RunStyle {
                bold: Some(true),
                ..RunStyle::default()
            },
            children: vec![Block::paragraph((), "小计")],
        },
        Block::paragraph((), "含税"),
    ],
    col_span: NonZeroU16::new(2).unwrap(),
    row_span: NonZeroU16::MIN,
}
```

空段落 `Block::paragraph(font, "")` 计 **1** 个块（0.6 行为变化）。
旧的空 `CellBlock::Text` 计 0。

### 格内允许 / 禁止

| 允许 | 禁止（`InvalidCellContent`，`"table cell"`） |
| --- | --- |
| `Paragraph`、`Spacer`、`Image`、`Canvas`、`HLine` | `Columns` |
| `Styled`（lower 时摊平，矩阵看不到它） | 嵌套 `Table` |
| `Inset` | `KeepTogether` |
| 合法 `List`（项内仍受格规则约束） | `PageBreak` |
| `Anchor`（目标块仍须是格内合法内容） | |

格内列表与「限制随外层继承」见 [lists.md](lists.md) 项内开放块一节。

---

## 合并单元格与跨页

- `col_span` / `row_span` 是描述性字段，不进 `CellStyle`。
- 一行（物理绘制行）不会从中间拆开。
- 若逻辑行因 `row_span` 绑在一起，分页缝不能切在尚未结束的合并块中间：
  放不下则整块去下一页；高于整页正文框则报错。

需要「标题行 + 数据行永不拆开」且不能用格内 `KeepTogether` 时，把整表放进外层
`Block::KeepTogether`，或调整分页策略与行内容高度。

---

## 外层 Styled 与格内继承

```rust
Block::Styled {
    style: RunStyle {
        bold: Some(true),
        ..RunStyle::default()
    },
    children: vec![Block::Table(table)],
}
```

0.6 起，格内段落继承外层 `Styled` 的**全部** `RunStyle` 字段（含粗体、斜体、
下划线、删除线、字距）。旧 `CellBlock::Text` 只继承字号和颜色。缺粗体面会
`BoldFaceRequired`。列表标记仍只要字号和颜色。

格内再包一层 `Styled`，只作用于它包住的那些段落。

更多段落层规则见 [paragraphs.md](paragraphs.md)。

---

## 常见翻车

| 现象 | 原因 | 做法 |
| --- | --- | --- |
| 找不到 `CellBlock` | 类型已删除 | `children: vec![Block::paragraph(...)]` |
| `InvalidCellContent` `"table cell"` | 格内放了栏 / 表 / keep / 分页 | 把复杂结构挪到格外 |
| 有 `Some(header)` 但看不见表头 | `rows` 为空 | 填行，或改成 `header: None` |
| 第二页没有表头 | `repeat: false` 或没有非空 header | `repeat: true` 且 `rows` 非空 |
| 表比页宽还宽直接失败 | 不静默缩放 | 改 `tracks` / 页边距 / 内容 |
| 单元格看起来没加粗 | 只改了某个 span，或缺粗体面 | 外层 `Styled` 或注册 bold 面 |

---

## 明确不做（请别设计成依赖）

- `CellBlock` 或单元格专用「短文本」节点
- 格内分栏、嵌套表、`KeepTogether`、`PageBreak`
- 按内容自动把超宽表缩进页宽
- 单元格级垂直对齐枚举、每格独立四边 padding（当前是表级 `cell_padding`）
- 自动生成 `1.1.1` 式表题编号

---

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [guide.md](guide.md) | 全库用法。Tables、Nesting、Limits |
| [paragraphs.md](paragraphs.md) | 段落、Span、样式叠放 |
| [lists.md](lists.md) | 列表；含「表格单元格里的列表」 |
| [migrate-0.6.md](migrate-0.6.md) | `0.5` 到 `0.6` 字段对照 |
| [migrate-table-row.md](migrate-table-row.md) | 更早的具名表行迁移史 |
| [crate CHANGELOG](../CHANGELOG.md) | `0.6.0` Breaking / Added / Changed |
| 提案书 §2.6、§12.7 | 已确认产品口径（仓库 `docs/提案书.md`） |
| `examples/bill.rs` / `hello.rs` | 表格与跨页样例 |
