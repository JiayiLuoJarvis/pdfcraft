# pdfcraft-pdf §12.5 列表补项与 §12.6 `Inset` 实现规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-03 |
| 状态 | **产品条已定稿**（提案书 §12.5、§12.6，2026-11-03 三轮逐条确认）。本文把它们落成可执行细则。开工前须确认项已清零（§12） |
| 范围 | 只改 `pdfcraft-pdf`。forms / orgchart / post 不动 |
| 版本 | 五个 PR 合起来发 `pdfcraft-pdf 0.5.0`（当前 `0.4.0`） |
| 相关 | [`提案书.md`](提案书.md) §2.4、§2.6、§2.8、§2.9、§12.5、§12.6；[`rust-engineering-standard.md`](rust-engineering-standard.md) VER-1 至 VER-4、TOOL-5；[`module-dependency.md`](module-dependency.md)；决策日志 `.audit/list-capabilities.tsv` |
| 冲突 | 与提案书冲突时以提案书为准。实现中发现与代码事实冲突，先停下来改提案书或本文，不要自行裁决 |

本文是实现合同，不重开产品讨论。行号以 2026-11-03 工作区（HEAD `4dbbaa4`）为准，动手前重新核对。

## 0. 工作区初始状态（新会话先处理）

上一会话在用户确认前抢跑，留下了 PR (1) 的半成品，**crate 编译不过**：

- `crates/pdfcraft-pdf/src/spec/mod.rs`：公开 `ListSpec` 加了 `item_gap` 字段、rustdoc、默认值（+7 行）。
- `crates/pdfcraft-pdf/src/layout/list.rs`：内部 `ListSpec` / `ListNode` 加了 `item_gap` 字段和赋值（+3 行）；`split` 里的项间逻辑**没改**。
- `lower.rs`、`node_tests/list.rs` 没跟上。

处理方式：先 `git checkout -- crates/` 撤掉，按本文 §5 从头做 PR (1)。`docs/提案书.md`、本文和 `.audit/list-capabilities.tsv` 是未提交的文档改动，保留。

## 1. 产品一句话

调用方能把一章条款写成**一个** `ListSpec` 加多个 `ListItemSpec`：序号自增，项间有间距，序号可选格式、字体和对齐方式；项里能放表格、分栏、图片、整体缩进的地址段等任意块，可以任意层嵌套。限制随外层容器继承，嵌套深度有上限，过深时报错而不是栈溢出。另外提供通用的块级左右缩进 `Inset`。

## 2. 范围

### 2.1 做

| PR | 内容 |
| --- | --- |
| (1) | `ListSpec.item_gap` |
| (2) | `ListSpec.marker_format`（`MarkerFormat`）、`marker_font`、`marker_align`；`nest_indent` 改名 `body_indent` |
| (3) | 列表项内容放开到「除 pageBreak 外全可」；限制随外层容器继承；`LimitKind::NestingDepth` 与 lower 前的非递归深度预检 |
| (4) | 序号基线对齐项内**第一个块**最靠上的文字基线；内部 `Fragment` 增加 `first_baseline` |
| (5) | `Block::Inset` / `CellBlock::Inset`；`GeometryKind::InsetWiderThanFrame` |

顺序固定。(4) 先于 (5)，所以 (5) 的 `Inset` 节点要传递 `first_baseline`。

### 2.2 不做（提案书 §12.5「不做」与相关条）

- 列表项内 pageBreak（延后）。
- 项级序号重设（HTML `<li value>`）。
- 倒序、多级自动 `1.1.1`、中文数字自动编号、全角括号 `（1）`、任意前后缀字符串（继续用自定义 `marker`）。
- 嵌套表、单元格内 columns、单元格内 keep_together（仍按 §2.6）。
- 单项独立 `item_gap`。
- 错误里定位到具体哪一项、哪一块（给 `Location` 加块路径）。另议。
- lower 阶段单元格错误补 `Location.cell`（D19）。另议。
- 页眉页脚里的 `Inset` 或列表（`BandSpec` 本来就不收 `Block`）。
- 自动拆分列表标记的中西文字体（§12.2 已定不做）。
- 不改 forms / orgchart / post；不改 hcsw；不擅自 `git commit` / `push`。

## 3. 已确认的决定（含 2026-11-03 本会话逐条确认）

