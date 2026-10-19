# pdfcraft-pdf §12.3 两端对齐（`Alignment::Justify`）规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-19 |
| 状态 | **已定稿。** 产品要点见提案书 §12.3（2026-10-16）；可执行细则经 2026-10-19 讨论钉死（§9）。与提案书冲突时以提案书为准 |
| 范围 | `pdfcraft-pdf` 段落行在摆放期分余量；断行算法不变 |
| 相关 | [`提案书.md`](提案书.md) §12.3、§12.1 字距与装饰；`spec/mod.rs`（`Alignment`）；`layout/style.rs`（`TextAlign`）；`layout/paragraph.rs` / `line_run.rs`；`text/line_break.rs` / `break_class.rs` |
| 调用方 | 段落 `ParagraphSpec.align`；`ImageSpec` / `HLineSpec` / `CanvasSpec` 的同名字段见 §3.3 |
| 作废 | 设计稿「v1 不做 Justify」及「不在字符之间插入额外 advance」——以提案书与本文为准 |

本文是实现合同。按 Feature / poteto playbook 落地；不重开产品讨论。实现中若发现与 skrifa / krilla / 断行事实冲突，先停下来改提案或本文。

## 1. 产品一句话

`Alignment::Justify`：断行结果不变，只在摆放字形时把行内余量分给空格（有上限），再分给合格的字素簇间隙。段末行、硬换行前行、带 `Blank::Fill` 的行、无可拉伸槽的行按 `Start` 摆。

## 2. 范围

### 2.1 做（本主题）

- 公共 `Alignment` 增加 `Justify`；内部 `TextAlign` 同步增加。
- `spec/lower` 把段落的 `Alignment::Justify` 降成 `TextAlign::Justify`。
- 摆放期两端对齐算法（§4）：断行、测量、字距、行盒高度不变。
- 链接矩形与装饰线跟拉开后的水平位置走（现有 `LinePainter` 按 `draws` 宽推进即可）。
- 集成 / 单元测试：`RecordingCanvas` 钉拉开后的 glyph x；覆盖 §4.7 与 §5。
- `CHANGELOG.md`、`guide.md`、crate README 去掉「无 justify」表述，写明 `Justify` 行为。

### 2.2 不做

- §12.4 锚点 / 书签 / 文内 `LinkTarget::Anchor`。
- 标点挤压、标点悬挂、字面压缩（缩小字形）。
- 改断行表、改行首丢 U+0020 规则、改 U+3000 保留规则。
- 完整 UAX #14、bidi、kerning、连字、竖排。
- 章跟流走、某页关 header/footer、流内 canvas、具名 images、TOC / QR / SVG。
- 改 loco-rs / hcsw 工程文件。
- 不擅自 `git commit` / `push`。

## 3. 公共 API

### 3.1 `Alignment`

```rust
pub enum Alignment {
    Start,
    Center,
    End,
    /// 两端对齐。仅段落行按 §4 分余量；见 §3.3。
    Justify,
}
```

- `Default` 仍是 `Start`。
- `PartialEq` / `Eq` / `Copy` / `Clone` / `Debug` 保持不变。
- 穷尽 match 的调用方（含本仓库测试与示例）必须补 `Justify` 臂。

### 3.2 内部 `TextAlign`

```rust
pub(crate) enum TextAlign {
    Start,
    Center,
    End,
    Justify,
}
```

`align_x`：`Justify` 与 `Start` 相同（行起点贴框左；余量进簇宽，不进整体平移）。

### 3.3 非段落上的 `Justify`

`ImageSpec.align`、`HLineSpec.align`、`CanvasSpec.align` 与段落共用 `Alignment`。

| 位置 | `Justify` 行为 |
| --- | --- |
| `ParagraphSpec.align` | §4 两端对齐 |
| `ImageSpec` / `HLineSpec` / `CanvasSpec` | **按 `Start` 摆放**，不报错 |

`lower` 里这三处把 `Alignment::Justify` 映射成 `TextAlign::Start`（或在 `place_atomic` / `hline` / `canvas` 入口把 `Justify` 当成 `Start`）。文档写明。

### 3.4 Breaking

- `Alignment` 新增变体：穷尽匹配需改。
- 行为：段落设 `Justify` 后，中间行字形 x 与现 `Start` 不同。
- loco-rs / 存量若未 match `Alignment`、未用 `Justify`，预期零改动。

## 4. 算法

### 4.1 输入与余量

对已断好的一行：

