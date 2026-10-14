# 执行规格：企业开源就绪

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-14 |
| 状态 | **里程碑完结（2026-10-14）。** 工作项 D–R 已落地（N 不做）。三包 `0.1.0`，可发布。 |
| 来源 | 2026-10-14 企业开源就绪只读审查；逐条讨论确认 |
| 权威 | [`提案书.md`](提案书.md)；[`AGENTS.md`](../AGENTS.md)；包边界见 [`workspace-crates-spec.md`](workspace-crates-spec.md)；模块 DAG 见 [`module-dependency.md`](module-dependency.md) |
| 相关 | [`batch-readiness-spec.md`](batch-readiness-spec.md)（限额与信件已完成，发布仍按住）；[`code-quality-assessment.md`](code-quality-assessment.md)（历史探讨稿，见工作项 D） |

## 1. 一句话

工程门禁与主线架构已经像认真的 0.x Rust workspace。相对企业可采纳的开源库，还缺发布面、供应链、文档一致、模块 DAG 结案、公共面收窄与工艺收尾。本规格把缺口收成可验收工作项；**最后**才删 `publish = false` 并切 `0.1.0`。

## 2. 目标与非目标

### 目标

1. 对外文档与附属包 OSS 面达到可评审状态。
2. `pdfcraft-pdf` 模块 DAG 按硬规则无环，并由 CI 守住。
3. 公共 API 不再暗示可插拔后端；`DocumentSpec` 能设置文档元数据。
4. 三包错误约定对齐（不抽共享 error crate）；清 orgchart 生产路径 `expect`。
5. 工艺尾巴收干净：`dead_code`、公共 `# Errors`、排版内核品牌长度。
6. 供应链与直依升级达标后，删除 `publish = false` 并发布 `0.1.0`。

### 非目标（本规格明确不做）

- **整文档流式写出 / 按页释放 `RecordingCanvas`。** 现状是分页片段 → 全量 `RecordingCanvas` → krilla marks → PDF 字节。批跑靠 [`Limits`](../crates/pdfcraft-pdf/src/limits.rs)。若内存日后成实测瓶颈，另开性能规格。这与提案书「正文流式布局」不是同一件事。
- 不抽 `quire-error` / `quire-geom` 共享 crate。
- 不为消环把 `pdfcraft-pdf` 拆成多 crate；不为消环把 `FlowNode` 改成 trait 对象热路径。
- 不做 fuzz、完整威胁模型长文、viewer 视觉回归农场。
- 不并行开页或并行画页（AGENTS）。
- 不在本规格里改加密、标签 PDF、OTF、字体子集、完整 i18n shaping 等提案书范围外能力。
- 不 fork krilla、不换 PDF 后端。

## 3. 验收总闸

全部勾选后，本里程碑完结：

- [x] 工作项 D、O（文档与附属包 OSS）
- [x] 工作项 G（模块 DAG + CI 环检查）
- [x] 工作项 P、I、E（公共面、DocumentInfo、错误约定）
- [x] 工作项 C（工艺尾巴，含内核品牌长度）
- [x] 工作项 S、L（供应链；依赖升级单独 PR）
- [x] 工作项 R（`publish = false` 删除 + `0.1.0`）
- [x] AGENTS 门禁命令全绿（含新增环检查与 deny/audit）

工作项 N（流式写出）不进总闸，只保留在 §5 记录。

## 4. 工作项顺序

```text
D 对外文档对齐
  ∥ O 附属包 OSS 面
  → G 模块 DAG 整治（含 CI 环检查）
  → P 公共面收窄  ∥  I DocumentInfo + 提案小项  ∥  E 错误约定 + orgchart expect
  → C 工艺：dead_code / Errors 文档 / 内核品牌长度
  → S 供应链  ∥  L 依赖升级（单独依赖 PR）
  → R 删 publish=false + 0.1.0（最后；依赖 S、L、D、O 等前置）
```

一项一事；依赖钉版本变更必须单独 PR（AGENTS）。升 `lopdf` / 查双栈不要与 DAG、公共面重构混在同一 PR。

---

## 5. 审查条目 → 已确认处置

### 5.1 未发布（条 1）→ 工作项 R

| | |
| --- | --- |
| 现象 | workspace `publish = false`，三包 `0.0.0`。批量就绪工程前提已过，发布仍按住。 |
| 已确认 | **做，排最后。** 删除 `publish = false`，版本切 `0.1.0`。不与其它主题同 PR。 |
| 前置 | D、O、S、L 完成；G/P/I/E/C 至少达到「不阻塞对外契约」的验收（见各工作项）。 |
| 验收 | 三包可 `cargo publish --dry-run`（或等价）；CHANGELOG 有 `0.1.0` 段；`AGENTS.md` / 提案书里发布门槛表述更新。 |

