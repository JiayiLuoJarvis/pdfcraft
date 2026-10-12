# 错误可诊断性：已确认方向

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10（刀 1–3）；刀 4 落地 2026-10-12 |
| 状态 | **刀 1–4 已落地**（`Error { kind, at }`、`Location`、`source()` 链、`Errors` 多错收集）。 |
| 相关 | [`code-quality-assessment.md`](code-quality-assessment.md) §1–§4；姿态见 [`AGENTS.md`](../AGENTS.md)「现阶段姿态」 |
| 代码 | 主要 `src/error.rs`；构造点遍布 `document/`、`layout/`、`text/`、`fonts.rs`、`backend/` |

## 已确认

1. **根形状**改为大致：
   ```rust
   pub struct Error {
       pub kind: ErrorKind,
       pub at: Option<Location>,
   }
   ```
   `kind` = 是什么错；`at` = 在哪（未知则 `None`）。不为每个变体重复抄位置字段。
2. **§1 变体结构化**：`GeometryKind` / `InvalidCellSpan` 带 kind+行列 / `UnsplittableTallerThanFrame { height, avail }` / `ColumnWidthsReason` 枚举。禁止再扩 `&'static str` reason。
3. **§2**：本阶段承诺填 `Location`（节、表行列、页码、header/footer 等 layout/组装可知信息）。**不做**调用方源码行号。多错收集见刀 4（已落地）。
4. **§3**：手写 `Error`，实现真正的 `source()`；不引入 `thiserror` / `anyhow`。依赖错误按下文「依赖错误约定」处理。
5. **§4**：修正 `DecorationTallerThanMargin` 的 `height` 语义（禁止用 `page_h` 冒充内容高）；**删除**无构造点的 `UnsupportedCanvasOperation`。
6. **粗体未注册**（评估 §6）：刀 1 增加独立 kind（如 `BoldFaceRequired`），禁止再报 `UnknownFont { id: 1 }`。
7. **0.x 可 breaking**；主题内不将就。

### 刀序

| 刀 | 内容 |
| --- | --- |
| 1 | `Error` / `ErrorKind` / `Location` 骨架；§1 结构化；§4 删/修；`BoldFaceRequired`；测试跟 |
| 2 | 组装与 layout 填 `Location` |
| 3 | 依赖错误的 `source()` 链（Font / Image / Backend 等） |
| 4 | 多错收集 API — **已落地** |

`Location` / `ErrorKind` 的精确字段名与变体表在刀 1 实现 PR 定；勿提前冻进提案书。

### 落地记录（刀 1–3）

- `Location { section, page, region, cell }`：`section` 从 0 计；`page` 是全文档 1 起的页码；`region` 为 `Body` / `Header` / `Footer` / `Behind` / `Front`；`cell` 为表格网格 0 起的行列（含表头行）。内层先写，外层不覆盖。
- `InvalidCellSpan` 只带 `CellSpanReason`，行列放在 `at.cell`，不在变体里重复。
- `source` 私有，存 `Arc<dyn Error + Send + Sync>`，以保住 `Error: Clone`。`PartialEq` 只比 `kind` 与 `at`。
- `Display` 不拼 source 文本，按 Rust 错误惯例留给 `source()` 链。
- 仍为 `None` 的位置：组装期在单元格构造时就拒绝的内容（此时还没有网格行列），以及 `to_pdf` 收尾阶段的序列化失败。

### 刀 4：多错收集（已落地）

1. **做不做 / 何时**  
   本里程碑做。不延后到限额或信件夹具之后。

2. **收集范围**  
   - 组装期：凡今日走 `note()` / `Flow::push(Err)` / `Vec<Result>` collect 的路径，改为写入同一错误表。  
   - 测量期：同一跑里收集可安全继续的失败，至少包括 `MissingGlyph`、`BoldFaceRequired`、`UnknownFont`（与组装错同一桶）。  
   - 几何 / 分页硬失败：记入表后可 hard-stop 后续 layout；不假装继续排出稳定后续页。  
   - `to_pdf` / backend：仍单错，不并进「一次跑多错」承诺。  
   - 缺字、几何、单元格：同一 `Error` / `Errors` 桶，不按业务拆 API。