- `line_width`：现有 `SpanLine::width()`（已含 §12.1 字距；行末簇的 tracking gap 已按现规则丢掉；**不含**行首被丢掉的 U+0020）。
- `frame_width`：该行可用宽。首行已是 `段落宽 − first_line_indent`；缩进只体现在 `frame_x`，**不**参与分余量。
- `remainder = frame_width − line_width`（pt）。

断行不变：不因 Justify 重断、不回插已丢掉的行首空格。

### 4.2 何时按 Start（不分余量）

下列任一成立则整行按 `Start`（`align_x` 为 0，不改各簇宽）：

1. **段落最后一行**（该段 `broken` 的最后一条 `SpanLine`）。
2. **硬换行前的行**：该行因 `BreakClass::Hard`（`\n`，含 CRLF 正规化后的 BK）结束。实现须在断行产物上保留标志（例如 `SpanLine::ended_by_hard_break`）；Hard 原子本身仍不进入行内绘制。
3. **带 `BlankWidth::ToEndOfLine`（`Advance::Fill`）的行**：该行在投影原子时含 Fill blank。Fill 解析成定宽之后仍须保留「本行含 Fill」标志（例如 `SpanLine::has_fill`），不能只看解析后的宽度。
4. **无可拉伸槽**：本行没有可参与第一阶段的 U+0020，且没有可参与第二阶段的簇间隙（定义见 §4.4、§4.5）。
5. **`remainder ≤ 0`**（已满或浮点下微负）：不分余量，按 Start。

单行段落同时是「最后一行」，故整段 `Justify` 时该行仍按 Start。

### 4.3 第一阶段：空格加宽

**空格**：本行已绘制的、断行类为 `PairClass::Sp` 的字素簇。现行断行里只有 **U+0020** 是 `Sp`。U+00A0、U+3000 等不是本阶段对象（U+3000 不是可撑开簇；若邻接汉字，该缝可走第二阶段，见 §4.4）。

对每个空格簇 `i`：

- `cap_i = 0.5 × font_size_i`，`font_size_i` 为该空格所在 run 解析后的字号（与 `GlyphStyle.font_size` 相同）。
- 设本行有 `N ≥ 1` 个空格，余量 `R`。`add_i` 初值为 0。

**分配（二次泼水 / water-filling，先用满空格额度，对齐 Word「先词距」）：**

```text
while R > 0 且存在 add_j < cap_j 的空格:
    S = { 仍有额度的空格 }
    对每个 i ∈ S:
        room_i = cap_i − add_i
        share = R / |S|
        delta = min(share, room_i)
        add_i += delta
        R -= delta
phase1 = Σ add_i
R2 = R   # 全部空格触顶后仍剩的，交给第二阶段
```

实现可用等价的多轮均分，或按 cap 排序的一次水填；测例按最终 `add_i` 断言即可。`N = 0` 时 `phase1 = 0`，`R2 = 原余量`。

同行空格字号相同时，与「一次 `min(R/N, cap)`」数值相同。

加宽体现在该空格簇的绘制宽：`draw_space_i = body_i + add_i`（body 为字形 advance；空格通常无 tracking 落在自身——tracking 仍按现规则只加在「后接文字簇」的前一簇上，本阶段不改 tracking 语义）。

### 4.4 第二阶段：合格簇间隙

在 **相邻两个字素簇**（均为本行 Ink 簇，中间无 Blank）之间，若至少一侧是 **可撑开簇**，则该间隙可分余量。英文词内部（两侧皆非可撑开簇，典型为 `Al|Al`）不拉开。

中间夹着 Blank 的两侧文字簇之间：**不**形成第二阶段间隙（Blank 宽不变；Fill 行已在 §4.2 整行 Start）。

**可撑开簇**（看簇的第一个标量；对齐 Word 中文「两端对齐」的 inter-ideograph 语义，**不**复用断行 `Id` 全集）：

| 资格 | 定义 |
| --- | --- |
| 汉字 | U+3400–U+4DBF（扩展 A）、U+4E00–U+9FFF（统一表意）。**不含**假名、谚文、U+3000、扩展 B、兼容表意 |
| 全角标点 | 断行 `Op` / `Cl` 表中 **码位 ≥ U+3000** 的字符（全角括号与 CJK 标点）。**排除** ASCII `()[]{}` |

对齐依据：Word / MSO 东亚两端对齐（`text-justify: inter-ideograph`）在表意文字之间及中西文交界加空隙，拉丁词内不加；本库 v1 以中文公文为主，假名/谚文不按字拉开。提案已排除标点挤压，Word 的 punctuation compression 不做。

因此：

