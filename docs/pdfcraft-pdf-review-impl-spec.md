# pdfcraft-pdf 审查补项实现规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-19 |
| 状态 | **可开工**。§12 的待决问题在对应 PR 开工前问用户，其余按本文 |
| 依据 | [`提案书.md`](提案书.md) §15（2026-11-19 确认）。讨论记录见 [`pdfcraft-pdf-review-proposals.md`](pdfcraft-pdf-review-proposals.md) |
| 前置 | [`pdfcraft-pdf-bugfix-spec.md`](pdfcraft-pdf-bugfix-spec.md) 的 PR 1 到 8 先合入并发 `0.7.1` |
| 范围 | `crates/pdfcraft-pdf`，加上工程规范 API-9 与 CI 配置。不改其余三包 |
| 冲突 | 与提案书冲突时以提案书为准 |

行号以 HEAD `05c3efd` 为准，缺陷修复合入后会漂移，开工时用 `rg` 重新定位。

---

## 1. 版本与 PR 总表

两个版本。`0.7.2` 只改行为与内部结构，不改公共签名。`0.8.0` 收齐全部 breaking。

| PR | 项 | 提案书 | 版本 | 依赖 |
| --- | --- | --- | --- | --- |
| R1 | API-9 例外登记（只改文档） | §15.8 | — | 无 |
| R2 | 中文行首禁则扩展 | §15.4 | 0.7.2 | 缺陷 B2（共用「看前一簇」分类） |
| R3 | 不可见字符规范化 | §15.3 | 0.7.2 | 缺陷 B4、B5 |
| R4 | lower 期单元格错误带 `CellIndex` | §15.5 | 0.7.2 | 无 |
| R5 | 表格边线去重与外框内收 | §15.6 前两条 | 0.7.2 | R4（复用放置结果） |
| R6 | 发 `0.7.2` | | 0.7.2 | R2 到 R5 |
| R7 | 删 `test-utils` | §15.7 | 0.8.0 | R6 |
| R8 | `LinkTarget` / `LinkDest` 拆型 | §15.2 | 0.8.0 | R6 |
| R9 | `TableRules` 与三线表 | §15.6 第三条 | 0.8.0 | R5 |
| R10 | 孤行、寡行与 `keep_with_next` | §15.1 | 0.8.0 | R6 |
| R11 | `Errors` 内部改非空 | 讨论记录 P5 | 见 Q1 | 无 |
| R12 | 发 `0.8.0` | | 0.8.0 | R7 到 R11 |

每个 PR：带本文对应小节的回归测试；`CHANGELOG.md` 写进 `Fixed` / `Changed` / `Breaking`；提交前跑工程规范 TOOL-5 完整门禁。不擅自 `git commit` / push。

---

## 2. R1 API-9 例外登记

- 用 `rg -n 'pub struct' crates/pdfcraft-pdf/src/spec` 列出全部公共结构体，逐个归入 API-9 五类例外，或确认已标 `#[non_exhaustive]`。
- 已知归类：第 3 类（保留 `..Default::default()` 的 Options struct）`SectionSpec`、`ParagraphSpec`、`TableSpec`、`CellSpec`、`BandSpec`、`BandSlot`、`BlankSpec`；第 5 类（字面量构造的协议记录）`LayerMark`、`OutlineEntry`。清单以 `rg` 结果为准。
- 改 `docs/rust-engineering-standard.md` 的 API-9 行与 `docs/google-rust-style-disposition.md` 的延后项。
- 不改代码。

## 3. R2 中文行首禁则扩展

**分类。** 缺陷 B2 已让 `classify` 能拿到前一簇的类别。本 PR 复用它：

| 字符 | 前一簇是 `Id` | 否则 |
| --- | --- | --- |
| `,` `.` `;` `:` `!` `?` `)` `]` `}` `%` | `CL` | 现分类不变 |
| `·` U+00B7、`・` U+30FB、`～` U+FF5E | `CL` | `CL` |

「前一簇」是同一段落里紧邻的前一簇，跨 `Span` 边界也算。段首没有前一簇，按「否则」。

