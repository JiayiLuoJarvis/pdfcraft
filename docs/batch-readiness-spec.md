# 执行规格：批量就绪（正确性、限额、信件夹具、性能）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-12 |
| 状态 | **待讨论确认后按工作项落地**。本文是执行规格，未经确认前不改提案书已定语义。 |
| 来源 | 2026-10-12 只读审查（提案对照 / 排版 / 文字链接 / backend）；[`code-quality-assessment.md`](code-quality-assessment.md) |
| 权威 | [`提案书.md`](提案书.md) §1–§9 与确认总表；[`AGENTS.md`](../AGENTS.md) 门禁与「限额与信件夹具通过前 `publish = false`」 |
| 设计稿 | [`quire-design.md`](quire-design.md) PR 18–19 为限额与信件目标；其中未经提案确认的数字，限额工作项实现时再钉进本规格或提案小项 |

## 1. 目标与非目标

### 目标

1. 修掉审查里会伤信件/合同的实现与文档打架（列表编号、`Table::build` 图片、`single_line_run` 换行）。
2. 落地公共 **限额** 与 **信件夹具**，满足解除 `publish = false` 的工程前提（版本仍 `0.0.x`，是否上架另议）。
3. 补一批低成本、高定位价值的错误上下文与测试锁。
4. 在夹具与限额就绪后，用**可复现测量**决定要不要做性能工作项；把候选优化写进同一规格，避免散落口头债。

### 非目标（本规格明确不做）

- 不提前做提案搁置/延后项（章跟流走、某页关 header/footer、流内 canvas、qr/svg/toc、具名 images、加密）。
- 不做完整 UAX #14、bidi、kerning、连字、justify。
- 不为口味做 viewer 视觉回归农场。
- 不在未测热点前「顺手」加字体缓存、流式写出、并行开页（AGENTS 禁止并行开页/画页）。
- 不把 skrifa / krilla 钉版本升级塞进本规格（须单独 PR）。

## 2. 验收总闸

全部满足才算「批量就绪」里程碑完结（性能工作项可标为可选通过）：

- [x] 工作项 A–C（正确性）行为符合下文契约，有回归测试。
- [x] 工作项 D（限额）公共 `limits` 模块存在；超限返回结构化 `ErrorKind`；有单测；分页循环检查页数。
- [x] 工作项 E（信件）`tests/letter_layout.rs`（或等价）+ 带真实 CJK 子集的字体夹具；`RecordingCanvas` 钉关键坐标；`to_pdf` 只冒烟 `%PDF`。
- [x] 工作项 F（诊断与文档）错误片段 / rustdoc / 提案小项对齐完成。
- [x] `cargo fmt` / `clippy -D warnings` / `test`（含 `backend-krilla`）/ `doc --no-deps` 绿。
- [x] **在工作项 D+E 通过之前**，`Cargo.toml` 保持 `publish = false`（仍保留，未开删除 PR）。
- [x] 工作项 P（性能）要么有前后对比数字并落地改动，要么书面记录「测过、不赚、不做」并关掉候选项。

## 3. 工作项顺序（必须按序，可再拆 PR）

```text
A 列表自定义标记编号
  → B Table::build DocToken / 格内图
  → C single_line_run 强制换行
  → D limits
  → E letter fixture（依赖 D；可与 F 部分并行）
  → F 诊断补强 + 文档对齐（可与 E 交错）
  → P 性能（仅在 D+E 之后，且必须先有基准数字）
```

一项一事；最小 diff。未列入的「polish」见 §6，不阻塞 D/E。

---

### 工作项 A — 自定义列表标记不占自动编号