| # | 决定 | 出处 |
| --- | --- | --- |
| D1 | `item_gap` 与该项末块自带的尾随间距**相加**，不取较大值。注：公开 API 下尾随间距眼下恒为 0（`spec/types.rs:96` 段落间距种子为 0；`HLine` 的 `space_after` 计入自身高度、不走 `spacing_after`），所以相加和取大目前输出相同 | 提案书 §12.5；AskQuestion q1 |
| D2 | `item_gap` 放不下就丢，不带到下一页，续页顶部不留空白；丢掉后下一项若能在本页放下首行，就紧贴上一项排在本页。测自然高度时照常计入 | §12.5 |
| D3 | `MarkerFormat` 标 `#[non_exhaustive]` | q2 |
| D4 | `marker_font` 只用该键字面组的**常规面**，不随 `Block::Styled` 换粗体或斜体；字号与颜色照常继承；**也作用于自定义 `marker`**；未注册的键报 `UnregisteredFont`；缺字按 §4 整份失败 | q3 |
| D5 | `InvalidCellContent` **形状不变**。`container` 填**最内层禁止该节点的容器**：单元格 > 列表 > 项 > columns 报 `"table cell"`；单元格 > 列表 > 项 > pageBreak 报 `"list item"` | 第二轮 q4 |
| D6 | 嵌套深度计数见 §7.3，含 `InlineRun::Link`，块与 `Link` 共用一个上限（默认值见 D15） | q5 |
| D7 | `Block::Inset { left: Length<NonNegative>, right: Length<NonNegative>, children: Vec<Block<K>> }`，内联字段变体，和 `Block::Styled` 同形 | q6a |
| D8 | 要 `CellBlock::Inset`，孩子是 `CellBlock<K>`，单元格限制不变 | q6b |
| D9 | `Inset` 收窄后宽度不大于 0 报新增的 `GeometryKind::InsetWiderThanFrame { left, right, avail }`。排版时按实际框宽检查，lower 不查。`left` 与 `right` 都为 0 时允许。宽度为正但孩子放不下时由孩子自己报错，`Inset` 不设最小宽度 | 第二轮 q6c |
| D10 | `ListKind::Ordered(Numbering)` 不变；`marker_format` 挂在 `ListSpec` 上，和 `start` 一样只对 ol 生效，ul 忽略 | §12.5 |
| D11 | `nest_indent` 改名 `body_indent`，语义不变，不留旧名（VER-3） | §12.5 |
| D12 | `marker_align` 的类型是新建的 `MarkerAlign { Start, End }`，标 `#[non_exhaustive]`；不复用 `Alignment`（`Center`、`Justify` 对序号无定义，复用会违反 API-1 / PAN-4）。按 API-9 默认加 `non_exhaustive`，不登记 t7 例外；以后加 `Center`（Word 编号有此档）按 VER-1 不算 breaking | 第三轮 Q1 |
| D13 | `Inset` 在 `LimitKind::Blocks` 里计 1 块再进孩子，同 `Anchor` / `KeepTogether`；不计的话 `Inset` 可生成任意多个不受块数限额约束的节点 | 第三轮 Q2 |
| D14 | `Inset` 孩子为空不报错，照常生成空 `InsetNode`，同 `KeepTogether(vec![])`：计 1 块，§8.2 中作为无效片段跳过，排版时照常查 `InsetWiderThanFrame`，被 `Anchor` 包住报 `AnchorWithoutFragment`。不学 `Styled` 跳过（那是摊平的副产品），不新增错误 | 第三轮 Q6 |
| D15 | 总嵌套深度（§7.3 口径，不只是 ul/ol）默认 **16**，可调，上限按 `admit` 四倍规则为 **64**。取代原「默认 64」。依据：2026-11-03 在 HEAD `4dbbaa4` 临时 worktree 实测，2 MiB 测试线程下逐层嵌套的最大通过层数：debug 列表 66、columns 68、keep_together 71、anchor 159，release 列表 431、columns 427；只跑 lower 时 debug 列表 329，即每层约 25 KiB 栈耗在排版递归。原默认 64 在 debug 下只余约 3%，256 必爆；压帧开销要把 `split` 改成不递归，超出本主题。16 在 debug 2 MiB 下余约 4 倍；调到 64 时未优化构建可能要更大的栈，rustdoc 写明 | 第三轮 Q3 |
| D16 | 项的第一个块是有效 Spacer（高度大于 0）时，序号行盒顶对齐项顶，按提案书字面，不把 Spacer 当透明。这改变现行为（现在跟到后面段落的基线），CHANGELOG 写进 Changed。调用方要让序号随正文下移：首项前的空白放在列表之前，项间用 `item_gap` | 第三轮 Q4 |
| D17 | 「第一个块」边界：`Block::Styled` 透明（lower 摊平，layout 看不到）；按 `fragment_is_effective`（`flow.rs:122`，高 0 且无指令）跳过空 Stack、空 KeepTogether、空 Inset、高 0 Spacer，与 Anchor 判断落地片段同一口径；表取片段第一条物理行，有表头即表头行，含从该行起的跨行格，不为表头特判。续页片段不画序号，续页取值不影响输出，只按同一规则算 | 第三轮 Q5 |
| D18 | 预检判定超限后，在 `spec/depth.rs` 里用显式栈把调用方的 `sections` 拆平再析构，然后返回 `LimitExceeded`。取代原「析构递归不在保护范围、只写 rustdoc」。依据：2026-11-03 实测 debug 2 MiB 下析构约 9800 层 `KeepTogether` 或 `Link` 即溢出。预检通过的路径深度不超过上限，析构自然有界，不用拆。不给 `Block` / `CellBlock` / `InlineRun` 实现 `Drop`（会禁止按值解构，E0509） | 第三轮 R1 |
| D19 | `InvalidCellContent` 等 lower 期单元格错误本主题**不**补 `Location.cell`。`CellIndex` 是摆放跨格后的网格坐标（`error.rs:884-892`），lower 时只有源下标；缺坐标的是 lower 期全部单元格错误（`lower.rs:1034-1040`）。作为后续主题登记在提案书 §12.5「不做」与本文 §2.2 | 第三轮 R2 |

## 4. 公共 API 终态（五个 PR 全部合入后）

```rust
// spec/mod.rs
pub struct ListSpec<K> {
    pub kind: ListKind,
    pub start: NonZeroU32,
    pub marker_width: Length<Positive>,
    /// Body column left edge, measured from the list's left edge.
    /// `None` uses `marker_width`. Must not be less than `marker_width`.
    pub body_indent: Option<Length<Positive>>,          // PR (2)，原 nest_indent
    pub item_gap: Length<NonNegative>,                   // PR (1)，默认 0
    pub marker_format: MarkerFormat,                     // PR (2)，默认 DotSuffix
    pub marker_font: Option<K>,                          // PR (2)，默认 None
    pub marker_align: MarkerAlign,                       // PR (2)，默认 Start（D12）
    pub items: Vec<ListItemSpec<K>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarkerFormat {
    #[default]
    DotSuffix,          // 1.
    IdeographicComma,   // 1、   (U+3001)
    ClosingParen,       // 1)
    Parens,             // (1)
    Bare,               // 1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarkerAlign {
    #[default]
    Start,
    End,
}

pub enum Block<K> {
    // ...现有变体不变...
    Inset {                                              // PR (5)
        left: Length<NonNegative>,
        right: Length<NonNegative>,
        children: Vec<Self>,
    },
}

pub enum CellBlock<K> {
    // ...现有变体不变...
    Inset {                                              // PR (5)
        left: Length<NonNegative>,
        right: Length<NonNegative>,
        children: Vec<Self>,
    },
}

// error.rs
pub enum LimitKind { /* ... */ NestingDepth }          // PR (3)，已是 non_exhaustive
pub enum GeometryKind { /* ... */ InsetWiderThanFrame { left: f32, right: f32, avail: f32 } } // PR (5)

// limits.rs
impl Limits {
    pub const fn max_nesting_depth(&self) -> u32;        // PR (3)，默认 16，可调上限 64（D15）
    pub fn set_max_nesting_depth(&mut self, depth: u32) -> Result<(), Error>; // PR (3)
}
```

`MarkerFormat`、`MarkerAlign` 和 `ListKind` 一样只从 `spec` 模块公开（`pdfcraft_pdf::spec::MarkerFormat`）。`lib.rs:115` 的根重导出**不加**它们：`ListKind` 本来就不在根上，加了就是扩大公共面。

`Block` 和 `CellBlock` 没有 `#[non_exhaustive]`，加变体按 VER-1 是 breaking，写进 0.5.0 的 Breaking。

