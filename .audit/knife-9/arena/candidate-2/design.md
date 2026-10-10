# Knife 9 candidate 2 — parallel `ImageStore`

Baseline `7cbe823`. Authority: 提案书 §2.7 / §2.12 > `AGENTS.md` > 设计稿里已核对的 krilla 事实。

这条设计把图片资源做成和 `FontStore` 同一条所有权链上的**一张表**：`Document` → `Rendered` → `KrillaCanvas` → `emit::render`。表在 `load_image` 时解码并缓存 krilla 句柄。mark、layout 节点、`FragmentCommand` 都不带编码字节，也不带 `krilla::image::Image`。

## Problem

流内 `image` 和 `behind` / `front` 的 `image_at` 要共用一次解码、同一条 `Canvas::draw_image`，分页规则却不同。layout 只能留下 `ImageId` 和 pt 矩形；`Image::from_*` 只能活在 `backend/`；没开 `rayon` 时 `from_*` 会在返回前同步跑完像素解码。现有字体通道解决不了缓存：`Mark::Glyphs` 自带 `Arc<Vec<u8>>`，因为 `KrillaCanvas::finish` 在 `emit::render` 之前 drop 掉 `FontStore`。电子章每页画一次，mark 里再带一份字节就会按页重复解码。`DecodedImage` 今天只有 id 和像素边，没有分配器。`document()` 在没有 `backend-krilla` 时也要排版，而「只给一边」和「1 px = 1 pt」在 `split` 时就要像素尺寸。

## Usage (caller's view)

注册在 `Document` 上做一次。句柄是 `Copy`，闭包外（相对 `behind` / `front`）造好，流和逃逸层共用。具名字典、路径、URL、`fit` / `cover` 都没有。对齐不另设枚举：`ImageBlock::new` 是 Start，`.center()` / `.end()` 改成框内居中或贴尾。`TextAlign` 留在 crate 内部，这一刀不把它变成公共类型。

```rust
use quire::{document, Cell, ImageBlock, ImageFormat, ImageSize, Positive, Rect, A4};

let seal_png: Vec<u8> = /* 调用方自己读文件；库不收路径 */ vec![];
let logo_png: Vec<u8> = vec![];

let rendered = document(A4, |doc| {
    let seal = doc.load_image(seal_png, ImageFormat::Png)?;
    let logo = doc.load_image(logo_png, ImageFormat::Png)?;

    // 只给宽：高按像素比。对齐相对当前框 avail.width。
    doc.image(
        ImageBlock::new(logo, ImageSize::Width(Positive::pt(120.0)?)).center(),
    );

    doc.table(|t| {
        t.header(["图", "说明"]);
        t.row([
            Cell::image(ImageBlock::new(logo, ImageSize::Natural)),
            Cell::new("标志"),
        ]);
    });

    let stamp = Rect::try_pt(430.0, 680.0, 90.0, 90.0)?;
    // front 闭包在 build 返回之后才跑，必须 move 走 Copy 句柄。
    doc.front(move |page| {
        if page.is_last() {
            page.image_at(&seal, stamp);
        }
    });
    Ok(())
})?;

assert!(rendered.to_pdf()?.starts_with(b"%PDF"));
```

另外两个调用点：

```rust
// 宽高都给：用给定 pt，不保持像素比。可以和同一句柄的 image_at 并存。
doc.image(ImageBlock::new(
    banner,
    ImageSize::Fixed {
        width: Positive::pt(200.0)?,
        height: Positive::pt(40.0)?,
    },
));

// 只给高，贴当前框的尾边。
doc.columns(|row| {
    row.col(|c| {
        c.image(ImageBlock::new(icon, ImageSize::Height(Positive::pt(24.0)?)).end());
    });
});
```

