# pdfcraft-orgchart 智能宽度与对外投资卡片排版规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-25 |
| 状态 | **已随 `pdfcraft-orgchart 0.4.0` 落地**。§8 五项拍板已写入实现；公共文档与 CHANGELOG 见 §7 |
| 关联文档 | [`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md)；[`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md)；[`rust-engineering-standard.md`](rust-engineering-standard.md)；[`提案书.md`](提案书.md) |
| 范围 | `crates/pdfcraft-orgchart` 的公共宽度策略 API、框内字段展示、两阶段平衡折行、三档自适应宽度、按宽预算分行、正交避障绕线与渲染层级 |

---

## 0. 背景与产品决策

1. **脱离旧 Python 实现历史包袱**。不再维持与旧 Python 版（`pdfcraft-equity`）常量和框内样式的逐像素或逐行对齐。本规格确认后，同步修订 [`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md)、[`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md) 对应章节及测试。
2. **解决的核心问题**。
   - 现行所有非主体节点固定为 `38 mm`（内容区 `34 mm`），在 `10 pt` 下每行仅容约 `9` 个汉字，对外投资节点（名称、实缴、地区、日期）频繁折成 `5~7` 行瘦长条，主次不清。
   - 自然人等短名节点（`2~4` 字）与长公司名同宽，浪费横向空间。
   - `10~12` 字提示性节点（如「没有其他超过25%的股东」）在 `38 mm` 下会把末尾「股东」二字折到第二行，易造成阅读歧义。
3. **现阶段姿态（0.x 初期）**。允许对 `ChartStyle` 做清晰的 breaking 调整，不保留过渡别名或双字段兼容层（见工程规范 `VER-3`）。版本按 `VER-1`/`VER-2` 升至 `0.4.0`。

---

## 1. 范围

### 1.1 做

- `ChartStyle` 引入 `NodeWidth` 枚举（`Adaptive` / `Fixed(f64)`），默认 `Adaptive`。
- `Adaptive` 模式启用三档离散节点宽度：`32 / 48 / 60 mm`。
- 对外投资节点将 `location` 与 `approval_date` 合并为一行 `{location} · {approval_date}`。
- 框内文字维持全部水平居中，不增加额外字段前缀；详情/法人用 `9 pt` 与紧凑行距，字色与名称同为黑色；不画名称/详情分隔线。
- 升级 `font::wrap_text` 为两阶段结构（无状态词法分类加全局 DP 平衡折行与防孤字）；普通节点名称 `>= 3` 行时自动降一档至 `9.0 pt` 兜底。
- 修复 `promote_to_page_subject` 保留原始逻辑行边界，避免投资节点多条 `Detail` 升格时粘连；与 `build_node_boxes`、`paint_node_lines` 共用同一套垂直布局公式。
- 升级 `layout/groups.rs`、`layout/mod.rs`、`layout/place.rs`、`layout/route.rs` 与 `paginate/session/`，彻底移除等宽列数 `cols` 假设，按实际行宽预算分行，并修复异宽多行下的股东/投资绕线穿框（含 `route_multi_out` 树形支干避障）与单列脊线判定。
- 限制 `ContCue` 框宽上限（`38.0 mm`）。
- 按 `0.4.0` breaking 变更同步版本、CHANGELOG、README、guide 与提案书 §13。

### 1.2 前置独立修复（先于 0.4.0 单独落地，遵循「一 PR 一件事」）

1. **修复 HEAD 模块 DAG 检查**：先从 `scripts/pdfcraft-orgchart-module-edges.txt` 移除当前未使用的 `page request` 边，恢复 `TOOL-5` 门禁全绿；`0.4.0` 实现 `page -> request` 时再加回。
2. **独立修复 `Graph::overflow_ids` 缓存键（`0.3.2` 潜伏缺陷）**：先写单测复现首孩 `Person`、次孩 `LeafCompany` 驱逐时的过期缓存复用，再将键改为 `(subject, first_moved, moved_count)` 并同步修订 `docs/pdfcraft-orgchart-structure-spec.md` §4，不混入 `0.4.0` 排版改动。

### 1.3 不做

- 本期不做「无分支叶子对外投资转汇总表格清单」模式，保留独立方框与现有 `overflow` / `ContCue` 续页机制。
- 不允许 `Adaptive` 暴露细分档位或上下限参数给调用方。
- 不改变 `Chart::new` 的双锥校验规则、`top_n` 折叠规则、错误码闭集（`ErrorCode`）及线旁比例标签算法逻辑（依赖最小安全框宽 `32 mm` 保障不越界）。
- 不打破 [`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md) §3 定义的模块依赖 DAG 方向。

---

## 2. 公共 API 变更（`src/request.rs`、`src/lib.rs`）

### 2.1 新增 `NodeWidth` 枚举并从 crate 根重导出

```rust
/// Non-subject node width policy.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum NodeWidth {
    /// Snap each non-subject node to a discrete tier (`32`, `48`, or `60` mm)
    /// based on its natural content width.
    #[default]
    Adaptive,
    /// Fixed width in mm for every non-subject node (`20.0..=80.0`).
    Fixed(f64),
}

