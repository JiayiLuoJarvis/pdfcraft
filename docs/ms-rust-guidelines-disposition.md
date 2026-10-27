# Microsoft Pragmatic Rust Guidelines disposition 矩阵（Wave 0）

裁决：提案书 > workspace-crates-spec > AGENTS（Google-wins 写法层 + 本矩阵增量层）> 设计稿未确认部分。
Google 写法冲突仍 Google-wins；产品语义冲突提案书仍最高。本矩阵是设计与静态质量增量层，不做第二套格式手册。

指南版本：Microsoft Pragmatic Rust Guidelines 2026.6，快照日期 2026-10-27，
来源 <https://microsoft.github.io/rust-guidelines/guidelines/checklist/index.html>。
快照 ID 列表见 `.omo/evidence/ms-rust-guidelines-incremental/task-1-ms-checklist-snapshot.md`
（主仓路径；快照 N=90，本文件 M-* 行数须等于 N）。

配置注：lint 级别变更属于门禁行为变更，先记 disposition 行再改 `Cargo.toml` `[lints]`；
本波（T1）只写文档，不改任何产品 Rust 文件。

disposition 枚举只用：`done` | `adopt` | `N/A` | `defer`。
其中 `adopt` 表示本计划后续波要做（T3 起），`defer` 表示明确延后（见 `## Deferred`），
ADAPT 表示有理由的偏离（只用于双路径与已锁定的 N/A 形状，证据列写清）。

expect-toolchain 耐久注：`#[expect(..., reason)]` 只在当前 toolchain/MSRV 下被 deny-probe 验证；
每次 toolchain 或 MSRV 升级 PR 须重跑 T3 口径的 deny-probe，清理过期 expect。本条为硬性要求。

## Universal

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-UPSTREAM-GUIDELINES | Follow the upstream guidelines | adopt | Google-wins 写法层已落地（见 google disposition）；Rust API Guidelines 按 spec 稳定度跟进 |
| M-STATIC-VERIFICATION | Use static verification | adopt | fmt/clippy/test/doc/DAG/deny 现有；Wave-1 enabled set（T5）：仅 pdf+forms 加 `missing_debug_implementations = "warn"`（`[lints.rust]`）与 `allow_attributes_without_reason = "warn"`（`[lints.clippy]`，rust 表名已废弃），post/orgchart 不动；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-5-lints.log`。cargo-deny check advisories（0.20.2，fail-closed，zero ignore，yanked=deny）与 cargo audit 同等覆盖 RustSec 公告：CI deny job 无 continue-on-error、无 `\|\| true`，`deny.toml [advisories]` 仅 `yanked = "deny"`（ignore 计数 0），本地 `cargo deny check advisories --show-stats` 为 `0 errors, 0 warnings, 0 notes`；deny 的 `--audit-compatible-output` 语义说明其 advisories 检查查同一 RustSec 库、同一 Cargo.lock 图，未命中即非零退出，故 deny-fail-closed 等价 audit-fail-closed，不另加 cargo-audit job。安全 triage 默认：未来公告走升级受影响依赖，或记有时限的显式 `[advisories] ignore` 例外（deny-exception，带理由），绝不弱化 CI job；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-12-audit.log` |
| M-LINT-OVERRIDE-EXPECT | Lint overrides should use expect | adopt | 现状 allow 约 46 处、expect 0；T3 清单 + T4 起按 probe 迁移；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-3-allow-inventory.log`（36 处清单 + deny-probe）、`.omo/evidence/ms-rust-guidelines-incremental/task-4-expect-pdf-forms.log`（pdf+forms 转 expect，3 处 target-conditional 死代码回退 keep 见永久 allow-list）、`.omo/evidence/ms-rust-guidelines-incremental/task-8-forms.log`、`task-9-post.log`（post 仅 keep 一处）、`task-10-orgchart.log` |
| M-PUBLIC-DEBUG | Public types are Debug | adopt | T6/T8/T9/T10 补齐；先开 missing_debug_implementations warn（pdf+forms 先行）；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-6-pdf-debug-inline.log`（pdf 零缺口审计）、`task-8-forms.log`、`task-9-post.log`、`task-10-orgchart.log` |
| M-PUBLIC-DISPLAY | Public types meant to be read are Display | done | 各包 Error/错误面已有 Display；行为不变 |
| M-SMALLER-CRATES | If in doubt, split the crate | N/A | 锁定：四包结构见 workspace-crates-spec，不拆分（Google 小包条同步 N/A） |
| M-WEASEL-WORDS | Names are free of weasel words | adopt | 后续波顺手清理，不做全仓改名运动 |
| M-SHORT-NAMES | Names of items are short | adopt | 同上；提案书锁定的公开名不改 |
| M-REGULAR-FN | Prefer regular over associated functions | done | 无违规模式；调用点人体工学保持 |
| M-DOCUMENTED-MAGIC | Magic values are documented | adopt | 后续文档波补注释；不改数值语义 |
| M-LOG-STRUCTURED | Use structured logging with message templates | N/A | 锁定：纯库无日志框架，不引入 tracing/log |

