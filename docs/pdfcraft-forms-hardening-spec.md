# pdfcraft-forms 加固 Spec

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-19 |
| 状态 | Ready for implementation |
| 范围 | `crates/pdfcraft-forms` 的 `src/`、`tests/`、`examples/`、`CHANGELOG.md`、`README.md`、`docs/guide.md`；本文件；实现时同步改动的规格句子 |
| 非范围 | 提案书 §10 已定的产品行为（见 §1.2）；`cff_subset.rs` 与 OTF；`pdfcraft-pdf` / `pdfcraft-post` / `pdfcraft-orgchart`；hcsw 工程 |
| 来源 | 2026-11-19 对 `pdfcraft-forms 0.3.0` 的逐行审查。复现脚本见 §8 |

相关权威文档：

- [`提案书.md`](提案书.md) §10、§14：产品范围与公共形状。本 spec 不改其中任何已定条目。
- [`pdfcraft-forms-spec.md`](pdfcraft-forms-spec.md)：行为合同。§5 写明「坏字体在 `Font` 的构造函数」失败，本 spec 的 W2 就是把实现拉回这句合同。
- [`pdfcraft-forms-font-subset-spec.md`](pdfcraft-forms-font-subset-spec.md)：`glyf` 子集合同。保留原 glyph id、对象顺序、`with_compression(false)`、两次 `to_pdf` 字节相同，这些都继续成立。
- [`rust-engineering-standard.md`](rust-engineering-standard.md)：规则号出处。门禁命令见 TOOL-5。
- [`../AGENTS.md`](../AGENTS.md)：版本号与产品约束。

本文件只定义要做什么。每个工作项（W1–W9）是一个独立 PR。

---

## 0. 总成功标准

全部工作项合并后必须同时满足：

1. TOOL-5 中与 forms 相关的门禁全绿：`cargo fmt --all -- --check`、`cargo clippy -p pdfcraft-forms --all-targets -- -D warnings`、`cargo test -p pdfcraft-forms`、`cargo doc -p pdfcraft-forms --no-deps`（`RUSTFLAGS` / `RUSTDOCFLAGS` 设为 `-D warnings`）。最后一个 PR 跑完整 TOOL-5。
2. §8 的四个复现场景全部变成仓库内的自动化测试，并且通过。
3. 已有测试不删、不放宽断言。确需改断言的，PR 描述里逐条写明原因。
4. 同一份 `Fill` 两次 `to_pdf` 字节相同（含 CJK 字体路径）。
5. 提案书 §10 已定行为不变（§1.2 列表逐条成立）。
6. 不新增直接依赖。读字体结构用 `skrifa::raw`（即 skrifa 传递的 `read-fonts 0.45.0`），不在 `Cargo.toml` 里新加 `read-fonts`。
7. `pdfcraft-forms` 升到 `0.4.0`，`CHANGELOG.md` 按 VER-2 写 `Breaking` / `Changed` / `Fixed`，`AGENTS.md` 里的版本句同步。只在最后一个 PR 改版本号。

---

## 1. 背景

### 1.1 审查结论摘要

`0.3.0` 的数值卫生好：`forbid(unsafe)`、生产代码无 `unwrap`、`BTreeMap` 保证确定性、子集保留原 glyph id、对象嵌套和 `/Parent` 链都有上限。问题集中在四处：

