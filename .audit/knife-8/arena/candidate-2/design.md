# candidate-2：公开 `Block` 树，一次 `lower`

刀 8 公共 DSL。基线 `feat/document-dsl` @ `6dbf40c`。不写 `src/`。权威是 `docs/提案书.md`；设计稿里未经提案书确认的条目不进入本形状。

`document(PageSize, |doc| …)` 只跑组装闭包，返回 `Doc`。闭包里的 `paragraph` / `table` / `columns` / `ul` / `ol` / `keep_together` / `spacer` / `page_break` 当场把数据推进 `Block` 树，闭包结束就丢掉。`header` / `footer` 把闭包留下来，分页之后每一页调用一次。`render` 和 `to_pdf` 各自调用 `lower`，再交给现有的 `paginate_root`。组装阶段不测量、不断行、不画页。

`lower(&Doc) -> (FlowNode, Decorations, FontStore, PaginateConfig)` 是正文进入刀 7 的唯一出口。它只调用已经落地的构造器（`TextNode::new`、`ColumnsNode::new`、`TableNode::new`、`ListNode::new`、`Cell::new`、`ListItem::new` / `custom`）。不改 `split`，不改 `paint_pages`，不改 `check_content`，不改 `Decorations` 的类型。

---

## Usage

测试放在 `src/document/tests.rs`（crate 内，能调用 `lower` 和 `line_metrics`）。坐标用 `to_bits()`。`to_pdf` 只断言字节以 `%PDF` 开头。夹具字体是 `fonts::fixture_bytes`（ASCII、数字、U+2022、`第` / `页` / `共`）。

`probe_style` 与 `lower` 的缺省样式相同：第一张脸、12 pt、行高系数 1.2、黑色、段后距 0、`TextAlign::Start`。`texts` 取一页里 `DrawGlyphRun` 的源字符串。`glyph` 按整段文字找那一条 run。

下面例子里的 `Ok(())` 只出现在 `document` 这层：`register_font` 和长度构造会失败。正文闭包（`paragraph`、`table`、`columns`、列表、`keep_together`）不返回 `Result`。

### 1. 组装闭包写出 AST，这一步不排版

`if` / `for` 在闭包里决定哪些 `Block` 被推进去。没走到的分支不会留在树上。`document` 返回之后可以读 `blocks()`，此时还没有 `RecordingCanvas`，也没有页码。

```rust
#[derive(Clone, Copy, PartialEq)]
enum Kind { A, B }

struct Item {
    name: &'static str,
    amount: &'static str,
}

struct Contract {
    id: &'static str,
    kind: Kind,
    items: &'static [Item],
    party_a: &'static str,
    party_b: &'static str,
}

fn assemble(contract: &Contract) -> Result<Doc> {
    document(PageSize::A4, |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.paragraph(|p| p.text(format!("合同 {}", contract.id)));
        if contract.kind == Kind::A {
            doc.paragraph(|p| p.text("按甲方案执行。"));
            doc.table(|t| {
                t.header(["项目", "金额"]);
                for item in contract.items {
                    t.row([&item.name, &item.amount]);
                }
            });
        } else {
            doc.paragraph(|p| p.text("按乙方案执行。"));
            doc.paragraph(|p| p.text("乙方案没有明细表。"));
        }
        doc.columns(|row| {
            row.col(|c| c.paragraph(|p| p.text(contract.party_a)));
            row.col(|c| c.paragraph(|p| p.text(contract.party_b)));
        });
        Ok(())
    })
}

#[test]
fn assemble_keeps_only_the_branch_that_ran() {
    let items = [Item { name: "纸", amount: "1" }, Item { name: "墨", amount: "2" }];
    let base = Contract {
        id: "C-18",
        kind: Kind::A,
        items: &items,
        party_a: "甲",
        party_b: "乙",
    };
    let a = assemble(&base).unwrap();
    match a.blocks() {
        [
            Block::Paragraph(title),
            Block::Paragraph(plan),
            Block::Table(table),
            Block::Columns(columns),
        ] => {
            assert_eq!(title.text, "合同 C-18");
            assert_eq!(plan.text, "按甲方案执行。");
            assert_eq!(table.header.len(), 1);
            assert_eq!(table.body.len(), 2);
            assert!(table.repeat_header);
            assert_eq!(columns.columns.len(), 2);
            assert_eq!(columns.gap, NonNegative::pt(12.0).unwrap());
        }
        other => panic!("kind A shape: {other:?}"),
    }

    let mut b = base;
    b.kind = Kind::B;
    let b = assemble(&b).unwrap();
    assert!(matches!(
        b.blocks(),
        [
            Block::Paragraph(_),
            Block::Paragraph(p1),
            Block::Paragraph(p2),
            Block::Columns(_),
        ] if p1.text == "按乙方案执行。" && p2.text == "乙方案没有明细表。"
    ));
}
```

`PageSize::A4.resolve()` 等于 `geom::A4`。横向是宽高对调：`PageSize::Paper(Paper::A4, Orientation::Landscape).resolve()` 等于 `Size::new(A4.height(), A4.width())`。`PageSize::Custom(size)` 原样返回 `size`。

缺字在这一步不失败。`document` 成功返回 `Doc`；`render` 才返回 `MissingGlyph`，画布上没有任何 `StartPage`。

```rust
#[test]
fn missing_glyph_fails_at_render_not_at_assemble() {
    let doc = document(PageSize::A4, |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.paragraph(|p| p.text("\u{2603}"));
        Ok(())
    })
    .unwrap();
    assert!(matches!(doc.blocks(), [Block::Paragraph(_)]));
    let mut rec = RecordingCanvas::new();
    let err = doc.render(&mut rec).unwrap_err();
    assert!(matches!(err, Error::MissingGlyph { scalar: '\u{2603}', .. }));
    assert!(rec.commands().is_empty());
}
```

夹具的 cmap 若含 U+2603，换一个 cmap 里没有的标量，断言仍是 `MissingGlyph` 且命令日志为空。

### 2. 页眉在每一页绘制时跑；`is_last` 也在这一刻

正文里的 `if` 已经在组装时取舍完。页眉闭包里的 `if page.is_last()` 要等总页数出来才跑。单节文档上，`section_number == number`，`section_count == count`。