## Libs/Interop

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-TYPES-SEND | Types are Send | adopt | 后续波确认公共类型 Send/Sync；无线程池设计，不改结构 |
| M-ESCAPE-HATCHES | Native escape hatches | adopt | 后端密封保持；转义口只按需加 |
| M-DONT-LEAK-TYPES | Don't leak external types | done | krilla/lopdf/skrifa 不进公共签名；F1 有 no-leak probe |
| M-FOREIGN-REEXPORTS | Items come from their original crate | done | 无跨包重导出；四包互不依赖 |
| M-IMPL-ASREF | Accept impl AsRef where feasible | defer | 见 Deferred t-ms-asref；`&[u8]` 现状保持 |
| M-IMPL-RANGEBOUNDS | Accept impl RangeBounds where feasible | defer | 同上；无区间 API 需求 |
| M-IMPL-IO | Accept impl IO where feasible (sans IO) | N/A | 无 IO 抽象需求；纯内存库 |

## Libs/UX

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-SIMPLE-ABSTRACTIONS | Abstractions don't visibly nest | done | 分层调用扁平；无新增嵌套抽象 |
| M-AVOID-WRAPPERS | Avoid smart pointers and wrappers in APIs | done | Arc 只在论证处（Error source、FontBytes），见 Google disposition |
| M-DI-HIERARCHY | Prefer types over generics, generics over dyn traits | done | Google 类型节已 done；无 dyn 扩张 |
| M-ERRORS-CANONICAL-STRUCTS | Errors are canonical structs | N/A | 锁定 ADAPT：Error+ErrorKind/Location 与 orgchart ErrorCode 保持；不加 Backtrace，不私有化 Kind（Clone 模型优先） |
| M-FROM-ERROR | Canonical error conversion uses From, not map_err | done | Error::new 配 From<ErrorKind>（pdf/forms/post），见 Google disposition |
| M-INIT-BUILDER | Complex type construction has builders | N/A | 锁定：DocumentSpec 用结构体字面量 + Default，不复活 builder DSL |
| M-INIT-CASCADED | Complex type initialization hierarchies are cascaded | N/A | 同上；无级联初始化层级 |
| M-SERVICES-CLONE | Services are Clone | N/A | 锁定：DocumentSpec 是值类型文档描述，非服务；不加 Clone 语义承诺 |
| M-ESSENTIAL-FN-INHERENT | Essential functionality should be inherent | done | 核心能力为 inherent 方法；扩展 trait 加 Ext 后缀 |
| M-BALANCED-MODULES | Modules are balanced in size and scope | done | 模块 DAG 由脚本锁定；后续拆分只按主题 |
| M-NO-PRELUDE | Don't define preludes | done | 无 prelude；test-utils 只经 feature 门暴露 |
| M-PARAMETER-CONSISTENCY | Parameter ordering is consistent | done | 同类函数参数顺序一致；新增 API 沿用 |
| M-COLLECTION-TRAITS | Collections implement the appropriate iter traits | adopt | Errors::iter 已有；后续波查漏 |
| M-ASYNC-FN | Functions are async over returning a Future | N/A | 无异步 API（Google 并发节同步 N/A） |