**不改。** `……`、`——` 现已不拆。`3.14`、`e.g.`、`a,b` 的分类不变。

**测试**（`break_class.rs` 与 `line_break.rs` 单元测试）：

- `汉字,` 在窄框里 `,` 不落行首；`abc,` 的分类与改前相同。
- `3.14` 与 `e.g.` 的断行点与改前相同。
- `·`、`・`、`～` 在任何前一簇后都不落行首。
- 半角 `)` 跟在汉字后不落行首，跟在拉丁字母后保持原行为。

## 4. R3 不可见字符规范化

在 `text/cluster.rs` 的规范化阶段（`normalize_tagged`）处理，测量与绘制看到同一份结果。

| 字符 | 处理 |
| --- | --- |
| U+FEFF、U+200C、U+200D | 从字形文本里删掉，不产生簇 |
| U+200B | 产生一个零宽、不绘制的簇，类别允许其前后断行 |
| U+2028、U+2029 | 归 `BK`，与 `\n` 相同 |
| U+FE00–U+FE0F、U+E0100–U+E01EF | 并入前一簇。绘制该簇基础字时用 skrifa `Charmap::map_variant(base, selector)`；返回 `Variant` 时用变体 glyph id，返回 `UseDefault` 或无映射时用基础字的 glyph id |

- 这些字符不调 charmap 查字，不会报 `MissingGlyph`。
- 字距（§12.1）只加在相邻两个可见簇之间。删掉的字符不占位置；零宽空格簇两侧不加字距。
- `Span::split_latin` 不变，它按原字符串切分，规范化发生在其后。
- 细节见 Q2。

**测试：**

- `"A\u{200D}B"` 与 `"AB"` 的 `DrawGlyphRun` 字形序列相同。
- 含 U+FEFF 的段落不再报 `MissingGlyph`。
- `"长长\u{200B}长长"` 在窄框里于 U+200B 处断行，无可见字形。
- `"A\u{2028}B"` 与 `"A\nB"` 行数相同。
- 有 cmap14 的夹具字体（环境变量未设则跳过）：变体选择符取到变体 glyph id；无映射的选择符只画基础字。
- 无 cmap14 的字体：`"葛\u{E0100}"` 画出与 `"葛"` 相同的 glyph id。

## 5. R4 lower 期单元格错误带 `CellIndex`

- 把网格放置（跨格占位、`CellIndex` 计算）从 `layout/table.rs` 的 `TableNode::new` 抽成只依赖 `TableSpec` 形状的函数，lower 时调用一次。
- lower 每个单元格时，用 `ctx.located` 把 `Location.cell` 填成该格的 `CellIndex`。
- 放置结果随降级后的表节点交给排版，排版不再重算。两阶段的 `CellIndex` 来自同一次放置。
- 放置本身的错误（跨格越界、重叠）报错位置不变。

**测试**（`spec/tests.rs`）：

- 单元格里的段落用未注册字体键：错误的 `at.cell` 等于该格 `CellIndex`。
- 前面有 `row_span = 2` 的格时，后续格的 `CellIndex` 与排版阶段错误报的一致。
- 单元格里放 columns：`InvalidCellContent` 带 `cell`。

## 6. R5 表格边线去重与外框内收

**去重。** `paint` 不再每格描一个矩形。按放置结果收集线段：

- 横线：每条行边界上，按列切成段，被跨行格覆盖的段不画。竖线同理。
- 共线、相邻、同笔的段合并成一条路径再描，每段只描一次。
- 跨页片段：每个片段的顶边与底边都画。

**外框内收。** 线宽 `w` 时：

- 列轨道在 `W - w` 上解析，网格整体向右、向下偏移 `w/2`。外框整条落在 `[0, W]` 内。
- 表格高度多 `w`（顶部与底部各 `w/2`）。
- 单元格内容内缩仍是 `w/2 + padding`，所有格对称。

`w = 0` 时与现状相同。会改变带边框表格的列宽与高度，`CHANGELOG` 的 `Changed` 写明。

