# AGENTS.md

给在本仓库里改代码的人类和 coding agent 用。短、可执行；细节去查提案书和设计稿，不要在这里复述长表。

## 文档优先级

1. [`docs/提案书.md`](docs/提案书.md) — **已确认条目的唯一权威**。
2. [`docs/pdfcraft-design.md`](docs/pdfcraft-design.md) — 设计草稿。其中**未经提案书确认**的决定先不算数（提案书抬头写明）。
3. [`docs/workspace-crates-spec.md`](docs/workspace-crates-spec.md) — 仓库 workspace / 多 crate 边界。
4. [`docs/rust-engineering-standard.md`](docs/rust-engineering-standard.md) — 代码写法与工程规范的唯一出处（风格、panic、错误、类型、架构、确定性、依赖、semver、测试、文档、lint、门禁）。
5. 本文件 — 产品约束、目录与 agent 工作方式。

冲突时：提案书 > workspace 规格（包边界）> 工程规范 > 本文件 > 设计稿未确认部分。

提案书 §1 到 §9 主线已确认（见提案书文末总表）。§10 确认 `pdfcraft-forms` 读取已有 AcroForm 并 flatten。`list_fields` 与 `fill_and_flatten` 已在 `crates/pdfcraft-forms`，含对齐、按字段字号、`line_below`、`absolute_lines`、TTC 拆面与 TrueType `glyf` 子集。OTF 仍未做。不要把填表写进 `pdfcraft-pdf`。§11 确认 `pdfcraft-post`：对已有 PDF 字节做 `SetMetadata`、`Encrypt`、`StampAndEncrypt`、`merge_pdfs`、`Pipeline`。Watermark 与输入限额延后。不要把加密、合并、后处理写 Info 写进 `pdfcraft-pdf`。`pdfcraft-pdf` 的实现按层推进。预置边距枚举等小项可后补。搁置项（章跟流走）不要提前做进公共 API。页眉页脚可以是 `BandFn`，并可放文字、图片和形状，见提案书 §2.10。§2.12 已确认 `LayerMark { when, child: Mark::Rotated { .. } }` 可包 path / Text / Image（`LayerShape<K>`，斜向水印）。流内 canvas 已保留为 `Block::Canvas`（§2.14、§12.7）。具名 images 字典仍不做。

仍须遵守：未确认功能不「顺便」做。`pdfcraft-pdf` 是 `0.6.0`，`pdfcraft-forms` 是 `0.2.0`，`pdfcraft-orgchart` 是 `0.3.1`，`pdfcraft-post` 是 `0.3.0`。都可以发布。版本号不必四包相同。实现只在本仓库；禁止修改 hcsw 工程中的任何文件（只读对照允许）。

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
- 版本保持 `0.x`（各包版本见上文）。

调用方式（提案书 §1，已定）：`DocumentSpec` 值，经 `layout_spec` 排版。正文流式。页眉、页脚、`behind` / `front` 在每一页绘制时由库填页码。调用方不给正文块写函数，页眉页脚可为 `BandFn`。`Flowable` 只在库内部，不向调用方暴露。

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
    pdfcraft-orgchart/       股权与对外投资图。不依赖 pdfcraft-pdf。Font；skrifa 0.48.0 测量；krilla 0.8.2 绘制
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

写法与工程规则只在 [`docs/rust-engineering-standard.md`](docs/rust-engineering-standard.md)。门禁命令见其 TOOL-5，lint 见根 `Cargo.toml` `[workspace.lints]`，工具链与 MSRV（1.98）见 TOOL-1 与 [`docs/msrv.md`](docs/msrv.md)。评审意见引用规则号（如 `PAN-3`），不在本文件复述。

### 字形管线与依赖（产品约束）

