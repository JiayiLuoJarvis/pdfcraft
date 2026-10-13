# Workspace 多 crate 规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-12 |
| 状态 | **结构已落地**；`quire-forms` 仍为空壳；`quire-orgchart` 已有协议、测量与 `generate_pdf` |
| 相关 | [`提案书.md`](提案书.md) §9；[`module-dependency.md`](module-dependency.md)；[`AGENTS.md`](../AGENTS.md) |

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
    quire-forms/                    # PDF 表单填写（空壳）
      Cargo.toml
      src/lib.rs
    quire-orgchart/                 # 股权架构图（独立于 quire-pdf）
      Cargo.toml
      src/ tests/ examples/
```

## Crate 职责

| Crate | 职责 | 现状 |
| --- | --- | --- |
| `quire-pdf` | `DocumentSpec` → 排版 → Canvas → krilla 出 PDF | 主线实现 |
| `quire-forms` | 读第三方 PDF → 填 AcroForm → 写出 | 空壳；`publish = false` |
| `quire-orgchart` | 股权架构图。`FontBytes` + skrifa `0.48.0` 测量，krilla `0.8.2` 绘制 | 已实现 `validate_request`、`document_title`、`generate_pdf`。不依赖 `quire-pdf` |

## 依赖硬规则

```text
quire-forms     ──X──→  quire-pdf
quire-pdf       ──X──→  quire-forms
quire-orgchart  ──X──→  quire-forms
quire-forms     ──X──→  quire-orgchart
quire-orgchart  ──X──→  quire-pdf
```

- `quire-forms` 可引入**独立的** PDF 读写依赖；禁止为填表把第二套 PDF 栈引进 `quire-pdf`。
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
cargo check -p quire-forms
cargo clippy -p quire-orgchart --all-targets -- -D warnings
cargo test -p quire-orgchart
cargo doc -p quire-orgchart --no-deps
```

CI 对 `quire-pdf` 保留原有 feature 矩阵。`quire-forms` 仍为空壳，至少 `cargo check` / `clippy`。`quire-orgchart` 跑测试与文档。

## 明确不做（本规格）

- 拆多个 git 仓库。
- 在 `quire-forms` 空壳里预写填表 API。
- 把 `lopdf` / `pdf-writer` 等引进 `quire-pdf`。
- 让 `quire-orgchart` 依赖 `quire-pdf`，或把架构图做成 `quire-pdf` 的 feature。

## 落地记录

| 项 | 状态 |
| --- | --- |
| 根 `[workspace]` + `crates/*` | 已落地 |
| `quire-pdf` 迁入 `crates/quire-pdf` | 已落地 |
| `quire-forms` 空壳 | 已落地 |
| `quire-orgchart` 协议、测量、`generate_pdf` | 已落地。行为见 `quire-orgchart-spec.md` |
| 填表产品 API | 未开始（另开提案书条目） |
