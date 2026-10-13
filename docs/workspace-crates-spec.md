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
    quire-forms/                    # 读已有 AcroForm 并 flatten（§10；基础填写已实现）
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
| `quire-forms` | 读第三方 PDF 字节，填写已有 AcroForm，写出已 flatten 的 PDF | 提案书 §10 已确认。`list_fields` 与 ASCII `fill_and_flatten` 已实现。中文填写、OTF、对齐覆盖、`line_below`、`absolute_lines` 未做。`publish = false` |
| `quire-orgchart` | 股权架构图。`FontBytes` + skrifa `0.48.0` 测量，krilla `0.8.2` 绘制 | 已实现 `validate_request`、`document_title`、`generate_pdf`。不依赖 `quire-pdf` |

## 依赖硬规则

```text
quire-forms     ──X──→  quire-pdf
quire-pdf       ──X──→  quire-forms
quire-orgchart  ──X──→  quire-forms
quire-forms     ──X──→  quire-orgchart
quire-orgchart  ──X──→  quire-pdf
```

- `quire-forms` 使用 `lopdf =0.34.0` 与 `skrifa =0.48.0`。`quire-forms` 不依赖 `krilla`。`quire-forms` 不依赖 `quire-pdf`。禁止为填表把第二套 PDF 栈引进 `quire-pdf`。
- `quire-pdf` 内部模块 DAG 不变，见 [`module-dependency.md`](module-dependency.md)。不为「无环」再拆 `quire-pdf`。
- 共享类型默认不抽包；重复到不可忍受再考虑极薄 `quire-geom` 类 crate。

## Feature 边界

| 合法 | 非法（长期） |
| --- | --- |
| `quire-pdf` 内 `backend-krilla`、`test-utils` 等同一生成管线上的可选能力 | `quire-pdf` features 打开 forms / orgchart |
| 各 crate 自有 feature | 用 feature 假装独立产品 |

## 版本与发布

- 三包均可 `publish = false`，直到各自产品门槛通过。
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
- 在本规格的结构落地里预写未确认的产品 API。填表基础 API 已在 `quire-forms`。架构图行为见 `quire-orgchart-spec.md`。对齐覆盖、`line_below`、`absolute_lines`、中文填写仍未做（以提案书 §10 与 forms 实现为准）。
- 把 `lopdf` / `pdf-writer` 等引进 `quire-pdf`。
- 让 `quire-orgchart` 依赖 `quire-pdf`，或把架构图做成 `quire-pdf` 的 feature。

## 落地记录

| 项 | 状态 |
| --- | --- |
| 根 `[workspace]` + `crates/*` | 已落地 |
| `quire-pdf` 迁入 `crates/quire-pdf` | 已落地 |
| `quire-forms` 产品 API 与依赖 | 已确认，见提案书 §10。依赖为 `lopdf =0.34.0` 与 `skrifa =0.48.0`。不引入 `krilla`。与 `quire-pdf` 无依赖边。基础 `list_fields` 与 `fill_and_flatten` 已落地 |
| `quire-orgchart` 协议、测量、`generate_pdf` | 已落地。行为见 `quire-orgchart-spec.md` |
