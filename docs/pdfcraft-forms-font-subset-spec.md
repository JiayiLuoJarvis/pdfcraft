# pdfcraft-forms 字体子集 Spec

| 项 | 值 |
|---|---|
| 日期 | 2026-10-17 |
| 状态 | Ready for implementation |
| 范围 | `crates/pdfcraft-forms` 成功路径嵌入的 TrueType 面；本文件；实现时同步改动的规格句子与测试 |
| 非范围 | OTF/CFF、`pdfcraft-pdf` / krilla 的子集、`pdfcraft-post` 加密、`loco-rs` 业务模板与字体缓存 |
| 调用方 | `loco-rs` 的 `/v1/internal/ird-tax-refund-guarantee` 只传字体字节，不持有嵌入逻辑 |

相关权威文档：

- [`提案书.md`](提案书.md) §10：子集不在该条已确认的依赖里；「单独的子集 PR 之前，实现可以把整份 TrueType 嵌进输出」。本 spec 就是那个 PR 的合同。
- [`pdfcraft-forms-spec.md`](pdfcraft-forms-spec.md) §4.3、§6：缺字整份失败；v1 过渡期允许整份嵌入。本 spec 结束后，成功路径改为嵌入子集。
- [`pdfcraft-forms-repair.md`](pdfcraft-forms-repair.md) R4：`FontFile` / `PreparedFont` / `embed` 的对象顺序、`/W`、ToUnicode、`with_compression(false)`、两次填充字节相同。那些句子里「不子集」被本 spec 替换；对象顺序与字节稳定性保留。
- [`../AGENTS.md`](../AGENTS.md)：OTF 与字体子集仍未做。实现合并后改这一句，不要在本 spec 落地时顺手改 `AGENTS.md`。

本文件只定义要做什么。落地 PR 才改 Rust。

---

## 0. 成功标准

完成时必须同时满足：

1. `cargo fmt -p pdfcraft-forms && cargo clippy -p pdfcraft-forms --all-targets -- -D warnings && cargo test -p pdfcraft-forms`
2. 公共函数签名不变：`list_fields`、`fill_and_flatten`、`FillOptions`、`FontBytes`、`ErrorKind`。调用方不用改。
3. 有文本需要嵌入时，`FontFile2` 的流是**用到的 glyph 的 glyf 子集**，不是源字体整份拷贝。
4. 内容流里的 glyph id、`CIDFontType2` 的 `/W`、`ToUnicode` 的 bfchar 键，仍是测量阶段 `PreparedFont::record` 记下的**原 glyph id**。`/CIDToGIDMap /Identity` 保持不变。
5. 同一输入两次 `fill_and_flatten` 字节相同。
6. 缺字仍是 `MissingGlyph`，不写出缺字的 PDF。只勾选、不画字时仍不嵌入字体。
7. 在设置了 `PDFCRAFT_FORMS_FONT` 且该文件是 `SourceHanSerifHK-Regular.ttf`（约 9.1MB）时，填写 `tests/fixtures/chinese_form.pdf` 的输出 PDF **小于 512 KiB**。字体流本身小于 256 KiB。未设置该环境变量时，这条测试跳过，CI 不因此失败。

非目标（本轮不做）：

- 打开 `FontFile2` 的 flate（`with_compression(false)` 保持）。体积靠子集，不靠压缩实现。
- OTF、CFF、`CFF2`、可变字体、`sbix` / `CBDT` 位图。
- 在 `pdfcraft-forms` 里做 OpenType 整形（GSUB/GPOS）。测量仍走现有 cmap → glyph id。
- 改 `pdfcraft-post` 的 AES，或改 `loco-rs` 的 `load_source_han_serif_hk_regular` 缓存。
- 把 9.1MB 思源字体提交进 quire 仓库。
- 重编号 glyph id 来缩短 `loca`。

---

## 1. 问题

### 1.1 嵌入点

`PreparedFont::embed`（`crates/pdfcraft-forms/src/font.rs`）把测量用的整份 `sfnt` 放进不压缩的 `FontFile2`：

```rust
let file = Stream::new(Dictionary::new(), self.bytes.to_vec()).with_compression(false);
```

`used: BTreeMap<u16, (char, f32)>` 只供给 `/W` 和 ToUnicode。glyf 表不读这张表。

`FontFile::from_source` 在知道用了哪些字之前就复制整份面，并对整份字节做 `head` 时间戳清零和校验和。TTC 会先拆出单个面，这步要保留。

