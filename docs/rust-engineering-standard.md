# pdfcraft Rust 工程规范（草案）

状态：草案，待确认。适用于整个 workspace 的四个包：`pdfcraft-pdf`、`pdfcraft-forms`、`pdfcraft-orgchart`、`pdfcraft-post`。

本文管「代码怎么写、工程怎么组织」。产品范围与行为不在这里，以 [`提案书.md`](提案书.md) 为准。

## 0. 地位与读法

**优先级。** 提案书（产品）> [`workspace-crates-spec.md`](workspace-crates-spec.md)（包边界）> 本文 > 设计稿未确认部分。本文不能推翻提案书已确认的 API 与行为。写法层的规则只在本文出现一次。`AGENTS.md`、`CONTRIBUTING.md` 只放指针。两份 disposition（[Google](google-rust-style-disposition.md)、[Microsoft](ms-rust-guidelines-disposition.md)）只做「外部条目 → 本文规则号」的映射与证据，不再各写一份规则。

**来源顺序。** 写法冲突时按此顺序取舍：

1. SG：[Rust Style Guide](https://doc.rust-lang.org/style-guide/)
2. API：[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
3. MS：[Microsoft Pragmatic Rust Guidelines](https://microsoft.github.io/rust-guidelines/)
4. G：[Google Rust 风格指南](https://google.github.io/styleguide/rustguide.html)。带 `-chrome` / `-fuchsia` 锚点的条目不适用，见 §15。

另用 SV = [Cargo SemVer](https://doc.rust-lang.org/cargo/reference/semver.html)，FEAT = [Cargo features](https://doc.rust-lang.org/cargo/reference/features.html)，CL = [Clippy lints](https://rust-lang.github.io/rust-clippy/master/)，BOOK = [The Book ch11-03](https://doc.rust-lang.org/book/ch11-03-test-organization.html)。

**执行方式。** 每条规则写明执行方式，取值只有以下几种：

| 记号 | 含义 |
| --- | --- |
| `fmt` | `cargo fmt --all -- --check`，配置在 `rustfmt.toml` |
| `lint:<名>` | 根 `Cargo.toml` `[workspace.lints]` 中的 rustc 或 clippy lint，CI 以 `-D warnings` 跑 |
| `clippy.toml` | clippy 配置项 |
| `script:<文件>` | `scripts/` 下的检查脚本，CI 调用 |
| `ci:<job>` | `.github/workflows/ci.yml` 中的 job |
| `deny` | `cargo deny check`，配置在 `deny.toml` |
| 评审 | lint 抓不到，由评审者对照本文执行 |

能用 lint 强制的规则一律用 lint。只写「评审」的规则，是因为 1.98 上没有可用的 lint，或可用的 lint 误报到不可接受（见 §14 拒绝表）。

**规则号。** `<章>-<序号>`，例如 `PAN-2`。评审意见引用规则号，不复述规则。

## 1. 工具链与门禁（TOOL）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| TOOL-1 | 工具链、MSRV、clippy `msrv` 三者相同，当前是 **1.98**。edition 保持 2021。 | MS M-MSRV；SV#env-new-rust | `rust-version`（根 `Cargo.toml`）、`clippy.toml msrv`、`ci.yml toolchain` 三处一致，评审 |
| TOOL-2 | 升工具链单独一个 PR。PR 内按 LINT-3 重新定价 lint，清掉新 lint 与 rustdoc 的命中和失效的 `#[expect]`，并按 [`msrv.md`](msrv.md) 改齐所有版本号出现处。 | MS M-LINT-OVERRIDE-EXPECT | 评审 |
| TOOL-3 | 不用 `unsafe`。 | MS M-UNSAFE | `lint:unsafe_code`（forbid） |
| TOOL-4 | lint 配置只在根 `[workspace.lints]`。成员 crate 只写 `[lints] workspace = true`。单包例外不开表，写在代码位置上（见 LINT-2）。 | MS M-CARGO-WORKSPACE | 评审；`[lints]` 下出现 `workspace` 以外的键即打回 |
| TOOL-5 | 门禁命令只在本节列出一次。其他文档引用本节。 | 本仓库去重 | 评审 |

**合并前门禁（TOOL-5）。** 与 `ci.yml` 的 `basics`、`test`、`deny` 三个 job 逐条对应。CI 设 `RUSTFLAGS=-D warnings`、`RUSTDOCFLAGS=-D warnings`，本地同设。CI 用 nextest 加 `cargo test --doc`；本地用 `cargo test`，两者覆盖相同。

```bash
export RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings"
python3 scripts/check_pdfcraft_pdf_modules.py
python3 scripts/check_readme_identity.py
cargo fmt --all -- --check
cargo clippy -p pdfcraft-forms -p pdfcraft-orgchart -p pdfcraft-post --all-targets -- -D warnings
cargo clippy -p pdfcraft-pdf --all-targets --no-default-features --features test-utils -- -D warnings
cargo clippy -p pdfcraft-pdf --all-targets --features backend-krilla,test-utils -- -D warnings
cargo test -p pdfcraft-forms
cargo test -p pdfcraft-post
cargo test -p pdfcraft-orgchart
cargo test -p pdfcraft-pdf --no-default-features --features test-utils
cargo test -p pdfcraft-pdf --features backend-krilla,test-utils
cargo doc -p pdfcraft-pdf --no-deps --no-default-features
cargo doc -p pdfcraft-pdf --no-deps --features backend-krilla
cargo doc -p pdfcraft-forms -p pdfcraft-orgchart -p pdfcraft-post --no-deps
cargo deny check advisories licenses bans sources
```

`pdfcraft-forms` 的填写测试需要 `pdftotext`，字体相关测试读 `PDFCRAFT_*` 环境变量（见 `ci.yml` 的 Resolve test fonts）。CI 另跑 `minrust`、`semver-checks`、`feature-matrix`、`minimal-versions` 四个 job。

改 CI 的门禁时，同一个 PR 改本节。

**小步检查。** 一个 PR 拆成多步时，中间步只跑该包的 `fmt --check`、`clippy --all-targets -- -D warnings` 和这一步新增的测试。合并前跑完整门禁。

## 2. 代码风格（STY）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| STY-1 | 格式完全交给 rustfmt。`rustfmt.toml` 只用稳定项。`#[rustfmt::skip]` 只用于手工对齐的表和生成代码，并带一行理由。 | SG；G（Code formatting） | `fmt`；skip 靠评审 |
| STY-2 | 命名遵循 RFC 430：类型 `UpperCamelCase`，其余 `snake_case`，常量 `SCREAMING_SNAKE_CASE`。缩写词按单词处理（`PdfError`，不写 `PDFError`）。 | API#c-case；SG/advice | rustc 命名 lint；`lint:clippy::upper_case_acronyms`（style） |
| STY-3 | 转换方法用 `as_`（借用、廉价）、`to_`（昂贵或拷贝）、`into_`（消耗 self）。getter 不加 `get_` 前缀。 | API#c-conv、#c-getter；G#naming-methods | `lint:clippy::wrong_self_convention`；getter 靠评审 |
| STY-4 | 不缩写（`cfg`、`ctx` 这类全生态通用词除外），不用空泛词（`Manager`、`Helper`、`Util`、`Data`、`Info` 作后缀）。文件和叶子模块按其中主要条目命名，不建 `utils.rs`、`helpers.rs`、`common.rs`（测试的 `tests/common/` 除外）。 | G#avoid-abbreviations、#name-source-files…；MS M-WEASEL-WORDS、M-SHORT-NAMES | 评审 |
| STY-5 | 标识符只用 ASCII。 | 本仓库（与 DOC-1 一致） | `lint:non_ascii_idents` |
| STY-6 | 不用 glob 导入。测试模块里的 `use super::*;` 除外。 | G#dont-use-glob-imports | `lint:clippy::wildcard_imports`、`lint:clippy::enum_glob_use`（pedantic） |
| STY-7 | 一条 `use` 只导入一个模块里的条目。trait 导入放在用到它的函数里。跨模块的顶层函数带模块名调用（`layout::paginate(..)`）。类型导入后不带路径使用。 | G#a-use-statement…、#prefer-locally-scoped-trait-imports、#qualify-calls…、#type-names… | 评审（`imports_granularity` 仍是 nightly 选项） |
| STY-8 | 不写多余的路径限定。 | SG | `lint:unused_qualifications` |
| STY-9 | 用表达式写法：`let x = if ..`、`let x = match ..`，不先声明再赋值。 | SG/advice | `lint:clippy::needless_late_init`（style） |
| STY-10 | 不重用名字表达另一件事。遮蔽只允许同一个值换形态（`let s = s.trim();`）。 | 本仓库 | `lint:clippy::shadow_unrelated` |
| STY-11 | 类型推断不跨越函数边界去猜。读者需要猜才能知道类型的 `let`、`collect()`、`parse()` 写出类型。 | G#dont-overuse-type-inference | 评审 |
| STY-12 | 组合子链超过三四步、或要在链里处理错误时，改写成 `for` 循环。 | G#dont-use-iterator-combinator-chains… | 评审 |
| STY-13 | 拼字符串用 `format!` / `write!`，不用 `+` 和 `push_str(&format!(..))`。 | G#prefer-format--write… | `lint:clippy::string_add`、`lint:clippy::format_push_string` |
| STY-14 | 函数参数不超过 7 个。超过就用命名字段的 Options struct。 | G#options-structs…；CL too_many_arguments | `lint:clippy::too_many_arguments`。目标阈值是默认 7；`clippy.toml` 暂留 8，因为它立刻作用于现有门禁，生产 12 处清完后删掉该行（§16 第 5 步） |
| STY-15 | 模式里不写 `ref` / `ref mut`，靠绑定模式。不写 `field: _`，用 `..`。完全绑定的 struct 模式不写 `..`。 | SG；CL | `lint:clippy::ref_patterns`、`lint:clippy::unneeded_field_pattern`、`lint:clippy::rest_pat_in_fully_bound_structs` |
| STY-16 | 不写不必要的生命周期。路径里省略的生命周期写成 `'_`。 | API；rust_2018_idioms | `lint:rust_2018_idioms`、`lint:unused_lifetimes`、`lint:single_use_lifetimes`、`lint:redundant_lifetimes` |
| STY-17 | 一个 crate 内不同模块形态一致：目录模块用 `mod.rs`。 | 本仓库现状（模块 DAG 脚本依赖固定路径） | `lint:clippy::self_named_module_files` |

## 3. Panic、`expect` 与断言（PAN）

拍板结论：方案 A 加 lint。公共与协议边界零 panic；内部已证明的不变量允许 panic，但每一处都要显式登记。

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| PAN-1 | 区分三种失败。调用方可以合理遇到的失败返回 `Result`。合法的「没有」返回 `Option`。只有程序自身的 bug 才 panic。 | G#reporting-errors；MS M-PANIC-ON-BUG、M-PANIC-IS-STOP | 评审 |
| PAN-2 | 生产代码不用 `unwrap()`。 | G#prefer-expect-over-unwrap；MS M-PANIC-MESSAGE | `lint:clippy::unwrap_used`；测试由 `clippy.toml allow-unwrap-in-tests` 放行 |
| PAN-3 | 生产代码的 `expect`、`panic!`、`unreachable!` 只用于已证明的内部不变量。每一处在所在语句或函数上写 `#[expect(clippy::expect_used, reason = "<不变量为何成立>")]`（`panic` / `unreachable` 同理）。消息写成「期望成立的事实」，如 `"page stack is non-empty"`。 | MS M-PANIC-MESSAGE；G#prefer-expect… | `lint:clippy::expect_used`、`lint:clippy::panic`、`lint:clippy::unreachable`；理由靠评审 |
| PAN-4 | 加 `#[expect]` 之前先看能不能从类型上消掉这个 panic：常量用 `const` 构造，「长度为 1」用切片模式，「非空」用非空类型或在构造处保证。 | API#c-newtype；MS M-STRONG-TYPES-GUARD | 评审 |
| PAN-5 | 公共函数可能 panic 时，rustdoc 写 `# Panics`。 | API#c-failure | `lint:clippy::missing_panics_doc` |
| PAN-6 | 断言带上下文消息。 | G#use-context-strings…；MS M-PANIC-MESSAGE | `lint:clippy::missing_assert_message` |
| PAN-7 | 不用 `catch_unwind`。不写 `todo!`、`unimplemented!`、`dbg!`。库代码不调用 `process::exit`。 | G#dont-call-paniccatch_unwind；MS M-PANIC-CONTINUATION | `lint:clippy::todo`、`lint:clippy::unimplemented`、`lint:clippy::dbg_macro`、`lint:clippy::exit`；`catch_unwind` 靠评审 |
| PAN-8 | 数组和切片下标必须由类型保证（arena 的类型化 id），或在同一函数内已检查长度，否则用 `get`。 | 本仓库拍板；MS M-PANIC-ON-BUG | 评审。`indexing_slicing` 暂不开，生产命中 287 处，登记在 §16 |

测试与示例：`clippy.toml` 放行测试函数和 `#[cfg(test)]` 模块里的 `unwrap`、`expect`、`panic` 和打印。`tests/` 里 `#[test]` 之外的辅助函数不在放行范围内，在文件头写 `#![expect(clippy::expect_used, reason = "integration test: a panic is the failure report")]`。示例是给调用方看的程序，`main` 返回 `Result` 并用 `?`，不 `unwrap`。

## 4. 错误模型（ERR）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| ERR-1 | 每个包有自己的错误类型，不共用 error crate。`pdfcraft-pdf`：`Error` / `Errors`，位置放可选的 `Location`。`pdfcraft-forms`、`pdfcraft-post`：单条 `Error` 加 `ErrorKind`。`pdfcraft-orgchart`：`ErrorCode` 加只给日志看的 `message`，调用方匹配 `ErrorCode::as_str`。 | workspace-crates-spec；MS M-ERRORS-CANONICAL-STRUCTS（部分采纳，见 §15） | 评审 |
| ERR-2 | 错误类型实现 `std::error::Error + Send + Sync + 'static`，并有编译期断言测试。依赖的错误经 `source()` 可取。 | API#c-good-err；MS M-TYPES-SEND | 编译期断言测试（现状：只有 `spec` 有 `assert_send_sync`，其余三包待补，见 §16） |
| ERR-3 | `Display` 小写开头，句末不加标点，不带 `Error:` 前缀，不重复 `source()` 的内容。 | API#c-good-err | 评审 |
| ERR-4 | 不丢弃有载荷的底层错误。`map_err(\|_\| ..)`、`.ok()`、`unwrap_or_default()` 只用于两种情况：错误本身无载荷（如 `TryFromIntError`），或「缺省即合法」是写明的语义。 | 本仓库；MS M-FROM-ERROR | 评审（`map_err_ignore` 分不清有无载荷，不开，见 §14） |
| ERR-5 | 错误转换用 `From`，同时提供显式构造函数（`Error::new(kind)`）。 | MS M-FROM-ERROR；G（Trait 专项，见 Google disposition §7） | 评审 |
| ERR-6 | 公共函数返回 `Result` 时，rustdoc 写 `# Errors`，列出各个 kind。 | API#c-failure | `lint:clippy::missing_errors_doc` |
| ERR-7 | 错误 kind 的变体代表调用方会分别处理的情况。载荷用结构化字段，不用拼好的字符串。`pdfcraft-forms`、`pdfcraft-post` 的 `ErrorKind` 是 `#[non_exhaustive]`。orgchart 的 `ErrorCode` 是给调用方穷尽匹配的稳定码闭集，不加 `non_exhaustive`，理由写「调用方按稳定码穷尽匹配」，不写「对齐 Python」。 | API#c-good-err；SV#enum-variant-new | 评审 |
| ERR-8 | 不加 `Backtrace`。 | 本仓库（§15） | 评审 |

## 5. 类型与公共 API（API）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| API-1 | 有语义的原始值（长度、比例、页码、id）用 newtype。构造可能失败时返回 `Result` / `Option`，构造后不变量一直成立。 | API#c-newtype；MS M-STRONG-TYPES、M-STRONG-TYPES-GUARD；G#newtypes-versus-type-aliases | 评审 |
| API-2 | 内部标识用类型化 id（`NodeId(u32)` 之类）加 arena，不用 `String` 作键，不拼前缀字符串表达类别。 | API#c-newtype；MS M-STRONG-TYPES | 评审（§13 TRK-4） |
| API-3 | 含义不明的 `bool` 参数改 enum。保留 `bool` 时调用点能自解释，或已在提案书或 disposition 里登记（`merge_pdfs`、`FieldValue::CheckBox`、`EncryptionConfig`）。 | API#c-custom-type；G#prefer-enum-over-bool… | `lint:clippy::fn_params_excessive_bools`、`lint:clippy::struct_excessive_bools`（pedantic） |
| API-4 | 多个返回值用命名字段的 struct，不返回三项以上的元组。 | G#prefer-named-fields… | 评审 |
| API-5 | 参数收 `&[T]`、`&str`、`Option<&T>`，不收 `&Vec<T>`、`&String`、`&Option<T>`、`&Box<T>`。只迭代的入参收 `impl IntoIterator`。 | G#accept-flexible-references… | `lint:clippy::ptr_arg`、`lint:clippy::ref_option`（pedantic） |
| API-6 | 私有模块里的条目写 `pub(crate)`，不写 `pub`。`pub` 只给真正对外的条目。 | API#c-struct-private；本仓库拍板 | `lint:unreachable_pub`、`lint:unnameable_types`、`lint:clippy::pub_without_shorthand` |
| API-7 | 公共类型实现 `Debug`，输出非空。按语义实现 `Clone`、`PartialEq`、`Eq`、`Hash`、`Default`。能 derive 的不手写。 | API#c-common-traits、#c-debug；MS M-PUBLIC-DEBUG | `lint:missing_debug_implementations`、`lint:clippy::missing_fields_in_debug`、`lint:clippy::derivable_impls` |
| API-8 | 重要返回值和构造器加 `#[must_use]`。 | G#must_use…；API | `lint:clippy::must_use_candidate`、`lint:clippy::return_self_not_must_use`（pedantic） |
| API-9 | 对外 enum 和带公有字段的 struct 加 `#[non_exhaustive]`。三类例外：描述既有本体的闭集，调用方需要穷尽匹配的文档语言 enum（提案书 §2 / §12 已确认），以及要保留 `..Default::default()` 初始化的 Options struct（`FillOptions`、`DocumentSpec`）。例外登记在 Google disposition 的延后项里。 | G#apply-non_exhaustive…；SV#attr-adding-non-exhaustive | 评审（`exhaustive_*` 不开，见 §14） |
| API-10 | crate 根的 `pub use` 加 `#[doc(inline)]`。不 glob 重导出。新增双路径须在 MS disposition 的路径审计表登记。 | MS M-DOC-INLINE、M-NO-GLOB-REEXPORTS、M-SINGLE-ITEM-PATH | 评审 |
| API-11 | 公共签名不出现依赖的类型（krilla、lopdf、skrifa、hayro、image）。 | MS M-DONT-LEAK-TYPES；API#c-stable | 评审 |
| API-12 | `Rc` / `Arc` 只在借用结构会不合理地复杂时用，并在旁边写一行论证。现有论证：错误的 `source`（`Error: Clone`）、`FontFace.bytes` 与 `FontBytes.data`（克隆不复制字体内容）。 | G#avoid-rc-and-arc；MS M-AVOID-WRAPPERS | `lint:clippy::clone_on_ref_ptr`、`lint:clippy::rc_mutex`；论证靠评审 |
| API-13 | `Deref` 只表示智能指针语义，不用来模拟继承。 | API#c-deref；[`deref-flow-api.md`](deref-flow-api.md) | 评审 |
| API-14 | 公共 API 落地前按 [API checklist](https://rust-lang.github.io/api-guidelines/checklist.html) 走一遍。 | API checklist | 评审 |

## 6. 架构（ARC）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| ARC-1 | 四包边界锁定，互不依赖，见 workspace-crates-spec。不把 forms / orgchart / post 做成 `pdfcraft-pdf` 的 feature。 | workspace-crates-spec | Cargo（crate 环编译失败）；`deny`；评审 |
| ARC-2 | 模块依赖是 DAG，同一 crate 内的模块之间也不许有环。 | [`module-dependency.md`](module-dependency.md) | `script:check_pdfcraft_pdf_modules.py`（只覆盖 pdf），其余三包靠评审（缺口，见 §16） |
| ARC-3 | 在系统边界把外部数据一次解析成内部类型（调用方传入的 spec、字体字节、已有 PDF）。内部函数信任类型，不重复校验。 | API#c-validate；MS M-STRONG-TYPES-GUARD | 评审 |
| ARC-4 | `krilla` 只出现在 `pdfcraft-pdf` 的 `backend/` 和 `pdfcraft-orgchart` 里。`lopdf` 只出现在 forms 与 post。 | AGENTS；workspace-crates-spec | `script:check_pdfcraft_pdf_modules.py`；评审 |
| ARC-5 | 不用可变 `static`、全局缓存、`OnceLock` 单例。状态显式传递。 | MS M-AVOID-STATICS | 评审 |
| ARC-6 | 不用 `Box<dyn Any>` 加 downcast 绕过类型设计。 | MS M-DI-HIERARCHY | 评审（现存一处，见 §16） |
| ARC-7 | 库代码不打印，不引入日志框架。 | MS M-LOG-NOT-PRINT（日志部分 N/A） | `lint:clippy::print_stdout`、`lint:clippy::print_stderr` |
| ARC-8 | 不异步，不起线程，不并行开页或画页。 | AGENTS；G（chrome 条 N/A） | 评审 |

## 7. 确定性与批量生产（DET）

同样的输入必须产出同样的字节。批量调用方依赖这一点做比对与缓存。

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| DET-1 | 迭代顺序会影响输出的集合用 `BTreeMap` / `BTreeSet` 或 `Vec`。迭代 `HashMap` / `HashSet` 的地方写 `#[expect(clippy::iter_over_hash_type, reason = "<为何顺序不影响结果>")]`。 | MS M-MOCKABLE-SYSCALLS（引申） | `lint:clippy::iter_over_hash_type` |
| DET-2 | 输出不依赖时钟、随机数、环境变量、文件系统遍历顺序。需要时间或 id 时由调用方传入。`getrandom` 只用于调用方显式要求的加密材料。 | 本仓库 | 评审 |
| DET-3 | 浮点不直接 `==`。需要可哈希或可比较的浮点，定义只接受有限值的 newtype，在里面实现 `Eq` / `Hash` / `Ord`。不在外面用 `to_bits()` 当键。 | CL float_cmp | `lint:clippy::float_cmp`、`lint:clippy::float_cmp_const`（pedantic、restriction）；`to_bits` 靠评审 |
| DET-4 | 不写会悄悄丢精度的浮点字面量。数值转换见 CONV。 | CL lossy_float_literal | `lint:clippy::lossy_float_literal` |
| DET-5 | 构建与测试使用提交进仓库的 `Cargo.lock`，CI 带 `--locked`。 | FEAT；本仓库 | `ci:minrust` |

## 8. 数值转换（CONV）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| CONV-1 | 无损的数值加宽用 `From`。可能失败的用 `TryFrom`，并按 ERR-4 处理失败。 | API#c-conv-traits | `lint:clippy::cast_lossless`、`cast_possible_truncation`、`cast_sign_loss`、`cast_possible_wrap`、`cast_precision_loss`（pedantic）；`lint:trivial_numeric_casts` |
| CONV-2 | `as` 只剩两种用途：浮点与整数之间、协议规定截断的转换（每处 `#[expect(clippy::cast_…, reason)]`）；以及转成 `dyn Trait`。 | API#c-conv-traits | 同上；`as_conversions` 不开（见 §14） |
| CONV-3 | 不用 `Option::unwrap_or(u32::MAX)` 这类哨兵值兜住失败的转换。 | 本仓库 | 评审（§13 TRK-2） |

## 9. 依赖与 feature（DEP）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| DEP-1 | 依赖只写在根 `[workspace.dependencies]`，成员用 `workspace = true`，feature 写在使用处。未用到的依赖不写。 | MS M-CARGO-WORKSPACE | 评审；`deny`（bans） |
| DEP-2 | 字体、PDF、渲染这条栈精确钉版（`=x.y.z`）：`krilla`、`skrifa`、`read-fonts`、`lopdf`、`png`、`hayro`、`image`。它们决定坐标与字形。改这些钉版单独一个 PR，并重跑坐标与字形相关测试。 | 本仓库（`emit::check_upem`、`TJ` 夹具） | 评审；`deny`（bans 的 `skip` 必须写理由） |
| DEP-3 | 不影响测量与绘制的叶子工具用 caret（`getrandom`、`unicode-segmentation`），下游解析更宽松。确定性由 DET-5 的 lock 文件保证。 | Cargo specifying-dependencies#caret | `ci:minimal-versions` 检查 caret 下限可编译 |
| DEP-4 | 不用 `*` 版本和 git 依赖，不 fork krilla。 | 本仓库 | `lint:clippy::wildcard_dependencies`；`deny`（`wildcards = "deny"`、`unknown-git = "deny"`） |
| DEP-5 | feature 只做加法。feature 名不用 `no-` 前缀和 `use-` / `with-` 前缀。每个 feature 单独能编译。 | FEAT#feature-unification；MS M-FEATURES-ADDITIVE | `lint:clippy::negative_feature_names`、`lint:clippy::redundant_feature_names`；`ci:feature-matrix`（现只覆盖 pdf，见 §16） |
| DEP-6 | 安全公告、许可证、重复版本、来源由 cargo-deny fail closed。例外写进 `deny.toml` 并带理由。 | MS M-STATIC-VERIFICATION | `deny` |
| DEP-7 | 包元数据完整（description、license、repository、readme、keywords、categories）。 | API#c-metadata | `lint:clippy::cargo_common_metadata` |

## 10. SemVer 与 0.x（VER）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| VER-1 | 0.x 阶段，`0.y.z` 的 `y` 位等同 major。删除或改名公共条目、给公共 struct 加字段、给穷尽 enum 加变体、加 `#[non_exhaustive]`、删除 feature 都算 breaking。提高 MSRV 按 Cargo 原文属 possibly-breaking，本仓库按 minor 处理：记进各包 CHANGELOG 的 `Changed`，下一次发版升 `y` 位。 | SV#change-categories、#item-remove、#struct-add-private-field-when-public、#enum-variant-new、#attr-adding-non-exhaustive、#cargo-feature-remove、#env-new-rust | `ci:semver-checks`；评审 |
| VER-2 | breaking 改动先升 `y` 位，并在该包 CHANGELOG 的 `Breaking` 下写迁移方法。 | SV；API#c-relnotes | `ci:semver-checks`；评审 |
| VER-3 | 0.x 可以 breaking，但不留过渡形态：不提供对外双键、弃用别名、`#[doc(hidden)]` 过渡 API。内部可以分几次提交，每次门禁绿，但在同一个 PR 内完成。 | MS（Migrate callers then delete）；orgchart structure-spec §12.3 | 评审 |
| VER-4 | 四包版本号各自独立。 | AGENTS | 评审 |

## 11. 测试（TST）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| TST-1 | 三层：单元测试放在被测模块的 `#[cfg(test)] mod tests` 或同目录的 `*_tests.rs` / `node_tests/`；集成测试放 `tests/`，只用公共 API；公共条目的示例写成 doctest。 | BOOK；MS M-INTEGRATION-TESTS；API#c-example | `cargo test`；评审 |
| TST-2 | 测试断言调用方能观察到的行为。排版断言 `RecordingCanvas` 记录的坐标，填表断言 `pdftotext` 的文本，不断言压缩后的 PDF 字节。 | MS M-TAUTOLOGICAL-TESTS | 评审 |
| TST-3 | golden 快照只在显式 `UPDATE_GOLDEN=1` 时重写，CI 只读。 | 本仓库现状 | 评审 |
| TST-4 | 测试函数名描述行为（`rejects_empty_uri`），不加 `test_` 前缀。 | BOOK 惯例 | 评审（现有 4 处 `fn test_`） |
| TST-5 | 测试工具经 feature 暴露（`test-utils`），不用测试依赖反向打破生产 DAG。 | MS M-TEST-UTIL | 评审 |
| TST-6 | 依赖环境字体的测试在缺字体时要显式跳过并打印原因，不能静默通过。 | 本仓库 | 评审 |

## 12. 文档与注释（DOC）

| 号 | 规则 | 出处 | 执行 |
| --- | --- | --- | --- |
| DOC-1 | rustdoc、代码注释、错误消息、panic 消息用英文。`docs/` 下的设计文档与规格用中文。 | 本仓库拍板 | 评审；标识符见 STY-5 |
| DOC-2 | 每个公共条目有 `///`，每个公共模块有 `//!`。首句一行，约 15 个词，说清这是什么。 | API#c-crate-doc；MS M-FIRST-DOC-SENTENCE、M-MODULE-DOCS | `lint:missing_docs`；`RUSTDOCFLAGS=-D warnings`；首句靠评审 |
| DOC-3 | 规范小节：`# Errors`、`# Panics`、`# Examples`。示例用 `?`，不用 `unwrap`。 | API#c-failure、#c-example、#c-question-mark；MS M-CANONICAL-DOCS | `lint:clippy::missing_errors_doc`、`missing_panics_doc`；示例靠评审 |
| DOC-4 | rustdoc 只写终态行为，不写设计过程、合规表和历史。 | MS M-NO-META-DESIGN-DOCUMENTATION | 评审 |
| DOC-5 | 注释只写代码表达不了的约束或「为什么」，不复述下一行代码，不写来历。 | 本仓库；MS M-NO-META-DESIGN-DOCUMENTATION（引申） | 评审 |
| DOC-6 | 魔法数字写成具名常量，并注明出处（协议条款、规格章节）。 | MS M-DOCUMENTED-MAGIC | 评审 |
| DOC-7 | 用户可见行为写进该包的 CHANGELOG。 | API#c-relnotes | 评审 |

## 13. 禁止的取巧写法（TRK）

「取巧」指读起来像在做一件事、实际上在做另一件事的写法，或靠巧合而非类型成立的写法。lint 抓不全，评审时以下面的现存实例作反例。

| 号 | 禁止 | 正派写法 | 反例（现存） | 执行 |
| --- | --- | --- | --- | --- |
| TRK-1 | 丢弃有载荷的底层错误 | 映射成带 `source` 的领域错误 | `post/thumbnail.rs:111`、`forms/inspect.rs:105` | 评审（ERR-4） |
| TRK-2 | 用哨兵值表示「没有」或「失败」（`u32::MAX`、`0`、空串） | `Option` 或专用 enum。协议规定的常量除外，注明出处 | `pdf/spec/mod.rs:124`、`orgchart/ids.rs:14` | 评审 |
| TRK-3 | 浮点位模式当键或比较手段 | 有限值 newtype 内实现 `Eq` / `Hash` | `orgchart/route.rs:284`、`pdf/spec/lower.rs:2041` | 评审（DET-3） |
| TRK-4 | 字符串前缀或魔法字符串表达身份或类别 | enum 或类型化 id | `orgchart/graph.rs:13` `__overflow__:` | 评审（API-2） |
| TRK-5 | `Box<dyn Any>` 加 downcast | 泛型或 trait 方法 | `pdf/backend/mod.rs:29` | 评审（ARC-6） |
| TRK-6 | 用 `if let Some(..) = ..find()` 或返回 `Option` 的 `?` 静默跳过本应成立的不变量 | 失败即错误或 PAN-3 的登记 panic | `forms/inspect.rs:90`、`pdf/layout/justify.rs:223` | 评审 |
| TRK-7 | `let _ =` 丢弃 `Result` 或有意义的值 | 处理它，或写明为何可丢 | `pdf/layout/frame.rs:317` | `lint:let_underscore_drop`、`lint:clippy::let_underscore_must_use`、`lint:clippy::let_underscore_untyped` |
| TRK-8 | 为凑签名保留不用的参数 | 改签名 | `orgchart/captions.rs:72` | 评审 |
| TRK-9 | 用 `#[allow]` 压 lint | `#[expect(lint, reason)]`；只在部分 target 成立的用 `#[cfg_attr(<条件>, expect(..))]` | 现存 11 处生产 `allow` | `lint:clippy::allow_attributes`、`lint:clippy::allow_attributes_without_reason` |
| TRK-10 | 为了躲 lint 改写形式而不改语义（如把 `\|_\|` 改成 `\|_e\|`） | 修正语义，或登记 `#[expect]` | — | 评审 |
| TRK-11 | `mem::forget`、函数指针转整数、`verbose` 文件读取 | 按 RAII 与 std 便捷 API 写 | — | `lint:clippy::mem_forget`、`lint:clippy::fn_to_numeric_cast_any`、`lint:clippy::verbose_file_reads` |
| TRK-12 | 冗余 `clone`、对 `&mut` 不必要的要求 | 借用或移动 | `forms/fill.rs:293`、`pdf/backend/krilla_canvas.rs:214` | `lint:clippy::redundant_clone`、`lint:clippy::needless_pass_by_ref_mut`（nursery） |

## 14. Python 音译识别

「Python 音译」指把 Python 的数据模型和控制流逐字搬进 Rust。下表左列是要认出的形态，右列是 Rust 写法。

| 号 | Python 形态 | Rust 写法 | 出处 | 执行 |
| --- | --- | --- | --- | --- |
| PY-1 | `for i in range(len(v))` | `for x in &v`、`enumerate`、`zip` | CL | `lint:clippy::needless_range_loop`（style） |
| PY-2 | 手动 `i += 1` 计数 | `enumerate` | CL | `lint:clippy::explicit_counter_loop`（complexity） |
| PY-3 | `if x is not None:` 后再取值 | `if let`、`let else`、`?` | G#prefer-pattern-matching… | `lint:clippy::unnecessary_unwrap`、`lint:clippy::manual_let_else`、`lint:clippy::question_mark` |
| PY-4 | `-1`、`None` 字符串、空串作哨兵 | `Option` 或 enum | API#c-custom-type | 评审（TRK-2） |
| PY-5 | 字符串当枚举（`mode="left"`） | enum，边界处 `FromStr` 一次 | API#c-newtype；MS M-STRONG-TYPES | 评审 |
| PY-6 | dict 当记录、字符串键查字段 | struct | MS M-RUST-SHAPED | 评审 |
| PY-7 | 异常式控制流 | 带类型 kind 的 `Result` | MS M-RUST-SHAPED | 评审（PAN-1） |
| PY-8 | 到处 `.clone()` 模拟引用语义 | 借用或移动 | G#prefer-borrows… | `lint:clippy::redundant_clone`、`implicit_clone`、`needless_pass_by_value`、`unnecessary_to_owned` |
| PY-9 | 平行数组 | `Vec<Struct>` | 社区惯例 | 评审 |
| PY-10 | 元组多返回值 | 命名 struct | G#prefer-named-fields… | 评审（API-4） |
| PY-11 | 关键字参数加默认值 | Options struct 加 `Default` | G#options-structs… | `lint:clippy::too_many_arguments` |
| PY-12 | `isinstance` 链 | enum 加穷尽 `match`，不用通配臂吞掉新变体 | 社区惯例 | `lint:clippy::wildcard_enum_match_arm` |
| PY-13 | 模块级全局缓存 | 显式上下文 | MS M-AVOID-STATICS | 评审（ARC-5） |
| PY-14 | 按字符下标切字符串 | `char_indices`、字素迭代 | CL string_slice | 评审（`string_slice` 不开，pdf 9 处切的是已记录字节范围） |
| PY-15 | int / float 隐式互转 | `From` / `TryFrom` | API#c-conv-traits | CONV |
| PY-16 | `d.get(k, default)`、先查后插 | `get().copied().unwrap_or(..)`、entry API | CL | `lint:clippy::map_entry`（perf） |
| PY-17 | f-string 后 `+=` 拼接 | `write!` | G#prefer-format… | `lint:clippy::format_push_string`、`string_add` |
| PY-18 | `len(x) == 0` | `is_empty()` | CL | `lint:clippy::len_zero`（style） |
| PY-19 | 每层函数开头重复 `if not x: return` 校验 | 边界解析一次（ARC-3） | API#c-validate | 评审 |
| PY-20 | `utils.py` 式杂物模块 | 按主要条目命名（STY-4） | G#name-source-files… | 评审 |
| PY-21 | `"string".to_string()`、`str()` 式转换 | `to_owned()` / `String::from` | CL | `lint:clippy::str_to_string` |
| PY-22 | 理由写「对齐 Python 实现」 | 写 Rust 侧理由（调用方合同、类型保证） | MS M-RUST-SHAPED | 评审 |

## 15. lint 取舍

### 15.1 原则

- **LINT-1.** clippy `all` 与 `pedantic` 整组开（`warn`，CI 以 `-D warnings` 变成错误）。`restriction`、`nursery`、`cargo` 不整组开，只单挑（CL 原文不建议整组开 restriction）。单挑的 lint 写 `deny`，因为它们承载本文的规则。
- **LINT-2.** 例外写在代码位置上：`#[expect(lint, reason = "...")]`。只在部分 target 成立的用 `cfg_attr`。不写 `#[allow]`（TRK-9）。`#[expect]` 落空时会报 `unfulfilled_lint_expectations`，所以过期的例外会自动暴露。
- **LINT-3.** 新增或调整 lint、升工具链的 PR 附定价结果：每个 lint 在四包生产与测试代码里的命中数。运行 `python3 scripts/price_lints.py`。CI 不调用该脚本。

### 15.2 采纳清单

以根 `Cargo.toml` `[workspace.lints]` 为准。上面各章已注明每个 lint 对应的规则号。

### 15.3 拒绝清单（1.98 实测）

| lint | 拒绝理由 |
| --- | --- |
| `indexing_slicing` | 拍板为规则加评审（PAN-8）。生产 287 处，计数登记在 §16，后续专项清完再议 |
| `as_conversions` | 转成 `dyn Trait` 必需 `as`；cast 系列 pedantic lint 已覆盖数值转换 |
| `map_err_ignore` | 分不清有无载荷；forms 25 处是正当的无载荷 `TryFromIntError` |
| `unwrap_in_result` | 会把已登记的 `expect` 一并报出，与 PAN-3 重复 |
| `redundant_pub_crate` | 与 `unreachable_pub` 方向相反，已选后者 |
| `exhaustive_enums` / `exhaustive_structs` | 与 API-9 的三类例外冲突 |
| `mod_module_files` | 与 STY-17 相反 |
| `absolute_paths`、`std_instead_of_core` | 本库是 std 库；`std::result::Result` 用来避开 crate 自己的 `Result` |
| `missing_inline_in_public_items` | 没有性能依据 |
| `float_arithmetic` | 几何本就是浮点 |
| `rc_buffer` | 就是 API-12 论证过的 `Arc<Vec<u8>>` |
| `error_impl_error` | 公共错误类型就叫 `Error` |
| `impl_trait_in_params` | 与 API-5 的 `impl IntoIterator` 冲突 |
| `tests_outside_test_module` | 集成测试就在 `tests/` |
| `trivial_casts` | 3 处都是 `Error::source` 的 `'static` 转换 |
| `shadow_reuse`、`shadow_same` | 生产 127 处，同值换形态是 Rust 惯用法（STY-10 只禁 `shadow_unrelated`） |
| `default_numeric_fallback` | orgchart 生产 127 处，信号弱 |
| `missing_asserts_for_indexing` | 鼓励多加 panic，与 PAN-8 方向相反 |
| `string_slice` | pdf 切的是已记录的字节范围（PY-14） |
| `if_then_some_else_none`、`unseparated_literal_suffix`、`semicolon_*_block` | 纯口味，四个来源都没有要求 |
| `cognitive_complexity` | restriction 组，阈值无依据；`clippy.toml` 里不生效的阈值已删 |
| `module_name_repetitions` | 1.98 已属 restriction，未开即不生效，原 `allow` 删除 |

## 16. 落地清单

四个包切到 `[lints] workspace = true` 之前，先清掉下列生产代码命中。数字来自 `.omo/evidence/engineering-standard/workspace_lints_probe.py`（1.98.1，`--all-targets --all-features`，生产/测试）。

| lint | pdf | forms | orgchart | post | 处理 |
| --- | --- | --- | --- | --- | --- |
| `unreachable_pub` | 20/26 | 0/0 | 66/8 | 0/11 | 机械改 `pub(crate)` |
| `unused_qualifications` | 79/15 | 1/0 | 2/5 | 0/0 | `cargo fix` |
| `expect_used` | 27/44 | 0/5 | 0/17 | 0/24 | 先按 PAN-4 消，余下按 PAN-3 登记；测试辅助按 §3 文件头登记 |
| `wildcard_enum_match_arm` | 6/35 | 8/3 | 0/4 | 2/0 | 改穷尽匹配 |
| `shadow_unrelated` | 11/17 | 1/3 | 3/3 | 0/7 | 改名 |
| `too_many_arguments`（阈值 7 时） | 6/0 | 0/0 | 6/0 | 0/0 | Options struct |
| `allow_attributes` | 4/9 | 2/0 | 4/2 | 1/7 | 改 `expect` / `cfg_attr` |
| `str_to_string` | 0/0 | 9/65 | 0/34 | 0/4 | `to_owned()` |
| `let_underscore_untyped` / `_must_use` / `_drop` | 2+1+1 | 0 | 4 | 0 | 逐条审（`frame.rs:317` 丢 `Result`） |
| `iter_over_hash_type` | 0 | 2 | 1 | 0 | DET-1 |
| `unreachable` | 2/10 | 0 | 0 | 0 | PAN-3 登记 |
| 其余（`unneeded_field_pattern` 2、`missing_assert_message` 1、`string_add` 1、`redundant_clone` 1、`needless_pass_by_ref_mut` 1） | | | | | 逐条改 |
| examples 里的 `unwrap` / `expect` / `panic` / 打印 | | | | | `main` 返回 `Result`；打印在文件头登记 |

生产合计约 278 处。建议的 PR 顺序（每个 PR 单一主题、门禁绿）：

1. 本规范与收敛文档、工具链升 1.98。本草案不改产品代码，但这一步合并前必须修 3 处 rustdoc 冗余链接：`pdf/src/spec/mod.rs:705`、`:706`、`:746`。1.98 的 rustdoc 报 `redundant_explicit_links`，CI 现有的两条 pdf `cargo doc` 命令在 1.98 上都 exit 101。同一步把定价脚本合成 `scripts/price_lints.py`（LINT-3）。MSRV 的 CHANGELOG 条目已在草案里。
1.5. 修 pdf `--no-default-features` 基线红项（单独 PR，不动库代码）：`anchors` example 补 `required-features = ["backend-krilla"]`；用到 `to_pdf` 的集成测试按 feature 门控（`[[test]] required-features` 或文件头 `#![cfg(feature = "backend-krilla")]`）；删 `golden` 测试里未用的 `ImageFormat`、`ImageSpec` import。
2. 机械类：`unused_qualifications`、`unreachable_pub`、`str_to_string`。
3. `allow` 改 `expect`，`let_underscore_*`，`iter_over_hash_type`。
4. panic 面：`expect_used`、`unreachable`，以及测试和示例的登记。
5. 语义类：`wildcard_enum_match_arm`、`shadow_unrelated`、`too_many_arguments`。
6. 四包切到 `[lints] workspace = true`，删各自的 `[lints.*]` 表。同一 PR 把 `[workspace.lints]` 的 `deny` 当作门禁，重跑 probe 确认零命中。

规范已立、执行缺口待补：

- ARC-2 的模块 DAG 脚本只覆盖 pdf。
- `ci:feature-matrix` 只跑 pdf。post 的 `thumbnail` feature 不在 CI 的 clippy 与 test 里（本地实测 `--all-features` clippy 与 doc 为绿）。
- ERR-2 的编译期断言只有 pdf 的 `spec` 有。
- ARC-6 现存 `backend/mod.rs:29` 一处。
- PAN-8 的 287 处下标。
- TRK 表中的各个反例。

字体覆盖：未设 `PDFCRAFT_*` 时，字体相关用例提前返回，本地「全绿」不等于字体全覆盖（TST-6 要求改为显式跳过并说明）。forms 的 CJK 用例只有 CI 的 NotoSansCJK 能覆盖，`fills_chinese_text` 仍是 `#[ignore]`。

## 17. 明确不采纳

- G 的 chrome / fuchsia 专属条：小 crate、扁平布局、`chromium::import!`、async、起线程、`fuchsia_sync::Mutex`。G#reporting-errors 中允许 `catch_unwind` 粗粒度恢复的部分。
- MS M-SMALLER-CRATES（四包锁定）、M-LATEST-EDITION（锁 2021）。
- MS M-ERRORS-CANONICAL-STRUCTS 的 `Backtrace` 与私有 `ErrorKind`。理由是批量调用方需要按 kind 穷尽匹配。
- 日志条（M-LOG-STRUCTURED、M-LOG-OVERHEAD）。builder 条（M-INIT-BUILDER、M-INIT-CASCADED、M-BUILD-RESULT、API c-builder）：`DocumentSpec` 用结构体字面量加 `Default`。
- API c-serde（serde 不是主接口）、c-bitflag、c-num-fmt、c-rw-value。
- 与本仓库无关的 MS 条：M-SERVICES-CLONE、M-IMPL-IO、M-ASYNC-FN、M-YIELD-POINTS，以及 Apps / FFI / Macros 三组。
- 没有测量依据的 MS performance 条。
- 不稳定 rustfmt 选项（`imports_granularity`、`hex_literal_case`、`format_macro_matchers`）。
- 把 feature `test-utils` 改名 `test-util`（删 feature 属 breaking，不值）。
