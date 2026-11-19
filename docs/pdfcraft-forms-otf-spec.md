# pdfcraft-forms OTF / CFF1 嵌入规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-19 |
| 状态 | **已定稿。** 产品钉板经 2026-10-19 讨论确认；可执行细则以本文为准。与提案书冲突时以提案书为准 |
| 范围 | `crates/pdfcraft-forms`：接受并子集嵌入 **CFF1** OpenType 面；本文件；实现时同步改动的规格句子与测试 |
| 非范围 | 重做 TrueType `glyf` 子集（已有合同 [`pdfcraft-forms-font-subset-spec.md`](pdfcraft-forms-font-subset-spec.md)）；`CFF2`；可变字体；Type 1；位图色字；`pdfcraft-pdf` / krilla 子集；`pdfcraft-post`；loco-rs / hcsw |
| 调用方 | 继续传 [`FontBytes`]；签名不变。`.otf`（CFF1）与集合内 CFF 面在本主题后可用 |

相关权威文档：

- [`提案书.md`](提案书.md) §10：成功路径已有 TrueType `glyf` 子集；**OTF 仍未做**——本 spec 就是补上 OTF/CFF1 的合同。
- [`pdfcraft-forms-spec.md`](pdfcraft-forms-spec.md) §4.3、§6：当前句子要求 TrueType（含 `glyf`）；本 spec 落地后改为「`glyf` 或 CFF1」。
- [`pdfcraft-forms-font-subset-spec.md`](pdfcraft-forms-font-subset-spec.md)：`glyf` 子集的权威；其「非目标」里的 OTF/CFF 由本文接管，**不回改**该文件的历史成功标准。
- [`../AGENTS.md`](../AGENTS.md)：OTF 仍未做。实现合并后改这一句。

本文只定义要做什么。落地 PR 才改 Rust。

---

## 0. 成功标准

完成时必须同时满足：

1. `cargo fmt -p pdfcraft-forms && cargo clippy -p pdfcraft-forms --all-targets -- -D warnings && cargo test -p pdfcraft-forms`
2. 公共函数签名不变：`list_fields`、`fill_and_flatten`、`FillOptions`、`FontBytes`。调用方不用为 OTF 改调用方式。
3. `FontReason` **新增** `NoOutline`（见 §3）。既无 `glyf` 也无 `CFF ` 时用它。`NoGlyf` 保留。
4. 纯 CFF1 面（无 `glyf`、有 `CFF `）可以成功填写文本：输出 PDF 可抽到填入文字；嵌入走 **`FontFile3` + `CIDFontType0`**（见 §4）。
5. CFF 成功路径嵌入的是**用到的 glyph/CID 的 CFF 子集**，不是源 `CFF ` 表整份拷贝；内容流、`/W`、ToUnicode 的键仍是测量阶段记下的**原 glyph id（作 CID）**。
6. 同一输入两次 `fill_and_flatten` 字节相同（CFF 路径与 glyf 路径都要）。
7. 缺字仍是 `MissingGlyph`，不写出缺字的 PDF。只勾选、不画字时仍不嵌入字体。
8. 现有 glyf 子集行为与 [`pdfcraft-forms-font-subset-spec.md`](pdfcraft-forms-font-subset-spec.md) 的验收**不回退**（含 `PDFCRAFT_FORMS_FONT` 指向 HK Regular `.ttf` 时的体积上限）。

非目标（本轮不做）：

- `CFF2`、可变字体（含带 `fvar` 且只能走 CFF 路径的面）、纯 Type 1、`sbix` / `CBDT` / `COLR` 位图或色字。
- 打开 `FontFile3` / `FontFile2` 的 flate（`with_compression(false)` 保持）。
- 在 `pdfcraft-forms` 里做 OpenType 整形（GSUB/GPOS）。测量仍走现有 cmap → glyph id。
- 重编号 CID/glyph id。
- 把思源或其他大字库 `.otf` 提交进 quire 仓库。
- 改 `pdfcraft-pdf`、`pdfcraft-post`、loco-rs、hcsw。
- 重开或改写 `glyf` 子集算法合同。

---

## 1. 问题

### 1.1 现状

`Face::load` 在没有 `glyf` 时直接 `FontReason::NoGlyf`。因此常见 CFF1 `.otf`（以及集合里的 CFF 面）无法用于 `fill_and_flatten`，即使 `skrifa` 能量宽、cmap 也齐全。

TrueType 成功路径已经：