| | |
| --- | --- |
| 现状 | `List::item_marked` rustdoc 写不消耗序号；`layout/list.rs` 用 `start + enumerate` 下标，自定义项占号。`node_tests/list.rs` 把占号锁成 `IV.` 后接 `VI.`。 |
| 目标契约 | 有序表中，仅 **自动生成标记** 的项消耗计数器。自定义标记项 **不** 前进计数器。`ul` 的 `start` 仍忽略。 |
| 改哪里 | `src/layout/list.rs` 编号循环；`src/document/list.rs` rustdoc 与实现对齐；改/补 `node_tests/list.rs` 与 `tests/document.rs`（自定义项夹在两项自动项之间）。 |
| 验收 | 序列「自动 / 自定义 / 自动」、`start = 1` → 标记为 `1.`、自定义串、`2.`（不是 `3.`）。`start = 4` 罗马 → `IV.`、自定义、`V.`。 |
| 非目标 | 不改标记枚举算法、悬挂缩进、跨页不重复标记。 |

---

### 工作项 B — `Table::build` 与父文档 `DocToken` 对齐

| | |
| --- | --- |
| 现状 | `Table::build` 用 `DocToken::next()`；格内 `load_image` 的图在 `ImageBox::lower` 因 `image.doc != flow.doc` → `UnknownImage`。`FlowWriter::table` 的 `build_on` 无此问题。 |
| 目标契约 | 工厂 `Table::build` 产出的表，经 `FlowWriter::add`（或糖）进入某 `Flow` 后，格内已在**同一 `Document`** 注册的图片可用，样式继承与 `FlowWriter::table` 一致（或文档明确写出差异并测死）。**优先重绑父 token**，不要只改文档推责。 |
| 改哪里 | `src/document/table.rs`、`src/document/image.rs`（若需入流时重绑）、相关 `document/tests` / `tests/document.rs`。 |
| 验收 | 辅助函数 `fn make_table(doc: &Document) -> Table` 用 `Table::build` + 格内 `doc` 上 `load_image` 的图，再 `flow.add(table)`，排版成功且 `RecordingCanvas` 有对应 `DrawImage`（或等价命令）。 |
| 非目标 | 不开放跨 `Document` 共用图片；不引入具名 images 字典。 |

---

### 工作项 C — `single_line_run` 遇强制换行

| | |
| --- | --- |
| 现状 | `text_at` / 列表 marker 走 `single_line_run`：BK 从 `glyph_text` 去掉后拼成一行；layer `max_width = INFINITY`。页眉页脚 `TextNode` 会硬断行。 |
| 目标契约 | **禁止静默把含强制换行的字符串画成一行。** 推荐默认：**遇 BK（或原文 `\n` / `\r\n`）返回明确 `ErrorKind`**（名实现时定，如 `UnexpectedLineBreakInSingleLine`），并带 `Location`（layer / band / list 可知处）。备选「拆成多行」仅当产品确认 layer 要多行；本规格默认报错，与「单行 API」名实相符。 |
| 改哪里 | `src/layout/paragraph.rs`（`single_line_run` 入口校验）、`decorate.rs` / `list.rs` 调用点、测试。 |
| 验收 | `text_at("A\nB", …)` 与含 `\n` 的自定义 marker 均失败且画布为空；无 `\n` 的路径行为不变；页眉多行文本路径不回归。 |
| 非目标 | 不给 `text_at` 做完整段落断行引擎；不改 §3 断行分类表。 |

---

### 工作项 D — 公共限额模块

| | |
| --- | --- |
| 现状 | 无 `src/limits.rs`；字体/图片字节与像素为私有常量；像素检查在 `from_*` 之后；无页数/块数等。 |
| 目标契约 | 公开（或 `pub` 可配置）`Limits` 默认值；在分页前/组装中执行；超限 → 结构化错误（如 `ErrorKind::LimitExceeded { … }`），整份失败。至少覆盖设计稿 PR 18 方向且实现时钉死的集合：**字体字节、图片字节、图片像素、页数**；块数/单元格/段落字节若本工作项一并做，须各有单测。像素与边长检查尽量在**完整解压占内存之前**失败（若 krilla API 做不到，规格里写明「尽力」与测试能钉到的最早点）。 |
| 数字 | 提案总表未钉具体数字。实现 PR 对照 `quire-design.md` 限额段起草默认值，在本文件「落地记录」表补齐最终数；改默认值须单独说明。 |
| 改哪里 | 新建 `src/limits.rs`；`document` / `paginate` / `fonts` / `backend/images` 接线；`lib.rs` 重导出；`CHANGELOG`。 |
| 验收 | 每个限额至少一测；故意超页数在分页循环失败；`publish = false` **仍不删**（等工作项 E）。 |
| 非目标 | 不删 `publish = false`；不做可热更新远程策略；不引入异步。 |

