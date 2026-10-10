# How explorer: canvas/backend image

Source agent: `8423f1cd-7aca-40cb-982b-0fca35094039` (2026-10-09).

图片在画布层已经能被记录和回放，但 krilla 后端仍把 `draw_image` 当成未实现操作：不解码、不入 `Mark`、不调用 `Surface::draw_image`，并让 `finish` 失败。`raster-images` 已打开，解码器依赖进了 lockfile，源码里没有调用点。

### Components Found

**描述符（无像素字节、无解码）** — `src/canvas.rs`

- `ImageId`（53–69 行）：`pub struct ImageId(pub(crate) u32)`。`Copy + Eq + Hash`。公共构造是 `ImageId::from_raw(u32)` / `as_u32()`。没有分配器，任何 `u32`（含 0）都能造出来。
- `DecodedImage`（71–81 行）：三个字段全是 `pub`：`id: ImageId`、`pixel_width: u32`、`pixel_height: u32`。`Copy`。文档写的是「调用方已经解码过的图」的标识和像素尺寸。没有 `new`，不拒绝 0 边，不存编码字节或 krilla 句柄。
- 公共重导出：`src/lib.rs` 26–28 行导出 `ImageId`、`DecodedImage`、`Canvas`、`CanvasCommand`、`RecordingCanvas`。

**`Canvas` 协议** — `src/canvas.rs` 167–195 行

- `fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()>`。
- 对应日志变体 `CanvasCommand::DrawImage { image: DecodedImage, rect: Rect }`（145–151 行）。`CanvasCommand` 标了 `#[non_exhaustive]`。

**`RecordingCanvas`** — `src/canvas.rs` 217–342 行

- `draw_image`（323–329 行）无条件 `push(CanvasCommand::DrawImage { image: *image, rect })` 并返回 `Ok(())`。不查页是否打开，不查 `ImageId`，不查像素边。
- `pages()`（245–280 行）在 `StartPage`…`FinishPage` 之间保留 `DrawImage`（267 行与 Fill/Stroke/Glyph/Link 同一臂）。页内的 `SetDocumentInfo` 被跳过。未闭合的 `StartPage` 整页丢弃。`StartPage` 之前的 `DrawImage` 留在 `commands()` 里，但不会进 `RecordedPage::ops`。
- `replay`（344–365 行，`pub(crate)`）：`CanvasCommand::DrawImage { image, rect }` → `canvas.draw_image(image, *rect)?`（357 行）。无 `backend-krilla` 时 `#[allow(dead_code)]`（344 行）。

**`KrillaCanvas`（无 krilla 类型）** — `src/backend/krilla_canvas.rs`

- 模块注释（1–4 行）：本文件不 `use krilla`。`Document` / `Page` / `Surface` 只在 `emit` 里构造。
- 状态（92–101 行）：`font_store: Arc<FontStore>`、`info`、`closed_pages: Vec<PagePlan>`、`open_page: Option<OpenPage>`、`failure: Option<Error>`、`serialization`。没有 image 表，没有 session 类型，仓库里没有 `session.rs`。
- `PagePlan`（39–44 行）/ `OpenPage`（47–52 行）：`marks: Box<[Mark]>` 或 `Vec<Mark>`，外加 `links`。
- `Mark`（54–83 行）只有 `Fill`、`Stroke`、`Glyphs`。没有 `Image`。`Glyphs` 自带 `Arc<Vec<u8>>` 字体字节。
- `QueuedLink`（85–90 行）是唯一的「先排队、surface 结束后再写」结构，只给链接。图片不入队。
- `draw_image`（246–251 行）：

```246:251:src/backend/krilla_canvas.rs
    fn draw_image(&mut self, _image: &DecodedImage, _rect: Rect) -> Result<()> {
        self.open_page_mut("draw_image")?;
        self.fail(Error::UnsupportedCanvasOperation {
            operation: "draw_image",
        })
    }
```

  形参 `_image`、`_rect` 被丢掉，不 `push` 任何 `Mark`。

**毒化** — 同文件 139–185 行