页眉段落按调用顺序竖排，各占一行。`Align::End` 只影响该段在**整页宽**里的水平位置。两条页眉不并排。带高是 `margin.top`；文字从 `y = 0` 起排。正文 `x` 是 `margin.left`。

```rust
#[test]
fn header_stacks_full_width_lines_and_footer_end_is_last_page_only() {
    let (mut probe, style) = probe_style();
    let face = probe.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let top = NonNegative::pt(metrics.line_height * 2.0 + 4.0).unwrap();
    let side = NonNegative::pt(20.0).unwrap();
    let bottom = NonNegative::pt(metrics.line_height + 4.0).unwrap();

    let doc = document(PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()), |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.set_margins(Margins::new(top, side, bottom, side));
        let name = "ACME".to_owned();
        doc.header(move |page, band| {
            band.paragraph(|p| p.text(&name));
            band.paragraph(|p| {
                p.align(Align::End);
                p.text(format!(
                    "第 {} 页 / 共 {} 页",
                    page.section_number, page.section_count
                ));
            });
        });
        doc.footer(|page, band| {
            if page.is_last() {
                band.paragraph(|p| p.text("END"));
            }
        });
        doc.paragraph(|p| p.text("AB"));
        doc.page_break();
        doc.paragraph(|p| p.text("CD"));
        Ok(())
    })
    .unwrap();

    let mut rec = RecordingCanvas::new();
    doc.render(&mut rec).unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(texts(&pages[0]), ["ACME", "第 1 页 / 共 2 页", "AB"]);
    assert_eq!(texts(&pages[1]), ["ACME", "第 2 页 / 共 2 页", "CD", "END"]);

    let b = metrics.baseline;
    let line = metrics.line_height;
    let acme = glyph(&pages[0], "ACME");
    let label = glyph(&pages[0], "第 1 页 / 共 2 页");
    let body = glyph(&pages[0], "AB");
    assert_eq!(acme.origin.x().as_pt().to_bits(), 0.0f32.to_bits());
    assert_eq!(acme.origin.y().as_pt().to_bits(), b.to_bits());
    let label_w: f32 = label
        .glyphs
        .iter()
        .map(|g| g.advance_norm * label.font_size.as_pt())
        .sum();
    assert_eq!(label.origin.x().as_pt().to_bits(), (200.0 - label_w).to_bits());
    assert_eq!(label.origin.y().as_pt().to_bits(), (line + b).to_bits());
    assert_eq!(body.origin.x().as_pt().to_bits(), 20.0f32.to_bits());
    assert_eq!(body.origin.y().as_pt().to_bits(), (top.as_pt() + b).to_bits());

    let end = glyph(&pages[1], "END");
    assert_eq!(end.origin.x().as_pt().to_bits(), 0.0f32.to_bits());
    assert_eq!(
        end.origin.y().as_pt().to_bits(),
        (400.0 - bottom.as_pt() + b).to_bits()
    );
}
```

连续两个 `page_break` 仍走刀 7 的游标：中间那一页正文是空的，页眉照画，页码把空白页算进去。

```rust
#[test]
fn blank_body_page_is_numbered_in_the_header() {
    let doc = document(PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()), |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.set_margins(Margins::uniform(NonNegative::pt(36.0).unwrap()));
        doc.header(|page, band| {
            band.paragraph(|p| {
                p.text(format!(
                    "{} {} {} {}",
                    page.number, page.count, page.section_number, page.section_count
                ));
            });
        });
        doc.paragraph(|p| p.text("A"));
        doc.page_break();
        doc.page_break();
        doc.paragraph(|p| p.text("B"));
        Ok(())
    })
    .unwrap();
    let mut rec = RecordingCanvas::new();
    doc.render(&mut rec).unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 3);
    assert_eq!(texts(&pages[0]), ["1 3 1 3", "A"]);
    assert_eq!(texts(&pages[1]), ["2 3 2 3"]);
    assert_eq!(texts(&pages[2]), ["3 3 3 3", "B"]);
}
```

`margin.top == 0` 又写了非空页眉时，`render` 返回 `DecorationTallerThanMargin`，命令日志为空。`document` 本身成功。

第二次 `doc.header(...)` 替换上一次，不叠两条钩子。`footer` 同样。

### 3. 两栏等分，列 x 由刀 7 决定

未指定列宽时，每一列是 `Track::Ratio(1.0)`，`gap` 写入 AST 为 12 pt。`lower` 把它交给 `ColumnsNode::new`，列宽在 `split` 时才解。

```rust
#[test]
fn equal_columns_keep_x_across_the_gap() {
    let (mut probe, style) = probe_style();
    let face = probe.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let left = 20.0f32;
    let doc = document(PageSize::custom(Size::try_pt(200.0, 400.0).unwrap()), |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.set_margins(Margins::uniform(NonNegative::pt(left).unwrap()));
        doc.columns(|row| {
            row.col(|c| c.paragraph(|p| p.text("AB")));
            row.col(|c| c.paragraph(|p| p.text("CD")));
        });
        Ok(())
    })
    .unwrap();

    let columns = match doc.blocks() {
        [Block::Columns(columns)] => columns,
        other => panic!("columns: {other:?}"),
    };
    assert_eq!(columns.gap, NonNegative::pt(12.0).unwrap());
    assert!(matches!(
        columns.columns.as_slice(),
        [Column { width: Track::Ratio(a), .. }, Column { width: Track::Ratio(b), .. }]
            if *a == 1.0 && *b == 1.0
    ));

    let mut rec = RecordingCanvas::new();
    doc.render(&mut rec).unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 1);
    let body_w = 200.0 - left - left;
    let col_w = (body_w - 12.0) / 2.0;
    let y = left + metrics.baseline;
    assert_eq!(glyph(&pages[0], "AB").origin.x().as_pt().to_bits(), left.to_bits());
    assert_eq!(
        glyph(&pages[0], "CD").origin.x().as_pt().to_bits(),
        (left + col_w + 12.0).to_bits()
    );
    assert_eq!(glyph(&pages[0], "AB").origin.y().as_pt().to_bits(), y.to_bits());
    assert_eq!(glyph(&pages[0], "CD").origin.y().as_pt().to_bits(), y.to_bits());
}
```