- 测量：cmap → 原 glyph id → advance
- 嵌入：`subset_sfnt` → `FontFile2` + `CIDFontType2` + `/CIDToGIDMap /Identity`
- 内容流：一行一个 `Tj`，十六进制 CID == 原 glyph id

### 1.2 本主题要补的缺口

在**不改调用方 `FontBytes` 形状**的前提下：

1. 接受 CFF1 面；
2. 按用到的 id 子集 CFF；
3. 用 PDF 的 CIDFontType0 / FontFile3 路径写出；
4. 与 glyf 路径共用测量、排版、缺字、字节稳定约定。

---

## 2. 产品一句话

调用方把 CFF1 OpenType（或集合里按 `index` 拆出的 CFF1 面）放进 `FillOptions::font`；库按现有规则量字、折行、缺字失败；成功时嵌入**子集后的 CFF**，内容流仍用原 glyph id 当 CID。

---

## 3. 公共 API 与错误面

### 3.1 不变的签名

```rust
pub struct FontBytes<'a> {
    pub bytes: &'a [u8],
    pub index: u32,
}

pub fn fill_and_flatten(
    pdf: &[u8],
    fields: &HashMap<String, FieldValue>,
    options: &FillOptions<'_>,
) -> Result<Vec<u8>>;
```

不新增「字体格式」枚举，不要求调用方声明 `.ttf` / `.otf`。

### 3.2 面选择（加载期）

对 `FontBytes` 解析出的单个 sfnt 面（`ttcf` / OTC 仍先按 `index` 拆成单面，与今日 TTC 行为一致）：

| 条件 | 路径 |
| --- | --- |
| 存在 `glyf` | **TrueType 路径**（现有 `FontFile2` / `CIDFontType2` / `subset_sfnt`） |
| 无 `glyf`，存在表标签 `CFF `（四字节，末字节为空格） | **CFF1 路径**（本文） |
| 既无 `glyf` 也无 `CFF ` | `FontNotConfigured { reason: NoOutline }` |

同一面同时有 `glyf` 与 `CFF ` 时：**优先 glyf**（上表第一行）。不提供调用方开关。

### 3.3 CFF 路径上的拒绝

在已选定 CFF1 路径后：

| 情况 | 错误 |
| --- | --- |
| 存在 `fvar`（可变字体） | `FontNotConfigured { reason: Head }` |
| `CFF ` 表损坏、子集无法完成、偏移越界等 | `FontNotConfigured { reason: Head }` |
| cmap 缺字 | `MissingGlyph`（子集之前，与今日相同） |
| 需画字但未提供 font | `FontNotConfigured { reason: Missing }` |

仅有 `CFF2`、没有 `glyf`、没有 `CFF ` → `NoOutline`（本轮不实现 CFF2）。

### 3.4 `FontReason::NoOutline`

```rust
pub enum FontReason {
    Missing,
    Collection,
    NoGlyf,
    /// Neither `glyf` nor CFF1 (`CFF `) is available for embedding.
    NoOutline,
    UpemZero,
    NoAdvance,
    Head,
    Width,
    Unreadable,
}
```

- **`NoOutline`**：加载期「没有可用轮廓表」。
- **`NoGlyf`**：保留。用于已判定走 glyf 路径后发现表不可用、或 `subset_sfnt` 假定有 `glyf` 而失败等一致性情况。纯 CFF1 面**不得**再报 `NoGlyf`。
- Display 建议：`font has no usable outline table (glyf or CFF)`。
- 这是 0.x 可接受的枚举扩展；穷尽匹配 `FontReason` 的调用方需要补一支。

不把子集失败报成 `FlattenFailed`。

---

## 4. PDF 嵌入形状（CFF1 路径）

与 glyf 路径平行，对象顺序固定以保证字节稳定：

1. `FontFile3` 流（`with_compression(false)`）
2. `FontDescriptor`（`FontFile3` 引用上述流）
3. `CIDFontType0` 字典
4. `ToUnicode` 流
5. Type0 字体字典（`Encoding = Identity-H`，`DescendantFonts` 指向 CIDFontType0）

### 4.1 `FontFile3`

- 流内容是**子集后的 CFF 字体程序**（CFF 数据本身），不是整份 sfnt 包装。
- 流字典：`Subtype` = `CIDFontType0C`。
- 不打开 flate。

### 4.2 `FontDescriptor`

- 与今日字段同骨架（`FontName`、`Flags`、`FontBBox`、`Ascent`、`Descent`、`CapHeight`、`StemV` 等）。
- 用 **`FontFile3`** 键引用字体流（不用 `FontFile2`）。