- `fail`（162–167 行）：`failure` 为空才写入；之后每次都返回已存的那次错误（`clone`）。
- `ensure_ok`（169–174 行）：已有 `failure` 则直接返回它。
- `open_page_mut`（176–185 行）：先 `ensure_ok`；没有打开的页则 `fail(InvalidCanvasState { operation, state: "no page is open" })`。
- `finish`（139–160 行）：先 `drop(font_store)`（注释：字形 mark 已自带字节，发射不再查字体），再若 `failure` 为 `Some` 则返回它，然后才检查「页仍开着」。毒化优先于「页未关」。
- `finish_page`（211–225 行）先 `ensure_ok`。`draw_image` 之后再 `finish_page` 会失败，打开的页不会进入 `closed_pages`。

**发射** — `src/backend/emit.rs`

- 只 import：`LinkAction`、`LinkAnnotation`、`Target`、`Metadata`、`PageSettings`、`Surface`、`Font`、`GlyphId`、`KrillaGlyph`、`Data`、`Document`、`SerializeSettings`（13–19 行）。没有 `krilla::image`，没有 `Surface::draw_image`，没有 `push_transform` / `pop`。
- `render`（30–45 行）签名是 `(info, pages, serialization)`，没有图片表参数。
- `draw_mark`（115–174 行）对 `Mark` 穷尽匹配 `Fill` / `Stroke` / `Glyphs`。`Glyphs` 走 `surface.draw_glyphs`。没有图片臂。
- 文件头注释（7–9 行）：`Surface::finish` 即 drop；`push`/`pop` 不平衡会 panic。当前图片路径没有成对的 transform。

**测试 `unsupported_image_poisons_finish`** — `src/backend/emit.rs` 478–497 行

- `KrillaCanvas::new()`，`start_page(A4)`。
- `DecodedImage { id: ImageId::from_raw(0), pixel_width: 1, pixel_height: 1 }`，矩形 `Rect::try_pt(0.0, 0.0, 10.0, 10.0)`。
- 断言 `Err(UnsupportedCanvasOperation { operation: "draw_image" })`。
- 只断言 `canvas.finish().is_err()`，不匹配 `finish` 的变体，也不断言 `finish_page`。对照：`unknown_font_poisons_finish`（343–361 行）同时断言 `finish_page().is_err()` 和 `finish().is_err()`。

**`RecordingCanvas` 测试** — `src/canvas.rs` 386–538 行

- `records_draw_order_advances_and_one_page`：`ImageId::from_raw(7)`，200×100，矩形 `(10, 30, 120, 48)`。命令种类序列含 `"image"`（442 行）。`pages()[0].ops` 长度为 5（fill、stroke、image、glyphs、link），但只断言 `ops[0]` 是 Fill、`ops[4]` 是 AnnotateLink，不断言 `DrawImage` 的 id / 像素 / 矩形。
- `replay_reissues_every_command_in_order`：`ImageId::from_raw(2)`，4×3，矩形 `(1, 2, 3, 4)`，回放到另一个 `RecordingCanvas`，8 条命令相等。不回放到 `KrillaCanvas`。

**错误** — `src/error.rs`

- 与图片相关的现有变体只有 `UnsupportedCanvasOperation { operation: &'static str }`（40–44 行）。`Display`（124–126 行）：`"{operation} is not supported yet"`。
- 不存在：`Error::Image`、`ImageTooLarge`、`UnknownImage`、`LimitExceeded`、`BackendRequired`、`LimitKind`。
- `Error` 派生 `Debug, Clone, PartialEq`（13 行），不是 `Copy`（已有 `String` 字段，例如 `Font { message }`、`Backend(String)`）。

**限额** — `src/limits.rs` 全文是模块注释加 `// Placeholder module.`（1–6 行）。没有 `Limits`、`LimitKind`、`max_image_bytes`、`max_image_pixels`。字体字节上限在别处：`src/fonts.rs` 33 行 `MAX_FONT_BYTES = 32 * 1024 * 1024`，图片没有对应常量。

**Cargo** — `Cargo.toml` 13–18 行