| 编号 | 问题 | 严重度 | 是否已复现 |
| --- | --- | --- | --- |
| F1 | TTC 拆面按表记录复制字节，无去重、无上限。1 MB 输入峰值 RSS 约 1 GB | P0 | 是 |
| F2 | 追加的页面内容流不与原内容流隔离图形状态。原流泄漏的 `cm` 让填入内容画到页外 | P0 | 是 |
| F3 | 字体在 `Font::new` 只过 skrifa 的宽松解析；`loca`/`glyf`/复合字形的校验在 `to_pdf` 才做，违反 `pdfcraft-forms-spec.md` §5 | P1 | 读码确认 |
| F4 | 三套手写 SFNT 目录解析；子集把整份字体逐表 `to_vec`；PDF 侧 `lookup` 每次克隆对象 | P1 | 读码确认 |
| F5 | `FontReason::Head` 被当作子集阶段约 40 处失败的统称；若干原因名错配 | P1 | 读码确认 |
| F6 | 不可能状态靠运行时错误兜底，且借用了 `FlattenFailed(ReadPage)` 这个不相干的原因 | P1 | 读码确认 |
| F7 | `Fill::new` rustdoc 写的错误顺序与实现不符 | P1 | 是 |
| F8 | 嵌入字体字典缺 `Length1`、无子集前缀、`FontBBox` / `CapHeight` / `Flags` 是假值 | P1 | 是 |
| F9 | 缩字循环每一步全量重排，无提前退出 | P2 | 读码确认 |
| F10 | 排版语义小毛病与内部重复代码 | P2 | 读码确认 |

### 1.2 不是缺陷、本 spec 不改的已定行为

下列行为审查时一度被当成问题，核对提案书后确认是已定产品决定。实现时不得「顺手修」：

1. 字段名取 `/T` 沿 `/Parent` 第一个已有键，**不拼接点分全名**（提案书 §10；`pdfcraft-forms-spec.md` §4.1 第 5 条）。
2. flatten **不保留**未绘制控件的原外观（`/AP`）（提案书 §10；`pdfcraft-forms-spec.md` §4.2）。
3. TTC **拆成单个面再嵌入**（提案书 §10）。W1 改的是拆的方式，嵌入的仍是单个面。
4. 不读 `/Q`、`/DA`、`/MaxLen`、多行标志、`/Rotate`、CropBox，也不读 `/MK /R`。
5. `FontFile2` 不压缩（`with_compression(false)`）。

想改其中任何一条，先改提案书，不在本 spec 内做。

### 1.3 其他明确不做

- `src/cff_subset.rs`：对应 [`pdfcraft-forms-otf-spec.md`](pdfcraft-forms-otf-spec.md)。该文件当前未被 `lib.rs` 挂载，也不在 `Cargo.toml` 的 `include` 里。本 spec 不改、不删、不挂载它。它的去留与 OTF spec 中过时的 API 名（`list_fields`、`FontBytes`）另议。
- 输入配额（PDF 大小、对象数、文本长度上限作为新错误）。提案书只把 post 的输入限额列为延后，forms 没有确认。W7 只做不改变结果的算法提前退出。
- `lopdf` 内部解析的资源上限。
- `impl From<ErrorKind> for Error` 与 `pdfcraft-forms-spec.md` §5「没有 `From<ErrorKind>`」的矛盾。记录在案，另议。
- 删除子集里的 `cmap` 表（`CIDFontType2` + Identity 理论上不读它）。需要多阅读器验证，另议。
- ToUnicode 只映射每个 gid 首次出现的码位。

---

## 2. W1：TTC 零拷贝，消除内存放大（F1）

### 2.1 问题

```text
src/font/mod.rs  extract_collection_face
records.push((tag, checksum, bytes[off..off + len].to_vec()));
```

每条表记录复制 `off..off+len`。`numTables` 最多 65535，每条可以指向整个文件，放大倍数等于表数。另有 `sfnt + 12`、`dir + tables * 16`、`12 + index * 4` 未用 checked 算术，在 32 位目标上可能回绕后越界 panic。

### 2.2 方案

