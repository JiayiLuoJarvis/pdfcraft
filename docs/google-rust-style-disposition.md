# Google Rust 风格指南 disposition 矩阵（Wave 1-4 收口）

裁决：Google-wins。产品语义冲突时提案书仍高于风格（见 `AGENTS.md`「Google Rust 风格裁决」节）。
范围：`pdfcraft-pdf` + `pdfcraft-forms` 全量做；`pdfcraft-orgchart` / `pdfcraft-post`
生产代码冻结，只跑门禁，列后续波。指南版本：https://google.github.io/styleguide/rustguide.html。

配置注：`rustfmt.toml` 只留稳定 1.92 可用项（`imports_granularity` 等为 nightly-only，
实测不生效，故不编码）；`clippy.toml` 保持阈值配置不变，lint 级别住在各包 `Cargo.toml`
`[lints]`（`pedantic` + pdf 的 `missing_errors_doc`/`missing_panics_doc` warn），改级别会动门禁
行为，故本波不碰，只在 `AGENTS.md` 裁决节记录规则。

## 1. 代码组织（Code organization）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| Prefer small crates [chrome] | N/A | Cargo workspace 四包结构已定，见 `docs/workspace-crates-spec.md`；偏差理由见 `AGENTS.md` 裁决节第 1 条 |
| flat file layout [chrome] | N/A | 同上；模块 DAG 由 `scripts/check_pdfcraft_pdf_modules.py` 锁定 |
| 文件/模块按主项命名、不在非叶模块写 API | done (pdf+forms) | 既有布局保持，无新增违规 |
| 不用 glob 导入 | done (pdf+forms) | 生产 src 零 glob；`tests/` 内 `use super::*` 保留。抽查基线：`crates/pdfcraft-pdf/src` 旧 13 处 glob 全在 tests |
| `imports_granularity = Module` | N/A（stable 1.92 为 nightly-only，实测 `cargo fmt` 报 warning 且不生效，故 `rustfmt.toml` 不编码；单模块粒度靠评审 + grep 抽查门禁，见 `AGENTS.md` 裁决节第 2 条） | `decorate.rs:163` 附带把 `check_content`/`stacked_height` 改模块限定调用；生产 glob 抽查为空 |
| trait 导入局部化、顶层函数模块限定、类型名非限定 | done (pdf+forms) | 同上，clippy 全绿 |
| `chromium::import!()` [chrome] | N/A | 非 Chromium 构建 |

## 2. 格式与美学（Code formatting）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| `max_width = 100`（稳定项） | done | `rustfmt.toml`；`cargo fmt --all -- --check` 退出 0 |
| `hex_literal_case` / `format_macro_matchers` / 不可见字符转义 | N/A | 不稳定选项，稳定 1.92 不用，见 `AGENTS.md` 裁决节第 5 条 |
| `#[rustfmt::skip]` 只留手动对齐/生成代码 | done | 全仓生产 src 零 `rustfmt::skip`（grep 空） |

## 3. 所有权与借用（Ownership）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| 不用 `Rc`/`Arc`，例外须论证 | done | `crates/pdfcraft-pdf/src/error.rs:52-56`（`Error.source` 论证注释）；`FontFace.bytes` / orgchart `FontBytes.data` 用 `Arc<Vec<u8>>` 使 `Clone` 不复制字体内容；post `Error.source` 论证注释已补（`crates/pdfcraft-post/src/error.rs:12-15`，`Error: Clone` 而 `Box<dyn StdError>` 不可 `Clone`） |
| 借用优先于深拷贝 | done | 无新增 `clone` 代借用；clippy pedantic 全绿 |