**测试**（`RecordingCanvas`）：

- 2×2 网格：描边路径里每条内边界只出现一次。
- `col_span = 2` 的格：其内部没有竖线段。
- 外框左缘 `x - w/2` 等于表格框左缘，右缘 `x + w/2` 等于框右缘。
- 在 gap 为 0 的 columns 里，相邻两表的外框不重叠。

## 7. R7 删 `test-utils`

- 删 `crates/pdfcraft-pdf/Cargo.toml` 的 `test-utils` feature 与 `lib.rs` 的根重导出。
- `canvas::pt_near`、`canvas::PT_ABS_TOLERANCE` 改为 `pub(crate)`，删掉 rustdoc 里「Also at the crate root」的句子。
- 集成测试新建 `tests/common/mod.rs`（或已有共享模块）放容差比较。改 `tests/anchors.rs`、`document.rs`、`golden/snapshot.rs`、`inline_style.rs`、`letter_layout.rs`、`spec_parity.rs` 与 `tests/README.md`。
- `.github/workflows/ci.yml` 里全部 `test-utils` 去掉；工程规范 TOOL-5 的门禁命令如带此 feature 一并改。
- `Breaking`：下游如用过这两个符号，自写容差比较。

## 8. R8 `LinkTarget` / `LinkDest` 拆型

- `spec::LinkTarget { Uri(String), Anchor(String) }`，`#[non_exhaustive]`。从 `canvas` 移到 `spec`，`lib.rs` 的重导出跟着改。
- `canvas::LinkDest { Uri(String), Page(LinkXyz) }`，`#[non_exhaustive]`。`CanvasCommand::AnnotateLink` 与录制页用它。`LinkXyz` 不变。
- 排版把 `Anchor` 解析成 `Page`，把 `Uri` 原样转过去。删掉 `lower.rs` 里拒绝 `Xyz` 的分支及其 `ErrorKind::Backend` 文案。
- 内部 `layout/paragraph.rs`、`line_run.rs`、`frame.rs`、`anchor.rs` 在锚点解析前用内部类型持有「未解析目标」，解析后才产出 `LinkDest`。不要让 `LinkDest` 出现 `Anchor`。
- `backend/` 只认 `LinkDest`。
- `Breaking` 迁移：读录制输出的代码把 `LinkTarget::Xyz` 换成 `LinkDest::Page`，`LinkTarget::Uri` 换成 `LinkDest::Uri`。

**测试：** 锚点链接录制成 `LinkDest::Page`，页码与点同改前；URI 链接录制成 `LinkDest::Uri`；`to_pdf` 冒烟。

## 9. R9 `TableRules` 与三线表