1. `Font` 改为持有 `Arc<Vec<u8>>`（调用方给的原始字节，单面或集合）加 `face_index: u32`。单面的 `face_index` 为 0。
2. 测量与子集都用 `FontRef::from_index(bytes, face_index)` 读取，零拷贝。删除 `extract_collection_face`。
3. 子集输出本来就是重新拼装的单个 `sfnt`，因此嵌入物仍是单个面，满足 §1.2 第 3 条。
4. `Font::from_shared_collection` 保存调用方的 `Arc`（不再新建字节）。rustdoc 中「The extracted face is a new byte buffer」一句改掉。`from_shared` / `from_shared_collection` 仍不修改该 `Arc`。
5. `Font::new` 遇到 `ttcf` 仍返回 `FontReason::Collection`；`from_collection` 遇到非 `ttcf` 仍返回 `Collection`；索引越界仍是 `Collection`。
6. `Debug` 继续只在集合时打印 `index`。

### 2.3 验收

- 新增集成测试 `tests/font_collection.rs`：
  - 构造一个约 1 MiB 的 `ttcf`，单个面声明 4096 条表记录，每条 `offset = 0`、`length = 文件长`。`Font::from_collection(bytes, 0)` 返回 `Err`，种类为 `FontNotConfigured(_)`。
  - 该测试二进制安装一个计数全局分配器（`#[global_allocator]`，只在该测试文件内），断言调用期间峰值新增分配 **小于 4 × 输入长度**。
  - 合法集合：用测试内拼出的两面 `ttcf`（复用现有 `collection_of` 的思路），`from_collection(_, 1)` 能测量第二个面的字形。
- `from_shared_collection` 后 `Arc::ptr_eq(&font_bytes, &shared)` 为真（通过 crate 内单元测试断言）。
- 在设置了 `PDFCRAFT_FORMS_FONT` 且指向 `.ttc` 时，填写 `chinese_form.pdf` 的输出与本 PR 之前**字节相同**（W1 不应改变嵌入结果；PR 描述里贴对比命令与结果）。
- `rg 'extract_collection_face' crates/pdfcraft-forms/src` 无结果。

---

## 3. W2：字体在边界一次校验，弃用手写目录解析，细分错误原因（F3、F4 字体侧、F5）

### 3.1 问题

- `Font::new` 接受的字体，可能在 `to_pdf` 时因 `parse_sfnt` 更严的检查失败（4 字节对齐、表重叠、`loca` 单调、复合字形越界）。这违反 `pdfcraft-forms-spec.md` §5 与 ARC-3。
- 三套目录解析：`font::head_span`、`font::extract_collection_face`（W1 已删）、`subset::parse_sfnt`。`subset::TableTag` 重造了 `Tag`。
- `subset::parse_sfnt` 把每张表 `to_vec` 进 `BTreeMap<TableTag, Vec<u8>>`，每次 `to_pdf` 复制整份字体。
- `head_error()` 是子集阶段所有失败的统称。`ascent`/`descent` 非有限报 `Width`；glyph id 超 `u16` 报 `NoAdvance`。
- CONV-3：`head_span` 里 `unwrap_or(usize::MAX)`、`unwrap_or(0)` 哨兵值。API-4：`head_span` 返回三元组。

### 3.2 方案

1. 新增 crate 内类型（名字可调整，形状固定）：

   ```rust
   /// A face whose glyf/loca/maxp/head/hhea/hmtx agree. Built only by `GlyfFace::parse`.
   pub(crate) struct GlyfFace<'a> {
       font: FontRef<'a>,
       num_glyphs: u16,
       loca_long: bool,
       // 其余按需要缓存，全部借用 `font` 的数据
   }
   ```

   `Font` 的每个构造函数调用一次 `GlyfFace::parse`，成功才返回 `Font`。`Fill::new` 与子集只接受 `GlyfFace`，不再自行校验结构。
