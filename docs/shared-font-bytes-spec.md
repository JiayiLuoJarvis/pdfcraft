# 共享字体字节所有权 Spec

| 项 | 值 |
|---|---|
| 状态 | Ready for implementation |
| 范围 | `pdfcraft-pdf`、`pdfcraft-orgchart` 的字体 bytes 所有权；`pdfcraft-forms` 所有权审计；文档、测试 |
| 非范围 | 字体回退、字体解析缓存、全局字体缓存、排版行为 |
| 调用方 | sibling `loco-rs/crates/pdfcraft-docs` 将在本变更后迁移 |

相关权威文档：

- [`提案书.md`](提案书.md) §4 / §4.1：调用方传字体 bytes；缺字整份失败；
  `pdfcraft-pdf` 使用调用方键，orgchart 继续单字体输入。
- [`pdfcraft-design.md`](pdfcraft-design.md)「字体与测量」：同一份字体 bytes
  由 skrifa 与 krilla 共用 `Arc<Vec<u8>>`。
- [`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md)：orgchart 行为合同。
- [`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md)：
  orgchart 公共面与所有权结构。
- [`../AGENTS.md`](../AGENTS.md)：0.x 阶段允许为长期正确的 API 做 breaking
  change，不保留已知错误的双轨接口。

---

## 0. 执行前注意

编写本 spec 时，quire 工作区已有未提交修改：

```text
M crates/pdfcraft-pdf/docs/guide.md
M crates/pdfcraft-pdf/src/spec/lower.rs
M crates/pdfcraft-pdf/src/spec/mod.rs
```

执行者必须先读取并保留这些修改，在其基础上合并字体变更。禁止
`git checkout --`、`git reset --hard` 或覆盖写回上述文件。

建议在独立分支完成一个原子 PR，例如 `opt/shared-font-bytes`。本主题同时修改
`pdfcraft-pdf` 与 `pdfcraft-orgchart`，因为它们共同决定调用方能否端到端共享同一
块字体内存；不拆成一个 crate 已改、另一个仍深拷贝的长期中间状态。

---

## 1. 背景与问题

### 1.1 pdfcraft-pdf

当前公共边界：

```rust
pub struct FontFace {
    pub bytes: Vec<u8>,
    pub index: u32,
}
```

`pdfcraft-docs` 等调用方如果缓存一份字体，每构造一个 `FontFace` 仍必须深拷贝
`Vec<u8>`。`spec::lower::register_face` 随后才执行：

```rust
Arc::new(face.bytes)
```

因此内部虽已使用 `Arc<Vec<u8>>`，共享发生得太晚，只能在单份文档内部复用，不能
让多个并发文档或多个模板共享调用方的字体 buffer。

这也与 `pdfcraft-design.md` 已写明的目标不一致：字体 bytes 应由调用方持有，
skrifa 与 krilla 使用同一块 `Arc<Vec<u8>>`。

### 1.2 pdfcraft-orgchart

当前公共边界：

```rust
pub struct FontBytes {
    pub data: Vec<u8>,
}
```

当前一份 orgchart render 至少有两处额外整份复制：

1. `FontMetrics::open` 使用 `Arc::from(font.data.as_slice())`，把输入复制到新的
   `Arc<[u8]>`。
2. `render` 使用 `font.bytes().to_vec()` 再复制一份交给 krilla。

当 audit package 同时生成 checklist、equity 与 outward investment，或者多个
请求并发生成 orgchart 时，这些复制会叠加。

### 1.3 为什么应用层缓存不够

应用层启动时读一次字体只能消除重复文件 IO。只要依赖库公共 API 仍要求
`Vec<u8>`，调用方在每次 render 边界仍会深拷贝整份字体。

正确的所有权链应为：

```text
调用方 FontCatalog
  → Arc<Vec<u8>>
  → FontFace / FontBytes（clone Arc）
  → skrifa 测量（borrow Arc 内 slice）
  → krilla Data（clone 同一个 Arc）
```

全过程只增加引用计数，不复制字体文件内容。

---

## 2. 已定方案

### 2.1 共享类型

使用标准库 `Arc<Vec<u8>>`，不增加新 crate，不增加第三方依赖。