调用方拿到的是 `Image`。他们不看到 `ImageStore`、像素缓存、`ImageId` 分配，也不为 `image_at` 再解码一次。`load_image` 的失败（空字节、超 32 MiB、文件头、像素过大、0 边）在这次调用返回。排版失败（超宽、超高、没开 backend 又要像素尺寸）从 `document(...)?` 返回；`plan_pages` 在 `paint_pages` 之前失败，调用方拿不到 `Rendered`。

`Band` 没有图片方法。列表项里调用 `flow.image` 会在 lower 时被 `check_content` 拒绝（`InvalidCellContent { container: "list item", node: "image" }`）。

## Shape

### 数据

一张 `Vec`，下标就是 `ImageId`。不另建 HashMap，也不按字节去重。

```text
ImageStore
  slots[i]     format + pixels（探测成功时）
               encoded: Option<Arc<Vec<u8>>>  // 仅没探测时保留
  decoded[i]   krilla::image::Image           // 仅 backend-krilla；与 slots 等长
       │
       │  register 写出一次，之后不再改
       ▼
Image { id, pixels: Option<PixelSize> }   // Copy，给 DSL / 层闭包
       │
       │  lower 时拷贝进节点；layout 不拿 store
       ▼
ImageNode { id, pixels, spec, align }
       │
       ▼
FragmentCommand::Image { image: DecodedImage, rect }   // rect 相对 fragment 原点
       │
       ▼
Mark::Image { id, pixel_width, pixel_height, rect }    // 无字节、无 krilla 句柄
       │
       ▼
emit 用 id 下标进 store.decoded
```

`PixelSize` 的两边都是 `NonZeroU32`。探测过的图不可能是 0 边。`None` 只表示这次构建没有探测（没开 `backend-krilla`）。

像素尺寸在 `register` 写进 slot，并拷进 `Image`。节点、`DecodedImage`、mark 上的像素边都是这次写入的拷贝，没有第二个写入者。

### 公共表面

```rust
// src/images.rs — 不 use krilla
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat { Png, Jpeg, Gif, Webp }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Image { /* private: id: ImageId, pixels: Option<PixelSize> */ }

// src/document.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageSize {
    /// 两边都用调用方的 pt，不看像素比。
    Fixed { width: Length<Positive>, height: Length<Positive> },
    /// 高 = 宽 * pixel_h / pixel_w。
    Width(Length<Positive>),
    /// 宽 = 高 * pixel_w / pixel_h。
    Height(Length<Positive>),
    /// 1 px = 1 pt。
    Natural,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageBlock { /* source: Image, size: ImageSize, align: TextAlign */ }

impl ImageBlock {
    pub fn new(source: Image, size: ImageSize) -> Self { /* align = TextAlign::Start */ }
    pub fn center(self) -> Self { unimplemented!() }
    pub fn end(self) -> Self { unimplemented!() }
}

impl Document<'_> {
    /// 校验字节上限，分配 id。开了 backend-krilla 时在这里解码并缓存句柄。
    pub fn load_image(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<Image> {
        unimplemented!()
    }
}

impl Flow {
    pub fn image(&mut self, block: ImageBlock) { /* push lower_image(block) */ }
}

impl Cell {
    pub fn image(block: ImageBlock) -> Self { unimplemented!() }
    // Cell::new(text) 保持文字格。col_span / row_span 照旧可链式调用。
}

impl Layer {
    /// 页面坐标，不扣 margin，不参与分页。闭包仍返回 ()。
    pub fn image_at(&mut self, source: &Image, rect: Rect) { unimplemented!() }
}
```

`lib.rs` 追加重导出：`Image`、`ImageFormat`、`ImageSize`、`ImageBlock`。不导出 `ImageStore`、`PixelSize`、`DecodedImages`。

`load_image` 只在 `Document` 上。`Flow` / `Columns` / `Table` 碰不到 store，嵌套闭包不能注册第二份。

### 模块