`row.col_fixed(width, …)`、`row.col_ratio(weight, …)`、`row.col_auto(…)` 把对应的 `Track` 写进那一列。两列 `Auto`，或 `Ratio` 不是有限正数，留到 `render`：`Error::ColumnWidths`。只有一列时同样，`reason` 为 `"column-count"`。`document` 仍返回 `Ok`。

### 4. 表：字符串单元格，续页重复全部表头

`t.header` / `t.row` 的 `&str` 各变成一个 `Cell`：一块段落，`col_span` / `row_span` 都是 1。多次 `header` 是多行表头。`repeat_header` 默认 `true`。未调用 `widths` 时，`lower` 按各行 `col_span` 之和的最大值生成同样数量的 `Track::Ratio(1.0)`。`padding` 与边框宽默认 0。

页高沿用刀 7 表测试：`padding = 0`、边框宽 0 时，行高等于 `line_height`。正文框高 `2 * line_height + 1` 正好放下两行。默认 `repeat_header` 时，续页要再放表头，所以这一页高只再放得下一行正文。下面用两行正文，得到两页，第二页仍以表头开头。

```rust
#[test]
fn table_repeat_header_reaches_the_second_page() {
    let (mut probe, style) = probe_style();
    let face = probe.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let page = PageSize::custom(Size::try_pt(200.0, metrics.line_height * 2.0 + 1.0).unwrap());
    let doc = document(page, |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.table(|t| {
            t.header(["h", "k"]);
            t.row(["r0", "1"]);
            t.row(["r1", "1"]);
        });
        Ok(())
    })
    .unwrap();
    let mut rec = RecordingCanvas::new();
    doc.render(&mut rec).unwrap();
    let pages = rec.pages();
    assert_eq!(pages.len(), 2);
    assert_eq!(texts(&pages[0]), ["h", "k", "r0", "1"]);
    assert_eq!(texts(&pages[1]), ["h", "k", "r1", "1"]);
}
```

同一页高、三行正文、`t.repeat_header(false)` 时，续页不再画表头，文字是 `["r1", "1", "r2", "1"]`。这与刀 7 `repeat_header_off_continues_body_only` 相同，只是行从 DSL 进来。

合并格用 `NonZeroU16`，0 写不出来：

```rust
let cols = NonZeroU16::new(2).unwrap();
doc.table(|t| {
    t.widths([Track::Ratio(1.0), Track::Ratio(1.0)]);
    t.header([Cell::text("H").col_span(cols)]);
    t.row(["A", "B"]);
});
```

`col_span` 越过列数、留下空槽、或表头 `row_span` 伸进正文，`render` 返回 `InvalidCellSpan`。单元格里放 `Block::Columns`（手写 AST，`Cell::blocks`）时，`render` 返回 `InvalidCellContent { container: "table cell", node: "columns" }`。这些都是 `TableNode::new` / `Cell::new` 已有的结果，DSL 不再判一遍。

### 5. 列表：标记在首页，正文缩进 `marker_width`

默认 `marker_width` 为 18 pt，写进 AST。`ul` 的自动标记是 `"•"`。`ol(NumberStyle::Decimal)` 从 1 起，标记字符串由 `ListNode` 生成（`"1."`）。自定义标记原样进入 `ListItem::custom`，不加句点。

```rust
#[test]
fn bullet_hangs_and_ordered_marker_is_generated_later() {
    let width = Positive::pt(24.0).unwrap();
    let doc = document(PageSize::A4, |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.ul(|list| {
            list.marker_width(width);
            list.item(|item| item.paragraph(|p| p.text("AB")));
        });
        doc.ol(NumberStyle::Decimal, |list| {
            list.item(|item| item.paragraph(|p| p.text("CD")));
            list.item_marker("X", |item| item.paragraph(|p| p.text("EF")));
        });
        Ok(())
    })
    .unwrap();

    match doc.blocks() {
        [Block::List(ul), Block::List(ol)] => {
            assert_eq!(ul.marker_width, width);
            assert!(ul.nest_indent.is_none());
            assert!(matches!(ul.kind, ListKind::Bullet));
            assert!(matches!(
                ol.kind,
                ListKind::Ordered { style: NumberStyle::Decimal, start } if start.get() == 1
            ));
            assert_eq!(ol.items[1].marker.as_deref(), Some("X"));
        }
        other => panic!("lists: {other:?}"),
    }

    let mut rec = RecordingCanvas::new();
    doc.render(&mut rec).unwrap();
    let page = &rec.pages()[0];
    assert_eq!(texts(page)[0], "\u{2022}");
    assert_eq!(glyph(page, "\u{2022}").origin.x().as_pt().to_bits(), 0.0f32.to_bits());
    assert_eq!(glyph(page, "AB").origin.x().as_pt().to_bits(), 24.0f32.to_bits());
    assert_eq!(glyph(page, "AB").origin.y().as_pt().to_bits(), glyph(page, "\u{2022}").origin.y().as_pt().to_bits());
    assert!(texts(page).iter().any(|s| s == "1."));
    assert!(texts(page).iter().any(|s| s == "X"));
}
```

`item` 上只有 `paragraph`、`spacer`、`ul`、`ol`。嵌套列表再缩进一层，缩进量是内层自己的 `marker_width`（`nest_indent: None` 时刀 7 就是这么做的）。

### 6. `keep_together`、`spacer`、字体

两行包进 `keep_together`，页高只够一行（`line_height + 0.5`）时，`render` 返回 `UnsplittableTallerThanFrame`，命令日志为空。

```rust
#[test]
fn keep_together_taller_than_the_page_fails_before_paint() {
    let (mut probe, style) = probe_style();
    let face = probe.metrics(style.font).unwrap();
    let metrics = line_metrics(face, style.font_size, style.line_height_factor).unwrap();
    let doc = document(
        PageSize::custom(Size::try_pt(200.0, metrics.line_height + 0.5).unwrap()),
        |doc| {
            doc.register_font(fixture_bytes().as_ref().to_vec())?;
            doc.keep_together(|group| {
                group.paragraph(|p| p.text("A"));
                group.paragraph(|p| p.text("B"));
            });
            Ok(())
        },
    )
    .unwrap();
    let mut rec = RecordingCanvas::new();
    let err = doc.render(&mut rec).unwrap_err();
    assert!(matches!(err, Error::UnsplittableTallerThanFrame));
    assert!(rec.commands().is_empty());
}
```