impl NodeWidth {
    /// Minimum fixed width accepted by [`NodeWidth::Fixed`], in mm.
    pub const MIN_FIXED_MM: f64 = 20.0;
    /// Maximum fixed width accepted by [`NodeWidth::Fixed`], in mm.
    pub const MAX_FIXED_MM: f64 = 80.0;
}
```

### 2.2 调整 `ChartStyle`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartStyle {
    /// Width policy for non-subject nodes.
    pub width: NodeWidth,
    /// Page margin in mm. Must be finite, > 0, and <= MAX_MARGIN_MM.
    pub margin_mm: f64,
}

impl ChartStyle {
    /// Maximum page margin accepted by [`ChartStyle::margin_mm`], in mm.
    pub const MAX_MARGIN_MM: f64 = 40.0;
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            width: NodeWidth::Adaptive,
            margin_mm: 8.0,
        }
    }
}
```

> 移除旧 `ChartStyle::MIN_NODE_WIDTH_MM` 与 `ChartStyle::MAX_NODE_WIDTH_MM`，同步清理 `src/request.rs` 中指向 `[ChartStyle::node_width_mm]` 的旧 rustdoc 链接，防止 `RUSTDOCFLAGS="-D warnings" cargo doc` 失败。

### 2.3 校验与主体宽度规则

- **协议校验（`src/graph/check.rs`）**。
  - `NodeWidth::Adaptive` 无需额外参数校验，恒合法。
  - `NodeWidth::Fixed(w)` 校验 `w.is_finite() && (NodeWidth::MIN_FIXED_MM..=NodeWidth::MAX_FIXED_MM).contains(&w)`；否则返回 `ErrorCode::InvalidProtocol`，`subject` 由原 `ErrorSubject::Field("style.node_width_mm")`（`src/graph/check.rs:120`）同步更新为 `ErrorSubject::Field("style.width")`。
- **与《提案书》§13 的关系**。本节对 `ChartStyle` 字段与常量位置的调整覆写 [`提案书.md`](提案书.md) §13（`0.3.0` 旧约定）；按 `AGENTS.md` 文档优先级，落地实现前/同步在《提案书》新增对应确认条。
- **页主体宽度（含升格附页根，`page::boxes::subject_width_mm(NodeWidth)`）**。
  - `NodeWidth::Fixed(w)` 取 `min(w * SUBJECT_WIDTH_FACTOR, SUBJECT_MAX_WIDTH_MM)`（`min(w * 3.0, 120.0)` mm）。
  - `NodeWidth::Adaptive` 使用具名常量 `ADAPTIVE_SUBJECT_BASE_WIDTH_MM = 38.0` 计算，即 `min(38.0 * 3.0, 120.0) = 114.0` mm。
- **`lib.rs`**。新增 `#[doc(inline)] pub use request::NodeWidth;`，并同步更新 crate 级文档与模块表。
- **内部传递**。`Graph.style`、`Session`、`assemble.rs`、`fit.rs`、`chart.rs`、`page/boxes.rs` 原本持有或传递 `node_width: f64` 标量的位置，改为直接传递 `NodeWidth`，避免丢失模式语义。

---

## 3. 展示与框内排版规范（`src/content/lines.rs`、`src/units.rs`、`src/page/boxes.rs`、`src/render/`）

### 3.1 投资节点字段合并

`DisplayRole::Investment` 的框内逻辑行顺序调整为：

1. `LineKind::Name`：`node.name`（原样保留）。
2. `LineKind::Detail`（若有实缴金额）：`实缴 {amount}` 或 `实缴 {amount} {currency}`（逻辑不变）。
3. `LineKind::Detail`（地区与核准日期合并，各自 `trim()` 后判空）：
   - 两者均非空：`{location} · {approval_date}`（半角空格 + `U+00B7` 中圆点 + 半角空格）；
   - 仅 `location` 非空：`{location}`；
   - 仅 `approval_date` 非空：`{approval_date}`；
   - 两者皆空：省略该行。

`DisplayRole::Subject` 与 `DisplayRole::Shareholder` 行顺序不变（名称加法人姓名，无前缀）。

