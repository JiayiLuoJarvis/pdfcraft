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

### 1–4. 错误可诊断性 — **方向已确认**

现象摘要（仍成立）：`InvalidGeometry` 等不可定位；只记首错且无位置；依赖错误字符串化且无 `source()`；死变体与误导字段。

**已确认方向**见 [`error-diagnosability.md`](error-diagnosability.md)：`Error { kind, at }`、变体结构化、Location、依赖错误翻译+`source()` 链、删 `UnsupportedCanvasOperation`。多错收集为后续刀。

### 5–6. 公开面清理与隐性默认 — **方向已确认**

现象摘要（仍成立）：`pt_near` / `PT_ABS_TOLERANCE` 进 crate 根；空的 `pub mod limits`；粗体缺失报成 `UnknownFont { id: 1 }`；`ImageBox` rustdoc 与提案书/实现不一致。

**已确认：**

| 项 | 方向 |
| --- | --- |
| `pt_near` / `PT_ABS_TOLERANCE` | 移出默认根 API；经 **`feature = "test-utils"`** 导出，供集成测试使用 |
| `limits` | **先撤公开空壳**（删或改内部）；`MAX_*` 等限额 PR 做实后再以完整模块公开 |
| 粗体未注册 | **并入错误刀 1**：独立 `ErrorKind`（如 `BoldFaceRequired`），不再用 `UnknownFont { id: 1 }` |
| `ImageBox` 默认尺寸 | **只修 rustdoc**，行为保持提案书 §2.7：两边都不给 → **1 px = 1 pt** |

实现可与错误刀分开的小 PR；粗体 kind 必须跟错误刀 1。

### 7. 组织与依赖 — **方向已确认**

现象摘要：`lower` 名实不符；`document` → `backend`；`error`↔`fonts`、`flow`↔节点等环；巨石 `document.rs`（**已因拆模块缓解**）。

**硬规则：** 禁止循环依赖；单向 DAG 见 [`AGENTS.md`](../AGENTS.md) 与 [`module-dependency.md`](module-dependency.md)。

**已确认：**

| 项 | 方向 |
| --- | --- |
| 环 | 不接受「单 crate 能编译就留环」；按目标 DAG 拆 |
| `lower` | 改名为 `nesting`（或 `content_rules`），文档不再写 DSL lowering |
| `document` → `backend` | **解除**：图片注册缝在 canvas 层；`to_pdf` 进 backend；document 不 `use backend` |
| `error` ↔ `fonts` | 错误刀顺带：`Error` 不依赖 `FontId` 类型 |
| `flow` ↔ 节点 | 抽出 `layout/frame`；节点只依赖 frame；`flow` 做枚举分发 |
| `paginate` / `decorate` | 生产单向 `paginate` → `decorate`；测试勿反转 |
| 巨石 document | **已缓解**；不再当开项 |

刀序：改名/frame 拆环可与 document↔backend 分 PR；后者独立、在错误刀之后为宜。

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
| 模块组织 | 目标 DAG 与禁环已确认（[`module-dependency.md`](module-dependency.md)）。拆环与 document↔backend 解耦待落地 |
| 风格 | 机械层统一。API 三种形态 0.x 可接受，1.0 前宜收敛 |
| 报错 / 批跑 | 安全够用。可诊断性不够，是上生产前最大缺口 |
| 对照官方 Rust | 错误骨架像 rustls / std。上下文与 `source()` 达不到 serde_json / toml。正文入流已改为 `FlowWriter`（不再用 `Deref` 转发） |

## 两类读者分别要知道的

**迁几十份 PDF 的调用方。**

- 可以依赖「一份失败就整份不出」和「缺字必报」。
- 需要自己包装每份文档的上下文。不要指望 `Error` 告诉你第几页、哪个单元格。
- 遇到 `InvalidGeometry` / `InvalidCellSpan` / `ColumnWidths` 时，预期要二分模板。
- `pt_near` 将只在 `test-utils` feature 下可用；不要把空 `limits` 当正式 API（将先撤壳）。

**维护者。**

- 依赖只许沿 [`module-dependency.md`](module-dependency.md) 的 DAG 向下；禁止环。
- DSL→节点转换在 `document/*`；`layout/lower`（将改名 nesting）只做嵌套内容规则。
- 补错误上下文：`note()` 与各处 `lower()`；动 `decorate` 时注意它会改写下层错误语义。

## 讨论顺序建议

1. 报错可诊断性（本文 §1–§4）——**方向已确认**，见 [`error-diagnosability.md`](error-diagnosability.md)。
2. 公开面清理（本文 §5–§6）——**方向已确认**。
3. 组织与依赖（本文 §7）——**方向已确认**，见 [`module-dependency.md`](module-dependency.md)。
4. `Deref` 到 `Flow`（[`deref-flow-api.md`](deref-flow-api.md)）——**已落地**。

评估条目讨论已收束。实现按各专项刀序；未开刀前不改代码。
