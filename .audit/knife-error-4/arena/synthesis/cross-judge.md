# 刀 4 交叉评审：C1（bag 为中心） vs C2（边界返回值收集）

结论：**以 C1 为底稿**，从 C2 移植 soft 类型约束和若干装配细节。总分 C1 25 / C2 21。

C2 的 soft/hard 语义更严（soft 不产出占位字形，用类型区分），但它在 layout 侧的落地有一个没写出来的大洞：`Measured` 要穿过所有容器的 `split`。C1 的形状更贴合现有代码，而且收集策略（上限、顺序、计数、补位置）只由一个类型持有。

核对过的现状代码：`LayoutCtx { fonts }` 已经传进每个容器的 `split`（`layout/frame.rs:12`、`flow.rs:56-62`、`table.rs:204`）。`TextNode::split` → `layout_runs` → `measure_inline(fonts, …)` → `measure_text(fonts, …)` 目前只拿 `fonts`。表格单元格在 `table::measure` 里只测一次，之后以值的方式移动，`stacked_height` 续排的是已经断好行的 `ParagraphNode`。`FlowWriter::spacer` / `page_break` / `keep_together` / `stack` 直接调用 `Flow::push(Result)`，或者写 `flow.error`。

---

## 1. 分项打分

| # | 标准 | C1 | C2 |
| --- | --- | --- | --- |
| 1 | 遵守刀 4 已确认条目 | 4 | 4 |
| 2 | 接口深度 | 5 | 4 |
| 3 | 对 agent 的可维护性 | 4 | 3 |
| 4 | 贴合现有代码 | 4 | 2 |
| 5 | soft / hard 清晰度 | 4 | 5 |
| 6 | 避开红旗 | 4 | 3 |
| | **合计** | **25** | **21** |

### 1. 遵守已确认条目（C1 4，C2 4）

两份都做到了：`Error` 保持单数；`Errors` 非空、`Clone`、`PartialEq` 按 kind+at 比较、多行 `Display`；上限 64 加截断标记；按发现顺序；容器不再覆盖子节点的错误；兄弟节点继续 lower 但被排除出成功树；有任何错误都不产出 `Rendered`；backend 保持单个错误；`error` 模块不新增依赖；没有第二个取错误的 API。

- C1 扣分：单原因的公共调用（geom 构造器、`Document::font`、`load_image`、`Canvas`）返回 `Result<T, Error>`，公共面上于是有两种错误类型出现在 `Result` 位置。第 3 点写的是"公共 `Result<T, Errors>`"，这里是擦边，需要拍板（C1 自己也列成了开放问题）。
- C2 扣分：`Errors::len()` 是公共方法却没有 `is_empty`，在 `-D warnings` 下会触发 clippy 的 `len_without_is_empty`。geom 改成返回 `Errors`，但内部到处用的 `crate::error::Result` 怎么处理没写（见第 4 项）。
- 两份共同的缺口：第 64 条之后的硬错误会被上限丢掉，"为什么停下"可能不在列表里（见风险 1）。

### 2. 接口深度（C1 5，C2 4）

- 公共面两份都很小。C1 是 `first / as_slice / iter / truncated` 加标准 trait，刻意不给 `len()`。C2 多了 `len()` 和公共的 `MAX_ERRORS`。
- 内部概念：C1 是 `ErrorBag`、`Mark`、`Fallible` 三个；C2 是 `ErrorList`、`Lowered`、`Assembled`、`Measured`、`FontFault` 五个。另外 C2 的 `Lowered::Drop` 内部装的是公共封装类型 `Errors`，因此还需要一个 crate 内部的解封方法 `append_errors`。两份都没有把收集类型泄漏到公共面。

### 3. 对 agent 的可维护性（C1 4，C2 3）

- C1：上限、顺序、`seen` 计数、`absorb`、`locate_since` 全在 `ErrorBag` 一处。容器的 `lower` 只有"mark → absorb → 收集 → clean_since"一种写法，结构上没法再"选自己的错误还是子节点的错误"。用 `seen` 而不是 `items.len()` 判断 `clean_since`，意味着 bag 满了以后容器仍然知道子节点失败过，考虑得周到。
  - 扣分：`ErrorBag::record` 什么错误都收。agent 可能在 layout 里把一个几何错误也 `record` 了然后继续跑，soft 集合只是靠"三个调用点"的约定维持。
- C2：类型系统强制在 soft 和 hard 之间做选择，`FontFault` 的构造器就是 soft 集合的唯一清单，这一点比 C1 强。
  - 扣分：每个容器的 `split`（table、columns、list、keep-together、stack）都要自己合并子节点返回的 `Measured::Soft`，合并策略分散到 N 处。agent 改其中一个容器时，很容易在遇到 `Soft` 时提前返回，等于把首错优先请回来。`soft_scan` 又是第二个遍历 FlowNode 树的入口。

### 4. 贴合现有代码（C1 4，C2 2）