3. **API 形状**  
   - `Error` 仍表示单错（`kind` + `at` + 可选 `source`）。  
   - 新增非空 `Errors`（名称实现时定；须 `Clone`、按条 `PartialEq`、可迭代）。  
   - 公共 `Result<T> = std::result::Result<T, Errors>`。`From<Error> for Errors`。  
   - `document` / `paint` / `render` 失败时，`Err` 即为完整列表（受上限截断）。无第二套「再取列表」入口。

4. **与整份失败**  
   有任一错即不产出 `Rendered` / PDF。刀 4 只增加诊断，不引入部分页成功。与提案书 §4「缺字整份失败」一致。

5. **上限与排序**  
   - 最多收集 64 条。满则停止追加，并标明截断。  
   - 顺序为库遍历发现序（组装文档序，再测量序）。纠正今日「容器 `self.error` / `Document.error` 在 lower 时压过更早子项错」的优先级扭曲。不做按 kind 稳定重排。

6. **继续 vs hard-stop**  
   - 组装：记下错误后 builder 闭包仍走完；已有错时仍 lower 兄弟以收集，但不把节点纳入成功树。  
   - 必须 hard-stop 的例子：画布生命周期已破坏、backend 序列化失败、分页已 stalled / 版心非法以致继续测量无意义。  
   - 内层今日已跳过闭包的路径（空 link、非法 `col_fixed` 等）可保持跳过，不强行制造更多错。

7. **Display / PartialEq / Clone**  
   - `Errors` 的 `Display`：多行（或编号）打印每条，单条格式与今日 `Error` 一致。  
   - `PartialEq`：逐条比 `kind` 与 `at`（与单条一致，忽略各条 `source`）。  
   - 保持 `Clone`，以便文档侧克隆延迟错误表。

8. **明确不做**  
   - 调用方源码行号  
   - `thiserror` / `anyhow` 进公共面；公共面泄漏 krilla/skrifa 错误类型  
   - 并行收集  
   - 部分页 / 部分 PDF  
   - 未确认功能顺手塞进本刀  

实现提示（非 API 承诺）：改动重心是 `note` 与各 `error: Option<Error>` → 错误表、`Flow::push`、`List`/`Columns` 的 `collect`、`measure_text` 的缺字继续策略、`src/error.rs` 的 `Errors` 与 `Result`。遵守 [`module-dependency.md`](module-dependency.md)：`error` 不依赖 document/layout。

### 落地记录（刀 4）

- 公共 `Result<T>` 是 `Result<T, Errors>`。`Errors` 非空，最多保留 64 条，顺序是发现序。访问器是 `first`、`as_slice`、`iter`、`truncated`。`From<Error> for Errors`。
- 恰好一条且未截断时，`Display` 与该条 `Error` 相同。否则按编号逐条打印。截断时末行是 `further errors omitted`。
- `PartialEq` 只比各条 `kind` 与 `at`，不比 `truncated`。`source()` 仅在恰好一条时转给该条，多条时为 `None`。
- `Document::section` 返回 `()`。闭包返回的 `Err` 并进文档的错误表，后面的节继续执行。调用方 `build` 闭包若返回 `Err`，吸收该错误后不再测量。
- 组装记下错误后仍 lower 兄弟节点。失败节点不进入成功树。表在 lower 时先收集单元格，再记录列数等容器错误，容器错不再盖住更早的子项。
- 测量把 `MissingGlyph`、`BoldFaceRequired`、`UnknownFont` 写入同一张表并继续扫描。缺字保留在簇序列里，前进宽度为 0，绘制时跳过，不用 `.notdef` 占位。同一个 `UnknownFont` id 只记录一次，重复项不占 64 条名额，也不因此把表标成截断。
- 几何、分页停滞、版心非法、画布生命周期、backend 序列化仍是单错并停止后续测量。表已满时，这条硬错误替换最后保留的一项，并保持 `truncated`，使停止原因仍可见。`to_pdf` 把 backend 的单错升成 `Errors`。
- 有任一错就不产出 `Rendered` 或 PDF。
- 几何、画布、`Document::font`、`bold_font`、`load_image` 仍返回单个 `Error`。

