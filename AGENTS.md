# AGENTS.md

给在本仓库里改代码的人类和 coding agent 用。短、可执行；细节去查提案书和设计稿，不要在这里复述长表。

## 文档优先级

1. [`docs/提案书.md`](docs/提案书.md) — **已确认条目的唯一权威**。
2. [`docs/quire-design.md`](docs/quire-design.md) — 设计草稿。其中**未经提案书确认**的决定先不算数（提案书抬头写明）。
3. 本文件 — 工程约定与禁止项。

冲突时：提案书 > 本文件 > 设计稿未确认部分。

提案书 §1–§9 主线已确认（见提案书文末总表）。实现按层推进；预置边距枚举、分隔线最终方法名等小糖可后补。搁置项（章跟流走、某页关 header/footer）不要提前做进公共 API。

仍须遵守：未确认功能不「顺便」做；`publish = false` 直到限额与信件夹具通过。

## 现阶段姿态（无生产包袱）

当前仍是 **0.x 初期**：尚未用于生产，**不背历史包袱**。宗旨是做成扎实的开源库与企业可批量生产的底座，而不是在半成品上打补丁凑合。

- **主题内不将就。** 已确认要改的设计/API/错误面，优先选长期正确的形状；不要因为「改动小」「兼容旧写法」「少动测试」而留下已知歪的公共面或内部结构。
- **「最小 diff」指克制范围，不是降低标准。** 一 PR 一件事、不顺手改无关模块、不把未确认功能塞进当前 PR——这些仍成立。但在该主题 internally，允许为夯实基础做必要的较大重构。
- **公开 API 与错误诊断** 按开源 / 批跑生产预期设计；0.x 可以 breaking，宁可现在拆干净，不要把「以后再补上下文」当默认答案。

## 这是什么库

`quire`：调用方用 **Rust DSL** 描述文档；库做测量、断行、分页、表格、页眉页脚；经自有 `Canvas` 画出矢量 PDF。唯一后端是私有 **krilla** 适配器（相当于 pdfmake 下面的 pdfkit）。

已钉死（见提案书「你已经定过的」）：

- 不做 FOP / XSL-FO；股权架构图不做。
- 底座 krilla；不改用 pdf-writer / lopdf / printpdf。
- 行宽用 **skrifa** 量，不靠 krilla 文字绘制反推。
- 上层只依赖自有画布；`krilla` 类型关在适配器里，公共 API 不重导出 krilla。
- 排版坐标：**左上原点，y 向下**；与 krilla 0.8.2 Surface 一致，适配器里对几何做恒等映射，不预翻转。
- 版本保持 `0.x`；`publish = false` 直到限额与信件夹具通过。

调用方式（提案书 §1，已定）：闭包式 DSL；正文流式；`behind` / `front` 等定位绘制在**每一页绘制时**跑。`Flowable` 只在库内部，不向调用方暴露。

## 概念三分（对照 pdfmake，谈范围时用）

这三件事不要混成一种「节点」：

| 层 | 是什么 | 例子 |
| --- | --- | --- |
| **正文节点** | 出现在内容流里，可嵌进栏 / 单元格 / 列表项 | text、stack、columns、table、ul、ol、image、… |
| **挂在节点上的属性** | 不是另一种节点 | 样式、link、pageBreak、unbreakable、absolutePosition、… |
| **文档级设置** | 不进内容数组 | header/footer、watermark、pageSize、styles、images、info、加密 |

和已定用法对齐的方向：流式正文用 text / stack / columns / table / ul / ol / image；水印用文档级 watermark（或 behind）；电子章用 **absolutePosition 这类属性**（或 front 逃逸），不是新节点类型。`canvas` 是流里的一块矢量，和页面固定坐标粘贴不是一回事。

**第一版具体封装哪些元素，以提案书 §2 取舍为准；未取舍前不要在 `document` 里发明 API。**

## 目录与模块

```text
quire/
  AGENTS.md CONTRIBUTING.md README.md CHANGELOG.md
  Cargo.toml rustfmt.toml clippy.toml
  LICENSE-MIT LICENSE-APACHE
  docs/                 提案与设计（先定产品，再写接口）
  src/
    lib.rs              只挂模块；公共重导出随实现 PR 增加
    error.rs geom.rs
    # limits.rs returns with the limits PR (not an empty pub shell)
    canvas.rs           Canvas / RecordingCanvas；无 krilla
    document.rs         公共 DSL（§2 定后再写）
    fonts.rs
    text/               cluster / measure / break_class / line_break
    layout/             frame flow nesting paragraph columns table list decorate paginate dump
    backend/            仅 feature backend-krilla；唯一可 use krilla 处
  tests/                集成测试；fixtures/ 放字体与样张
  examples/             可运行示例（API 稳定后再填）
  .github/workflows/    CI
```

### 依赖方向（禁止环）

**不允许循环依赖**（含同 crate 模块之间）。边只许沿 DAG 向下；单 crate 能编译出环也不接受。目标图与拆环刀序见 [`docs/module-dependency.md`](docs/module-dependency.md)。

```text
geom
error       → geom；不依赖 fonts / layout / document / backend
fonts       → error, geom
canvas      → error, geom
text        → fonts, error, geom
layout      → text, fonts, canvas, error, geom
document    → layout, canvas, fonts, error, geom     # 禁止 → backend
backend     → canvas, fonts, error, geom, krilla     # 禁止 → layout / document
```

