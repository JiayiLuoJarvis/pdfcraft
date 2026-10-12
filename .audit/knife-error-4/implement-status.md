# Knife 4 implementation status

未提交。工作树保持脏。

## 改了什么

公共 `Result<T>` 改为 `Result<T, Errors>`。`Errors` 非空，最多 64 条，发现序。`first` / `as_slice` / `iter` / `truncated`。`From<Error>`。`PartialEq` 只比各条 `kind` 与 `at`。恰好一条且未截断时 `Display` 与 `Error` 相同，否则编号。截断末行是 `further errors omitted`。`source()` 仅在恰好一条时转给该条。

`error` 模块不依赖 document / layout。几何、画布、字体注册、`Document::font` / `bold_font` / `load_image` 仍返回单个 `Error`。内部单错路径用 `pub(crate)` 别名 `Fallible`。

组装侧 `Option<Error>` 换成 `ErrorBag`。`Flow::push` 记下错误后仍收集后面的兄弟。失败节点不进成功树。表先 lower 单元格，再记轨道和列数错误。`Document::section` 返回 `()`，吸收闭包 `Err`，后面的节继续跑。调用方 `build` 返回 `Err` 时吸收后不再测量。任一错误都不画页、不产出 `Rendered` 或 PDF。`to_pdf` 把 backend 单错升成 `Errors`。

测量侧 `LayoutCtx` 持有 `&mut ErrorBag`。`MissingGlyph`、`BoldFaceRequired`、`UnknownFont` 记入同一张表并继续。缺字前进宽度为 0，绘制时跳过。同一 `UnknownFont` id 只记一次。几何等硬错误仍是 `Fallible`，在绘制边界 `fail`。表已满时 `fail` 替换最后一条并保持 `truncated`。

## 门禁

| 命令 | 退出码 |
| --- | --- |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --all-targets --features test-utils -- -D warnings` | 0 |
| `cargo test --features test-utils` | 0 |
| `cargo doc --no-deps` | 0 |

测试：库内 218 通过、1 个既有忽略，集成 96 通过，doctest 0。`Location` 构造器 `cell` 已改名为 `at_cell`，避免公共 rustdoc 链到 `pub(crate)` 方法；`RUSTDOCFLAGS=-D warnings cargo doc --no-deps` 通过。

## 与合成稿的差别

- 缺字留在簇带上，用 `omit` 和 0 前进宽度跳过绘制，不从带上删掉，以便簇下标对齐。不使用 `.notdef`。
- `Fallible` 不能出现在公共签名里。几何和画布写成 `std::result::Result<T, Error>`。
- `paginate_sections` 仍只把袋子收成第一条 `Error`，供 layout 单测使用。公共 `document` / `paint` / `render` 返回完整 `Errors`。
- `note()` 留作 `record` 的薄包装。
- `ErrorBag` 派生 `Clone` 和 `PartialEq`，因为 `HLine` 和 `ImageBox` 派生了这两个 trait。
- 恰好一条且已经截断时，`Display` 是编号行加上 omitted，不是单条 `Error` 原文。
- 重复的 `UnknownFont { id }` 不增加 `seen`，不占 64 条名额，也不因此标成截断。