- C1：给 `LayoutCtx` 加一个 `errors: &mut ErrorBag` 就能顺着现有的 `split(ctx, …)` 到达所有容器，`measure_inline` / `layout_runs` 只需从拿 `fonts` 改成拿 `ctx`。把 `paginate_sections` 拆成 `layout_pages` / `paint_pages`，现有的 `paint_pages` 已经独立存在（`paginate.rs:278`）。内部 `Result` → `Fallible` 的改名是全仓库机械改动，没有行为变化，可以单独作为第一个提交。
  - 扣分：§5.2 的 `Flow::nested(&self, into: &mut ErrorBag, …)` 在调用方写成 `flow.nested(&mut flow.errors, …)` 时会借用冲突，编译不过。要改成 `&mut self` 内部 absorb，或者改成关联函数。
- C2：装配侧对现有代码非常熟（`Band::inset` / `Layer::line` 的双边错误、`RunAfterFillBlank` 合并、Columns 级联、跳过闭包的清单）。
  - layout 侧：只给了 `measure_text`、`measure_inline`、`plan_sections` 的签名。但 soft 错误来自表格单元格、栏、列表项里的段落，要冒上去就必须改掉 `Flowable::split -> Result<Split<Self>>` 以及 `natural_height` / `stacked_height`，变成 `Measured<Split<Self>>`。设计里没提这一点，C1 的备选方案恰好把它列为否决理由。
  - 内部 `Result` 别名指向 `Errors` 之后，现有的 `map_err(|e| e.in_region(..).on_page_index(..))` / `in_cell` 链也全部要改；"给一个 `Errors` 里的所有条目补位置"这种操作没有设计（`ErrorList` 只有 `stamp_section`）。

### 5. soft / hard 清晰度（C1 4，C2 5）

- C2：`Measured::Soft` 不带值，缺字不产出 `MeasuredText`，也就不可能被放上页面。hard 错误逐种列明；body 出现 soft 后不跑 bands；`Keep` 是唯一会构造 `RecordingCanvas` 的路径。
- C1：soft 集合正好是三种。用 `.notdef` 或正文字体替身继续测量；只要 bag 非空就不 paint，所以替身到不了 PDF。硬错误通过 `errors.fail(hard)` 先记录再停，backend 保持单个错误。
  - 扣分：替身的 advance 会改变断行，有可能制造出假的 `UnbreakableWiderThanFrame`，让一个本来不存在的几何错误把运行硬停下来。另外 bag 不干净时页码是按"有洞的树"算出来的。

### 6. 红旗（C1 4，C2 3）

- C1：保留的 `in_*` 包装方法和 `located` 是同一规则的两种写法（自己已标出，实现只有一份 `fill_missing`）。Error 与 Errors 并存于公共面，有"两种做法"的味道。其余项都过关。
- C2：
  - `soft_scan` 加 `plan_sections` 是两个遍历 FlowNode 的入口，属于 hand-synced 风险。好在 `match` 写全、不用通配分支的话，编译器能兜底。
  - 合并 `Soft` 的逻辑分散在各容器里，属于 split ownership。
  - `Lowered::Drop(Errors)` 加 `append_errors` 解封，等于内部有两种表示同一个列表的方式。

---

## 2. 推荐底稿：C1

1. **一处持有策略。** `ErrorBag` 是唯一拥有上限、顺序、计数、合并和补位置的地方。容器 `lower` 的模板让覆盖 bug 在结构上消失，且不依赖每个容器各自写对。
2. **沿用现有通道。** `LayoutCtx` 已经到达每个 `split`。C1 不用改 `Flowable` 的签名；C2 必须改，而且没写出来。
3. **位置只有一个实现。** `Mark` 加 `locate_since` 加 `LayoutCtx::located` 用同一套代码给 soft 和 hard 错误补位置，深层先补、外层只填空白，和现在 `Location` 的"内层优先"一致。C2 对嵌套 soft 错误的位置没有给出方案。
4. **迁移顺序可行。** 改名（无行为变化）→ `ErrorBag` 单元测试 → 装配侧换 bag → `LayoutCtx` 加 bag → 拆 render。每一步都能单独过门禁。

C2 的长处是 soft/hard 的类型化边界，可以用下面第一条移植以较低成本拿到，不需要接受它在 layout 侧的签名扩散。

---

## 3. 值得从 C2 移植的部分

