# pdfcraft-orgchart 代码与架构规范审查

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-25（复核修订版） |
| 性质 | 只读静态审查。不改源码，不改行为合同，未执行 `cargo` 编译或测试 |
| 范围 | `crates/pdfcraft-orgchart/`（`src/`、`tests/`、`examples/`、本 crate `Cargo.toml`，共 59 个 `.rs` 文件） |
| 对标 | `docs/rust-engineering-standard.md`；`AGENTS.md`；`docs/workspace-crates-spec.md`；`docs/module-dependency.md`（orgchart 节）；`docs/quality-benchmarks.md`（orgchart 条）；`docs/pdfcraft-orgchart-structure-spec.md` |
| 合同参考 | `docs/pdfcraft-orgchart-spec.md` 与 `docs/提案书.md` §13（仅判定越权与公共类型泄漏） |
| 工作树 | 审查时 `crates/pdfcraft-orgchart` 无未提交改动 |

---

## 1. 架构裁决

当前是**干净分层，带局部软失败与结构收口残留**，不是字典式混层遗留。

`structure-spec` §12.1 至 §12.3 主线已落地。公共面收敛到 `src/lib.rs:96-103`，保持 `#![forbid(unsafe_code)]`（`src/lib.rs:79`），无 `#[doc(hidden)]`，无 `krilla` / `skrifa` 类型泄漏。`graph` 负责拓扑与折叠，`font` / `page` 负责测字与建框，`layout` 负责几何放置与走线，`paginate` 负责分页与搜索，`render` 负责 PDF 绘制。依赖满足 `ARC-1` 与 `DEP-2`（`Cargo.toml:29-34` 不依赖 `pdfcraft-pdf`、`pdfcraft-forms`、`pdfcraft-post`）。

残留债集中在五类问题。第一类是渲染出口无错误通道的静默跳过与布局内部不变量软跳过。第二类是 52 处生产 `HashMap[k]` 无 `PAN-3` 登记。第三类是浮点 `to_bits` 键与隐式浮点相等。第四类是缺环境字体测试静默返回（覆盖唯一锁住 PDF 字节确定性的测试）。第五类是框表、`EdgeId`、`subject_protocol_id` 等未收口结构债。

```mermaid
flowchart TB
  req["ChartRequest (request.rs)"] --> chart_new["Chart::new (chart.rs)"]
  chart_new --> graph["graph: validate + fold + recheck (NodeId / EdgeId arena)"]
  font_in["Font bytes"] --> font_parse["Font::new / from_shared (font/mod.rs)"]
  graph --> chart["Chart"]
  chart --> to_pdf["Chart::to_pdf (chart.rs)"]
  font_parse --> to_pdf
  to_pdf --> content["content::display_of (NodeDisplays / RatioFact)"]
  to_pdf --> metrics["FontMetrics (shared Arc + glyph cache)"]
  content --> boxes["page::build_node_boxes"]
  metrics --> boxes
  boxes --> paginate["paginate::Session (&mut self) + family PageOracle"]
  metrics --> paginate
  paginate --> layout["layout::WrappedEngine (ranks -> groups -> place -> route)"]
  layout --> paginate
  paginate --> cues["paginate/cues (absorb_cue_slots -> finalize_pages -> PageSpec::check)"]
  cues --> render["render (krilla draw_glyphs + links)"]
  render --> pdf["PDF Vec<u8>"]
```

模块 DAG 经静态检索全量核对，与 `docs/module-dependency.md` orgchart 节一致。`graph`、`font`、`content`、`page`、`layout` 不指向 `paginate`、`render`、`chart`。`paginate` 不指向 `render`。`paginate/family` 不指向 `paginate/session`。`use krilla` 仅出现在 `render/{mod,chrome,marks,paint}.rs`，文字绘制仅用 `draw_glyphs`，无 `Surface::draw_text`。

---

## 2. 规范违规表