### 1.2 调用链上的体积

`loco-rs` 的偿还税款保证书在 `fill_form` 里把 `SourceHanSerifHK-Regular.ttf` 交给 `fill_and_flatten`。2026-10-17 在本机拆阶段测得（页缓存已热）：

| 阶段 | debug | release | 产物 |
|---|---:|---:|---|
| 读 HK Regular | 7 ms | 5 ms | 9,467,292 字节 |
| 排信（`pdfcraft-pdf`，已子集） | 60 ms | 8 ms | 75,533 字节 |
| `fill_and_flatten` | 106 ms | 27 ms | 9,641,406 字节 |
| `merge_pdfs` | 53 ms | 40 ms | 9,711,398 字节 |
| 只写元数据 | 25 ms | 23 ms | 约 9.7 MB |
| AES-256（`StampAndEncrypt`） | 930–980 ms | 38–47 ms | 约 9.7 MB |

空白表约 170 KB。多出来的约 9.5 MB 是未压缩的整份字体。debug 下接口约 1.6 s，其中加密在扫这 9.7 MB。release 下加密约 40 ms，但响应体仍然是 9.7 MB。

信那一页走 krilla，已经子集，所以同样的思源字体在别的导出里只有几十 KB。慢的是 forms 这条嵌入，不是加密算法本身。

### 1.3 已有合同里要守住的句子

- 内容流：一行一个 `Tj`，glyph id 按 `{gid:04X}` 连成十六进制串（repair R4）。
- `/W` 按 `BTreeMap` 的 gid 序。ToUnicode 用 `push_str` / `format!`。
- `embed` 的对象顺序：FontFile2、FontDescriptor、CIDFontType2、ToUnicode、Type0。
- `record` 用 `or_insert`，第一次写入的 `(char, units)` 获胜。
- `used` 为空则 `paint` 不调用 `embed`。
- 两次成功调用字节相同：子集结果里的 `head` created/modified 必须是 0，`checkSumAdjustment` 按子集字节重算。

---

## 2. 方案

在 `embed` 写流之前，用 `used` 的键从当前 `sfnt` 裁出新的 TrueType，再对**这份新字节**做与今天 `zero_head_times` 相同的 `head` 处理，然后把新字节放进 `FontFile2`。

测量仍读完整的面（cmap 与 advance 都在原面上）。子集只出现在输出 PDF 里。

### 2.1 为什么保留原 glyph id

内容流、`/W`、ToUnicode、`/CIDToGIDMap /Identity` 已经用原 id。重编号要同时改这三处和 `content.rs`，basic 夹具的运算符也会被迫重写。

保留 id 时，`loca` 仍覆盖 `0 ..= max_gid`。没用到的槽长度是 0（相邻 `loca` 项相等）。`maxp.numGlyphs` 保持源字体的值。Source Han 这种面，空 `loca` 大约是几十到几百 KB，glyf 才是 9 MB。空槽可以接受。

### 2.2 必须带上的 glyph

集合从 `used` 的键开始，再闭合：

1. **glyph 0**（`.notdef`）始终加入，即使这次一个字都没画到它。
2. 每个简单字形：原样复制 glyf 数据（含轮廓与指令）。
3. 每个复合字形（`numberOfContours < 0`）：原样复制该 glyph 的字节，并把它引用的组件 glyph id 递归加入集合。组件 id 不必写进 `/W` 或 ToUnicode，内容流不直接点它们。

复合字形的组件 id 按 OpenType glyf 复合描述读取。从 glyph 起点 + 10 字节开始循环，直到 flags 带 `MORE_COMPONENTS`（bit 5）的项结束：

| flag | 值 | 组件 id 之后要跳过的参数 |
|---|---:|---|
| `ARG_1_AND_2_ARE_WORDS` | bit 0 | 两个 int16；否则两个 int8 |
| `WE_HAVE_A_SCALE` | bit 3 | 一个 F2DOT14（int16） |
| `WE_HAVE_AN_X_AND_Y_SCALE` | bit 6 | 两个 F2DOT14 |
| `WE_HAVE_A_TWO_BY_TWO` | bit 7 | 四个 F2DOT14 |

组件 glyph index 是参数前面的 `uint16`。标志位组合互斥时按上表只取一种缩放。解析越界 → `FontNotConfigured { reason: Head }`（字体字节不可用，不是缺字）。缺字已经在 `LaidText::fit` 失败，到不了子集。