选择 `Arc<Vec<u8>>` 而不是 `Arc<[u8]>` 的原因：

- `pdfcraft-pdf` 内部 `FontSource` / `ResolvedFont` 已使用 `Arc<Vec<u8>>`；
- krilla 0.8.2 的 `Data` 已实现 `From<Arc<Vec<u8>>>`；
- `pdfcraft-design.md` 已钉定这一形状；
- 可以直接把同一个 Arc 交给 skrifa 和 krilla，不需要 slice → Vec 转换。

字体 bytes 在库内按不可变数据使用。不得用 `Arc<Mutex<Vec<u8>>>`、`RwLock`、
进程全局 cache 或可变静态。

### 2.2 不抽公共字体 crate

`pdfcraft-pdf` 与 `pdfcraft-orgchart` 继续没有依赖边。两者公开字段都使用标准库
`Arc<Vec<u8>>`，调用方可以把同一个 Arc clone 给两个 crate。

本次不为一个标准库容器建立 `pdfcraft-fonts`。以后只有在多个 crate 需要共享
字体 metadata、解析结果或稳定新类型时，才另立提案。

### 2.3 pdfcraft-forms 审计结论：生产 API 保持不变

`pdfcraft-forms::FontBytes<'a>` 当前借用 `&'a [u8]`：

```rust
pub struct FontBytes<'a> {
    pub bytes: &'a [u8],
    pub index: u32,
}
```

调用方持有 `Arc<Vec<u8>>` 时可直接借用，不复制内容：

```rust
let font = pdfcraft_forms::FontBytes {
    bytes: shared_font.as_slice(),
    index: 0,
};
```

forms 内部存在两类整份复制，但都不能通过把公共输入改成 Arc 消除：

1. `FontFile::from_source` 必须取得私有可变 `Vec<u8>`，因为它会把 TTC 拆成单面，
   并清零 / 重算字体 `head` 时间戳与 checksum，不能修改调用方共享 buffer。
2. `PreparedFont::embed` 必须把处理后的字体交给 lopdf `Stream` 持有，输出 PDF
   需要自己的 owned bytes。

因此 forms 与 pdf / orgchart 的情况不同：

- 输入边界已经零拷贝；
- 后续复制属于生成输出所必需的可变副本和所有权转移；
- 改成 `Arc<Vec<u8>>` 会增加 API/lifetime 复杂度，却不降低这两份必要内存。

本 PR 不修改 forms 生产类型，不让 forms 依赖 pdf 或 orgchart。执行时增加或保留
一条测试，证明 `FontBytes` 可以直接借用 `Arc<Vec<u8>>::as_slice()`；并运行 forms
全部门禁，确保 sibling 字体 API 变更未造成文档或调用示例漂移。

---

## 3. 目标公共 API

### 3.1 pdfcraft-pdf::FontFace

目标：

```rust
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct FontFace {
    /// Shared font file bytes.
    pub bytes: Arc<Vec<u8>>,
    /// Face index inside `bytes`. Zero is the first face.
    pub index: u32,
}
```

增加便利构造：

```rust
impl FontFace {
    /// Wrap owned bytes without parsing them.
    pub fn new(bytes: impl Into<Vec<u8>>, index: u32) -> Self;

    /// Reuse an existing shared font buffer without copying its contents.
    pub fn from_shared(bytes: Arc<Vec<u8>>, index: u32) -> Self;
}
```

约束：

- `new` 只做 `Arc::new(bytes.into())`，不解析字体。
- `from_shared` 原样保存 Arc。
- 字段继续公开，保持提案书定义的“调用方组值”模型。
- 不引入 Font trait、文件路径构造或 process-global 默认字体。
- `Clone` 只 clone Arc；`PartialEq` / `Eq` 保持按内容比较的现有语义。
- `index`、空字体、大小限额和损坏字体仍在现有 register / layout 路径校验。

这是 breaking source change：原来的 `FontFace { bytes: vec, index }` 必须改成
`FontFace::new(vec, index)`、`FontFace::from_shared(arc, index)`，或字段中显式
传入 `Arc<Vec<u8>>`。0.1.x 阶段不保留另一套 owned `FontFace`。

### 3.2 pdfcraft-orgchart::FontBytes