---

### 工作项 E — 信件夹具

| | |
| --- | --- |
| 现状 | `quire-measure.ttf` 几乎只有 ASCII +「第/页/共」；示例是 hello/bill，无询证函式信件 layout 测。 |
| 目标契约 | 增加 OFL（或同等可再分发）**含真实 CJK 子集**的字体夹具。集成测试至少覆盖：两段中文正文、中英混排一句、表（含表头）、一张图、页脚全文页码；可选 section 双页码。断言以 `RecordingCanvas` 坐标为主；`to_pdf` 仅 `%PDF` 冒烟。 |
| 改哪里 | `tests/fixtures/`、`tests/letter_layout.rs`（名可微调）、fixtures README。 |
| 验收 | 测试稳定绿；夹具授权与来源写进 README。**本工作项通过后**，才允许另开「考虑去掉 `publish = false`」的讨论/PR（仍保持 `0.0.x`）。 |
| 非目标 | 不自动化人工视觉审阅；不断言 krilla 压缩内容流数字。 |

---

### 工作项 F — 诊断补强与文档对齐（可与 E 交错）

按优先级，可拆多个小 PR：

| 项 | 契约 |
| --- | --- |
| F1 错误片段 | `MissingGlyph` / `UnbreakableWiderThanFrame`（及工作项 C 新 kind）的 `Display` 或结构化字段带**短文本片段**或 scalar；`Location` 能填的继续填。 |
| F2 坏图时机 | `bad-idat`（或等价）尽量在 `load_image` / 注册期失败；若必须延到 `to_pdf`，rustdoc 写明，且 `at` 尽量带页。 |
| F3 提案/API 小项 | 栏内禁 `page_break` 写进提案或 element-repair 权威句；§2.12 旋转范围（仅 path vs 含 text/image）写清；公开 `stack` = 样式作用域；`Page::is_last` = 全文末页。 |
| F4 死导出 | `DocumentInfo` 接线或停止根重导出；`Border` 可构造或停止导出。 |
| F5 Track | `Track::Fixed(Length<Positive>)` 等与栏宽 API 对齐；非法 Ratio 构造期拒绝（见 [`element-repair.md`](element-repair.md)）。 |

**公开 `paragraph_spacing` 之前**必须先修：`keep_together` / 单元格 `natural_height` 路径不得静默丢掉组内段后距（`flow.rs` `split_stack`）；修时补「A+spacing+B 超页 → `UnsplittableTallerThanFrame`」测试。该修可挂在 F 或单独工作项 **F0**，阻塞「公开段后距」而**不**阻塞 D/E。

---

### 工作项 P — 性能（连带写入本规格；执行靠测量门）

性能项**允许写进本规格**，但**禁止**与 A–C 混在同一 PR，也**禁止**在工作项 E 之前以「顺手优化」落地。

#### P0. 基准（落地任何 P 改动之前）

| | |
| --- | --- |
| 产物 | `benches/` 或 `tests` 门控的可复现脚本 + 一段「落地记录」数字（机器、commit、墙钟、峰值 RSS 量级）。 |
| 样张 | 至少：工作项 E 信件 ×N 页；合成「宽表」与「多段落」各一份（规模在 limits 默认值以内）。 |
| 指标 | 墙钟（`document`+`to_pdf` 可拆开）；可选峰值内存。 |
| 门槛 | 未写出 P0 数字，不得合入 P1–P4 行为改动。 |