## 4. 函数出入参（Function inputs/outputs）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| 灵活引用（`&[T]`/`&str`/`Option<&T>`） | done (pdf+forms) | `decorate.rs:163`：`content: &Option<BandContent>` 改 `Option<&BandContent>`，调用点 `decorations.header.as_ref()`；生产 src 零 `&Vec<`/`&String`/`&Box<`/`&Option<` |
| 返回 `impl Iterator` vs `Vec` | done | 按调用分布选择并记录理由（见任务 6 记录） |
| 参数 `impl Into<T>`（调用点人体工学） | done | 关键调用点采用，应用层代码从简 |
| bool 改 enum | done + deferred | 含糊 bool 已改 enum；`FieldValue::CheckBox(bool)`（`crates/pdfcraft-forms/src/fields.rs:76`）保持 bool：调用点自解释且文档已澄清（true=checked）。见下「Deferred」t5。post（Q1 决议：保持 bool）：`merge_pdfs(parts, preserve_outlines: bool)` 不提 `OutlinePolicy` enum，`merge.rs` 文档已写明取舍并要求调用点具名实参/注释；`EncryptionConfig` 8 bool 保留（镜像 PDF 安全位，`struct_excessive_bools` allow 保留）。orgchart（Q3 确认：deferred）：`measure.rs:341/349/357 is_subject` + `captions.rs:60 has_investments` 保持 bool，全私有、同一文件内调用点自解释、无调用方收益 |

## 5. 错误报告（Reporting errors）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| Option / Result / panic 三选 | done (pdf+forms) | `crates/pdfcraft-pdf/src/error.rs` 模块文档「Reporting model」节：`LinkTarget::uri_str` 回 `None`、`Error`/`Errors` 走 `Result`、`Errors::first` 是唯一公共 panic（`# Panics` 有档）；依赖失败走 `source()`。`crates/pdfcraft-forms/src/error.rs` 同模型。行为零变化，错误单测语义无漂移 |
| orgchart `ErrorCode` 模型 / post `Error` | done（orgchart：N/A-独立模型；post：`Error::new` 已补） | orgchart：`ErrorCode` 8 值稳定码（`as_str`，`const fn` + `#[must_use]`）+ `message` 只给日志，匹配 Python `EquityErrorCode.value` 穷尽匹配；`Error::new` 保持 `pub(crate)`，不加 `#[non_exhaustive]`（加了会破坏穷尽匹配，IS-2）。post（Q2 落地）：`Error::new(kind)` + `#[must_use]` + rustdoc Example 已补（`error.rs`），`From<ErrorKind>` 转发给它，行为不变；`ErrorKind` 6 变体 `#[non_exhaustive]` 未改动 |

## 6. 类型（Types）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| newtype vs 别名、命名字段优先 | done | 品牌长度类型（`Positive`/`NonNegative`/`Finite`，`geom.rs`）保持；坐标断言全过 |
| 派生优先、已知 std trait、泛型 vs dyn、扩展 trait `Ext` | done | clippy `should_implement_trait` / `new_without_default` 无警告 |

## 7. Trait 专项（Specific guidance）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| `From` 配显式构造 | done | `Error::new(kind)` 配 `From<ErrorKind>`、`Errors::from_error(error)` 配 `From<Error>`（pdf）；forms `Error::new(kind)` 配 `From<ErrorKind>`；post `Error::new(kind)` 配 `From<ErrorKind>`（`error.rs`，各有 rustdoc Example；post 的 `From` 转发给 `new`，行为不变） |
| Options struct 实现 `Default` | done | forms `AbsoluteLine` 得 `Default`（与 `new(0,0,0,0)` 同笔触）；`FillOptions` 本就支持 `..Default::default()` |

## 8. 并发（Concurrency）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| 毒化 Mutex 不触及 | N/A | 本库无全局锁 |
| 不用 `async` / 起线程 [chrome]、用 `fuchsia_sync::Mutex` [Fuchsia] | N/A | 见 `AGENTS.md` 裁决节第 6 条 |

## 9. 宏（Macros）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| 常规函数优先，`macro_rules!` 优先于过程宏 | done | 无新增宏；无过程宏 |

## 10. 命名（Naming）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| 方法名跟 std（`as_`/`to_`/`into_` + self 约定） | done | clippy `wrong_self_convention` 全绿；`Errors::len`/`is_empty` 配对补齐（含 `len_without_is_empty`） |
| 生命期/泛型/初始化函数/缩写、getter-setter 规整 | done | 消灭非通用缩写；访问器按 `foo`/`foo_mut`/`take_`/`set_`/`with_` 规整 |