- `好|好`、`好|，`、`（|好`（全角开括号）可拉。
- `好|A`、`A|好` 可拉（提案：中英交界也算；与 Word 脚本交界一致）。
- `A|B`、`A| `、` |B`（U+0020）**不**作为第二阶段间隙；空格加宽只在第一阶段。
- `です`、`한국어` 词内：两侧皆非汉字/全角标点 → 不拉。
- `　|好`（U+3000 后接汉字）：U+3000 不是可撑开簇，汉字是 → 缝可拉。
- `Ａ|Ｂ`（全角字母）两侧皆非 → 不拉。
- 扩展 B / 兼容表意不是可撑开簇；与汉字相邻时，因一侧是汉字，缝可拉。

设合格间隙有 `M` 个，余量 `R2`：

```text
若 M = 0：不再分配；行宽 = line_width + phase1（右侧可留白）
若 M ≥ 1：每个合格间隙加 R2 / M
```

间隙加宽加在 **左侧簇** 的绘制宽上（与现有 tracking 落在簇末字形同一机制：增大该簇 `draw`，`LinePainter` 把超出 body 的部分加进最后非 omit 字形的 advance）。这样装饰线与链接矩形自动覆盖拉开后的区间。

### 4.5 与字距、装饰、链接的次序

1. 测量与断行（含 §12.1 tracking 参与宽度）。
2. 算 `line_width`、`remainder`。
3. 若 §4.2 命中 → Start 摆放。
4. 否则第一阶段空格 → 第二阶段合格间隙。
5. 用拉开后的逐簇 `draws` 调用 `LinePainter`（装饰线、链接矩形已按拉开后 x 计算）。

行中空格与拉开间隙上的下划线 / 删除线：沿用 §12.1（连续同字号同色合成一条；行尾丢掉的空格不画——那些空格本就不在行上）。

### 4.6 浮点

- 用现有 `Length` / `f32` pt；均分直接除。
- 最后一处拉伸槽用「总应分余量 − 已分配」吃掉舍入残差，使行末贴 `frame_x + frame_width`（在 `remainder > 0` 且至少分出了一份时）。测试可用 `pt_near`，但实现必须贴边。
- `remainder` 非有限：按现有几何错误路径失败（与其它非有限长度一致），不静默当 0。

### 4.7 例子（规范意图）

| 场景 | 期望 |
| --- | --- |
| 多行中文段，`Justify`，中间行 | 汉字间隙被拉开，行末贴框右（在可分间隙足够时） |
| 同段最后一行 | 与 `Start` 同 x，不拉 |
| `"第一行\n第二行"` 的第一行 | 硬换行前 → Start |
| 行内有 `BlankWidth::ToEndOfLine` | 整行 Start；Fill 仍吃满剩余宽 |
| 纯英文多行，行内有空格 | 只拉空格，每空格 ≤ 0.5em；触顶后右侧可留白 |
| 纯英文且一行无空格（极少见，因 AL 内部不断） | 无可拉伸槽 → Start |
| `中A` 交界 | `中|A` 间隙可拉 |
| `Hello` 词内 | `H|e|…` 不拉 |
| 首行缩进 | 内容在缩进后的框内两端对齐；缩进量不参与分配 |
| 带 `tracking` 的 run | 先按字距算行宽，再分余量 |
| 链接跨多个簇 | 链接 `[x0,x1]` 覆盖拉开后整段 |
| 非段落 `ImageSpec { align: Justify }` | 图片按 Start |

## 5. 实现约束

### 5.1 推荐改动落点

1. **`text/line_break.rs`**：`SpanLine` 增加 `ended_by_hard_break: bool`、`has_fill: bool`（名可调整）。`pack_lines` 遇 Hard 收行时置前者；行内出现 `Advance::Fill` 时置后者。
2. **`layout/style.rs` + `spec/mod.rs` + `lower.rs`**：枚举与映射；非段落 Justify→Start。
3. **`layout/paragraph.rs` `place_line`**（或紧邻纯函数）：根据 align / 行标志 / 是否末行计算逐簇 `draws` 增量；再交给 `LinePainter`。
4. **可撑开判定**：抽出 `is_justify_cluster(first_scalar) -> bool`（汉字两段码位 + Op/Cl 中 ≥U+3000）。全角标点与 `break_class` 的 Op/Cl 表同源，避免标点表漂移；**不要**直接调用 `is_id`（`Id` 含假名/谚文/U+3000，宽于 Word 中文两端对齐）。
5. **模块 DAG**：不改动 `spec → backend` 禁边；不引入新依赖。