#### 候选改动（按预期收益排序；测完可砍）

| ID | 改动 | 动机（审查） | 验收 |
| --- | --- | --- | --- |
| P1 | `plan_pages` / `split_stack` 的 `Vec::remove(0)` → `VecDeque`（或索引游标） | 根上大量块时 O(n²) 搬移 | 行为测全绿；P0 样张墙钟有可解释改善或证明无改善则关闭项 |
| P2 | 测量路径缓存 `FontRef` / charmap（或按 face 复用 parse） | 每段 `measure_text` 都 `parse_face` | 同字体多段样张加速；缺字/粗体语义不变 |
| P3 | 绘制路径复用 krilla `Font`（每 `GlyphRun` 勿盲目 `Font::new`） | `emit.rs` 热点候选 | glyph id / 子集行为不变；有前后数字 |
| P4 | 表跨页：避免无必要的整表 `Fragment` 深克隆（表头 `cloned`、glyph 再克隆） | 大表峰值内存与 CPU | 跨页表坐标测不回退；内存指标有记录 |

#### 明确延后（本规格记录，默认不做）

- 整文档流式写出 / 按页释放 `RecordingCanvas` 命令（架构大；合同体量不刚需）。
- 合并进程内两份 skrifa（钉版本单独 PR）。
- 并行排版或并行画页（AGENTS 禁止）。

#### 性能工作项原则

- 一次一热点；证明来自 P0，不是感觉。
- 行为契约不变：坐标测、缺字整份失败、链接矩形规则不变。
- 改善写 `CHANGELOG`（Changed / Performance）；无改善则在本文件落地记录写「关闭」原因，删掉待办，避免假债。

## 4. 与提案书 / AGENTS 的关系

| 点 | 处理 |
| --- | --- |
| 产品语义已确认者 | 实现跟提案；本规格只钉执行契约与工作项顺序 |
| 栏内 `page_break`、§2.12 旋转范围、marker 计数器 | 工作项 F 把权威句补进提案或修理文档，避免代码与文两套 |
| `publish = false` | 仅工作项 D+E 通过后才能开「删除」PR；删除本身不是本规格自动步骤 |
| 预置边距枚举、hline 最终名、A 系列列表 | 已有实现可记 polish，不阻塞本里程碑 |

## 5. PR 纪律

- 标题写工作项编号（例：`fix(list): custom markers do not advance counters (A)`）。
- 每个工作项更新本文件「落地记录」勾选与日期。
- 用户可见行为写 `CHANGELOG.md`。
- 门禁命令见 `AGENTS.md`；有 backend 时测 feature。

## 6. Polish 清单（不阻塞 D/E，可选穿插）

- 表外框半线宽侵入页眉页脚带：要么盒子计入外半线，要么文档接受并加注释级测试说明。
- `StalledOnFreshPage` 与设计稿命名是否合并（纯错误 kind 清理）。
- 组合测：整表挪页 + rowspan + 表头重复；多 section 双页码；`keep_together` 内嵌 `PageBreak`。
- backend 适配器测：颜色 alpha → opacity；图片 `cm` 几何（仅 backend 测，不断言生产内容流细节到 layout）。
- builder 三种返回形态收敛、prelude（1.0 前再谈）。

## 7. 落地记录