### 5.2 供应链（条 2）→ 工作项 S

| | |
| --- | --- |
| 现象 | `.gitignore` 忽略 `Cargo.lock`；无 deny/audit、无 `SECURITY.md`；CI 工具链跟 `@master`；仅 Ubuntu；无 Dependabot。 |
| 已确认 | **做满。** 入库锁文件；CI `cargo-deny`（许可证 + advisory）；根 `SECURITY.md`；toolchain action 不再跟浮动 `@master`；Dependabot；多 OS 矩阵（至少再加一个非 Ubuntu runner）。 |
| 非目标 | fuzz；完整威胁模型长文。 |
| 验收 | 锁文件在 git；CI 跑 deny；有安全联系页；矩阵含 ≥2 OS；Dependabot 配置存在。 |

### 5.3 对外文档不一致（条 3）→ 工作项 D

| | |
| --- | --- |
| 现象 | 根 README / `workspace-crates-spec` 仍写 forms 若干能力未做；`code-quality-assessment` 仍提 builder / `FlowWriter`；`pdfcraft-forms-spec` 抬头与提案书 §10 打架。 |
| 已确认 | **做。** 修准 README 与 workspace 规格；评估稿标过时或改错句；forms 规格抬头/清单对齐 §10。不做全库 docs 考古。 |
| 验收 | 新人只读 README + workspace 规格 + 提案书总表，不会得到「forms 对齐线未做」的假结论。 |

### 5.4 附属包 OSS 面（条 4）→ 工作项 O

| | |
| --- | --- |
| 现象 | forms / orgchart 无包内许可证与 crate README；orgchart 字体仅有 PROVENANCE、无 OFL 正文；forms 夹具 PDF 再分发边界不清。 |
| 已确认 | **做。** 双许可证副本、短 README、`Cargo.toml` 的 `readme` / `include`；字体旁附 OFL 1.1；夹具写清边界（能附则附，否则排除出 crate `include`）。 |
| 验收 | 两包元数据与 `pdfcraft-pdf` 同级可评审；字体/夹具许可证可跟。 |

### 5.5–5.6 生产环与 DAG 文档（条 5–6）→ 工作项 G

| | |
| --- | --- |
| 现象 | `error` ↔ `limits` 生产环；目标图与树不符（真实为 `geom → error` 等）；`decorate` 测试反向碰到 `paginate`；横向边未入图；无 CI 环检查；文档仍写「已落地无反向边」。 |
| 已确认 | **做满（非最小挪类型）。** 见下方契约。`FontId` / `GlyphRun` 归属留给工作项 P。 |
| 非目标 | 多 crate 拆分；`FlowNode` trait 对象热路径。 |

**G 契约：**

1. 重写 [`module-dependency.md`](module-dependency.md) 目标图：承认 `geom → error`；写入 `limits`（`limits → error`；`fonts` / `canvas` 对 `limits` 的边按整治后真实图）。
2. `LimitKind` 与 `ErrorKind::LimitExceeded` 同属 error 侧；禁止 `error → limits`。
3. `decorate` 测试不得经 `test_util` 反向依赖 `paginate`。
4. `list → paragraph`、`table → columns`：抽中立共享模块，或在目标图写成合法单向共享。
5. CI 增加模块环检查。
6. 删除过时结案语；整治完成后写新落地记录。

**验收：** 生产无环；CI 环检查红灯能拦住回归；文档与树一致。

### 5.7 双字体栈与依赖升级（条 7）→ 工作项 L

| | |
| --- | --- |
| 现象 | 直依 `skrifa 0.48` 与 krilla 传递 `skrifa 0.42` 并存；forms 的 `lopdf =0.34.0` 落后。 |
| 已确认 | **做。** 全工作区直依升到 crates.io 最新（单独依赖 PR）；加 `[workspace.dependencies]`；调查能否经 krilla 升级消双栈。 |

**2026-10-14 crates.io 核对：**

| 直依 | 现状 | 最新 | 动作 |
| --- | --- | --- | --- |
| `lopdf` | `0.34.0` | `0.45.0` | 已升到 `=0.45.0`，`default-features = false`。0.45 无 `nom_parser`。未开 `default` 与 `font_embedding` |
| `skrifa` | `0.48.0` | `0.48.0` | 已最新 |
| `krilla` | `0.8.2` | `0.8.2` | 已最新。传递 `skrifa ^0.42.0`，本次解析 `0.42.1`。升版本消不掉双栈 |
| `read-fonts` / `png` / `unicode-segmentation` | `0.45.0` / `0.18.1` / `1.13.3` | 同左 | 不动 |