### 5.2 错误面

本主题 **不新增** 公开错误码。Justify 本身不分失败（吸不满只留白）。既有错误（缺字、负字距、不可断过宽、Fill 后还有 run 等）不变。

### 5.3 测试要求

优先 `RecordingCanvas` / 布局测里的 glyph origin x；`to_pdf` 仅 `%PDF` 冒烟。

至少覆盖：

- 中间行拉开 vs 末行 Start（同段对照）。
- 硬换行前行 Start。
- 含 `ToEndOfLine` 的行 Start，且 Fill 行为不回退。
- 空格封顶 0.5em（构造窄框 + 大余量 + 少空格，断言空格增量 ≤ 0.5×字号）。
- 中英交界可拉；纯拉丁词内不拉。
- 字距之后再分余量（可选：固定 tracking，断言行宽计算顺序）。
- 下划线或链接在拉开后变宽 / 右移（至少一条）。
- `ImageSpec { align: Justify }` 与 `Start` 同位。
- 穷尽 match / 示例编译：`hello` / `bill` 等若 match `Alignment` 则补臂或改 `_`。

夹具：现有 `PDFCRAFT_PDF_FONT` / CJK 环境变量；无字体则跳过的测例保持可跳过。

### 5.4 文档与门禁

- `CHANGELOG.md`：breaking（新变体）+ 行为。
- `crates/pdfcraft-pdf/docs/guide.md`、`README.md`：删除「v1 无 justify」；简述两阶段与 Start 回退行。
- 设计稿 Non-Goals 里「不做两端对齐」视为过时；本主题 **不强制** 大段改设计稿，可在设计稿该条加一句「已由提案书 §12.3 取代」或留给文档整理 PR。
- 提交前跑 AGENTS.md 门禁（至少 `pdfcraft-pdf` 的 module check / fmt / clippy / test / doc）。

## 6. PR 切分建议

单主题一 PR 即可。若 diff 过大可拆：

1. `SpanLine` 硬换行 / Fill 标志 + 枚举 / lower 映射（行为仍等于 Start）。
2. `place_line` 分余量 + 测试 + changelog / guide。

不要把 §12.4 或无关重构塞进本主题。

## 7. 验收清单

- [ ] 提案书 §12.3 与本文一致；未超出 §2.2「不做」
- [ ] `Alignment::Justify` / `TextAlign::Justify` 已导出或按 §3 接线
- [ ] 断行输出不变；仅摆放分余量
- [ ] 第一阶段空格 ≤ 0.5em（按该空格 run 字号）
- [ ] 第二阶段仅合格间隙；英文词内不拉；中英交界可拉
- [ ] 段末行、硬换行前、Fill 行、无槽行、`remainder≤0` → Start
- [ ] 行尾丢掉的空格与首行缩进不参与；字距先于分余量
- [ ] 链接与装饰跟拉开后位置
- [ ] 非段落 `Justify` ≡ Start
- [ ] §5.3 测试覆盖；门禁通过
- [ ] CHANGELOG + guide/README 已更新

## 8. 权威与冲突

1. [`提案书.md`](提案书.md) §12.3（及 §12.1 中与装饰 / 字距交叉的句子）
2. 本文
3. 设计稿未确认或已作废部分（含「v1 不做 Justify」）

冲突时按上序。实现中若发现提案与 Canvas / 断行 / skrifa 事实冲突，停下来改提案或本文，不要 silently 歪实现。

## 9. 本会话已钉的产品细则（相对提案原文的可执行化）

提案原文未写死、经本会话确认后写入上文的：

| 点 | 决定 |
| --- | --- |
| 「汉字或全角标点」 | 可撑开簇 = 汉字（U+3400–U+4DBF、U+4E00–U+9FFF）∪ 全角 Op/Cl（码位 ≥ U+3000）；对齐 Word 中文 inter-ideograph；不含假名/谚文/U+3000；ASCII `()[]{}` 不算 |
| 空格阶段字符 | 仅 U+0020（`PairClass::Sp`） |
| 空格分配 | 二次泼水：在仍有额度的空格间均分，直到余量用完或全部触顶，再进第二阶段（对齐 Word 先词距） |
| 第二阶段与空格 | U+0020 两侧间隙不进第二阶段 |
| 吸不满 | 能拉多少拉多少，右侧留白；不改为 Center |
| 非段落 Align | `Justify` 按 `Start` |
| 硬换行 / Fill 检测 | 断行产物显式标志，不靠启发式猜 |
| 浮点残差 | 最后一处拉伸槽吃残差，行末贴框右 |
