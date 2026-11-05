# pdfcraft-post 结构重塑规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-05 |
| 状态 | **已按本文实施，随 `0.3.1` 发布。** D1 至 D4 不改公共签名、不改错误种类、不改成功路径的字节，不需要新的提案书条目。§5 在写入提案书之前不要实现 |
| 范围 | 只改 `crates/pdfcraft-post` 的私有模块划分、两处与已确认行为不一致的文档，并在 `tests/merge.rs` 补两条测试。不改 `apply`、自由函数、`Pipeline` 的语义 |
| 版本 | 不升 `0.y`。`0.3.0` 已在 crates.io，本次以补丁 `0.3.1` 发布。公共签名与成功路径字节不变 |
| 相关 | [`pdfcraft-post-spec.md`](pdfcraft-post-spec.md)（行为合同）；[`提案书.md`](提案书.md) §11；[`pdfcraft-forms-reshape-spec.md`](pdfcraft-forms-reshape-spec.md)（对照用的公共面整理，规则只适用于 forms）；[`rust-engineering-standard.md`](rust-engineering-standard.md) ARC-2、ARC-3、VER-3 |
| 基线 | 2026-11-05 工作区 HEAD `f8b79ef`。动手前重新核对 |
| 冲突 | 与提案书或 `pdfcraft-post-spec.md` 冲突时，以那两份为准，并先改本文。实现中发现与代码事实冲突，先停下来改本文，不要自行裁决 |

本文只规定模块归属。填哪些字段、加密哪一版、合并是否留大纲，仍以行为合同为准。

## 1. 产品一句话

调用方继续把已有 PDF 字节交给 `SetMetadata::apply`、`Encrypt::apply`、`StampAndEncrypt::apply`、`merge_pdfs`、`split_pdfs`、`first_page_jpeg` 或 `Pipeline::run`，拿回新的字节。库不向调用方交出已解析文档。forms 0.3 的 `Form::open`、`Fill::new`、`Fill::to_pdf` 不搬到本 crate。

## 2. 对标范式

forms 0.3 把读取、校验和写出拆开，是因为同一份模板会填写很多次，而且批跑要把请求错误和写出错误分成两次调用。post 的每次调用处理一份输入，输入通常各不相同。能迁过来的只有模块归属和「外部字节在边界解析一次」。

| 层 | forms 0.3 的做法 | 在本 crate 的落点 |
| --- | --- | --- |
| 公共入口 | 先打开，再编译，再写出 | 保持字节进、字节出。一次调用内部仍是打开、修改、保存 |
| 一次打开 | 打开后的几何来自快照，后面不再扫注释 | 已有。`StampAndEncrypt` 在一次 `open_pdf` 里先写 `/Info` 再加密。`split_pdfs` 打开源文件一次。其余入口各自打开 |
| 外部字节 | ARC-3。字体在 `Font::new` 解析一次 | PDF 字节只经过 `open_pdf` 进入 `lopdf::Document`。开 `thumbnail` 时 hayro 另外解析同一份字节，见下文不采用的第五条。本次不把 `Pipeline` 的多步收成一次打开 |
| 模块 | 一种知识一个模块 | `open_pdf` 与 `save_pdf` 离开 `metadata.rs`。写 `/Info` 留在 `metadata`。合并、拆分、加密、缩略图不再为了打开文件依赖 `metadata`（ARC-2 的模块图见 §4） |
| 错误 | 种类与位置分开，字段私有 | 本次不动。`Error.kind`、`Error::new`、`From<ErrorKind>`、`PartialEq` 保持现状，与 `pdfcraft-post-spec.md` §5 一致 |
| 成功字节 | 只挪阶段时与上一版相同 | 本次没有阶段可挪。成功路径字节与 `f8b79ef` 相同。加密路径每次随机生成文件密钥与 `/ID`，本来就不逐字节可比 |

不采用的方向，以及原因：