## 11. 文档（Documentation）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| `///` + `Errors`/`Panics` + 示例 | done (pdf+forms) | `cargo doc` 零警告（`RUSTDOCFLAGS=-D warnings`）；`missing_docs` 保持 `warn`，spec 稳定后升 `deny` |
| `#[must_use]`（函数与类型） | done + deferred | pdf spec/mod.rs 与 canvas 主要类型和方法已带，types.rs:103 起（`Rendered` 及方法）亦有；types.rs:13 `Numbering`、:38 `Ratio`、:59 `Track` 与 canvas.rs `ImageFormat` 四处未带，属冻结文档语言类型，随 Deferred t7 留后波；forms `fields.rs` 九个公共类型全 `#[must_use]`；`Errors::iter` 等经 `clippy::double_must_use` 确认无需重复标注；post（Q2 落地）：`Error::new` 加 `#[must_use]`，`Pipeline::run` 补 `#[must_use]` + `# Errors`；其余 `run/apply` 类此前已带；`Step` 不加 `Default`（N/A：无空操作语义，中性元是空 `Pipeline`，`pipeline.rs` 已注）；orgchart 查漏结论：公共 `pub fn` 已全覆盖（`model.rs` 6 处 + `error.rs` `as_str`），无漏项，`document_title` 补 `# Errors`（`SubjectNotFound`） |
| `#[non_exhaustive]` | done + deferred | forms `FieldType` 既有 `#[non_exhaustive]`（本波补 `#[must_use]`）；`FieldInfo`/`FieldValue` 本波新增 `#[non_exhaustive]`。`FieldValue` 用通配臂迁移；`FieldInfo` 是 struct，外部不可 struct literal（E0639），pattern 用 `..`（如 `FieldInfo { name, .. }`）。pdf error/canvas/geom 零新增，基线与本波同为 13 处（error 8、canvas 3、geom 2，属历史存量）。pdf 文档语言 enum 见下「Deferred」t7。post `ErrorKind` 的 `#[non_exhaustive]` 未改动（6 变体，新增变体非 breaking）；orgchart `ErrorCode` 刻意不加（8 值稳定码 + Python `EquityErrorCode.value` 穷尽匹配是特性，IS-2） |

## 12. 其他语言特性（Other Rust features）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| 无 unsafe（forbid）、无 unstable feature | done | 整仓 `forbid(unsafe_code)`；`grep -rn unsafe\|feature(` 生产 src 为零（forbid 行除外） |
| `const fn` 负责使用、重载只做数学语义、不碰 `doc(hidden)`、main 前不跑代码 | done | 无新增违反 |

## 13. 实现关切（Implementation concerns）

| 指南条 | disposition | 证据 |
| --- | --- | --- |
| `expect` 带上下文（不用裸 `unwrap`） | done (pdf+forms+post+orgchart) | 生产 src（`#[cfg(test)]`/`tests/` 除外）零 `.unwrap()`；断言补上下文字符串；`missing_panics_doc`（pdf warn）无新增。orgchart 生产零 `.expect`/`.unwrap()` 复核通过：`prepare.rs`/`graph.rs`/`display.rs` 的 `.expect` 命中全在 `#[cfg(test)] mod tests` 内，`lib.rs:51` 系 rustdoc 注释行；post 生产零 `.expect`（grep 空） |
| 模式匹配优先于 check-then-unwrap、直白 for 循环、`format!`/`write!`、零 `catch_unwind`/`todo!` | done | clippy + 测试全过 |

## Known gate gaps（基线同现，不掩盖）

以下组合不是本分支引入；它们是 `AGENTS.md` 默认 `fmt`/`clippy`/`test`/`doc` 门禁之外的已知红项。本文中的“全绿”只指默认组合，不含本节。