| 规则号 | 位置 | 现象 | 为何违规 | 正解方向 |
| --- | --- | --- | --- | --- |
| TRK-6 / PAN-1 | `render/marks.rs:33-40,44-58`；`render/mod.rs:91-136`；`page/model.rs:253-321` | 绘制 cue、leader、link 时遇缺失 slot、节点或 `rect_pt` 为 `None` 静默跳过；`PageSpec::check` 校验了 `cont_cues ↔ layout.slots`，`cues.rs:158-161` 在构造期拦截了缺锚点，但 `PageSpec::check` 未校验 `link.node_id ∈ layout.nodes` | 渲染出口无错误通道，链接节点失配会被静默丢弃 | 在 `PageSpec::check` 补齐 `slot.anchor_id` 与 `link.node_id` 属于 `layout.nodes` 的校验；渲染层缺项即返 `LAYOUT_FAILED` |
| TRK-6 / PAN-1 | `render/paint.rs:133-135,142-144,158-160,172-174,181-183,218-220`；`render/marks.rs:135-142,241-243,278-280` | `round_path` 或 `PathBuilder::finish` 返回 `None` 时直接 `return` 或取消裁切 | 几何退化或非有限坐标会被静默画成空白框或溢出文字 | 绘制原语改为返回 `Result<(), Error>`，路径构造失败映射为 `LAYOUT_FAILED`（保留 `len < 0.01` 零长箭头保护） |
| TRK-6 / PAN-1 | `layout/route.rs:111-113,125-138`；`layout/place.rs:262-266`；`layout/ranks.rs:24-35`；`paginate/family/score.rs:99-101` | 走线缺端点 `continue`；缺框用默认 `node_width_mm` 算 x（同文件 `place.rs:693,696,726,729` 直接索引 panic）；`boxes` 中不在非空 `node_order` 内的框被静默丢弃且 `node_order` 未去重；`sparse_risk` 缺 `cut.task` `continue` | 内部不变量遭破坏时以静默跳过或默认值掩盖；宽高策略分裂 | 缺端点、缺框、缺 task 改为返 `LAYOUT_FAILED` 或由类型消除；`ranks::prepare` 拒绝或补齐未列于 `node_order` 的框并去重 |
| PAN-3 / PAN-8 | `graph/fold.rs`、`font/mod.rs`、`layout/{mod,ranks,groups,place,route}.rs`、`paginate/family/{mod,head,chain}.rs`（共 10 文件 52 处） | 生产代码直接 `HashMap[k]` 索引 | 未开启 `clippy::indexing_slicing`，绕过 `unwrap_used` / `expect_used` 门禁，缺键直接 panic 且无 PAN-3 理由 | 按 `NodeId` / `EdgeId` 改为下标对齐 `Vec`；确实已证明的不变量处改 `get().expect(...)` 并按 PAN-3 登记 |
| DET-3 / TRK-3 / TRK-10 | `layout/route.rs:26,66,84,284,334`；`graph/fold.rs:190-196`；`units.rs:160-169` | `y.to_bits()` 作总线行键；`BusSpan` derive `PartialEq` 隐藏 `f64 == f64`；`exact_sum` 用 `to_bits()` 绕过 `clippy::float_cmp`；`GeometryCache` 用 5 元组 `f64::to_bits()` 作键 | 禁止浮点位模式当键、禁止隐式直接浮点相等、禁止改形规避 lint | 总线改用有限值 newtype 或行号索引；`exact_sum` 显式标注 `#[expect(clippy::float_cmp, reason = "Shewchuk exact sum")]`；`GeometryCache` 键收为 `Orientation` 枚举 |
| DET-1 | `paginate/session/mod.rs:428` | `PageSpec.boxes` 来自 `HashMap::values()` | 迭代顺序不确定，下游 `cues.rs:379` 按该序调用 `write_overflow_line` / `ensure_glyphs`（当前是否影响 PDF 字节尚未实测） | 按 `order` 或 `NodeId` 顺序收集 `PageSpec.boxes` |
| TST-6 | `tests/to_pdf.rs:33,66,126`；`tests/page_observations.rs:25-30,199,238,262`；`tests/stroke_color.rs:89`；`src/paginate/chain_split_tests.rs:74`；`src/font/font_tests.rs:86,107,118,133` | 5 个文件共 12 个用例在缺环境字体时裸 `return`（另 7 处遇 `MissingGlyph` 裸 `return`），包含唯一锁住字节确定性的 `tests/to_pdf.rs:125` | 无字体环境变量时测试套全绿却未执行真实字体与确定性锁测 | 抽共享测试辅助，缺字体显式打印跳过原因并区分缺字与断言失败；CI 配置字体覆盖确定性测试 |
| structure-spec §11 | `src/paginate/chain_split_tests.rs:60-112` | 真实字体锁测仅走 `paginate_with` 加 `FakeEngine` 验证 `promote_to_page_subject` | 未覆盖生产 `Chart::to_pdf` / `WrappedEngine` 与 `overflow_box` 折行对照 | 补齐真实字体下的生产 `to_pdf` 升格与 `overflow_box` 对照测试 |
| DOC-4 / ERR-7 / PY-22 | `src/error.rs:9,31` | 公共 `ErrorCode` 与 `as_str` 文档写 `It matches Python EquityErrorCode.value` | 公共文档暴露迁移来历并以对齐 Python 作理由 | 改为描述稳定协议码字符串合同，删除 Python 引用 |
| TRK-6（死分支） | `paginate/cues.rs:418-424` | `require_numbered_line_fits` 缺 slot 时 `continue` | 紧随其后的 `cues.rs:406` `spec.check()?` 已拦截缺 slot，该分支走不到，属 `structure-spec §7:222` 点名待删代码 | 改为缺槽即返 `LAYOUT_FAILED` 或依赖前置校验后移除 `continue` |
| API-2 / structure-spec §4,§5 | `paginate/family/mod.rs:46,72,167,222-223`；`paginate/session/assemble.rs:112`；`graph/arena.rs:19,39` | `FamilyEdge.id` 与 `AttachmentTask.edge_ids` 用裸 `usize`；`NodeId/EdgeId::from_index` 为 `pub(crate)` | 类型无法区分边 id 与普通索引；arena 外可伪造 id | 改用 `EdgeId`；清理 `content/lines.rs:287` 后把 `from_index` 收窄为 `pub(super)`（测试下保留 `#[cfg(test)]`） |
| TRK-2 / CONV-3 | `layout/groups.rs:8,35,43,116,123,160`；`paginate/session/mod.rs:470`；`graph/check.rs:443`；`layout/ranks.rs:81`；`paginate/family/head.rs:263` | `order_index` 缺键用 `usize::MAX` 哨兵；`check.rs:443` 用 `usize::MAX`；`ranks.rs:81` 与 `head.rs:263` 用 `i32::try_from(...).unwrap_or(i32::MAX)` | 哨兵与饱和值掩盖缺键或类型转换 | `order_index` 走不变量索引；`check.rs:443` 直接匹配 `NodeOrigin::Caller`；`head.rs:263` 用 `Reverse<usize>` |
| TRK-8 | `src/captions.rs:77,86` | `shareholder_overview` / `investment_overview` 保留未用的 `_index: usize` | 工程规范点名反例 | 删除 `_index` 形参并更新调用点 |
| TRK-13（轻） | `src/layout/place.rs:343-354,373-400,501-507` | `place_plan_xs` 顺序 if-return；`try_align_single_group` 混入写 `xs` 副作用且与 `place_slotted_plan_xs` 重复判断单父 | 判定与写入耦合且条件重复 | 拆出 `XPlacementMode` 枚举，先纯判定再穷尽 `match` |
| PY-8 / 卫生 | `layout/mod.rs:102-103,131-132`；`layout/place.rs:55-60`；`paginate/session/mod.rs:188-191`；`paginate/session/assemble.rs:141-145`；`render/paint.rs:71-73`；`graph/fold.rs:289,312`；`graph/arena.rs:150` | `layout` 每次整份克隆 `NodeBox`；`ParentSlot::node_id` 零读者 `#[expect(dead_code)]`；`Session::new` 从 `boxes.values().find` 绕路反推 `node_width` 且无非主体框时回退常量而非样式宽；`investment_id_refs` 与 `direct_count` 重复按 `boxes` 过滤；`center_text` 同串分配两次 `GlyphUse`；`fold.rs:312` 不可达 `Overflow => continue`；`Graph.subject_protocol_id` 双键残留 | 冗余堆克隆、死字段、单节点样式宽度偏差、重复过滤与双键 | 改传尺寸视图或借用、删死字段与双键、`Session::new` 直读 `graph.style.node_width_mm`、删 `investment_id_refs`、复用单次 `GlyphRun`、清死分支 |
| ARC-2（执行缺口） | `crates/pdfcraft-orgchart/src/` | 无自动化模块边检查脚本 | `rust-engineering-standard.md §16:327` 已登记缺口，当前仅靠人工检索 | 仿 `check_pdfcraft_pdf_modules.py` 补 orgchart 边表与 CI 检查 |