- 依赖钉版规则见工程规范 DEP-2 / DEP-3。字体与 PDF 栈的直依：`krilla =0.8.2`（`default-features = false`，`pdfcraft-pdf` 再开 `raster-images`）、`skrifa =0.48.0`（workspace 关闭 default；`pdfcraft-pdf` 与 `pdfcraft-forms` 打开 `std` 和 `autohint_shaping`，`pdfcraft-orgchart` 只打开 `std`）、`read-fonts =0.45.0`（`default-features = false`，`pdfcraft-orgchart` 再开 `std`）、`lopdf =0.45.0`（`default-features = false`）、`png =0.18.1`；post 的 `thumbnail` feature 另用 `hayro =0.8.0`、`image =0.25.10`。
- 不启用 krilla 的 `simple-text` / `rayon`；禁止 `Surface::draw_text`。
- 测量与绘制同一条字形管线：charmap → advance → 断行 → 同一批 glyph id 再画。
- 缺字策略以提案书 §4 为准（设计稿默认「缺字即错」；未确认前不要实现回退字体）。

### PR 纪律

- **一 PR 一件事**：不顺手重构无关模块；不把未确认功能「顺便」做进当前 PR。主题内为夯实基础的必要大改可以做（见「现阶段姿态」）。换键、改形状不留过渡层，见工程规范 VER-3。
- 用户可见行为写入该包的 `CHANGELOG.md`；breaking 先升 `0.y` 的 `y` 位（工程规范 VER-1、VER-2）。
- 外部贡献说明见 `CONTRIBUTING.md`（英文短版，规则以工程规范为准）。

## Agent 工作方式

1. **先读**提案书里与当前任务相关的已确认条；再读设计稿里仍有效的技术核实（krilla/skrifa API 事实）。
2. **主线已确认后。** 按层实现。`spec` 类型和元素字段以提案书为准。预置与搁置项见提案书总表，不提前做。
3. 实现时按层推进：geom/error（含长度类型）→ canvas →（backend）→ fonts/measure → line break → flow/paginate → table/decorate → `spec` 接线。
4. 排版测试优先断言 `RecordingCanvas` 记录；`to_pdf` 冒烟只断言 `%PDF` 之类，不断言压缩内容流细节（内容流数字实验仅属于 backend 适配器测试）。
5. 对话过长、上下文明显膨胀时，建议用户开新对话，并指向本文件与提案书。
6. 改代码前读周边文件。简单功能按小步改时，中间步骤按工程规范 TOOL-5 的小步检查，不每步全量 `cargo test`。提交前跑 TOOL-5 的完整门禁。不擅自 `git commit` / push，除非用户明确要求。

## 明确不要做

- **`pdfcraft-pdf`** 第二 PDF 后端；该 crate 生产代码依赖 pdf-writer / lopdf / printpdf。读第三方 PDF 填表只在 `pdfcraft-forms`。已有 PDF 的加密、合并、后处理写 `/Info` 只在 `pdfcraft-post`。见 [`workspace-crates-spec.md`](docs/workspace-crates-spec.md)。
- 完整 UAX #14、bidi、阿文/印度系 shaping、竖排、kerning、连字。
- 向调用方暴露 `Flowable` trait。
- 在提案书定为不做/延后的能力上抢跑：TOC / QR / SVG / 具名 images 字典 / `pdfcraft-pdf` 内加密 / 标签 PDF / `pdfcraft-post` 的 Watermark 与输入限额等。流内 canvas 已是 `Block::Canvas`。单元格合并、§2.12 `Rotated` 含 Text/Image 等**已确认要做的**按对应层实现。股权架构图在 `pdfcraft-orgchart`，行为以 `docs/pdfcraft-orgchart-spec.md` 为准，不进 `pdfcraft-pdf`。后处理以提案书 §11 与 `docs/pdfcraft-post-spec.md` 为准。
- 用 JSON/serde 当主接口（设计稿旧表述已作废；serde 若将来做，也只是可选，不是主路径）。
- 并行开页或并行画页。
- 为「像大厂」堆流程：不建 viewer 视觉回归农场；不放宽成 `^` 依赖。
- 把 forms / orgchart / post 做成 `pdfcraft-pdf` 的 feature，或让 `pdfcraft-forms` ↔ `pdfcraft-pdf`、`pdfcraft-post` ↔ 任一兄弟 crate 互相依赖。
- 修改 hcsw 工程（含 `/mnt/data/dev/hcsw-pdfcraft`）里的任何文件。