### 2.3 表怎么处理

源 `sfnt` 的表目录按 tag 读出。写出的目录按 tag 字节序排序，表体 4 字节对齐。这是 sfnt 要求，也保证字节稳定。

| 表 | 处理 |
|---|---|
| `glyf` | 重写。按 glyph id 从 0 到 `numGlyphs-1` 排。在集合内的复制原字节；不在集合内的长度为 0。每个非空 glyph 在 glyf 内的起点保持 2 字节对齐（短 `loca` 的偏移要能除以 2）。尾部补 0 到 4 字节对齐后再挂到 sfnt。 |
| `loca` | 重写。偏移个数是 `numGlyphs + 1`。`head.indexToLocFormat` 沿用源字体：0 为 `uint16`（偏移/2），1 为 `uint32`。不要为了省 `loca` 把 long 改成 short。子集后若某个偏移放不进 short 格式，保持 long，并写回 `head` 的 `indexToLocFormat`。源字体已经是 long 时，输出也是 long。 |
| `head` | 复制，然后清零 created/modified（偏移 20..36）和 `checkSumAdjustment`（偏移 8..12），等全部表就位后再算校验和。`indexToLocFormat` 按上一行。其余字段不动，包括 `unitsPerEm`、`magicNumber`。 |
| `maxp` | 原样复制。`numGlyphs` 不变。`maxPoints` 等上限可以大于子集实际值。 |
| `hhea` | 原样复制。 |
| `hmtx` | 原样复制。PDF 宽度走 `/W`，但查看器仍要能按原 gid 读到 `hmtx`。整表远小于 glyf。 |
| `cmap` | **不复制原表。** 写一张只含 `used` 里字符的 cmap：platform 3、encoding 10（UTF-32）的 format 12，加上 platform 3、encoding 1（BMP）的 format 4，仅当全部标量都落在 BMP 时才写 format 4。映射是标量 → **原 glyph id**。`record` 的 `BTreeMap` 序就是 gid 序；format 12 的 group 按标量升序。同一 glyph 对应多个标量时，每个标量各一条。 |
| `name` `OS/2` `post` `cvt ` `fpgm` `prep` `gasp` | 原样复制（存在才复制）。`post` 不改成 format 3。 |
| `GSUB` `GPOS` `GDEF` `BASE` `JSTF` `mort` `morx` `kern` `meta` `SVG ` `COLR` `CPAL` `sbix` `CFF ` `CFF2` | 不写入子集。填写器已经选定 glyph id，嵌入字体不再整形。 |

`glyf` 不存在 → 现有 `FontReason::NoGlyf`，在 `Face::load` 就已经失败，子集函数可以假定有 `glyf`。`loca` 与 `head` 对不上 → `FontReason::Head`。

不引入 `fonttools`、`klippa`、`subsetter`、`allsorts` 或第二份 `skrifa`。子集器放在 `pdfcraft-forms` 内，建议 `src/subset.rs`，由 `PreparedFont::embed` 调用。`lopdf` 继续 `default-features = false`，不要开 `font_embedding`。

### 2.4 `head` 校验和放在哪

`from_source` 今天对整份面做 `zero_head_times`，可以留着，让测量用的 `FontFile` 与现在一样。子集是**另一块**字节：glyf/loca/cmap 都变了，必须对子集再跑一遍同一套清零与 `0xB1B0AFBA` 调整。这不是对同一块缓冲打两次戳。

repair R4 里「不要在 `embed` 时再打一次时间戳」指的是当时嵌入的就是 `FontFile` 那一份字节。本 spec 之后，嵌入的是子集；戳打在子集上。

### 2.5 字节稳定性

- 表顺序按 tag 排序，不按源文件出现顺序。
- `used` 的遍历用现有 `BTreeMap`。
- 不读时钟，不写非零的 created/modified。
- 复合字形的字节原样复制，不重算轮廓。
- `with_compression(false)` 不动。flate 实现一变，夹具就会漂。

basic 夹具的**整份 PDF 字节会变**，因为 `FontFile2` 从「测试字体全文」变成「Ada / Senior 用到的 glyph」。测试若有全文件黄金字节，改黄金值或改成结构断言。语义断言保留：`%PDF` 开头、无 `/AcroForm`、抽得到填入的字、两次调用相等、空 map 不嵌字体。

---

## 3. 错误

