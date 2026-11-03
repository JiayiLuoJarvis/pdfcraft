# 列表用法指南（调用方）

当前 `pdfcraft-pdf` 版本是 `0.5.0`。本文只讲正文流里的 `Block::List` /
`ListSpec`，以及和列表常一起用的 `Block::Inset`。完整字段与错误面见
[guide.md](guide.md) 与 [docs.rs](https://docs.rs/pdfcraft-pdf)。

目标：把一章条款写成**一个** `ListSpec` + 多个 `ListItemSpec`，让库自增序号，
而不是「每项一个只有一项的列表再手写 `start`」。

下文代码里字体键用 `()`，和 guide 一样。正式项目里换成你的 `K`。

---

## 强烈建议

| 做 | 不要 |
| --- | --- |
| 同一章用**一个**有序列表，项放进 `items` | 每条条款单独一个 `ListSpec` 再改 `start` |
| 项间距用 `item_gap` | 用项首 `Spacer`「顶」出项间空白（会把序号钉在 spacer 顶上） |
| 多位数序号用 `MarkerAlign::End`，并留够 `marker_width` | 靠猜宽度；过宽会整份失败，库不裁剪 |
| 中文「一、」「（一）」用每项 `marker: Some(...)` | 指望库自动出中文数字 |
| 地址、备注整体右缩用项内 `Inset` | 给每行段落手写不同缩进 |
| 整表相对正文右缩用外层 `Inset` 包住 `List` | 改 `body_indent` 当「整表外边距」（那只动正文列，不动序号列） |
| 多级编号各自嵌套一层 `List` | 指望自动 `1.1.1` |
| 结构体字面量写 `..ListSpec::default()` | 漏写 `item_gap` / `marker_*` / `body_indent` |

---

## 心智模型

```text
列表左缘
│
├─ 标记列（宽 = marker_width）── 只在项的首页片段画一次
│     1. / • / 自定义串
│
└─ 正文列（左缘 = body_indent，默认 = marker_width）
      所有换行都从这里起，不伸进标记列
```

- 项可跨页。续页**不**重画标记，但仍保持正文悬挂缩进。
- 标记基线对齐该项**第一个有效块**的首条文字基线（见下文「序号对齐」）。
- `ListKind::Bullet` 默认圆点是 `•`（U+2022）。`marker_format` / `start` 对 ul 无效。

---

## 最小有序列表

```rust
use std::num::NonZeroU32;
use pdfcraft_pdf::spec::{Block, ListItemSpec, ListKind, ListSpec, MarkerFormat};
use pdfcraft_pdf::{NonNegative, Numbering, Positive};

fn article_list() -> ListSpec<()> {
    ListSpec {
        kind: ListKind::Ordered(Numbering::Decimal),
        start: NonZeroU32::new(1).unwrap(),
        marker_width: Positive::pt(24.0).unwrap(),
        body_indent: None, // 等于 marker_width
        item_gap: NonNegative::pt(8.0).unwrap(),
        marker_format: MarkerFormat::IdeographicComma, // 1、2、3、
        marker_font: None,
        marker_align: Default::default(), // Start
        items: vec![
            ListItemSpec {
                marker: None,
                blocks: vec![Block::paragraph((), "当事人应当书面通知对方。")],
            },
            ListItemSpec {
                marker: None,
                blocks: vec![Block::paragraph((), "通知自送达之日起生效。")],
            },
            ListItemSpec {
                marker: None,
                blocks: vec![Block::paragraph((), "双方另有约定的除外。")],
            },
        ],
    }
}
```

无序列表把 `kind` 换成 `ListKind::Bullet` 即可，其余字段仍建议写全或用
`..ListSpec::default()`：

```rust
ListSpec {
    kind: ListKind::Bullet,
    marker_width: Positive::pt(16.0).unwrap(),
    item_gap: NonNegative::pt(4.0).unwrap(),
    items: vec![
        ListItemSpec {
            marker: None,
            blocks: vec![Block::paragraph((), "measure with skrifa")],
        },
        ListItemSpec {
            marker: None,
            blocks: vec![Block::paragraph((), "break lines in the frame")],
        },
    ],
    ..ListSpec::default()
}
```

---

## 编号样式与后缀

`Numbering` × `MarkerFormat` 都有效。字母到 Z（1–26），罗马数字到 3999；
超出报 `GeometryKind::MarkerOutOfRange`。

| `Numbering` | 例子主体 |
| --- | --- |
| `Decimal` | `1` `12` |
| `UpperAlpha` / `LowerAlpha` | `A` `z` |
| `UpperRoman` / `LowerRoman` | `IV` `iv` |

| `MarkerFormat` | 十进制示例 |
| --- | --- |
| `DotSuffix`（默认） | `1.` |
| `IdeographicComma` | `1、`（U+3001） |
| `ClosingParen` | `1)` |
| `Parens` | `(1)` |
| `Bare` | `1` |

```rust
use pdfcraft_pdf::spec::MarkerFormat;
use pdfcraft_pdf::Numbering;

// 1)  2)  3)
ListSpec {
    kind: ListKind::Ordered(Numbering::Decimal),
    marker_format: MarkerFormat::ClosingParen,
    marker_width: Positive::pt(22.0).unwrap(),
    items: /* ... */,
    ..ListSpec::default()
}

// a)  b)  c)
ListSpec {
    kind: ListKind::Ordered(Numbering::LowerAlpha),
    marker_format: MarkerFormat::ClosingParen,
    start: NonZeroU32::new(1).unwrap(),
    marker_width: Positive::pt(22.0).unwrap(),
    items: /* ... */,
    ..ListSpec::default()
}

// (iv)  (v)  …
ListSpec {
    kind: ListKind::Ordered(Numbering::LowerRoman),
    marker_format: MarkerFormat::Parens,
    start: NonZeroU32::new(4).unwrap(),
    marker_width: Positive::pt(36.0).unwrap(), // 罗马数字可能很长
    items: /* ... */,
    ..ListSpec::default()
}
```

从中间续号只改 `start`，不要拆成多个列表：

```rust
// 接上一章的第 4 条起
ListSpec {
    kind: ListKind::Ordered(Numbering::Decimal),
    start: NonZeroU32::new(4).unwrap(),
    marker_format: MarkerFormat::DotSuffix,
    items: vec![
        ListItemSpec {
            marker: None,
            blocks: vec![Block::paragraph((), "续写条款")],
        },
    ],
    ..ListSpec::default()
}
```

---

## 多位数：右对齐标记列

`9.` 与 `10.` 要让小数点齐，用 `MarkerAlign::End`，并按**最宽最终串**留
`marker_width`（含后缀/括号）：

```rust
use pdfcraft_pdf::spec::MarkerAlign;

ListSpec {
    kind: ListKind::Ordered(Numbering::Decimal),
    marker_width: Positive::pt(28.0).unwrap(), // 够放 "10."
    marker_align: MarkerAlign::End,
    marker_format: MarkerFormat::DotSuffix,
    items: (1..=12)
        .map(|n| ListItemSpec {
            marker: None,
            blocks: vec![Block::paragraph((), format!("条款 {n}"))],
        })
        .collect(),
    ..ListSpec::default()
}
```

标记比 `marker_width` 宽 → 整份失败，不会裁切或换行。

---

## 自定义标记（中文序号、旁注）

`ListItemSpec.marker = Some(...)` 时：

- 画你给的字符串，不套 `marker_format`
- **不占用**自动计数（下一项继续按剩余自动项编号）
- 仍受 `marker_width` / `marker_font` 约束

```rust
ListSpec {
    kind: ListKind::Ordered(Numbering::Decimal), // kind 仍要；自定义项不读它的数字
    marker_width: Positive::pt(32.0).unwrap(),
    items: vec![
        ListItemSpec {
            marker: Some("一、".into()),
            blocks: vec![Block::paragraph((), "总则")],
        },
        ListItemSpec {
            marker: Some("（一）".into()),
            blocks: vec![Block::paragraph((), "适用范围")],
        },
        ListItemSpec {
            marker: Some("（二）".into()),
            blocks: vec![Block::paragraph((), "定义")],
        },
        ListItemSpec {
            marker: None, // 若混用自动号，这里会是 1.
            blocks: vec![Block::paragraph((), "一般不要与中文序号混在同一列表")],
        },
    ],
    ..ListSpec::default()
}
```

**建议：** 需要「一、二、三」时，该列表每一项都写自定义 `marker`，不要和自动
`1. 2. 3.` 混排。旁注、星号例外项可以夹在自动编号中间：

```rust
items: vec![
    ListItemSpec {
        marker: None,
        blocks: vec![Block::paragraph((), "正式条款")],
    },
    ListItemSpec {
        marker: Some("*".into()), // 不占号
        blocks: vec![Block::paragraph((), "编辑备注，不进正式序号")],
    },
    ListItemSpec {
        marker: None, // 仍是下一条自动号
        blocks: vec![Block::paragraph((), "下一条正式条款")],
    },
],
```

全角括号 `（1）`、任意前后缀字符串：同样走自定义 `marker`，库不提供格式变体。

---

## 项间距：`item_gap` vs spacer

```rust
ListSpec {
    item_gap: NonNegative::pt(10.0).unwrap(),
    // …
    ..ListSpec::default()
}
```

规则：

- 只加在**相邻两项之间**；首项前、末项后不加
- 与上一项末块自带的尾随间距**相加**，不取较大值
- 本页剩余高度放不下就**丢掉**，不带到下一页顶
- 项内部块与块之间的空隙，仍在 `blocks` 里放 `Block::Spacer`

**错误示范（序号会钉在空白顶上）：**

```rust
// 不要用项首 Spacer 制造「项间空白」
ListItemSpec {
    marker: None,
    blocks: vec![
        Block::Spacer(NonNegative::pt(12.0).unwrap()),
        Block::paragraph((), "正文"),
    ],
}
```

项前要留白：把 `Spacer` 放在**列表之前**的正文流里。项与项之间：用
`item_gap`。

---

## 序号对齐（基线）

序号对齐该项**第一个有效块**最靠上的文字基线：

| 第一个有效块 | 对齐到 |
| --- | --- |
| 段落 | 首行基线 |
| 表 | 本片段第一条物理行（有表头就是表头行） |
| `Columns` | 各栏里最靠上的那一行 |
| 嵌套列表 | 内层第一个标记 |
| `Inset` | 孩子的首条基线（左右平移不改基线高度） |
| `Styled` | 透明，看里面的第一个孩子 |
| 图片 / 矢量块 / 分隔线 / 高度大于 0 的 Spacer | **无文字基线** → 标记行盒顶对齐项顶 |

空 stack、空 keep-together、空 `Inset`、高度 0 的 spacer 会跳过。

```rust
// 推荐：第一块就是要和序号对齐的段落
ListItemSpec {
    marker: None,
    blocks: vec![
        Block::paragraph((), "标题行"),
        Block::Spacer(NonNegative::pt(4.0).unwrap()),
        Block::paragraph((), "说明段落"),
    ],
}

// 项里先放图再写字：序号在图顶，不会落到图下段落
ListItemSpec {
    marker: None,
    blocks: vec![
        Block::Image(/* … */),
        Block::paragraph((), "图注"),
    ],
}
```

---

## 嵌套列表与 `body_indent`

每一层是独立的 `ListSpec`。库**不会**生成 `1.1.1`。外层项的 `blocks` 里再
放一个 `Block::List(...)`。

```rust
ListSpec {
    kind: ListKind::Ordered(Numbering::Decimal),
    marker_width: Positive::pt(20.0).unwrap(),
    body_indent: None,
    item_gap: NonNegative::pt(6.0).unwrap(),
    items: vec![ListItemSpec {
        marker: None,
        blocks: vec![
            Block::paragraph((), "外层条款"),
            Block::List(ListSpec {
                kind: ListKind::Ordered(Numbering::LowerAlpha),
                marker_format: MarkerFormat::ClosingParen,
                marker_width: Positive::pt(18.0).unwrap(),
                item_gap: NonNegative::pt(2.0).unwrap(),
                items: vec![
                    ListItemSpec {
                        marker: None,
                        blocks: vec![Block::paragraph((), "分项 a")],
                    },
                    ListItemSpec {
                        marker: None,
                        blocks: vec![Block::paragraph((), "分项 b")],
                    },
                ],
                ..ListSpec::default()
            }),
        ],
    }],
    ..ListSpec::default()
}
```

`body_indent`：本层正文列左缘距**本层列表左缘**的距离。

- `None` → 等于 `marker_width`（常见悬挂缩进）
- `Some(x)` → 必须 `x >= marker_width`，否则 `ListIndent`
- 它**不是**整表相对页面的外边距

若要标记列与正文列之间多留一点缝：

```rust
ListSpec {
    marker_width: Positive::pt(18.0).unwrap(),
    body_indent: Some(Positive::pt(24.0).unwrap()), // 正文比标记列右缘再右 6pt
    // …
    ..ListSpec::default()
}
```

---

## `Inset`：整表缩进与项内地址块

`Inset` 画什么都没有，只把孩子右移 `left`，并把排版宽度减掉 `left + right`。
续页保持同样缩进。`left + right >=` 当前框宽 → `InsetWiderThanFrame`。

### 整表相对周围正文右缩

```rust
Block::Inset {
    left: NonNegative::pt(24.0).unwrap(),
    right: NonNegative::ZERO,
    children: vec![Block::List(article_list())],
}
```

序号和正文一起右移。这和改 `body_indent` 不同：后者只动正文列。

### 项内多行地址 / 引用块整体缩进

```rust
ListItemSpec {
    marker: None,
    blocks: vec![
        Block::paragraph((), "收件信息"),
        Block::Inset {
            left: NonNegative::pt(12.0).unwrap(),
            right: NonNegative::ZERO,
            children: vec![
                Block::paragraph((), "Acme Corp"),
                Block::paragraph((), "1 Market Street"),
                Block::paragraph((), "San Francisco, CA"),
            ],
        },
    ],
}
```

序号仍对齐「收件信息」那一行；地址块整体右移，不影响标记。

单元格里用 `CellBlock::Inset`，规则相同，孩子必须是 `CellBlock`。

---

## 项内富内容

项内除 `PageBreak` 外都可以放：段落、线、spacer、stack、`Styled`、anchor、
`keep_together`、columns、table、嵌套列表、image、矢量块、`Inset`。

```rust
use pdfcraft_pdf::spec::{CellBlock, CellSpec, TableBorder, TableRow, TableSpec};
use pdfcraft_pdf::{Color, NonNegative};

ListItemSpec {
    marker: None,
    blocks: vec![
        Block::paragraph((), "费用明细如下："),
        Block::Table(TableSpec {
            header: vec![],
            body: vec![TableRow {
                cells: vec![
                    CellSpec {
                        content: vec![CellBlock::Text {
                            text: "咨询费".into(),
                            font: (),
                        }],
                        ..CellSpec::default()
                    },
                    CellSpec {
                        content: vec![CellBlock::Text {
                            text: "10,000".into(),
                            font: (),
                        }],
                        ..CellSpec::default()
                    },
                ],
            }],
            repeat_header: false,
            tracks: None,
            cell_padding: NonNegative::pt(3.0).unwrap(),
            border: TableBorder {
                width: NonNegative::pt(0.5).unwrap(),
                color: Color::rgb(0, 0, 0),
            },
        }),
        Block::KeepTogether(vec![
            Block::paragraph((), "以下两段须同页："),
            Block::paragraph((), "签署栏说明……"),
        ]),
    ],
}
```

限制**随外层容器继承**：列表在表格单元格里时，项内仍不能放单元格禁止的块
（嵌套表、columns、`keep_together`）。报错时 `InvalidCellContent` 的
`container` 是最内层禁止者：单元格里的列表项放 columns → `"table cell"`；
放 `PageBreak` → `"list item"`。

---

## 序号字体与样式

```rust
ListSpec {
    marker_font: Some(()), // 换成你的字体键；None = 注册成功的第一个面
    // …
    ..ListSpec::default()
}
```

- 圆点、自动编号、自定义 `marker` 都走 `marker_font`
- **只用常规面**；`Block::Styled` 的粗体/斜体不换序号字面
- 字号与颜色仍继承外层 `Styled`
- 未注册键 → `UnregisteredFont`；缺字 → 整份失败（与正文同一策略）

```rust
Block::Styled {
    style: RunStyle {
        font_size: Some(Positive::pt(11.0).unwrap()),
        color: Some(Color::rgb(32, 32, 32)),
        ..RunStyle::default()
    },
    children: vec![Block::List(/* 序号字号/颜色跟这里走 */)],
}
```

---

## 组装数据时的推荐写法

用 Rust 循环推项，不要手写重复结构：

```rust
fn clauses(lines: &[&str]) -> Block<()> {
    Block::List(ListSpec {
        kind: ListKind::Ordered(Numbering::Decimal),
        marker_format: MarkerFormat::IdeographicComma,
        marker_width: Positive::pt(24.0).unwrap(),
        marker_align: MarkerAlign::End,
        item_gap: NonNegative::pt(8.0).unwrap(),
        items: lines
            .iter()
            .map(|line| ListItemSpec {
                marker: None,
                blocks: vec![Block::paragraph((), (*line).to_string())],
            })
            .collect(),
        ..ListSpec::default()
    })
}
```

中文序号同理，zip 标记与正文：

```rust
fn cn_items(pairs: &[(&str, &str)]) -> Vec<ListItemSpec<()>> {
    pairs
        .iter()
        .map(|(mark, text)| ListItemSpec {
            marker: Some((*mark).into()),
            blocks: vec![Block::paragraph((), (*text).to_string())],
        })
        .collect()
}
```

---

## 常见失败与对策

| 现象 / 错误 | 常见原因 | 对策 |
| --- | --- | --- |
| 编译缺字段 | `ListSpec` 新增了公开字段 | `..ListSpec::default()` 或显式写出 |
| `ListIndent` | `body_indent < marker_width` | 增大 `body_indent` 或改回 `None` |
| 标记过宽 | `10.`、`MMMCMXCIX.`、`（一）` 超过列宽 | 加大 `marker_width`；长罗马慎用 |
| `MarkerOutOfRange` | 字母超过 26 或罗马超过 3999 | 换 `Decimal`，或拆章，或改自定义标记 |
| `InsetWiderThanFrame` | `left + right` 吃光当前框 | 减小缩进；窄栏里尤其要查 |
| `InvalidCellContent` | 格内列表项放了表/栏/`keep_together`/`PageBreak` | 把复杂块挪到格外，或改用正文列表 |
| `LimitExceeded` + `NestingDepth` | 总嵌套超过默认 16 | 扁平化结构；必要时 `set_max_nesting_depth`（未优化构建可能要更大线程栈） |
| 序号和正文错位 | 项首是 Spacer/图 | 第一块放段落；空白用列表前 Spacer 或 `item_gap` |
| 续页顶上多出空白 | 指望跨页带上 `item_gap` | 设计上就会丢掉；不要依赖页顶空隙 |

---

## 明确不做（请别设计成依赖）

- 项内 `PageBreak`
- 项级重设序号（HTML `<li value>`）
- 倒序编号
- 多级自动 `1.1.1`
- 中文数字自动编号
- 单项独立的 `item_gap`
- 页眉页脚里的列表（`BandSpec` 不收 `Block`）

需要多级大纲观感时：**嵌套多个列表**，每层自己的 `Numbering` /
`marker_format` / 自定义 `marker`。

---

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [guide.md](guide.md) | 全库用法；Lists / Nesting / Limits 节 |
| [crate CHANGELOG](../CHANGELOG.md) | `0.5.0` Breaking / Added / Changed 与迁移 |
| 提案书 §2.8、§12.5、§12.6 | 已确认产品口径（仓库 `docs/提案书.md`） |
| `examples/hello.rs` | 嵌套 ul、自定义标记、项内 `Inset` 地址块 |
