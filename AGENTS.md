# AGENTS.md

给在本仓库里改代码的人类和 coding agent 用。短、可执行；细节去查提案书和设计稿，不要在这里复述长表。

## 文档优先级

1. [`docs/提案书.md`](docs/提案书.md) — **已确认条目的唯一权威**。
2. [`docs/pdfcraft-design.md`](docs/pdfcraft-design.md) — 设计草稿。其中**未经提案书确认**的决定先不算数（提案书抬头写明）。
3. [`docs/workspace-crates-spec.md`](docs/workspace-crates-spec.md) — 仓库 workspace / 多 crate 边界。
4. 本文件 — 工程约定与禁止项。

冲突时：提案书 > workspace 规格（包边界）> 本文件 > 设计稿未确认部分。

提案书 §1 到 §9 主线已确认（见提案书文末总表）。§10 确认 `pdfcraft-forms` 读取已有 AcroForm 并 flatten。`list_fields` 与 `fill_and_flatten` 已在 `crates/pdfcraft-forms`，含对齐、按字段字号、`line_below`、`absolute_lines`、TTC 拆面与 TrueType `glyf` 子集。OTF 仍未做。不要把填表写进 `pdfcraft-pdf`。§11 确认 `pdfcraft-post`：对已有 PDF 字节做 `SetMetadata`、`Encrypt`、`StampAndEncrypt`、`merge_pdfs`、`Pipeline`。Watermark 与输入限额延后。不要把加密、合并、后处理写 Info 写进 `pdfcraft-pdf`。`pdfcraft-pdf` 的实现按层推进。预置边距枚举等小项可后补。搁置项（章跟流走、某页关 header/footer）不要提前做进公共 API。§2.12 已确认 `LayerMark::Rotated` 可包 path / Text / Image（`LayerShape<K>`，斜向水印）；流内 canvas 与具名 images 字典为不做。

仍须遵守：未确认功能不「顺便」做。四包版本是 `0.1.0`，可以发布。实现只在本仓库；禁止修改 hcsw 工程中的任何文件（只读对照允许）。

## 现阶段姿态（无生产包袱）

当前仍是 **0.x 初期**：尚未用于生产，**不背历史包袱**。宗旨是做成扎实的开源库与企业可批量生产的底座，而不是在半成品上打补丁凑合。

- **主题内不将就。** 已确认要改的设计/API/错误面，优先选长期正确的形状；不要因为「改动小」「兼容旧写法」「少动测试」而留下已知歪的公共面或内部结构。
- **「最小 diff」指克制范围，不是降低标准。** 一 PR 一件事、不顺手改无关模块、不把未确认功能塞进当前 PR——这些仍成立。但在该主题 internally，允许为夯实基础做必要的较大重构。
- **公开 API 与错误诊断** 按开源 / 批跑生产预期设计；0.x 可以 breaking，宁可现在拆干净，不要把「以后再补上下文」当默认答案。

## 这是什么库

`pdfcraft` 的调用方用 `DocumentSpec` 描述文档。库做测量、断行、分页、表格、页眉页脚，经自有 `Canvas` 画出矢量 PDF。唯一后端是私有 **krilla** 适配器（相当于 pdfmake 下面的 pdfkit）。

已钉死（见提案书「你已经定过的」）：