**核对后未构成违规的项目。** 包边界无 `pdfcraft-pdf` / `forms` / `post` 依赖。`DEP-2` 钉版与 `workspace = true` 完整。`krilla` 仅关在 `render` 且只用 `draw_glyphs`。公共面与 `structure-spec §2` 及 `提案书 §13` 一致。`Chart::new` / `to_pdf` 职责正交。`Error` 结构化且具备 `Send + Sync` 编译期测试（`src/chart.rs:130-136`）。`forbid(unsafe_code)` 生效。`ARC-9` 不适用（无字符分类器）。`ARC-10` 字体校验收敛在 `face_metrics`。

---

## 3. 结构债对照（保留 vs 大改）

| structure-spec 大改项 | 状态 | 实测证据 |
| --- | --- | --- |
| §2 公共面只导出 Chart / Font / Request / Error，删 `doc(hidden)`、`LaidOutNode` 与内部 `as_str` | 已落地 | `src/lib.rs:80-103`，无 `doc(hidden)` 与 `LaidOutNode` |
| §3 模块依赖严格单向，krilla 关在 `render` | 已落地（缺 CI 脚本） | 全量静态检索无违规边；`module-dependency.md` 仍为人工检查 |
| §4 `NodeId` / `EdgeId` / `NodeOrigin` arena 与 `fold` 建新 arena | 大体落地，3 处残留 | `graph/arena.rs`、`graph/fold.rs`；残留 `subject_protocol_id`（`arena.rs:150`）、`from_index` 为 `pub(crate)`（`arena.rs:19,39`）、`fold.rs:312` 死 `Overflow => continue` |
| §5 `PageOracle` 替换 `dyn FnMut`、memo 用 id 与方案下标、删未读字段 | 大体落地，2 处残留 | `PageOracle` 与 `plans: Vec<SearchPlan>` 已落地；残留 `FamilyEdge.id` 为 `usize`（`family/mod.rs:46`）、`ParentSlot::node_id` 死字段（`place.rs:55-60`） |
| §6 `Session` 去 `RefCell` 用 `&mut self`、复用同一 `FontMetrics`、`reach_cache` 挂图 | 已落地 | `session/mod.rs:83-104`、`chart.rs:92-113`、`arena.rs:154`、`font/fixed.rs` 仅 `#[cfg(test)]` |
| §7 缺框静默跳过消除、框表 `Vec` 下标对齐、消除裸 `HashMap[k]` | 部分落地 | 主路径缺框已报 `LAYOUT_FAILED`（同时保留 §9 边界校验）；布局、走线、渲染仍有软跳过，框表仍是 `HashMap`，生产 `HashMap[k]` 余 52 处 |
| §7 `center_text` 复用单次 `GlyphRun` | 未落地（轻） | `render/paint.rs:71-73` 仍先算宽再绘制 |
| §8 `Font` 持有 `Arc<Vec<u8>>`、`FontMetrics` 字形缓存、`Measurer` 用 `&mut self` | 已落地 | `font/mod.rs:30-36,112-168,217-229` |
| §9 私有 `Error`、`#[non_exhaustive] ErrorSubject`、无生产 `unwrap` | 已落地 | `error.rs:62-163`，生产零 `unwrap()`，`chart.rs:130-136` 有 `Send + Sync` 测试 |
| §11 测试迁入与真实字体升格/overflow 锁测 | 迁移已落地，锁测未完 | `chain_split_tests.rs:60-112` 仅测了 `FakeEngine` 升格，缺 `to_pdf` / `WrappedEngine` 与 `overflow_box` 对照 |

