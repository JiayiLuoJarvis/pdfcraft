# pdfcraft-post 规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-14 |
| 状态 | **已确认。** 提案书 §11（2026-10-14）。本文件是产品合同 reference；实现以提案书确认条目与本文为准 |
| 相关 | [`workspace-crates-spec.md`](workspace-crates-spec.md)；[`提案书.md`](提案书.md) §11；执行计划 `/home/abc/.cursor/projects/mnt-data-demo-quire/docs/pdfcraft-post-port-plan.md` |
| 意图来源 | `/mnt/data/dev/hcsw-pdfcraft/backend/packages/pdfcraft-post` 的行为（bytes → bytes）。按**独立开源库**重做 API，不逐文件翻译，不迁入 Web/finalize 编排 |

本文是 **reference**。实现细节以提案书确认条目与本规格为准。行为可对照 Python 包与其测试，但公共面按 Rust 库习惯设计。

## 1. 产品一句话

`pdfcraft-post` 对**已有 PDF 字节**做后处理：写 `/Info`、加密、合并多份 PDF，并可把步骤收成 `Pipeline`。输入输出都是内存中的 PDF 字节。

它不是 `pdfcraft-pdf` 的排版管线，也不服务某一 Web 项目。

## 2. 范围

### 2.1 做（本栈 / v1）

- `SetMetadata`。写或覆盖 PDF `/Info` 字段（见 §3.2）。
- `Encrypt`。AES-256（Standard Security Handler revision 6 / lopdf `EncryptionVersion::V5`）。权限配置与预设见 §3.3。
- `StampAndEncrypt`。同一次打开中先写 `/Info` 再加密（相对 `SetMetadata` 再 `Encrypt` 少一轮完整保存）。
- `merge_pdfs`。按顺序拼接页面；见 §3.5。
- `split_pdfs`。把一份 PDF 按页拆成多个单页 PDF；见 §3.7。
- 首页缩略图（可选 `thumbnail` feature）。把第 1 页渲染为 JPEG 字节；见 §3.8。缩略图渲染在 PDF 之外，不画进 PDF；与延后的 Watermark（往 PDF 里叠字）不是一回事。
- `Pipeline`。按顺序对同一串字节应用多个后处理步骤。
- 错误面与 `pdfcraft-forms` / `pdfcraft-pdf` 同骨架（`Error` + `ErrorKind`，见 §5）。
- 版本 `0.1.0`，可发布（与 forms / orgchart 同姿态）。

### 2.2 不做（本栈明确裁掉或延后）

- **Watermark**（延后）。Python 有斜向叠字；本栈不实现。
- **输入限额**（延后）。不做 `configure_limits` / `max_bytes` / `max_pages`，也不做进程全局限额。
- HTTP / `to_dict` / `AppError` 映射。
- hcsw `finalize_pdf` 编排（自动生成 owner 密码、`exported_at` 写入 keywords、落库、脱敏 payload）。
- 库内「未传密码则自动 `token_urlsafe`」；密码由调用方显式提供。
- 依赖或反向依赖 `pdfcraft-pdf`、`pdfcraft-forms`、`pdfcraft-orgchart`。
- 把加密、合并、后处理写 Info 做进 `pdfcraft-pdf` 的 feature。
- **禁止修改** `/mnt/data/dev/hcsw-pdfcraft`（或任何 hcsw 树）里的任何文件。只读对照行为允许。

### 2.3 与其它 crate 的边界

| | `pdfcraft-pdf` | `pdfcraft-forms` | `pdfcraft-post` |
| --- | --- | --- | --- |
| 输入 | `DocumentSpec` | 已有 PDF + 字段值 | 已有 PDF 字节（合并时为多份） |
| `/Info` | 生成时写 `DocumentInfo`（Creator 固定 `pdfcraft`，不开放 Producer） | 不写加密；可保留/处理 Info 视实现而定 | **改已有** PDF 的 `/Info`；Creator / Producer **可写** |
| 加密 | 不做 | 拒绝加密输入；不写加密 | **写入**加密 |
| 水印 | 文档级布局 watermark（若日后做） | 无 | 后处理叠字（本栈延后） |
| PDF 栈 | krilla（backend） | lopdf + skrifa | lopdf（与 forms 同钉版本） |

## 3. 公共 API

命名以 Rust 习惯为准。下列是讨论已定的形状。提案书确认后若需微调可见性或模块路径，不改变语义即可。

### 3.1 共同约定

- 入参与出参均为 PDF **字节**（`&[u8]` → `Vec<u8>`），不接受路径、不访问网络。
- 各处理器提供 `apply`（或等价关联函数）。`Pipeline` 按顺序调用。
- 不向公共 API 重导出 `lopdf` 类型。
- 依赖：workspace `lopdf =0.45.0`，`default-features = false`。不引入 `krilla`（本栈无水印）。