- 不做 FOP / XSL-FO；股权架构图不做。
- 底座 krilla。`pdfcraft-pdf` 不改用 pdf-writer、lopdf、printpdf。`pdfcraft-forms` 与 `pdfcraft-post` 用自有的 `lopdf =0.45.0`（`default-features = false`，无 `nom_parser`）；forms 另用 `skrifa =0.48.0`，见提案书 §10 与 §11。二者都不依赖 `pdfcraft-pdf`。
- 行宽用 **skrifa** 量，不靠 krilla 文字绘制反推。
- 直依 `skrifa =0.48.0` 与 `krilla =0.8.2` 传递的 `skrifa ^0.42.0` 是两份库。2026-10-14 的解析结果是 `0.42.1`。当天 crates.io 上 krilla 最新仍是 0.8.2，升 krilla 消不掉这两份。上游 `main` 钉 `skrifa 0.44.0`，尚未发布，也不是 0.48.0。不要 fork krilla，不要改成 git 依赖。`emit` 的 `check_upem` 拒绝 units per em 不一致，夹具字形的 `TJ` 调整量锁住两边的 hmtx。
- 上层只依赖自有画布；`krilla` 类型关在适配器里，公共 API 不重导出 krilla。
- 排版坐标：**左上原点，y 向下**；与 krilla 0.8.2 Surface 一致，适配器里对几何做恒等映射，不预翻转。
- 版本保持 `0.x`。四包当前是 `0.1.0`，可以发布。

调用方式（提案书 §1，已定）：`DocumentSpec` 值，经 `layout_spec` 排版。正文流式。页眉、页脚、`behind` / `front` 在每一页绘制时由库填页码。调用方不提供闭包。`Flowable` 只在库内部，不向调用方暴露。

## 概念三分（对照 pdfmake，谈范围时用）

这三件事不要混成一种「节点」：

| 层 | 是什么 | 例子 |
| --- | --- | --- |
| **正文节点** | 出现在内容流里，可嵌进栏 / 单元格 / 列表项 | text、stack、columns、table、ul、ol、image、… |
| **挂在节点上的属性** | 不是另一种节点 | 样式、link、pageBreak、unbreakable、absolutePosition、… |
| **文档级设置** | 不进内容数组 | header/footer、watermark、pageSize、styles、images、info、加密 |

和已定用法对齐的方向：流式正文用 text / stack / columns / table / ul / ol / image；水印用文档级 watermark（或 behind）；电子章用 **absolutePosition 这类属性**（或 front 逃逸），不是新节点类型。`canvas` 是流里的一块矢量，和页面固定坐标粘贴不是一回事。

**第一版具体封装哪些元素，以提案书 §2 取舍为准。未取舍前不要在 `spec` 里发明 API。**

## 目录与模块

单仓库 Cargo workspace。包边界见 [`docs/workspace-crates-spec.md`](docs/workspace-crates-spec.md)。

```text
pdfcraft/                 # git 仓库 / workspace 根（无业务 package）
  AGENTS.md CONTRIBUTING.md README.md CHANGELOG.md
  Cargo.toml
  rustfmt.toml clippy.toml
  LICENSE-MIT LICENSE-APACHE
  docs/                   提案与设计；含 workspace-crates-spec
  crates/
    pdfcraft-pdf/            主线排版库
      Cargo.toml
      src/
        lib.rs            只挂模块；公共重导出随实现 PR 增加
        error.rs geom.rs limits.rs
        canvas.rs         Canvas / RecordingCanvas；无 krilla
        spec/             公共文档面。DocumentSpec、Resources、layout_spec
        fonts.rs
        text/             cluster / measure / break_class / line_break
        layout/           frame flow nesting column_width line_run paragraph columns table list decorate paginate
        backend/          仅 feature backend-krilla；唯一可 use krilla 处
      tests/              集成测试；fixtures/ 放字体与样张
      examples/           可运行示例
    pdfcraft-forms/          读已有 AcroForm 并 flatten（§10 已确认；基础填写已实现；禁止依赖 pdfcraft-pdf；不用 krilla）
    pdfcraft-orgchart/       股权与对外投资图。不依赖 pdfcraft-pdf。FontBytes；skrifa 0.48.0 测量；krilla 0.8.2 绘制
    pdfcraft-post/           已有 PDF 后处理（§11 已确认；实现按层推进；禁止依赖 pdf / forms / orgchart；不用 krilla）
  .github/workflows/      CI
```

### 依赖方向（禁止环）

**不允许循环依赖**（含同 crate 模块之间）。边只许沿 DAG 向下；单 crate 能编译出环也不接受。目标图与检查命令见 [`docs/module-dependency.md`](docs/module-dependency.md)。

