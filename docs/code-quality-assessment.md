# `src/` 代码质量判断（不含 `backend/`）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10 |
| 状态 | 探讨稿。只记录判断，不改代码。逐条讨论后再定要不要动 |
| 范围 | `src/`，明确排除 `src/backend/` |
| 性质 | explanation。尚未写入提案书 |

本文汇总一次只读评估里「已经够好 / 开源或上生产前宜处理 / 0.x 可暂留」的分项。  
`Deref` 到 `Flow`：已迁到 `FlowWriter`（见 [`deref-flow-api.md`](deref-flow-api.md)、[`flow-writer-spec.md`](flow-writer-spec.md)）。

讨论与落地时遵守 [`AGENTS.md`](../AGENTS.md)「现阶段姿态」：**无生产包袱、主题内不将就**；不为「改动小」留下已知歪的错误面或 API。

## 一句话

引擎可以信任。错误还不能拿来定位。

可以继续往下做。还不宜直接拿去批量生产几十份 PDF，除非调用方自己包一层很强的上下文与二分手段。

## 已经够好的

这些不作为当前讨论的优先改动项。

- `#![forbid(unsafe_code)]`，rustfmt / clippy 钉死，机械风格统一。
- 几何品牌类型（`Finite` / `NonNegative` / `Positive`）在边界拒绝非法长度，接近 `NonZero*` 思路。
- 失败是全有或全无。画页前返回第一个错误。缺字报 `MissingGlyph`，不出静默缺字 PDF。
- 生产路径上未见由用户输入触发的 panic。`expect` 多在常量或不变量上。
- 单一手写 `Error`、不引 `thiserror` / `anyhow`，依赖面干净。骨架接近 rustls / `std::io`。
- `text` / `layout` 为 `pub(crate)`，`FlowNode` 不外泄，公共面不重导出 krilla。

## 开源或上生产前宜处理

下面按优先级粗排。真正动手顺序等逐条讨论后再定。

### 1–4. 错误可诊断性 — **工作项 1–4 已落地**

见 [`error-diagnosability.md`](error-diagnosability.md)。`Error { kind, at }`、结构化变体、`Location`、`source()` 链、`BoldFaceRequired`、删 `UnsupportedCanvasOperation`、修正 decoration 高度。**工作项 4（多错收集）已落地。**

### 5–6. 公开面清理与隐性默认 — **已落地**

| 项 | 状态 |
| --- | --- |
| `pt_near` / `PT_ABS_TOLERANCE` | 默认根 API 已撤；`feature = "test-utils"` 下从 crate 根导出 |
| `limits` | 空 `pub mod` 已删；限额 PR 再公开完整模块 |
| 粗体未注册 | `ErrorKind::BoldFaceRequired`（随错误工作项 1） |
| `ImageBox` 默认尺寸 | rustdoc 已对齐 1 px = 1 pt；行为未改 |

### 7. 组织与依赖 — **已落地**

现象摘要：`lower` 名实不符；`document` → `backend`；`error`↔`fonts`、`flow`↔节点等环；巨石 `document.rs`（**已因拆模块缓解**）。

**硬规则：** 禁止循环依赖；单向 DAG 见 [`AGENTS.md`](../AGENTS.md) 与 [`module-dependency.md`](module-dependency.md)。

**落地：** 下表各项均已实现，形态见 [`module-dependency.md`](module-dependency.md)「落地记录」。节点单元测试在 `layout/node_tests`。对 `flow` 的依赖留在该目录。

**已确认：**

| 项 | 方向 |
| --- | --- |
| 环 | 不接受「单 crate 能编译就留环」；按目标 DAG 拆 |
| `lower` | 改名为 `nesting`（或 `content_rules`），文档不再写 DSL lowering |
| `document` → `backend` | **解除**：图片注册缝在 canvas 层；`to_pdf` 进 backend；document 不 `use backend` |
| `error` ↔ `fonts` | 错误工作项顺带：`Error` 不依赖 `FontId` 类型 |
| `flow` ↔ 节点 | 抽出 `layout/frame`；节点只依赖 frame；`flow` 做枚举分发 |
| `paginate` / `decorate` | 生产单向 `paginate` → `decorate`；测试勿反转 |
| 巨石 document | **已缓解**；不再当开项 |

工作项顺序：改名/frame 拆环可与 document↔backend 分 PR；后者独立、在错误工作项之后为宜。

## 0.x 可暂留

这些可以记着，不阻塞当前里程碑。

- 大量 `#![cfg_attr(not(test), allow(dead_code))]`（接线期常见）。`hline` / `image` 与多数 layout 文件不统一，是小问题。
- builder 返回值三种并存（`()`、`&mut Self` 链、消费 `Self`）。`must_use` / `Debug` / `#[non_exhaustive]` 半覆盖。体验问题，不是正确性问题。
- 测试浮点断言三套（`to_bits`、`pt_near`、裸 `assert_eq`）。只影响维护。
- 公开与内部同名类型（如 `Border`）。内部为 `pub(crate)`，外部看不到。
- ~~`Deref` / `DerefMut` 到 `Flow`~~。**已迁移**为 `FlowWriter` + 公开 `add` + 糖，见 [`deref-flow-api.md`](deref-flow-api.md)。

## 分项 verdict

| 维度 | 判断 |
| --- | --- |
| 模块组织 | 目标 DAG 已落地（[`module-dependency.md`](module-dependency.md)）：生产代码无环，document 不再依赖 backend |
| 风格 | 机械层统一。API 三种形态 0.x 可接受，1.0 前宜收敛 |
| 报错 / 批跑 | 工作项 1–4 已落地（定位 + `source()` + `Errors` 多错收集） |
| 对照官方 Rust | `Error` 已是 kind+location+source。正文入流为 `FlowWriter` |

## 两类读者分别要知道的

**迁几十份 PDF 的调用方。**

- 可以依赖「一份失败就整份不出」和「缺字必报」。
- `Error.at` 可带节/页/页眉页脚/单元格。`document` 失败时返回 `Errors`（最多 64 条，发现序，截断另报）。见 [`error-diagnosability.md`](error-diagnosability.md)。
- `pt_near` 仅在 `test-utils` feature 下从 crate 根导出。

**维护者。**

- 依赖只许沿 [`module-dependency.md`](module-dependency.md) 的 DAG 向下；禁止环。
- DSL→节点转换在 `document/*`；`layout/nesting`（原 lower）只做嵌套内容规则。
- 补错误上下文：组装与 layout 的 `Location` 钩子；动 `decorate` 时注意它会改写下层错误语义。

## 讨论顺序建议

1. 报错可诊断性——**工作项 1–4 已落地**。
2. 公开面清理——**已落地**。
3. 组织与依赖——**已落地**，见 [`module-dependency.md`](module-dependency.md)「落地记录」。
4. `Deref` 到 `Flow`——**已落地**。
