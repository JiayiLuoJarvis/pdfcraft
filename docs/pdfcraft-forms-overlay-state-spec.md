# pdfcraft-forms 叠绘图形状态隔离与 lopdf 0.47 升级 Spec

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-23 |
| 状态 | Ready for implementation |
| 范围 | 根 `Cargo.toml` / `Cargo.lock` 的 lopdf 钉版；`crates/pdfcraft-forms` 的 `src/`、`tests/`、`CHANGELOG.md`、`Cargo.toml` 版本；`crates/pdfcraft-post` 的 `tests/encrypt.rs`、`CHANGELOG.md`、`Cargo.toml` 版本；§6 列出的规范文档句子；本文件 |
| 非范围 | 提案书 §10 已定的产品行为（见 §1.4）；`pdfcraft-forms-hardening-spec.md` 除 W3 以外的工作项；`pdfcraft-pdf`；`pdfcraft-orgchart` 源码；hcsw 工程的任何文件 |
| 来源 | 2026-11-23 hcsw CR-ND2A 事故（`nd2a_base.pdf` 第 1 页填充文字不可见）的根因审查，以及对 hcsw 全部底稿的只读扫描 |
| 取代 | [`pdfcraft-forms-hardening-spec.md`](pdfcraft-forms-hardening-spec.md) §4 W3。W3 只处理 `cm` 泄漏；本文件 P2 覆盖 W3 的全部内容并扩展。P2 合并时在 hardening spec §4 标题下加一句「已由 `pdfcraft-forms-overlay-state-spec.md` P2 取代」，§8 表中 W3 一行的「落在」改为「overlay-state P2」 |

相关权威文档：

- [`提案书.md`](提案书.md) §10、§11、§14：forms 与 post 的产品范围与依赖钉版。P1 需要改其中的钉版句子（§6）。
- [`pdfcraft-forms-spec.md`](pdfcraft-forms-spec.md)：forms 行为合同。
- [`pdfcraft-forms-hardening-spec.md`](pdfcraft-forms-hardening-spec.md)：forms 加固系列。本文件排在它之前执行（§8）。
- [`rust-engineering-standard.md`](rust-engineering-standard.md)：规则号出处。门禁命令见 TOOL-5。
- [`../AGENTS.md`](../AGENTS.md)：版本号与产品约束。

本文件只定义要做什么。三个工作项 P1、P2、P3 各是一个独立 PR。

---

## 0. 总成功标准

三个 PR 全部合并后必须同时满足：

1. 完整 TOOL-5 门禁全绿（每个 PR 收尾至少跑 forms 与 post 相关的条目，P3 收尾跑完整 TOOL-5）。
2. §5 复现矩阵里的每个场景都变成仓库内的自动化测试，并且通过。
3. 已有测试不删、不放宽断言。确需改断言的（P1 的两处），PR 描述逐条写明原因。
4. 同一份 `Fill` 两次 `to_pdf` 字节相同，包括 §5 的状态泄漏夹具。
5. 提案书 §10 已定行为不变（§1.4 列表逐条成立）。
6. 不新增直接依赖。lopdf 仍是 `default-features = false`。
7. 本机验收（§5.3）：用 hcsw 的 `nd2a_base.pdf` 填 `brNo` 与 `companyName`，`pdftoppm` 第 1 页与第 2 页的 `brNo` 区域内都有墨迹。
8. 版本：`pdfcraft-forms` `0.3.0 → 0.3.1`，`pdfcraft-post` `0.3.1 → 0.3.2`。只在 P3 改版本号（§7）。

---

## 1. 背景

### 1.1 事故与根因

hcsw CR-ND2A 用 `nd2a_base.pdf`（3 页）填写。第 1 页的公司名、BR、董事名肉眼全空；第 2 页同名字段 `brNo` 清晰可见。`pdftotext` 两页都能抽出填入的字，消融实验（只保留第 1 页的填充流）字立刻可见。

根因：forms 用 lopdf 的 `Document::add_page_contents` 把填充流追加到 `/Contents` 末尾。PDF 把一页的多个内容流当成一条连续的流解释，后一条流继承前一条流结束时的图形状态。lopdf 只追加，不给原内容包 `q … Q`；forms 自己也不重置任何状态。`nd2a_base.pdf` 第 1 页在 `ca = 0` 时执行 `q`，内层 `/GS2 gs` 画完再 `Q`，于是页末的非描边 alpha 恢复成 0。forms 每个 stamp 的 `q` 保存的正是 `ca = 0`，块内没有 `gs`，整页填充全部透明。

底稿合规。PDF 不要求内容流结束前把图形状态恢复为默认值，三页 `q`/`Q` 都平衡。pdftoppm 的渲染符合规范。**这是 forms 的缺陷**：往别人的页面上叠内容的一方必须自己隔离状态（qpdf、pypdf 的叠加功能都会先包裹原内容）。

### 1.2 同类泄漏的影响面

对 hcsw `backend/assets/forms/*.pdf` 中每个带 Widget 的页，模拟页末留下的图形状态（脚本见附录 A）。下表只列与默认值不同的页：

