# Google Rust 风格指南 disposition

指南：<https://google.github.io/styleguide/rustguide.html>。适用范围：四个包。

本表只做映射：每个指南条目对应 [`rust-engineering-standard.md`](rust-engineering-standard.md) 的哪条规则，或为何不适用。规则正文、执行方式（lint、脚本、评审）只写在工程规范里。优先级：提案书 > workspace-crates-spec > 工程规范 > 本表。写法冲突的取舍顺序见工程规范 §0。

disposition 取值：`adopt`（已写成规范规则）、`adapt`（有理由地偏离，理由在规范里）、`N/A`（不适用）、`defer`（见文末延后项）。

## 映射

| 指南节 / 条目 | disposition | 规范规则 |
| --- | --- | --- |
| Code organization：Prefer small crates [chrome] | N/A | §17；四包锁定（ARC-1） |
| Code organization：flat file layout [chrome] | N/A | §17；模块 DAG 依赖固定路径（ARC-2、STY-17） |
| 文件与叶子模块按主要条目命名；非叶模块不写 API | adopt | STY-4 |
| Don't use glob imports | adopt | STY-6 |
| 一条 `use` 一个模块；trait 局部导入；顶层函数模块限定；类型非限定 | adopt | STY-7 |
| `chromium::import!()` [chrome] | N/A | §17 |
| Code formatting：rustfmt、稳定选项、`rustfmt::skip` 范围 | adopt | STY-1；不稳定选项见 §17 |
| Avoid `Rc` / `Arc` | adopt | API-12 |
| Prefer borrows over deep copies | adopt | PY-8、TRK-12 |
| Accept flexible references；`impl IntoIterator` 入参 | adopt | API-5 |
| 返回 `Vec` 还是 `impl Iterator` | adopt | API-5（按调用分布选，评审） |
| Prefer enum over bool in APIs | adopt | API-3 |
| Prefer named fields over tuples | adopt | API-4、PY-10 |
| Newtypes versus type aliases | adopt | API-1 |
| Options structs should implement `Default` | adopt | STY-14、PY-11 |
| Reporting errors：Option / Result / panic 三分 | adopt | PAN-1 |
| Reporting errors：`catch_unwind` 粗粒度恢复 | N/A | PAN-7、§17 |
| Prefer `expect` over `unwrap` | adopt | PAN-2、PAN-3（orgchart 与 post 也按此执行，不再单独全禁） |
| Use context strings to clarify assertions | adopt | PAN-6 |
| Prefer pattern matching over check-then-unwrap | adopt | PY-3 |
| Iterator combinator chains vs direct iteration | adopt | STY-12 |
| Prefer `format!` / `write!` for concatenation | adopt | STY-13 |
| Don't overuse type inference | adopt | STY-11 |
| `From` 配显式构造 | adopt | ERR-5 |
| derive 优先；已知 std trait 优先；扩展 trait 加 `Ext` | adopt | API-7 |
| `#[must_use]` on functions and types | adopt | API-8 |
| `#[non_exhaustive]` on public enums / structs with public fields | adapt | API-9（五类例外；文档语言 enum 见延后项 t7，样式结构体见 t10，协议记录见 t11） |
| Naming：方法名跟 std、getter / setter、缩写 | adopt | STY-2、STY-3、STY-4 |
| Documentation：`///`、`Errors` / `Panics` / 示例 | adopt | DOC-2、DOC-3 |
| 不用 `unsafe`、不用 unstable feature | adopt | TOOL-3、TOOL-1 |
| `const fn` 只用于语义 const；运算符重载只做数学语义；不 `doc(hidden)`；main 前不跑代码 | adopt | API-14（checklist）、VER-3 |
| Macros：函数优先，`macro_rules!` 优先于过程宏 | adopt | 现无宏；新增宏走 API-14 评审 |
| Concurrency：毒化 Mutex | N/A | 无全局锁（ARC-5） |
| async、标准库起线程 [chrome]；`fuchsia_sync::Mutex` [fuchsia] | N/A | ARC-8、§17 |

## 延后项

这些是提案书已确认的 API 形状，改动须先改提案书。

- **t5：`FieldValue::CheckBox(bool)`。** `CheckBox(bool)` 保留。`true` 画勾，`false` 不画。本轮不增加 `CheckboxState`。调用点自解释（API-3 的登记例外）。
- **t7：pdf 文档语言 enum 不加 `#[non_exhaustive]`。** `spec/mod.rs` 的 `Block`、`Alignment`、`Span`、`BlankWidth`、`ListKind`、`CanvasMark`、`BandSlot`、`TextRun`、`PageWhen`、`LayerMark`、`LayerShape` 与 `spec/types.rs` 的 `Numbering`、`Track` 描述提案书 §2、§12 已确认的文档语言。调用方穷尽 `match` 才能在新增元素时得到编译期提醒（API-9 例外第二类）。
- **t9：文档小项。** `emit.rs` `check_upem` 的 upem 为 0 时的说明，`Positive::pt` 的零值错误注记。
- **t10：样式结构体不加 `#[non_exhaustive]`。** 各 crate 里类型名以 `Style` 结尾的公有结构体，按名字归入此类。成员包括 `FillStyle`、`FieldStyle`、`StrokeStyle`、`pdfcraft_pdf::spec` 的样式和 orgchart 的 `ChartStyle`。它们是公有字段 + `Default` + 结构体字面量（`..XxxStyle::default()`）的可复用参数（提案书 §12.7），标 `non_exhaustive` 会让 crate 外写不出字面量。0.x 加字段按 VER-1 / VER-2 记 breaking。`ParagraphStyle` 的 rustdoc 示例在 crate 外用 `..ParagraphStyle::default()`，`cargo test --doc` 守住这一条（API-9 例外第四类）。
- **t11：协议记录不加 `#[non_exhaustive]`。** orgchart 的 `Node`、`Edge`、`PaidIn`、`ChartRequest`，以及 forms 的 `FieldFill`、`FieldRule`、`FillRequest`、`AbsoluteLine`，由调用方用结构体字面量构造。`ChartRequest` 没有 `Default`，用 `ChartRequest::new` 填缺省。`FillRequest` 用 `FillRequest::new` 填缺省。标 `non_exhaustive` 会让 crate 外写不出这些字面量。0.x 加字段按 VER-1 / VER-2 记 breaking（API-9 例外第五类）。
- **post 的 bool。** `merge_pdfs(parts, preserve_outlines: bool)` 保持 `bool`，`merge.rs` 文档写明取舍，调用点要具名实参或注释。`EncryptionConfig` 的 8 个 `bool` 镜像 PDF 安全位，保留（API-3 的登记例外）。
- **orgchart 私有 bool。** `measure.rs` 的 `is_subject` 与 `captions.rs` 的 `has_investments` 全私有、同文件内调用点自解释，保持。

## 已知门禁红项

见工程规范 §16 末尾。