> 字符覆盖说明：`U+00B7`（`·`）成为库内置文案字符，经 `wrap_text` 自动走 `ensure_glyphs`。若调用方传入的子集字体缺少 `U+00B7` 且投资节点同时存在非空 `location` 与 `approval_date`，将返回 `ErrorCode::MissingGlyph`（`ErrorSubject::Character('·')`）。该约束同步写入 `pdfcraft-orgchart-spec.md`。

### 3.2 视觉层级常量与绘制（`src/units.rs`、`src/font/mod.rs`、`src/page/boxes.rs`、`src/render/`）

下列视觉层级全局统一生效，覆盖 `NodeWidth::Adaptive`、`NodeWidth::Fixed(w)` 以及所有框角色（含页主体与附页升格主体）：

| 视觉项 | 现行值 | 新规格值 |
| --- | --- | --- |
| `NAME_FONT_PT` | `10.0 pt` | `10.0 pt`（普通节点超长兜底降档 `NAME_FALLBACK_FONT_PT = 9.0 pt`） |
| `DETAIL_FONT_PT` | `9.0 pt` | **`9.0 pt`**（作用于 `LegalPerson` 与 `Detail`；与名称同色黑色，行距改用 `DETAIL_LINE_GAP_PT`） |
| `DETAIL_LINE_GAP_PT` | 共用 `LINE_GAP_PT = 2.0 pt` | **`1.2 pt`**（名称行仍用 `LINE_GAP_PT = 2.0 pt`） |
| 框内字色 | 黑色 | 名称、法人、详情均为黑色（`INK_BYTE = 0`）；不画名称/详情分隔线 |
| 框内对齐 | 全部水平居中 | 维持全部水平居中 |

### 3.3 算高与绘制单一真相源

1. **`Measurer` 行距接口（`src/font/mod.rs`）**。新增单一方法：
   - `fn line_height_with_gap_mm(&mut self, size_pt: f64, gap_pt: f64) -> f64`
   - 不另设 `glyph_height_mm`；无行距字形高度统一表达为 `line_height_with_gap_mm(size_pt, 0.0)`，`FontMetrics`、`FallbackMeasurer` 与所有测试替身实现天然一致。
   - 原 `line_height_mm(size_pt)` 保留为 `line_height_with_gap_mm(size_pt, LINE_GAP_PT)` 的默认实现，供页眉、页脚与 `ContCue` 复用。
2. **盒内垂直布局与 `render` 边界解耦（`src/page/boxes.rs` 导出 `pub(crate)` 纯布局模型）**。
   - 盒内行布局输出结构含每行 `y_mm`、`font_size_pt`、`text` 及内容总高，由 `page::boxes` 单一纯函数按 `gap_for(line.kind)` 顺序推进（名称用 `LINE_GAP_PT`，法人/详情用 `DETAIL_LINE_GAP_PT`）。
   - `build_node_boxes`、`promote_to_page_subject` 与 `render/marks.rs::paint_node_lines` 均消费该布局模型；`render` 框内文字一律 `ink_color()`，不依赖 `content::LineKind`，也不通过 `page` 转发重导出 `LineKind`。
3. **文字颜色传递（`src/render/paint.rs`）**。`paint_run` 与 `center_text` 接收 `color: rgb::Color`。框内名称、法人、详情与页眉、页脚、比例标签、`ContCue` 均传 `ink_color()`（黑）。
4. **升格逻辑行保真（`src/page/boxes.rs`）**。
   - `WrappedLine` 新增字段 `pub logical_line_index: usize`，记录该折行片段归属的原逻辑行下标。
   - `promote_to_page_subject` 的 `join_segments` 按 `logical_line_index` 分组拼接，而不再把连续同 `LineKind` 拼成一条字符串，彻底消除投资节点升格时「实缴行」与「地区 · 日期行」无分隔粘连的问题。

---

## 4. 三档智能宽度与折行算法（`src/page/boxes.rs`、`src/font/mod.rs`）

### 4.1 三档吸附规则（仅 `NodeWidth::Adaptive`）

定义具名常量（`src/units.rs`，遵守 `DOC-6`）：
- `ADAPTIVE_TIERS_MM: [f64; 3] = [32.0, 48.0, 60.0]`
- `ADAPTIVE_SUBJECT_BASE_WIDTH_MM: f64 = 38.0`
- `CONT_CUE_ANCHOR_MATCH_MAX_MM: f64 = 38.0`

