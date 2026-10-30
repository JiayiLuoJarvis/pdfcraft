# Workspace 多 crate 规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-14 |
| 状态 | **结构已落地**。`pdfcraft-forms` 与 `pdfcraft-orgchart` 均有产品实现。`pdfcraft-post` 产品范围已在提案书 §11 确认；crate 实现按层推进 |
| 相关 | [`提案书.md`](提案书.md) §9–§11；[`pdfcraft-post-spec.md`](pdfcraft-post-spec.md)；[`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md)；[`module-dependency.md`](module-dependency.md)；[`AGENTS.md`](../AGENTS.md) |

## 目标

单一 git 仓库，Cargo workspace 按能力拆 crate。调用方按需依赖；维护者不必维护多个项目。

不把「读第三方 PDF 填表」、「股权架构图」或「已有 PDF 后处理（加密 / 合并 / 改 Info）」塞进 `pdfcraft-pdf` 的 feature。

## 仓库布局

```text
pdfcraft/                           # workspace 根（无业务 package）
  Cargo.toml                        # [workspace] members
  Cargo.lock
  AGENTS.md README.md CHANGELOG.md …
  docs/
  crates/
    pdfcraft-pdf/                      # 现有排版库（主线）
      Cargo.toml
      src/ tests/ examples/
    pdfcraft-forms/                    # 读已有 AcroForm 并 flatten。§10 已实现。OTF 与字体子集未做
      Cargo.toml
      src/ tests/ examples/
    pdfcraft-orgchart/                 # 股权与对外投资图（独立于 pdfcraft-pdf）
      Cargo.toml
      src/ tests/ examples/
    pdfcraft-post/                     # 已有 PDF 后处理（§11）。实现按层推进
      Cargo.toml
      src/ tests/ examples/
```

## Crate 职责

| Crate | 职责 | 现状 |
| --- | --- | --- |
| `pdfcraft-pdf` | `DocumentSpec` → 排版 → Canvas → krilla 出 PDF | 主线实现。不加密、不合并第三方 PDF、不提供后处理改 `/Info` |
| `pdfcraft-forms` | 读第三方 PDF 字节，填写已有 AcroForm，写出已 flatten 的 PDF | 提案书 §10 已确认。`list_fields` 与 `fill_and_flatten` 已实现，覆盖文本、勾选和选项，成功时一律 flatten。默认对齐、按字段覆盖、`font_size_overrides`、`line_below` 和 `absolute_lines` 已实现。`FontBytes.index` 从 TTC 拆出单个面。调用方提供带 `glyf` 的字体时可以填写中文。夹具里的中文字体不入库，相关测例要设置 `PDFCRAFT_FORMS_FONT`。OTF 与字体子集未做。版本 `0.1.0`。拒绝加密输入；不写加密 |
| `pdfcraft-orgchart` | 股权与对外投资图。`FontBytes` + skrifa `0.48.0` 测量，krilla `0.8.2` 绘制 | 已实现 `validate_request`、`document_title`、`generate_pdf`。行为合同是 [`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md)。版本 `0.1.1`。不依赖 `pdfcraft-pdf` |
| `pdfcraft-post` | 已有 PDF 字节 → 后处理 → PDF 字节。`SetMetadata`、`Encrypt`、`StampAndEncrypt`、`merge_pdfs`、`Pipeline` | 提案书 §11 已确认。API 已落地。Watermark 与输入限额延后。HTTP / finalize 编排不做。版本 `0.2.0`，可发布 |

## 依赖硬规则

```text
pdfcraft-forms     ──X──→  pdfcraft-pdf
pdfcraft-pdf       ──X──→  pdfcraft-forms
pdfcraft-orgchart  ──X──→  pdfcraft-forms
pdfcraft-forms     ──X──→  pdfcraft-orgchart
pdfcraft-orgchart  ──X──→  pdfcraft-pdf
pdfcraft-post      ──X──→  pdfcraft-pdf
pdfcraft-post      ──X──→  pdfcraft-forms
pdfcraft-post      ──X──→  pdfcraft-orgchart
pdfcraft-pdf       ──X──→  pdfcraft-post
pdfcraft-forms     ──X──→  pdfcraft-post
pdfcraft-orgchart  ──X──→  pdfcraft-post
```