## Libs/Resilience

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-MOCKABLE-SYSCALLS | I/O and system calls are mockable | N/A | 无系统调用面；字体/文件注入走现有参数 |
| M-TEST-UTIL | Test utilities are feature gated | done | test-utils feature 门；集成测试不用测试依赖反转生产 DAG |
| M-INTEGRATION-TESTS | Integration tests live under tests/ | done | 各包 tests/ 现有；RecordingCanvas/pdftotext/parity 格局保持 |
| M-STRONG-TYPES | Use the proper type family | done | Positive/NonNegative/Finite 品牌长度类型保持 |
| M-STRONG-TYPES-GUARD | Newtypes guard their invariants | done | 构造期校验保持；坐标断言全过 |
| M-BUILD-RESULT | Builders validate in final build() | N/A | 无 builder；M-INIT-BUILDER 联动 N/A |
| M-NO-GLOB-REEXPORTS | Don't glob re-export items | done | 生产 src 零 glob 重导出；crate-root 用显式 pub use 列表 |
| M-AVOID-STATICS | Avoid statics | done | 无生产 static 可变状态；无全局锁 |
| M-LOG-NOT-PRINT | Production code uses telemetry, not println | done | 生产 src 零 println!/dbg!；F 波有 grep probe |

## Libs/Building

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-OOBE | Libraries work out of the box | done | 默认 feature 可用；字体注入有文档说明 |
| M-SYS-CRATES | Native -sys crates compile without dependencies | N/A | 无 -sys 包 |
| M-FEATURES-ADDITIVE | Features are additive | done | feature 只做加法；cargo-hack 延后（见 Deferred t-ms-hack） |

## Macros

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-MACRO-LAST-RESORT | Macros are a last resort | N/A | 无生产宏；常规函数优先已满足 |
| M-EXAMPLE-OVER-PROC | Prefer macros by example over proc macros | N/A | 无过程宏 |
| M-MACROS-DONT-LIE | Macros don't lie about signatures | N/A | 无宏 |
| M-MACRO-MAIN-CRATE | Macros assume main crate | N/A | 无宏 |
| M-MACRO-VERSION-PIN | Pin supporting proc macro crates | N/A | 无过程宏依赖 |
| M-MACRO-HELPERS | Third party items come from hidden private module | N/A | 无宏 |
| M-PROC-IMPL | Proc macros should have separate impl crate incl. tests | N/A | 无过程宏 |
| M-PROC-IMPLIED-ITEMS | Proc macros don't produce implied or hidden items | N/A | 无过程宏 |

## Apps

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-MIMALLOC-APPS | Use mimalloc for apps | N/A | 本仓全是库，无二进制应用 |
| M-APP-ERROR | Applications may use Anyhow or derivatives | N/A | 同上；库错误模型不变 |
| M-TARGET-CPU | Applications target highest viable target-cpu | N/A | 同上 |

## FFI

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-ISOLATE-DLL-STATE | Isolate DLL state between FFI libraries | N/A | 无 FFI 面 |
| M-FFI-TRANSLATES | Business logic belongs in core crates, FFI only translates | N/A | 无 FFI 面 |
| M-FFI-NAMING | FFI crates follow established naming conventions | N/A | 无 FFI 面 |

## Correctness

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-UNSAFE | Unsafe needs reason, should be avoided | done | 整仓 forbid(unsafe_code)；生产 src 零 unsafe |
| M-UNSAFE-IMPLIES-UB | Unsafe implies undefined behavior | done | 同上；无 unsafe 即无 UB 暴露面 |
| M-UNSOUND | All code must be sound | done | 同上；miri 记 N/A（forbid 下空转） |
| M-PANIC-IS-STOP | Panic means stop the program | done | 唯一公共 panic 是 Errors::first（有 Panics 文档）；不做 panic 续行 |
| M-PANIC-ON-BUG | Detected programming bugs are panics, not errors | done | 缺字/越界等编程错误走 panic 或 Result 按现有模型；行为不变 |
| M-PANIC-CONTINUATION | Panic continuation is last resort | done | 零 catch_unwind；F 波 grep probe |
| M-PANIC-MESSAGE | Custom panics have a helpful message | done | expect 带上下文；生产零裸 unwrap |