对每个非主体节点（包括 `FoldedHolders` 与 `Overflow` 合成节点）：
1. 在 `NodeWidth::Fixed(w)` 下直接使用固定宽 `w`，不执行档位吸附。
2. 在 `NodeWidth::Adaptive` 下：
   - 逐行测量各逻辑行不折行时的自然宽度：`Name` 按 `NAME_FONT_PT`（`10.0 pt`），`LegalPerson` 与 `Detail` 按 `DETAIL_FONT_PT`（`9.0 pt`）。
   - 对于 `Overflow` 节点（`overflow_box`）：额外把第二行槽位预算 `detail_budget = width("详见附件", NAME_FONT_PT) + 3.0 * max_digit_width('0'..='9', NAME_FONT_PT)` 纳入自然宽度测量（与 `sized_cue_anchors` 的 3 位最宽数字预算一致），确保回填页码时 `write_overflow_line` 永不溢出。
   - 取 `needed_box_w = max_natural_w + 2.0 * PADDING_X_MM`。
   - 向上取 `ADAPTIVE_TIERS_MM` 中首个 `>= needed_box_w` 的档位；若全部小于 `needed_box_w`，取最大档 `60.0 mm`。
3. **安全下限与 `Fixed` 超窄宽边界**。
   - `Adaptive` 最小档 `32.0 mm` 半宽 `16.0 mm`，`100.00%`（`9 pt` 约 `13.3 mm`）加 `RATIO_LABEL_OFFSET_X_MM`（`1.2 mm`）右伸 `14.5 mm < 16.0 mm`，不越过自身右沿；内宽 `28.0 mm` 足以容纳 `详见附件999`（约 `20 mm`）。
   - 调用方显式指定超窄 `NodeWidth::Fixed(w)`（例如 `Fixed(20.0)` 内宽仅 `16.0 mm`）时，保持 `0.3.2` 原样：`Overflow` 第二行仍单行居中回填 `详见附件{n}`，由调用方承担极窄边框的水平溢出风险。

### 4.2 名称字号兜底与两阶段平衡折行

1. **名称降字号兜底（仅 `Adaptive` 模式下的普通非主体、非 `Overflow` 节点）**。
   - 针对单条 `LineKind::Name` 逻辑行，先用 `NAME_FONT_PT`（`10.0 pt`）折行。
   - 若折行结果行数 `>= 3`，尝试用 `NAME_FALLBACK_FONT_PT`（`9.0 pt`）重新折行；仅当 `9.0 pt` 下行数严格少于 `10.0 pt` 时采用 `9.0 pt` 结果，否则保留 `10.0 pt`。
   - `Overflow` 节点禁用 `9.0 pt` 降档，保持与 `paginate/cues.rs::write_overflow_line` 硬编码的 `NAME_FONT_PT` 一致。
   - 升格为页主体时仍按 `SUBJECT_NAME_FONT_PT`（`12.0 pt`）与主体宽重折行。

2. **两阶段平衡折行与防孤字（`src/font/mod.rs`，严格遵守 `ARC-9` 与 `TRK-13`）**。
   - **触发条件与无损不变量**。仅在 `allow_wrap == true` 且整行宽度 `> max_width_mm` 时触发；输出切片必须满足无损拼接不变量 `parts.concat() == text`（供 `promote_to_page_subject` 还原原文）。
   - **阶段 1：无状态词法切分与静态边界惩罚（纯函数，不携带上下文状态）**。
     1. 沿用现有不可分 token 规则切分 `text`（ASCII 连续词直至空格成块，超过 `max_width_mm` 的超长 ASCII 词按单标量拆开；非 ASCII 逐标量成块）。
     2. 无状态字符分类器 `classify_scalar(ch: char) -> CharClass`（如 `OpenBracket` 含 `(` / `（`、`ClosePunct` 含 `)` / `）` / `、` / `，` / `。`、`MiddleDot` 含 `·`、`Whitespace`、`Other`）。
     3. 边界转移裁决 `boundary_penalty(prev: CharClass, next: CharClass) -> f64` 使用穷尽 `match (prev, next)`，三级具名惩罚常量（单位同 `mm²`，压倒常规行宽平方差 `0..900 mm²`）：
        - `WRAP_FORBID_BREAK_PENALTY = 10_000.0`：开括号后断行、闭括号或避头标点前断行、中圆点 `·` 前后断行；
        - `WRAP_ORPHAN_LINE_PENALTY = 2_000.0`：末行非空白字符数 `<= 2`（优先级低于括号与 `·` 禁断规则）；
        - `WRAP_SUFFIX_SPLIT_PENALTY = 1_500.0`：切断静态机构后缀表（`["股份有限公司", "有限责任公司", "有限公司", "合伙企业", "分公司", "股东"]`）内部边界。
     4. 静态机构后缀表通过纯切片匹配标出后缀内部边界并施加 `WRAP_SUFFIX_SPLIT_PENALTY`。
   - **阶段 2：全局最优 DP 折行（一次性求全局最优，禁止事后 `if` 回退补丁）**。
     1. 对每个 token 测宽一次，构建宽度前缀和 `prefix_w[0..=M]`，先以贪心扫描求得满足单行上限的最少行数 `N`（当单个 token 本身宽度 `> max_width_mm` 时允许该单 token 独占一行，与现行逻辑一致）。
     2. 当 `N >= 2` 时，记目标行宽 `target_w = total_w / N`，执行 `O(N * M^2)` DP 将 `M` 个 token 顺序切分为 `N` 行，硬约束为每行满足 `line_w <= max_width_mm` 或该行仅含 1 个 token，最小化总代价：
        `cost = Σ (line_w - target_w)^2 + Σ boundary_penalty(split_k) + last_line_orphan_penalty`
        其中 `last_line_orphan_penalty` 在末行非空白字符数 `<= 2` 时取 `WRAP_ORPHAN_LINE_PENALTY`（否则 `0.0`）。
     3. `font` 模块保持仅依赖 `error, units`，不引入 `graph` / `content` 领域类型。
   - **适用范围**。新 `wrap_text` 对全部模式（`Adaptive` 与 `Fixed(w)`）及全部允许折行的逻辑行（`Name`、`LegalPerson`、`Detail`）统一生效；`NodeWidth::Fixed(w)` 仅锁定框宽为 `w` 并禁用 `9.0 pt` 名称兜底降档。