## 5. PR (1) `item_gap`

### 5.1 改动点

| 文件 | 改什么 |
| --- | --- |
| `src/spec/mod.rs` `ListSpec`（约 538–561 行） | 加 `pub item_gap: Length<NonNegative>`，`Default` 里是 `NonNegative::ZERO`。rustdoc 写清：只加在相邻两项之间；与上一项自身尾随间距相加；放不下就丢，不带到下一页 |
| `src/spec/lower.rs` `lower_list`（约 1140–1191 行） | 把 `spec.item_gap` 传给 layout 层 `ListSpec` |
| `src/layout/list.rs` `ListSpec` / `ListNode` | 各加 `item_gap: Length<NonNegative>`；`ListNode::new` 里赋值 |
| `src/layout/list.rs` `ListNode::split` 项间段（约 268–277 行） | 见 §5.2 |
| `src/layout/node_tests/list.rs` `spec()` 辅助函数 | 字面量加 `item_gap: NonNegative::ZERO` |

公开构造点（`tests/document.rs`、`src/spec/tests.rs`、`examples/hello.rs`）都用了 `..ListSpec::default()`，不用改。

### 5.2 算法

现有代码：

```rust
if self.items.is_empty() {
    trailing_spacing = spacing;
} else if spacing > ZERO && avail.height >= spacing {
    y += spacing; avail.height = avail.height.sub_exact(spacing);
} else if spacing > ZERO && ctx.natural {
    y += spacing; avail.height = ZERO;   // 测量时也计入
}
```

改成：最后一项照旧 `trailing_spacing = spacing`（**不加** `item_gap`）；非最后一项令 `gap = spacing + self.item_gap`，后面两个分支用 `gap` 代替 `spacing`。

- 项在内部跨页（`Split::Partial` 带 `tail`）时提前返回，不涉及 `item_gap`。现有 `partial(..., spacing, ...)` 不变。
- 下一项 `MoveToNextPage` 时，已加到 `y` 里的 `gap` 留在本页 head 高度里，这与现有 `spacing` 的行为一致，不改。
- 嵌套列表各用各的 `item_gap`，互不继承。

### 5.3 测试

layout 层（`src/layout/node_tests/list.rs`，用 `render` + `runs` 断言 `RecordingCanvas` 坐标）：

1. 两项，`item_gap = 6`：第二项序号和正文基线 = 第一项基线 + 行高 + 6。
2. 首项前、末项后不加：单项列表总高度 = 项高；三项列表高度 = 三项高 + 2×gap。
3. 放不下就丢：页高刚好放下第一项、放不下 gap 时，第二项落到第 2 页第一行（y = baseline），第 2 页顶部没有空白。
4. 丢 gap 后紧贴：gap 大于剩余高度、但剩余高度仍够第二项首行时，第二项紧贴第一项排在本页（D2）。
5. 测量计入：把列表包进 `KeepTogether`，用 `natural_height` 量出的高度含所有 gap。
6. ul 和 ol 都生效。
7. 嵌套列表的 `item_gap` 只作用于内层项之间。

公开面（`tests/document.rs`，用 `font_pages` 和 `glyph(...)` 断言）：一个 `item_gap: pt(10)` 的 ol，断言第二项文字 y 比第一项大一行高加 10。

### 5.4 文档与版本

- `Cargo.toml` 版本 `0.4.0` → `0.5.0`，`Cargo.lock` 跟着变。按先例（`a9456d1` 一个 feature PR 把版本和全部版本提及一起改），同一 PR 内更新：`README.md:27`、`crates/pdfcraft-pdf/README.md:21,33,41`、`AGENTS.md:17`、`SECURITY.md:11`、`docs/workspace-crates-spec.md:42,77`、`docs/提案书.md:665`、`crates/pdfcraft-pdf/docs/migrate-*.md` 第 3 行的「当前版本」、根 `CHANGELOG.md`。用 `rg -n '0\.4\.0'` 核对，不要漏。**不要机械替换**：`Cargo.lock` 里 `glifo`、`hayro-ccitt`、`polycool` 也是 `0.4.0`，只改 `pdfcraft-pdf` 条目（由 `cargo` 自己更新 lock，不手改）；各 CHANGELOG 里历史的 `0.4.0` 节不动。
- `crates/pdfcraft-pdf/CHANGELOG.md` 新开 `## [0.5.0] - <日期>`，PR (2)–(5) 往同一节追加：
  - **Breaking**：`ListSpec` 增加公开字段 `item_gap`。迁移方法：结构体字面量末尾写 `..ListSpec::default()`，或显式写 `item_gap: NonNegative::ZERO`。
  - **Added**：`item_gap` 的语义一句话。
- `crates/pdfcraft-pdf/docs/guide.md` 的「Lists」节（约 393–421 行）加 `item_gap` 示例。

## 6. PR (2) `marker_format` + `marker_font` + `marker_align` + `body_indent`

### 6.1 `MarkerFormat`

- 公开枚举见 §4。rustdoc 给出每个变体与五种 `Numbering` 组合的示例：`1.`、`1、`、`1)`、`(1)`、`1`；`a)`、`(iv)`、`A、`。
- 依赖方向是 `spec → layout`，layout 不能引用 `spec::MarkerFormat`。在 `layout/list.rs` 定义内部同构枚举（建议名 `MarkerAffix`，避免同名混淆），在 `spec/types.rs` 加 `marker_affix(MarkerFormat) -> MarkerAffix`，照 `numbering_kind`（`types.rs:26`）的写法做同 crate 内穷尽 `match`（同 crate 匹配 `non_exhaustive` 枚举不需要 `_`，不要写 `_`）。
- `MarkerKind::label(number)`（`list.rs:29`）改成 `label(number, affix)`：先算主体（数字、字母、罗马），再按 affix 包装。范围检查不变：字母 1–26、罗马 1–3999，超出报 `GeometryKind::MarkerOutOfRange`。
- layout 层 `ListKind::Ordered { style, start }` 加 `affix`，或在 layout `ListSpec` 上单列 `affix` 字段。选一种，ul 不读它。
- 自定义 `marker` 仍优先，仍不占号，不套格式。

### 6.2 `marker_font`

- `lower_list` 现在用 `ResolvedFamily::first_face()`（`lower.rs:164–173`，`lower.rs:1171`）。改成：
  - `None`：仍用 `first_face()`（现行为）。
  - `Some(key)`：`family_of(fonts, key, &mut bag)`（`lower.rs:274`）取字面组，失败已由它记 `UnregisteredFont`；成功则 `resolve_run(family, paint.plain(), &mut bag)`。