| 区域 | 可以 | 不可以 |
| --- | --- | --- |
| `layout/`、`text/`、`fonts/`、`document/` | 自有几何、Canvas、skrifa（测量 PR 起） | `use krilla::…`；`document` → `backend` |
| `backend/` | krilla、把 Canvas 指令译成 PDF | 被 `layout` / `document` 反向依赖 |
| `error` | 自有 kind / Display | 依赖 `fonts::FontId` 等上层类型 |
| `lib.rs` 重导出 | 公共 DSL / geom / error / Canvas | 任何 krilla 类型 |
| 测试 | 排版用 `RecordingCanvas` 断言坐标 | 用测试依赖反转生产 DAG；把 krilla 压缩字节当排版断言 |

## 代码与贡献规范

按开源 Rust 库标准要求自己。权威优先级（与「文档优先级」正交：那边管产品，这边管怎么写代码）：

1. [Rust Style Guide](https://doc.rust-lang.org/style-guide/) + 本仓库 `rustfmt.toml` / `clippy.toml` / `Cargo.toml` `[lints]`
2. [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) — 公共 API 命名、文档、错误、trait；DSL 落地时按 [checklist](https://rust-lang.github.io/api-guidelines/checklist.html) 过一遍
3. 工程门禁气质对齐 [krilla](https://github.com/LaurenzV/krilla)（`-Dwarnings`、fmt、clippy、`cargo doc`、MSRV）；贡献粒度对齐 Typst（单主题、少改接口、新公共类型有文档）
4. 评审纪律对齐 [Google Eng Practices](https://google.github.io/eng-practices/)：审行为与可测性，不为口味重写；改动范围保持单主题（见上「现阶段姿态」）

不照搬：krilla 的多 viewer 视觉回归与 `^` 依赖；大厂内部语言手册与巨型 monorepo 流程。

### 门禁（每个可合并改动）

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --features test-utils -- -D warnings
cargo test --features test-utils
cargo doc --no-deps
# CI 另设 RUSTFLAGS=-D warnings 与 RUSTDOCFLAGS=-D warnings
# `--all-targets` / 集成测试需要 `test-utils`（根导出 `pt_near`）。
#
# feature 组合变多后再强制：
#   cargo hack check --each-feature
```

### 语言与依赖

- **Edition** `2021`，**MSRV** `1.92`（跟 krilla 0.8.2）；CI 用同一 toolchain。
- **禁止** `unsafe`（`Cargo.toml` / `#![forbid(unsafe_code)]`）。
- 依赖**按需引入、精确钉版本**（`=x.y.z`）。计划中的钉：`krilla =0.8.2`（`default-features = false`，开 `raster-images`）、`skrifa =0.47.0`、`unicode-segmentation =1.13.3`。未用到的依赖不要写进清单。
- 不启用 krilla 的 `simple-text` / `rayon`；禁止 `Surface::draw_text`。
- 测量与绘制同一条字形管线：charmap → advance → 断行 → 同一批 glyph id 再画。
- 缺字策略以提案书 §4 为准（设计稿默认「缺字即错」；未确认前不要实现回退字体）。
- 改 `krilla` / `skrifa` 钉版本必须**单独 PR**，并重跑坐标与字形相关测试。

### 风格、文档、错误

- rustfmt / clippy 即风格；禁止大段 `#[allow(...)]`，必要时单点 + 一行理由。
- `missing_docs`：当前 `warn`。公共 DSL / 稳定对外 API 落地后升为 `deny`。
- 公共项写 `///`（模块级说明 + 类型/方法）；`Errors` / `Panics` 按 API Guidelines。`pub(crate)` 与私有代码短注释即可。
- 库代码用 `Result` + 本库 `Error`；公共路径不 `unwrap` / `expect`（测试与已证明的私有不变量除外）。

### PR 纪律

- **一 PR 一件事**：不顺手重构无关模块；不把未确认功能「顺便」做进当前 PR。主题内为夯实基础的必要大改可以做（见「现阶段姿态」）。
- 用户可见行为写入 `CHANGELOG.md`（有公共行为起记；`publish = false` 期间也记）。
- 外部贡献说明见 `CONTRIBUTING.md`（英文短版，细节仍以本文件为准）。

## Agent 工作方式

1. **先读**提案书里与当前任务相关的已确认条；再读设计稿里仍有效的技术核实（krilla/skrifa API 事实）。
2. **主线已确认后**：按层实现；DSL / 元素签名以提案书为准。预置与搁置项见提案书总表，不提前做。
3. 实现时按层推进：geom/error（含长度类型）→ canvas →（backend）→ fonts/measure → line break → flow/paginate → table/decorate → DSL 接线。
4. 排版测试优先断言 `RecordingCanvas` 记录；`to_pdf` 冒烟只断言 `%PDF` 之类，不断言压缩内容流细节（内容流数字实验仅属于 backend 适配器测试）。
5. 对话过长、上下文明显膨胀时，建议用户开新对话，并指向本文件与提案书。
6. 改代码前读周边文件；提交前跑「门禁」一节命令；不擅自 `git commit` / push，除非用户明确要求。

## 明确不要做

- 第二 PDF 后端；生产代码依赖 pdf-writer / lopdf / printpdf。
- 完整 UAX #14、bidi、阿文/印度系 shaping、竖排、kerning、连字。
- 向调用方暴露 `Flowable` trait。
- 在提案书定为不做/延后的能力上抢跑：TOC / QR / SVG / 流内 canvas / 加密 / 标签 PDF 等；单元格合并等**已确认要做的**按对应层实现。
- 用 JSON/serde 当主接口（设计稿旧表述已作废；serde 若将来做，也只是可选，不是主路径）。
- 并行开页或并行画页。
- 删除 `publish = false`，除非限额 PR 与信件夹具已通过。
- 为「像大厂」堆流程：不建 viewer 视觉回归农场；不放宽成 `^` 依赖。