### 4.3 `ContCue` 框宽上限（`src/page/boxes.rs`）

- 在 `sized_cue_anchors` 中，将续页虚框宽度公式统一更新为（`Adaptive` 与 `Fixed(w)` 均适用）：
  `(text_width + 2.0 * CONT_CUE_PAD_MM).max(box_.width_mm.min(CONT_CUE_ANCHOR_MATCH_MAX_MM)).max(CONT_CUE_MIN_WIDTH_MM)`
  确保宽投资卡或 `114 mm` 主体附带 Cue 侧放（`beside_rect`）时不会因虚框虚胖导致无法放入页面。

---

## 5. 布局分行与正交绕线适配（`src/layout/`、`src/paginate/`、`src/units.rs`）

### 5.1 彻底废弃等宽列数 `cols`：按实际宽度预算分行

1. **全链路清理等宽假设**。
   - 删除 `PageGeometry::max_cols` 与 `count_units`（`src/units.rs`，同步修订 `pdfcraft-orgchart-structure-spec.md` §10）。
   - 删除 `LayoutInput.cols`、`LayoutInput.node_width_mm`（`src/layout/mod.rs`）以及 `LayoutEngine::family_row_metrics` 的 `cols`、`node_width_mm` 参数。
   - 删除 `Spacing.node_width_mm`（`src/layout/place.rs`），`place_layers` 的 `content_width` 初值与 `group_band_width_mm` 空 band 返回值改为 `0.0`。
2. **`group_band_rows` 单行直通条件（`src/layout/groups.rs`）**。
   - 废弃原有的 `group_ids.len() <= safe_cols` 判断。
   - 先计算 `ordered` 排成单行的总宽 `single_row_span(ordered, box_by_id)`。若 `subject_rank == group_rank` 或 `single_row_span <= max_row_width_mm + 1e-9`（单行放得下，即 `R <= 1`），直接返回 `vec![ordered]`，不触发 `partition_outer_leaf` 重排。
   - 仅当必须折行（`R >= 2`）时执行 `partition_outer_leaf`。
3. **按宽平衡分行 `balanced_rows_by_width`（`src/layout/groups.rs`）**。
   - **最小行数 `R`**。顺序贪心扫描，按 `row_span <= max_row_width_mm + 1e-9`（单节点本身超限时独占一行）求得最小行数 `R`。使用严格预算加 `1e-9` 浮点容差（不加 `FIT_EPSILON_MM`，把 `FIT_EPSILON_MM` 保留给 `LayoutResult::fits`）；同时移除旧 `count_units`（`units.rs:237-247`）的人为 `64` 列上限，由实际页宽自然约束单行节点数。
   - **`outer/leaf` 行边界保护**。当 `outer` 与 `leaf` 均非空且 `min_rows(outer) + min_rows(leaf) == min_rows(outer ++ leaf)` 时，分别对 `outer` 和 `leaf` 独立调用平衡切分再拼接结果，不增加总行数且杜绝 `outer` 掉进 `leaf` 行；当独立切分会多出 1 行时，回退至对 `outer ++ leaf` 联合切分（与 `0.3.2` 行数语义一致，允许过渡行左放 `outer` 尾、右放 `leaf` 头），由 §5.2 无阻挡垂直通道绕线兜底防穿框。
   - **全局平衡 DP 与确定性全序 tie-break**。将长度为 `K` 的序列切分为指定 `R` 行，硬约束每行满足 `row_span <= max_row_width_mm + 1e-9`（单节点行豁免），最小化行宽方差 `Σ (row_span - mean)^2`。当多种切分代价相等（差值 `<= 1e-9`）时，按严格全序择优：先比较各行节点数序列的字典序更大（前多后少），仍相等再比较各行宽度序列的字典序更大（前宽后窄），确保 7 个等宽节点在 3 列宽预算下稳定得到 `[3, 2, 2]`。