| 底稿 | 页 | 页末与默认不同的状态 | 对 forms 输出的后果 |
| --- | --- | --- | --- |
| `nd2a_base.pdf` | 1 | `ca 0` `CA 0`，描边灰 1 | 文字、勾、线全透明 |
| `nd2a_base.pdf` | 2 | `Tc -0.001` `Tw 0.005` | 字距微偏 |
| `nd2a_change_secretary.pdf` | 1 | `ca 0` `CA 0`，描边灰 1 | 全透明 |
| `nar1_first_page.pdf` | 1 | `ca 0` `CA 0`，描边灰 1 | 全透明 |
| `ndr1_brn.pdf` | 1 | `ca 0` `CA 0`，描边灰 1 | 全透明 |
| `request_return_of_remuneration_and_pensions.pdf` | 1 | `3 Tr`（不可见文字） | 文字不可见；只修 alpha 不够 |
| `nr1.pdf`、`nr1_address_change_letter.pdf`、`nnc1_d.pdf` | 1 | 描边灰 1（白） | 勾是白色 |
| `nature_of_business.pdf` | 1 | 描边 RGB `0.6 0.2 0` | 勾是红褐色 |
| `deregistration_no_objection_2024.pdf` | 1、2 | `Tc 0.2837`，第 1 页描边 CMYK | 每字形多 0.2837 pt：40 字形多约 11 pt，右/中对齐偏移、长值被 `/Rect` 裁掉 |
| `nar1_base.pdf`（7 页）、`nar1_c.pdf`、`nd2a_b.pdf`、`nnc1_base.pdf` 3/4/6、`office_lease.pdf` 2/3 | — | 小的 `Tc` / `Tw` | 字距微偏 |

说明：

- `Tw` 只作用于单字节编码中的码 32。forms 用 Identity-H 双字节编码，`Tw` 实际无影响，但仍按统一规则重置。
- `Tc` 是未缩放的文本空间单位，不乘字号。forms 的量宽假定 `Tc = 0`。
- 扫描的底稿里没有顶层 `cm` 或裁剪路径泄漏，但 hardening spec §8 已在合成夹具上复现过 `cm` 泄漏把文字画到页外。第三方 PDF 里很常见。

### 1.3 forms 代码现状（逐项）

| 位置 | 现状 | 缺口 |
| --- | --- | --- |
| `pdf/write.rs` `add_page_contents` | 直接转调 lopdf，追加到 `/Contents` 末尾 | 不隔离原内容；0.45 下 `/Contents` 是间接数组时写成 `[指向数组的引用, 新流]`，结构非法 |
| `pdf/write.rs` `attach_font` | 只在页面需要字体时复制 Resources 并加 `/Font` | 只有勾 / 线的页不改 Resources，无处注册 ExtGState |
| `paint/stream.rs` `page_bytes` | 流以 `\n` 开头，随后是各 stamp | 不设 `gs` |
| `paint/stream.rs` `text` | `q re W n`、`0 0 0 rg`、`BT … ET`、`Q` | 不重置 `Tr` `Tc` `Tw` `Tz` `Ts` |
| `paint/stream.rs` `check` | `q re W n`、`w` `1 J` `1 j`、路径 `S`、`Q` | 不设描边色、不设虚线 |
| `paint/stream.rs` `line_below` / `stroke` | 设了 `RG` 与 `w` | 不设虚线、线帽、线连接 |
| `pdf/snapshot.rs` `from_annot` | `inherited(&dict, b"Rect")` | `/Rect` 不是可继承键（P3） |

确认没有问题的部分：`fill.rs` 的 `push_stamps` 按 `widgets.zip(fields)` 逐个 widget 出 stamp，同名多页 widget 每页都画；填充流拼在 `/Contents` 末尾的顺序本身正确。

### 1.4 不是缺陷、本 spec 不改的已定行为

实现时不得「顺手修」：

1. flatten 去掉全部 Widget 与 `/AcroForm`，不保留未绘制控件的原外观（提案书 §10）。
2. 不读 `/Q`、`/DA`、`/MaxLen`、多行标志、Comb、`/Rotate`、CropBox、`/MK /R`（hardening spec §1.2 第 4 条）。hcsw 的 `nature_of_business.pdf`（27 个 Comb 字段）与 `nr1_address_change_letter.pdf`（8 个）会画成普通文本，这是已定范围。
3. 同名 widget 每个都画（提案书 §10「同名 widget」）。不得改成只画第一个。
4. 字体缺字即失败，不回退。
5. 填充色固定为黑。

### 1.5 明确不做

- 解析原内容流去数 `q`/`Q` 深度（见 §3.6 已知局限）。
- 读 Widget `/F` 的 Hidden / NoView / Print 位。hcsw 底稿中没有这类控件，另议。
- ToUnicode 同一 gid 多码位（hardening spec §1.3 已列为不做）。
- post 的输入限额作为新的错误种类或新的公共选项（提案书 §11 延后）。P1 只接受 lopdf 0.47 的内置默认上限，见 §2.4。

---

## 2. P1：workspace 升级 lopdf 到 0.47.0

### 2.1 事实核对（2026-11-23）

- crates.io 最新为 `0.47.0`（2026-11-04），中间有 `0.46.0`（2026-10-21）。两版均未 yank。
- `rust-version` 仍为 1.88（本仓库 MSRV 1.98，满足）。feature 列表不变。`default-features = false` 时的依赖与版本和 0.45 相同，不引入额外的 skrifa / read-fonts。
- forms 与 post 用到的 API 签名均未变：`Document::load_mem`、`page_iter`、`get_pages`、`add_object`、`new_object_id`、`save_to`、`encrypt`、`is_encrypted`、`was_encrypted`、`decode_text_string`。
- `lopdf::Error` 新增 `ObjectIdOverflow`。`DecompressError` 新增 `TooManyFilters`（经 `Error::Decompress` 传出）。
- `Document::add_page_contents`：`/Contents` 是间接引用且目标是数组时，0.47 展开该数组（0.45 不展开）。P2 自己写 `/Contents`，不依赖这一点。
- `LoadOptions::default()` 的 `max_decompressed_size` 从 `None` 改为 `Some(DEFAULT_MAX_DECOMPRESSED_SIZE)`，即每个流 256 MiB。`load_mem` 使用默认值。
- 读取时同时丢弃 trailer 的 `/Prev` 与 `/XRefStm`（0.45 只丢 `/Prev`）。
- 加密字典的键写出顺序改变：`/Filter/Standard/V 5/R 6/Length 256/EncryptMetadata …`。

