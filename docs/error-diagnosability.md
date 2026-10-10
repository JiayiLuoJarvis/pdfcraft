# 错误可诊断性：已确认方向

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-10 |
| 状态 | **方向已确认。** 未改代码；按刀序实现。 |
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
3. **§2**：本阶段承诺填 `Location`（节、表行列、页码、header/footer 等 layout/组装可知信息）。**不做**调用方源码行号。**多错收集另刀**（刀 4）。
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
| 4 | （可选）多错收集 API |

`Location` / `ErrorKind` 的精确字段名与变体表在刀 1 实现 PR 定；勿提前冻进提案书。

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

评估 §1–§4 的现象描述仍成立。处理方式以上表为准，不再采用「Display 补丁」或「无限期字符串化」。