| 工作项 | 状态 | 日期 | 备注 |
| --- | --- | --- | --- |
| A 列表标记编号 | 完成 | 2026-10-12 | 自定义标记不消耗序号 |
| B Table::build token | 完成 | 2026-10-12 | `add` 时重绑父 `DocToken`；样式仍是 build 时的默认样式 |
| C single_line_run 换行 | 完成 | 2026-10-12 | `ErrorKind::UnexpectedLineBreakInSingleLine`；`text_at` 与自定义 marker 遇 BK 失败且不画 |
| D limits | 完成 | 2026-10-12 | 默认 font bytes 与 image bytes 各 32 MiB，image pixels 40_000_000，image edge 8192，pages 10_000，blocks 100_000，table cells 50_000，paragraph bytes 1_000_000。setter 拒绝 0，也拒绝高于默认 4 倍的值。像素与边长在 krilla 解码出尺寸之后、句柄入库之前检查，krilla 没有只读头。块、单元格和段落字节在分页前统计正文流，堆叠容器不计块，页眉、页脚和 layer 不计入。页数在 `plan_pages` 提交每一页时检查。图片像素和边长并入 `LimitExceeded`，不另设 `ImageTooLarge`。 |
| E letter fixture | 完成 | 2026-10-12 | `tests/fixtures/quire-letter.ttf` 是 Noto Sans SC Regular 的 OFL 子集，族名 QuireLetter，23456 字节。`tests/letter_layout.rs` 用 `document` 排确认函。两段中文、中英混排、表头、一张图、全文页码页脚。坐标钉在 `RecordingCanvas`。`to_pdf` 只断言 `%PDF`。`publish = false` 仍保留。 |
| F 诊断/文档 | 完成 | 2026-10-12 | F1 三条错误带短文本或 scalar。F2 坏 PNG 在 `load_image` 失败。JPEG、GIF、WebP 的像素错误仍可能在 `to_pdf` 成为没有页码的 `Backend`。F3 提案书写明栏内禁止 `page_break`、旋转只包 path、`stack` 是样式作用域、`Page::is_last` 是全文末页。F4 根上不再导出 `DocumentInfo` 和 `Border`。F5 `Track::Fixed(Length<Positive>)`，`Ratio::new` 拒绝非法权重。 |
| F0 keep_together 间距 | 完成 | 2026-10-12 | `natural_height` 把组内段后距算进高度。A 与 B 两行正好放下、中间段后距放不下时，`keep_together` 和单元格报 `UnsplittableTallerThanFrame`。组尾段后距仍不进组高。分页里放不下的段后距仍丢掉。未公开 `paragraph_spacing`。 |
| P0 基准 | 完成 | 2026-10-12 | 可复现入口：`cargo test --release --features test-utils --test batch_baseline -- --ignored --nocapture`。机器 `ubuntu1710`，`Linux 6.8.0-139-generic x86_64`，基线 tip `f0b6139`。含 P1 的 P0 样张墙钟（release）：`letter_x8` layout 6.28 ms / to_pdf 6.15 ms / 8 页 / peak RSS 约 6.5 MiB；`wide_table_40x12` layout 2.74 ms / to_pdf 4.28 ms / 1 页；`paragraphs_400` layout 7.88 ms / to_pdf 12.06 ms / 10 页 / peak RSS 约 8.1 MiB。 |
| P1 VecDeque | 完成 | 2026-10-12 | `plan_pages` 与 `split_stack` 的 `Vec::remove(0)` 改为 `VecDeque`。决策样张 `paragraphs_5000`（同机 release）：改前 layout 135.04 ms，改后 115.92 ms（约 −14%）。行为测全绿。 |
| P2 FontRef 缓存 | 关闭 | 2026-10-12 | P0 信件与 400 段样张 layout 已在十毫秒量级。未做字形路径剖析；合同体量不刚需。有热路径数字后再开。 |
| P3 krilla Font 复用 | 关闭 | 2026-10-12 | 同上。`to_pdf` 在 P0 样张上不是秒级热点。 |
| P4 表跨页克隆 | 关闭 | 2026-10-12 | `wide_table_40x12` 单页、墙钟与 RSS 都低。无跨页大表峰值证据前不做。 |

## 8. 一句话

先修会错编号和错图的工作项，再上限额与信件夹具；性能写在同一规格里，但必须先有基准数字，否则不算本里程碑的完成条件。