- **no-default-features**：`cargo clippy -p pdfcraft-pdf --all-targets --no-default-features --features test-utils -- -D warnings` 与 `cargo test -p pdfcraft-pdf --no-default-features --features test-utils` 在本分支与 pristine `f3c23b2` 都是 exit 101。两者同样报 3 个 dead-code errors（`from_raw`、`ResolvedFont`、`resolve`）；test 另因 examples/tests 的 `to_pdf` 未解析而失败。CI 仍运行该组合（`.github/workflows/ci.yml:138,147,153`），在基座即红。
- **字体/CJK**：未设 `PDFCRAFT_*` 时字体相关用例提前返回，默认 447 passed 不是完整字体覆盖。设 `PDFCRAFT_PDF_FONT=DejaVuSans` 后 pdf 为 269 passed / 9 failed / 1 ignored，9 个失败名与基座一致。`pdfcraft-forms` 的 CJK 条件测试在 DejaVu 下跳过，用 `wqy-zenhei.ttc` 时在 `fill_and_flatten.rs:597` 报 `FontFile2 is 813408 bytes`；`fills_chinese_text` 仍为 `#[ignore]`（`:603`）。CI 的 NotoSansCJK 才能覆盖；本机没有该字体。
- 上述 gap 的修复（no-default gating、字体夹具/CI 字体）不在本波范围；本波只记录，避免把默认组合的绿误报为全矩阵绿。
- orgchart/post Wave 3（分支 `review/google-style-orgchart-post`）重跑同口径矩阵：8 命令门禁 exit 0，doc 零 warning；`--no-default-features` clippy 仍 exit 101（同 3 个 dead-code errors：`from_raw`、`ResolvedFont`、`resolve`），环境仍无 `PDFCRAFT_*` 变量，字体/CJK 覆盖仍以基线为准——均记 base-identical，非回归。证据见 `.omo/evidence/task-6-gates/`（`SUMMARY.md` + 各命令 `.log`/`.exit`）。

## Deferred（延后项，有理由）

- **t7：pdf 文档语言 enum 不加 `#[non_exhaustive]`。** `crates/pdfcraft-pdf/src/spec/mod.rs` 的
  `Block`、`Alignment`（:272）、`InlineRun`、`BlankWidth`、`CellBlock`、`ListKind`、`CanvasMark`、
  `BandSlot`、`TextRun`、`PageWhen`、`LayerMark`、`LayerShape` 与 `types.rs` 的 `Numbering`、`Track`
  描述提案书已确认的文档语言（§2、§12），调用方 `match` 保持穷尽才能在加字段时得编译期提醒。
  加 `non_exhaustive` 会强制通配臂，削弱冻结 DSL 的 total-match 保证。改须先修提案书，列后续波。
- **t5：`FieldValue::CheckBox(bool)` 改 `CheckboxState` 提案修正案。** 当前 bool 自解释
  （`fields.rs:76` 文档：true=checked 画勾，false=unchecked 不画），调用点注释足够。
  若后波要 enum，须先向提案书 §10 提修正案（`CheckboxState::Checked/Unchecked`），再改签名与迁移说明。
- **t9：文档小项。** `emit.rs:271` `check_upem` 的 upem==0 路径说明、`Positive::pt` 的零值错误注记，
  随 spec 文档稳定化（`missing_docs` 升 `deny`）时补。
- **orgchart / post 后续波（已收口，零 breaking）。** Q1（post `merge_pdfs` bool）：保持 bool，`merge.rs` 文档写明取舍 + 调用点具名要求；`EncryptionConfig` 8 bool 保留（镜像 PDF 安全位）。Q2（post `must_use`）：`Error::new` 加 `#[must_use]` + Example，`Pipeline::run` 补 `#[must_use]` + `# Errors`；`Default` 均已有，`Step` 记 N/A（无空操作语义）。Q3（orgchart 私有 bool）：deferred，`measure.rs:341/349/357` + `captions.rs:60` 保持 bool。`ErrorCode` 8 值 + 无 `non_exhaustive`（Python 穷尽匹配是特性）、`Error::new` 保持 `pub(crate)`（调用方不可构造，无迁移）；post `ErrorKind non_exhaustive` 未改动。证据：`.omo/evidence/task-6-gates/`（8 命令门禁 + 抽查 grep + T1-T6，缺口记 base-identical）。