「保留」列（评分、驱逐、错误码字符串、文案与页角色合同）未借机改动，不记为债。

---

## 4. 问题清单（治理序）

### P0：渲染出口无错误通道的静默丢弃

注：以下两项均经静态代码分析确认出口缺少错误返回通道，尚未通过运行构造合法请求复现触发。

1. 绘制侧 cue / leader / link 缺失静默跳过，且 `PageSpec::check` 未校验 `link.node_id ∈ layout.nodes`（`render/marks.rs:33-40,44-58`、`render/mod.rs:91-136`、`page/model.rs:253-321`）。其中 `cont_cues ↔ layout.slots` 已被 `page/model.rs:307-318` 校验，`slot.anchor_id ∈ layout.nodes` 已被构造期 `paginate/cues.rs:158-161` 拦截，当前无显式护栏的是链接节点与 `rect_pt`。
2. 绘制侧路径构建失败静默返回，可把畸形几何吞成空白框或无裁切文字（`render/paint.rs:133-135,142-144,158-160,172-174,181-183,218-220`、`render/marks.rs:135-142,241-243,278-280`）。

### P1：确定性、内部不变量、PAN-3、测试诚实与公共文档

3. 布局与搜索内部不变量软跳过或默认宽分裂：走线缺端点 `continue`（`layout/route.rs:111-113,125-138`）、缺框用默认宽算 x（`layout/place.rs:262-266`）、`boxes` 中不在非空 `node_order` 内的框被静默丢弃（`layout/ranks.rs:24-35`）、`sparse_risk` 缺 task `continue`（`paginate/family/score.rs:99-101`）。
4. 生产代码 52 处 `HashMap[k]` 直接索引无 PAN-3 登记（分布在 `graph/fold.rs`、`font/mod.rs`、`layout/{mod,ranks,groups,place,route}.rs`、`paginate/family/{mod,head,chain}.rs`）。
5. 浮点位模式键与隐式相等：`y.to_bits()` 作总线行键（`layout/route.rs:84,284,334`）、`BusSpan` 隐式 `f64 == f64`（`layout/route.rs:26,66`）、`exact_sum` 用 `to_bits()` 绕过 `float_cmp`（`graph/fold.rs:190-196`）、`GeometryCache` 5 元组 `to_bits()`（`units.rs:160-169`）。
6. `PageSpec.boxes` 取自 `HashMap::values()`（`paginate/session/mod.rs:428`，`DET-1`）。
7. 缺环境字体测试裸 `return`（5 文件 12 用例：`tests/to_pdf.rs:33,66,126`、`tests/page_observations.rs:25-30,199,238,262`、`tests/stroke_color.rs:89`、`src/paginate/chain_split_tests.rs:74`、`src/font/font_tests.rs:86,107,118,133`，`TST-6`），导致唯一锁住 PDF 字节确定性的 `tests/to_pdf.rs:125` 在未配字体时静默跳过。
8. §11 真实字体合同测试未写完，缺生产 `to_pdf` / `WrappedEngine` 与 `overflow_box` 对照（`src/paginate/chain_split_tests.rs:60-112`）。
9. 公共 rustdoc 残留「对齐 Python」说明（`src/error.rs:9,31`，`DOC-4` / `ERR-7` / `PY-22`）。