---

## 依赖错误约定（已确认）

本库依赖 `skrifa`、`krilla`（及 krilla 拉进来的图像解码等）。公共 API **不重导出**这些 crate 的类型（与「krilla 关在适配器里」一致）。错误面同样遵守：**调用方 match 的是 quire 的 `ErrorKind`，不是依赖的错误类型。**

### 原则

1. **翻译，不穿透。**  
   依赖失败 → 收成 quire 的 kind（如 `Font` / `Image` / `Backend`），带上本库语境字段（font index、image 限额原因等）。**不要**在公共 `ErrorKind` 里写 `Skrifa(skrifa::…)` / `Krilla(krilla::…)`，否则：
   - 公共面泄漏依赖类型；
   - 钉版本一变，调用方的 `match` 跟着炸；
   - 与 AGENTS「公共 API 不重导出 krilla」冲突。

2. **保留链：`source()` 指向底层。**  
   当底层值实现了 `std::error::Error + Send + Sync + 'static` 时，装箱保存，并在 `impl std::error::Error for Error` 里从对应 kind 返回 `Some(source)`。  
   这样 `anyhow` / `eyre` / `{:#}` 能看到因果链，同时对外稳定类型仍是 quire。

3. **底层不是 `Error` 时，诚实降级。**  
   有的 API 只给 `&str` / `String` / 自定义非 Error 类型（例如部分图像路径今天是 `map_err(|message| …)`）。此时：
   - 把可读说明放进本库字段（或 Display）；
   - `source()` 为 `None`；
   - **不要**再包一层假的 `std::io::Error::new(Other, msg)` 只为「看起来有 source」。

4. **本库自己的校验 vs 依赖失败，分开。**  
   - 空字节、超 `MAX_FONT_BYTES`、超像素限额 → **本库 kind + 本库 message/字段**，无 source。  
   - `FontRef::from_index` / `document.finish()` / 解码器失败 → **本库 kind + source（若可得）**。  
   不要把限额错误伪装成「解析器说的话」。

5. **Display 以本库语境为主。**  
   第一行应让批跑调用方看懂 quire 语义，例如 `font index 0: …` / `PDF backend failed: …`。底层的 `Display` 留给 `source()` 链，或在本库 Display 里简短附带，避免只回传一句依赖内部英文、丢失 index/限额等语境。

6. **feature / 可选依赖。**  
   `backend-krilla` 关闭时，不得要求公共 `ErrorKind` 里出现仅 krilla 才有的类型。Backend 相关 kind 可以始终存在（无 backend 时无构造点），或与 feature 对齐；实现时选不泄漏类型的那一种。装箱用 `Box<dyn Error + Send + Sync>` 时，公共面仍不出现 `krilla::`。

### 示意（非最终签名）

```rust
pub enum ErrorKind {
    Font {
        index: u32,
        /// 本库语境说明；可与 source 的 Display 互补。
        message: String,
        // 实现时：Option<Box<dyn Error + Send + Sync>> 放在 Error 旁路
        // 或私有字段，使 PartialEq 仍可测（比 source 或忽略 source）
    },
    Image { message: String /* + 可选 source */ },
    Backend { message: String /* + 可选 source */ },
    // …
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // 按 kind 取出已保存的依赖错误
    }
}
```

`PartialEq`：对带 dyn source 的 kind，测试比「kind + 稳定字段」、忽略或单独比 source 的 `Display`——刀 3 实现时定，避免为 DynError 牺牲可测性。

### 明确不做

- 公共 API `pub use` 依赖的错误类型。
- 仅 `to_string()` 后丢掉底层 Error（现状；刀 3 要改掉）。
- 为凑 `source()` 而引入 `anyhow::Error` 进公共面。

---

## 与评估原文的对照

刀 1–4 的处理方式以上文为准，不再采用「Display 补丁」或「无限期字符串化」。失败时交出 `Errors`，不再只留首错。