### 4.3 `CIDFontType0`

- `Subtype` = `CIDFontType0`。
- `CIDSystemInfo`：Registry `Adobe`，Ordering `Identity`，Supplement `0`（与今日一致）。
- `/W`：按用到的 CID（原 glyph id）升序，写法与今日 `CIDFontType2` 的 `/W` 相同（CID + 单宽数组）。
- **不写** `/CIDToGIDMap`（该键属于 Type2）。
- `DW` = 1000。

### 4.4 内容流与 ToUnicode

- 测量、`PreparedFont::record`、`LaidText`、内容流 `Tj` 十六进制串：**与 glyf 路径同一套原 id**。
- ToUnicode bfchar 键仍是这些 id；生成方式（`push_str` / `format!`、按 100 条分块）保持与今日一致。

### 4.5 集合字体

- `bytes` 以 `ttcf` 开头时：按 `FontBytes.index` 拆出单面 sfnt，再走 §3.2。
- 拆出的面是 CFF1 → CFF 路径；是 glyf → glyf 路径。
- index 越界或目录损坏 → 现有 `Collection` / `Head` 行为，不新造错误。

---

## 5. CFF 子集算法约束

测量仍读**完整**面（skrifa cmap / advance）。子集只出现在输出的 `FontFile3` 里。

### 5.1 必须保留的 glyph

集合从 `used` 的键（`PreparedFont::record`）开始，再闭合：

1. **glyph 0**（`.notdef`）始终加入。
2. 每个用到的 glyph 的 CharString 原样逻辑保留（可重打包，但绘制结果与源字体该 GID 一致）。
3. CharString 引用的 **local / global subr** 递归加入；未引用的 subr 不写入子集。
4. CID-keyed 面：保留用到的 FD 与 `FDSelect` 所需项；丢掉未用到的 FontDict。
5. name-keyed 面：嵌入前仍以 glyph id 作为 Identity-H 的 CID（与测量 id 一致）；子集后的 CFF 必须让这些 CID 点到对应 CharString。

### 5.2 表与包装

- 只子集 `CFF ` 表内容并写入 `FontFile3`；**不要**为了 CFF 路径再输出一整份 OTTO sfnt 当 FontFile3（本主题选定 `CIDFontType0C`）。
- 不把 `GSUB` / `GPOS` / `GDEF` / `BASE` / `JSTF` / `SVG ` / `COLR` / `CPAL` / `CFF2` 写进嵌入流。
- 不引入 `fonttools`、`klippa`、`subsetter`、`allsorts`，不打开 `lopdf` 的 `font_embedding`，不拉第二份 `skrifa`。子集器放在 `pdfcraft-forms` 内（例如 `src/cff_subset.rs` 或与现有 `subset` 模块并列的清晰边界）。

### 5.3 字节稳定性

- 同一 `used` 集合 → 同一 `FontFile3` 字节。
- 不读时钟；CFF 内若存在可清零的时间戳类字段则清零；输出全确定性排序（FD、charset、subr 索引重排规则在实现里写死并单测锁住）。
- `used` 遍历继续用现有 `BTreeMap` 序。

### 5.4 与 glyf 子集的关系

| | glyf 路径 | CFF1 路径 |
| --- | --- | --- |
| 合同 | `pdfcraft-forms-font-subset-spec.md` | **本文** |
| 嵌入 | `FontFile2` + `CIDFontType2` + `CIDToGIDMap Identity` | `FontFile3` + `CIDFontType0` + `Subtype CIDFontType0C` |
| 子集输入 | 整份 sfnt 的 glyf/loca/… | `CFF ` 表 |
| 原 id | 保留 | 保留 |

两条路径共享：`Face` 量宽、`LaidText`、`record`、缺字、两次填充相等、对象顺序纪律、`with_compression(false)`。

---

## 6. 错误表（汇总）

| 情况 | 错误 |
| --- | --- |
| 需字体未提供 | `FontNotConfigured { Missing }` |
| 集合拆面失败 | `Collection` 或 `Head`（与今日一致） |
| 无 `glyf` 且无 `CFF ` | `NoOutline` |
| CFF 路径但存在 `fvar` | `Head` |
| `CFF ` 损坏 / 子集失败 | `Head` |
| 已走 glyf 但 glyf/loca 不可用 | `NoGlyf` 或 `Head`（与今日子集合同一致） |
| cmap 缺字 | `MissingGlyph` |
| `used` 为空 | 不调用嵌入 |

---

## 7. 测试与验收