2. `GlyfFace::parse` 必须检查，并按下表给出原因：

   | 检查 | 失败原因 |
   | --- | --- |
   | `FontRef::from_index` 失败 | `Unreadable`（保留 `source`） |
   | 无 `glyf` | `NoGlyf`（不变） |
   | `head` 缺失或不足 54 字节，`indexToLocFormat` 不是 0/1 | `Head` |
   | `units_per_em == 0` | `UpemZero`（不变） |
   | `maxp` 缺失，或 `numGlyphs == 0` | `Malformed { table: FontTable::Maxp }` |
   | `loca` 缺失、长度不足 `numGlyphs + 1` 项、非单调、末项超 `glyf` 长度 | `Malformed { table: FontTable::Loca }` |
   | 任一复合字形的组件 gid ≥ `numGlyphs`，或组件记录越过该字形末尾 | `Malformed { table: FontTable::Glyf }` |
   | `hhea` 或 `hmtx` 缺失 | `Malformed { table: FontTable::Hhea }` / `FontTable::Hmtx` |
   | `ascent`/`descent` 非有限 | `Malformed { table: FontTable::Hhea }` |

   复合字形检查遍历全部 glyph 一次（CJK 约 6.5 万个，PR 描述里报告实测耗时）。用 `skrifa::raw::tables::glyf` 的 `Glyph` / `CompositeGlyph::components()`，不手写 flag 解析。
3. 公共错误面变化：

   ```rust
   #[non_exhaustive]
   pub enum FontReason {
       Missing, Collection, NoGlyf, UpemZero, NoAdvance, Head, Width, Unreadable, // 保留
       /// A required table is missing or disagrees with another table.
       Malformed { table: FontTable },                                            // 新增
       /// A cmap entry maps to a glyph id outside 0..=u16::MAX.
       GlyphIdOutOfRange,                                                         // 新增
   }

   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   #[non_exhaustive]
   pub enum FontTable { Maxp, Loca, Glyf, Hhea, Hmtx }
   ```

   - `Head` 只表示 `head` 表本身的问题。
   - `Width` 只表示 em-1000 宽度换算失败（`embed::em1000`）。
   - `NoAdvance` 只表示 skrifa 给不出 advance。
   - `FontTable` 在 crate 根重导出（`#[doc(inline)]`），不暴露 skrifa 的 `Tag`（API-11）。
   - `Display` 措辞按 ERR-3。
4. 子集改为借用：`subset_sfnt(face: &GlyfFace<'_>, used: ...)`，表数据经 `FontRef::table_data` 借用；输出 `BTreeMap` 的值改为 `Cow<'_, [u8]>` 或等价借用形态，只有被改写的表（`head`、`loca`、`glyf`、`cmap`）是新分配。子集阶段除输出长度溢出 `u32` 外不再有字体结构错误；该溢出用 `Malformed { table: FontTable::Glyf }`。
5. 删除 `subset::TableTag`、`subset::parse_sfnt`、`font::head_span`、`font::check_head`。内部用 `skrifa::raw::types::Tag`。
6. `zero_head_times` 并入子集输出的写出步骤：`write_sfnt` 返回时 `head` 的时间戳已清零、表校验和与 `checkSumAdjustment` 已正确。`embed` 不再单独调用它。`sfnt::checksum` 保留为叶子模块。
7. `Font::new` 等构造函数的 `# Errors` 按新原因重写。

### 3.3 验收

- 单元测试（crate 内，用现有 `abc_test_font` 夹具做变异）逐条覆盖 §3.2 第 2 点表格的每一行，断言 `kind()` 精确等于对应 `FontNotConfigured(..)`。
- 「构造成功则 `to_pdf` 不会因字体结构失败」：对上表每种变异，断言失败发生在 `Font::new`，而不是 `to_pdf`。
- `tests/fuzz_parser.rs` 现有用例全部通过。新增一个确定性的变异循环：对 `abc_test_font` 的每个字节位置分别翻转一个字节，调用 `Font::new`，若成功再走 `Fill::new` + `to_pdf`（只勾选/画一个已映射字符）。**不得 panic**；`to_pdf` 不得返回 `FontNotConfigured`。
- `rg 'fn head_error|struct TableTag|fn parse_sfnt|fn head_span' crates/pdfcraft-forms/src` 无结果。
- `rg 'unwrap_or\(usize::MAX\)' crates/pdfcraft-forms/src` 无结果。
- 子集输出：用 skrifa 打开子集，`head` 时间戳全 0；对整份子集求 SFNT 校验和等于 `0xB1B0AFBA`。
- 设置 `PDFCRAFT_FORMS_FONT` 时，`pdfcraft-forms-font-subset-spec.md` §0 第 7 条体积上限仍成立。
- `CHANGELOG.md`：`Changed` 写错误原因的重新划分与失败时机提前；`Added` 写 `FontReason::Malformed`、`GlyphIdOutOfRange`、`FontTable`。

