# 执行规格：`FlowWriter`、公开 `add`、拆分 `document.rs`

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10 |
| 状态 | **已落地**（`FlowWriter` + 模块拆分；Cell 对齐已在后续工作项落地，见 §3「明确后续工作项」）。 |
| 权威决策 | [`docs/deref-flow-api.md`](deref-flow-api.md) |
| 提案书 | 不改 § 已确认产品语义；本工作项是公开面建模与模块组织。勿提前把未钉签名写进提案书。 |
| 现状代码 | 单体 `src/document.rs`（约 2600 行），`Document` / `Section` `Deref` → `Flow` |

## 1. 目标与非目标

### 目标

1. 删除 `Document` / `Section` 对 `Flow` 的 `Deref` / `DerefMut`。
2. 引入公开 trait **`FlowWriter`**：`Flow` / `Section` / `Document` 实现之。
3. 元素为独立值；公开 **`add`** 入流；`paragraph` / `table` / … 为糖（内部走 `add` + `El::build` 或等价）。
4. 将单体 `src/document.rs` **拆成 `src/document/` 模块树**；`lib.rs` 对外重导出路径保持可用（`quire::Document` 等不变）。
5. 保留 Implicit / rejecting：显式 `section()` 后顶栏再写正文 → `ContentOutsideSection`。
6. 日常示例 / 集成测试主路径仍写糖；需要时 `use quire::FlowWriter`。

### 非目标（本规格明确不做）

- 不改 `Cell::*` 入口与 `Flow` 对齐（下一工作项）。
- 不改主 API 为「只教 `add`、废除糖」。
- 不做 `doc.body().paragraph` 方案 B。
- 不改 layout 分页/测量行为；不改 backend。
- 不把 `FlowNode` 升格为调用方日常类型（入流类型另定，见 §4）。
- 不改提案书已定产品语义（分节、延迟错误、缺字即错等）。

## 2. 验收标准

全部满足才算完：

- [x] 仓库内无 `impl Deref for Document` / `Section`（及 `DerefMut`）。
- [x] 存在 `pub trait FlowWriter`，crate 根重导出；`Flow` / `Section` / `Document` 均实现。
- [x] 存在公开 `add`；糖方法经 `add`（或经与 `add` 同一条入流路径）追加节点。
- [x] `src/document.rs` 变为目录模块（`src/document/mod.rs` + 子文件）；无巨型单文件承载全部 DSL。
- [x] 显式分节后 `doc.paragraph` / `doc.add(...)` 仍报 `ContentOutsideSection`（既有测试保留或等价加强）。
- [x] `cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 通过；有 backend feature 时一并测。
- [x] 集成测试 / 文档示例：主路径只教糖；至少一处（单元或文档测）展示 `add` + 工厂函数形态。
- [x] 最小行为 diff：布局坐标、错误种类不无故变更（除 API 表面从 Deref 改为 trait）。

## 3. PR 切分（必须按序）

### PR 1 — 机械拆分 `document.rs`（行为零变化）

**只搬家，不改语义。** 仍可暂时保留 `Deref`，降低单 PR 风险。

建议模块树（文件名可微调，职责不要混）：

```text
src/document/
  mod.rs          // document()、重导出、模块文档；薄
  token.rs        // DocToken（crate 内）
  section.rs      // Section、Sections（若现为私有枚举一并迁）
  document_ty.rs  // Document、font/margins/section/header/…、paint/Rendered 入口相关
                  // （若 Rendered 独立更清晰，可 rendered.rs）
  flow.rs         // Flow、push/nested/into_nodes、现有正文入口方法（PR1 可原样）
  flow_writer.rs  // PR1 可先占位空 mod 或留到 PR2；不要提前半套
  image.rs        // Image、ImageBox
  hline.rs        // HLine
  paragraph.rs    // Paragraph、Blank
  columns.rs      // Columns
  table.rs        // Table、Track、Border、Cell（Cell 本工作项只搬家，不改 API）
  list.rs         // List、Numbering
  page.rs         // Page
  band.rs         // Band
  layer.rs        // Layer、Shapes、TextAt
  style_props.rs  // TextProps（或并入 paragraph/layer 之一，避免循环依赖）
  tests.rs        // 原 mod tests 迁出，或留 mod.rs 底部；优先独立文件