| 模块 | 拥有的知识 | 这一刀 |
| --- | --- | --- |
| `src/images.rs` | id 分配、字节上限、slot、`Image` | 新文件。`#[cfg(feature = "backend-krilla")]` 字段 `decoded: DecodedImages` |
| `src/backend/decode.rs` | `Image::from_*`、`interpolate = false`、像素上限、句柄缓存 | 新文件。全 crate 只有这里调用 `from_png` / `from_jpeg` / `from_gif` / `from_webp` |
| `src/layout/image.rs` | 尺寸三规则、对齐、原子 split | 新文件。不 import `ImageStore` |
| `src/document.rs` | DSL、`lower_image`、把 store 放进 `Rendered` | `load_image` / `Flow::image` / `Cell::image` / `Layer::image_at` |
| `src/layout/flow.rs` | 命令平移与绘制 | `FlowNode::Image`、`FragmentCommand::Image` |
| `src/layout/lower.rs` | 容器矩阵 | Image：Column、TableCell 允许；ListItem、Band 拒绝 |
| `src/layout/decorate.rs` | 层测量 | `LayerDraw::Image` |
| `src/layout/list.rs` | 首行基线 | 图片命令返回 `None` |
| `src/backend/krilla_canvas.rs` | mark 缓冲 | `Mark::Image` 只有 id、像素边、`Rect`。`finish` 把 store 传给 `render`，不 drop |
| `src/backend/emit.rs` | krilla `Surface` | 按 id 取句柄；`push_transform` → `draw_image` → `pop` |
| `src/error.rs` | 失败类型 | 下面三个变体 |
| `src/canvas.rs` | 已有 `draw_image` | 协议不改。加一个 crate 内构造器，避免 layout 手写 0 边 |
| `src/limits.rs` | 占位 | 不动。上限常量放在 `images.rs`，对齐 `MAX_FONT_BYTES` |

依赖：`document` → `images` + `layout`。`layout` → `images` 只为了 `PixelSize`（以及已有的 `ImageId`）。`images` 在 feature 打开时持有 `DecodedImages`，所以指向 `backend::decode`。`decode` 使用 `ImageFormat` / `PixelSize`，**结构体本身不包含 `ImageStore`**，类型图不成环。`images.rs` 的模块文档写明：这个文件禁止 `use krilla`。`krilla_canvas.rs` 继续不 `use krilla`；它只问 store「这个 id 在不在」。

`LayoutCtx` 仍然只有 `fonts`。图片节点的 `split(self, avail)` 不接收 ctx。

### `ImageStore::register`

```rust
pub(crate) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_IMAGE_PIXELS: u64 = 40_000_000;
pub(crate) const MAX_IMAGE_EDGE: u32 = 8192;

pub(crate) struct ImageSlot {
    format: ImageFormat,
    pixels: Option<PixelSize>,
    /// 探测成功后是 None。krilla Image 已经吃掉编码字节。
    encoded: Option<Arc<Vec<u8>>>,
}

impl ImageStore {
    pub(crate) fn register(&mut self, bytes: Vec<u8>, format: ImageFormat) -> Result<Image> {
        if bytes.is_empty() {
            return Err(Error::Image { message: "image bytes are empty".into() });
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(Error::Image {
                message: format!("image is {} bytes; max is {MAX_IMAGE_BYTES}", bytes.len()),
            });
        }
        // 先解码（可能失败），成功后再 push。失败不改表。
        #[cfg(feature = "backend-krilla")]
        {
            let (pixels, handle) = crate::backend::decode::decode(bytes, format)?;
            let id = self.push_slot(ImageSlot { format, pixels: Some(pixels), encoded: None })?;
            self.decoded.push(handle); // push_slot 失败时不会走到这里
            if self.slots.len() != self.decoded.len() {
                return Err(Error::Image { message: "image store and decode cache diverged".into() });
            }
            return Ok(Image { id, pixels: Some(pixels) });
        }
        #[cfg(not(feature = "backend-krilla"))]
        {
            let id = self.push_slot(ImageSlot {
                format,
                pixels: None,
                encoded: Some(Arc::new(bytes)),
            })?;
            Ok(Image { id, pixels: None })
        }
    }
}
```

