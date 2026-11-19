# pdfcraft-pdf 审查提案讨论记录

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-19 起 |
| 状态 | **讨论完毕，已写入 [`提案书.md`](提案书.md) §15（2026-11-19）**。实现见 [`pdfcraft-pdf-review-impl-spec.md`](pdfcraft-pdf-review-impl-spec.md)。本文只作讨论记录，与提案书冲突时以提案书为准 |
| 来源 | 2026-11-19 `pdfcraft-pdf` 全包审查的提案议题 |
| 相关 | 缺陷修复另见 [`pdfcraft-pdf-bugfix-spec.md`](pdfcraft-pdf-bugfix-spec.md) |

审查提出 14 条提案议题。第 1 条（行尾空格规则）并入缺陷规格 B4 与 Q4。第 2 条（图片校验策略）并入 B6 与 Q6。本文记第 3 条起的讨论结果。

---

## P3 孤行、寡行与紧随下一块

**结论（2026-11-19）：两个都做。**

- `ParagraphStyle` 加孤行数与寡行数，默认都是 2。段落拆页时，页底至少留孤行数行，下一页顶至少有寡行数行。
- 段落总行数小于孤行数加寡行数时，这段不拆页。段落高于整页时约束放宽，不报错。
- `ParagraphStyle.keep_with_next: bool`，默认 `false`。为 `true` 时，这一段与下一块的开头放在同一页。「开头」是：段落的前孤行数行，表格的表头加第一行，其余块整块。连续多个 `keep_with_next` 段落串成一条链。
- 默认值 2 会改变现有文档的分页。新字段属于 breaking，随 `0.8.0` 发。

待写进提案书后再出实现规格。实现时要定的细节：字段类型（`NonZeroU8` 或带校验的新类型）、链条整体高于一页时的退让规则、`keep_with_next` 段落是文档最后一块时的行为。

## P4 `LinkTarget` 拆成入参与输出两个类型

**结论（2026-11-19）：拆，随 `0.8.0` 发。**

- 现状：`LinkTarget` 同时是调用方入参和录制输出，入参能放进只该由排版产生的 `Xyz`。误用时 lower 报 `ErrorKind::Backend { message }`，违反 ERR-7。
- 入参：`spec::LinkTarget { Uri, Anchor }`，与提案书 §12.4 一致。
- 输出：`canvas::LinkDest { Uri, Page(LinkXyz) }`，用在 `CanvasCommand::AnnotateLink` 与 `RecordedPage`。排版后不存在未解析的 `Anchor`。
- 删掉 `lower.rs` 里拒绝 `Xyz` 的分支。`BandSlot.link` 仍只收 URI。
- 两个枚举都标 `#[non_exhaustive]`。CHANGELOG 的 `Breaking` 写迁移：读录制输出的测试把 `LinkTarget::Xyz` 换成 `LinkDest::Page`。

## P5 `Length` 溢出与 `Errors` 非空

**结论（2026-11-19）：**

- `Length` 加法溢出：在 `f32::MAX` 处饱和，作为 B7 并入 [`pdfcraft-pdf-bugfix-spec.md`](pdfcraft-pdf-bugfix-spec.md)，随 `0.7.1` 发。根因只在加法与 `Sum`，`NaN` 都来自 `inf - inf`。
- `Errors`：内部改成 `{ first: Error, rest: Vec<Error> }`，去掉 `first()` 里的 `expect`。`len`、`is_empty`、`iter` 不变，`is_empty` 恒为 `false` 并在文档写明。不算 breaking，放进后续质量规格。
- 不做 `Length` 的 `Ord`。等饱和修复落地后另议。

## P6 不可见字符

**结论（2026-11-19）：分簇阶段统一规范化。**

| 字符 | 处理 |
| --- | --- |
| U+FEFF（BOM）、U+200C（ZWNJ）、U+200D（ZWJ） | 删掉。不做 shaping，连字控制符没有作用 |
| U+200B（零宽空格） | 零宽、不绘制的断行点 |
| U+2028、U+2029 | 硬换行，与 `\n` 相同 |
| U+FE00–U+FE0F、U+E0100–U+E01EF（变体选择符） | 与前一字合成一簇。字体 cmap14 有映射时用变体字形，没有时忽略选择符，只画基础字 |