## Performance

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-THROUGHPUT | Optimize for throughput, avoid empty cycles | defer | 见 Deferred t-ms-perf；无热点证据不重写 |
| M-HOTPATH | Identify, profile, optimize the hot path early | defer | 同上；证据门控 |
| M-YIELD-POINTS | Long-running tasks should have yield points | N/A | 无长任务/async；批跑为调用方串行 |
| M-MEM-REUSE | Reuse allocations where possible | defer | 同 t-ms-perf；只在热点处做 |
| M-LOG-OVERHEAD | Library telemetry does not tank performance | N/A | 无 telemetry 面 |
| M-AVOID-INDIRECTION | Nested type hierarchies avoid needless indirection | done | 无多余 Box/dyn 嵌套 |
| M-BOX-DST | Use boxed slices and strings for immutable owned sequences | defer | 同 t-ms-perf；不做全仓 DST 化 |
| M-SHRINK-TO-FIT | Shrink collections to fit after building | defer | 同 t-ms-perf |
| M-FAST-HASHER | Use a fast hasher where possible | defer | 无哈希热点证据；保持默认 hasher |
| M-INITIAL-CAPACITY | Collections are created with sufficient initial capacity | defer | 同 t-ms-perf；热点处补 with_capacity |
| M-ASYNC-STACK-SIZE | Hot async functions reduce stack size | N/A | 无 async |

## Project

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-CARGO-WORKSPACE | Common settings come from the workspace Cargo.toml | done | 版本/依赖钉在根 workspace.dependencies，成员用 workspace=true |
| M-CRATES-IN-WORKSPACE | The workspace lists and versions all crates | done | 四包 0.1.0 同仓；deny 配套 |
| M-CRATES-FLAT-FOLDER | All crates are siblings in one folder | done | crates/ 下四包平级 |
| M-LATEST-EDITION | New crates target latest edition | N/A | 锁定：保持 edition 2021 / MSRV 1.92（跟 krilla 0.8.2）；升级须单独 PR |
| M-MSRV | MSRV is conservatively updated | done | MSRV 1.92 文档化；CI 同 toolchain；升级 PR 重跑坐标与 expect probe |

## Docs

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-FIRST-DOC-SENTENCE | First sentence is one line; approx. 15 words | adopt | T7/T8/T9/T10 文档波；约 15 词首句；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-7-pdf-docs.log`、`task-8-forms.log`、`task-9-post.log`、`task-10-orgchart.log` |
| M-MODULE-DOCS | Has comprehensive module documentation | adopt | 公共模块补 //!；同上；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-7-pdf-docs.log`、`task-8-forms.log`、`task-9-post.log` |
| M-CANONICAL-DOCS | Documentation has canonical sections | adopt | Errors/Panics/Example 按现有 lint 级别补齐；不加 Parameters 表；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-7-pdf-docs.log`（Errors 计数 78→78 非减）、`task-8-forms.log`（missing_errors_doc/missing_panics_doc allow→warn）、`task-9-post.log` |
| M-DOC-INLINE | Mark pub use items with doc(inline) | adopt | crate-root 与有意 intra-crate pub use 加 doc(inline)；禁 glob；T6 起；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-6-pdf-debug-inline.log`、`task-8-forms.log`、`task-9-post.log`、`task-10-orgchart.log` |

## AI

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-DESIGN-FOR-AI | Design with AI use in mind | adopt | 本矩阵 + Google disposition 即机器可读规则；AGENTS 指针 |
| M-SINGLE-ITEM-PATH | Items are only visible through one path | adopt | ADAPT：双路径（crate-root + 模块路径）保留，加 doc(inline)，禁新增无 disposition 行的双路径；家族清单与无新增证明见 `## 路径审计`；证据 `.omo/evidence/ms-rust-guidelines-incremental/task-11-paths.log` |
| M-NO-META-DESIGN-DOCUMENTATION | Avoid meta design documentation | adopt | rustdoc 只写终态行为；合规表留在 docs/，不进 rustdoc |
| M-TAUTOLOGICAL-TESTS | Tests do not assert ground truth | done | 排版断言 RecordingCanvas 坐标；压缩字节不断言 |
| M-RUST-SHAPED | Rust code solves Rust problems | done | 无跨语言范式移植；错误/迭代照 Rust 惯用法 |

## 路径审计（M-SINGLE-ITEM-PATH ADAPT，T11）