在仓库副本中把钉版改为 `=0.47.0` 实测：forms 编译失败 1 处（§2.2 第 2 步）；补上后 forms、post、orgchart 的测试只有 2 个失败，均为测试自身对 0.45 行为的假设（§2.2 第 4、5 步）；`cargo clippy -p pdfcraft-forms -p pdfcraft-post --all-targets --all-features -- -D warnings` 通过。

### 2.2 改动

1. 根 `Cargo.toml` `[workspace.dependencies]`：`lopdf = { version = "=0.47.0", default-features = false }`。`cargo update -p lopdf --precise 0.47.0` 更新 `Cargo.lock`，lock 中只允许 lopdf 一行变化（PR 描述贴 `git diff --stat Cargo.lock` 与 lopdf 段的 diff）。
2. `crates/pdfcraft-forms/src/pdf/read.rs` `open_error`：在映射到 `ErrorKind::NotAPdf` 的分支里加 `lopdf::Error::ObjectIdOverflow`。继续保持穷尽匹配，不改成通配，以便下次升级仍被编译器拦下。
3. `crates/pdfcraft-forms/src/pdf/write.rs` `prune` 的 rustdoc：把「lopdf 0.45 already drops `/Prev` while reading」改为「lopdf 0.47 drops `/Prev` and `/XRefStm` while reading」。保留 `prune` 对这两个键的删除，作为与读取实现无关的保证。
4. `crates/pdfcraft-forms/tests/fill_and_flatten.rs` `drops_prev_xrefstm_and_trailer_keys_that_point_at_orphans`：
   - 前置断言 `before.trailer.get(b"XRefStm").is_ok()` 改为 `is_err()`。
   - 注释改为描述 0.47 的读取行为（读取时 `/Prev` 与 `/XRefStm` 都被删除）。
   - 输出侧断言（第 687–692 行）一字不改。
   - 第 805 行附近注释里的「lopdf 0.45」同步改为 0.47。
5. `crates/pdfcraft-post/tests/encrypt.rs` `v5_stdcf_declares_length_32`：顶层 `/Length 256` 的定位方式改为按对象号定位，不再取字节里第一个 `/Encrypt`（0.47 下它命中 `/EncryptMetadata`）：
   1. 在输出字节中找 trailer 或 xref 流字典里的 `/Encrypt N G R`，解析出 `N G`。
   2. 找 `N G obj` 对象头，取到其后第一个 `endobj` 为止的字节作为加密字典。
   3. 在该范围内断言 `/Length 256`。
   - 其它断言不改。`/StdCF` 一段的断言照旧。
6. 不改 post 的 `encryption.rs` 生产代码。0.47 的实际输出已含 `/Length 256`，post 的就地补丁继续生效，行为与 0.45 相同。

### 2.3 验收

- `cargo test -p pdfcraft-forms`、`cargo test -p pdfcraft-post`、`cargo test -p pdfcraft-orgchart` 全部通过（含 `--all-features`）。
- `cargo clippy -p pdfcraft-forms -p pdfcraft-orgchart -p pdfcraft-post --all-targets -- -D warnings` 通过。
- `cargo deny check advisories licenses bans sources` 通过。
- `cargo tree -d -e normal` 中不出现第二份 lopdf；skrifa / read-fonts 的重复与升级前相同（`skrifa 0.42.1` 与 `0.48.0` 两份，AGENTS.md 已记录）。
- `rg 'lopdf 0\.45|=0\.45\.0' AGENTS.md docs/提案书.md docs/workspace-crates-spec.md docs/pdfcraft-post-spec.md crates/` 无结果（历史文档见 §6.2，不改）。

### 2.4 行为变化：单流解压上限 256 MiB

0.47 的 `load_mem` 对每个流的解压结果设 256 MiB 上限，超过时加载失败。

- forms：`Form::open` 返回 `ErrorKind::NotAPdf`，`source()` 是 lopdf 的 `Decompress(MemoryLimitExceeded)`。不新增错误种类。
- post：`open_pdf` 返回 `ErrorKind::NotAPdf`，`source()` 同上。不新增错误种类，不新增公共选项。

决定：接受该默认值，不显式传 `None`。理由：它只拦截单流解压超过 256 MiB 的异常输入，对正常业务 PDF 无影响，并防止小文件解压炸弹。这是依赖的内置防护，不是提案书 §11 延后的「post 输入限额」功能（后者指可配置的大小 / 页数 / 对象数配额与对应错误面），二者不冲突。提案书 §11 的依赖句补一句说明（§6.1）。

不为此新增测试夹具（构造 256 MiB 解压流的测试太慢）。两个包的 `CHANGELOG.md` 记为 `Changed`（§7）。

---

## 3. P2：叠绘前隔离并重置图形状态

P2 依赖 P1 合并（测试里用 lopdf 0.47 的行为）。

### 3.1 目标

填入的文字、勾、框底横线和绝对线段，不论底稿页末留下什么图形状态，都以 PDF 默认图形状态加上本库显式设定的值绘制：不透明、正常混合、无软蒙版、单位 CTM（相对页面默认用户空间）、无额外裁剪、实线、文本渲染模式 0、无额外字距 / 词距 / 水平缩放 / 上升。

两层手段同时做：

- **A. 隔离**：用 `q … Q` 包住原内容，叠绘流从页面初始图形状态开始。覆盖全部状态，包括 CTM 与裁剪。
- **B. 显式重置**：叠绘流开头设一个本库自有的 ExtGState，每类标记在自己的 `q` 内重设它依赖的参数。原内容 `q` 多于 `Q` 时 A 失效，B 仍能保证可见性与宽度（§3.6）。