```

约束：

- `lib.rs` 的 `pub use document::{...}` 名单可增 `FlowWriter`（若 PR1 未引入则 PR2 再加），**既有名字不能无故改名/消失**。
- 依赖方向：元素模块 → `flow` / 共享私有辅助；`document_ty` → `section` / `flow`；**禁止** layout 反向依赖 document。
- `pub(crate)` / 私有项可见性以能编译为准，不借拆分扩大公开面。
- PR1 结束：测试全绿；`git diff` 应以 `mv` + 路径修正为主。

### PR 2 — `FlowWriter` + 删 `Deref` + 公开 `add` + 糖改道

在已拆模块上改语义。

1. 删除所有 `Deref`/`DerefMut` for `Document`/`Section`。
2. 实现 `FlowWriter`（见 §4）。
3. 引入入流类型与 `add`；各元素提供 `build`（或固有构造）产出可 `add` 的值。
4. `Flow` 上原 `pub fn table` 等：改为 `FlowWriter` 默认方法，或 `Flow` 固有方法转调 `add`——**对外调用仍是 `doc.table(|…|)`**，但方法解析来自 trait（或 `Flow` 固有 + Document/Section 经 trait）。推荐：**糖在 `FlowWriter` 默认方法**，避免三处固有重复。
5. `Document::flow_mut`：Implicit → 节 flow；Explicit → rejecting flow（从今日 `DerefMut` 原样迁）。
6. 测试：凡 `doc.paragraph` 等，按需 `use quire::FlowWriter`（或 crate 内 prelude 仅测用；**不强制**公开 `quire::prelude` 除非实现时发现必要）。
7. rustdoc：`Document` 模块文档改为 body 方法来自 `FlowWriter`；`add` 注明高级用途。

### PR 3（可选、可同 PR2 若 diff 仍小）— 教法与一处 `add` 示例

- 若有 README/examples：主路径只糖。
- 增加或改一条测试：`fn make_…() -> Table` + `doc.add(...)`。

### 明确后续工作项（本规格外）

- Cell 入口与 `FlowWriter`/`add` 共享构建结果。**已落地**：密封 `IntoCellItem`（`Paragraph`、`HLine`、`PlacedImage`、`Spacer`、`ListBlock`）+ `Cell::add` / `Cell::empty`；`Cell::*` 糖即 `empty().add(…)`；格内非列表子项经 `IntoFlowItem::add_to` 降低；正文 `ul` / `ol` / `spacer` 改走 `ListBlock` / `Spacer`。`Cell` 不实现 `FlowWriter`。
- 若 `document/` 某文件仍过大，再拆（如 `table/cell.rs`）。

## 4. API 形状（实现可微调名，语义不可偏）

### 4.1 `FlowWriter`

```rust
pub trait FlowWriter {
    /// 取得当前写入目标。Document 在此实现 rejecting 切换。
    /// 日常调用方应优先用糖或 `add`；勿在教程当主 API。
    fn flow_mut(&mut self) -> &mut Flow;

    fn add(&mut self, item: impl IntoFlowItem) { /* 默认：lower/push 到 flow_mut */ }