- `paint.plain()`（`lower.rs:244`）已经把字面重置为常规、去掉装饰线和字距，只保留字号和颜色，正好满足 D4，**不要**再把 `paint` 的粗斜体传进来。
- 同一个 `marker_style` 用于生成的序号、圆点和自定义 `marker`（D4）。
- 更新 `ResolvedFamily::first_face` 的注释（「never pick another」不再准确），以及 `ListItemSpec.marker` 的 rustdoc（`spec/mod.rs:586`「use the first registered face」）。
- `lower_list` 需要 `env.fonts`；`LowerEnv` 已带 `fonts`。

### 6.3 `marker_align`

- 类型 `MarkerAlign`，`#[non_exhaustive]`（D12）。和 `MarkerFormat` 一样，layout 层用内部同构枚举，`spec/types.rs` 里穷尽 `match` 转换，不写 `_`。
- `Start`：序号 x = 0（序号列左缘，现行为）。
- `End`：序号 x = `marker_width - marker.width`，`marker.width` 来自 `single_line_run` 返回的 `SingleLine.width`（`line_run.rs:484–489`）。
- 宽度按最终字符串（含后缀与括号）测量；超过 `marker_width` 沿用现有报错路径（`single_line_run(..., Some(marker_width))`，现有测试 `custom_marker_wider_than_marker_width_errors` 断言的是 `UnbreakableWiderThanFrame`），不裁剪。
- 只影响首页片段画的那一个序号；续页不画序号（不变）。

### 6.4 `body_indent` 改名

- 公开 `ListSpec.nest_indent` → `body_indent`，rustdoc 改为「本层正文左缘距列表左缘的距离；`None` 等于 `marker_width`；不得小于 `marker_width`」。
- 内部 `layout::list::ListSpec.nest_indent` 同步改名。
- `GeometryKind::ListIndent` 的字段文档「The nest indent」→「The body indent」；字段名 `indent` 和显示文字不变。
- 全仓 `rg -n nest_indent` 清零：`tests/document.rs:1736–1752`（测试名 `nest_indent_shifts_the_inner_list` 也改）、`node_tests/list.rs:125–141`、`examples/hello.rs:196`、`guide.md`。

### 6.5 测试

layout 层：

1. 五种 `MarkerFormat` 与五种 `Numbering` 的 `label` 输出表（纯函数，不需要字体），包括边界 26 / 3999 和越界错误。
2. ul 设了 `marker_format` 仍画 `•`。
3. 自定义 `marker` 不套格式、不占号（扩展现有三个 custom marker 测试）。
4. `marker_align: End`：`9.` 和 `10.` 两项（`start = 9`），断言两个序号的**右缘** x 都等于 `marker_width`（右缘 = run.x + 宽度；宽度用 `metrics`/测量辅助算，不写死数字）。
5. `Start` 时 x = 0（回归）。

公开面（`tests/document.rs`）：

6. `marker_font: Some(key)` 时序号 glyph run 的字体 id 是该键常规面，正文是另一个键。
7. 外层 `Block::Styled { bold: Some(true) }` 包住列表：序号仍用常规面，不报 `BoldFaceRequired`（即使字面组没有粗体），字号和颜色跟随 Styled。
8. `marker_font` 未注册：`UnregisteredFont`。
9. `IdeographicComma` 配只有拉丁字形的字体：`MissingGlyph`（U+3001）；换成含 `、` 的 `marker_font` 后通过。`tests/fixtures` 里没有字体文件，Linux CI 上 `PDFCRAFT_PDF_FONT` 指向 CJK 字体（`ci.yml:156-159`），所以：拉丁字体用 `PDFCRAFT_FORMS_FONT`（CI 是 DejaVu Sans，2026-11-03 本机 `fc-list ':charset=3001'` 核实 DejaVu / Liberation 都不含 U+3001），中文字体用 `PDFCRAFT_CJK_FONT`。读字体时先用 skrifa charmap 断言拉丁字体确实不含 U+3001、中文字体确实含，前提不成立就 `panic` 说明（测试前提错了，不是被测行为）。任一变量缺失时 `eprintln!` 写明原因后提前返回（TST-6），不要用静默的 `skip_without_font`。
10. `body_indent` 改名后原 `nest_indent` 测试行为不变。

### 6.6 文档

- CHANGELOG **Breaking**：`nest_indent` 改名 `body_indent`（迁移：字段名替换，语义不变）；`ListSpec` 增加 `marker_format`、`marker_font`、`marker_align` 三个公开字段（迁移同 §5.4，用 `..ListSpec::default()`）。**Added**：`MarkerFormat`、`MarkerAlign` 与 `marker_font` 语义。
- `guide.md`：205 行「List markers use the first face」、313 行继承表里「list markers use the regular face」、「Lists」节示例同步。
- `examples/hello.rs` 的 `nest_indent` 改名；可选加一个 `IdeographicComma` + `marker_font` 的示例项（要用仓库里已有字体）。

## 7. PR (3) 项内放开 + 限制继承 + 嵌套深度预检

### 7.1 规则矩阵（`layout/nesting.rs` `check_content`，约 28–66 行）

改后的唯一矩阵。不允许在别处另写第二套规则。

| 节点 \ 容器 | Column | TableCell | ListItem | Band |
| --- | --- | --- | --- | --- |
| Paragraph / Spacer / HLine | 可 | 可 | 可 | 可 |
| Stack / Anchor | 可，递归 | 可，递归 | 可，递归 | 可，递归 |
| KeepTogether | 可，递归 | **禁** | 可，**递归** | 禁 |
| Columns | 可（栏自己查） | **禁** | 可（栏自己查） | 可，递归（现行为） |
| Table | 可 | **禁** | 可 | 禁 |
| List | 可（项自己查） | 可，**递归进各项、仍按 TableCell 查** | 可（项自己查） | 禁 |
| Image / Canvas | 可 | 可 | **可** | 禁 |
| PageBreak | 禁 | 禁 | 禁 | 禁 |
| Inset（PR 5 加） | 可，递归 | 可，递归 | 可，递归 | 不会出现 |

要点：