4. **多组层缩减 `family_bands_within_width`（`src/layout/mod.rs`）**。
   - 定位说明：`family_columns == true` 仅发生在非直接层，且正常分页路径由 `PackedLayout::fits()`（`paginate/session/mod.rs:70`）强制 `max_nondirect_rows <= 1` 触发拆页；以下第二步缩减循环服务于直接调用 `LayoutEngine::layout` 的测试以及无法继续拆分的退化单页，使层宽尽可能落入 `printable_width_mm`。
   - 确定性两步策略：
     1. 先令每组以 `printable_width_mm` 为单组行宽上限分别分行（记录各组初始行数 `R_i = rows_i.len()`）。若 `within(layer_width, printable_width_mm)` 成立，立即返回（避免均分预算误将本可单行通过的非直接层强行折成 2 行）。
     2. 若总宽仍超限，进入循环：在所有「仍满足 `R_i < group_i.len()`」的组中，选取当前 band 宽度最宽的那一组（相等取下标更小者），将其目标行数严格加 1（`R_i += 1`）并调用指定 `R_i` 行的平衡 DP 重排该组与重算层宽；循环直至 `within(layer_width, printable_width_mm)` 或所有组均达到 `R_i == group_i.len()`（每行仅 1 节点）时终止，最多迭代 `Σ(group_i.len() - 1)` 步，无浮点步长黑魔法。

### 5.2 正交绕线防穿框与单列判定修复（`src/layout/route.rs`）

1. **多行单列脊线判定（`column_stack`）**。
   - 原实现检查 `sources.iter().all(|node| (node.x - sources[0].x).abs() < 0.01)`。
   - 改为检查至少 2 个源节点、每行仅 1 个节点且中心 X 对齐：
     `sources.len() >= 2 && row_bottoms.len() == sources.len() && sources.iter().all(|node| (node.x + node.width / 2.0 - first_center_x).abs() < 0.01)`。
2. **统一的无阻挡垂直通道搜索（`side_channel_x` 与 `group_spine_x` 共用）**。
   - **第一级优先候选**。先检查紧邻左右通道（`side_channel_x` 为 `src.x - 3.0` 与 `src.x + src.width + 3.0`；`group_spine_x` 为 `min_x - 3.0` 与 `max_x + 3.0`，均使用 `NODE_SPACING_MM * 0.5`）。若至少一条满足 `vertical_clear`，直接取其中安全且距 `trunk_x` 最近者（保证等宽常规布局走线零变化）。
   - **第二级间隙与外沿搜索（仅第一级双侧均被阻挡时触发）**。
     - 收集与 `(y_lo, y_hi)` 在 Y 轴重叠的所有非排除节点的水平碰撞区间 `[node.x - 0.5, node.x + node.width + 0.5]`（按 `0.5 mm` 碰撞容差而非 `3.0 mm`，避免合并区间时吞掉间距恰好 `6.0 mm` 的合法中隙）。
     - 排序并合并重叠区间，生成候选 X 集合：整体最左外沿 `min_left - NODE_SPACING_MM * 0.5`、整体最右外沿 `max_right + NODE_SPACING_MM * 0.5`，以及每对相邻合并区间之间的空隙中点 `(prev.right + next.left) / 2.0`。
     - 过滤满足 `vertical_clear` 的候选，取距 `trunk_x` 最近的一条。
3. **`outward_points` 水平出线避开同行高框**。
   - `outward_points` 走侧通道时，用源节点所在行的最大底边 `row_max_bottom` 计算 `exit_y`（若无同行信息则退回 `src_bottom.y`），避免矮框横向拐出时切割同行的更高节点框。