---

## 4. W3：页面内容流隔离图形状态（F2）

已由 [`pdfcraft-forms-overlay-state-spec.md`](pdfcraft-forms-overlay-state-spec.md) P2 取代。

### 4.1 问题

`paint` 直接 `add_page_contents` 追加新流。原内容流结尾若留有未 `Q` 的 `cm`、颜色、线宽、裁剪，新内容会继承它们。复现中原流以 `2 0 0 2 0 0 cm` 开头且不恢复，填入文字落到 MediaBox 之外。

### 4.2 方案

1. 对每个要绘制的页：在 `/Contents` 最前插入一个只含 `q\n` 的新流；本库追加的流以 `Q\n` 开头，随后是现有的 stamp 与 stroke 内容。
2. `/Contents` 是单个流或数组都要处理；处理后统一为数组，顺序为 `[q 流, 原有流…, 本库流]`。
3. 只给本次确实追加内容的页做这件事。只剥离、不绘制的页不改 `/Contents`。
4. 新增私有方法放在 `pdf/write.rs`，例如 `PdfDoc::wrap_and_append(page, bytes)`，替换现有 `add_page_contents`。失败原因仍为 `FlattenFailed(AppendContent)`。
5. 不改写原有流的字节。

### 4.3 验收

- 集成测试（`tests/fill_and_flatten.rs` 或新文件），用 lopdf 在测试内生成夹具：一页，原内容流为 `2 0 0 2 0 0 cm\n`（不恢复），一个 CheckBox 控件和一个 Text 控件。
  - 结构断言：输出页 `/Contents` 是数组；第一个流解码后去除空白等于 `q`；最后一个流以 `Q` 开头；中间是原流且字节不变。
  - 几何断言（需要 `PDFCRAFT_FORMS_FONT`，未设置则跳过）：`pdftotext -bbox` 中填入词的 `xMin`、`yMin`（换算回 PDF 用户空间）落在该控件 `/Rect` 内。
- 无 `cm` 泄漏的普通夹具：所有已有 `pdftotext` 断言不变。
- 两次 `to_pdf` 字节相同。

---

## 5. W4：不可能状态由类型排除；修正 `Fill::new` 错误顺序（F6、F7）

### 5.1 问题

- `fill.rs` 中「有 `Intent::Text` 但没有 face」返回 `FlattenFailed(ReadPage)`。
- `paint/stream.rs` 中「有文本 stamp 但没有 `font_id`」返回 `FlattenFailed(ReadPage)`。
- `Fill::new` 里 `push_stamps` 的两个分支只差一个 `Option`。
- `accept` 对每个 widget 调用两次，每次都克隆文本。
- rustdoc 写「`InvalidArgument`（含线段非有限、线段页号越界）最先返回」，实现中线段检查在最后。复现：未知字段名加 NaN 线段，实际返回 `UnknownField`。

### 5.2 方案