- **ListItem 要递归进 KeepTogether 与 Stack/Anchor**，因为项里 `keep_together` 里的 pageBreak 只有这样才查得到。Columns 不用递归（`ColumnsNode::new` 会用 Column 容器查每栏，栏禁 pageBreak）；Table 不用递归（单元格自己查）；List 不用递归（`ItemBody::new` 自己查）。
- **TableCell 遇到 List 时递归进每一项的正文，容器仍用 TableCell**。这样单元格 > 列表 > 项 > columns 报 `"table cell"`（D5）。
- 构造顺序天然满足「最内层禁止者」：lower 自底向上，`ItemBody::new`（ListItem 规则）先于 `CellBody::new`（TableCell 规则）执行。所以单元格 > 列表 > 项 > pageBreak 先被 ListItem 拒，报 `"list item"`；单元格 > 列表 > 项 > columns 过了 ListItem，再被 TableCell 拒，报 `"table cell"`。要有测试锁住这两条。
- 拿项正文：`Flowable::nested`（`frame.rs:203`）对 List 返回空，`ListNode` 用 `VecDeque<PendingItem>` 存，给不出 `&[Self]`。推荐给 `Flowable` 加一个方法（如 `fn list_bodies(&self) -> Vec<&Self>`，`FlowNode::List` 调现有 `ListNode::append_bodies`，`list.rs:186`），只在 `check_content` 的 List 分支用。不要为此改 `ListNode` 的存储。
- `ErrorKind::InvalidCellContent` 的 rustdoc 补一句：`container` 是最内层禁止该节点的容器。

### 7.2 `LimitKind::NestingDepth` 与 `Limits`

- `error.rs:216` `LimitKind` 加 `NestingDepth`，`Display` 为 `"nesting depth"`。
- `limits.rs`：常量 `DEFAULT_NESTING_DEPTH: u32 = 16`；字段 `nesting_depth`；`max_nesting_depth()`；`set_max_nesting_depth(u32)` 走现有 `admit`（拒 0 和大于默认四倍，即 64；D15）。模块文档首段的默认值列表加上「16 nesting depth」。`limits.rs` 内 `rejects(...)` 测试表加一行。
- `max_nesting_depth` / `set_max_nesting_depth` 的 rustdoc 写明：这是整份文档的总嵌套深度（§7.3 口径）；默认值在 2 MiB 线程栈、未优化构建下不会栈溢出；调到默认值以上时，未优化构建可能需要更大的线程栈。不写具体 KiB 数字（随编译器变）。
- `Limits` 字段私有，加字段不 breaking。

### 7.3 深度定义（以此为准）

正文里**直接**放在 `SectionSpec.body` 的块是第 0 层。设某元素在第 d 层：

| 元素 | 其孩子所在层 |
| --- | --- |
| `Block::KeepTogether(children)` | d+1 |
| `Block::Styled { children }` | d+1 |
| `Block::Anchor { child }` / `CellBlock::Anchor { child }` | d+1 |
| `Block::Inset { children }` / `CellBlock::Inset { children }`（PR 5 起） | d+1 |
| `Block::Columns`：每栏的 `blocks` | d+1（columns 连同某一栏算一层） |
| `Block::List` / `CellBlock::List`：每项的 `blocks` | d+1（列表连同某一项算一层） |
| `Block::Table`：每个单元格的 `content` | d+1（表格连同某个单元格算一层） |
| `Block::Paragraph` / `CellBlock::Paragraph`：顶层 `runs` | d（段落本身不加层） |
| `InlineRun::Link { runs }` | d+1 |
| 其余（Spacer、PageBreak、Image、Canvas、HLine、`CellBlock::Text`、`InlineRun::Text`、`InlineRun::Blank`） | 叶子 |

文档深度 = 所有元素层数的最大值。深度 > `max_nesting_depth()` 时失败。默认 16：深度 16 通过，17 失败。

例：`Styled{[List{items:[{blocks:[Paragraph{runs:[Link{[Link{[Text]}]}]}]}]}]}`。Styled 0，List 1，Paragraph 2，外层 Link 2，内层 Link 3，Text 4，深度 4。

### 7.4 预检实现

- 新文件 `src/spec/depth.rs`（`spec` 内部模块，`spec → limits, error` 已在 DAG 内，不新增边；仍要跑 `scripts/check_pdfcraft_pdf_modules.py`）。
- 入口 `pub(super) fn check_depth<K>(sections: &[SectionSpec<K>], max: u32) -> Result<(), Error>`。
- **不递归**：显式栈 `Vec<(Node<'_, K>, u32)>`，`enum Node<'a, K> { Block(&'a Block<K>), Cell(&'a CellBlock<K>), Run(&'a InlineRun<K>) }`。
- 必须覆盖的路径（逐条对照 lower 里的递归点）：`Block::List` 与 `CellBlock::List` 的 `items[*].blocks`；`TableSpec.header` 与 `body` 两组行里每格的 `content`；`ColumnsSpec.columns[*].blocks`；`Block::Paragraph` 与 `CellBlock::Paragraph` 的 `runs`；`InlineRun::Link.runs` 递归（`lower.rs:724` `push_inline_runs` 自调用）；`CellBlock::Anchor.child`。
- 对 `Block`、`CellBlock`、`InlineRun` 都写**穷尽 `match`，不写 `_`**，将来加变体时编译期就报错（PR 5 加 `Inset` 时会被逼着补上）。
- 走完整棵树，算出真实最大深度：`LimitExceeded { kind: NestingDepth, value: 实测最大深度, max }`。错误带 `Location::section(index)`，`index` 是第一个超限元素所在节。
- 调用点：`layout_spec`（`lower.rs:66`）里，`sections.is_empty()` 补默认节之后、`lower_resources` 之前。超限时不进入 lower（lower 和 `check_content`、`split` 都是递归，必须挡在前面），先调 `dismantle` 再 `return Err(...)`（D18）。
- 拆平析构（D18）：`fn dismantle<K>(sections: Vec<SectionSpec<K>>)`，同在 `spec/depth.rs`，不递归。显式栈装**拥有所有权**的节点（`Block<K>`、`CellBlock<K>`、`InlineRun<K>`）；每弹出一个，就把它的孩子集合用 `std::mem::take` 移进栈（`Vec` 直接 take；`Block::Anchor.child` 换成 `Block::PageBreak`，`CellBlock::Anchor.child` 换成 `CellBlock::Spacer(NonNegative::ZERO)`，取出原值入栈），然后让它自己析构（此时已是浅层）。覆盖路径与 `check_depth` 相同：`KeepTogether`、`Styled`、`Anchor`、`Inset`、`Columns` 各栏 `blocks`、`Table` 的 `header` / `body` 每格 `content`、`List` 各项 `blocks`、`Paragraph` 的 `runs`、`InlineRun::Link.runs`、`CellBlock::{List, Anchor, Paragraph, Inset}`。三个枚举都写穷尽 `match`，不写 `_`。只在超限路径调用；预检通过时深度不超过上限，析构自然有界。代码注释写一句「Drop glue recurses per level; a rejected tree can be arbitrarily deep」这类约束说明（DOC-5），不写来历。
- 页眉页脚（`BandFn`）、`behind`/`front`（`LayerMark`）不收 `Block`，不在预检范围。已核对（2026-11-03）：`CanvasMark`（`spec/mod.rs:612`）、`LayerMark`（`:919`）、`LayerShape`（`:987`）、`BandMark`（`:826`）都不自引用，最深两层（`Rotated` 含 `Vec<LayerShape>`），预检与 `dismantle` 都不用管；`Block::Canvas` 是叶子。spec 里自引用的只有 `Block`（`:204,213,222`）、`InlineRun`（`:366`）、`CellBlock`（`:531`）。动手时若行号变了重新核对。
- 现有 `flow::check_limits`（`flow.rs:369`）不改，仍在 lower 之后管块数、单元格数、段落字节。