MS 原意：每个条目只经一条路径可见，避免 `crate::X` 与 `crate::mod::X` 并存造成文档与导入分叉。
本仓 ADAPT：`pdfcraft-pdf` 的 crate-root 重导出 + 定义模块路径均为已发布公共面，删任一方向都是 breaking，故双路径保留、两边都加 `#[doc(inline)]`；其余三包模块全私有，本就单路径。禁止新增无本节 ADAPT 行的双路径；F1 可 grep 复核。

### 双路径家族清单（F1-greppable）

只有 `pdfcraft-pdf` 有双路径（`pub mod error/geom/limits/canvas/fonts/spec` + crate-root `pub use`）。`pdfcraft-forms`、`pdfcraft-post`、`pdfcraft-orgchart` 的全部 `mod` 均为私有（无 `pub mod`），条目只经 crate-root 可见，零双路径。

| 家族 | crate-root 路径 | 定义模块路径 | 备注 |
| --- | --- | --- | --- |
| pdf-A error 组（11 项） | `crate::{CellIndex, CellSpanReason, ColumnWidthsReason, DecorationKind, Error, ErrorKind, Errors, GeometryKind, Location, Region, Result}` | `crate::error::{同名}` | 定义在 `error.rs` |
| pdf-B geom 组（28 项） | `crate::{Color, Finite, Ink, IntoFinite, IntoNonNegative, IntoPoint, IntoPositive, IntoRect, Length, MarginPreset, Margins, NonNegative, Orientation, PageSize, Paint, Paper, Path, PathBuilder, PathVerb, Pen, Point, Positive, Rect, Size, Stroke, A4, LETTER, MM_TO_PT}` | `crate::geom::{同名}` | 定义在 `geom.rs` |
| pdf-C limits 组（2 项） | `crate::{LimitKind, Limits}` | `crate::limits::{LimitKind, Limits}` | `LimitKind` 定义在 `error.rs`，经 `limits.rs` 的 `pub use crate::error::LimitKind` 中转，故 `LimitKind` 为三路径：`crate::LimitKind` + `crate::limits::LimitKind` + `crate::error::LimitKind` |
| pdf-D spec 自有组（7 项） | `crate::{layout_spec, DocumentSpec, Numbering, OutlineEntry, Ratio, Rendered, Track}` | `crate::spec::{同名}` | 定义在 `spec/mod.rs`（`DocumentSpec`、`OutlineEntry`）/`spec/lower.rs`（`layout_spec`，`lower` 私有）/`spec/types.rs`（`Numbering`、`Ratio`、`Rendered`、`Track`，`types` 私有）；其余 spec 类型（`Block`、`SectionSpec`、`Resources` 等 20+ 项）只经 `crate::spec::` 可见，单路径 |
| pdf-E spec 转导出组 | `crate::{FontFace, FontFamily, FontRegistry}`（见 pdf-G） | `crate::spec::{FontFace, FontFamily, FontRegistry}` + `crate::fonts::{同名}` | 三路径；`crate::spec::DocumentInfo` + `crate::canvas::DocumentInfo` 为双路径（root 未重导出 `DocumentInfo`）；`crate::spec::{Numbering, Ratio, Rendered, Track}` + root（见 pdf-D） |
| pdf-F canvas 组（10 项 + test-utils 2 项） | `crate::{CanvasCommand, DecodedImage, GlyphPlacement, GlyphRun, ImageFormat, ImageId, LinkTarget, LinkXyz, RecordedPage, RecordingCanvas}`（+ feature 门 `pt_near`、`PT_ABS_TOLERANCE`） | `crate::canvas::{同名}` | 定义在 `canvas.rs`；`DocumentInfo` 例外见 pdf-E |
| pdf-G fonts 组（4 项） | `crate::{FontFace, FontFamily, FontId, FontRegistry}` | `crate::fonts::{同名}` | `FontId` 仅双路径；其余三项三路径（见 pdf-E） |

单路径（非家族，不列为双路径）：`crate::to_pdf`（`lib.rs` 定义）、`crate::Result` 别名（forms/post各自定义，单路径）、spec 仅模块路径类型（`Block` 等）、forms/post/orgchart 全条目（模块私有）。