`spacer` 的高度是 `Length<NonNegative>`，可以为 0。高于本页剩余空间时由现有 `SpacerNode::split` 拆开。先一段 `"A"`，再一个高度为 `3 * line_height` 的 spacer，页高 `2 * line_height`：两页，字形只有第一页的 `"A"`。

`register_font` 在闭包里解析字节。空字节或垃圾字节让 `document` 返回 `Error::Font`，不会留下半份 `Doc`。成功时 `FontId` 按注册顺序从 0 编号。段落不调用 `.font` 时用第一张脸；`.font(id)` 写在该段上。第二张脸的 run 在 `RecordingCanvas` 里 `font.as_u32() == 1`。

没注册任何字体就 `paragraph`：`document` 成功，`render` 返回 `Error::Font { index: 0, message: "no font registered" }`，命令日志为空。

`lower` 之后正文第一段是 `FlowNode::Text`，不是已经断过行的 `Paragraph`。断行仍发生在 `TextNode::split` 看到 `avail.width` 的时候。

```rust
#[test]
fn lower_emits_lazy_text_not_a_wrapped_paragraph() {
    let doc = document(PageSize::A4, |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.paragraph(|p| p.text("AB"));
        Ok(())
    })
    .unwrap();
    let (root, _, _, _) = lower(&doc).unwrap();
    match root {
        FlowNode::Stack(kids) => assert!(matches!(kids[0], FlowNode::Text(_))),
        other => panic!("root: {other:?}"),
    }
}
```

同一份 `Doc` 可以对两块新的 `RecordingCanvas` 各 `render` 一次，两次的 `texts` 相同。`render` 不修改 `blocks()`。

### 7. `to_pdf` 与 `render` 分开

`render` 画进调用方给的 `Canvas`。`to_pdf` 在 `backend-krilla` 下自己做一次 `lower`，把字体 `Arc` 交给 `KrillaCanvas::new_with_fonts`，再 `paginate_root`，最后 `finish`。它不读上一次 `render` 的命令日志。

```rust
#[cfg(feature = "backend-krilla")]
#[test]
fn to_pdf_starts_with_pdf_header() {
    let doc = document(PageSize::A4, |doc| {
        doc.register_font(fixture_bytes().as_ref().to_vec())?;
        doc.paragraph(|p| p.text("AB"));
        Ok(())
    })
    .unwrap();
    let pdf = doc.to_pdf().unwrap();
    assert!(pdf.starts_with(b"%PDF"));
}
```

没开 `backend-krilla` 时没有 `to_pdf`。`render` 两种 feature 都在。

### §1 调用形状

提案书示例里返回 `Vec<u8>` 的那一行，在这里是先拿到 `Doc` 再 `to_pdf`。页眉闭包直接收到 `Page`，页码在闭包体内格式化。`p.bold`、`behind`、`front` / `image_at` 不在本候选里（见文末缺口）。

```rust
let name = company.name.clone();
let doc = document(PageSize::A4, |doc| {
    doc.register_font(std::fs::read(font_path)?)?;
    doc.set_margins(Margins::uniform(NonNegative::mm(20.0)?));
    doc.header(move |page, band| {
        band.paragraph(|p| p.text(&name));
        band.paragraph(|p| {
            p.align(Align::End);
            p.text(format!("第 {} 页 / 共 {} 页", page.number, page.count));
        });
    });
    doc.paragraph(|p| p.text(format!("合同 {}", contract.id)));
    if contract.kind == Kind::A {
        doc.paragraph(|p| p.text("按甲方案执行。"));
        doc.table(|t| {
            t.header(["项目", "金额"]);
            for item in &contract.items {
                t.row([&item.name, &item.amount]);
            }
        });
    } else {
        doc.paragraph(|p| p.text("按乙方案执行。"));
        doc.paragraph(|p| p.text("乙方案没有明细表。"));
    }
    doc.columns(|row| {
        row.col(|c| c.paragraph(|p| p.text(&contract.party_a)));
        row.col(|c| c.paragraph(|p| p.text(&contract.party_b)));
    });
    Ok(())
})?;
let pdf = doc.to_pdf()?;
```

读文件的 `std::io::Error` 与 `quire_pdf::Error` 不是同一种。字体字节在闭包外读好，以 `Vec<u8>` 传给 `register_font`。上面的 `?` 只说明哪一层返回 `Result`。

---

## Types

公开类型由 `lib.rs` 重导出。`src/document` 模块改成私有：根上同时放 `pub mod document` 和函数 `document` 会重名。构建器类型必须重导出，否则公开方法的签名会比方法本身更私有。

`lower`、`FlowNode`、`Decorations`、`FontStore`、`PaginateConfig`、`PageInfo` 保持 `pub(crate)`。

### 入口

```rust
pub fn document(page: PageSize, build: impl FnOnce(&mut Doc) -> Result<()>) -> Result<Doc>;

pub struct Doc { /* 私有字段 */ }

impl Doc {
    pub fn new(page: PageSize) -> Self;
    pub fn register_font(&mut self, bytes: impl Into<Vec<u8>>) -> Result<FontId>;
    pub fn register_face(&mut self, bytes: impl Into<Vec<u8>>, index: u32) -> Result<FontId>;
    pub fn set_margins(&mut self, margins: Margins);
    pub fn margins(&self) -> Margins;
    pub fn page_size(&self) -> Size;
    pub fn blocks(&self) -> &[Block];

    pub fn paragraph(&mut self, f: impl FnOnce(&mut ParagraphBuilder));
    pub fn table(&mut self, f: impl FnOnce(&mut TableBuilder));
    pub fn columns(&mut self, f: impl FnOnce(&mut ColumnsBuilder));
    pub fn keep_together(&mut self, f: impl FnOnce(&mut GroupBuilder));
    pub fn ul(&mut self, f: impl FnOnce(&mut ListBuilder));
    pub fn ol(&mut self, style: NumberStyle, f: impl FnOnce(&mut ListBuilder));
    pub fn spacer(&mut self, height: Length<NonNegative>);
    pub fn page_break(&mut self);

    pub fn header<F>(&mut self, f: F)
    where
        F: for<'a> Fn(&Page, &mut BandBuilder<'a>) + 'static;
    pub fn footer<F>(&mut self, f: F)
    where
        F: for<'a> Fn(&Page, &mut BandBuilder<'a>) + 'static;

    pub fn render(&self, canvas: &mut dyn Canvas) -> Result<()>;

    #[cfg(feature = "backend-krilla")]
    pub fn to_pdf(&self) -> Result<Vec<u8>>;
}

pub(crate) fn lower(doc: &Doc) -> Result<(FlowNode, Decorations, FontStore, PaginateConfig)>;
```