- 这些字符本身不该画字形，不算「缺字」。可见字符缺字仍是 `MissingGlyph`，提案书 §4 不变。
- 变体字形用 skrifa `Charmap::map_variant` 查。测量与绘制用同一个 glyph id。
- 会改变含这些字符的文档：原来报错的现在能排出来。不改公共 API，但属于提案书 §4 的补充，要先写进提案书。
- 待实现规格定的细节：孤立的变体选择符（前面没有字，或前面是空格）怎么办；零宽空格前后的字距；`Span` 边界上的变体选择符归哪个 run。

## P7 CJK 禁则扩展

**结论（2026-11-19）：做以下两项，规则依据 W3C CLREQ 的行首禁则。**

- 半角 `,` `.` `;` `:` `!` `?` `)` `]` `}` `%` 前一簇是汉字（`Id`）时归 `CL`，否则保持现分类。`3.14`、`e.g.` 不受影响。与缺陷规格 Q2 的 `’` 共用「看前一簇」的分类机制。
- `·`（U+00B7）、`・`（U+30FB）、`～`（U+FF5E）一律归 `CL`。
- 不改：`……` 与 `——` 已经不拆，CLREQ 不禁止它们在行首。日文 `ー` 与小假名不在 v1 范围。
- 不改公共 API，会改变含这些字符的文档的断行。提案书 §3「小断行表」需补这几项。可随 `0.7.x` 或 `0.8.0` 发，实现排在缺陷规格 B2 之后。

## P8 lower 阶段的单元格错误补位置

**结论（2026-11-19）：单元格放置提前到 lower。**

- 现状：排版阶段的单元格错误已带 `Location.cell`（`table.rs:354`）。lower 阶段的不带，包括格内字体、图片、锚点、内容类型错误。提案书 §12.5 把它记为「另议」，原因是 `CellIndex` 是放置跨格后的网格坐标，lower 时还没算。
- 改法：网格放置只取决于 `TableSpec`，与字体、页面无关。lower 里放置一次，lower 每个单元格时用 `ctx.located` 填 `cell`。排版阶段复用这份放置结果，不再重算。
- 两个阶段报的 `CellIndex` 是同一套网格坐标。公共 API 不变。
- 提案书 §12.5 的这条「另议」改为已确认。块路径（第几个列表的第几项）仍是另议，不在本条。

## P9 `*Spec` 结构体的 `#[non_exhaustive]`

**结论（2026-11-19）：不加，在工程规范登记例外。只改文档。**

- 加了 `#[non_exhaustive]` 后，外部 crate 不能再用结构体字面量构造，`..Default::default()` 也不行。提案书 §12.7 确认的调用方式正是字面量构造。
- 这些类型已经符合 API-9 的例外类别，只是清单没点名：
  - 第 3 类（保留 `..Default::default()` 的 Options struct）：`SectionSpec`、`ParagraphSpec`、`TableSpec`、`CellSpec`、`BandSpec`、`BandSlot`、`BlankSpec`。
  - 第 5 类（调用方用字面量构造的协议记录）：`LayerMark`、`OutlineEntry`。
- 动作：在 `rust-engineering-standard.md` 的 API-9 与 Google disposition 延后项里补上这些类型。实施前用 `rg 'pub struct' crates/pdfcraft-pdf/src/spec` 列全清单，不要只靠上面的列举。
- 代价：给这些结构体加字段算 breaking，要升 `y` 位（P3 的孤行字段因此随 `0.8.0` 发）。

## P10 `to_pdf` 去掉 `PagePlan` 副本

**结论（2026-11-19）：不做。实测收益很小。**

2026-11-19 实测，release 构建，A4，文泉驿正黑 16 MiB，640 段中文正文共 139 页（接近默认全文字节上限 1 MB）：

| 阶段 | 峰值内存 VmHWM | 增量 |
| --- | --- | --- |
| 读入字体后 | 19.5 MB | |
| `layout_spec` 完成 | 40.1 MB | +20.6 MB |
| `to_pdf` 完成 | 42.4 MB | +2.3 MB |

- `to_pdf` 整体只让峰值多 2.3 MB，`PagePlan` 副本不超过这个数，约每页 16 KB。审查估计的「峰值减半」不成立。
- 全文字节上限最大 4 MB，文档规模有界，副本开销不会无限增长。
- 峰值主要在排版阶段：1 MB 正文用掉约 20 MB。要优化应先拆清这 20 MB 的去向，另立议题。
- 测量时不要调 `Rendered::pages()`，它返回整份录制的克隆，会抬高峰值。

