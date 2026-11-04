# 从 0.5 迁到 0.6

这份说明是 `pdfcraft-pdf` `0.5` 到 `0.6` 的字段对照。公共面按提案书 §12.7 重排。没有旧名别名。

更早的三步仍单独成文。它们记录当时的迁移，文中「现在」已经改成当前形状。

- [行内样式](migrate-inline.md)
- [表格行与边框](migrate-table-row.md)
- [页脚标记进 band](migrate-band-marks.md)

`pdfcraft-forms`、`pdfcraft-orgchart`、`pdfcraft-post` 的类型不变。

下面每一节先给 0.5 的写法，再给当前写法。改完后看文末三条行为变化。那三条能编译通过，PDF 会不一样。

## 1. 段落与行内

`InlineRun` 改名为 `Span`。`Link.runs` 改名为 `children`。`split_latin` 挂在 `Span` 上。

段落的颜色、字号、对齐、首行缩进、行高因子收进 `ParagraphStyle`。行高因子的类型是 `LineHeight`。

以前：

```rust
ParagraphSpec {
    runs: vec![InlineRun::Text {
        text: "正文".into(),
        font: DocFont::Body,
        style: RunStyle::default(),
    }],
    align: Alignment::Start,
    color: None,
    font_size: None,
    first_line_indent: NonNegative::ZERO,
    line_height_factor: Some(1.4),
}
```

现在：

```rust
ParagraphSpec {
    style: ParagraphStyle {
        line_height: Some(LineHeight::new(1.4)?),
        ..ParagraphStyle::default()
    },
    children: vec![Span::Text {
        text: "正文".into(),
        font: DocFont::Body,
        style: RunStyle::default(),
    }],
}
```

`LineHeight::new` 拒绝非有限值、0 和负数，错误仍是 `GeometryKind::LineHeightFactor`。段落层的 `style.text` 可以设置 `RunStyle` 的全部字段。叠放顺序是文档种子、外层 `Block::Styled`、段落 `style.text`、span 自己的 `style`。

## 2. 单元格

删除 `CellBlock`。单元格孩子是 `Vec<Block<K>>`。短文本用 `Block::paragraph`。`content` 改名为 `children`。`fill` 收进 `CellStyle`。

以前：

```rust
CellSpec {
    content: vec![CellBlock::Text {
        text: "项目".into(),
        font: (),
    }],
    fill: None,
    ..CellSpec::default()
}
```

现在：

```rust
CellSpec {
    children: vec![Block::paragraph((), "项目")],
    ..CellSpec::default()
}
```

单元格里可以放 `Block::Styled`。栏、嵌套表、`KeepTogether`、`PageBreak` 在 `layout_spec` 里失败，错误是 `InvalidCellContent`，`container` 为 `"table cell"`。

## 3. 分栏、列表、表格、包裹

分栏的间距在 `ColumnsStyle.gap`，默认 0。列宽在 `ColumnStyle.width`。孩子字段都叫 `children`。

以前：

```rust
ColumnsSpec {
    gap: NonNegative::pt(12.0).unwrap(),
    columns: vec![ColumnSpec {
        width: Track::Auto,
        blocks: vec![Block::paragraph((), "左栏")],
    }],
}
```

现在：

```rust
ColumnsSpec {
    style: ColumnsStyle {
        gap: NonNegative::pt(12.0).unwrap(),
    },
    children: vec![ColumnSpec {
        style: ColumnStyle { width: Track::Auto },
        children: vec![Block::paragraph((), "左栏")],
    }],
}
```

列表的版式在 `ListStyle`。有序列表的 `numbering`、`start`、`format` 只能写在 `ListKind::Ordered` 上。项的 `blocks` 改名为 `children`。

以前：

```rust
ListSpec {
    kind: ListKind::Ordered(Numbering::Decimal),
    start: NonZeroU32::new(1).unwrap(),
    marker_format: MarkerFormat::DotSuffix,
    marker_width: Positive::pt(18.0).unwrap(),
    items: vec![ListItemSpec {
        marker: None,
        blocks: vec![Block::paragraph((), "条款")],
    }],
    ..ListSpec::default()
}
```

现在：

```rust
ListSpec {
    style: ListStyle {
        kind: ListKind::Ordered {
            numbering: Numbering::Decimal,
            start: NonZeroU32::new(1).unwrap(),
            format: MarkerFormat::DotSuffix,
        },
        marker_width: Positive::pt(18.0).unwrap(),
        ..ListStyle::default()
    },
    children: vec![ListItemSpec {
        marker: None,
        children: vec![Block::paragraph((), "条款")],
    }],
}
```

表头是 `Option<TableHeader>`。`repeat_header` 改名为 `TableHeader.repeat`，默认 `true`。没有表头就写 `None`。`Some` 且 `rows` 为空时，与 `None` 相同。行的 `cells` 改名为 `children`。列宽、内边距、边框在 `TableStyle`。

以前：

