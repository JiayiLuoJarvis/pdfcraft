# 模块依赖方向（已确认）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10 |
| 状态 | **方向已确认，组织刀已落地。** 硬规则见 [`AGENTS.md`](../AGENTS.md)；本文是目标 DAG、拆环刀序与落地记录 |
| 相关 | [`code-quality-assessment.md`](code-quality-assessment.md) §7 |

## 硬规则

1. **禁止循环依赖**（crate 间与 `src/` 模块间同一标准）。边只许沿 DAG 向下。
2. **单 crate 能编译出环，也不接受。** Rust 允许同 crate 模块互引用；本仓库仍按 DAG 约束，发现环就拆。
3. **测试代码不反转生产依赖。** 若 A 的单元测试需要 B，而生产上是 B→A，把测试挪到 B、`layout/test_util`，或集成测试，而不是让 A 的非 test 模块 `use` B。

## 目标 crate 级 DAG

箭头表示「可以使用 / 依赖」。

```text
geom
error          → geom（若需要）；不依赖 fonts / layout / document / backend
fonts          → error, geom
canvas         → error, geom
text           → fonts, error, geom
layout         → text, fonts, canvas, error, geom
document       → layout, canvas, fonts, error, geom
backend        → canvas, fonts, error, geom, krilla
lib / 薄 glue  → document, backend（仅组装 document() 入口与 to_pdf，不引入新业务环）
```

**禁止：**

- `document` → `backend`（含 `ImageTable`、`KrillaCanvas`）
- `layout` → `document` 或 `backend`
- `backend` → `layout` 或 `document`
- `error` → `fonts`（`FontId` 由 fonts 拥有；错误里用 `u32` 或只在 Display 层格式化）
- 任何模块 → krilla，除 `backend/`

**图片注册缝：** document 只依赖 canvas（或同层）上的注册抽象与 `DecodedImage` / `ImageId`；krilla 解码表与 `to_pdf` 实现落在 `backend`。无 `backend-krilla` 时用拒绝实现，document 源码不 `use crate::backend`。

## 目标 layout 内部 DAG

今日环：`flow` 持有 `Frame`/`Split` 且枚举引用各节点，各节点又 `use flow` → **环**。目标拆成：

```text
layout/style
layout/frame          → Frame, Split, Fragment, LayoutCtx（无具体节点类型）
layout/nesting        → 原 lower：check_content / CellBody / ItemBody
                        （可依赖 FlowNode 若枚举上提；或只依赖节点 kind）
layout/paragraph ─┐
layout/columns   ─┤
layout/table     ─┼→ frame, style, nesting, text/fonts/canvas…
layout/list      ─┤
layout/image     ─┤
layout/hline     ─┘
layout/flow           → 上述节点 + frame：FlowNode 枚举与分发 split/draw
layout/decorate       → flow, nesting, …
layout/paginate       → decorate, flow, …
```

**方向：** `paginate` → `decorate` → `flow` → 各节点 → `frame`。  
**禁止：** `decorate`（生产代码）→ `paginate`；各节点 → `flow`（节点只依赖 `frame`）。

`lower` **改名为** `nesting`（或 `content_rules`），文档不再写「DSL lowering」。

## 与现状的差（拆环刀）

| 环 / 偏离 | 处理 | 建议刀 |
| --- | --- | --- |
| `error` ↔ `fonts` | `Error`/`ErrorKind` 不引用 `FontId` 类型 | 随错误刀 1 |
| `flow` ↔ 各节点 | 抽出 `layout/frame`；节点只依赖 frame | 组织刀（可先于或后于 document↔backend） |
| `decorate` 测试 → `paginate` | 测试挪走，生产边只保留 paginate→decorate | 同上组织刀内 |
| `document` → `backend` | Image 注册缝 + `to_pdf` 进 backend | **独立刀**（评估 §7-B） |
| `lower` 名实 | 改名 `nesting` | 小刀，可并组织刀 |
| 巨石 `document.rs` | 已拆模块 | **已缓解**，仅更新评估表述 |

## 落地记录

全部拆环刀已落地，生产代码里没有反向边：

| 项 | 落地形态 |
| --- | --- |
| `error` ↔ `fonts` | 错误刀 1 已解；`ErrorKind::MissingGlyph.font` 是 `u32` |
| `lower` 名实 | 改名 `layout/nesting`；规则矩阵按 `NodeKind` 判断，不依赖 `FlowNode` |
| `flow` ↔ 各节点 | `layout/frame` 持有 `Frame`、`Split<T>`、`Fragment`、`FragmentCommand`、`PlacedFragment`、`LayoutCtx`、`natural_height`、`stacked_height`、`draw_fragment`，以及库内 trait `Flowable`（split / stack / kind / nested）。叶子节点返回 `Split<Self>`；容器（栏、表、列表）对子节点类型泛型 `N: Flowable`。`flow` 持有 `FlowNode` 枚举，把各节点的 tail 映射回 `FlowNode`。静态分发，没有 trait 对象 |
| `decorate` → `paginate` | `SectionSpan` 归 `decorate`；`decorate` 测试的页面夹具挪进 `layout/test_util` |
| `document` → `backend` | `canvas::ImageRegistry`（`Any + Send + Sync`）是注册缝，`canvas::RejectingImageRegistry` 是无后端实现。`backend::ImageTable` 实现它；`backend::write_pdf` 把录制回放成 PDF。`lib.rs` 的 `document()` 在开 `backend-krilla` 时换上 `ImageTable`；`quire_pdf::to_pdf(rendered)` 调 `backend::write_pdf` |

**未清项：** 各节点模块的单元测试（`#[cfg(test)]`）仍 `use crate::layout::flow::FlowNode`，并经 `test_util` 走 `paginate`。这些是只在测试里出现的反向边，与上文硬规则 3 不符；整体挪到 `flow` / `paginate` 测试或集成测试是单独一刀。

## 明确不做

- 为「无环」拆成多个 crates（现阶段单 crate + 模块 DAG 即可）。
- 为消环把 `FlowNode` 改成 trait 对象热路径（除非测量证明必要）。
- 在 AGENTS 里把 `document → backend` 写成合法例外。