### P2：结构收口、类型收紧与代码卫生

10. `paginate/cues.rs:418-424` 缺 slot `continue` 死分支（紧邻 `cues.rs:406` `spec.check()?` 已拦截，按 `structure-spec §7:222` 清理）。
11. `FamilyEdge.id` 与 `AttachmentTask.edge_ids` 改回 `EdgeId`（`paginate/family/mod.rs:46,72,167,222-223`、`paginate/session/assemble.rs:112`）。
12. `NodeId` / `EdgeId::from_index` 收窄为 `graph` 内部可见（`graph/arena.rs:19,39`、`content/lines.rs:287`）。
13. 框表收成 `Vec` 下标对齐或借用视图，消除 `layout/mod.rs:102-103,131-132` 每次整份克隆 `NodeBox`（同时保留 §9 边界缺框报 `LAYOUT_FAILED`）。
14. 删除 `Graph.subject_protocol_id` 双键残留（`graph/arena.rs:150`、`graph/check.rs:450`、`graph/fold.rs:274`）。
15. 清理 `fold` 不可达的 `Overflow => continue`（`graph/fold.rs:289,312`）。
16. 清除 `usize::MAX` / `i32::MAX` 哨兵与饱和写法（`layout/groups.rs:8,35,43,116,123,160`、`paginate/session/mod.rs:470`、`graph/check.rs:443`、`layout/ranks.rs:81`、`paginate/family/head.rs:263`）。
17. 删除 `captions` 的 `_index` 形参（`src/captions.rs:77,86`，`TRK-8`）。
18. 拆出 `place_plan_xs` 策略枚举并解耦 `try_align_single_group`（`layout/place.rs:343-354,373-400,501-507`）。
19. 代码卫生杂项：删 `ParentSlot::node_id` 死字段（`layout/place.rs:55-60`）；`Session::new` 改直读 `graph.style.node_width_mm` 避免无非主体框时回退常量 `NODE_WIDTH_MM`（`paginate/session/mod.rs:188-191`，注：`page/boxes.rs:108-113` 决定所有非主体框同宽，此处非 `DET-1`）；删重复过滤的 `investment_id_refs`（`paginate/session/assemble.rs:141-145`、`cues.rs:331-336`）；`center_text` 复用单次 `GlyphRun`（`render/paint.rs:71-73`）。
20. `graph/fold.rs:306` 的 `stands_in_for` 清零语义补单测锁住（详见第 5 节说明）。
21. 补齐 `pdfcraft-orgchart` 模块 DAG 的 CI 边检查脚本（`ARC-2`）。