`push_slot` 用 `u32::try_from(len)` 发 id，溢出时 `Error::Image { message: "too many registered images" }`，并且不 push。id 0 是第一张图。

同一个字节序列注册两次得到两个 id、两份解码。这是句柄语义，不是具名字典。

### `decode`（仅 `backend/decode.rs`）

```rust
pub(crate) struct DecodedImages {
    handles: Vec<krilla::image::Image>, // 下标 = ImageId
}

pub(crate) fn decode(bytes: Vec<u8>, format: ImageFormat) -> Result<(PixelSize, krilla::image::Image)> {
    let data = krilla::Data::from(bytes);
    let image = match format {
        ImageFormat::Png => krilla::image::Image::from_png(data, false),
        ImageFormat::Jpeg => krilla::image::Image::from_jpeg(data, false),
        ImageFormat::Gif => krilla::image::Image::from_gif(data, false),
        ImageFormat::Webp => krilla::image::Image::from_webp(data, false),
    }
    .map_err(|message| Error::Image { message })?;
    let (w, h) = image.size();
    if w == 0 || h == 0 {
        return Err(Error::Image { message: "image pixel size is 0".into() });
    }
    let count = u64::from(w).saturating_mul(u64::from(h));
    if w > MAX_IMAGE_EDGE || h > MAX_IMAGE_EDGE || count > MAX_IMAGE_PIXELS {
        return Err(Error::ImageTooLarge { width: w, height: h });
    }
    let pixels = PixelSize {
        width: NonZeroU32::new(w).unwrap(),
        height: NonZeroU32::new(h).unwrap(),
    };
    Ok((pixels, image))
}
```

`interpolate` 只有这里的字面量 `false`，没有字段可以改。GIF 走 krilla `read_next_frame`（第一帧）；WebP 走 `from_webp` 返回的那一张。不设帧号参数。

文件头失败在 `from_*` 的 `Err(String)`，映射成 `Error::Image`。像素数据损坏时，krilla 把错误留在 `Deferred` 里，序列化才变成 `KrillaError::Image`，现有 `emit` 把它映射成 `Error::Backend`。`document()` 对这种图可以成功，`to_pdf()` 失败。测试要钉住这个分界，不要在 `load_image` 里把 `Backend` 吞掉或改写成 `Image`。

字节上限在 `from_*` 之前。像素上限在 `from_*` 之后，拦不住已经发生的同步分配；这是没开 `rayon` 时的剩余风险。

### 所有权穿过绘制

```rust
pub struct Rendered {
    recording: RecordingCanvas,
    #[cfg_attr(not(feature = "backend-krilla"), allow(dead_code))]
    fonts: FontStore,
    #[cfg_attr(not(feature = "backend-krilla"), allow(dead_code))]
    images: ImageStore,
}

// to_pdf
let mut pdf = KrillaCanvas::new_with_resources(Arc::new(self.fonts), self.images);
replay(&self.recording, &mut pdf)?;
pdf.finish()

// KrillaCanvas::finish
drop(font_store); // 字形 mark 仍自带字节。这一刀不改字形。
emit::render(info.as_ref(), &closed_pages, serialization, &images)
```

`new()` / `new_with_fonts` 继续给测试一个空 `ImageStore`。`to_pdf` 只用 `new_with_resources`。空 store 上的 `draw_image` 因 id 不存在而毒化，替换今天的 `UnsupportedCanvasOperation`。

```rust
pub(super) enum Mark {
    // Fill / Stroke / Glyphs 不变
    Image {
        id: ImageId,
        pixel_width: u32,
        pixel_height: u32,
        rect: Rect,
    },
}

fn draw_image(&mut self, image: &DecodedImage, rect: Rect) -> Result<()> {
    self.open_page_mut("draw_image")?;
    if !self.images.contains(image.id) {
        return self.fail(Error::Image {
            message: format!("image {} is not registered", image.id.as_u32()),
        });
    }
    let page = self.open_page_mut("draw_image")?;
    page.marks.push(Mark::Image {
        id: image.id,
        pixel_width: image.pixel_width,
        pixel_height: image.pixel_height,
        rect,
    });
    Ok(())
}
```