## P11 表格边线与跨行高度

**结论（2026-11-19）：**

- **边线去重：修。** 现在每格各描一个矩形，相邻格的公共边描两次，半透明边框变深。改为收集去重后的横线段与竖线段，每段只描一次。跨行跨列合并区域内部不画线。不改公共 API，放进质量规格，随 `0.7.x` 发。
- **外框收进表格盒：修。** 现在外框以格边为中心，半个线宽落在表格盒外，在 gap 为 0 的栏里压到相邻栏。外框整条收进盒内，内线仍居中。现有输出的外框位置内移半个线宽。与上一项同一个 PR。
- **边线样式：做，随 `0.8.0` 发。** `TableStyle.border: TableBorder` 改为枚举：

  ```rust
  #[non_exhaustive]
  pub enum TableRules {
  	Grid(Pen),
  	ThreeLine { top: Pen, header: Pen, bottom: Pen },
  	None,
  }
  ```

  `ThreeLine` 的 `header` 线画在表头最后一行之下。没有表头时不画。表头跨页重复时，每页的顶线与表头线都重画，底线画在每页表格片段的底部。
- **跨行高度分配：维持。** 多出的高度全部加到所跨的最后一行。

待实现规格定的细节：`ThreeLine` 没有竖线时格内内容宽的「边框占位」怎么算（提案书 §2.6 的 `列宽 - 2*padding - 边框占位`）；`ThreeLine` 三条线的默认粗细（常见是顶底 1pt、表头 0.5pt）；`TableRules::None` 与 `Grid` 线宽 0 是否合并为一种写法。

## P12 段后距放不下时结束本页

**结论（2026-11-19）：不做，维持提案书现行为。** 本页剩余高度装不下段后距时丢掉间距，下一块放得下就接着排。提案书 §12 已写明这一行为，与 stack 一致。

## P13 `test-utils` feature

**结论（2026-11-19）：删掉，随 `0.8.0` 发。**

- 删除 `Cargo.toml` 的 `test-utils` feature 与 `lib.rs` 的根重导出。
- `canvas::pt_near` 与 `canvas::PT_ABS_TOLERANCE` 改为 `pub(crate)`，只供单元测试用。
- 集成测试（`tests/anchors.rs`、`document.rs`、`golden/snapshot.rs`、`inline_style.rs`、`letter_layout.rs`、`spec_parity.rs`）改用 `tests/` 下自带的辅助模块，同步改 `tests/README.md`。
- `.github/workflows/ci.yml` 里所有 `--features …test-utils` 一并去掉。
- CHANGELOG 写明是 breaking：下游如用过这两个符号，需自写容差比较。

## P14 Knuth-Plass 断行、自动断词、OpenType shaping

**结论（2026-11-19）：不重开。** 提案书 §3、§4 与 AGENTS.md「明确不要做」维持原样。

---

## 汇总

| 项 | 结论 | 去向 |
| --- | --- | --- |
| P1 行尾空格 | 并入缺陷规格 | B4 |
| P2 图片校验 | 并入缺陷规格 | B6 |
| P3 孤寡行与 keep_with_next | 做 | 提案书 + `0.8.0` |
| P4 链接目标拆型 | 做 | 提案书 + `0.8.0` |
| P5 Length 饱和 / Errors 内部形状 | 饱和并入 B7；Errors 改内部 | 缺陷规格 / 质量规格 |
| P6 不可见字符归一 | 做 | 提案书 §4 + 实现 |
| P7 中文行首禁则扩展 | 做 | 提案书 + 实现 |
| P8 单元格放置移到 lower | 做 | 提案书 §12.5 改为确认 |
| P9 API-9 例外登记 | 做（不加 non_exhaustive） | 工程规范 |
| P10 录制内存 | 不做 | — |
| P11 边线去重、外框内收 | 做 | 质量规格，`0.7.x` |
| P11 三线表 `TableRules` | 做 | 提案书 + `0.8.0` |
| P11 跨行高度 | 维持 | — |
| P12 段后距结束本页 | 不做 | — |
| P13 删 `test-utils` | 做 | `0.8.0` |
| P14 KP / 断词 / shaping | 不重开 | — |