1. **用类型约束 soft 集合（最重要）。** layout 侧不直接暴露 `&mut ErrorBag::record`，改为 `LayoutCtx::soft(FontFault)`，`FontFault` 只有 `missing_glyph` / `bold_face_required` / `unknown_font` 三个构造器。所有其他错误只能走 `Fallible` 的 `?`。这样"三种 soft"从约定变成编译期清单，也补上 C1 第 3 项的扣分点。装配侧的 bag 仍可以通用 `record`。
2. **明确的 hard 清单作为测试表。** 把 C2 §Soft measure 里逐种列出的 hard kind（`UnbreakableWiderThanFrame`、`UnsplittableTallerThanFrame`、`InvalidGeometry` 的各个子类、`StalledOnFreshPage`、`InvalidCanvasState` 等）做成一张"记录后停止"的表驱动测试。
3. **装配细节**（C2 写得比 C1 具体）：
   - 容器子节点的错误**在子调用发生时**就记录，不在 `lower` 开头补；全网格检查（span、列表标记、`ColumnCount`）在 `lower` 时追加。
   - `tracks()` / 列宽在数据完整时就校验，不在 `lower` 时重放到前面。
   - Columns：某列宽度失败时不再级联出 `ColumnCount`；`gap` 只记自己的错误，不占列槽。
   - `Band::inset` / `Layer::line` 两边都失败时，先左后右记两条；删掉 `(Err(e), _) | (_, Err(e))` 写法。
   - `RunAfterFillBlank` 不再合并。
   - "仍然不运行的闭包"清单保持不变（空链接 URI、非有限 rotated、宽度错误的 `col_fixed` / `col_ratio`、style apply 失败的 `stack` 等）。
   - `FlowWriter::stack` 里 `if flow.error.is_none()` 的分支一并删掉。
4. **`ContentOutsideSection` 的锁存标志放在拒收的 flow 上**（每个拒收 flow 记一次，之后被拒的 push 仍合并子节点自己的错误）。这比 C1 的"整个文档一次"更精确。两者择一，需要拍板。
5. **C2 的 Worked traces 直接作为集成测试**：脏列表加干净兄弟的缺字、缺字后跟不可断行、`StalledOnFreshPage`、`to_pdf` 只有一个错误。外加 C2 的 `ColumnCount` 后跟 `InvalidCellSpan(EmptySlot)` 兄弟用例（替换今天的 `first_builder_error_wins_and_later_calls_are_ignored`）。
6. **`Display` 截断行文案**用 C2 的 `further errors omitted`，或者统一成 C1 的写法，但只能有一种。
7. 可选：公共 `MAX_ERRORS` 常量便于调用方和测试断言。会多一点公共面，可以先不加。

不移植的部分：`Measured` 穿过 `split`、`soft_scan` 第二遍遍历、`Lowered` / `Assembled` 枚举、公共 `len()`。

---

## 4. 风险与开放问题

1. **硬错误被上限吞掉。** bag 已满时 `fail(hard)` 只把 `seen` 加一，停止原因不在列表里，两份设计都有这个问题。可选做法：给硬停止留一个槽（实际收 63 条 soft 加 1 条 hard），或者接受现状靠 `truncated` 提示。需要拍板，因为第 5 点只说了"上限 64"。
2. **公共单原因调用返回 `Error` 还是 `Errors`**（C1 开放问题 1，对照第 3 点"公共 `Result<T, Errors>`"）。倾向：geom 等保持返回 `Error`，靠 `?` 和 `From` 桥接，并在提案书或文档里写明规则；否则 `Positive::pt(-1.0).unwrap_err().first()` 很别扭。
3. **替身字形引发假几何错误。** C1 用 `.notdef` 替身继续断行，可能制造假的 `UnbreakableWiderThanFrame`。缓解办法有几种：(a) 测量缺字时用 advance 0；(b) bag 里已经有 soft 错误时，几何硬错误照常记录但在 `Display` 里不作区分（就是现状）；(c) 采用 C2 规则，soft 段落不排版、不推进 y。需要拍板，并加一个"缺字的长单词恰好在边界"的测试。
4. **装配有错时页码不可信。** C1 在有洞的树上继续排版，页码会偏移；C2 给 `page: None`。倾向：保留 C1 的做法，在文档里说明"装配有错时页码仅供参考"，或者只在装配干净时填页码。
5. **没注册任何字体时会刷满 64 条 `UnknownFont { id: 0 }`**（C1 开放问题）。建议同一 `FontId` 的 `UnknownFont` 只记一次，或者首条后硬停。两种都属于对"不去重"规则的例外，需要拍板。
6. **"只测一次"不变量。** 现在靠 `TextNode` 按值移动、`TableState::Measured` 缓存来保证（已核对 `table.rs:204-260`）。band 每页重新执行属于预期内的重复。需要加一个"跨页段落缺字只出现一次"的计数测试来守住它。
7. **`Flow::nested` 签名借用冲突**（C1 §5.2），实现时改成 `&mut self`。
8. **`section` 改为返回 `()`** 是破坏性改动，两份一致。0.x 加 `publish = false` 可以接受，但要写进 CHANGELOG 并同步 examples / README。
9. **调用方 build 返回 `Err` 后是否继续测量。** C1 停止；C2 仍然跑 `soft_scan` 并把结果追加在调用方的 `Err` 之后。倾向 C1：调用方要求停就停。
10. **`Errors::source()`**：两份都返回 `None`。单个 backend 失败时会丢掉 krilla 的错误链。可以考虑当只有一条时委托给 `first().source()`。
11. **`PartialEq` 包含 `truncated`**：两份都这么做，超出第 7 点字面（只提了逐条 kind+at）。倾向保留，并在提案书里补一句。