目标：

```rust
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontBytes {
    /// Shared TrueType or OpenType bytes for one face.
    pub data: Arc<Vec<u8>>,
}
```

保留现有便利入口并增加共享入口：

```rust
impl FontBytes {
    /// Wrap owned bytes without parsing them.
    pub fn new(data: impl Into<Vec<u8>>) -> Self;

    /// Reuse an existing shared font buffer without copying its contents.
    pub fn from_shared(data: Arc<Vec<u8>>) -> Self;
}
```

约束：

- `FontBytes::new` 的调用体验不变，内部改为 `Arc::new(data.into())`。
- `from_shared` 原样保存 Arc。
- orgchart 仍拒绝 TTC / OTC；不增加 index，不改变 `FONT_ERROR`。
- `generate_pdf(&ChartRequest, &FontBytes)` 签名不变。
- 不增加 `pdfcraft-pdf` 依赖。

### 3.3 不增加兼容双轨

不增加以下类型或 API：

- `OwnedFontFace` / `SharedFontFace`
- `OwnedFontBytes` / `SharedFontBytes`
- 接受 Vec 与 Arc 的两套 registry
- deprecated Vec 字段

这些方案会让调用方和内部长期维护两条所有权路径，并允许新代码继续深拷贝。
0.x 阶段直接收敛到共享所有权。

---

## 4. 内部实现

### 4.1 pdfcraft-pdf

修改 `spec::lower::register_face`：

```rust
fonts.register(FontSource {
    bytes: face.bytes,
    index: face.index,
})
```

这里 move Arc，不再 `Arc::new(face.bytes)`，也不复制内容。

以下内部结构继续使用现有 `Arc<Vec<u8>>`：

- `FontSource`
- `LoadedFace`
- `ResolvedFont`
- text measurement
- backend `Font::new(Data::from(Arc::clone(...)), index)`

不得顺手修改：

- `FontRegistry<K>` 的 caller-defined key 设计；
- normal / bold 行为；
- missing glyph、unregistered font、bold required 错误；
- limits；
- skrifa / krilla 版本；
- layout、分页或 PDF backend 行为。

### 4.2 pdfcraft-orgchart

`FontMetrics` 改为：

```rust
pub struct FontMetrics {
    bytes: Arc<Vec<u8>>,
    // existing metrics and glyph cache
}
```

`FontMetrics::open`：

- 用 `font.data.as_slice()` 做 TTC、empty、skrifa 解析；
- 保存 `Arc::clone(&font.data)`；
- 删除 `Arc::from(font.data.as_slice())`。

内部 accessor 返回共享 Arc 的引用：

```rust
pub(crate) fn bytes(&self) -> &Arc<Vec<u8>>;
```

render 使用：

```rust
Font::new(Data::from(Arc::clone(font.bytes())), 0)
```

删除 `.to_vec()`。

保持：

- 测量与绘制使用同一份 bytes；
- glyph id / advance 管线不变；
- TTC / OTC 仍为 `FONT_ERROR`；
- 缺字、分页、文案、页序与所有稳定错误码不变；
- 不启用 krilla `simple-text` / `rayon`。

### 4.3 Arc 只共享 bytes，不共享可变排版状态

以下状态继续每份文档独立：

- glyph advance cache；
- layout / paging 状态；
- krilla `Font` handle；
- PDF document / page / surface；
- 输出 bytes。

本变更不在 Arc 内放 `FontMetrics`、glyph cache 或 krilla handle，不增加锁。并发
render 只共享不可变字体文件内容。

---

## 5. 文档同步

同一 PR 更新：

1. `docs/提案书.md` §4.1：明确 `FontFace` / orgchart `FontBytes` 的 bytes
   使用 `Arc<Vec<u8>>`，调用方可跨文档共享；不改变 caller-defined key 与单字体
   合同。
2. `docs/pdfcraft-design.md`：保留已写明的 Arc 设计，修正与当前公开类型名不一致
   的示例（若有）。
3. `docs/pdfcraft-orgchart-spec.md` §3.2 / §9.1：`FontBytes.data` 从 `Vec<u8>`
   改为 `Arc<Vec<u8>>`；可观察行为不变。