### 3.2 `/Contents` 隔离（`pdf/write.rs`）

新增私有方法，替换 `PdfDoc::add_page_contents`（原方法删除，不留过渡层，VER-3）：

```rust
impl PdfDoc {
    /// 把 `overlay` 叠到 `page` 的原内容之上，原内容的图形状态不泄漏到 `overlay`。
    pub(crate) fn append_isolated(
        &mut self,
        page: ObjectId,
        save: ObjectId,     // 只含 `q\n` 的共享流
        overlay: Vec<u8>,   // 由 `page_bytes` 生成，以 `\nQ\n` 开头
    ) -> Result<()>;
}
```

规则：

1. 解析页面自身的 `/Contents`（`/Contents` 不可继承，不沿 `/Parent` 找），得到原有流的引用列表 `original`：

   | `/Contents` 的值 | `original` |
   | --- | --- |
   | 缺失 | 空 |
   | `null`，或指向不存在对象的引用 | 空（PDF 中悬空引用等同 null） |
   | 指向流的引用 | `[该引用]` |
   | 指向数组的引用 | 该数组的元素，原样复制 |
   | 直接数组 | 该数组的元素，原样复制 |
   | 其它任何值（整数、名字、字典、指向非流非数组对象的引用等） | 失败：`FlattenFailed(AppendContent)` |

   数组元素本身不校验、不解引用、不改写。这个分类写成一个穷尽 `match`（ARC-9），不写成 `if … return` 链（TRK-13）。
2. 新建 overlay 流对象（`Stream::new(Dictionary::new(), overlay)`，压缩设置与现在的 `add_page_contents` 相同）。
3. 把页面的 `/Contents` 设为**新的直接数组** `[save, original…, overlay]`。
4. 不修改 `original` 中任何流的字节或字典。不修改被引用的原数组对象：多页共享同一个间接 `/Contents` 数组时，其它页保持原样。
5. 只对本次确有叠绘内容的页调用（与现在 `pages_to_paint` 的集合相同）。只剥离、不绘制的页不碰 `/Contents`。
6. `save` 流对象全文档只建一个，在 `paint` 的页循环之前创建，各页共享引用。内容恰为 `q\n`。

### 3.3 资源注册（`pdf/write.rs`）

`attach_font` 改为单一入口 `attach_overlay_resources`（ARC-10：资源合并只在这一处）：

```rust
pub(crate) struct OverlayIds {
    pub(crate) font: Option<ObjectId>,   // 本页有非空文本时为 Some
    pub(crate) opaque: ObjectId,         // §3.4 的 ExtGState 对象
}

pub(crate) struct OverlayNames {
    pub(crate) font: Option<String>,
    pub(crate) opaque: String,
}

impl PdfDoc {
    pub(crate) fn attach_overlay_resources(
        &mut self,
        page: ObjectId,
        ids: &OverlayIds,
    ) -> Result<OverlayNames>;
}
```

规则：

1. 复制页面的有效 Resources（沿 `/Parent` 继承，沿用现在的 `copied_resources`，包括 `ResourcesNotDictionary` 失败）。
2. 对每个要写的子字典（`/ExtGState` 必写，`/Font` 在 `ids.font` 为 `Some` 时写），用同一个私有函数取出可修改的副本：值是字典，或引用指向字典时克隆该字典；缺失、悬空或不是字典时从空字典开始（与现在 `/Font` 的规则相同）。原子字典里的已有条目全部保留。
3. 名字分配泛化为 `fresh_name(dict, base)`：先试 `base`，再试 `base2` 到 `base64`，与现在 `fresh_font_name` 的序列相同。字体基名仍为 `PdfCraftForms`，ExtGState 基名为 `PdfCraftOpaque`。
4. 新增 `FlattenReason::GraphicsStateNameTaken`，rustdoc：「`PdfCraftOpaque` through `PdfCraftOpaque64` are already used on the page.」`FontNameTaken` 不变。`FlattenReason` 已是 `#[non_exhaustive]`，新增变体不是 breaking。
5. 新 Resources 作为新对象加入文档，页面 `/Resources` 改为指向它（与现在相同）。每个叠绘页恰好新建一个 Resources 对象，不论该页是否有文字。

### 3.4 不透明 ExtGState 对象

全文档一个，在 `paint` 的页循环之前创建（紧随字体对象之后），各页共享引用：

```text
<< /Type /ExtGState
   /CA 1  /ca 1
   /BM /Normal
   /SMask /None
   /AIS false
   /OP false  /op false >>
```

- `CA` / `ca`：描边 / 非描边 alpha。事故的直接原因。
- `BM`、`SMask`、`AIS`：底稿可能留下的混合模式、软蒙版、alpha 是否作为形状，任何一个都能让叠绘变淡或消失。
- `OP` / `op`：叠印。底稿留下叠印时，在分色输出里可能被吞掉。
- 不写 `OPM`、`LW`、`LC`、`LJ`、`D`、`RI`、`TR`、`HT`、`Font`：线型相关参数由各标记用操作符显式设置（§3.5），其余对本库的标记无影响。

对象写出顺序固定为：字体相关对象（与现在相同）→ ExtGState → `save` 流 → 各页按页序依次 Resources、overlay 流。`to_pdf` 两次字节相同。

### 3.5 叠绘流内容（`paint/stream.rs`）

`page_bytes` 改为接收 `&OverlayNames`。每类标记依赖的状态重置写成具名常量，标记函数按固定顺序拼接，不按底稿状态做条件分支：

