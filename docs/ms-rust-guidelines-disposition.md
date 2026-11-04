# Microsoft Pragmatic Rust Guidelines disposition

本表只做映射：每个 M-ID 对应 [`rust-engineering-standard.md`](rust-engineering-standard.md) 的哪条规则，或为何不适用。规则正文与执行方式（lint、脚本、评审）只写在工程规范里。优先级：提案书 > workspace-crates-spec > 工程规范 > 本表。

指南版本：Microsoft Pragmatic Rust Guidelines 2026.6，快照日期 2026-10-27，
来源 <https://microsoft.github.io/rust-guidelines/guidelines/checklist/index.html>。
快照 N=90，本文件 M-* 行数须等于 N。

disposition 取值：`done`（已满足，规则在规范里）、`adopt`（规则已写进规范，代码尚待清理，见规范 §16）、`N/A`、`defer`（见文末），ADAPT 表示有理由的偏离（理由写在证据列）。

lint 级别的改动先改规范与根 `[workspace.lints]`，本表同一 PR 更新对应行。

## Universal

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-UPSTREAM-GUIDELINES | Follow the upstream guidelines | done | 规范 §0 的来源顺序：Style Guide、API Guidelines、本指南、Google |
| M-STATIC-VERIFICATION | Use static verification | done | 规范 TOOL-5（门禁）、DEP-6（cargo-deny fail closed）、§15（lint 取舍）。cargo-deny 的 advisories 检查与 cargo-audit 查同一 RustSec 库与同一 `Cargo.lock`，不另加 cargo-audit job；公告的处理是升级依赖，或在 `deny.toml` 记带理由的显式例外，不弱化 CI job |
| M-LINT-OVERRIDE-EXPECT | Lint overrides should use expect | done | 规范 LINT-2、TRK-9。`clippy::allow_attributes` 在 `[workspace.lints]` 里 deny。例外写在代码上的 `#[expect(lint, reason = "...")]`，只在部分 target 成立的用 `cfg_attr(.., expect(..))`。`price_lints.py` 对该 lint 为 0 |
| M-PUBLIC-DEBUG | Public types are Debug | done | 规范 API-7，`lint:missing_debug_implementations`（四包 deny） |
| M-PUBLIC-DISPLAY | Public types meant to be read are Display | done | 各包 Error/错误面已有 Display；行为不变 |
| M-SMALLER-CRATES | If in doubt, split the crate | N/A | 锁定：四包结构见 workspace-crates-spec，不拆分（Google 小包条同步 N/A） |
| M-WEASEL-WORDS | Names are free of weasel words | adopt | 规范 STY-4（评审）；不做全仓改名运动，提案书锁定的公开名不改 |
| M-SHORT-NAMES | Names of items are short | adopt | 规范 STY-4（评审） |
| M-REGULAR-FN | Prefer regular over associated functions | done | 无违规模式；调用点人体工学保持 |
| M-DOCUMENTED-MAGIC | Magic values are documented | adopt | 规范 DOC-6、TRK-2（评审） |
| M-LOG-STRUCTURED | Use structured logging with message templates | N/A | 锁定：纯库无日志框架，不引入 tracing/log |

## Libs/Interop

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-TYPES-SEND | Types are Send | adopt | 规范 ERR-2：编译期断言；现只有 pdf `spec` 有 `assert_send_sync`，其余三包待补 |
| M-ESCAPE-HATCHES | Native escape hatches | adopt | 后端密封保持；转义口只按需加 |
| M-DONT-LEAK-TYPES | Don't leak external types | done | 规范 API-11：krilla / lopdf / skrifa / hayro / image 不进公共签名（评审） |
| M-FOREIGN-REEXPORTS | Items come from their original crate | done | 无跨包重导出；四包互不依赖 |
| M-IMPL-ASREF | Accept impl AsRef where feasible | defer | 见 Deferred t-ms-asref；`&[u8]` 现状保持 |
| M-IMPL-RANGEBOUNDS | Accept impl RangeBounds where feasible | defer | 同上；无区间 API 需求 |
| M-IMPL-IO | Accept impl IO where feasible (sans IO) | N/A | 无 IO 抽象需求；纯内存库 |