4. `docs/pdfcraft-orgchart-structure-spec.md`：公共面与所有权更新为 shared bytes。
5. `crates/pdfcraft-pdf/docs/guide.md`、README、crate rustdoc、examples：
   使用 `FontFace::new` / `from_shared` 或 Arc 字段。
6. `pdfcraft-orgchart` guide、README、crate rustdoc、example：
   说明 `new` 接受 owned Vec，`from_shared` 复用 Arc。
7. `CHANGELOG.md` Unreleased/Changed：标记两个 crate 的 breaking source change，
   明确行为不变、clone 不再复制字体内容。

不得把 Loco 的 `FontCatalog`、字体路径或业务 `FontKey` 写进 quire 文档。quire
只负责接收调用方提供的共享 bytes。

---

## 6. 测试

### 6.1 pdfcraft-pdf

增加或更新：

1. `FontFace::new` 把 Vec 包装进 Arc。
2. `FontFace::from_shared` 保持 pointer identity：

```rust
assert!(Arc::ptr_eq(&source, &face.bytes));
```

3. `register_face` / lowering 后的 `FontSource.bytes` 与输入 Arc pointer identity
   相同；测试放 crate 内，不为测试公开内部字段。
4. normal / bold 使用同一或不同 Arc 均可正常布局。
5. TTC index、empty、font byte limit、invalid font、missing glyph 等现有错误测试
   全部保持。
6. backend-krilla 的 PDF smoke 与 glyph placement / TJ 断言保持。
7. no-default-features 继续编译。

### 6.2 pdfcraft-orgchart

增加或更新：

1. `FontBytes::new` 包装 Vec。
2. `FontBytes::from_shared` 保持 pointer identity。
3. `FontMetrics::open` 保存同一 Arc，不复制输入。
4. render 直接把同一 Arc clone 给 krilla，生产代码不再出现
   `font.bytes().to_vec()`。
5. TTC / garbage / empty / missing glyph / page structure / text contract tests 保持。
6. `generate_pdf` 输出仍以 `%PDF` 开头，既有抽字 / 页序断言不变。

### 6.3 静态回归检查

执行：

```sh
rg 'Arc::new\(face\.bytes\)' crates/pdfcraft-pdf/src
rg 'Arc::from\(font\.data\.as_slice\(\)\)' crates/pdfcraft-orgchart/src
rg 'font\.bytes\(\)\.to_vec\(\)' crates/pdfcraft-orgchart/src
```

三条均应无匹配。

不要写依赖 `Arc::strong_count` 固定数值的脆弱测试；只用 `Arc::ptr_eq` 证明 buffer
身份。

### 6.4 pdfcraft-forms 兼容证明

forms 生产代码不改所有权。增加或更新一个测试，以 `Arc<Vec<u8>>` 持有输入字体，
通过 `shared.as_slice()` 构造 `pdfcraft_forms::FontBytes<'_>`，证明未来
FontCatalog 可直接复用同一输入 buffer。

该测试不要求 forms 内部 pointer identity：`FontFile` 为了 TTC 拆面、规范化 head
和嵌入输出而创建 owned copy 是合同所需。

---

## 7. 非目标

- 不改变字体键为字符串；`pdfcraft-pdf` 继续由调用方定义 `K`。
- 不增加字体 fallback / latin 自动分流。
- 不增加 process-global 字体注册表或缓存。
- 不缓存已解析 skrifa face、glyph metrics 或 krilla handle。
- 不修改 `pdfcraft-forms` 的生产 API 或必要的 owned 输出副本。
- 不共享图片 bytes。
- 不修改字体 limits、错误类型或错误码。
- 不更换 allocator，不引入 Rayon。
- 不承诺跨进程共享 heap；每个 worker 进程仍有自己的 Arc 和字体 buffer。
- 不做 benchmark 驱动的 layout / backend 重构。

---

## 8. 实施顺序

一个 PR 内按以下顺序推进：