### 3.2 `SetMetadata`

可选字段（`None` 表示不改动已有值）：

| 字段 | 含义 |
| --- | --- |
| `title` | `/Title` |
| `author` | `/Author` |
| `subject` | `/Subject` |
| `keywords` | 写入时用逗号拼接为 `/Keywords` |
| `creator` | `/Creator`（**允许**调用方设置） |
| `producer` | `/Producer`（**允许**调用方设置） |
| `creation_date` | `/CreationDate`，PDF `D:` 日期语法 |
| `modification_date` | `/ModDate` |

与 `DocumentSpec.info` 的差异必须在文档中写明：排版生成锁 Creator/Producer；后处理改已有文件时开放。

### 3.3 `Encrypt`

- 必须提供 `EncryptionConfig`（或等价）；缺省配置失败。
- 预设（无密码，密码由调用方另传）：
  - `READ_ONLY`
  - `READ_ONLY_PRINTABLE`（可打印，含高质量打印；其余编辑/复制等关）
  - `FULL_PERMISSIONS`
- 权限标志对齐 Python / PDF 语义：print、high-quality print、copy、edit、annotate、fill forms、accessibility、assemble。预设里 Python 默认 `allow_accessibility=True` 的，Rust 侧也要打开对应位。
- 算法：AES-256，R=6（与 pikepdf `R=6` / lopdf V5 互通）。
- 调用方可覆盖 `owner_password` / `user_password`。空 user 密码打开是合法场景（生产常用「谁都能开、owner 管权限」）。
- 库**不**在内部替调用方随机生成 owner 密码；需要随机时由调用方生成再传入。

行为验收可对照 Python `tests/test_encryption.py`：魔法头 `%PDF`、空 user 可开、权限位、密码覆盖、非法字节失败。

### 3.4 `StampAndEncrypt`

持有一份 `SetMetadata` 与一份 `Encrypt`，在**单次**文档打开/保存路径中先写 Info 再加密。

语义上等价于先 metadata 再 encrypt；实现上应避免两次完整 serialize。公共 API 仍保留分步 `Pipeline`，本类型是便捷一等公民，不是隐藏实现细节。

### 3.5 `merge_pdfs`

```text
merge_pdfs(parts: &[Vec<u8>] | impl IntoIterator, preserve_outlines: bool) -> Result<Vec<u8>, Error>
```

（具体签名实现时按 Rust 习惯微调。）

- `parts` 为空 → 失败。
- 仅一份 → 返回原字节（不强制 round-trip）。
- 多份 → 按列表顺序拼接页面。
- `preserve_outlines == false`（默认）：不保留大纲。
- `preserve_outlines == true`：尽量保留**第一份**的大纲；后续份的大纲不合并。
- 不保留附件、结构树（与 Python 一致）。

### 3.6 `Pipeline`

有序步骤列表。空 pipeline 原样返回输入字节。步骤为已实现的后处理器（metadata / encrypt / stamp-and-encrypt 等）；`merge_pdfs` 因多输入，通常在 pipeline 外先合并再 `run`。

### 3.7 `split_pdfs`

```text
split_pdfs(pdf: &[u8]) -> Result<Vec<Vec<u8>>, Error>
```

（具体签名实现时按 Rust 习惯微调。）

- 无页面 → `Ok(vec![])`（输入是一份文档，没有空列表可拒绝；与 `MergeEmpty` 不同）。
- N 页 → N 个输出，输出 `i` 放第 `i` 页，阅读顺序不变。
- 每个输出是自包含单页 PDF：自己的 `Pages` 父节点（`Count` 1）、物化后的 `MediaBox` / `Resources` / `CropBox` / `Rotate`、页级 `Annots`。文档级大纲、附件、结构树不保留（与 `merge_pdfs` 不保留大纲同规则）。
- 拆分后喂回 `merge_pdfs` 可按序拼回（往返；`tests/split.rs` 的 `split_then_merge_round_trip` 覆盖）。
- 加密输入 → `Encrypted`（含空 user 密码能被自动解密的文件，一律拒绝，不重存自动解密后的字节）。非 PDF → `NotAPdf`。页复制失败 → `SplitFailed`（显示 `split_pdfs failed`）。

`split` 不设输入限额；输入限额仍延后（见 §2.2）。

### 3.8 首页缩略图（可选 `thumbnail` feature）

```text
first_page_jpeg(pdf: &[u8], opts: &ThumbnailOptions) -> Result<Option<Vec<u8>>, Error>
```

（具体签名实现时按 Rust 习惯微调。）