不新增 `ErrorKind`。子集只在测量和排版都成功之后运行。

| 情况 | 错误 |
|---|---|
| cmap 没有该字符 | 现有 `MissingGlyph`（`LaidText::fit`，子集之前） |
| 无 `glyf` | 现有 `FontReason::NoGlyf` |
| 表目录、`loca`、`head`、复合描述越界，或偏移溢出 | `FontNotConfigured { reason: Head }` |
| `used` 为空 | 不调用子集，也不调用 `embed` |

不要把子集失败报成 `FlattenFailed`。那是 PDF 写出失败。字体字节自身的问题仍是 `FontNotConfigured`。

---

## 4. 测试

放在 `crates/pdfcraft-forms`，不把思源字体提交进仓库。

### 4.1 不依赖外部字体

用测试里造的最小 TrueType，或从现有小夹具字体裁一块进 `tests/fixtures/`（只含若干 glyph 的小文件，几 KB，可以进仓）：

- 只用到其中一个字时，输出 `FontFile2` 的 glyf 里该字与 glyph 0 非空，另一个已知 glyph 的 `loca` 跨度为 0。
- 复合字形会把组件 glyph 带进 glyf，即使组件 id 不在 `used` 里。
- 子集 `sfnt` 能被 `Face::load` 打开，`glyph` 对用到的标量返回**同一个 id**。
- 两次子集字节相同；`head` 的 created/modified 为 0。
- 只勾选不填字的 `fill_and_flatten` 输出里没有 `FontFile2`。

### 4.2 依赖 `PDFCRAFT_FORMS_FONT`

沿用 `tests/fill_and_flatten.rs` 的 `font()`：变量未设置就跳过。

- `two_fills_match_byte_for_byte` 继续通过。
- `chinese_form` 在字体是 HK Regular（文件名或魔数加大小：大于 1 MB 且能画出夹具里的中文）时，输出长度 `< 512 * 1024`，且能抽到填入的中文。抽字沿用现有测试手段（内容流或 `pdftotext`，与该测试今天的做法一致）。
- 缺字用例仍是 `MissingGlyph`，不因子集而变成写出半份 PDF。

### 4.3 明确不测

- debug/release 的毫秒数不进 CI。
- `loco-rs` 的 HTTP 与合并后加密不在本 PR。子集落地后，那条接口的表单页应从约 9.6 MB 降到空白表加几十 KB 字体；加密耗时会跟着掉。那是调用方的回归，本 PR 不改 `loco-rs`。

---

## 5. 实现时要改的文档

与代码同一个 PR，只改这些句子，不要重写整篇：

| 文件 | 改什么 |
|---|---|
| `docs/pdfcraft-forms-spec.md` 抬头、§4.3、§6 字体子集行、§8 | 「仍未做 / 整份嵌入」改为「成功路径嵌入 glyf 子集，保留原 glyph id」。OTF 仍未做。 |
| `docs/提案书.md` §10 依赖段与文末总表里「字体子集仍未做」 | 改成已做，并指向本 spec。不要改 §10 的产品范围（仍然不填 OTF、仍然缺字失败）。 |
| `AGENTS.md` 里「OTF 与字体子集仍未做」 | 改成子集已做、OTF 未做。 |
| `crates/pdfcraft-forms/README.md`「How it works」 | 补一句：嵌入的是用到的 glyph，不是整份字体。 |
| `docs/pdfcraft-forms-repair.md` | 不回改历史修复记录。R4 的「不子集」是当时的修复边界。本 spec 在文首声明它替换那一条即可。 |

---

## 6. 执行顺序

一个 PR，分支名建议 `opt/forms-font-subset`。

1. `subset.rs`：给定 `sfnt` 字节和 glyph id 集合，返回新 `sfnt`。先做 §4.1 的单元测试，此时尚未接到 `embed`。
2. `PreparedFont::embed` 改为嵌入子集，并对子集调用现有的 `head` 清零与校验和。对象顺序、`/W`、ToUnicode、内容流不动。
3. 跑 `cargo test -p pdfcraft-forms`。basic 夹具若锁了全文件字节，只更新这批期望。
4. 按 §5 改文档句子。
5. 有 `PDFCRAFT_FORMS_FONT` 指向 HK Regular 时，跑中文夹具，确认输出 `< 512 KiB`。

不要在这个 PR 里改 `loco-rs`，不要开 `FontFile2` 压缩，不要把 glyph id 重排成从 0 连续编号。