放在 `crates/pdfcraft-forms`。大字库不进仓。

### 7.1 不依赖外部字体

- 最小 CFF1 夹具（几 KB，可进 `tests/fixtures/`）：只填一个字符时，`FontFile3` 流小于源 `CFF `；PDF 含 `CIDFontType0` 与 `FontFile3`；`pdftotext` 或内容流能见到该字。
- 纯 CFF1 面不再返回 `NoGlyf`；既无 glyf 也无 CFF 的字节返回 `NoOutline`。
- 两次 `fill_and_flatten` 字节相同。
- 缺字仍 `MissingGlyph`。
- 只勾选不填字：无 `FontFile2`、无 `FontFile3`。

### 7.2 双路径回归

- 现有 glyf 单测与集成测全部保持绿。
- 可选：造一份「同时有 glyf 与 CFF」的最小面，断言走 `FontFile2`（优先 glyf）。

### 7.3 依赖 `PDFCRAFT_FORMS_FONT`

- 变量指向 **`.ttf` / glyf** 时：现有中文体积上限（输出 `< 512 KiB`、字体流 `< 256 KiB`）继续有效。
- 变量指向 **CFF1 `.otf`** 且能画出 `chinese_form` 中文时：输出也能抽到中文；建议另加体积上限（实现时按该字体实测钉一个宽松上限，写入测试；未设置变量则跳过）。不因缺少 `.otf` 使 CI 失败。

### 7.4 明确不测

- debug/release 毫秒数。
- loco-rs HTTP 与合并加密链路（只读对照允许；本 PR 不改 loco-rs）。
- CFF2 / 可变字体的「半支持」。

---

## 8. 实现时要改的文档

与代码同一个 PR，只改这些句子，不要重写整篇：

| 文件 | 改什么 |
| --- | --- |
| `docs/pdfcraft-forms-spec.md` 抬头、§4.3、§6、§8 | 「须 TrueType / glyf」「OTF 仍未做」改为：成功路径支持 glyf 子集或 CFF1 子集；指向本文与 glyf 子集合同 |
| `docs/提案书.md` §10 依赖段与确认总表 | 「OTF 仍未做」改为已做（CFF1），并指向本文；缺字失败等其余产品句不动 |
| `AGENTS.md` | 「OTF 仍未做」改为 CFF1 已做；仍写明不做 CFF2 / 可变字体 |
| `crates/pdfcraft-forms/README.md` | Fonts 一节：接受 glyf TrueType 与 CFF1 OpenType；集合 `index`；嵌入为子集 |
| `docs/pdfcraft-forms-font-subset-spec.md` | **不改**成功标准正文；可选在文首「非范围」加一句「OTF/CFF1 见 `pdfcraft-forms-otf-spec.md`」 |
| `CHANGELOG.md` | 记 `FontReason::NoOutline` 与 OTF/CFF1 支持 |

---

## 9. 执行顺序

一个 PR，分支名建议 `feat/forms-otf-cff1`。

1. 加载期分支：`Face::load` 按 §3.2 选择路径；补 `NoOutline`；纯 CFF 不再 `NoGlyf`。
2. CFF 子集器 + 单元测试（§5、§7.1），尚未接到 `embed`。
3. `PreparedFont::embed`：CFF 路径写 §4 对象图；glyf 路径调用保持不变。
4. `cargo test -p pdfcraft-forms`；更新仅因错误文案 / 新变体受影响的断言。
5. 按 §8 改文档句子。
6. 有 CFF1 中文字体时跑 §7.3。

不要在这个 PR 里改 loco-rs / hcsw，不要做 CFF2，不要重编号 CID，不要回退 glyf 子集。

---

## 10. 验收清单

- [ ] 纯 CFF1 `.otf` 可 `fill_and_flatten`，抽字正确
- [ ] 输出为 `FontFile3` + `CIDFontType0`，内容流 CID == 测量 glyph id
- [ ] CFF 为子集；两次填充字节相同
- [ ] 无轮廓 → `NoOutline`；纯 CFF 不报 `NoGlyf`
- [ ] `fvar` 的 CFF 面 → `Head`；缺字 → `MissingGlyph`
- [ ] 集合 `index` 拆 CFF 面可用；双表时优先 glyf
- [ ] glyf 子集合同验收不回退
- [ ] §8 文档句子已同步；未改 loco-rs / hcsw
- [ ] 门禁：`cargo fmt` / `clippy -D warnings` / `cargo test -p pdfcraft-forms` / `cargo doc -p pdfcraft-forms --no-deps`