`Doc::new` 与 `document` 做同一件事的前半：解析纸张、边距置 0、正文为空、没有页眉页脚。`document` 接着调用闭包。闭包返回 `Err` 时整次 `document` 失败，调用方拿不到 `Doc`。`register_font` 失败则那次调用不追加字体，已推进的 `Block` 仍在这份尚未返回的 `Doc` 里，但闭包的 `?` 会让 `document` 把它丢掉。

`register_face` 用临时 `FontStore::register` 检查字节，通过后保存 `(index, Arc<Vec<u8>>)`。`FontId::from_raw` 等于当时的下标。`lower` 按同一顺序再注册一遍，得到新的 `FontStore`。两次 `render` 不共用 advance 缓存。

`header` / `footer` 的闭包是 `'static`。调用方把要用的字符串 `clone` 进 `move` 闭包。闭包放在 `Arc<dyn for<'a> Fn(&Page, &mut BandBuilder<'a>) + 'static>` 里，所以 `lower` 可以克隆 `Arc` 装进 `'static` 的 `BandContent`，不必给 `Decorations` 加生命周期。同一闭包每页调用一次，`BandBuilder` 只活在那一次调用里。

### 纸张与页

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paper {
    A3,
    A4,
    A5,
    Letter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    Paper(Paper, Orientation),
    Custom(Size),
}