- `pdfcraft-forms` 使用 `lopdf =0.45.0`（`default-features = false`，无 `nom_parser`）与 `skrifa =0.48.0`。直依版本在根 `[workspace.dependencies]`。`pdfcraft-forms` 不依赖 `krilla`。`pdfcraft-forms` 不依赖 `pdfcraft-pdf`。禁止为填表把第二套 PDF 栈引进 `pdfcraft-pdf`。
- `pdfcraft-post` 使用同一份 workspace `lopdf =0.45.0`（`default-features = false`）。不依赖 `krilla`（本栈无水印）。不依赖 `pdfcraft-pdf` / `pdfcraft-forms` / `pdfcraft-orgchart`。禁止把加密、合并、后处理写 Info 做进 `pdfcraft-pdf` 的 feature。
- `pdfcraft-pdf` 内部模块 DAG 不变，见 [`module-dependency.md`](module-dependency.md)。不为「无环」再拆 `pdfcraft-pdf`。
- 共享类型默认不抽包；重复到不可忍受再考虑极薄 `pdfcraft-geom` 类 crate。

## Feature 边界

| 合法 | 非法（长期） |
| --- | --- |
| `pdfcraft-pdf` 内 `backend-krilla`、`test-utils` 等同一生成管线上的可选能力 | `pdfcraft-pdf` features 打开 forms / orgchart / post |
| 各 crate 自有 feature | 用 feature 假装独立产品 |

## 版本与发布

- `pdfcraft-pdf` 与 `pdfcraft-forms` 是 `0.1.0`，`pdfcraft-orgchart` 是 `0.1.1`，`pdfcraft-post` 是 `0.2.0`。都可以发布。
- 版本号可同号可不同；不强制 workspace 统一 version。
- crates.io 发布名保持各 crate 的 `name` 字段；口头仓库名仍可称 pdfcraft。

## 门禁

根目录执行（路径相对仓库根）：

```bash
cargo fmt --all -- --check
cargo clippy -p pdfcraft-pdf --all-targets --features test-utils -- -D warnings
cargo test -p pdfcraft-pdf --features test-utils
cargo doc -p pdfcraft-pdf --no-deps
cargo clippy -p pdfcraft-forms -p pdfcraft-orgchart --all-targets -- -D warnings
cargo test -p pdfcraft-forms
cargo doc -p pdfcraft-forms --no-deps
cargo test -p pdfcraft-orgchart
cargo doc -p pdfcraft-orgchart --no-deps
cargo clippy -p pdfcraft-post --all-targets -- -D warnings
cargo test -p pdfcraft-post
cargo doc -p pdfcraft-post --no-deps
```

CI 对 `pdfcraft-pdf` 保留原有 feature 矩阵。`pdfcraft-forms`、`pdfcraft-orgchart` 与 `pdfcraft-post` 跑测试与文档。

## 明确不做（本规格）

- 拆多个 git 仓库。
- 在本规格的结构落地里预写未确认的产品 API。`pdfcraft-forms` 的能力以提案书 §10 为准。`pdfcraft-post` 以提案书 §11 与 [`pdfcraft-post-spec.md`](pdfcraft-post-spec.md) 为准。架构图行为见 `pdfcraft-orgchart-spec.md`。
- 把 `lopdf` / `pdf-writer` 等引进 `pdfcraft-pdf`。
- 让 `pdfcraft-orgchart` 依赖 `pdfcraft-pdf`，或把架构图做成 `pdfcraft-pdf` 的 feature。
- 让 `pdfcraft-post` 依赖三兄弟任一，或把后处理做成 `pdfcraft-pdf` / `pdfcraft-forms` 的 feature。
- 修改 hcsw 工程中的任何文件。行为对照只读。

## 落地记录

| 项 | 状态 |
| --- | --- |
| 根 `[workspace]` + `crates/*` | 已落地 |
| `pdfcraft-pdf` 迁入 `crates/pdfcraft-pdf` | 已落地 |
| `pdfcraft-forms` 产品 API 与依赖 | 已确认，见提案书 §10。依赖为 `lopdf =0.45.0`（`default-features = false`）与 `skrifa =0.48.0`。不引入 `krilla`。与 `pdfcraft-pdf` 无依赖边。能力与上表「现状」相同，含对齐、`line_below`、`absolute_lines` 与 TTC 拆面。OTF 与字体子集未做 |
| `pdfcraft-orgchart` 协议、测量、`generate_pdf` | 已落地。行为见 `pdfcraft-orgchart-spec.md` |
| `pdfcraft-post` 产品范围与实现 | 已确认，见提案书 §11。crate 已落地 |