- `default = ["backend-krilla"]`，`backend-krilla = ["dep:krilla"]`。
- `krilla = { version = "=0.8.2", optional = true, default-features = false, features = ["raster-images"] }`。
- 没有单独的 image feature。`simple-text` / `rayon` 未开。
- `Cargo.lock` 里 `krilla` 0.8.2 的依赖包含 `gif`、`image-webp`、`imagesize`、`png`、`zune-jpeg`（205–229 行）。源码没有引用这些 API。

**到 PDF 的接缝（画布之外，但是 `draw_image` 的唯一生产调用链）** — `src/document.rs` 675–680 行

- `Rendered::to_pdf`：`KrillaCanvas::new_with_fonts`，然后 `canvas::replay(&self.recording, &mut pdf)?`，再 `pdf.finish()`。
- 若日志里有 `DrawImage`，`replay` 在 `draw_image` 处返回 `Err`，`finish` 不会被调用。`src/layout` 下没有任何 `draw_image` / `DrawImage` / `DecodedImage` 引用，所以这条链目前走不到图片。

### Flow

1. 没有解码入口。仓库里不存在 `decode_image`、`ImageFormat`、`Image::from_png` / `from_jpeg` / `from_gif` / `from_webp` 的调用。像素字节从不进入 `DecodedImage`。
2. 记录路径：调用方自己拼 `DecodedImage` → `RecordingCanvas::draw_image` → `CanvasCommand::DrawImage`（按值拷贝，类型是 `Copy`）→ `commands()` / `pages()` 可读。始终 `Ok(())`。
3. 回放路径：`replay` 按序再调用 trait。目标若是 `RecordingCanvas`，再记一条。目标若是 `KrillaCanvas`，进入步骤 4。
4. 后端 stub：
   - 已有更早的 `failure` → `open_page_mut` → `ensure_ok` 返回那次错误，不会改成 `draw_image`。
   - 没有打开的页 → 存并返回 `InvalidCanvasState { operation: "draw_image", state: "no page is open" }`。这条没有测试。
   - 有打开的页 → 存并返回 `UnsupportedCanvasOperation { operation: "draw_image" }`。id、矩形、像素尺寸全部丢弃。`marks` 不变。
5. 之后任何会 `ensure_ok` 的调用（含 `finish_page`、再次 `draw_image`、`fill_path`）都返回第一次的错误。`finish` 在写 PDF 之前返回同一错误，不调用 `emit::render`。
6. 即使将来有人往 `marks` 里塞图片，`draw_mark` 今天也没有对应臂，`Surface::draw_image` 不会被调用。

字形对照（已经接上、图片还没有的形状）：`KrillaCanvas::draw_glyph_run`（253–271 行）用 `font_store.resolve` 查出字节，`push(Mark::Glyphs { ... })`；`emit::draw_mark` 再 `Font::new` + `surface.draw_glyphs`。图片没有 `ImageStore`，`KrillaCanvas` 只持有 `FontStore`。

### Files Read

- `src/canvas.rs`（全文：类型、trait、`RecordingCanvas`、`replay`、测试）
- `src/backend/krilla_canvas.rs`（全文）
- `src/backend/emit.rs`（`render` / `draw_mark` / imports；测试 478–497 与 343–361）
- `src/backend/mod.rs`、`src/backend/convert.rs`（仅几何/画笔转换，无图片）
- `src/error.rs`（全文）
- `src/limits.rs`（全文）
- `src/lib.rs`
- `Cargo.toml`；`Cargo.lock` 中 `krilla` 0.8.2 依赖块
- `src/fonts.rs` 1–70、141 行附近（`FontStore` / `MAX_FONT_BYTES`，作对照）
- `src/document.rs` 107–129、660–681 行（`paint` 不画图片；`to_pdf` 走 `replay`）
- `src/geom.rs` 257–296 行（`Rect` 只有 `origin()` / `size()`）
- `docs/quire-design.md`：12、43、51、56–57、184、213、222、337–397、638–662、1017–1025、1093–1131、1235–1258、1424、1512、1534、1543–1544、1570–1582、1640–1645 行
- `docs/提案书.md` 386–419 行（§2.7，产品方向，不是画布签名）