`contains` 在 feature 构建里看 `decoded` 的长度（id 下标），不把 `krilla::image::Image` 暴露给 `krilla_canvas.rs`。

### layout 节点

```rust
// src/layout/image.rs
pub(crate) enum ImageSpec {
    Fixed { width: f32, height: f32 },
    Width(f32),
    Height(f32),
    Natural,
}

pub(crate) struct ImageNode {
    pub(crate) id: ImageId,
    pub(crate) pixels: Option<PixelSize>,
    pub(crate) spec: ImageSpec,
    pub(crate) align: TextAlign,
}

impl ImageNode {
    pub(crate) fn split(self, avail: Frame) -> Result<Split> { unimplemented!() }
}
```

`document.rs` 里唯一的 lower：

```rust
fn lower_image(block: ImageBlock) -> FlowNode {
    let spec = match block.size {
        ImageSize::Fixed { width, height } => ImageSpec::Fixed {
            width: width.as_pt(),
            height: height.as_pt(),
        },
        ImageSize::Width(w) => ImageSpec::Width(w.as_pt()),
        ImageSize::Height(h) => ImageSpec::Height(h.as_pt()),
        ImageSize::Natural => ImageSpec::Natural,
    };
    FlowNode::Image(ImageNode {
        id: block.source.id(),
        pixels: block.source.pixels(),
        spec,
        align: block.align,
    })
}
```

`Flow::image` 和 `Cell::image` 的 lower 都走这个函数。格的 spans 仍在 `table::Cell::with_span`。`Flow::image` 用现有 `push`，本身不返回 `Result`。

`split` 伪代码（绝不返回 `Partial`；`spacing_after` 为 0）：

```text
(w, h) = display_size(spec, pixels)?
    Fixed: 用存好的 pt
    Width:  pixels? 否则 Error::Image { "image pixel size requires the backend-krilla feature" }
            h = f64(w) * f64(pixel_h) / f64(pixel_w)，再转 f32
    Height: 对称
    Natural: pixels? 然后 (pixel_w as f32, pixel_h as f32)
    结果非有限或 ≤ 0 → Error::Image { "image display size is not positive" }

w > avail.width        → Error::ImageWiderThanFrame { width: w, avail: avail.width }
h > avail.full_height  → Error::UnsplittableTallerThanFrame
h > avail.height       → Split::MoveToNextPage(FlowNode::Image(self))
否则 x = match align
    Start  → 0
    Center → (avail.width - w) / 2
    End    → avail.width - w
Fit {
  height: h,
  fragment: Fragment {
    spacing_after: 0,
    commands: [FragmentCommand::Image {
      image: DecodedImage::from_placed(id, pixels),
      rect: Rect::try_pt(x, 0, w, h)?,
    }],
  },
}
```

先比宽，再比整页高，再比剩余高。又宽又高时报告 `ImageWiderThanFrame`。节点自己比较 `full_height`，不依赖分页器在 `y == 0` 时把 `MoveToNextPage` 改写成超高。

`avail.width` 就是 §2.7 的当前框 `W`：正文用 body 宽，栏用栏宽，格子用 `table::measure` 算出来的 `inner_w`。表已经用 `content.with_offset(x + inset, y + inset)` 把格内容平移进格子；图片命令必须实现 `with_offset`，否则格内图会贴在行片段原点。

`DecodedImage::from_placed`：有 `PixelSize` 就写入两边；`None` 写 `pixel_width: 0, pixel_height: 0`。这是现有公共结构（两边都是 `u32`）上的未探测编码。feature 打开时 `register` 总会探测，走到 `KrillaCanvas` 的图两边都非 0。0 边到达 `draw_image` 且 id 存在时，`emit` 再对一下句柄的 `size()`，不一致就 `Error::Image`。