```rust
const RESTORE_PAGE_STATE: &str = "Q\n";
const TEXT_STATE_RESET: &str = "0 Tr\n0 Tc\n0 Tw\n100 Tz\n0 Ts\n";
const SOLID_DASH: &str = "[] 0 d\n";
const BUTT_MITER: &str = "0 J\n0 j\n";
```

流的整体结构：

```text
\n                       ← 保留现有的前导换行，防止与上一流末尾的操作符粘连
Q\n                      ← 弹回 save 流压入的状态
/<opaque> gs\n           ← 每页一次
<stamp 1> <stamp 2> … <stroke 1> …
```

各标记（`<…>` 为现有的数值格式化，未列出的部分与现在逐字相同）：

| 标记 | 新内容（按顺序） |
| --- | --- |
| `text` | `q` `<rect> re W n` → `0 0 0 rg` → `TEXT_STATE_RESET` → `BT` → 每行 `/<font> <size> Tf`、`1 0 0 1 <x> <y> Tm`、`<glyphs> Tj` → `ET` `Q` |
| `check` | `q` `<rect> re W n` → `0 0 0 RG` → `SOLID_DASH` → `<w> w` `1 J` `1 j` → 路径 `S` → `Q` |
| `line_below` | `q` → `0 0 0 RG` → `1 w` → `BUTT_MITER` → `SOLID_DASH` → 路径 `S` → `Q` |
| `stroke` | `q` → `<r> <g> <b> RG` → `<w> w` → `BUTT_MITER` → `SOLID_DASH` → 路径 `S` → `Q` |

说明：

- `TL` 不重置：本库不用 `T*`、`'`、`"`。
- `Tf` 每行都设，不需要额外重置字体。
- `0 0 0 rg` 与 `RG` 都切到 DeviceRGB，覆盖底稿留下的颜色空间（包括 Pattern、Separation）。
- `check` 保留现在的圆帽、圆角（`1 J 1 j`）。`line_below` 与 `stroke` 显式写平头、斜接，长度与 `/Rect` 宽度和线段端点一致，不随底稿的 `2 J`（方帽）伸长。
- 每个标记仍在自己的 `q … Q` 内，互不影响。

### 3.6 已知局限

- 原内容 `q` 多于 `Q`（不合规，但现实存在）：叠绘流的 `Q` 只弹出 `save` 那一层，原内容未闭合的 `cm` 与裁剪仍会生效。§3.4 的 ExtGState 与 §3.5 的重置仍保证不透明、颜色、线型和文本宽度正确，只有位置可能偏。完全修复需要解析原内容流数 `q` 深度，不在本 spec 范围（§1.5）。
- 原内容 `Q` 多于 `q`：多余的 `Q` 会提前弹出 `save` 那一层，叠绘流开头的 `Q` 落空。PDF 规范不保证图形状态栈下溢之后的行为，本库对此类输入不作保证。
- 两种情况都只写进 `pdfcraft-forms-spec.md` 的限制说明，不新增错误。

### 3.7 `paint` 的接线

`paint` 的流程改为：

1. 有字体时嵌入字体（不变）。
2. 页集合 `pages_to_paint` 非空时，建 ExtGState 对象与 `save` 流。为空时跳过 3–4，直接到 5。
3. 对每个页（按页序）：`attach_overlay_resources` → `page_bytes(stamps, strokes, &names)` → `append_isolated`。
4. 本页有非空文本但 `font_id` 为 `None` 的兜底错误保持现状（hardening W4 负责消除）。
5. `strip_form`、`prune`（不变）。

### 3.8 验收

**单元测试**（`paint/stream.rs` 的 `tests` 模块）：

- `page_bytes_restores_and_sets_opaque_first`：任意一组 stamp 的输出以 `\nQ\n/PdfCraftOpaque gs\n` 开头。
- `text_resets_text_state_before_bt`：`text` 块中 `TEXT_STATE_RESET` 出现在 `BT` 之前、`q` 之后。
- `check_sets_black_stroke_and_solid_dash`：`check` 块含 `0 0 0 RG` 与 `[] 0 d`，且都在 `S` 之前。
- `rules_and_strokes_set_cap_join_and_dash`：`line_below` 与 `stroke` 块含 `0 J`、`0 j`、`[] 0 d`，都在 `S` 之前。

**集成测试**（新文件 `tests/overlay_state.rs`，夹具在测试内用 lopdf 生成，不新增二进制夹具）：

夹具 `leaky_page_pdf(contents_shape)`：一页 200×200，Resources 含 `/ExtGState << /GSz << /Type /ExtGState /ca 0 /CA 0 >> >>`，原内容流为

```text
/GSz gs 1 G 1 g 3 Tr 0.5 Tc 120 Tz 2 Ts [3 2] 0 d 2 J
```

（不恢复），控件：一个 Text、一个 CheckBox；请求另带一条 `FieldRule` 与一条 `AbsoluteLine`。`contents_shape` 取单流、直接数组（两条流）、间接数组、缺失四种。

