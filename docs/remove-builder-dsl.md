# 去掉剩余 builder DSL，还原惯用 Rust

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-12 |
| 状态 | **工作区已完成（PR-A 到 PR-D），未提交，待操作者提交** |
| 基线 | `drop-dsl-ready`（`c5196a9`）：公开入口已是 `DocumentSpec` + `layout_spec` |
| 权威 | [`提案书.md`](提案书.md) §1；冲突时提案书优先 |
| 相关 | [`AGENTS.md`](../AGENTS.md)、[`module-dependency.md`](module-dependency.md) |

工作区里 PR-B、PR-C、PR-D 已落地，改动都未提交。`layout_spec` 直接构造 layout 节点。`src/document` 已删除。

## 目标

调用方与库内组装都不再使用会话式 / 闭包式 DSL。

```text
DocumentSpec（pub, owned）
  → lower_*（纯函数）→ FlowNode / SectionJob / Decorations
  → layout_pages → Rendered → to_pdf
```

## 非目标

- 不改 `layout/` / `text/` / `canvas` / `backend` 的排版语义（除非直降必须用到已有类型）。
- 不引入 serde / JSON 主接口。
- 不做提案搁置项；不删 `publish = false`。
- 不处理操作者工作区未提交的 `examples/` WIP（另分支按 Spec 重接）。

## 现状问题

公开闭包已删，但 `layout_spec` 仍绕道 `src/document/` builder：

- `Paragraph::build` / `Table::build_on` / `Flow::nested(FnOnce)` 等
- 公开空壳 `Document`（与 `Resources` 重复）

## 终态公共面（摘要）

**保留：** `DocumentSpec` 树、`layout_spec`、`Rendered`、`to_pdf`、`Resources` / `ImageRef`、既有 geom / error / limits / fonts / canvas。

**删除：** 公开 `Document` 会话 API；全部 `*::build(FnOnce)`；`IntoFlowItem` / `FlowWriter` 痕迹；理想情况下整个 `src/document/` builder 树。

**错误：** 保持 `ErrorBag` 多错误收集（未确认前不做 fail-fast）。

## PR 栈（线性）

### P0 — 前提

- 以 `drop-dsl-ready` 为基线合入或开本栈。
- 操作者 `examples/` 脏改动 stash / 另分支，不与本栈缠车。

### PR-A — 文档与契约

- 提案书扫掉 §2.0 / §2.10–§2.12 / §8 等「闭包 / DSL 入口」表述，改为 `BandSpec` / `TextRun` / `PageWhen` / `LayerMark` / `Block::Styled`。
- `AGENTS.md`、`module-dependency.md`：公共文档面是 `spec/`；DAG 含 `spec → layout, …`；过渡期可暂留 `document` 直至 PR-C。
- `CHANGELOG`：契约条目（组装亦无闭包会话）。

**验收：** 提案书与 §1 总表一致；「闭包画 header」不再是现行调用方式。

### PR-B — 单一资源入口

- 资源只走 `Resources`；删公开 `Document::new` / `set_limits` / `load_image`。
- `Rendered` 挪到 `spec`（或薄模块），`lib` 从那里导出。
- 更新文档链接与测试。

**验收：** 公开 API 无 `Document::load_image`；`cargo doc` 无死链。

### PR-C — 直降（核心）

- 在 `src/spec/`（可拆 `lower*.rs`）实现 `Spec → FlowNode / Decorations`，直接构造 layout 节点。
- 禁止再调用 builder / `FnOnce` 拼树；子流用函数返回 `Vec<FlowNode>`。
- 删除 `src/document/` builder 模块（或删到空）。
- 测试改为 Spec / `lower`；行为与 `drop-dsl-ready` 基线一致。
- 门禁全绿；perf ≤ 1.15× trunk（`letter_x8`、`wide_table_40x12`、`paragraphs_400`）。
- **不新增**公开 `Fn` / `FnMut` / `FnOnce` 参数。
- 尽量不改 `layout/` 行为。

**验收：** `rg 'FnOnce|FnMut|::build\(|FlowWriter|IntoFlowItem' src/` 无命中；examples/tests 文本与渲染对照通过。

### PR-D — 模块收尾

- 删除 `pub mod document`（若 C 后仍在）。
- `lib.rs` / README / AGENTS / `module-dependency.md` 最终态。
- 可选另 PR：重命名 `layout_spec`（默认保留，避免无意义 churn）。

## 决策（已钉）

| 项 | 决定 |
| --- | --- |
| 多错误 | 保持 `ErrorBag` |
| 图片句柄 | 终态只留 Spec 侧 `ImageRef` + layout 期 id |
| 样式作用域 | 文档描述 `Block::Styled`，不写闭包 stack |
| `layout_spec` 改名 | 默认不改；若改则单独小 PR |

## 门禁（每支可合并改动）

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --features test-utils -- -D warnings
cargo test --features test-utils
cargo doc --no-deps
```

## 操作备忘

- 本计划分支：`remove-builder-dsl`（基于 `drop-dsl-ready`）。
- 主工作区 `drop-dsl` 上未提交的 `examples/` 不属于本栈。