impl PageSize {
    pub const A4: Self = Self::Paper(Paper::A4, Orientation::Portrait);
    pub const LETTER: Self = Self::Paper(Paper::Letter, Orientation::Portrait);
    pub fn resolve(self) -> Size;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Page {
    pub number: u32,
    pub count: u32,
    pub section_number: u32,
    pub section_count: u32,
    pub size: Size,
    pub margins: Margins,
}

impl Page {
    pub fn is_last(self) -> bool;
}
```

`resolve`：`A4` 竖向返回 `geom::A4`，`Letter` 竖向返回 `geom::LETTER`。`A3` 竖向是 `Size::new(Positive::mm(297.0)?, Positive::mm(420.0)?)`，`A5` 竖向是 148 mm × 210 mm，换算只走 `Positive::mm`。横向用竖向结果的 `Size::new(height, width)`。`Custom` 不再套方向。

`Page::is_last` 为 `number == count`。单节时 `lower` 填进 `BandContent` 的 `Page` 里，`section_number` / `section_count` 分别抄 `PageInfo` 的全文页码和总页数。没有 `section` 构建器，`PaginateConfig` 仍是一整份文档一个 `page_size`。

默认边距是 `Margins::default()`，四边 0。不设预置边距套装。

### `Block`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    Columns(Columns),
    KeepTogether(Vec<Block>),
    PageBreak,
    Spacer(Length<NonNegative>),
    List(List),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub text: String,
    pub font: Option<FontId>,
    pub size: Option<Length<Positive>>,
    pub line_height: Option<f32>,
    pub color: Option<Color>,
    pub align: Align,
    pub spacing_after: Length<NonNegative>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Track {
    Fixed(Length<Positive>),
    Ratio(f32),
    Auto,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Columns {
    pub gap: Length<NonNegative>,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub width: Track,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub widths: Option<Vec<Track>>,
    pub header: Vec<Vec<Cell>>,
    pub body: Vec<Vec<Cell>>,
    pub repeat_header: bool,
    pub padding: Length<NonNegative>,
    pub border_width: Length<NonNegative>,
    pub border_color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub blocks: Vec<Block>,
    pub col_span: NonZeroU16,
    pub row_span: NonZeroU16,
}

impl Cell {
    pub fn text(text: impl Into<String>) -> Self;
    pub fn blocks(blocks: Vec<Block>) -> Self;
    pub fn col_span(self, cols: NonZeroU16) -> Self;
    pub fn row_span(self, rows: NonZeroU16) -> Self;
}

impl From<&str> for Cell {}
impl From<String> for Cell {}
impl From<&String> for Cell {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    Decimal,
    UpperAlpha,
    LowerAlpha,
    UpperRoman,
    LowerRoman,
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub kind: ListKind,
    pub marker_width: Length<Positive>,
    pub nest_indent: Option<Length<Positive>>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListKind {
    Bullet,
    Ordered { style: NumberStyle, start: NonZeroU32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub marker: Option<String>,
    pub blocks: Vec<Block>,
}
```

字段公开。构建器写入的值和手写枚举是同一棵树，`lower` 不区分来源。`From<&str> for Cell` 等于 `Cell::text`。

构建器在闭包结束时盖上的默认值：

| 字段 | 写入 AST 的值 |
| --- | --- |
| `Paragraph::align` | `Align::Start` |
| `Paragraph::spacing_after` | 0 |
| `Paragraph::{font, size, line_height, color}` | `None`（`lower` 再填） |
| `Columns::gap` | 12 pt，除非 `row.gap(...)` |
| `Column::width`（`row.col`） | `Track::Ratio(1.0)` |
| `Table::repeat_header` | `true` |
| `Table::padding` / `border_width` | 0 |
| `Table::border_color` | `Color::rgb(0, 0, 0)` |
| `Table::widths` | `None`，除非 `t.widths(...)` |
| `List::marker_width` | 18 pt，除非 `list.marker_width(...)` |
| `List::nest_indent` | `None` |
| `ListKind::Ordered::start` | 1，除非 `list.start(...)` |
| `Cell` 的 span | `NonZeroU16::MIN` |

`lower` 填段落样式时：`font` 缺省是第一张已注册的脸；`size` 缺省 `Positive::pt(12.0)`；`line_height` 缺省 `1.2`；`color` 缺省黑；然后 `Style::new`。`line_height` 非有限或 `<= 0` 时 `Style::new` 返回 `InvalidGeometry`，发生在 `render`。标记用这份缺省样式，不用该项正文的字体。没有注册字体时 `lower` 返回 `Error::Font { index: 0, message: "no font registered" }`，不调用 `paginate_root`。

`Track::Ratio` 在 `lower` 里交给 `Ratio::new`。`Track::Fixed` / `Auto` 直接变成 `ColumnWidth`。

### 构建器方法

正文构建器只改 `Vec<Block>`。实现放在一个私有缓冲上，各公开构建器露出下面的子集。

| 方法 | `Doc` | 列 | `keep_together` | 列表项 | 页眉/页脚 |
| --- | --- | --- | --- | --- | --- |
| `paragraph` | 有 | 有 | 有 | 有 | 有 |
| `table` | 有 | 有 | 有 | | |
| `columns` | 有 | 有 | 有 | | |
| `keep_together` | 有 | 有 | 有 | | |
| `ul` / `ol` | 有 | 有 | 有 | 有 | |
| `spacer` | 有 | 有 | 有 | 有 | 有 |
| `page_break` | 有 | | | | |

```rust
impl ParagraphBuilder {
    pub fn text(&mut self, text: impl AsRef<str>);
    pub fn font(&mut self, font: FontId);
    pub fn size(&mut self, size: Length<Positive>);
    pub fn line_height(&mut self, factor: f32);
    pub fn color(&mut self, color: Color);
    pub fn align(&mut self, align: Align);
    pub fn spacing_after(&mut self, spacing: Length<NonNegative>);
}

impl TableBuilder {
    pub fn widths(&mut self, widths: impl IntoIterator<Item = Track>);
    pub fn repeat_header(&mut self, on: bool);
    pub fn padding(&mut self, padding: Length<NonNegative>);
    pub fn border(&mut self, width: Length<NonNegative>, color: Color);
    pub fn header<I, C>(&mut self, cells: I)
    where
        I: IntoIterator<Item = C>,
        C: Into<Cell>;
    pub fn row<I, C>(&mut self, cells: I)
    where
        I: IntoIterator<Item = C>,
        C: Into<Cell>;
}

impl ColumnsBuilder {
    pub fn gap(&mut self, gap: Length<NonNegative>);
    pub fn col(&mut self, f: impl FnOnce(&mut ColumnBuilder));
    pub fn col_fixed(&mut self, width: Length<Positive>, f: impl FnOnce(&mut ColumnBuilder));
    pub fn col_ratio(&mut self, weight: f32, f: impl FnOnce(&mut ColumnBuilder));
    pub fn col_auto(&mut self, f: impl FnOnce(&mut ColumnBuilder));
}

impl ListBuilder {
    pub fn marker_width(&mut self, width: Length<Positive>);
    pub fn nest_indent(&mut self, indent: Length<Positive>);
    pub fn start(&mut self, start: NonZeroU32);
    pub fn item(&mut self, f: impl FnOnce(&mut ItemBuilder));
    pub fn item_marker(&mut self, marker: impl Into<String>, f: impl FnOnce(&mut ItemBuilder));
}
```

`paragraph` 上多次 `text` 按顺序拼进同一个 `String`，中间不加分隔符。`font` / `size` / `align` 后再调用则覆盖前一次。`start` 只对 `ol` 有意义；`ul` 上调用它不改变 `ListKind::Bullet`。

`item_marker` 把字符串放进 `Item::marker`。空字符串留到 `lower`：`ListItem::custom` 返回 `InvalidGeometry`。

列、组和列表项上的 `paragraph` / `table` / `columns` 与 `Doc` 相同，只是块推进该容器自己的 `Vec<Block>`。

### `lower` 对照

`lower_blocks(&[Block], &Defaults) -> Result<Vec<FlowNode>>` 是唯一的 `match`。`lower` 用它处理正文；页眉钩子在每一页里用它处理临时的 `Vec<Block>`。

| `Block` | 产出 |
| --- | --- |
| `Paragraph` | `FlowNode::Text(TextNode::new(text, style))` |
| `Spacer` | `FlowNode::Spacer(SpacerNode { height })` |
| `PageBreak` | `FlowNode::PageBreak` |
| `KeepTogether` | `FlowNode::KeepTogether(KeepTogetherNode::Open(kids))` |
| `Columns` | `ColumnsNode::new(gap, columns)`，每列的块先 `lower_blocks` |
| `Table` | `TableNode::new(TableSpec { … })` |
| `List` | `ListNode::new(ListSpec { … })` |
| 根 | `FlowNode::Stack`，空栈也包一层 |

表单元格：`Cell::new(lower_blocks(&cell.blocks)?)?.with_span(col_span, row_span)`。列表项：`marker` 为 `None` 时 `ListItem::new`，否则 `ListItem::custom`。`NumberStyle` 按同名变体写成 `MarkerKind`。`Align` 写成 `TextAlign`。

`widths: None` 时，列数是各行 `col_span` 之和的最大值；没有行则 `ColumnWidths { reason: "column-count" }`。每一列 `ColumnWidth::Ratio(Ratio::new(1.0)?)`。

页眉钩子：

```rust
Decorations {
    header: Some(Box::new(move |info: &PageInfo| {
        let mut blocks = Vec::new();
        hook(page_from(info), &mut BandBuilder { blocks: &mut blocks });
        lower_blocks(&blocks, &defaults)
    })),
    footer: /* 同一形状 */,
}
```

`page_from` 把 `PageNumber` / `PageCount` 收成 `u32`，section 字段抄全文。钩子返回的节点进入现有 `measure_bands`。`Container::Band` 仍拒绝栏、表、`page_break`、列表、`keep_together`。`BandBuilder` 的方法表里没有这些方法；手写进 `Block` 再塞进页眉的路径不存在，因为页眉不读 `Doc::blocks`。

`PaginateConfig` 使用 `page_size()` 和 `margins()`。

`render`：

```rust
let (root, decorations, mut fonts, config) = lower(self)?;
paginate_root(root, &mut LayoutCtx { fonts: &mut fonts }, config, &decorations, canvas)
```

`to_pdf` 在 `lower` 之后 `fonts.clone()` 进 `KrillaCanvas::new_with_fonts(Arc::new(...))`，排版仍用原来的 `mut fonts`（advance 缓存在这边）。`draw_glyph_run` 只通过 `FontId` 取字节。然后 `canvas.finish()`。

不新增 `Error` 变体。

### 本候选不提供的构建器

刀 7 没有对应节点，或提案书明确不做。本候选不改 `paint_pages` 来安放它们。

| 不提供 | 原因 |
| --- | --- |
| `qr` / `svg` / `toc` / 流内 `canvas` | 提案书 §2.14–§2.17 |
| `image`、`image_at` | 没有 `FlowNode::Image`；`Canvas::draw_image` 仍是 `UnsupportedCanvasOperation` |
| `behind` / `front` / `text_at` | 绘制顺序固定为页眉 → 正文 → 页脚 |
| 分隔线、填空下划线 | 没有对应 `FlowNode`，也没有行内 run |
| `p.bold`、一段里多种字体 | `TextNode` 只有一个 `Style`。另一张脸用另一个段落的 `.font` |
| 链接 | `LinkTarget` 在画布上，段落不携带 URI |
| `section` | `PaginateConfig` 只有一个页尺寸 |
| 某一页关掉页眉页脚 | 提案书搁置 |
| 具名 images、serde、加密、预置边距名 | 不在本刀 |

`Page` 上的 `is_last` 与 section 字段留给页眉页脚闭包。它们不引入逃逸层。

---

## Module map

```text
src/lib.rs                 pub mod document 改为私有 mod document
                           pub use document::{document, Align, Block, Cell, Column, Columns,
                             Doc, Item, List, ListKind, NumberStyle, Orientation, Page,
                             PageSize, Paper, Paragraph, Table, Track,
                             ParagraphBuilder, TableBuilder, ColumnsBuilder, ColumnBuilder,
                             ListBuilder, ItemBuilder, GroupBuilder, BandBuilder}
src/document/mod.rs        document()、Doc、render、to_pdf
src/document/ast.rs        Block 与上表里的数据；只依赖 geom 与 FontId
src/document/lower.rs      lower / lower_blocks；本目录里唯一 use layout 的文件
src/document/tests.rs      上文的 crate 测试
src/layout/*               不改
src/backend/*              不改；to_pdf 只调用 KrillaCanvas::new_with_fonts 与 finish
```

依赖：

```text
document/mod.rs → ast、lower、canvas、（cfg）backend::KrillaCanvas
document/lower.rs → ast、layout::{flow, paragraph, columns, table, list, decorate, paginate, style}
                    fonts::FontStore
document/ast.rs → geom、fonts::FontId
layout 不 use document
ast 不 use layout
```

`document` 是组合根：只有 `to_pdf` 点名 `KrillaCanvas`。这个函数体里不出现 `krilla::` 类型。

公开方法都要有文档注释。`cargo doc` 在 `-D warnings` 下会把缺注释当成失败（`Cargo.toml` 里 `missing_docs = "warn"`）。

---

## Problem

刀 7 的引擎已经能分页：`FlowNode` 的 `split`、`paginate_root`、边距带上的 `Decorations`。`document.rs` 仍是空文件。刀 8 要把提案书 §1 的闭包接到这套引擎上。

正文闭包在组装时运行，那时还没有页数。页眉页脚要在每一页绘制时运行，页码才存在。`TextNode` 在第一次 `split` 时按当时的框宽断行，所以 DSL 不能在组装时调用 `ParagraphNode::layout`。`BandContent` 的类型是 `'static` 的 `Fn(&PageInfo) -> Result<Vec<FlowNode>>`。`check_content(Container::Band)` 拒绝 `Columns`。`paint_pages` 的顺序是页眉、正文、页脚。

调用方还要能在不生成 PDF 的情况下看出 `if` / `for` 留下了哪些块，并在测试里用 `RecordingCanvas` 对坐标，在开了 `backend-krilla` 时拿到以 `%PDF` 开头的字节。

## Usage (caller's view)

上文的测试是契约。组装测试只读 `blocks()`。绘制测试把同一份 `Doc` 交给 `render`。`lower` 的测试只确认正文是 `FlowNode::Text`。`to_pdf` 不接收 `RecordingCanvas`。

页眉示例把公司名和页码分成两段，第二段 `Align::End`。这是整页宽上的两行，不是同一基线上的左右两栏。

## Shape

`Doc` 拥有一棵公开的 `Block` 树、按注册顺序存放的字体字节、已解析的 `Size`、四边边距，以及可选的页眉页脚钩子。构建器是往这棵树上推节点的方法。`document` 不调用 `lower`。

`lower` 每次 `render` / `to_pdf` 跑一遍。正文在进入 `paginate_root` 之前变成 `FlowNode::Stack`。页眉页脚钩子被包成 `Decorations` 里的 `BandContent`：`measure_bands` 每页调用它，它再跑用户闭包，并用同一个 `lower_blocks` 把临时 `Block` 变成 `FlowNode`。页码因此只存在于绘制阶段。

段落样式的缺省值（12 pt、行高系数 1.2、黑色、段后距 0、`Start`、第一张脸）集中在 `lower`。栏距 12 pt、列表标记列 18 pt、表头重复、零 padding、零边框写在 AST 上，`blocks()` 能读到。

纸张是 `PageSize`。`A4` / `Letter` 的竖向结果就是 `geom` 里的常量。方向只和 `Paper` 搭配。

嵌套是否合法仍由 `Cell::new`、`ListItem::new`、`ColumnsNode::new`、`measure_band` 里的 `check_content` 决定。构建器的方法表少暴露几种方法，避免普通调用写出必然失败的树；手写 `Block` 仍然会走到同一处检查。

`to_pdf` 与 `render` 各做一次排版。`Doc` 在两次调用之后树不变。

不变量：组装闭包结束时，未执行的分支不在 `blocks()` 里；页眉字符串里的页码不在 `blocks()` 里。`render` 失败且错误来自分页或量带时，传入的 `RecordingCanvas` 没有 `StartPage`。

## Synthesis decision

（空。由 arena 综合填写。）

## Tradeoffs accepted

- 接受页眉两条字符串分成两行，以换取不修改 `check_content` 和 `paint_pages`。左右并排要在带里放 `Columns`，刀 7 的带不允许。
- 接受 `behind` / `front` 不进入本刀，以换取 `paginate_root` 的绘制顺序保持页眉 → 正文 → 页脚。
- 接受页眉闭包是 `'static`，调用方克隆要捕获的字符串，以换取 `Decorations` 继续是现在的 `'static` `BandContent`。
- 接受 `render` 与 `to_pdf` 各排版一次，以换取 `Doc` 在绘制之后仍然完整，并且 `to_pdf` 不依赖某次 `RecordingCanvas` 日志。
- 接受字体字节在 `register_face` 和 `lower` 各解析一次，以换取 `Doc` 不持有带 advance 缓存的 `FontStore`。两次 `render` 的缓存互不污染。
- 接受非法栏宽、非法单元格、空的自定义标记在 `render` 时失败，以换取正文闭包保持不返回 `Result`。字体字节解析仍在 `register_font`，组装期就能 `?`。
- 接受一段只有一个 `Style`。`text` 多次调用拼成一个字符串。另一张脸是另一个段落。
- 接受列表标记用文档缺省样式，以换取 `ListSpec::marker_style` 在 `lower` 时就能填好，不必从项里反推。
- 接受 A 系列只做 A3 / A4 / A5，外加 Letter 与 `Custom`。其余幅面走 `Custom(Size)`。
- 接受根节点永远是 `FlowNode::Stack`，包括零块和单块。空文档因此得到刀 7 已有的一页空白。

## Alternatives considered

- **构建器在闭包里直接 `push` `FlowNode`，`document` 末尾立刻 `paginate_root`，结果收成一份已画好的记录。** 页眉闭包若在组装时还没跑，记录里就还没有页码；若在 `document` 末尾和正文一起排完，调用方就失去一棵与绘制无关的树。`if` 的结果只能从字形反推。本形状把树留在 `Doc` 上，绘制另算。
- **给 `Decorations` 加生命周期，让页眉闭包借用调用方栈上的 `&str`。** 能少一次 `clone`，但 `BandContent`、`paginate_root` 和所有现有装饰测试都要改签名。`'static` + `Arc` 把改动留在 `document`。
- **为了 §1 的 `h.left` / `h.right` 放在同一行，允许带内容使用 `Columns`，或在 `paint_pages` 里加三槽。** 这会改刀 7 的内容规则或绘制顺序。本形状用两段全宽段落加上 `Align`。
- **在 `paint_pages` 外包一层 `behind` / `front`，`text_at` 走 `single_line_run`。** 提案书要这层，但刀 7 的 `paginate_root` 自己负责 `start_page` 与绘制顺序，外层插不进去。要画水印就得改 `paint_pages`。本刀不改。
- **`document(...)?.to_pdf()` 先画进 `RecordingCanvas`，再把命令重放到 `KrillaCanvas`。** 测试和 PDF 共用一次排版，但 PDF 路径会依赖命令日志的完备性，而 `to_pdf` 也无法在没人要记录时跳过这份日志。两条路径都从 `Block` 走 `lower`。
- **单元格和列表项各做一个与 `Block` 平行的枚举，在类型上禁止 `columns`。** 禁止表会有两份，和 `check_content` 容易分叉。公开的就是一个 `Block`。构建器少给几个方法；`lower` 仍调用现有构造器。
- **正文闭包一律返回 `Result`，以便 `line_height` 和 `col_span(0)` 当场 `?`。** §1 的 `doc.paragraph(|p| p.text(..))` 每一行都要多一个 `Ok(())`。行高系数交给 `Style::new`，span 用 `NonZeroU16`，零写不出来。
- **组装期就把段落断行，`lower` 只搬运 `ParagraphNode`。** 栏宽和单元格宽度要等 `split` 才知道。提前断行会把框宽定死，续页换框时对不上刀 7 的 `TextNode`。

## Open questions and risks

- 页眉两行是否以后要改成同一基线的左右两槽。那一步要动带的内容规则或绘制，不属于本候选。在那之前，与正文左缘对齐的页眉靠调用方自己在字符串前留空，库不提供段前缩进。
- 缺省字号 12 pt、行高系数 1.2、标记列 18 pt 没有提案书条款。它们写在上面的默认值表里。若要改数字，改 `lower` 的缺省样式和列表构建器盖章的那一处，并改对应测试。
- 自定义标记是否消耗 `ol` 的序号，由现有 `ListNode::new` 决定（消耗）。DSL 不另作计数。
- `ul` 上调用 `start` 被忽略。若这容易让人以为无序列表会从该数起编号，实现时可以不给 `ListBuilder` 这个方法，只在 `ol` 的构建器上提供。当前为了少一个类型，两个列表共用 `ListBuilder`。
- `register_face` 的 `index` 覆盖 TTC。调用方是否真有集合字体，本刀没有样例。单文件字体用 `register_font`（index 0）。
- U+2603 是否不在夹具 cmap 里，实现时用一次 `MissingGlyph` 确认。断言的是错误种类和空的命令日志。
- 空段落 `text == ""` 的分页结果就是 `TextNode` 对空字符串的现有行为。本候选不另定。
- `to_pdf` 与 `render` 各排一次。长文档若两者都要，测量成本加倍。本刀没有缓存 `FlowNode` 的字段；加上缓存就要处理「树改了没有」.

## Next implementation step

把 `src/document.rs` 换成 `src/document/{mod,ast,lower,tests}.rs`，`lib.rs` 改为私有模块加重导出。先让 `document(PageSize::A4, |doc| Ok(()))` 能 `render` 出一页空白，并且 `lower` 得到空的 `FlowNode::Stack`。然后按上文测试的顺序补段落、页眉、栏、表、列表。`layout/` 与 `backend/` 保持不动。