| 测试 | 断言 |
| --- | --- |
| `wraps_original_contents_for_every_contents_shape` | 对四种形状：输出页 `/Contents` 是直接数组；首元素指向的流解码后去空白等于 `q`；末元素的流去掉前导空白后以 `Q` 开头；中间元素与原 `original` 列表逐个相同（同一对象号），原流字节不变 |
| `rejects_contents_of_wrong_type` | `/Contents 5`（整数）→ `FlattenFailed(AppendContent)` |
| `leaves_shared_contents_array_untouched` | 两页共享一个间接 `/Contents` 数组，只填第 1 页：第 2 页 `/Contents` 仍指向原数组对象，原数组内容不变 |
| `untouched_pages_keep_contents_and_resources` | 无标记的页 `/Contents` 与 `/Resources` 和输入相同 |
| `overlay_sets_opaque_before_any_paint` | 用 `lopdf::content::Content::decode` 解码 overlay：第一个操作符是 `Q`，第二个是 `gs` 且操作数为 Resources 里新注册的名字；任何 `Tj`、`S` 之前都已出现该 `gs` |
| `opaque_extgstate_has_expected_entries` | 新 ExtGState 字典恰含 §3.4 的键与值；原 `/GSz` 仍在 |
| `opaque_name_avoids_existing_entries` | 底稿 ExtGState 已有 `PdfCraftOpaque` → 新名为 `PdfCraftOpaque2`；底稿 ExtGState 是间接引用时同样成立，且原间接对象未被修改 |
| `opaque_name_exhaustion_fails` | 底稿占满 `PdfCraftOpaque` 到 `PdfCraftOpaque64` → `FlattenFailed(GraphicsStateNameTaken)` |
| `marks_only_page_gets_opaque_resource` | 只有 CheckBox 的页（不需要字体）也注册了 ExtGState，且没有新增 `/Font` 条目 |
| `leaky_page_marks_are_inked` | 用 `pdftoppm -r 72 -gray` 光栅化：CheckBox 的 `/Rect` 内存在灰度 < 128 的像素；`FieldRule` 横线所在行、`AbsoluteLine` 所在位置存在灰度 < 128 的像素。不需要字体 |
| `leaky_page_text_is_inked_and_measured` | 设置 `PDFCRAFT_FORMS_FONT` 时运行，未设置时与现有字体测试一样跳过：Text 的 `/Rect` 内有灰度 < 128 的像素；`pdftotext -bbox` 中填入词的宽度与 `Tc = 0` 时（对照组：同一请求填无泄漏的同尺寸页）相差小于 0.5 pt |
| `cm_leak_does_not_move_text` | hardening §8 的场景：原内容 `2 0 0 2 0 0 cm` 不恢复。需要字体：`pdftotext -bbox` 中填入词落在控件 `/Rect` 内 |
| `leaky_fill_is_deterministic` | 同一 `Fill` 两次 `to_pdf` 字节相同 |

光栅测试的约定：`pdftoppm` 与 `pdftotext` 同属 poppler-utils，CI 已安装（`ci.yml` 的 Install poppler）。调用方式与现有 `pdftotext` 辅助函数相同（`Command`，经 stdin 传入 PDF）。像素坐标换算：`px = x`，`py = page_height − y`（72 dpi 下 1 pt = 1 px），取 `/Rect` 向内收 1 pt 的区域，避免边缘抗锯齿。

**回归**：

- 已有 `tests/fill_and_flatten.rs`、`tests/list_fields.rs`、`tests/fuzz_parser.rs` 全部通过。依赖 `pdftotext` 的断言不改。
- 测试里任何比较 `/Contents` 长度或结构的已有断言，如因 P2 失败，按新结构更新，并在 PR 描述逐条说明。

**文档**：

- `pdfcraft-forms-spec.md` 的 flatten 一节补一句：「本库追加的内容与原内容的图形状态隔离。原内容 `q` 多于 `Q` 时，只保证不透明、颜色、线型与文本宽度。原内容 `Q` 多于 `q` 时，PDF 规范不保证图形状态栈下溢后的行为，本库对此类输入不作保证」。
- hardening spec 按本文件抬头「取代」一行修改。

---

## 4. P3：`/Rect` 只读注释自身，并收尾发布

### 4.1 问题

`snapshot.rs` 用 `doc.inherited(&dict, b"Rect")` 沿 `/Parent` 找 `/Rect`。`/Rect` 是注释字典自己的键，不可继承。提案书 §10 写的是「名字、`/FT`、`/Ff`、`/Opt` 和 `/AP` 各自沿 `/Parent` 取第一个已经存在的键」「库跳过缺少 `/T` 或 `/Rect` 的注释」。缺 `/Rect` 的 kid widget 现在会拿到父字段的矩形，画到错误位置。

### 4.2 方案

1. `from_annot` 读 `/Rect` 只看注释字典自身：`dict.get(b"Rect")` 后经 `lookup` 解引用，再 `parse_rect`。缺失或不是合法矩形时跳过该注释（与现在缺 `/T` 的处理相同）。
2. 不改 `inherited` 本身，不改 `/T`、`/FT`、`/Ff`、`/Opt` 的读取。

### 4.3 验收

- `tests/list_fields.rs` 新增 `widget_without_own_rect_is_skipped_even_if_parent_has_rect`：父字段有 `/Rect`，kid widget 没有 → `Form::fields()` 不含该 widget。
- 同文件已有测试不变。

### 4.4 发布收尾

- 版本号与 `CHANGELOG.md` 见 §7。
- `AGENTS.md`、提案书 §9 / §10 / §11 中的版本句同步为 `pdfcraft-forms` `0.3.1`、`pdfcraft-post` `0.3.2`。
- 跑完整 TOOL-5。

---

## 5. 复现矩阵

### 5.1 仓库内自动化