---

## 5. 非问题清单

- `src/graph/fold.rs:306` 在 `Caller` 节点存活而 `stands_in_for` 指向的父节点被折叠移除时清零 `stands_in_for`。静态推理表明在共享菱形图中（`P` 声明 `folded_holders_of = Q` 且另持有存活父 `A`，`Q` 被折掉而 `P` 存活）该分支可合法触发，改走 `remap` 报错会让 `Chart::new` 返回仅属 `to_pdf` 的 `LAYOUT_FAILED`。因行为规格 §5 未明文约定该边界且现有 `fold_tests.rs:186,372` 未覆盖此拓扑，已列为 P2-20 补测试确认而非报错违规。
- `src/layout/ranks.rs:24-28` 忽略 `node_order` 中不在 `boxes` 的 id。`layout/mod.rs:47` 明文约定此合同，且生产上游 `paginate/session/fit.rs:171-181` 已拦截缺框并报 `LAYOUT_FAILED`（实际风险在反方向，已计入 P1-3）。
- 搜索预算用尽（8192 次）`break` 返回当前最优，以及 `Context.expansions` / `budget_exhausted` 作为内部控制流保留（`structure-spec §1,§5`）。
- `cues::required_row_gap`（`paginate/cues.rs:111-123`）锚点不在页或非邻行 `continue`（`structure-spec §7:221` 明确保留）。
- `layout/ranks.rs:142-146` 主体不可达节点保留跳数 0（`structure-spec §1:42` 明确保留）。
- `render/ratio_labels.rs:60-72` 缺贴边 stub 或页主体不画比例标签（行为规格 §8.3 产品规则）。
- `units.rs:80` `n64` 饱和转换（`structure-spec §10:273` 明确保留）。
- `layout/mod.rs:110-111` 已对放置节点按 `NodeId` 排序后再画，绘制序确定。
- `examples/equity_chart.rs` 读环境变量字体路径，核心库 `Font::new` 只收字节（Sans-I/O）。
- 集成测试用 `lopdf` 检查 `MediaBox`、虚线操作数与 `LinkAnnotation` 目标页，断言的是可观察 PDF 结构，不违反 `TST-2`。
- 不做 Knuth-Plass、完整 UAX#14 / bidi / 复杂 shaping、评分调参、框观感微调或公共 `Mm` / `Percent` 新类型（`提案书 §13` 明确不做）。