没开 feature 时：`Fixed` 可以排进 `RecordingCanvas`（矩形有效，像素边为 0）。`Width` / `Height` / `Natural` 在 `split` 返回 `Error::Image`。`to_pdf` 在这个构建里不存在。feature 打开时不为 `Fixed` 单独跳过解码：同一 `Image` 可以被流、格子和 `image_at` 共用，像素尺寸是句柄的属性，不是某一次放置的属性。跳过解码会迫使 layout 在 `split` 时回写 store。

### 绘制命令

`FragmentCommand::Image { image: DecodedImage, rect: Rect }`。`rect` 相对 fragment 原点。页上的最终位置是 `PlacedFragment.origin + rect`，和字形的两段平移同一套。

- `Fragment::with_offset`：只平移 `rect.origin`，宽高不动。
- `draw_fragment`：加上 placed origin 后 `canvas.draw_image`。
- `list::first_baseline`：图片臂返回 `None`。
- `FlowNode::split` / `check_content` / `node_name`：补 `Image`。`node_name` 返回 `"image"`。
- `error::Display`：补新变体。

层：

```rust
pub(crate) enum LayerDraw {
    Text(LayerText),
    Image { image: DecodedImage, rect: Rect },
}
// LayerContent 改为 Fn(&PageInfo) -> Vec<LayerDraw>
```

`measure_layer` 对图片臂直接推 `FragmentCommand::Image`，`rect` 就是调用方的页面矩形。片段原点保持 `(0, 0)`，高度保持 0。不查 `W`，不查是否超出纸张，不占正文高度。叠层顺序仍由 `paint_pages` 保证：behind → header → 正文 → footer → front。

`image_at` 放进层闭包时不能失败。矩形在进闭包之前就由 `Rect` 保证宽高为正。

### emit

```rust
pub(super) fn render(
    info: Option<&DocumentInfo>,
    pages: &[PagePlan],
    serialization: Serialization,
    images: &ImageStore,
) -> Result<Vec<u8>> { unimplemented!() }

// draw_mark 的 Image 臂
Mark::Image { id, pixel_width, pixel_height, rect } => {
    let image = images.decoded().get(*id)?; // Arc clone，不 from_*
    let (w, h) = image.size();
    if (*pixel_width, *pixel_height) != (w, h) {
        return Err(Error::Image {
            message: format!("image {} pixel size does not match the store", id.as_u32()),
        });
    }
    let size = to_krilla_size(rect.size()).ok_or(Error::InvalidGeometry)?;
    // 失败发生在 push 之前，避免 Surface::finish 因不成对的 pop 而 panic。
    surface.push_transform(&krilla::geom::Transform::from_translate(
        rect.origin().x().as_pt(),
        rect.origin().y().as_pt(),
    ));
    surface.draw_image(image, size);
    surface.pop();
}
```

几何恒等映射，不翻 y。`draw_image` 把图的顶边放在当前原点，所以平移量就是矩形左上角。同一 id 在多页、多个 mark 里只对应 `decoded` 里的一个句柄。

### 错误

```rust
/// 空字节、超字节上限、文件头、0 像素边、未知 id、store/cache 长度不一致、
/// 显示尺寸不是正数、未探测却要像素尺寸。
Image { message: String },

/// 解码后的像素边或像素数超过 crate 内上限。句柄已丢弃。
ImageTooLarge { width: u32, height: u32 },

/// 显示宽大于当前框。不缩小。
ImageWiderThanFrame { width: f32, avail: f32 },
```

高于整页正文框复用 `UnsplittableTallerThanFrame`。不用 `UnbreakableWiderThanFrame`：`measure_band` 会把它改写成 `DecorationWiderThanFrame`，图片超宽要保持自己的变体。

### 接口深度