1. `Fill` 的字段改为：

   ```rust
   pub struct Fill {
       doc: Arc<PdfDoc>,
       marks: Vec<Stamp>,          // 勾选、框底横线：不需要字体
       strokes: Vec<Stroke>,
       text: Option<TextPlan>,     // 有非空文本时才存在
   }

   struct TextPlan {
       font: Font,
       used: BTreeMap<u16, UsedGlyph>,   // 非空由构造保证
       stamps: Vec<TextStamp>,
   }

   struct UsedGlyph { scalar: char, advance: f32 }
   ```

   `Mark::Text` 从 `Mark` 中移出，只能出现在 `TextPlan.stamps` 里。`paint` 接收 `Option<&TextPlan>`；有 `TextPlan` 就一定能嵌入字体。两处 `FlattenFailed(ReadPage)` 兜底随之删除。
2. `PaintFont`、`used: BTreeMap<u16, (char, f32)>` 元组全部改为 `UsedGlyph`（API-4）。
3. `Intent::Text` 借用请求里的 `&str`，不克隆；`accept` 每个 widget 只调用一次。
4. `Fill::new` 的检查顺序固定为下面这样，代码与 rustdoc 一致：
   1. 只依赖请求的检查：文档字号 → 每条线段的数值有限 → 每条线段的页号。
   2. 未知名字（`missing = Error` 时）：先 `fields` 键，后 `rules` 键，各按字节序。
   3. 已知名字的请求值：字段字号（按 `fields` 字节序）→ 规则偏移（按 `rules` 字节序）。
   4. 控件类型与值的匹配：按 `Form::fields` 的顺序，每个名字只看第一个同名 widget。产生 `SignatureNotWritable`、`WidgetUnsupported`、`InvalidCheckbox`、`IllegalChoice`、`InvalidArgument(CheckboxOnText | CheckboxOnChoice)`。
   5. 有文本而 `font` 为 `None` → `FontNotConfigured(Missing)`。
   6. 排版：`MissingGlyph`、`FontNotConfigured(NoAdvance)`、`BoxOverflow`，按 `Form::fields` 顺序。
5. `pdfcraft-forms-spec.md` §4.5 同步写入这个顺序。

### 5.3 验收

- 新增测试，对第 4 点中**每两个相邻步骤**构造一个同时触发两者的请求，断言返回前一步的错误。至少包含：「NaN 线段 + 未知名字 → `InvalidArgument(NonFinite)`」「线段页号越界 + 未知名字 → `InvalidArgument(PageOutOfRange)`」「未知 field 名 + 未知 rule 名 → `field()` 为 field 名」「Signature 名 + 缺字体 → `SignatureNotWritable`」「缺字体 + 缺字 → `FontNotConfigured(Missing)`」。
- `rg 'FlattenReason::ReadPage' crates/pdfcraft-forms/src` 只剩真正读页失败的位置（`strip_form`、`copied_resources`、页下标越界）。PR 描述里列出剩余每一处并说明为何可达。
- `rg '\(char, f32\)' crates/pdfcraft-forms/src` 无结果。
- 已有测试全部通过，输出字节与 W3 后相同。

---

## 6. W5：嵌入字体字典合规（F8）

### 6.1 问题

已核对的输出：`FontFile2` 无 `/Length1`；`/BaseFont` 是 `PdfCraftForms`，没有子集前缀；`/FontBBox [0 -236 1000 928]` 是拼出来的；`/CapHeight` 等于 Ascent；`/Flags 32`（Nonsymbolic）。

### 6.2 方案

1. `FontFile2` 流字典写 `/Length1` = 子集字节长度。
2. 子集前缀：6 个大写字母，由 `used` 的 gid 集合确定性导出（对排好序的 gid 大端字节做 FNV-1a 64，依次取 `hash % 26` 共 6 次）。不得使用时钟、随机数或指针地址（DET-2）。`/BaseFont`（Type0 与 CIDFontType2）和 `/FontName` 都写 `ABCDEF+PdfCraftForms`。页面资源名 `/PdfCraftForms…` 不变。
3. `/FontBBox` 取 `head` 的 `xMin yMin xMax yMax`，按 em-1000 换算。
4. `/CapHeight` 取 OS/2 `sCapHeight`（version ≥ 2 且非 0 时）；否则取 `ascent`。
5. `/Flags` 为 4（Symbolic）。
6. 对象顺序保持 FontFile2、FontDescriptor、CIDFontType2、ToUnicode、Type0（字体子集 spec §1.3）。