**2026-10-14 复核。** crates.io 的 `max_version` 仍是上表。krilla 0.8.2 的发布清单把 `skrifa` 写成 `0.42.0`（caret 需求 `^0.42.0`）。没有更新的 crates.io 版本。上游 `main` 的 workspace 钉 `skrifa = "0.44.0"`，未发布，而且不是 0.48.0。

**已知缝。** 直依 `skrifa =0.48.0` 与 krilla 0.8.2 里的 `skrifa ^0.42.0` 同时存在。2026-10-14 解析到 `0.42.1`。测量走 0.48.0。`Font::new` 与 `draw_glyphs` 走 0.42.1。不要 fork krilla，不要改成 git 依赖，不要把这两份写成已经统一。`emit::check_upem` 在绘制前拒绝 units per em 不一致。回归用 `quire-measure.ttf` 的字形 34（U+0041，hmtx 707）锁住 krilla 侧的 advance。两边一致时，多画 0.25 em 的 `TJ` 调整量是 `-250`。

**验收：** 直依为核对时的最新钉版（或更新一轮核对表）；workspace.dependencies 存在；双栈要么消失，要么有书面已知问题 + 测试锁。

### 5.8 公共面偏大（条 8）→ 工作项 P

| | |
| --- | --- |
| 现象 | 公开 `Canvas` / `GlyphRun` / `CanvasCommand`；后端不可替换；`FontId::from_raw` / `ImageId::from_raw` 可外部构造。 |
| 已确认 | **做。** `Canvas` 收成 `pub(crate)` 或密封；`from_raw` 收成 `pub(crate)` 或删除对外；命令类型保留为只读观测并写明非扩展点；`FontId` 归属仅在不伤测试契约时整理；`pt_near` 本项不动。 |
| 验收 | 外部不能实现可接入 `layout_spec` / `to_pdf` 的 `Canvas`；测试仍能钉 `RecordingCanvas` / 页面命令；rustdoc 不暗示可插拔后端。 |

### 5.9 元数据路径断（条 9）→ 工作项 I

| | |
| --- | --- |
| 现象 | `DocumentInfo` 在 canvas；`DocumentSpec` 无 `info`；规格到 PDF 几乎设不了标题/作者。 |
| 已确认 | **做。** `DocumentSpec` 增加 `info`；`layout_spec` / `to_pdf` 接通到 emit；提案书补确认小项。不扩 Creator/Producer 定制（保持现有 `"pdfcraft"` creator 即可）。 |
| 验收 | 设置 title/author 后，产出 PDF 元数据可读到对应值（测试方式在实现 PR 钉）；提案书有对应确认表述。 |

### 5.10 非流式写出（条 10）→ 记录 N

| | |
| --- | --- |
| 现象 | 整文档多遍内存表示。 |
| 已确认 | **本规格不做。** 写入非目标；靠 `Limits`；后续优化另开性能规格。 |
| 说明 | 正文流式布局已存在，不要与流式写出混淆。 |

### 5.11 三 crate 错误模型（条 11）→ 工作项 E

| | |
| --- | --- |
| 现象 | pdf 有 `Errors`+`Location`；forms 单条无位置；orgchart 为协议码+message，生产 4 处 map `expect`。 |
| 已确认 | **做约定对齐，不抽共享 error crate。** 消 orgchart 生产 `expect`（改为 `ErrorCode` + 可诊断 message）；forms 保持单条 `Error`，字段级 `Location` 延后；依赖错误要可 `source()` 或稳定码。 |
| 验收 | orgchart `src/` 生产路径无 `expect`/`unwrap`（测试除外）；规格/AGENTS 写明三包错误纪律。 |

### 5.12 工艺尾巴（条 12）→ 工作项 C

| | |
| --- | --- |
| 现象 | 文件级 `allow(dead_code)`；`missing_errors_doc` / `missing_panics_doc` 放行；品牌长度未贯穿排版内核。 |
| 已确认 | **做满。** 清 blanket `dead_code`（删死代码或单点 allow+理由）；公共 API 打开 `missing_errors_doc`（及可行的 panics 文档）；排版内核推进 `Length<C>` 品牌长度贯穿，不只守边界。 |
| 验收 | 无文件级无条件/接线期 blanket `dead_code`（残留须有理由）；公共失败路径有 `# Errors`；内核热路径以品牌长度为主类型（实现 PR 列模块清单）。 |

---

## 6. 与提案书 / 其它文档的关系