- **为 post 增加可复用的已解析文档，或 `open`、`new`、`to_pdf` 三段会话。** 没有跨调用复用的模板。会话类型多一层，删不掉 `load_mem`。
- **用一个新类型包住 `lopdf::Document`，让合并、拆分、加密都不再写 lopdf。** 这些模块的工作就是改文档对象。包一层只是把同一组方法再写一遍。
- **用只证明「已过 `open_pdf`」的 newtype 承载打开后的文档。** 它不重写方法，能让 `encrypt_document` 的 `is_encrypted` 防御检查改由类型保证。不做的原因是生产者只有 `open_pdf` 一个，调用点全在本 crate，类型证明的收益小于多一层的成本。
- **让 `Pipeline` 共享一次打开、一次保存。** 步与步之间现在会整份重写。合成一次打开会改变输出字节。`StampAndEncrypt` 已经是「先 metadata 再 encrypt」的一次保存。
- **把缩略图的 lopdf 测量和 hayro 渲染合成一次解析。** lopdf 量第 1 页的尺寸。hayro 把同一份字节画成 JPEG。收成一次解析会改这张 JPEG 从哪一个库来。
- **按 forms 0.3 收紧 `Error`。** 行为合同要求 `From<ErrorKind>`、按 kind 判等，以及若干 `Display` 句子。forms 重塑规格写明只改 `pdfcraft-forms`。
- **把 `Encrypt.config` 改成必填，或合成两组口令。** 这是公共签名。行为合同写的是缺配置失败，以及调用方口令覆盖配置里的口令。
- **在 `open_pdf` 之前先查日期和配置。** 同一次输入返回的 `ErrorKind` 会变。语义变更先改提案书，见 §5。
- **让 `merge_pdfs`、`split_pdfs` 或 `first_page_jpeg` 进入 `Step`。** 拆分返回多份 PDF，缩略图返回 JPEG，合并不止一份输入。`Pipeline` 只跑一条 PDF 字节。
- **顺手做 Watermark 或输入限额。** 提案书仍把这两项延后。

## 3. 本次决定

| # | 决定 |
| --- | --- |
| D1 | 新增私有模块 `src/pdf.rs`。把 `open_pdf` 与 `save_pdf` 从 `metadata.rs` 移入，同时删去 `open_pdf` 不起作用的 `encrypted` 参数。`write_docinfo`、`check_pdf_date` 与 `/Info` 写入留在 `metadata`。crate 根不导出这个模块 |
| D2 | `lib.rs` 的处理器表补上 `split_pdfs` 与 `first_page_jpeg`。表里已有的五行保留。crate 文档开头一段同样补上拆分与首页缩略图，与 README 一致。该段「输入输出都是 PDF 字节」一句改为输入是 PDF 字节，输出是一份或多份 PDF 字节，或缩略图的 JPEG 字节 |
| D3 | `merge_pdfs` 的 rustdoc 写明：一份输入原样返回，不调用 `open_pdf`，因此不报 `NotAPdf` 或 `Encrypted`。这两种种类只出现在两份及以上。行为仍是 `pdfcraft-post-spec.md` §3.5 |
| D4 | 公共签名、`ErrorKind` 变体、`Display` 文本、非加密成功路径的字节与 `f8b79ef` 相同。加密与 `StampAndEncrypt` 每次随机生成文件密钥，整份密文都会变，不按字节比较。不升版本。不留别名（VER-3 在本次没有被替换的入口） |

`open_pdf` 的签名改为 `open_pdf(pdf: &[u8]) -> Result<Document, Error>`。`f8b79ef` 的第二个参数 `encrypted: ErrorKind` 在全部八个调用点都是 `ErrorKind::Encrypted`，随搬家删除，加密输入固定报 `Encrypted`。这是私有签名，错误种类与字节不变。`save_pdf` 仍返回 `Result<Vec<u8>, lopdf::Error>`。每种操作在自己的调用点把保存失败映射成自己的种类。metadata 的保存失败仍是 `MetadataFailed`。加密与 `StampAndEncrypt` 的保存失败仍是 `EncryptFailed`。合并的保存失败仍是 `MergeFailed`。拆分的保存失败仍是 `SplitFailed`。

`encrypt_document` 留在 `encryption.rs`。页拷贝留在 `pages.rs`。这些文件继续使用 `lopdf`。

## 4. 模块终态