```text
geom        → error
error                          # LimitKind 定义在此。禁止 → geom、limits、fonts、layout、spec、backend
limits      → error
fonts       → error, limits
canvas      → error, geom, fonts, limits
text        → error, fonts, geom
layout      → text, fonts, canvas, error, geom, limits
spec        → layout, canvas, fonts, error, geom, limits    # 公共文档面；禁止 → backend
backend     → canvas, fonts, error, geom, limits, krilla    # 禁止 → layout / spec
lib         → spec, backend, canvas, fonts, error, geom, limits
```

`layout_spec` 在 `spec` 里把 `DocumentSpec` 降成 layout 节点。打开 `backend-krilla` 时，`lib.rs` 的 `image_registry` 构造 `ImageTable`。`spec` 不引用 `backend`。

| 区域 | 可以 | 不可以 |
| --- | --- | --- |
| `layout/`、`text/`、`fonts/`、`spec/` | 自有几何、Canvas、skrifa（测量 PR 起） | `use krilla::…`；这些模块 → `backend` |
| `backend/` | krilla、把 Canvas 指令译成 PDF | 被 `layout` 或 `spec` 反向依赖 |
| `error` | 自有 kind / Display，含 `LimitKind` | 依赖 `limits`、`fonts::FontId` 等上层类型 |
| `lib.rs` 重导出 | 公共 `spec`、geom、error、Canvas | 任何 krilla 类型 |
| 测试 | 排版用 `RecordingCanvas` 断言坐标 | 用测试依赖反转生产 DAG；把 krilla 压缩字节当排版断言 |

上表是 `pdfcraft-pdf` 内部模块。`pdfcraft-orgchart` 在自己的 crate 里使用 krilla `0.8.2` 与 skrifa `0.48.0`，不引用 `pdfcraft-pdf`。`pdfcraft-post` 在自己的 crate 里使用 lopdf，不引用 `pdfcraft-pdf` / forms / orgchart。

## 代码与贡献规范

按开源 Rust 库标准要求自己。权威优先级（与「文档优先级」正交：那边管产品，这边管怎么写代码）：