### 7.5 测试

layout 层（`nesting.rs` 旁路或 `node_tests/list.rs`）：

1. 项内可放 table、columns、image、canvas、keep_together、嵌套 ol>li>ul>li>ol（替换现有 `item_rejects_table_page_break_and_image`，只保留 pageBreak 被拒）。
2. 项内 `keep_together` 里的 pageBreak：`InvalidCellContent { container: "list item", node: "page break" }`。
3. 栏 > 列表 > 项 > table：通过。

公开面（`tests/document.rs`）：

4. 单元格 > 列表 > 项 > columns：`{ container: "table cell", node: "columns" }`。**不断言 `Location.cell`**：单元格内容错误在 lower 阶段由 `Cell::new`（`table.rs:57`）报出，`lower_table_rows` 不附行列坐标，现有 `.in_cell(..)`（`table.rs:157–278`）只覆盖跨度和测量错误。本主题不补（D19，已登记为后续主题）。
5. 单元格 > 列表 > 项 > table、> keep_together：同样报 `"table cell"`。
6. 单元格 > 列表 > 项 > pageBreak：报 `"list item"`。
7. 单元格 > 列表 > 项 > image：通过（单元格本来允许 image）。
8. 项内放 table 跨页：表在项内按行切开，续页正文左缘保持 `body_indent`，续页不画序号（RecordingCanvas 断言 x 坐标）。
9. 深度：16 层通过；17 层得 `LimitExceeded { NestingDepth, value: 17, max: 16 }`；`set_max_nesting_depth(10)` 后 11 层失败；`set_max_nesting_depth(65)` 被 `admit` 拒（`max: 64`）；`Link` 嵌套也计入（只靠 Link 嵌套到 17 层也失败）；混合 Styled、List、Table、Columns 的计数和 §7.3 的例子一致。
10. 栈安全（默认值）：构造 16 层最重的结构（List、Columns、KeepTogether、Inset 交替；PR 5 之前不含 Inset），在默认测试线程（2 MiB 栈）、debug 构建里跑完 `layout_spec` 不崩。**验收**：同一结构在 debug 2 MiB 下至少 32 层仍能跑完（默认值 2 倍余量）；这条用临时实验验证（在 `/tmp` 的临时 git worktree 里做，做完删掉 worktree 与 target，不留在工作区），结果记进决策日志，不留成测试。PR (4)、PR (5) 结束时各重跑一次（PR 4 给每层 `split` 加了 `first_baseline` 计算，PR 5 加了 `Inset` 帧并把 Inset 加入交替结构）。任何一次达不到就停下来问用户。
10a. 栈安全（上限）：`set_max_nesting_depth(64)` 后同一结构 64 层，在显式给定 8 MiB 栈的线程里跑完。这测的是 rustdoc 写明的「调高需要更大栈」合同，不是掩盖问题；第 11 条的析构测试仍不许用大栈线程。
11. 超深输入不崩（D18）：在默认 2 MiB 测试线程、debug 构建里，分别构造 100 000 层的 `KeepTogether`、100 000 层嵌套的 `InlineRun::Link`、以及 `List` / `Columns` / `Table` 单元格 > `CellBlock::Anchor` 交替的超深树，`layout_spec` 都返回 `LimitExceeded { NestingDepth, .. }`，进程不 abort。100 000 是实测析构溢出点（约 9800 层）的 10 倍以上。**不许用大栈线程**。测试自己构造树时用循环，不递归；测试函数里不能在 `layout_spec` 之外再持有一棵超深树（否则测试自己析构时会爆栈）。
11a. 覆盖完整性：再构造一棵 100 000 层、**每种带孩子的变体轮流出现**的树（`Block` 的 `KeepTogether` / `Styled` / `Anchor` / `Inset` / `Columns` / `Table` / `List` / `Paragraph`→`Link`，`CellBlock` 的 `Anchor` / `List` / `Inset` / `Paragraph`），在默认 2 MiB 线程里返回 `LimitExceeded` 不崩。`dismantle` 漏掉任何一条路径，那条路径上的析构就会递归，这条测试会爆栈，以此锁住覆盖面。PR 5 加 `Inset` 时同步加入轮换。
11b. rustdoc（`layout_spec` 的 `# Errors` 或 `max_nesting_depth`）写终态：超出深度上限时返回错误，不会因输入过深而栈溢出。不写实现方式（DOC-4）。
12. 顺序：深度超限且同时有坏字体字节时，只报 `NestingDepth`（证明预检在 `lower_resources` 之前）。

### 7.6 文档

- CHANGELOG **Changed**：列表项内容放开到除 pageBreak 外全部可放，限制随外层容器继承，`container` 报最内层禁止者。**Added**：`LimitKind::NestingDepth`、`Limits::{max_nesting_depth, set_max_nesting_depth}`，总嵌套深度默认 16、可调上限 64，在 lower 前检查。这两项都不 breaking（`LimitKind` 已 non_exhaustive，`Limits` 字段私有），但项内放开会让原本报错的文档变成能排版，属用户可见行为变化。
- `guide.md` 265–268 行的单元格与列表项允许表改写。
- `guide.md` 「Limits」节（约 526–538 行）的默认值表加一行「Nesting depth | 16」，并注明这是整份文档的总嵌套深度、调高后未优化构建可能需要更大的线程栈。
- `ListItemSpec.blocks` 的 rustdoc 写清能放什么、不能放什么。