## Libs/UX

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-SIMPLE-ABSTRACTIONS | Abstractions don't visibly nest | done | 分层调用扁平；无新增嵌套抽象 |
| M-AVOID-WRAPPERS | Avoid smart pointers and wrappers in APIs | done | Arc 只在论证处（Error source、`FontFace.bytes`、orgchart `Font`），见 Google disposition |
| M-DI-HIERARCHY | Prefer types over generics, generics over dyn traits | done | Google 类型节已 done；无 dyn 扩张 |
| M-ERRORS-CANONICAL-STRUCTS | Errors are canonical structs | N/A | ADAPT，规范 ERR-1、ERR-7、ERR-8：`Error` + 公开 `ErrorKind` / `Location` 与 orgchart `ErrorCode` 保持；不加 `Backtrace`，不私有化 kind。理由是批量调用方需要按 kind 穷尽匹配 |
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
| M-INTEGRATION-TESTS | Integration tests live under tests/ | done | 规范 TST-1、TST-2 |
| M-STRONG-TYPES | Use the proper type family | adopt | 规范 API-1、API-2；orgchart 页层字符串键与魔法前缀待改（规范 TRK-4） |
| M-STRONG-TYPES-GUARD | Newtypes guard their invariants | done | 规范 API-1、ARC-3 |
| M-BUILD-RESULT | Builders validate in final build() | N/A | 无 builder；M-INIT-BUILDER 联动 N/A |
| M-NO-GLOB-REEXPORTS | Don't glob re-export items | done | 生产 src 零 glob 重导出；crate-root 用显式 pub use 列表 |
| M-AVOID-STATICS | Avoid statics | done | 规范 ARC-5；无生产 static 可变状态 |
| M-LOG-NOT-PRINT | Production code uses telemetry, not println | done | 规范 ARC-7、PAN-7：`print_stdout`、`print_stderr`、`dbg_macro` 进 `[workspace.lints]`；生产零命中 |

## Libs/Building

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-OOBE | Libraries work out of the box | done | 默认 feature 可用；字体注入有文档说明 |
| M-SYS-CRATES | Native -sys crates compile without dependencies | N/A | 无 -sys 包 |
| M-FEATURES-ADDITIVE | Features are additive | done | 规范 DEP-5；`ci:feature-matrix` 已跑 `cargo hack --each-feature`，但只覆盖 pdf（规范 §16 缺口） |

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
| M-PANIC-CONTINUATION | Panic continuation is last resort | done | 规范 PAN-7：零 `catch_unwind`（评审） |
| M-PANIC-MESSAGE | Custom panics have a helpful message | done | 规范 PAN-2、PAN-3。`unwrap_used`、`expect_used`、`panic`、`unreachable` 在 `[workspace.lints]` 里 deny。已证明的不变量在原地 `#[expect(reason)]`。`price_lints.py` 对这四条为 0 |

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
| M-CARGO-WORKSPACE | Common settings come from the workspace Cargo.toml | done | 版本与依赖钉在根 `workspace.dependencies`，成员用 `workspace = true`。lint 钉在根 `[workspace.lints]`，成员只写 `[lints] workspace = true`（规范 TOOL-4） |
| M-CRATES-IN-WORKSPACE | The workspace lists and versions all crates | done | 四包同仓，版本各自独立（规范 VER-4） |
| M-CRATES-FLAT-FOLDER | All crates are siblings in one folder | done | crates/ 下四包平级 |
| M-LATEST-EDITION | New crates target latest edition | N/A | 锁定 edition 2021（规范 TOOL-1、§17）；升级须单独 PR |
| M-MSRV | MSRV is conservatively updated | done | 规范 TOOL-1、TOOL-2 与 [`msrv.md`](msrv.md)：MSRV = CI 工具链 = clippy `msrv` = 1.98 |