```rust
TableSpec {
    header: vec![TableRow {
        cells: vec![CellSpec {
            content: vec![CellBlock::Text { text: "名称".into(), font: () }],
            ..CellSpec::default()
        }],
    }],
    body,
    repeat_header: true,
    border: TableBorder::default(),
    ..TableSpec::default()
}
```

现在：

```rust
TableSpec {
    style: TableStyle {
        border: TableBorder::default(),
        ..TableStyle::default()
    },
    header: Some(TableHeader {
        rows: vec![TableRow {
            children: vec![CellSpec {
                children: vec![Block::paragraph((), "名称")],
                ..CellSpec::default()
            }],
        }],
        ..TableHeader::default()
    }),
    body,
}
```

包裹块：

| 0.5 | 0.6 |
| --- | --- |
| `KeepTogether(vec![...])` | `KeepTogether { children }` |
| `Inset { left, right, children }` | `Inset { style: InsetStyle { left, right }, children }` |
| `Anchor { id, outline, child }` | `Anchor { id, outline, children }` |

单元格里的缩进也是 `Block::Inset`，不再有 `CellBlock::Inset`。

## 4. 图片、分隔线、流内 canvas

`ImageSpec` 的宽、高、对齐收进 `ImageStyle`。`CanvasSpec` 的对齐收进 `CanvasStyle`。`CanvasMark` 不变。流内 canvas 仍是 `Block::Canvas`。

`HLineSpec` 删除。`Block::HLine` 的参数是 `HLineStyle`。`space_before` 和 `space_after` 删除。线前后的空白改放 `Block::Spacer`。

以前：

```rust
Block::HLine(HLineSpec {
    space_before: NonNegative::pt(6.0).unwrap(),
    space_after: NonNegative::pt(6.0).unwrap(),
    ..HLineSpec::default()
})
```

现在：

```rust
Block::Spacer(NonNegative::pt(6.0).unwrap()),
Block::HLine(HLineStyle::default()),
Block::Spacer(NonNegative::pt(6.0).unwrap()),
```

需要三者留在同一页时，用 `Block::KeepTogether { children }` 包住。

## 5. 节、页眉页脚、图层

`SectionSpec.page_size` 与 `margins` 收进 `SectionStyle`。`BandSpec.inset` 收进 `BandStyle`。

`TextRun` 改名为 `PageText`。`BandMark` 改名为 `Mark`。`BandSlot` 从两个变体收成一个结构体。`link: None` 是原来的 `Text`。`link: Some(uri)` 是原来的 `Link`，可见文字仍在 `children`。

以前：

```rust
SectionSpec {
    margins: Margins::try_uniform(48.0)?,
    header: Some(BandFn::fixed(BandSpec {
        inset: BandInset::PageMargins,
        left: Some(BandSlot::Text {
            runs: vec![TextRun::PageNumber],
            font: (),
            style: RunStyle::default(),
        }),
        ..BandSpec::default()
    })),
    ..SectionSpec::default()
}
```

现在：

```rust
SectionSpec {
    style: SectionStyle {
        margins: Margins::try_uniform(48.0)?,
        ..SectionStyle::default()
    },
    header: Some(BandFn::fixed(BandSpec {
        style: BandStyle {
            inset: BandInset::PageMargins,
        },
        left: Some(BandSlot {
            children: vec![PageText::PageNumber],
            font: (),
            style: RunStyle::default(),
            link: None,
        }),
        ..BandSpec::default()
    })),
    ..SectionSpec::default()
}
```

带内标记写成 `Mark::Shape` 或 `Mark::Rotated`。旋转角度是 `Angle`，用 `Angle::degrees` 构造。非有限值在构造时失败，错误仍是 `GeometryKind::NonFiniteRotation`。

页面标记是 `LayerMark { when, child }`。`child` 与带内标记共用 `Mark`。原来的 `LayerMark::Text`、`Image`、`Line`、`Rect`、`Ellipse` 写成 `Mark::Shape(LayerShape::...)`。原来的 `LayerMark::Rotated { when, degrees, pivot, shapes }` 写成 `when` 加 `Mark::Rotated { degrees, pivot, shapes }`。

## 6. 会改变 PDF 的三条

字段搬迁本身不改坐标。下面三条会改。

单元格里的段落继承外层 `Block::Styled` 的全部字段，包括粗体、斜体、下划线、删除线和字距。以前的 `CellBlock::Text` 只继承字号和颜色。列表编号和圆点仍只继承字号和颜色。

分隔线的高度等于 `thickness`，笔画在框顶向下 `thickness / 2`。以前还加上 `space_before` 和 `space_after`。同页时，前后各放一个等高的 `Spacer` 可以回到旧坐标。跨页时 spacer 会拆开并把余量带到下一页，线本身仍整段搬家。每个 spacer 各计一个 `LimitKind::Blocks`。

行高因子在 `LineHeight::new` 校验。旋转角度在 `Angle::degrees` 校验。错误 kind 不变，失败比以前更早。