4. **`route_multi_out` 多行对外投资树形支干避障**。
   - 原实现将母线 `bus_y` 置于第一行投资节点上方，并对第二行及更下行节点直接从 `(dst_top.x, bus_y)` 垂直拉到 `dst_top`，在异宽多行投资布局下必然穿透第一行卡片。
   - 升级规则：对 `route_multi_out` 的每个目标节点 `dst`，检查 `vertical_clear(dst_top.x, bus_y, dst_top.y, &node_list, &child_exclude)`（`child_exclude = {edge.from_id, edge.to_id}`）。
     - 若垂直通畅，保持原折线直下 `dst_top`。
     - 若被上行投资框阻挡，按同父同行树形分叉语义路由（所有子边共享同一父节点 `edge.from_id`）：
       1. 对同一源节点 `edge.from_id` 且处于同一目标行（同 `row_top`）的被遮挡目标集合，以该目标行中心 X 为参考，调用统一垂直通道搜索求取共享安全支干 `channel_x`（覆盖 `bus_y .. entry_y`；若极端场景存在不同源节点争用同一通道，按 `BUS_STAGGER_MM` 错开或独立选通道）。
       2. 取目标行正上方相邻行的最大底边 `prev_row_bottom` 与行间净空 `gap = (row_top - prev_row_bottom).max(0.0)`，调用既有 `commit_bus_y` 分配该行的水平支母线 `entry_y`，候选值为：
          `candidate = prev_row_bottom + (gap * 0.35).clamp(1.5, ROW_GAP_MM * 0.5)`，并钳位在 `[prev_row_bottom + BUS_ENDPOINT_CLEARANCE_MM, row_top - BUS_ENDPOINT_CLEARANCE_MM]`。
          在默认 `gap = ROW_GAP_MM = 8.0 mm` 下，`entry_y = row_top - 5.2 mm`，严格位于 `RatioHug::Top` 比例标签顶沿（约 `row_top - 3.6 mm`）之上，且保留 `5.2 mm` 竖向 stub，确保不压标签并被 `ratio_labels::vertical_stub_x` 正常识别。
       3. 路径为 `(src_bottom.x, bus_y) -> (channel_x, bus_y) -> (channel_x, entry_y) -> (dst_top.x, entry_y) -> dst_top`（同一源节点同一目标行共享 `channel_x` 与 `entry_y` 支干，在 `entry_y` 水平分叉入各 `dst_top`，消除多目标叠线）。

---

## 6. 依赖方向与工程门禁检查

