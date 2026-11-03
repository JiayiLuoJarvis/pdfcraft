# 把 front 页脚迁到 band marks

这份说明记录从 `SectionSpec.front` 上的页脚 `LayerMark` 迁到 `SectionSpec.footer` 上的 `BandMark`。这些类型已经在仓库里。当前 `pdfcraft-pdf` 版本是 `0.5.0`。`pdfcraft-forms`、`pdfcraft-orgchart` 和 `pdfcraft-post` 的类型不变。

按下面的顺序改。第 1 步是编译错误。后面的步骤能编译通过，但坐标、字号或越界错误会和旧页脚不一致。

## 1. 给 `BandSpec` 补上 `marks`

`BandSpec` 新增公共字段 `marks: Vec<BandMark<K>>`。结构体字面量必须写出这个字段，或者用 `..BandSpec::default()`。`Default` 里 `marks` 是空的。

以前：

```rust
BandSpec {
    inset: BandInset::PageMargins,
    left: Some(slot),
    right: None,
}
```

现在：

```rust
BandSpec {
    inset: BandInset::PageMargins,
    left: Some(slot),
    right: None,
    marks: Vec::new(),
}
```

只改页脚标记、左右槽仍用默认值时，写成 `marks: vec![...]` 加上 `..BandSpec::default()`。

## 2. 把每个 `LayerMark` 改成 `BandMark`

不旋转的标记写成 `BandMark::Shape(LayerShape::...)`。旋转组写成 `BandMark::Rotated { degrees, pivot, shapes }`。删掉 `PageWhen`。按页选择时用 `BandFn::from_fn`，读 `query.number.get() % 2`，或对该页返回 `Ok(None)`。

`LayerMark` 用页面左上角。footer band 用页脚带左上角。`x` 不变。`y` 按下面的公式换算：

```text
y_band = y_page - (page_height - margin.bottom)
```

页面高 842 pt、底边距 80 pt、页面 `y` 为 770 pt 时，band `y` 是 `770 - (842 - 80) = 8` pt。`BandInset` 不平移标记。库先按 `marks` 的顺序画标记，再画左右槽。

以前：

```rust
front: vec![LayerMark::Rect {
    when: PageWhen::All,
    rect: Rect::try_pt(24.0, 770.0, 180.0, 65.0)?,
    ink: Ink::Stroke(pen),
}]
```

现在：

```rust
footer: Some(BandFn::fixed(BandSpec {
    marks: vec![BandMark::Shape(LayerShape::Rect {
        rect: Rect::try_pt(24.0, 8.0, 180.0, 65.0)?,
        ink: Ink::Stroke(pen),
    })],
    ..BandSpec::default()
}))
```

## 3. 把文字基线换成行盒左上角

`LayerShape::Text.at` 是行盒左上角，不是基线。已有坐标如果记的是基线，先把页面基线 `y` 换成 band 基线 `y`，再减去该字号的行盒 ascent：

```text
at.y = baseline_y_band - ascent
```

ascent 是该字号在 band 文字里的行盒顶到基线的距离。不要用段落默认行高 1.2 代替它。band 文字是单行，不断行，也不做对齐。

## 4. 把字号和颜色写进每个文字标记

band 文字从文档种子开始。种子是 12 pt、黑色、非粗体、非斜体。它不继承 `Block::Styled`。9 pt、8 pt、灰色或粗体写在该标记自己的 `RunStyle` 上。

## 5. 对照三条越界错误

包围盒含描边半宽。边缘相等时通过。库不裁切标记，也不增加 margin。

- `GeometryKind::BandMarkOutside { x, y }` 表示包围盒从 band 上方或左侧开始。检查 `y` 换算，以及描边向外扩出的半个线宽。
- `ErrorKind::DecorationTallerThanMargin { height, margin }` 表示包围盒底边超过该侧 margin。减小 `y` 或高度，或由调用方增大该节的 margin。`height` 是包围盒底边。
- `ErrorKind::DecorationWiderThanFrame { width, avail }` 表示包围盒右边超过页宽。检查 `x`、宽度和描边。`avail` 是页宽。