    fn paragraph(&mut self, f: impl FnOnce(&mut Paragraph)) { … }
    fn columns(&mut self, f: impl FnOnce(&mut Columns)) { … }
    fn table(&mut self, f: impl FnOnce(&mut Table)) { … }
    fn ul(&mut self, f: impl FnOnce(&mut List)) { … }
    fn ol(&mut self, numbering: Numbering, f: impl FnOnce(&mut List)) { … }
    fn image(&mut self, image: &Image, f: impl FnOnce(&mut ImageBox)) { … }
    fn hline(&mut self, f: impl FnOnce(&mut HLine)) { … }
    fn spacer(&mut self, height: impl IntoNonNegative) { … }
    fn page_break(&mut self) { … }
    fn keep_together(&mut self, f: impl FnOnce(&mut Flow)) { … }
    fn stack(&mut self, style: impl FnOnce(&mut TextProps), body: impl FnOnce(&mut Flow)) { … }
}
```

糖列表必须覆盖今日经 `Deref` 暴露的正文方法（与 [`deref-flow-api.md`](deref-flow-api.md) 清单一致）。

`stack` / `keep_together`：若不宜先建成「独立值再 add」，允许在 `FlowWriter` 默认方法里直接操作 `flow_mut()`，但不得因此保留 `Deref`。文档中可注明二者是流控制糖，而非与 `Table` 同类的块元素值——**实现时选一条并在 rustdoc 写清，勿两套并行语义**。

### 4.2 `add` 与入流类型

实现时选定一种，写入 PR 描述即可（不必先改提案书）：

| 候选 | 说明 |
| --- | --- |
| `IntoFlowItem` trait | 各元素 `impl IntoFlowItem`；`add` 内 lower 成节点并 `push` |
| 公开 `enum FlowItem` | 包装可入流块；元素 `From`/`Into` |
| 仅对具体类型重载 | 多个 `add_table` 等——**不推荐**，与「一个 add」决议不符 |

约束：

- **不要**把内部 `FlowNode` 直接变成调用方主 `add` 参数类型（除非评估后认为不可避免，且 rustdoc 标明不稳定/易变——默认避免）。
- `Table::build` / `Paragraph::build`（名可用 `new`+闭包）跑闭包并产出可 `add` 的值；延迟错误模型与今日一致（builder 记错，`document()` 出口返回）。

### 4.3 rejecting

- 逻辑从今日 `Document` 的 `DerefMut` 迁到 `Document` 的 `flow_mut`。
- 糖与 `add` 都必须走该入口，禁止旁路写到某一显式节。

### 4.4 可见性

| 项 | 要求 |
| --- | --- |
| `FlowWriter` | `pub`，`lib.rs` 重导出 |
| `add` | `pub`（经 trait） |
| `flow_mut` | trait 上需可调用；可用文档降权；若用 sealed 子 trait 藏钩子，须仍让默认方法可编译 |
| `DocToken` 等 | 保持非公开或 `pub(crate)` |

## 5. 拆分时的依赖与循环

已知风险点（搬家时注意）：

- `Table` / `Columns` / `List` 持有或创建子 `Flow`（`parent: Flow` / `child()`）。
- `Cell` 延迟列表闭包生命周期与 `Table` 交织。
- `Layer::stack` 与 `TextProps` 和 `Flow::stack` 相似但不入 `FlowWriter`。

规则：先让 PR1 编译通过，再在 PR2 改入口；不要在搬家同时重写 Cell。

## 6. 测试与迁移清单

- `tests/**`、`src/document/**/tests*`：补 `use quire::FlowWriter`（若需要）。
- 保留分节拒收、隐式节正文、嵌套 `col`/`item` 上的正文方法测试。
- 新增：`add` 路径与糖路径对同一简单段落/表产生等价 `RecordingCanvas` 或等价节点结构（选一种稳定断言，勿比 PDF 字节）。
- 不删除既有行为测试来「绿」；失败则修实现。

## 7. 文档同步（实现 PR 内）

- 更新 [`deref-flow-api.md`](deref-flow-api.md) 状态：方向已确认 → **已落地**（或勾选验收）。
- [`code-quality-assessment.md`](code-quality-assessment.md) 中 Deref 条改为已迁移。
- 本文件验收表打勾。
- **不要**顺手写无关 markdown。

## 8. 工程约束（仓库惯例）

- Edition 2021；`forbid(unsafe_code)`；最小 diff；不顺手做 Cell/报错上下文大改。
- 依赖方向：`document` → `layout` / `fonts` / `text` / `canvas`；无 `krilla`。
- 命令：`cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（及 feature）。

## 9. 建议实施顺序（单会话内）

1. 读本规格 + [`deref-flow-api.md`](deref-flow-api.md)「已确认」+ 现状 `Deref`/`Flow::table` 一段。
2. 做 **PR1 拆分**，绿测。
3. 做 **PR2 语义**，绿测。
4. 补 `add` 示例测与文档状态。
5. 停。Cell 对齐另开会话/工作项。