1. 在提案书 / 行为 spec 确认 shared bytes 公共形状。
2. 修改 `pdfcraft-pdf::FontFace` 与 constructors。
3. 修改 `spec::lower`，确保输入 Arc 原样进入 `FontSource`。
4. 更新 pdf crate 测试、guide、README、examples。
5. 修改 `pdfcraft-orgchart::FontBytes` 与 constructors。
6. 修改 `FontMetrics` / render，删除两次内容复制。
7. 更新 orgchart 测试、guide、README、examples。
8. 增加 forms 从 `Arc<Vec<u8>>` 借用 slice 的兼容测试；不改生产 API。
9. 更新 CHANGELOG。
10. 跑 §9 全部门禁。
11. 记录 quire commit SHA，交给 `loco-rs` 迁移。

不要在步骤 2–7 中修改排版常量、文案、页序或错误体系。

---

## 9. 验证门禁

```sh
python3 scripts/check_pdfcraft_pdf_modules.py

cargo fmt --all -- --check

cargo clippy -p pdfcraft-pdf --all-targets --features test-utils -- -D warnings
cargo test -p pdfcraft-pdf --features test-utils
cargo check -p pdfcraft-pdf --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc -p pdfcraft-pdf --no-deps

cargo clippy -p pdfcraft-orgchart --all-targets -- -D warnings
cargo test -p pdfcraft-orgchart
RUSTDOCFLAGS="-D warnings" cargo doc -p pdfcraft-orgchart --no-deps

cargo clippy -p pdfcraft-forms --all-targets -- -D warnings
cargo test -p pdfcraft-forms
RUSTDOCFLAGS="-D warnings" cargo doc -p pdfcraft-forms --no-deps
```

若现有字体环境变量可用，再执行全部字体相关测试：

```sh
PDFCRAFT_PDF_FONT=/path/to/font.ttf \
PDFCRAFT_CJK_FONT=/path/to/cjk.ttf \
PDFCRAFT_ORGCHART_FONT=/path/to/cjk.ttf \
cargo test -p pdfcraft-pdf --features test-utils

PDFCRAFT_ORGCHART_FONT=/path/to/cjk.ttf \
cargo test -p pdfcraft-orgchart
```

验收：

- 全部门禁通过；
- 所有新增公共项有 rustdoc；
- quire 生产路径没有上述三处字体内容复制；
- 现有错误码、PDF 文案、页序与 glyph placement 断言不变；
- CHANGELOG 明确 breaking source change；
- 未覆盖执行前已有的未提交修改。

---

## 10. loco-rs 配合修改

quire 独立门禁通过后，`loco-rs` 必须配合迁移。由于当前使用 path dependency，
本地 quire 一旦改为 Arc 字段，loco-rs 在迁移完成前可能暂时无法编译；建议使用
独立 branch/worktree，并记录 quire commit SHA，禁止靠复制源码维持两套依赖。

`loco-rs` 后续改动：

1. `pdfcraft-docs::fonts` 从读盘时就构造 `Arc<Vec<u8>>`。
2. `YhcFonts` / `TcspFonts` 的 face bytes 改为共享 Arc；Times regular / bold 在
   YHC 与 TCSP 之间使用同一 Arc。
3. 所有现有 template 的 `FontFace` 构造改为 `from_shared` 或 Arc 字段：
   - YHC tower；
   - YHC settle-in package；
   - authorization processing TCSP；
   - audit checklist；
   - audit equity / outward / package。
4. audit orgchart 使用 `FontBytes::from_shared`，不从 catalog 深拷贝 HC 字体。
5. 后续使用 forms 时，从 catalog 的共享 Arc 借用 slice：
   `pdfcraft_forms::FontBytes { bytes: shared.as_slice(), index }`；不要为 forms
   另读字体或先 clone Vec。
6. 建立全局真实 family `FontKey` 与 `FontCatalog`，并按
   `docs/pdfcraft-docs-architecture-concurrency-spec.md` 完成模板迁移。
7. Loco 启动时加载 catalog 一次并存入 `PdfRuntime`；请求路径不读字体文件。
8. 增加 `Arc::ptr_eq` 测试，证明 YHC / TCSP 共享 Times，audit checklist /
   orgchart 共享 HC regular。
9. 重跑 pdfcraft-docs 与 workspace 全部测试，再做并发 / RSS baseline。

只有 quire 改动不会自动降低 loco-rs RSS：调用方如果继续先 clone Vec 再构造 Arc，
仍会复制内容。两边都完成后，端到端共享才成立。