---

## 6. 建议收口顺序

1. 补齐 `PageSpec::check` 对 `layout.nodes` 的包含性校验，并将渲染层路径构建与缺项分支改为返回 `Result<(), Error>`（清 P0-1、P0-2）。
2. 收口测试诚实与确定性基线：修 5 文件 `TST-6` 显式 skip、补 §11 真实字体 `to_pdf` / `overflow_box` 锁测、固定 `PageSpec.boxes` 顺序并消除 `DET-3` 浮点键（清 P1-5 至 P1-8）。
3. 清理布局与搜索内部软跳过（含 `ranks::prepare` 反向丢框）与 52 处生产 `HashMap[k]`，同步修正 `error.rs` 公共文档（清 P1-3、P1-4、P1-9）。
4. 完成 P2 结构收口：`EdgeId`、框表借用/对齐、`subject_protocol_id`、`from_index` 收窄、哨兵与死分支清理、`fold.rs:306` 菱形图单测、orgchart DAG CI 脚本（清 P2-10 至 P2-21）。

---

## 7. 复核修订说明（相对初稿）

- **P0 从 6 条收窄为 2 条。** 初稿 P0-1、P0-2 受 `ranks::prepare` 前置过滤保护，降为 P1-3。初稿 P0-4 受 `cues.rs:406` `spec.check()?` 兜底属不可达死分支，降为 P2-10。初稿 P0-3 方向写反，合同允许 `node_order` 含多余 id，真正丢节点是 `boxes` 有但非空 `node_order` 无的反向漏洞，改计入 P1-3。保留的 P0-1、P0-2 已注明上游护栏范围与静态分析边界。
- **初稿 P1-10（`fold.rs:306`）撤出主违规、P1-8（`place_plan_xs`）降为 P2-18、P2-19（`investment_id_refs`）改归因为重复过滤。**
- **补入初稿漏判。** 补齐 `ranks.rs:24-35` 反向丢框、`fold.rs:190-196` / `route.rs:26,66` / `units.rs:160-169` 浮点比较与位键、`page/model.rs:253-321` 校验缺口、`error.rs:9,31` 公共文档残留 Python、`TST-6` 另 3 个测试文件及与字节确定性测试的关联、`layout/mod.rs:102,131` 整箱克隆、`place.rs:55-60` 死字段、`Session::new:188-191` 单节点样式宽度回退与 52 处 `HashMap[k]` 精确计数。