### 本波无新增双路径证明（T11）

工作树 `git diff` 在六处路经文件（四包 `lib.rs` + `pdf/src/spec/mod.rs` + `pdf/src/limits.rs`）中只增加 `#[doc(inline)]` 属性行，零新增 `pub use` 条目（`git diff -- <上列文件>` 无 `+pub use` 新增项名行；见 `task-11-paths.log` 粘贴）。已提交分支 `merge-base HEAD origin/main == HEAD`，故分支已提交部分亦无新增。

###  reviewer 检查清单

1. 新增 `pub use` 是否复用了已在别处 `pub` 的条目名——是则必须同 diff 更新上表，否则打回。
2. `#[doc(inline)]` 是否只加在 intra-crate 本地类型 `pub use` 上（不用于 std/第三方）。
3. 是否删除了上表任一既有路径（禁止；双向保留）。

## 永久 allow-list（起点骨架，后续波按 probe 同步）

- forms `format_push_string`：提案书 §10 填充语义要求，keep allow（reason 必备）。
- post `struct_excessive_bools`（EncryptionConfig）：镜像 PDF 安全位，keep。
- orgchart `struct_field_names` / `many_single_char_names` / `float_cmp`：领域 justify 后 keep。
- feature 门控 `dead_code`（如 no-default 下的 from_raw/ResolvedFont/resolve）：keep，基线红不借本主题修。
- target-conditional `dead_code`（T4 实测 `#[expect]` 在 `--all-targets` 下无法两边同时满足，keep `allow`；见 T4 log `task-4-expect-pdf-forms.log` DEVIATION）：
  - pdf `canvas.rs:390` `with_transform`：一侧 target 死、另一侧活，expect 必空一边（T4 复现 exit 101）。
  - pdf `geom.rs:1057` `Affine::apply`：`rotated` 烘焙路径外只有单测用，expect 必空一边。
  - pdf `geom.rs:1186` `Path::rotated`：layer 走 Canvas CTM，只有 geom 单测用，expect 必空一边。
- workspace `module_name_repetitions`：按需 keep。
- forms `missing_errors_doc` / `missing_panics_doc`：T8 由 allow 收紧为 warn（clippy/test/doc 三门禁绿，见 task-8-forms.log）。
- 其余 allow 一律经 T3 deny-probe 后转 expect；keep 集与本节双向 diff 为空（F1 验）。

## Known gate gaps（基线同现，不掩盖）

以下组合不是本主题引入；它们是 AGENTS.md 默认门禁之外的已知红项。本文的绿只指默认组合。

- **no-default-features**：pdf 的 `--no-default-features` clippy/test 在基座即 exit 101
 （3 个 dead-code：`from_raw`、`ResolvedFont`、`resolve`；test 另因 examples/tests 的 `to_pdf` 未解析）。
- **字体/CJK**：未设 `PDFCRAFT_*` 时字体用例提前返回；DejaVu 下 pdf 269 passed / 9 failed / 1 ignored
 （失败名与基座一致）；forms CJK 条件测试在 DejaVu 下跳过，`fill_and_flatten.rs` 的
  `FontFile2` 字节断言与 `fills_chinese_text` ignore 保持基线行为。CI 的 NotoSansCJK 才能覆盖。
- Google 延后项 t5/t7/t9（CheckBox bool、文档语言 enum 非穷尽、文档小项）不由本计划重开为 adopt，
  只在此披露。

## Deferred

- **t-ms-asref**：公共 `AsRef` 签名。触发：某调用点具名举证非加法不可，且不破坏推断；否则保持 `&[u8]`/`Into`。
- **t-ms-perf**：容量/复用/收缩（M-THROUGHPUT/M-HOTPATH/M-MEM-REUSE/M-BOX-DST/M-SHRINK-TO-FIT/M-FAST-HASHER/M-INITIAL-CAPACITY）。触发：profile 热点证据；否则只记 defer。
- **t-ms-hack**：cargo-hack。触发：feature 矩阵变多；现按 AGENTS 延后。
- **t-ms-restriction**：MS restriction lint 整块试跑。触发：scratch 分支报告制，不直接进 Cargo.toml。
- **Google t5/t7/t9**：OUT（见 Known gate gaps），本计划不实现。