严格遵守 [`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md) §3 模块依赖 DAG 与 `scripts/pdfcraft-orgchart-module-edges.txt`：

- `request`（`NodeWidth` / `ChartStyle`）无内部模块依赖；
- `font`（`Measurer::line_height_with_gap_mm`、两阶段平衡折行）→ `error, units`（禁止引入 `content` / `page` / `graph`）；
- `content`（合并地区与日期）→ `graph, request, error`；
- `page`（三档测宽、名称降字号、`ContCue` 上限、统一盒内垂直布局模型与 `TextTone`、`WrappedLine.logical_line_index`）→ `graph, content, captions, font, request, units, error`（`page request` 边由前置修复清理后在本期随 `NodeWidth` 正式加回）；
- `layout`（按宽预算分行、统一隙缝通道绕线与 `route_multi_out` 树形支干避障）→ `page, graph, units, error`；
- `render`（按 `page` 输出的盒内布局绘制，框内字色统一黑色）→ `page, captions, graph, font, units, error`（不引入 `request` 或 `content`）。

工程规范与门禁：
- 遵守 [`rust-engineering-standard.md`](rust-engineering-standard.md) `PAN-2`/`PAN-3`、`ARC-9`、`TRK-13`、`DOC-6`、`VER-1`/`VER-2`/`VER-3`。
- 提交前按 [`rust-engineering-standard.md`](rust-engineering-standard.md) `TOOL-5` 执行完整门禁（含 `python3 scripts/check_pdfcraft_orgchart_modules.py` 与 `python3 scripts/check_readme_identity.py`），本节不重复抄录命令清单。

---

## 7. 需同步更新的文档、版本与测试清单

1. **版本与公共文档（`0.3.2 -> 0.4.0` breaking 变更）**：
   - `crates/pdfcraft-orgchart/Cargo.toml`：升版至 `0.4.0`；
   - `crates/pdfcraft-orgchart/CHANGELOG.md`：在 `0.4.0` 的 `Breaking` 下写明 `ChartStyle.node_width_mm -> ChartStyle.width: NodeWidth`、`ChartStyle::MIN_NODE_WIDTH_MM / MAX_NODE_WIDTH_MM -> NodeWidth::MIN_FIXED_MM / MAX_FIXED_MM`、`ErrorSubject::Field("style.node_width_mm") -> "style.width"` 迁移指南与新排版行为；
   - `crates/pdfcraft-orgchart/README.md` 与根 `README.md`：同步 `0.4.0` 版本号与 Capsule API 表（补充 `NodeWidth`）；
   - `crates/pdfcraft-orgchart/docs/guide.md`：同步 `ChartStyle`、`NodeWidth` 与投资节点展示规则；
   - `AGENTS.md`：更新 `pdfcraft-orgchart` 版本记录；
   - `docs/提案书.md`：新增条款覆写 §13 的 `ChartStyle` 字段与常量约定；
   - `docs/pdfcraft-orgchart-spec.md`：更新 §3.2、§3.3、§8.2、§9.2、§11.2、§12、§14，并补充 `U+00B7` 字符覆盖说明；
   - `docs/pdfcraft-orgchart-structure-spec.md`：更新 §10（移除 `max_cols` / `count_units`）；
   - `crates/pdfcraft-orgchart/src/lib.rs` 与 `src/request.rs`：更新 crate 级文档与关联常量 rustdoc 链接。
2. **单元与集成测试**：
   - `src/content/lines_tests.rs`：更新投资节点行断言（地区·日期合并及单边缺省边界）；
   - `src/font/font_tests.rs`：补充两阶段平衡折行、无损拼接、三级具名惩罚权重、开闭括号与中圆点保护、后缀保护、末行防孤字、单 token 超宽豁免测试；
   - `src/page/boxes_tests.rs`：补充三档宽度吸附（`32 / 48 / 60`）、`Fixed` 兼容、名称 `>= 3` 行降 `9.0 pt` 兜底、`overflow_box` 3 位数字预算与禁降档、`promote_to_page_subject` 投资多条 `Detail` 不粘连、名称/详情行距推进、并更新 `cue_slots_budget_three_widest_digits` 的 `38.0 mm` 上限断言；
   - `src/units.rs`：清理已删除的 `max_cols` 单元测试；
   - `src/paginate/test_support.rs`：适配 `LayoutEngine` 签名变更；
   - `src/paginate/place_tests.rs`：移除 `cols` 参数并传入精确 `printable_width_mm`，补充异宽按宽预算平衡分行、`outer/leaf` 隔离与回退、`family_bands_within_width` 离散行数缩减、异宽多行股东与投资（`route_multi_out` 同父同行共享支干与 `entry_y` 不压标签）绕线不穿框及异宽单列脊线对齐测试；
   - `src/paginate/whole_page_tests.rs` 与 `src/paginate/chain_split_tests.rs`：适配 `NodeWidth` 参数；
   - `tests/chart_new.rs`：适配 `style.width: NodeWidth` 校验与 `ErrorSubject::Field("style.width")`；
   - `tests/stroke_color.rs`：断言不出现名称/详情分隔线与灰色详情字填充；
   - `tests/to_pdf.rs` 与 `tests/page_observations.rs`：补充对外投资多字段加混合宽度及多行投资绕线的端到端测试。

---

## 8. 已确认决策记录

1. **公共 API 与提案书覆写**：`NodeWidth` 标注 `#[non_exhaustive]` 并持有 `MIN_FIXED_MM`（`20.0`）/ `MAX_FIXED_MM`（`80.0`）；`ChartStyle` 仅保留 `MAX_MARGIN_MM`（`40.0`）；错误字段改为 `ErrorSubject::Field("style.width")`；同步覆写《提案书》§13；`U+00B7` 纳入库内置文案字符集（缺字报 `MissingGlyph(Character('·'))`）。
2. **模式与视觉适用范围**：新视觉层级（`9 pt` 详情、黑色字、`1.2 pt` 详情行距、无分隔线）、两阶段平衡折行与 `ContCue` `38 mm` 上限在 `Adaptive`、`Fixed(w)` 及页主体框（含附页升格主体）全局统一生效；`Fixed(w)` 仅锁定框宽为 `w` 并禁用 `9.0 pt` 名称降档。
3. **前置独立修复**：HEAD 上未用的 `page request` 边清理与 `Graph::overflow_ids` 三元组缓存键修复均从本次 `0.4.0` 剥离，作为独立前置 PR 先行落地。
4. **模块边界与门禁**：由 `page` 输出盒内垂直布局给 `render`，不增 `render -> content` 边也不重导出 `LineKind`；§6 引用 `TOOL-5` 不重抄命令。
5. **排版、分行与绕线细则**：`outer/leaf` 不增加行数（同最小行数时独立切分，否则允许过渡行混排由绕线避障）；等代价 tie-break 先比节点数序列字典序再比行宽序列字典序；`wrap_text` 采用 `10_000 / 2_000 / 1_500` 三级具名惩罚常量；`line_height_with_gap_mm(size_pt, 0.0)` 取代 `glyph_height_mm`；`family_bands_within_width` 第二步采用离散 `R -> R + 1` 递增；`route_multi_out` 同父同行共享安全垂直支干与 `commit_bus_y` 精确 `entry_y`。