### Boundaries

- **画布 trait 不含 krilla。** `DecodedImage` 只有 id + 像素宽高。设计稿 397 行说：有 `backend-krilla` 时，适配器内部另有 `ImageId -> krilla::image::Image`；trait 不接收 krilla 类型。这张表不存在。
- **krilla 只允许出现在 `src/backend/`。** `krilla_canvas.rs` 故意不 import krilla；唯一构造 `Document`/`Page`/`Surface` 的是 `emit.rs`。图片刀若要 `Image::from_*`，调用点必须落在这个目录（设计稿把 `decode_image` 放在适配器、`#[cfg(feature = "backend-krilla")]`）。
- **记录与发射是两条实现。** `RecordingCanvas` 接受图片命令；`KrillaCanvas` 拒绝。`Rendered::to_pdf` 用 `replay` 把前者送进后者，所以日志里一旦出现 `DrawImage`，PDF 会在 `replay` 失败，而不是画出缺图的 PDF。
- **`emit::render` 只看 `PagePlan`。** 句柄若只放在 `KrillaCanvas` 的旁路表上，现有 `finish` 会在 `render` 之前 `drop` 掉未放进 `Mark` 的状态（字体表就是这样被 drop 的，149 行）。字形的做法是把字节拷进 `Mark::Glyphs`。图片刀要在「mark 自带句柄/字节」和「给 `render` 加表参数」之间选一条；今天两条都不存在。
- **几何品牌已经不是设计稿里的裸 `f32`。** 设计稿 `draw_image_at`（340–347 行）写 `rect.x`、`rect.y`、`rect.size()`、`Error::InvalidGeometry`。当前 `Rect`（`src/geom.rs` 260–295 行）是私有 `origin: Point` + `size: Size`，访问器是 `origin()` 和 `size()`。`to_krilla_size` 已在 `src/backend/convert.rs` 20–23 行，返回 `Option`。
- **限额模块是空的。** 设计稿的 `max_image_bytes`（默认 32 MiB，且必须在 `Image::from_*` 之前）、`max_image_pixels`（4_000 万）、单边 8192、`u64` `saturating_mul`、`Error::ImageTooLarge`，都没有类型可挂。`src/limits.rs` 被 `lib.rs` 公开，但是占位。
- **错误枚举没有图片变体。** 设计稿 1100–1101、1116–1118、1131 行的 `Image { message: String }`、`ImageTooLarge { width, height }`、`LimitKind::{ImageBytes, ImagePixels}`、`BackendRequired` 均未实现。解码失败今天只能落到已有的 `UnsupportedCanvasOperation` 或以后新加的变体。`Error` 已含 `String`，加 `Image { message }` 不会破坏一个并不存在的 `Copy` 约束。
- **布局不持有 `ImageId`。** 提案书 419 行要求 layout 只持 `ImageId` 与 pt 矩形、解码和 krilla 句柄在 backend、`interpolate` 固定 `false`。`src/layout` 尚无图片节点。`Layer`（`document.rs`）只有 `text_at`，没有 `image_at`。

### Non-Obvious Things

- **`raster-images` 是空转的依赖开关。** feature 已钉上，lockfile 已拉入 png/jpeg/gif/webp 解码库，但 `src/` 零引用。设计稿 56、222 行：关掉默认 feature 后必须显式打开 `raster-images`，否则没有 `Image::from_png` 这一族。
- **设计稿声称的 krilla API（未在本仓库调用，只在 `docs/quire-design.md`）：**
  - `Image::from_png` / `from_jpeg` / `from_gif` / `from_webp`：`fn(data: Data, interpolate: bool) -> Result<Image, String>`，位于 `raster-images` 之后（56 行）。`interpolate` 固定 `false`（56、419 提案书、1254、1534 行）。
  - `from_gif` 经 `read_next_frame` 只解第一帧（56、1258 行）。WebP 只取 `from_webp` 返回的那一张。
  - `Image::size() -> (u32, u32)`（56 行）。
  - 未开 `rayon` 时，图片的 `Deferred` 在 `from_*` 内部同步执行；字节上限必须在 `from_*` 之前，否则像素缓冲已经分配（57、1424 行）。
  - `Surface::draw_image` 把 1×1 图像 XObject 变成 `Transform::from_row(width, 0, 0, -height, 0, height)`，根变换之后图像顶边在当前 surface 原点。适配器只 `push_transform(translate(x, y))`，不再翻一次 y（51、340–347、1512 行）。`push`/`pop` 必须成对，否则 krilla panic，适配器不把这次 panic 收成 `Error`。
  - 草图函数 `draw_image_at` 在设计稿里，不在源码里。