- 只渲染第 1 页；多页输入也只出一张 JPEG。Alpha 压成白色，输出永不带透明。
- 开启方式：`--features thumbnail`（`hayro` + `image`，纯 Rust 实现，无系统依赖，无需 Docker 服务）。未开 feature 时，能渲染的输入一律报 `ThumbnailFailed`；但输入门先跑，所以空输入仍得 `Ok(None)`，非 PDF 仍是 `NotAPdf`，加密输入仍是 `Encrypted`。
- `ThumbnailOptions`（`non_exhaustive`，从 `Default` 起改字段）：
  - `max_longest_side`（默认 320）：最长边像素上限；已更小的页保持原生尺寸，绝不放大；`0` 恒失败（`ThumbnailFailed`）。
  - `quality`（默认 75）：钳制到 `1..=100`；`0` 与超 100 不报错，按 `1` / `100` 处理。
- 无页面 → `Ok(None)`，不是错误。
- 非 PDF → `NotAPdf`；加密输入 → `Encrypted`（含空 user 密码能被自动解密的文件）。其余失败（页尺寸不可解、未开 feature、渲染或编码失败）→ `ThumbnailFailed`（显示 `thumbnail failed`）。
- 与 Watermark 的区别：缩略图渲染在 PDF 之外（得到 JPEG 字节），Watermark 是往 PDF 里叠字（本栈延后）。

## 4. 依赖与仓库规则

```text
pdfcraft-post  ──X──→  pdfcraft-pdf
pdfcraft-post  ──X──→  pdfcraft-forms
pdfcraft-post  ──X──→  pdfcraft-orgchart
（反向同样禁止）
```

- 钉 `lopdf =0.45.0`（与 forms 相同，根 `[workspace.dependencies]`）。
- `forbid(unsafe_code)`；edition / MSRV 跟 workspace。
- 门禁扩展：`clippy` / `test` / `doc` 含 `-p pdfcraft-post`。
- **硬规则：** 任何实现或 PR **不得修改** hcsw 工程。

## 5. 错误

对齐 `pdfcraft-forms`（及 `pdfcraft-pdf` 的 `Error` / `ErrorKind` 骨架），而不是 Python 的 `to_dict`：

- `Error { kind: ErrorKind, … }`，可选 `source` 挂依赖错误。
- `From<ErrorKind>`；`Display` 走 kind；相等性比 kind（及约定字段）。
- kind 覆盖本栈失败（至少）：非 PDF（`NotAPdf`，显示 `not a PDF`）、加密输入（`Encrypted`，显示 `encrypted pdf: password required`）、缺加密配置（`ConfigMissing`）、加密失败（`EncryptFailed`）、元数据失败（`MetadataFailed`）、合并空输入（`MergeEmpty`）、合并失败（`MergeFailed`）、拆分失败（`SplitFailed`，显示 `split_pdfs failed`）、缩略图失败（`ThumbnailFailed`，显示 `thumbnail failed`）等。具体变体名实现时按 forms 风格命名，可用 `#[non_exhaustive]`。显示字符串以 `src/error.rs` 的 `Display` 实现为准。

不提供 HTTP 序列化辅助。

## 6. 实现顺序（建议）

执行计划见 agent store 的 `pdfcraft-post-port-plan.md`。摘要：

1. 提案书 + workspace 规格 + AGENTS 确认本 crate（docs）
2. crate 壳 + `Error` + `Pipeline`
3. `SetMetadata`
4. `Encrypt`
5. `StampAndEncrypt`
6. `merge_pdfs`
7. 文档、example、CI、发布姿态收尾

## 7. 行为对照（只读）

| Python | 路径（只读） |
| --- | --- |
| 包 | `/mnt/data/dev/hcsw-pdfcraft/backend/packages/pdfcraft-post/` |
| 加密 | `.../encryption.py`，`tests/test_encryption.py` |
| 元数据 | `.../metadata.py` |
| 组合 | `.../combo.py`，`tests/test_combo.py` |
| 合并 | `.../merge.py` |
| 生产用法（勿迁） | `/mnt/data/dev/hcsw-pdfcraft/backend/core/documents/finalize.py` |

Encrypt 与 lopdf AES-256 互通已有一次性原型（2026-10-14）：pikepdf 对 lopdf V5 输出报 `R=6` / `aesv3`，空 user 可打开。

## 8. 确认清单（写入提案书时勾）

- [x] 新建 crate `pdfcraft-post`，职责与本规格 §1–§2 一致。
- [x] v1 含 SetMetadata、Encrypt、StampAndEncrypt、merge_pdfs、Pipeline。
- [x] v1 不含 Watermark、输入限额、HTTP、finalize 编排。
- [x] Creator/Producer 在 SetMetadata 可写；与 DocumentSpec.info 差异写明。
- [x] 与三 crate 无依赖边；用 lopdf 0.45.0。
- [x] 错误为 Error + ErrorKind。
- [x] `0.1.0` 可发布。
- [x] 文档写明禁止改动 hcsw；实现只在本仓库。