### 6.3 验收

- 集成测试用 lopdf 打开输出，找到 Type0 字体并断言：
  - `/BaseFont` 匹配 `^[A-Z]{6}\+PdfCraftForms$`，且与 CIDFont 的 `/BaseFont`、Descriptor 的 `/FontName` 相同。
  - `FontFile2` 流有 `/Length1`，值等于流内容长度。
  - `/Flags` 为 4。
  - `/FontBBox` 四个数等于测试字体 `head` bbox 的 em-1000 换算值。
- 同一请求两次输出的子集前缀相同；用到的字形集合不同的两次请求，前缀不同（用两个固定输入断言，不做概率断言）。
- `pdftotext` 抽字断言全部仍然通过。

---

## 7. W6–W9

### 7.1 W6：缩字循环提前退出（F9）

问题：`LaidText::fit` 最多试约 499 个字号，每次 `layout_at` 先折完全部行，再在 `place` 判断是否溢出。

方案：

1. `layout_at` 先由 `inner_h`、`line_h` 与当前字号算出该框最多能容纳的行数上限（保守，取向上的整数加 1）。`wrap` 产生的行数一旦超过上限，立即返回 `TooBig`。
2. 这是纯优化：对任何输入，`fit` 的返回值（字号、每行字形、基线、宽度、错误种类）与改动前**逐位相同**。

验收：

- 新增等价性测试：在 `abc_test_font` 上对一组固定输入（≥ 200 组，确定性生成：不同文本长度、框宽高、起始字号、三种 `VAlign`）比较新旧实现的结果。旧实现以 `#[cfg(test)]` 私有函数保留在测试模块里，PR 合并后的下一个 PR 可删除。
- 性能断言：1 MiB 的 `x` 文本填入 200×30 pt 框，`Fill::new` 返回 `BoxOverflow`。PR 描述里报告改动前后耗时（release 构建，三次中位数）。测试本身不对耗时做断言。

### 7.2 W7：PDF 侧借用与去重（F4 PDF 侧、F10 内部部分）

无行为变化。

1. `PdfDoc::lookup(&self, obj: &'a Object) -> Option<&'a Object>`，借用文档内对象，不克隆。删除 `Lookup` 枚举。
2. 列全 `Object` 所有变体、只为取出某一个变体的 match，改为 `let … else` 或 `if let`。
3. `collect_refs` 与 `refs_dangle` 合并为一个遍历函数（例如带回调或返回迭代结果）。
4. `MAX_PARENT` 只定义一次。
5. `embed::write_cmap` 里吞错误的 `Ok(()) | Err(_) => {}` 改为显式写法：向 `String` 写入不会失败，按 PAN-3 用带理由的 `expect`，或改用不返回 `Result` 的拼接方式。
6. `parse_rect` 对数组元素先 `lookup` 再取数；元素数必须正好为 4，否则跳过该 widget。

验收：已有测试全部通过；输出字节与 W6 后相同（用 `tests/fixtures` 下全部夹具各填一次，比较前后字节，PR 描述贴结果）。`rg 'enum Lookup' crates/pdfcraft-forms/src` 无结果。

### 7.3 W8：排版语义修正（F10 行为部分）

有行为变化，写进 `CHANGELOG.md` 的 `Changed`。

