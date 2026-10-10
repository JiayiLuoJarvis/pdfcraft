# 刀 9 交叉评审 —— image

评审对象：C1（字形同构，`candidate-1/design.md`）、C2（并行 `ImageStore`，`candidate-2/design.md`）。
只读评审，未改 `src/`。已对照源码核实：`ImageId::from_raw` / `DecodedImage` 是公共类型且字段公开（`canvas.rs:56-81`）；`KrillaCanvas::finish` 在发射前 `drop(font_store)`（`krilla_canvas.rs:149`）；`measure_band` 在 `check_content(Container::Band, ..)` **之后**才把 `UnbreakableWiderThanFrame` 改写成 `DecorationWiderThanFrame`（`decorate.rs:171-185`）；`LayerContent` 现在不可失败（`decorate.rs:55`）。

## 评分

| # | 评分项 | C1 | C2 |
| --- | --- | --- | --- |
| 1 | §2.7 尺寸规则与原子拆分语义 | 5 | 4 |
| 2 | 共用解码；公共 API 不含 krilla | 4 | 4 |
| 3 | `FragmentCommand::Image` → `Canvas::draw_image`；RecordingCanvas 钉矩形 | 5 | 5 |
| 4 | 单元格允许图片；band / 列表项拒绝 | 5 | 5 |
| 5 | 懒：薄 DSL、一条降级路径、backend 真实绘制 | 4 | 3 |
| 6 | 门禁可行（fmt / clippy×2 / test×2 / doc×2） | 4 | 3 |
| | **合计** | **27** | **24** |

评分理由：

1. C1 在节点构造时（`ImageNode::new` → `resolve_size`）就算好尺寸，`split` 只负责比较和放置；比较顺序是先宽、再整页高、再剩余高。C2 的规则同样正确，`ImageSize` 枚举也把三种情况写进了类型。但它把尺寸计算推迟到 `split`，并引入 `pixels: Option<PixelSize>`，还在公共的 `DecodedImage` 上用「0 像素边」表示「未探测」。这样一来，layout 里有了一条 `Option` 分支，公共结构上也多了一个带内哨兵值。
2. 两份设计都只解码一次，`image_at` 与流内图共用同一份解码结果，公共 API 也都不含 krilla 类型。C1 的扣分点是 krilla `Image` 进了 crate 内的 `Mark::Image`（包在 `Raster` 里），并作为 cfg 字段挂在 `Document` / `Rendered` 上。C2 的扣分点是非 backend 模块 `images.rs` 带着一个 cfg 字段 `backend::decode::DecodedImages`，而 `layout` 又依赖 `images`（为了 `PixelSize`），模块层面成了 `images ↔ backend` 的往返依赖；此外跨文档句柄会静默画成本文档里同下标的另一张图，C2 没有防护。C1 用 `DocToken` 把这个问题挡在了 DSL 边界。两边扣分点不同，分数相同。
3. 两份都是两段平移（`with_offset` 加 `draw_fragment`），都用 `to_bits` 比较矩形。C2 列出的测试清单更完整。
4. 两份的容器矩阵相同：Column、TableCell 允许；ListItem、Band 拒绝；都覆盖了 `node_name` 和 `list::first_baseline`。
5. C1：降级只有 `ImageBox::lower` 一条路径，`emit::render` 签名不变，`Mark` 自带发射材料，与 `Mark::Glyphs` 同构。代价是多出 `DocToken`、`LayerContent -> Result` 和公共 `Align`。C2：`ImageSize → ImageSpec` 是一比一的镜像转换；`slots` / `decoded` 两个 `Vec` 靠运行时长度检查保持一致；emit 里还要再核对一遍像素边；`emit::render` 要多一个参数；新增三个错误变体。管道比 C1 多。
6. C2 在无 backend 构建下写入 `encoded: Option<Arc<Vec<u8>>>` 却从不读取，加上 `#[cfg_attr(.., allow(dead_code))]`，在 `clippy -D warnings` × 两种 feature 下容易翻车。`register` 里 cfg 分支的提前 `return` 也会让两种构建下的控制流各不相同。C1 只有 3 处 cfg（Document 字段、Rendered 字段、`load_image` 分支），风险较小。

## 红旗命中

| 红旗 | C1 | C2 |
| --- | --- | --- |
| 浅模块 | 无 | 无 |
| 信息泄漏 | 轻微：`Raster` 把 krilla `Image` 带进 `Mark`，但被 `pub(super)` 和私有字段封住，`krilla_canvas.rs` 不 import krilla | **命中**：「0 像素 = 未探测」这个约定同时出现在 `DecodedImage::from_placed`、`split` 和 emit 的复核里；同一个约定散落在三处 |
| 时序拆分 | 无 | 无 |
| 直通方法 | 无 | 轻微：`lower_image` 主要是做 `ImageSize → ImageSpec` 的形状转换 |
| 所有权分裂 | 无（表线性移动：Document → Rendered → KrillaCanvas → drop） | **命中（弱）**：像素边同时存放在 slot、`Image`、节点、`DecodedImage`、`Mark` 五处；只有一个写入者，但 emit 要靠复核来防止不一致，说明设计自己也不信任这些副本 |
| 一事两法 | 无 | 无 |
| 可导入内部 | 无 | 无 |
| 手工同步清单 | 无（新变体都在穷尽 `match` 中） | **命中**：`slots` 与 `decoded` 两个平行 `Vec` 靠运行时 `len` 比较绑定（设计自己也承认）；`ImageSize` / `ImageSpec` 是两份一一对应的枚举 |
| 依赖方向（AGENTS.md） | `document` → `backend::ImageTable`（cfg），与字体现状同级 | `layout` → `images` →（cfg）`backend`，`emit` → `images`；layout 离 krilla 只隔一层 |