- **设计稿 12 行写明它不描述已有源码。** 与现状的差：`decode_image` → `DecodedRaster`（1246–1254 行）、`ImageId -> krilla::image::Image`（397 行）、`LayoutCtx.images: &ImageTable`（638 行）、`FragmentCommand::Image`（662 行）、`Block::Image` / PR 13（1640–1645 行）都还没落地。PR 13 还要求：显式宽高时 `RecordingCanvas` 上是矩形；1×1 PNG；超大编码字节；高于正文框报错；translate 之后不再翻转；GIF/WebP 只测错误字符串映射为 `Error::Image`。
- **无页时的错误变体和有页时不同。** 测试只覆盖「页已打开 → `UnsupportedCanvasOperation`」。无页时 `open_page_mut` 先失败，存的是 `InvalidCanvasState`，根本走不到 `UnsupportedCanvasOperation`。
- **第一次错误粘住。** 先 `draw_image` 再成功画路径是不可能的：`fill_path` 经 `open_page_mut` → `ensure_ok` 返回图片错误。调用方不能忽略 `draw_image` 的 `Err` 仍拿到一份「少了图」的 PDF。`finish` 在「页仍开着」检查之前返回毒化错误。
- **`DecodedImage` 可以被任意伪造。** 字段公开，`from_raw` 公开。记录画布不会发现 id 没有对应解码结果。后端 stub 甚至不读 id。
- **0 像素边合法。** 与早期画布草图里的 `NonZeroU32` / `DecodedImage::new -> Option` 不同；现行结构体没有这个构造器。目标矩形倒是不能为零：`Rect::try_pt` 要求正的宽高。
- **记录测试没有钉住图片坐标。** 顺序测试把 image 算进种类字符串和 `ops.len()==5`，不检查 200×100 或矩形 `(10,30,120,48)`。
- **没有 live session。** 早期审计草图里的 `session.rs` / 在结构体里存 `Document+Surface` 没有落地。现行模型是：`KrillaCanvas` 攒 `PagePlan`，`finish` 消费 self，`emit::render` 短寿命创建 krilla `Document`。图片刀不能把 `krilla::image::Image` 存在 `RecordingCanvas` 或公共 `DecodedImage` 上。

### Open Questions

- 设计稿写的 `Image::from_*(Data, bool) -> Result<Image, String>` 和 `Image::size() -> (u32, u32)` 没有在本仓库对照 krilla 0.8.2 源码复核；只存在于 `docs/quire-design.md` 56 行和 1543 行的「已核实」列表。
- `Surface::draw_image` 的精确签名（第二个参数是 `Size` 还是别的）源码里没有调用，设计稿草图写成 `draw_image(image, size)`（344 行）。未在本仓库验证。
- 图片句柄应放进新的 `Mark` 变体（与 `Glyphs` 自带字节一致），还是放进一张在 `finish` 时传给 `render` 的表：当前类型系统两条都没预留，需要刀自己定。
- `Error::Image { message: String }` 是否仍采用设计稿形状，还是并进已有的 `Backend(String)`：源码没有倾向，只有「现在用 `UnsupportedCanvasOperation`」这一事实。
- 无 `backend-krilla` 时，需要像素尺寸的解码应返回设计稿的 `BackendRequired`（1256 行），还是根本不提供解码函数：两种都不存在。显式宽高、不解码、只在 `RecordingCanvas` 上记矩形，与现行 `draw_image` 行为兼容，因为记录实现不看像素字节。