## 8. PR (4) 序号对齐首块基线

### 8.1 内部 `Fragment` 加字段

- `layout/frame.rs:61` `Fragment` 加 `first_baseline: Option<Length<Finite>>`，fragment 局部坐标（y 向下）。`Fragment::empty()` 为 `None`。
- `Fragment::with_offset(dx, dy)`（`frame.rs:78`）把 `first_baseline` 加上 `dy`。
- 所有构造 `Fragment { .. }` 的地方都要显式给值。2026-11-03 的清单（`rg -n "Fragment \{" src/layout` 复核）：`image.rs:42`、`canvas.rs:85`、`hline.rs:74`、`decorate.rs:272,369`、`flow.rs:196,321,343,357`、`columns.rs:122`、`table.rs:363,502`、`paragraph.rs:299,331`、`list.rs:284,296`、`node_tests/list.rs:307`。

### 8.2 每种节点的取值

「第一个块」是指项正文 stack 的第一个孩子。`Block::Styled` 在 lower 时被摊平进父级（`lower.rs:563–573`），所以 `[Styled{[para]}]` 的第一个块就是 `para`。

| 节点 | `first_baseline` |
| --- | --- |
| Paragraph / Text | 本片段第一行的基线（`paragraph.rs` `fragment_from_lines` 里已有行顶和基线） |
| Stack | **第一个产生有效片段的孩子**（`fragment_is_effective`）的 `first_baseline`（加上它的 y 偏移）。那个孩子是 `None` 就是 `None`，**不往后找**。这是和现在 `first_baseline()` 扫整个片段的行为差别 |
| KeepTogether | 内部 stack 的值 |
| Anchor | child 的值 |
| Columns | 各栏 `first_baseline` 的最小值（栏顶对齐，偏移 dy = 0）；都没有就是 `None` |
| Table | 本片段第一条物理行里，各单元格（含从这一行开始的跨行格）的 `行 y + padding + 边框占位 + 格内片段 first_baseline` 取最小值；有表头时第一行就是表头行，续页的重复表头同样照算，不特判（D17）；各格都是 `None` 时为 `None` |
| List | 第一项：本片段画了序号，就取序号基线；没画序号（续页），就取该项正文的 `first_baseline` |
| Inset（PR 5） | child 的值，dy 不变 |
| Image / Canvas / HLine / Spacer / PageBreak | `None` |

边界（D16、D17，已写入提案书 §12.5）：

- `Block::Styled` 被摊平，所以对「第一个块」透明。
- Stack 跳过不产生有效片段的孩子（沿用 `flow.rs:122` `fragment_is_effective`：高度 0 且没有绘制指令），比如空 Stack、空 Styled、空 KeepTogether、空 Inset、高度 0 的 Spacer。不另写第二套判断。
- **有效的** Spacer（高度大于 0）算第一个块，结果为 `None`，序号顶对齐。
- Table 取片段第一条物理行，有表头时就是表头行，含从该行起的跨行格，不为表头特判。List 续页取该项正文的值。续页片段不画序号，这两条续页取法不影响输出，只是同一规则的自然结果。

### 8.3 列表使用

- `ListNode::split`（`list.rs:247`）把 `first_baseline(&fragment).unwrap_or(marker.metrics.baseline)` 改成 `fragment.first_baseline.unwrap_or(marker.metrics.baseline)`。`None` 时序号行盒顶对齐项顶（提案书 §12.5）。
- 删除 `list.rs:314` 的扫描函数 `first_baseline()`，以及只为它存在的测试 `image_command_has_no_baseline`（`node_tests/list.rs:304`），改为 §8.4 的测试。

### 8.4 测试（全部用 RecordingCanvas 坐标）

1. 项 = [段落]：序号基线 = 段落首行基线（回归）。
2. 项 = [图片, 段落]：序号顶对齐项顶，基线 = 序号自身 `metrics.baseline`，**不是**段落基线。
3. 项 = [Spacer(10), 段落]：序号顶对齐项顶（行为变化，D16）。
4. 项 = [Spacer(0), 段落]、[KeepTogether([]), 段落]、[Styled{[]}, 段落]：被跳过，同第 1 条（D17）。
5. 项 = [table]：序号基线 = 表第一行格内文字基线（含 padding 偏移）。
6. 项 = [带表头的 table]：序号基线 = 表头行格内文字基线（D17）。
7. 项 = [columns[小字号段落, 大字号段落]]：序号基线 = 两栏首行基线里更靠上的那条。
8. 项 = [嵌套 ol]：外层序号基线 = 内层第一项序号基线。
9. 项 = [Styled{[段落]}]：同第 1 条。
10. 项 = [Anchor{段落}]、[KeepTogether[段落]]：同第 1 条。
11. 段落字号大于序号字号：序号基线与段落首行基线相同（现有能力回归）。
12. 每种节点单测 `first_baseline` 的取值；`with_offset` 平移 `first_baseline`。

### 8.5 文档

- CHANGELOG **Changed**：序号对齐项内第一个块的首条基线；第一个块没有文字（图片、矢量块、分隔线、高度大于 0 的 Spacer）时序号顶对齐项顶。以前会去对齐后面块的文字；用 Spacer 做项内首行留白的调用方，序号不再随正文下移，改用列表前的空白或 `item_gap`。
- `ListSpec` 或 `ListItemSpec` 的 rustdoc 写这条规则。

## 9. PR (5) `Block::Inset` / `CellBlock::Inset`

### 9.1 layout 节点

- 新模块 `src/layout/inset.rs`：`pub(crate) struct InsetNode<N> { left: Length<NonNegative>, right: Length<NonNegative>, child: Box<N> }`。在 [`module-dependency.md`](module-dependency.md) 的目标图里加 `layout/inset → error, geom, layout/frame`（按实际 `use` 定），跑 `scripts/check_pdfcraft_pdf_modules.py`。
- `FlowNode::Inset(InsetNode<FlowNode>)`；`NodeKind::Inset`；`kind_name` 返回 `"inset"`；`Flowable::nested` 返回 `std::slice::from_ref(child)`。
- `split(ctx, avail)`：
  1. `w = avail.width - left - right`；`w <= 0` 时报 `GeometryKind::InsetWiderThanFrame { left, right, avail: avail.width }`。
  2. `child.split(ctx, Frame { width: w, ..avail })`。
  3. 片段 `with_offset(left, 0)`；`spacing_after`、`first_baseline`、anchors 随片段平移（`with_offset` 已处理）。
  4. `Partial` / `MoveToNextPage` 的 tail 仍包在 `InsetNode` 里，续页保持同样缩进。
