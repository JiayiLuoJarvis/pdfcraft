# Workspace 多 crate 规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-12 |
| 状态 | **结构已落地**。`quire-forms` 与 `quire-orgchart` 均有产品实现 |
| 相关 | [`提案书.md`](提案书.md) §9 与 §10；[`quire-orgchart-spec.md`](quire-orgchart-spec.md)；[`module-dependency.md`](module-dependency.md)；[`AGENTS.md`](../AGENTS.md) |

## 目标

单一 git 仓库，Cargo workspace 按能力拆 crate。调用方按需依赖；维护者不必维护多个项目。

不把「读第三方 PDF 填表」或「股权架构图」塞进 `quire-pdf` 的 feature。

## 仓库布局

```text
quire/                              # workspace 根（无业务 package）
  Cargo.toml                        # [workspace] members
  Cargo.lock
  AGENTS.md README.md CHANGELOG.md …
  docs/
  crates/
    quire-pdf/                      # 现有排版库（主线）
      Cargo.toml
      src/ tests/ examples/
    quire-forms/                    # 读已有 AcroForm 并 flatten。§10 已实现。OTF 与字体子集未做
      Cargo.toml
      src/ tests/ examples/
    quire-orgchart/                 # 股权架构图（独立于 quire-pdf）
      Cargo.toml
      src/ tests/ examples/
```

## Crate 职责

| Crate | 职责 | 现状 |
| --- | --- | --- |
| `quire-pdf` | `DocumentSpec` → 排版 → Canvas → krilla 出 PDF | 主线实现 |
| `quire-forms` | 读第三方 PDF 字节，填写已有 AcroForm，写出已 flatten 的 PDF | 提案书 §10 已确认。`list_fields` 与 `fill_and_flatten` 已实现，覆盖文本、勾选和选项，成功时一律 flatten。默认对齐、按字段覆盖、`font_size_overrides`、`line_below` 和 `absolute_lines` 已实现。`FontBytes.index` 从 TTC 拆出单个面。调用方提供带 `glyf` 的字体时可以填写中文。夹具里的中文字体不入库，相关测例要设置 `QUIRE_FORMS_FONT`。OTF 与字体子集未做。版本 `0.1.0`。 |
| `quire-orgchart` | 股权架构图。`FontBytes` + skrifa `0.48.0` 测量，krilla `0.8.2` 绘制 | 已实现 `validate_request`、`document_title`、`generate_pdf`。不依赖 `quire-pdf` |

## 依赖硬规则

```text
quire-forms     ──X──→  quire-pdf
quire-pdf       ──X──→  quire-forms
quire-orgchart  ──X──→  quire-forms
quire-forms     ──X──→  quire-orgchart
quire-orgchart  ──X──→  quire-pdf
```

- `quire-forms` 使用 `lopdf =0.45.0`（`default-features = false`，无 `nom_parser`）与 `skrifa =0.48.0`。直依版本在根 `[workspace.dependencies]`。`quire-forms` 不依赖 `krilla`。`quire-forms` 不依赖 `quire-pdf`。禁止为填表把第二套 PDF 栈引进 `quire-pdf`。
- `quire-pdf` 内部模块 DAG 不变，见 [`module-dependency.md`](module-dependency.md)。不为「无环」再拆 `quire-pdf`。
- 共享类型默认不抽包；重复到不可忍受再考虑极薄 `quire-geom` 类 crate。

## Feature 边界

| 合法 | 非法（长期） |
| --- | --- |
| `quire-pdf` 内 `backend-krilla`、`test-utils` 等同一生成管线上的可选能力 | `quire-pdf` features 打开 forms / orgchart |
| 各 crate 自有 feature | 用 feature 假装独立产品 |

## 版本与发布

- 三包版本是 `0.1.0`，可以发布。
- 版本号可同号可不同；不强制 workspace 统一 version。
- crates.io 发布名保持各 crate 的 `name` 字段；口头仓库名仍可称 quire。

## 门禁

根目录执行（路径相对仓库根）：

```bash
cargo fmt --all -- --check
cargo clippy -p quire-pdf --all-targets --features test-utils -- -D warnings
cargo test -p quire-pdf --features test-utils
cargo doc -p quire-pdf --no-deps
cargo clippy -p quire-forms -p quire-orgchart --all-targets -- -D warnings
cargo test -p quire-forms
cargo doc -p quire-forms --no-deps
cargo test -p quire-orgchart
cargo doc -p quire-orgchart --no-deps
```

CI 对 `quire-pdf` 保留原有 feature 矩阵。`quire-forms` 与 `quire-orgchart` 跑测试与文档。

## 明确不做（本规格）

- 拆多个 git 仓库。
- 在本规格的结构落地里预写未确认的产品 API。`quire-forms` 的能力以提案书 §10 为准。架构图行为见 `quire-orgchart-spec.md`。
- 把 `lopdf` / `pdf-writer` 等引进 `quire-pdf`。
- 让 `quire-orgchart` 依赖 `quire-pdf`，或把架构图做成 `quire-pdf` 的 feature。

## 落地记录

| 项 | 状态 |
| --- | --- |
| 根 `[workspace]` + `crates/*` | 已落地 |
| `quire-pdf` 迁入 `crates/quire-pdf` | 已落地 |
| `quire-forms` 产品 API 与依赖 | 已确认，见提案书 §10。依赖为 `lopdf =0.45.0`（`default-features = false`）与 `skrifa =0.48.0`。不引入 `krilla`。与 `quire-pdf` 无依赖边。能力与上表「现状」相同，含对齐、`line_below`、`absolute_lines` 与 TTC 拆面。OTF 与字体子集未做 |
| `quire-orgchart` 协议、测量、`generate_pdf` | 已落地。行为见 `quire-orgchart-spec.md` |