依赖沿 DAG 向下。post 的模块图没有脚本检查（工程规范 §16），评审以下表为准。

```text
error
pdf        → error, lopdf
pages      → error, lopdf
metadata   → error, pdf, lopdf
encryption → error, pdf, lopdf
combo      → error, pdf, metadata, encryption
merge      → error, pdf, pages, lopdf
split      → error, pdf, pages, lopdf
thumbnail  → error, pdf, pages, lopdf（feature 开时另有 hayro、image）
pipeline   → error, metadata, encryption, combo
lib        → 以上公共项的重导出
```

`encryption` 不依赖 `metadata`。`pipeline` 只调用三个 `apply`，不引用 `Document`。

`Pipeline` 非空时，每一步仍自己 `load_mem` 再 `save_to`。空 `Pipeline` 仍只复制输入字节。

`SetMetadata::default().apply` 仍会打开并重存。全是 `None` 的请求不是恒等。本次不改这条行为。

一份 `merge_pdfs` 的早退留在 `merge.rs`。零份仍是 `MergeEmpty`，发生在任何解析之前。

## 5. 不在本次

下面各项会改公共签名或同一次输入的 `ErrorKind`。写入提案书之前不要实现。

| # | 留到提案 | 现在的事实 |
| --- | --- | --- |
| L1 | 坏的 `D:` 日期与缺 `EncryptionConfig` 在 `open_pdf` 之前返回 | `SetMetadata::apply` 与 `Encrypt::apply` 都先 `open_pdf`。日期检查在 `write_docinfo`。`ConfigMissing` 在 `resolve_passwords` |
| L2 | 坏日期单独一种 `ErrorKind`，不再占用 `MetadataFailed` | `check_pdf_date` 与写不进 `/Info` 都是 `MetadataFailed` |
| L3 | `Encrypt.config` 必填。两组口令是否合成，另写一条 | `config` 是 `Option<EncryptionConfig>`。`Encrypt` 上的口令覆盖配置里的口令 |
| L4 | `Error` 改为私有字段与 `kind()`。删除公共 `Error::new`、`From<ErrorKind>` 与 `PartialEq` | 行为合同 §5 要求现在的构造、`From` 和按 kind 判等。forms 0.3 的私有字段不套用 |
| L5 | 调用方能否清空一个已有 `/Info` 字段 | `None` 表示不改。规格没有清空的写法 |
| L6 | `Pipeline` 是否在类型上限制 `Encrypt` 与 `Metadata` 的先后 | 加密之后再写 metadata，下一步 `open_pdf` 返回 `Encrypted` |
| L7 | 合并是否保留注释、AcroForm 与链接 | 行为合同只写了大纲、附件和结构树 |

单份 `merge_pdfs` 不过 `open_pdf` 已由行为合同 §3.5 写明，不是 L 表里的待定项。D3 不改行为，只让 rustdoc 与该条一致，并用测试钉住（见 §6）。

## 6. 验收

- `cargo test -p pdfcraft-post` 与现有示例的结果不变。
- CI 不跑 `thumbnail` feature，本地另跑 `cargo clippy -p pdfcraft-post --all-features --all-targets -- -D warnings` 与 `cargo test -p pdfcraft-post --features thumbnail`。
- 不新增只为模块路径服务的测试。加密输入、非 PDF、空合并、单份合法 PDF 合并、空管道仍由现有测试覆盖。
- D3 写进 rustdoc 的行为补两条测试，放在 `tests/merge.rs`。单份非 PDF 字节原样返回。单份加密 PDF 原样返回，不报 `Encrypted`。
- 公共 `rustdoc` 里搜不到 `pdf` 模块。
- `metadata.rs` 不再定义 `open_pdf` 或 `save_pdf`。
- 成功路径字节不变由构造保证。`src/` 的 diff 只含函数位置、`use` 路径、`open_pdf` 删参与 rustdoc。本次不记录新的黄金文件。
- 需要实测时，用 `examples/merge.rs` 与 `examples/split.rs` 在搬家前后各跑一次再 `cmp`。metadata 没有示例，不另写驱动，靠构造保证。加密与 `StampAndEncrypt` 不比对字节。