## Docs

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-FIRST-DOC-SENTENCE | First sentence is one line; approx. 15 words | done | 规范 DOC-2（首句靠评审） |
| M-MODULE-DOCS | Has comprehensive module documentation | done | 规范 DOC-2：公共模块 `//!`，`missing_docs` 四包 deny |
| M-CANONICAL-DOCS | Documentation has canonical sections | done | 规范 DOC-3：`missing_errors_doc`、`missing_panics_doc` 四包 deny；不加 Parameters 表 |
| M-DOC-INLINE | Mark pub use items with doc(inline) | done | 规范 API-10：crate 根与有意的 intra-crate `pub use` 加 `#[doc(inline)]`；禁 glob 重导出 |

## AI

| M-ID | 标题 | disposition | 证据/验收 |
| --- | --- | --- | --- |
| M-DESIGN-FOR-AI | Design with AI use in mind | done | 工程规范带规则号与执行方式；AGENTS 只放指针 |
| M-SINGLE-ITEM-PATH | Items are only visible through one path | adopt | 规范 API-10。ADAPT：pdf 的双路径（crate 根 + 模块路径）保留并加 `doc(inline)`，禁止新增未登记的双路径；家族清单见 `## 路径审计` |
| M-NO-META-DESIGN-DOCUMENTATION | Avoid meta design documentation | done | 规范 DOC-4 |
| M-TAUTOLOGICAL-TESTS | Tests do not assert ground truth | done | 规范 TST-2 |
| M-RUST-SHAPED | Rust code solves Rust problems | adopt | 规范 §14（Python 音译识别 PY-1 到 PY-22）与 §13（取巧写法）；现存反例见 §13 表 |

## 路径审计（M-SINGLE-ITEM-PATH ADAPT）

MS 原意：每个条目只经一条路径可见，避免 `crate::X` 与 `crate::mod::X` 并存造成文档与导入分叉。
本仓 ADAPT：`pdfcraft-pdf` 的 crate-root 重导出 + 定义模块路径均为已发布公共面，删任一方向都是 breaking，故双路径保留、两边都加 `#[doc(inline)]`；其余三包模块全私有，本就单路径。禁止新增无本节 ADAPT 行的双路径。

### 双路径家族清单

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

### 评审检查清单

1. 新增 `pub use` 是否复用了已在别处 `pub` 的条目名——是则必须同 diff 更新上表，否则打回。
2. `#[doc(inline)]` 是否只加在 intra-crate 本地类型 `pub use` 上（不用于 std/第三方）。
3. 是否删除了上表任一既有路径（禁止；双向保留）。

## 现存 lint 例外（规范 LINT-2）

现存的 `#[allow]` 在规范 §16 第 3 步改成代码位置上的 `#[expect(lint, reason)]`。只在部分 target 成立的用 `#[cfg_attr(<条件>, expect(..))]`。改完后例外只存在于代码里，本表不再维护清单。

| 位置 | lint | 处理 |
| --- | --- | --- |
| forms | `format_push_string` | 先按规范 STY-13 改 `write!`；确属提案书 §10 要求时登记 `expect`，`reason` 写明哪一条 |
| post `EncryptionConfig` | `struct_excessive_bools` | 登记 `expect`：镜像 PDF 安全位（规范 API-3 例外） |
| orgchart | `struct_field_names`、`many_single_char_names`、`float_cmp` | 逐处复核；`float_cmp` 按规范 DET-3 优先改 newtype |
| pdf `canvas.rs` `with_transform`、`geom.rs` `Affine::apply`、`Path::rotated` | `dead_code`（部分 target） | `cfg_attr` 加 `expect`，条件写成实际死掉的那组 target |
| pdf no-default 下的 `from_raw`、`ResolvedFont`、`resolve` | `dead_code`（feature 门控） | 按 feature 门控定义本身，而不是压 lint |

## 已知门禁红项与缺口

见规范 §16 末尾。

## Deferred

- **t-ms-asref**：公共 `AsRef` 签名。触发：某调用点具名举证非加法不可，且不破坏推断；否则保持 `&[u8]` / `Into`。
- **t-ms-perf**：容量、复用、收缩（M-THROUGHPUT、M-HOTPATH、M-MEM-REUSE、M-BOX-DST、M-SHRINK-TO-FIT、M-FAST-HASHER、M-INITIAL-CAPACITY）。触发：profile 热点证据。
- **Google t5 / t7 / t9**：见 [Google disposition](google-rust-style-disposition.md) 延后项。