公共入口是 `load_image`、`ImageBlock`（含对齐链式方法）、`Flow::image`、`Cell::image`、`Layer::image_at`。藏在后面的有：32 MiB 预检、同步解码、像素上限、`interpolate = false`、GIF/WebP 第一帧、id 分配、句柄缓存、宽高比、原子分页、栏/格的 `W`、两段坐标平移、krilla 变换。调用方一次注册，然后要么放进流，要么给一个页面矩形。

`LayoutCtx` 不加图片表。`Flow` 不加 store 的可变借用。mark 不加字节。这三处如果开口，下一处调用就会把缓存策略抄进 layout 或抄进每一页 mark。

### 红旗自检

- 浅模块：注册和放置各一个入口，解码、限额、分页、发射都在被调方。
- 信息泄漏：公共 API 没有 krilla 类型。`ImageFormat` 是提案书要求调用方传入的格式，不是传输类型重导出。
- 按时间拆模块：解码和句柄缓存都在 store 的 `register` 与 `decoded` 查找上。layout 只拥有框内几何，因为那一层不能 `use krilla`。
- 透传：`Flow::image` 做 `ImageSize` → `ImageSpec` 的边界转换。`draw_image` 做 id 存在性检查再写入 mark。`emit` 才接触 krilla。三处类型不同。
- 分裂所有权：字节或句柄只由 `ImageStore` 写。`register` 先完成 `decode`，再 `push_slot`，再 `decoded.push`。layout 拿到的是 `Copy` 快照。
- 一件事两套做法：注册只有 `Document::load_image`。流内放置只有 `Flow::image(ImageBlock)`。逃逸只有 `image_at`。格子复用 `ImageBlock`，不另做一套尺寸参数。
- 可导入的内部：`ImageStore` / `DecodedImages` / `PixelSize` 均为 `pub(crate)`，`lib.rs` 不导出。layout 文件不 import `ImageStore`。
- 手抄清单：格式枚举的 `match` 是穷尽的，漏一个 `from_*` 编不过。`slots` 与 `decoded` 的长度在 `register` 和 `get` 检查。像素边只有 `register` 写入。

### 测试（实现时钉，这里只定断言落点）

- layout / DSL：`RecordingCanvas` 上的 `DrawImage`，用 `to_bits()` 比矩形。固定宽高、单边比例、`Natural`、Start/Center/End、超宽、超高、剩余高度不够则整张到下一页、栏内 `W`、格内 `with_offset`、`image_at` 不扣 margin 且 `is_last` 才出现、同一 id 的流和章是两条命令。列表项是 `InvalidCellContent`。只含图片的文档不必 `font()`。
- 无 backend：`Fixed` 录出矩形且像素边为 0；`Natural` 是 `Error::Image`。
- backend：1×1 PNG 的 `to_pdf` 以 `%PDF` 开头。垃圾字节在 `load_image` 是 `Error::Image`，不是 `Error::Backend`。`MAX_IMAGE_BYTES + 1` 在调用 `from_*` 之前失败。未知 id 毒化 `finish`。删掉 `unsupported_image_poisons_finish`。内容流数字只出现在 backend 测试里。
- `rg "Image::from_" src` 只命中 `backend/decode.rs`。多页重复 `image_at` 不会在 mark 里出现编码字节。

## Synthesis decision

n/a — filled by orchestrator

## Tradeoffs accepted