## 推荐底稿：C1

- **对后续维护者更好扩展。** C1 只是把字体已经走通的路再走一遍：注册一次、拿回句柄、`Mark` 自带材料、`finish` 丢表。维护者看懂字体就能看懂图片。`emit::render` 签名不变，`LayoutCtx` 不变，`paginate` / `table` / `columns` 都不用改。C2 要求维护者同时理解 `ImageStore`、`ImageSlot`、`DecodedImages`、`PixelSize`、`ImageSpec` 五个内部类型和两个平行 `Vec` 的不变量。
- **内部状态更少，失败模式更少。** C1 的 layout 里没有 `Option<px>`，也没有 0 像素哨兵；句柄永远带真实像素尺寸。代价是无 backend 时 `load_image` 一律报错，这在 frame 默认值允许的范围内（"both sides given: **may** record rect"），而且默认 feature 是开着的。
- **边界更清楚。** 跨文档句柄在 `document()` 返回前就以 `UnknownImage` 报错，不会静默画错图。krilla 只出现在 `backend/images.rs` 和 `emit.rs` 两个文件里。错误只新增 `Image { message }` 和 `UnknownImage`，沿用 `Error::Font { message }` 的先例。
- **公共面方面需要说明。** C1 比 C2 多一个公共 `Align`，用一个移植（第一条）去掉后，两边的公共面基本持平。C1 的闭包 builder `doc.image(&img, |i| ..)` 与 §1 已定的闭包式 DSL（paragraph / table / columns）一致；C2 的 `ImageBlock::new(img, ImageSize::..)` 是值 builder，风格上不一致。

C1 的承重假设是开放问题 2（同一 `Raster` 在多页上 clone，krilla 是否只写一个 XObject）。C2 同样依赖这一点：它每页传入的也是同一个句柄。所以这个风险不影响两者的取舍。但实现的第一步应按 C1 的建议，先写「三页同章，数 `/Subtype /Image`」的 backend 测试。

## 值得从 C2 移植的部分

- **不公开 `Align`。** 在 `ImageBox` 上用 `center()` / `end()`（或 `align` 只收 crate 内的 `TextAlign`），默认 Start。通用对齐名留给样式那一刀定，这样公共面少一个类型。
- **C2 的测试清单整份采用。** 包括：只含图片的文档不需要 `font()`；格内图经 `with_offset` 落进格子；`image_at` 不扣 margin，配合 `is_last` 只在末页出现；同一 id 的流内图和章是两条 `DrawImage`；垃圾字节在 `load_image` 报 `Error::Image` 而不是 `Error::Backend`；`MAX_IMAGE_BYTES + 1` 在调用 `from_*` 之前失败；删除 `unsupported_image_poisons_finish`；`rg "Image::from_" src` 只命中一个文件。
- **`register` 里显式拒绝空字节**，并保证失败时不改表（先解码，成功后再 push）。C1 的伪代码隐含了这一点，但没有写明。

## 拒绝项

- C2 的 `ImageTooLarge { width, height }` 和 `ImageWiderThanFrame`。第一个对调用方没有可分支的意义；第二个与 `UnbreakableWiderThanFrame` 语义重复。C2 给出的理由（band 路径会改写这个错误）不成立：`measure_band` 在 split 之前就用 `check_content` 拒绝了图片（`decorate.rs:171`），图片超宽的错误不会走到改写那一步。
- C2 的 `MAX_IMAGE_EDGE = 8192`。提案书和设计稿都没有这个数，属于新增的未确认限额。
- C2 在公共 `DecodedImage` 上用 0 像素边表示「未探测」，以及 emit 里的像素边复核。
- C2 的 `slots` / `decoded` 双 `Vec`，以及无 backend 构建下保留但从不读取的 `encoded` 字节。
- C2 的 `ImageSize` → `ImageSpec` 镜像枚举。如果想把 §2.7 的三条规则写进类型，只在 `layout/image.rs` 里留一份内部枚举即可，不需要一份公共、一份内部。
- C1 的 `DocToken` 保留，不拒绝。它防的是一种真实存在的静默错误，代价大约 30 行，外加一次 `LayerContent -> Result`，而这次改动与已有的 `BandContent` 同形。若综合阶段要压 diff，它是最先可以砍掉的部分，但砍掉时要在 CHANGELOG 或文档里写明「句柄不得跨文档使用」。
