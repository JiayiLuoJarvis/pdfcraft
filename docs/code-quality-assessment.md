# `src/` 代码质量判断（不含 `backend/`）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10 |
| 状态 | 探讨稿。只记录判断，不改代码。逐条讨论后再定要不要动 |
| 范围 | `src/`，明确排除 `src/backend/` |
| 性质 | explanation。尚未写入提案书 |

本文汇总一次只读评估里「已经够好 / 开源或上生产前宜处理 / 0.x 可暂留」的分项。  
`Deref` 到 `Flow`：已迁到 `FlowWriter`（见 [`deref-flow-api.md`](deref-flow-api.md)、[`flow-writer-spec.md`](flow-writer-spec.md)）。

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

### 1. 错误可诊断性不足

现象。

- `InvalidGeometry` 是单元变体，`Display` 固定为 `"geometry is invalid"`，却覆盖十余种原因（行宽非法、页边吃掉版心、零长线段、列表序号越界等）。
- `InvalidCellSpan` 无行列。
- `UnsplittableTallerThanFrame` 无尺寸。
- `ColumnWidths { reason }` 用 `&'static str`，且 `column-count`、`no-remaining-width` 一词多义。

对照。`serde_json` / `toml` 至少在 `Display` 里给出位置或更具体的原因。

对批跑。几十份里有一份报 `"geometry is invalid"`，只能二分模板。

### 2. 只保留第一个错误，且无文档位置

现象。DSL 的 `note()` 只记首错。后续错误丢弃。`Display` 无页码、节序号、节点路径、源文本片段。

对批跑。调用方必须自己包装文件名或模板名。修一个错再跑，才看到下一个。

### 3. 外来错误被字符串化，无 `source()` 链

现象。`Font { message }`、`Image { message }`、`Backend(String)` 把底层失败收成字符串。`impl std::error::Error for Error {}` 为空，`source()` 恒为 `None`。

对照。Rust API Guidelines 期望保留错误链。用 `{:#}` 或 anyhow 包装时看不到底层类型。

### 4. 死变体与误导性字段

现象。

- `UnsupportedCanvasOperation` 当前树无生产构造点，却因 `non_exhaustive` 仍须被外部 match。
- 页眉页脚超高时，`decorate` 可能把下层 `UnsplittableTallerThanFrame` 改写成 `DecorationTallerThanMargin { height: page_h }`。这里的 `height` 可能是页高，不是测得内容高。

### 5. 测试工具与空模块进了公开面

现象。

- `pt_near`、`PT_ABS_TOLERANCE` 从 crate 根重导出。
- `limits` 是空的 `pub mod`，crate 内无人使用。

风险。发版后都是 semver 负担。

### 6. 隐性默认难读

现象。

- 未注册粗体时，最终可能报 `UnknownFont { id: 1 }`。`1` 是内部编号，对调用方难读。
- `ImageBox` 两边尺寸都不设时，实现是 1 px = 1 pt（`layout/image.rs` 的 `resolve_size`）。公开文档没写全。

### 7. 组织名实不符与依赖方向

现象。

- `layout/lower.rs` 模块文档写 lowering，实际主要是 `check_content` 与 `CellBody` / `ItemBody`。DSL → `FlowNode` 的转换在 `document.rs`。
- feature `backend-krilla` 下，`document.rs` 直接依赖 `crate::backend`（如 `ImageTable`、`KrillaCanvas`）。与 AGENTS.md「document → layout → …；backend 单向」的叙述有第一次偏离。
- `error` ↔ `fonts`、`paginate` ↔ `decorate`、`flow` ↔ 各节点存在环。单 crate 能编译，但削弱按层阅读。
- `document.rs` 约 2613 行，builder 与转换混在一起。

这些主要影响维护者，其次影响开源读者的第一印象。

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
| 模块组织 | 分层方向对，封装到位。名实不符与 document→backend 依赖宜在开源前收 |
| 风格 | 机械层统一。API 三种形态 0.x 可接受，1.0 前宜收敛 |
| 报错 / 批跑 | 安全够用。可诊断性不够，是上生产前最大缺口 |
| 对照官方 Rust | 错误骨架像 rustls / std。上下文与 `source()` 达不到 serde_json / toml。正文入流已改为 `FlowWriter`（不再用 `Deref` 转发） |

## 两类读者分别要知道的

**迁几十份 PDF 的调用方。**

- 可以依赖「一份失败就整份不出」和「缺字必报」。
- 需要自己包装每份文档的上下文。不要指望 `Error` 告诉你第几页、哪个单元格。
- 遇到 `InvalidGeometry` / `InvalidCellSpan` / `ColumnWidths` 时，预期要二分模板。
- 不要依赖 `pt_near`、`limits` 这类公开项。

**维护者。**

- 读代码从 `document.rs` 开始，不要从 `layout/lower.rs` 开始。
- 以后要补错误上下文，最自然的接入点是 `note()` 和各处 `lower()`。
- 动 `decorate` 时注意它会改写下层错误语义。

## 讨论顺序建议

1. 报错可诊断性（本文 §1–§4）。
2. 公开面清理（本文 §5–§6）。
3. 组织与依赖（本文 §7）。
4. `Deref` 到 `Flow`（[`deref-flow-api.md`](deref-flow-api.md)）——**已落地**。

你确认或改掉一条，再进下一条。未确认前不改代码。