| 场景 | 当前（0.3.0 + lopdf 0.45）实测 | 期望 | 落在 |
| --- | --- | --- | --- |
| lopdf 钉 `=0.47.0` 编译 forms | `open_error` 穷尽匹配失败 | 编译通过 | P1 |
| 读取含 `/XRefStm` 的 trailer | 前置断言假定保留 | 读取时已删除；输出仍不含 | P1 |
| post V5 加密字典 | 测试按第一个 `/Encrypt` 定位，0.47 下误命中 `/EncryptMetadata` | 按对象号定位，`/Length 256` 存在 | P1 |
| 底稿页末 `ca 0 CA 0`，填 Text / CheckBox / rule / line | 全部不可见 | 全部有墨迹 | P2 |
| 底稿页末 `3 Tr` | 文字不可见 | 文字有墨迹 | P2 |
| 底稿页末描边白色、虚线、方帽 | 勾白色；线虚、线端伸长 | 黑色实线，端点与几何一致 | P2 |
| 底稿页末 `Tc 0.5`、`Tz 120`、`Ts 2` | 文字变宽、上移 | 宽度与对照组一致 | P2 |
| 底稿页末 `2 0 0 2 0 0 cm` | 文字画到页外 | 文字在控件 `/Rect` 内 | P2 |
| `/Contents` 为间接数组 | 0.45 写出非法结构 | `[q, 原元素…, overlay]` | P2 |
| 多页共享 `/Contents` 数组 | — | 未填的页不变 | P2 |
| 底稿已有 `PdfCraftOpaque` 资源名 | — | 用 `PdfCraftOpaque2` | P2 |
| kid widget 无 `/Rect`、父有 | 用父矩形 | 跳过该 widget | P3 |

### 5.2 不落测试、已确认的行为

- `fill.rs` 同名多 widget 每页都画：已有测试覆盖，P2 不改。
- 填充流在 `/Contents` 末尾：P2 之后仍在末尾。

### 5.3 本机验收（hcsw 资产只读，不入库）

hcsw 的底稿与字体不复制进本仓库。P2 的 PR 描述必须贴出下面的结果。

1. 在仓库外建一个临时 crate（例如 `/tmp/nd2a-probe`），`path` 依赖本仓库的 `pdfcraft-forms`。
2. 读 `/mnt/data/dev/hcsw-pdfcraft/backend/assets/forms/nd2a_base.pdf` 与 `…/assets/fonts/SourceHanSerifHC/SourceHanSerifHC-ExtraLight.ttf`。填 `brNo = "12345678"`、`companyName = "SAMPLE LIMITED 示例有限公司"`，样式按 hcsw 现用设置，写到 `/tmp/nd2a-probe/out.pdf`。
3. 光栅化并检查 `brNo` 框的内部（页高 841.68）。裁剪区域是 `/Rect` 四边各向内收 3 pt：底稿在框边印有黑色框线，不内收时修复前的最小灰度也是 0，判据失效。

   | 页 | `brNo` `/Rect` | 内收 3 pt 后的 `pdftoppm` 裁剪参数（72 dpi） |
   | --- | --- | --- |
   | 1 | `[421.53 684.93 561.17 706.93]` | `-f 1 -l 1 -x 425 -y 138 -W 134 -H 16` |
   | 2 | `[428.35 766.94 561.59 788.94]` | `-f 2 -l 2 -x 431 -y 56 -W 127 -H 16` |

   ```bash
   pdftoppm -r 72 -gray <裁剪参数> /tmp/nd2a-probe/out.pdf /tmp/nd2a-probe/brNo
   python3 -c "import sys;d=open(sys.argv[1],'rb').read().split(b'\n',3)[3];print(sum(b<128 for b in d))" /tmp/nd2a-probe/brNo-1.pgm
   ```

   两页灰度 < 128 的像素数都必须大于 0。2026-11-23 在仓库外用 pikepdf 对 hcsw 已有对账输出 `/tmp/nd2a-parity/out/new-gaidong.pdf` 套用 §3.2 与 §3.4 的结构做原型：修复前第 1 页 0 个、第 2 页 79 个；修复后第 1 页 81 个、第 2 页 79 个。实现后的数值应与原型同量级。
4. 用附录 A 的方法确认输出 PDF 中，第 1 页 overlay 起点的模拟状态为 `ca = 1`、`CA = 1`、`Tr = 0`、`Tc = 0`。
5. 建议（不强制）：对 §1.2 表中其余底稿各填一个 Text 字段和一个 CheckBox（若有），重复第 3 步。

---

## 6. 文档同步

### 6.1 规范性文档（必须改）

| 文件 | 位置 | 改动 | PR |
| --- | --- | --- | --- |
| `docs/提案书.md` | §10「依赖」段 | `lopdf` 钉 `=0.47.0`；「0.45 把解析器编进库」改为「自 0.45 起解析器编进库」；版本句 `0.3.0` 在 P3 改 `0.3.1` | P1 / P3 |
| `docs/提案书.md` | §11「依赖」段 | `lopdf =0.47.0`；补一句「lopdf 0.47 的 `load_mem` 对单流解压设 256 MiB 内置上限，超过时为 `NotAPdf`；这不是本条延后的输入限额功能」 | P1 |
| `docs/提案书.md` | 文末确认总表 §11 行 | 钉版号同步 | P1 |
| `docs/提案书.md` | §9 版本句 | P3 同步 forms / post 版本 | P3 |
| `AGENTS.md` | 「已钉死」lopdf 句、「字形管线与依赖」直依列表 | `=0.45.0` → `=0.47.0` | P1 |
| `AGENTS.md` | 版本句 | forms `0.3.1`、post `0.3.2` | P3 |
| `docs/workspace-crates-spec.md` | 第 63、64、117 行 | `=0.45.0` → `=0.47.0` | P1 |
| `docs/pdfcraft-post-spec.md` | 第 62、162、208 行 | `=0.45.0` / `0.45.0` → `0.47.0` | P1 |
| `docs/pdfcraft-forms-spec.md` | flatten 一节 | §3.8「文档」第一条 | P2 |
| `docs/pdfcraft-forms-hardening-spec.md` | §4 标题下、§8 W3 行 | 标注被本文件取代 | P2 |
| 根 `Cargo.toml` | 第 87 行钉版策略注释 | 如提到具体版本则同步；策略本身不变 | P1 |

### 6.2 历史文档（不改）