1. 只含空白的段落按空行处理（与空段落一致）。`"a\n \nb"` 与 `"a\n\nb"` 都排成三行。
2. `VAlign::Center` 改为在上下内边距之间居中：中线为 `((y0 + PAD_BOTTOM) + (y1 - PAD_TOP)) / 2`，与 `request.rs` 的 rustdoc 一致。
3. `check_font_size` 删除被 256 pt 上限覆盖的 i64 分支；`ArgumentReason::FontSize` 的 rustdoc 去掉 i64 的说法。
4. `geom::ordered` 删除非有限值分支（`parse_rect` 已拒绝非有限值）；若删除后 `PdfRect::new` 仍可能收到非有限值，则保留并补测试证明可达。

验收：

- 第 1 点：两种输入的行数与基线完全一致的测试。
- 第 2 点：对 `aligns_a_single_line_to_the_vertical_center` 按新公式更新期望，并在 PR 描述里说明这是有意改动。
- 其余已有测试不变。

### 7.4 W9：注释、示例与版本收尾

1. 删除讲来历或引用不存在名字的注释：`shrink_orphan` 的 "Python drops…"；`LaidText::lines` / `glyphs` 文档里的 `Content::text`、`record`；`expect` 理由里的「第 2 步」。按需改写成说明约束的句子。
2. `examples/fill_tax_refund_guarantee.rs`：字体变量必填与「可选设置字体」的说法统一为必填，删除恒为真的 `if`。
3. 两个 example 共用的 `open_font` 保留在 example 内（公共 API 不为此新增入口）。
4. 版本升到 `0.4.0`；`CHANGELOG.md` 汇总 W1–W8；`AGENTS.md` 版本句改为 `pdfcraft-forms` 是 `0.4.0`；`README.md` 与 `docs/guide.md` 中受影响的句子同步（`FontReason` 列表、`from_shared_collection` 的说明、`VAlign::Center`）。
5. 跑完整 TOOL-5 门禁。

验收：TOOL-5 全绿；`cargo semver-checks`（若 CI 有该 job）对 `0.4.0` 的判断与 CHANGELOG 的 `Breaking` 段一致。

---

## 8. 复现场景（必须变成仓库内测试）

2026-11-19 审查时在仓库外用探针程序复现，结果如下。实现时把每个场景落成测试，归属见右列。

| 场景 | 0.3.0 实测 | 期望 | 落在 |
| --- | --- | --- | --- |
| 约 1 MiB `ttcf`，512 条表记录都指向整文件，`Font::from_collection(_, 0)` | 返回 `FontNotConfigured(Head)`，峰值 RSS 1,063,424 KB | 失败且峰值新增分配 < 4 × 输入 | W1 |
| 原内容流 `2 0 0 2 0 0 cm` 不恢复，填 Text 控件 | 填入文字不在页面上；去掉 `cm` 的对照组文字在 (101, 188) | 文字落在控件 `/Rect` 内 | overlay-state P2 |
| 未知字段名 + NaN 线段 | `UnknownField` | `InvalidArgument(NonFinite)` | W4 |
| 输出字体字典 | 无 `/Length1`；`/FontBBox [0 -236 1000 928]`；`/Flags 32`；`/BaseFont` 无前缀 | 见 W5 | W5 |

审查时还复现了「层级字段塌缩为同名」「flatten 丢失预填外观」，二者属于 §1.2 的已定行为，不落测试、不改。

---

## 9. 执行顺序与 PR 纪律

顺序：W1 → W2 → W3 → W4 → W5 → W6 → W7 → W8 → W9。

- W2 依赖 W1（`Font` 的新形状）。W4 依赖 W2（`GlyfFace`）。其余按顺序做可以减少冲突。
- 一个 PR 只做一个 W。不在 W 内顺手改别的 W 的内容。
- 每个 PR 的中间步骤按 TOOL-5「小步检查」；PR 收尾跑 forms 相关门禁（§0 第 1 条）。
- 每个 PR 在 `CHANGELOG.md` 的 `Unreleased` 下记一条。版本号只在 W9 改。
- 不擅自 `git commit` / push，除非用户明确要求。
- 不修改 hcsw 工程中的任何文件；可以只读对照。