**形状。**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub enum TableRules {
	Grid(Pen),
	ThreeLine { top: Pen, header: Pen, bottom: Pen },
	#[default]
	None,
}
```

`TableStyle.border: TableBorder` 改为 `TableStyle.rules: TableRules`（字段名见 Q3），删 `TableBorder`。默认 `None` 与现在线宽 0 的默认一致。不加便捷构造函数。

**`Grid(pen)`** 即 R5 的行为，线宽取 `pen.width()`。

**`ThreeLine`：**

- 不画竖线。格内水平内缩只有 `padding`。
- 三条线各占自己的高度，不压在格上：顶线在片段顶部占 `top` 宽；表头线在表头最后一行之下占 `header` 宽；底线在片段底部占 `bottom` 宽。行与行之间没有线。
- 没有表头时不画表头线。
- 跨页：每个片段都有顶线与底线。表头重复时每页都有表头线；`TableHeader.repeat = false` 的续页没有表头线。
- 分页时，片段可用高度先扣去顶线与底线。

**测试：**

- `None`：没有描边命令。
- `ThreeLine` 有表头：恰好三条横线，y 坐标分别在顶部、表头之下、底部；没有竖线。
- `ThreeLine` 跨两页并重复表头：每页三条线。
- `ThreeLine` 无表头：每页两条线。

## 10. R10 孤行、寡行与 `keep_with_next`

**字段。** `ParagraphStyle` 加：

- `orphans: NonZeroU8`，默认 2。页底至少留这么多行。
- `widows: NonZeroU8`，默认 2。下一页顶至少有这么多行。
- `keep_with_next: bool`，默认 `false`。

字段类型见 Q4。值 1 等于不约束。

**段落拆页**（`layout/paragraph.rs` 的 `split`）：

- 求本页能放下的行数 `n`。剩余行数 `rest = total - n`。
- `rest > 0` 且 `rest < widows` 时，`n` 减到 `total - widows`。
- 结果 `n < orphans` 时，整段推到下一页（本页不放）。
- 段落总行数小于 `orphans + widows` 时不拆，整段推到下一页。
- 在空页（本页还没放任何正文）上上述规则导致放不下时，规则放宽：先放弃寡行，再放弃孤行，按原逻辑拆。不报错。

**`keep_with_next`**（`layout/paginate.rs` 与 `flow.rs`）：

- 连续的 `keep_with_next` 段落与其后的第一块组成一条链。链的「开头」：段落取前 `orphans` 行；表格取表头加第一行（无表头取第一行）；其余块取整块。
- 链在本页放不下时整条推到下一页。
- 在空页上链仍放不下时，忽略整条链的 `keep_with_next`，照常排。
- `keep_with_next` 段落是本流最后一块（后面没有块）时，标志无效。
- 后一块是 `PageBreak` 时，标志无效。
- 适用范围：正文流、栏、列表项、单元格内的流。表头行内不适用。

**测试：**

- 默认值下，五行段落在页底只剩一行空间时整段推到下一页。
- 五行段落本页能放四行：本页放三行，下页两行。
- `orphans = 1`、`widows = 1` 与改前分页相同。
- 标题段 `keep_with_next = true`，其后段落放不下首两行：标题跟着推到下一页。
- 三个 `keep_with_next` 段落成链，后接表格：链与表头加首行同页。
- 链高于整页：照常排，不报错，不死循环。

## 11. R11 `Errors` 内部改非空

- 内部改为 `first: Error` 与 `rest: Vec<Error>`，删掉 `first()` 里的 `expect` 与 `#[expect(clippy::expect_used)]`。
- `len`、`is_empty`、`truncated`、`first` 行为不变。`is_empty` 恒为 `false`，rustdoc 已写明。
- `as_slice` 与 `iter` 的处理见 Q1。

---

## 12. 待决问题

开工到对应 PR 前问用户。

**Q1（R11）`as_slice` 怎么办。** `Errors::as_slice()` 返回 `&[Error]`，要求连续存储。改成 `{ first, rest }` 后无法返回切片，讨论记录里「公共方法不变、不算 breaking」不成立。

| 方案 | 做法 | 代价 |
| --- | --- | --- |
| A（建议） | 删 `as_slice`；`iter` 返回 `impl ExactSizeIterator<Item = &Error>`。R11 改进 `0.8.0` | breaking，两处签名 |
| B | 内部保留 `Vec`，`first()` 用 `match` 加 `unreachable!` | 仍是 panic 路径，违背 PAN 规则的初衷 |
| C | 不做 R11 | `expect` 保留 |

**Q2（R3）不可见字符的边界情况。**

- 孤立的变体选择符（段首、跟在空格或换行后、或位于 `Span` 开头）：建议直接删掉，不报错。
- `Span` 边界上的变体选择符：前一簇在别的 run 时，建议按孤立处理并删掉，不跨 run 合簇。

**Q3（R9）字段名。** 建议 `TableStyle.border` 改名为 `rules`，因为三线表不是「边框」。也可保留 `border` 只换类型。

**Q4（R10）孤寡行字段类型。** 建议 `NonZeroU8`，1 表示不约束。也可用 `u8` 并把 0 与 1 都视为不约束，或用带校验的新类型。

---

## 13. 不在本文

- 审查报告里的其余质量项（静默兜底、死代码、诊断措辞、性能）另立规格。
- 提案书 §15.9 列明的不做项。
