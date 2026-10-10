# 刀 9 合成设计 —— image（C1 底 + C2 嫁接）

基线：`feat/image` @ `7cbe823`。权威：提案书 §2.7 / §2.12 > AGENTS.md > 设计稿 krilla 事实。

## Synthesis decision

**Base: C1（字形同构）。** 父会话通读后与 [cross-judge](../synthesis/cross-judge.md) 一致（27 vs 24）。选 C1 的原因是：`Mark` 自带发射材料、`emit::render` / `LayoutCtx` 不动、句柄永远带真实像素尺寸、`DocToken` 挡住跨文档静默错画、门禁 cfg 面更小。

**Grafts from C2:**

1. **不公开 `Align`。** `ImageBox` 用 `center()` / `end()`（默认 Start）。通用对齐名留给样式刀。内部映射到已有 `TextAlign`。
2. **C2 测试清单。** 纯图文档可不调 `font()`；垃圾字节 → `Error::Image` 非 `Backend`；字节上限在 `from_*` 前触发；`rg "Image::from_"` 只命中 `backend/images.rs`（及 emit 若需）；RecordingCanvas 钉 id/矩形 `to_bits`。
3. **`register` 显式拒空字节，失败不改表。**

**Rejected from C2:** `ImageTooLarge` / `ImageWiderThanFrame`（band 已拒 image，超宽复用 `UnbreakableWiderThanFrame`）；`MAX_IMAGE_EDGE`；0 像素哨兵；双 `Vec`；公共/内部双份尺寸枚举；无 backend 下的 `encoded` 旁路。

**Defaults locked:** ListItem/Band 拒 image；TableCell/Column 允许；`spacing_after = 0`；无 backend 时 `load_image` 报错；字节 32 MiB / 像素 40_000_000 为 crate 私有常量；`DocToken` + `LayerContent -> Result` 保留；头好像素坏 → `document` 成功、`to_pdf` → `Backend`（测钉住）。

## Named data shape

| 结构 | 角色 |
| --- | --- |
| `ImageFormat` | 公共枚举 Png/Jpeg/Gif/Webp |
| `Image` | `Copy` 句柄：`DecodedImage` + `DocToken` |
| `ImageBox` | 可选宽高 + Start/Center/End（方法，非公共 Align） |
| `ImageTable` / `Raster` | backend：注册时解码一次，存 krilla `Image` |
| `ImageNode` | layout：`DecodedImage` + 已解 `Size` + `TextAlign` |
| `FragmentCommand::Image` | `DecodedImage` + 相对 fragment 的 `Rect` |
| `Mark::Image` | `Raster` + 页面 `Rect` |
| `LayerItem` | `Text` \| `Image` |
| `Error::Image` / `UnknownImage` | 解码/限额/无 feature；跨文档或越界 id |

## Usage (caller's view)

```rust
let logo = doc.load_image(png_bytes, ImageFormat::Png)?;
doc.image(&logo, |i| {
    i.width(Positive::pt(120.0)?);
    i.center();
});
doc.table(|t| {
    t.row([Cell::image(&logo, |i| i.width(Positive::pt(40.0)?)), Cell::new("说明")]);
});
doc.front(move |page| {
    if page.is_last() {
        page.image_at(&seal, Rect::try_pt(430.0, 680.0, 90.0, 90.0)?);
    }
});
```

## Shape (承 C1)

完整类型草图与模块地图以 `../candidate-1/design.md` 的 Shape 节为准，并应用上述嫁接（删公共 `Align`、`lib.rs` 不导出 `Align`；`ImageBox::{center,end}`）。

关键不变量：

- layout 只持 `ImageId` + px + pt；krilla 只在 `backend/images.rs` + `emit.rs`。
- `ImageNode::new` 解尺寸；`split` 只比较/放置（先宽、再 full_height、再剩余高）。
- 原子：Fit / MoveToNextPage / Err，绝不 Partial。
- `finish` drop 表后 emit；`Mark::Image` 自带 `Raster`。
- 一条 lower：`ImageBox::lower` 服务 flow + cell。

## Tradeoffs accepted

见 C1 Tradeoffs，外加：不公开 Align，换样式刀命名空间干净。

## Next implementation step（可验证序列）

1. `backend/images.rs` + `Mark::Image` + emit；测 1×1 PNG、三页同章 XObject 数（若可观测）、替换毒化桩。
2. `layout/image.rs` + `FragmentCommand` / `with_offset` / `draw_fragment` / `check_content` / `first_baseline`；RecordingCanvas 钉矩形。
3. DSL：`load_image`、`Flow::image`、`Cell::image`、`Layer::image_at`、`Rendered` 带表；集成测试。
4. 七门禁 ×2；本地 commit 于 `feat/image`。

## Open (实现时钉，不挡刀)

- 多页同章是否单 XObject（第一步 backend 测）。
- 超宽是否严格 `>`（默认严格；格宽 ulp 若翻车再加容差）。
- 像素损坏延迟到 `to_pdf`（接受并测）。