- feature 打开时，即使用户只给了固定 pt，`load_image` 也完整解码并一直把句柄留到 `Rendered` 丢弃。换来的是：像素尺寸在 `split` 之前就固定，layout 不用 store，同一张图多页绘制不再 `from_*`。
- 接受「头好、像素坏」的图在 `document()` 成功、`to_pdf()` 以 `Error::Backend` 失败。换来的是不在库内再写一套像素解码器去提前 `wait` krilla 的 `Deferred`。
- 接受像素上限挡在同步解码之后，极端大图仍可能在 `from_*` 里分配。换来的是字节上限仍然在 `from_*` 之前，并且不引入第二份只读文件头的依赖。
- 接受 `slots` 与 `decoded` 是同一结构里的两段 `Vec`，用长度检查绑在一起。换来的是 `images.rs` 不 `use krilla`，句柄类型留在 `backend/decode.rs`。
- 接受未探测图在 `DecodedImage` 上写成 0 像素边。换来的是不改已经公开的画布命令结构；feature 构建的生产路径不会写出 0。
- 接受超高复用 `UnsplittableTallerThanFrame`，超宽用新的 `ImageWiderThanFrame`。换来的是超高和 `keep_together` 同一个信号，超宽不会被页眉路径改名。
- 接受 4_000 万像素和单边 8192 这两个设计稿数字先做成私有常量。换来的是解码后有一个确定的拒绝点；数字本身仍可调，见开放问题。
- 接受层闭包里的矩形可以画出纸外。换来的是和 `text_at` 同一条「调用方给页面坐标」的规则，这一刀不加第二套裁剪政策。
- 接受列表项拒绝图片。换来的是提案书没写的容器不先放开。
- 接受注册两次就是两张图。换来的是不做内容寻址，也不提前做具名字典。

## Alternatives considered

- **mark 自带编码字节（字形类比）。** `register` 只存字节，`Mark::Image` 携带 `Arc<Vec<u8>>` + 格式，`finish` 可以继续 drop 旁路状态，在 `emit` 里 `from_*`。调用方表面可以一样小，但缓存策略会漏到每一条 mark：要么每页重复同步解码，要么 `emit` 再按 id 建一张表。那张表就是 store，只是建晚了，而且字节已经在 mark 里复制过。本设计把表放在和 `FontStore` 相同的所有权链上，mark 保持 id + 矩形。
- **`LayoutCtx` 持有 `ImageStore`，节点只存 id，`split` 时再查像素。** 尺寸规则可以晚绑定，固定尺寸也更容易临时跳过解码。代价是 layout 知道 store 的可变性，层闭包和流在测量期都能触发解码，`image_at` 的闭包今天不能返回 `Result`。本设计在 `register` 把像素尺寸封进 `Copy` 句柄，`split` 是纯函数。
- **固定宽高在 feature 构建里也不解码，等 `to_pdf` 再解码。** `document()` 对「两边都给」更便宜，也贴着「可以不解码就录矩形」那句。同一句柄再用于 `Natural` 或单边时，解码点又回到 layout。本设计让解码跟随句柄，不跟随某一次放置。
- **单独的文件头解析器，只为 layout 提供像素边，krilla 留到发射。** 可以避免在 `register` 做全量像素解码。它是第二套解码，格式和「0 边 / 第一帧」要和 krilla 手写对齐。本设计只用 krilla `from_*`，并把返回的句柄留在 store 里。

## Open questions and risks

- 像素数 4_000 万、单边 8192 没有写进提案书。这一刀是把它们做成私有常量，还是只保留已点名的 32 MiB 字节上限？
- 纸外的 `image_at` 是否一直允许？现在与 `text_at` 相同，不检查页框。
- 列表项拒绝图片是否维持到提案书补一行？这一刀按 arena frame 拒绝。
- `TextAlign` 要不要在图片对齐落地时改成对调用方可见的名字？这一刀用 `ImageBlock::center` / `end` 避开公开这个名字。
- 头合法、像素损坏的图以 `Error::Backend` 在 `to_pdf` 失败。是否要在实现前接受这条分界，还是要求 `load_image` 就失败？后者要额外碰 krilla `Deferred`，这一稿没有那条路径。

## Next implementation step

先加 `Error::Image`、`Error::ImageTooLarge`、`Error::ImageWiderThanFrame`，并实现 `backend/decode.rs` 的 `decode` 与 `ImageStore::register`，让句柄和缓存存在，再写 `ImageNode::split`。