- 不画任何东西，不改纵向，不产生间距。
- 测量路径（`natural_height`、`stacked_height`、单元格测量）走同一个 `split`，同样报错。

### 9.2 规则与计数

- `check_content`：Inset 在所有容器里都「可，递归，容器不变」（§7.1 最后一行）。
- `census`（`flow.rs:403`）：`FlowNode::Inset` 计 1 块再进 child，同 `Anchor`（D13）。`check_limits` 的 rustdoc 不用改（它只说 stack 不计）。
- 深度预检（`spec/depth.rs`）：`Block::Inset`、`CellBlock::Inset` 各计一层（§7.3），穷尽 match 会逼着补上。
- 首块基线：child 的值（§8.2）。

### 9.3 lower

- `lower_blocks`（`lower.rs:531`）加 `Block::Inset` 分支：`lower_blocks(children, env, ...)` → `sole_or_stack`（`lower.rs:650`）→ `FlowNode::Inset`。
- `push_cell_block`（`lower.rs:1082`）加 `CellBlock::Inset`：孩子逐个 `push_cell_block` 进一个新 Vec → `sole_or_stack` → `FlowNode::Inset`。
- `Inset` 孩子为空：不报错，`sole_or_stack` 给出 `FlowNode::Stack(Vec::new())`，照常包成 `FlowNode::Inset`（D14）。不加特判分支。

### 9.4 错误

- `error.rs` `GeometryKind` 加 `InsetWiderThanFrame { left: f32, right: f32, avail: f32 }`，字段文档写 pt。`Display`：`inset left {left} plus right {right} leaves no room in width {avail}`。
- `GeometryKind` 已 `#[non_exhaustive]`，加变体不 breaking。

### 9.5 测试（RecordingCanvas 坐标）

1. `Inset{left:20, right:30}` 包段落：每行 x 从 20 起；断行宽度 = 框宽 − 50（构造一段刚好在框宽 − 50 处换行的文字）。
2. 跨页：长段落在 Inset 里跨页，第 2 页每行 x 仍是 20。
3. `left = right = 0`：与不包时坐标完全相同。
4. 过窄：`left + right >= 框宽` → `InsetWiderThanFrame { left, right, avail }`；同一个 Inset 在宽框里通过、放进窄栏里失败（证明是排版时按实际框宽检查）。
5. 宽度为正但图片比收窄后的宽度还宽：报图片自己的 `UnbreakableWiderThanFrame`，不报 Inset 错误。
6. 列表项里 `[段落, Inset{段落}]`：地址段整体右移，序号不受影响。
7. `Inset` 包整个 `ol`：序号和正文一起右移 `left`。
8. `CellBlock::Inset` 在单元格里：x = 格左 + padding + left。
9. 单元格里 `CellBlock::Inset` 包的内容仍受单元格限制（`CellBlock` 本身没有 columns 变体，所以测单元格 > Inset > 列表 > 项 > columns 报 `"table cell"`）。
10. 栏里 `Block::Inset` 包 table：通过；Inset 里 pageBreak：在栏内报 `"column"`，在列表项内报 `"list item"`。
11. 锚点在 Inset 里：跳转目标 x 含 `left` 偏移。
12. 首块基线：项 = [Inset{段落}] 时序号基线 = 段落首行基线。
13. 深度：Inset 计一层。

### 9.6 文档

- CHANGELOG **Breaking**：`Block` 与 `CellBlock` 各增加 `Inset` 变体（两者都是穷尽枚举，调用方对它们的穷尽 `match` 要补分支）。迁移：给 `match` 加 `Block::Inset { .. }` / `CellBlock::Inset { .. }` 分支或通配。**Added**：`Inset` 语义、`GeometryKind::InsetWiderThanFrame`。
- `guide.md` 块类型表（约 256 行一带）加 `Inset`；单元格允许表加 `Inset`。
- `examples/hello.rs` 或新示例展示「列表项内整体缩进的多行地址段」。

## 10. 每个 PR 的共同要求

- 一 PR 一件事。主题内需要的重构可以做，无关模块不碰。
- 公开条目都有 rustdoc。`RUSTDOCFLAGS=-D warnings` 下 `cargo doc` 通过。
- 用户可见行为写进 `crates/pdfcraft-pdf/CHANGELOG.md` 的 `0.5.0` 节；breaking 写进 Breaking 并附迁移方法（VER-2）。不留过渡别名或双字段（VER-3）。
- 排版断言只用 `RecordingCanvas` 坐标（`render` / `runs` / `font_pages` / `glyph`）。`to_pdf` 冒烟只断言 `%PDF`。
- 中间步骤只跑小步检查：`cargo fmt --all -- --check`、该包 `clippy --all-targets -- -D warnings`（两套 feature 组合各一次）、本步新增测试。
- 每个 PR 交付前跑 TOOL-5 完整门禁（见 [`rust-engineering-standard.md`](rust-engineering-standard.md) 第 46–65 行），全部绿才交给用户。CI 另有 `semver-checks`、`feature-matrix`、`minimal-versions`、`minrust`，本地不跑；本主题有多处 breaking，`semver-checks` 会报，这是预期，靠 `0.5.0` 版本号和 CHANGELOG Breaking 覆盖。交付说明里要写明这几项本地没跑。
- 不 `git commit`、不 `push`。每个 PR 做完停下来，等用户审阅和提交，再开始下一个，避免五个主题混在同一个工作区 diff 里。
- 决策和验证结果追加到 `.audit/list-capabilities.tsv`（只追加，不改历史行）。

## 11. 不变量（每个 PR 都不得破坏）

- 现有列表测试全部通过（序号只在首页片段画，续页正文缩进不变，自定义 marker 不占号，过宽 marker 报错）。
- `layout` / `text` / `fonts` / `spec` 不 `use krilla`；`spec` 不引用 `backend`。
- 测量与绘制同一条字形管线；序号不调用 `split_latin`。
- 不引入新依赖。

## 12. 开工前须确认

无。原 Q1–Q6 与风险 R1、R2 已于 2026-11-03 第三轮逐条确认，见 §3 D12–D19。实现中若发现与代码事实冲突，按抬头「冲突」一行处理：先停下来改提案书或本文，再继续。