下列文件记录的是当时的事实，保持原样：根 `CHANGELOG.md` 已发布段落、`docs/enterprise-readiness-spec.md`（2026-10-14 的升级记录）、`docs/pdfcraft-forms-reshape-spec.md` §7.2 的 lopdf 0.45 `Clone` 核对、`docs/pdfcraft-orgchart-draw-fix-spec.md` 第 214 行。实现时如发现 0.47 推翻了其中某条仍被引用的技术事实（例如 `Document: Clone + Send + Sync`），在 PR 描述中说明并另提文档 PR。

---

## 7. 版本与 CHANGELOG

只在 P3 改 `Cargo.toml` 版本号。P1、P2 在各包 `CHANGELOG.md` 的 `Unreleased` 下记条目。

`crates/pdfcraft-forms/CHANGELOG.md`（发布为 `0.3.1`）：

- `Fixed`：填入的文字、勾、框底横线与绝对线段不再继承原页面内容结束时的图形状态。之前原内容留下的透明度为 0、不可见文本渲染模式、白色描边、字距、虚线或坐标变换，会让填入内容不可见、变色、变宽或错位。原内容现在被 `q`/`Q` 包住，叠绘流另设不透明的 `/PdfCraftOpaque` ExtGState，并逐项重置文本与线型状态。（P2）
- `Fixed`：`/Contents` 为间接数组时写出合法结构。（P2）
- `Fixed`：`/Rect` 只读注释自身，不再沿 `/Parent` 继承。（P3）
- `Added`：`FlattenReason::GraphicsStateNameTaken`。（P2）
- `Changed`：依赖 `lopdf` 升到 `=0.47.0`。单个流解压超过 256 MiB 的 PDF，`Form::open` 返回 `NotAPdf`。（P1）
- `Changed`：已填页面的输出字节与 `0.3.0` 不同（新增 `q` 流、ExtGState 与重置操作符）。同一 `Fill` 两次 `to_pdf` 仍逐字节相同。（P2）

`crates/pdfcraft-post/CHANGELOG.md`（发布为 `0.3.2`）：

- `Changed`：依赖 `lopdf` 升到 `=0.47.0`。单个流解压超过 256 MiB 的输入返回 `NotAPdf`。加密输出中 `/Encrypt` 字典的键顺序随 lopdf 变化，内容不变。（P1）

semver：两个包都没有公共 API 的 breaking 变化（`FlattenReason` 为 `#[non_exhaustive]`），按 patch 发布。CI 的 `semver-checks` 应判为 patch 兼容。

---

## 8. 执行顺序与 PR 纪律

顺序：**P1 → P2 → P3**，三者都排在 hardening spec 的 W1 之前。hardening spec 的版本规划（W9 升 `0.4.0`）不受影响，W3 由本文件 P2 取代后不再单独执行。

- P2 依赖 P1（测试用 0.47 的行为）。P3 独立，但放在最后统一发版。
- 一个 PR 只做一个 P。P2 不顺手做 hardening W4（不可能状态）、W5（字体字典）、W7（`lookup` 借用）的内容。P2 新增的私有函数允许为 W7 的后续重构留出位置，但不提前改 `Lookup`。
- 每个 PR 的中间步骤按 TOOL-5「小步检查」。P1、P2 收尾跑 forms 与 post 的 clippy、test、doc；P3 收尾跑完整 TOOL-5。
- 不擅自 `git commit` / push，除非用户明确要求。
- 不修改 hcsw 工程中的任何文件；§5.3 只读使用其资产。

---

## 附录 A：页末图形状态模拟脚本（只读）

用于 §1.2 的扫描与 §5.3 第 4 步。需要 `pikepdf`。只跟踪顶层 `q`/`Q` 栈与本 spec 关心的参数，不解释 Form XObject 内部（XObject 有自己的 `q`/`Q` 包裹，不影响调用者状态）。

```python
import glob, pikepdf

DEFAULT = dict(ca=1.0, CA=1.0, Tr=0.0, Tc=0.0, Tw=0.0, Tz=100.0, Ts=0.0,
               stroke="black", dash=False)

def end_state(page):
    gs = page.Resources.get("/ExtGState", {})
    st, stack = dict(DEFAULT), []
    for operands, op in pikepdf.parse_content_stream(page):
        o = str(op)
        if o == "q":
            stack.append(dict(st))
        elif o == "Q" and stack:
            st = stack.pop()
        elif o == "gs":
            g = gs[str(operands[0])]
            st["ca"] = float(g.get("/ca", st["ca"]))
            st["CA"] = float(g.get("/CA", st["CA"]))
        elif o in ("Tr", "Tc", "Tw", "Tz", "Ts"):
            st[o] = float(operands[0])
        elif o in ("G", "RG", "K", "SC", "SCN"):
            v = [float(x) for x in operands if not isinstance(x, pikepdf.Name)]
            st["stroke"] = "black" if o != "K" and all(x == 0 for x in v) else str(v)
        elif o == "d":
            st["dash"] = len(operands[0]) > 0
    return {k: v for k, v in st.items() if v != DEFAULT[k]}

for path in sorted(glob.glob("/mnt/data/dev/hcsw-pdfcraft/backend/assets/forms/*.pdf")):
    pdf = pikepdf.open(path)
    for i, page in enumerate(pdf.pages, 1):
        if any(a.get("/Subtype") == "/Widget" for a in page.obj.get("/Annots", [])):
            diff = end_state(page)
            if diff:
                print(path.rsplit("/", 1)[-1], i, diff)
```

对 P2 之后的输出做第 4 步时，把 `parse_content_stream(page)` 换成只解析到 overlay 流开头为止（即跳过最后一个 `/Contents` 元素），再在结果上应用 overlay 的前两个操作符 `Q`、`/PdfCraftOpaque gs`，确认 `ca`、`CA` 为 1。