| 主题 | 关系 |
| --- | --- |
| `DocumentSpec.info` | 须提案书确认小项（工作项 I）后再当已定公共 API。 |
| `lopdf` / 依赖钉版本 | 升级后同步提案书 §10、`workspace-crates-spec`、`AGENTS.md`。 |
| 删 `publish = false` | 更新提案书与 AGENTS 发布门槛表述（工作项 R）。 |
| [`code-quality-assessment.md`](code-quality-assessment.md) | 历史探讨稿；工作项 D 标过时，不整篇重写。 |
| [`batch-readiness-spec.md`](batch-readiness-spec.md) | 限额/信件已完成；本规格接管「开 publish」与企业采纳剩余项。 |
| [`module-dependency.md`](module-dependency.md) | 工作项 G 重写目标图与落地记录。 |

## 7. 落地记录

| 工作项 | 状态 | 日期 | 说明 |
| --- | --- | --- | --- |
| D 对外文档 | 完成 | 2026-10-14 | 根 `README.md`、`workspace-crates-spec`、`pdfcraft-forms-spec` 抬头与提案书 §10 对齐（对齐 / `line_below` / `absolute_lines` / TTC 已实现；OTF 与子集未做）。`code-quality-assessment` 标为历史快照并划掉过时的 builder / `FlowWriter` 开项表述。验收：新人只读 README + workspace 规格 + 提案书总表，不会得到「forms 对齐线未做」的假结论。 |
| O 附属包 OSS | 完成 | 2026-10-14 | `pdfcraft-forms` / `pdfcraft-orgchart` 各有 `LICENSE-MIT`、`LICENSE-APACHE`、短 `README.md`，`Cargo.toml` 含 `readme` / `include`。orgchart 字体旁附 `OFL.txt`（SIL OFL 1.1），`PROVENANCE.txt` 指向它。forms 夹具：DejaVu 可附并进 `include`；PDF 无再分发许可说明，留在 git、排除出 crate `include`。 |
| E 错误约定 | 完成 | 2026-10-14 | orgchart 生产 4 处 `expect` 改为 `ErrorCode::LayoutFailed` + 含协议 id 的 message（`render.rs` / `paging.rs`）。`AGENTS.md` 写明三包错误纪律。`rg` 生产路径无 expect/unwrap；残留仅在 `#[cfg(test)]`。orgchart 门禁绿。 |
| G 模块 DAG | 完成 | 2026-10-14 | `LimitKind` 迁入 `error`；`limits → error` 单向。`decorate_tests` 旁路；`column_width` / `line_run` 中立共享。`python3 scripts/check_pdfcraft_pdf_modules.py` + 边文件入 CI；自检可红。`module-dependency.md` / AGENTS 图已对齐。pdfcraft-pdf 门禁绿。 |
| P 公共面 | 完成 | 2026-10-14 | `Canvas` 与 `FontId`/`ImageId::from_raw` 为 `pub(crate)`。命令类型仍公开并 rustdoc 标明非扩展点。compile_fail doctest 钉住。 |
| I DocumentInfo | 完成 | 2026-10-14 | `DocumentSpec.info`；经 `Rendered` → `to_pdf` → `set_document_info`。提案书已确认。`tests/document_info.rs` 用 `pdfinfo` 验收 Title/Author。 |
| C 工艺尾巴 | 完成 | 2026-10-14 | 清 pdfcraft-pdf 文件级 blanket `dead_code`；`missing_errors_doc`/`missing_panics_doc` 升为 warn（门禁 -D）；公共构造路径补 `# Errors`；品牌 `Length<C>` 推进 frame/flow/paginate/paragraph/columns/table/list 等热路径。环检查 114 边无环。 |
| S 供应链 | 完成 | 2026-10-14 | `Cargo.lock` 入库（不再 ignore）；`deny.toml` + CI `cargo deny check advisories licenses`；根 `SECURITY.md`；toolchain 钉 commit；Dependabot；CI 矩阵 ubuntu+macos。 |
| L 依赖升级 | 完成 | 2026-10-14 | `[workspace.dependencies]`；`lopdf =0.45.0`；skrifa/krilla 已最新。双栈书面已知缝 + `emit` upem/`TJ` 测试锁。父会话重生 `Cargo.lock`；`cargo deny check advisories licenses` 绿。 |
| R 发布 | 完成 | 2026-10-14 | 删 workspace `publish = false` 与三包 `publish.workspace`；版本 `0.1.0`；CHANGELOG / AGENTS / 提案书门槛已更新。orgchart 测试字体排除出 `include`（crates.io 10MiB）。三包 `cargo publish --dry-run --allow-dirty` 退出 0。父会话终检：环检查、deny、AGENTS 门禁全绿。 |
| N 流式写出 | 不做 | 2026-10-14 | 仅记录 |