1. [Rust Style Guide](https://doc.rust-lang.org/style-guide/) + 本仓库 `rustfmt.toml` / `clippy.toml` / `Cargo.toml` `[lints]`
2. [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) 管公共 API 的命名、文档、错误、trait。`spec` 的公共类型落地时按 [checklist](https://rust-lang.github.io/api-guidelines/checklist.html) 过一遍
3. 工程门禁气质对齐 [krilla](https://github.com/LaurenzV/krilla)（`-Dwarnings`、fmt、clippy、`cargo doc`、MSRV）；贡献粒度对齐 Typst（单主题、少改接口、新公共类型有文档）
4. 评审纪律对齐 [Google Eng Practices](https://google.github.io/eng-practices/)：审行为与可测性，不为口味重写；改动范围保持单主题（见上「现阶段姿态」）

不照搬：krilla 的多 viewer 视觉回归与 `^` 依赖；大厂内部语言手册与巨型 monorepo 流程。

### 门禁（每个可合并改动）

```bash
python3 scripts/check_pdfcraft_pdf_modules.py
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
# CI 另设 RUSTFLAGS=-D warnings 与 RUSTDOCFLAGS=-D warnings
# 集成测试用 `pdfcraft_pdf::canvas::pt_near`，不依赖 `test-utils`。
# 该 feature 只把同一函数再导出到 crate 根。
#
# feature 组合变多后再强制：
#   cargo hack check -p pdfcraft-pdf --each-feature
```

`pdfcraft-forms` 的填写测试会调用 `pdftotext`。

小步检查。功能简单、改动拆成连续小步时，不要每步全量跑上面的 `cargo test`。每步跑该包的 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo check --all-targets`。这一步新加的测试按名字跑。几步合在一起之后，以及改动要合并之前，再跑上面的完整 `cargo test`。

### 语言与依赖

- **Edition** `2021`，**MSRV** `1.92`（跟 krilla 0.8.2）；CI 用同一 toolchain。政策见 [`docs/msrv.md`](docs/msrv.md)。
- **禁止** `unsafe`（`Cargo.toml` / `#![forbid(unsafe_code)]`）。
- 依赖**按需引入、精确钉版本**（`=x.y.z`），写在根 `Cargo.toml` 的 `[workspace.dependencies]`。成员 crate 用 `workspace = true`，各自的 feature 留在使用处。当前直依是 `krilla =0.8.2`（`default-features = false`，`pdfcraft-pdf` 再开 `raster-images`）、`skrifa =0.48.0`（workspace 关闭 default。`pdfcraft-pdf` 与 `pdfcraft-forms` 打开 `std` 和 `autohint_shaping`。`pdfcraft-orgchart` 只打开 `std`）、`unicode-segmentation =1.13.3`、`png =0.18.1`、`read-fonts =0.45.0`（`default-features = false`，`pdfcraft-orgchart` 再开 `std`）、`lopdf =0.45.0`（`default-features = false`）。未用到的依赖不要写进清单。
- 不启用 krilla 的 `simple-text` / `rayon`；禁止 `Surface::draw_text`。
- 测量与绘制同一条字形管线：charmap → advance → 断行 → 同一批 glyph id 再画。
- 缺字策略以提案书 §4 为准（设计稿默认「缺字即错」；未确认前不要实现回退字体）。
- 改 `krilla` / `skrifa` 钉版本必须**单独 PR**，并重跑坐标与字形相关测试。

### 风格、文档、错误

- rustfmt / clippy 即风格；禁止大段 `#[allow(...)]`，必要时单点 + 一行理由。
- `missing_docs`：当前 `warn`。公共 `spec` 与其余稳定对外 API 落地后升为 `deny`。
- 公共项写 `///`（模块级说明 + 类型/方法）；`Errors` / `Panics` 按 API Guidelines。`pub(crate)` 与私有代码短注释即可。
- 库代码返回本 crate 的 `Result`。各 crate 不共用一个 error crate。
- `pdfcraft-pdf` 的失败是 `Error` 或 `Errors`。位置放在可选的 `Location`。依赖失败通过 `source()` 取出。
- `pdfcraft-forms` 与 `pdfcraft-post` 的失败是单条 `Error`（`Error` + `ErrorKind`），没有字段级 `Location`。依赖失败通过 `source()` 取出。
- `pdfcraft-orgchart` 的失败是 `ErrorCode` 加 `message`。调用方匹配 `ErrorCode::as_str`。`message` 只给日志。依赖失败映射到稳定码。`crates/pdfcraft-orgchart/src` 的生产路径不用 `expect` 或 `unwrap`。`#[cfg(test)]` 模块和 `tests/` 可以。

### Google Rust 风格裁决（Google-wins，用户已拍板）

优先级不变：产品语义冲突时提案书仍高于一切风格。本节只裁决「写法」冲突：
提案书（产品） > workspace 规格（包边界） > 本文件（含本节） > 设计稿未确认部分；
Google Rust 风格指南只在本文件这一层生效，不能推翻提案书已确认的 API 与行为。

1. **小包 / 扁平布局记 N/A。** 指南 chrome 专属两条（Prefer small crates、flat file layout）不适用。
   本仓库是 Cargo workspace，四包边界见 `docs/workspace-crates-spec.md`；`pdfcraft-pdf` 内按
   geom/error/limits → canvas/fonts/text → layout → spec → backend 分层开多文件模块（见上「目录与模块」）。
   偏差理由：已发布的 crates.io 包结构与模块 DAG 检查（`scripts/check_pdfcraft_pdf_modules.py`）
   要求稳定目录，不拆成单文件小包。`Name source files after the primary item` 与
   `Don't implement APIs in non-leaf modules` 仍照做。
2. **导入粒度显式化（rustfmt 选项 N/A，用评审 + grep 门禁）。** `imports_granularity` 在稳定
   1.92 上是 nightly-only（实测 `cargo fmt` 报 warning 且不生效），故 `rustfmt.toml` 不编码，
   单模块粒度靠评审与抽查门禁：生产代码零 glob（`tests/` 内 `use super::*` 保留）；trait 导入
   尽量函数局部；跨模块顶层函数用模块限定调用；类型名导入后非限定使用。
   抽查：`grep -rn 'use .*::\*' --include='*.rs' crates/*/src | grep -v 'mod test'` 为空。
   chrome 专属 `chromium::import!()` 记 N/A。
3. **expect 规则显式化。** `pdfcraft-pdf` 与 `pdfcraft-forms` 生产路径零 `.unwrap()`，
   一律用带上下文的 `.expect("...")`。`#[cfg(test)]` 模块与 `tests/` 目录可用 `unwrap`。
   `pdfcraft-orgchart` 生产路径不用 `expect` 或 `unwrap` 的旧规不变（本文件原规则）。
4. **Arc 保留须论证。** `Arc` 只在借用结构会不合理复杂时用，论证写进代码注释。
   现有论证：`crates/pdfcraft-pdf/src/error.rs`（`Error`.`source` 旁，`Error: Clone` 而
   `Box<dyn StdError>` 不可 `Clone`）；`FontFace.bytes` 与 orgchart `FontBytes.data` 的 `Arc<Vec<u8>>`
   让 `Clone` 不再复制字体内容（见 CHANGELOG Unreleased）。
5. **不稳定 rustfmt 选项记 N/A。** 以稳定 1.92 为准：`max_width = 100` 用稳定项；
   `hex_literal_case`、`format_macro_matchers`、不可见字符转义等不稳定选项不用。
   `#[rustfmt::skip]` 只留手动对齐与生成代码两类，且须写理由。
6. **chrome / fuchsia 专属记 N/A。** 不用 `async`/`.await`（chrome 条是 Chromium 内部约束，
   本库本就无异步 API）、不用标准库起线程、`fuchsia_sync::Mutex` 不用；`Mutex` 毒化不触及
   （本库无全局锁）。
7. **文档与属性。** 公共项 `///` 须有 `Errors` / `Panics` 节（见本文件「风格、文档、错误」）；
   重要返回值与跨团队公共类型补 `#[must_use]`；跨团队公共 enum/struct 补 `#[non_exhaustive]`，
   例外只有三类：描述既有本体（如色值二态）、承诺 LSC 跟着修、或要保留 FRU 初始化语法
   （`FillOptions` 保留 `..Default::default()`，见 `crates/pdfcraft-forms/src/fields.rs`）。
   `missing_docs` 保持 `warn`（公共 spec 稳定后升 `deny`，见下节）。
8. **其余照指南做。** 命名跟 std（`as_`/`to_`/`into_` 配 `wrong_self_convention`）；
   `&Vec`/`&String`/`&Box`/`&Option` 改 `&[T]`/`&str`/`&T`/`Option<&T>`；库函数只迭代输入时
   优先 `impl IntoIterator`；返回按调用分布在 `Vec` 与 `impl Iterator` 间选并记录理由；
   bool 参数含义不明改 enum（自解释 bool 调用点加注释）；Options 式 struct 实现 `Default`；
   重要 `From` 配显式构造（`Error::new` / `Errors::from_error`）；能 derive 不手写；
   已知 std trait 优先于自定义方法；扩展 trait 加 `Ext` 后缀；`const fn` 只用于语义 const 处；
   运算符重载只做数学语义；零 `catch_unwind`，`unsafe` 整仓 forbid；不断言压缩 PDF 字节。

### Microsoft 增量（增量层，不复述 lint 表）

优先级不变：提案书 > workspace 规格（包边界）> 本文件（含 Google 节加本节）> 设计稿未确认部分。
写法冲突仍 Google-wins，本节只加增量，不推翻提案书与 Google 裁决。完整 M-ID 表见
`docs/ms-rust-guidelines-disposition.md`，lint 级别变更先记 disposition 行再改 `Cargo.toml`。

硬 N/A：`Error` 上不加 `Backtrace`，现有 `Error`/`ErrorKind`/`ErrorCode` 形状保持；不拆 crate，
四包边界锁定；保持 edition 2021 与 MSRV 1.92，升级须单独 PR；不引入 tracing/log，纯库无日志面。

增量采用：allow 转 expect（`M-LINT-OVERRIDE` 口径，toolchain 升级重跑 deny probe）；
crate 内 `pub use` 补 `#[doc(inline)]`，不做 glob 重导出；公共类型补 `Debug`；
cargo-audit/deny 按 fail closed 接入门禁。

### PR 纪律

- **一 PR 一件事**：不顺手重构无关模块；不把未确认功能「顺便」做进当前 PR。主题内为夯实基础的必要大改可以做（见「现阶段姿态」）。
- 用户可见行为写入 `CHANGELOG.md`（有公共行为起记）。
- 外部贡献说明见 `CONTRIBUTING.md`（英文短版，细节仍以本文件为准）。

## Agent 工作方式

1. **先读**提案书里与当前任务相关的已确认条；再读设计稿里仍有效的技术核实（krilla/skrifa API 事实）。
2. **主线已确认后。** 按层实现。`spec` 类型和元素字段以提案书为准。预置与搁置项见提案书总表，不提前做。
3. 实现时按层推进：geom/error（含长度类型）→ canvas →（backend）→ fonts/measure → line break → flow/paginate → table/decorate → `spec` 接线。
4. 排版测试优先断言 `RecordingCanvas` 记录；`to_pdf` 冒烟只断言 `%PDF` 之类，不断言压缩内容流细节（内容流数字实验仅属于 backend 适配器测试）。
5. 对话过长、上下文明显膨胀时，建议用户开新对话，并指向本文件与提案书。
6. 改代码前读周边文件。简单功能按小步改时，中间步骤按「门禁」里的小步检查，不每步全量 `cargo test`。提交前跑完整门禁。不擅自 `git commit` / push，除非用户明确要求。

## 明确不要做

- **`pdfcraft-pdf`** 第二 PDF 后端；该 crate 生产代码依赖 pdf-writer / lopdf / printpdf。读第三方 PDF 填表只在 `pdfcraft-forms`。已有 PDF 的加密、合并、后处理写 `/Info` 只在 `pdfcraft-post`。见 [`workspace-crates-spec.md`](docs/workspace-crates-spec.md)。
- 完整 UAX #14、bidi、阿文/印度系 shaping、竖排、kerning、连字。
- 向调用方暴露 `Flowable` trait。
- 在提案书定为不做/延后的能力上抢跑：TOC / QR / SVG / 流内 canvas / 具名 images 字典 / `pdfcraft-pdf` 内加密 / 标签 PDF / `pdfcraft-post` 的 Watermark 与输入限额等；单元格合并、§2.12 `Rotated` 含 Text/Image 等**已确认要做的**按对应层实现。股权架构图在 `pdfcraft-orgchart`，行为以 `docs/pdfcraft-orgchart-spec.md` 为准，不进 `pdfcraft-pdf`。后处理以提案书 §11 与 `docs/pdfcraft-post-spec.md` 为准。
- 用 JSON/serde 当主接口（设计稿旧表述已作废；serde 若将来做，也只是可选，不是主路径）。
- 并行开页或并行画页。
- 为「像大厂」堆流程：不建 viewer 视觉回归农场；不放宽成 `^` 依赖。
- 把 forms / orgchart / post 做成 `pdfcraft-pdf` 的 feature，或让 `pdfcraft-forms` ↔ `pdfcraft-pdf`、`pdfcraft-post` ↔ 任一兄弟 crate 互相依赖。
- 修改 hcsw 工程（含 `/mnt/data/dev/hcsw-pdfcraft`）里的任何文件。
