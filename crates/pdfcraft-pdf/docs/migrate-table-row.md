# 迁到表格行与边框类型

这份说明记录从 `header: Vec<Vec<CellSpec>>` 和 `border_width` / `border_color` 迁到具名表格行。那一步已经并进当前公共面。从 `0.5` 迁到 `0.6` 的完整对照见 [migrate-0.6.md](migrate-0.6.md)。`pdfcraft-forms` 和 `pdfcraft-orgchart` 的类型不变。

按下面的顺序改。每一步都对应一类编译错误。改完再看文末的说明。版式没有变化。

## 1. 把行包进 `TableRow`

`body` 从 `Vec<Vec<CellSpec<K>>>` 改成 `Vec<TableRow<K>>`。表头是 `Option<TableHeader<K>>`，行在 `TableHeader.rows` 里。每行的单元格字段是 `children`。

以前：

```rust
TableSpec {
    header: vec![vec![
        CellSpec {
            content: vec![CellBlock::Text { text: "Item".into(), font: () }],
            ..CellSpec::default()
        },
    ]],
    body,
    ..TableSpec::default()
}
```

现在：

```rust
use pdfcraft_pdf::spec::{TableHeader, TableRow};

TableSpec {
    header: Some(TableHeader {
        rows: vec![TableRow {
            children: vec![CellSpec {
                children: vec![Block::paragraph((), "Item")],
                ..CellSpec::default()
            }],
        }],
        ..TableHeader::default()
    }),
    body,
    ..TableSpec::default()
}
```

动态拼行的地方也一样，每行多包一层：

以前：

```rust
let rows: Vec<Vec<CellSpec<()>>> = items
    .iter()
    .map(|item| {
        vec![CellSpec {
            content: vec![CellBlock::Text { text: item.name.clone(), font: () }],
            ..CellSpec::default()
        }]
    })
    .collect();
```

现在：

```rust
let rows: Vec<TableRow<()>> = items
    .iter()
    .map(|item| TableRow {
        children: vec![CellSpec {
            children: vec![Block::paragraph((), item.name.clone())],
            ..CellSpec::default()
        }],
    })
    .collect();
```

## 2. 把边框字段收进 `TableBorder`

`border_width` 和 `border_color` 收成 `TableStyle.border`，类型是 `TableBorder { width, color }`。不要边框时写 `TableBorder::default()`（宽度为零，不画线）。

以前：

```rust
TableSpec {
    border_width: NonNegative::pt(0.5).unwrap(),
    border_color: Color::rgb(0, 0, 0),
    ..TableSpec::default()
}
```

现在：

```rust
use pdfcraft_pdf::spec::{TableBorder, TableStyle};

TableSpec {
    style: TableStyle {
        border: TableBorder {
            width: NonNegative::pt(0.5).unwrap(),
            color: Color::rgb(0, 0, 0),
        },
        ..TableStyle::default()
    },
    ..TableSpec::default()
}
```

## 3. 批量改行的 helper 与 map 写法

行变多一层后，原来直接操作 `Vec<CellSpec>` 的 helper 要改成操作 `TableRow`。常见有三种写法，按你的代码选一种。

返回整行的函数，返回类型改掉：

```rust
fn header_row() -> TableRow<()> {
    TableRow {
        children: vec![CellSpec {
            children: vec![Block::paragraph((), "Name")],
            ..CellSpec::default()
        }],
    }
}
```

已有 `Vec<CellSpec>` 想复用时，用 `TableRow { children }` 包一下：

```rust
let children: Vec<CellSpec<()>> = build_cells(&item);
TableRow { children }
```

已有行要追加单元格时，改的是 `row.children`：

```rust
row.children.push(CellSpec {
    children: vec![Block::paragraph((), extra)],
    ..CellSpec::default()
});
```

## 4. FRU 语法的变化

`TableSpec` 仍实现 `Default`，`..TableSpec::default()` 继续可用。`TableRow` 也实现 `Default`（空行），所以 `TableRow { children }` 的简写仍然成立。

`TableBorder` 实现 `Default` 而不是要求全写。非默认边框用全写 `TableBorder { width, color }`，不要只写一半再补 `..Default::default()`。宽度为零时不看颜色，不画线。

`TableRow` 是 `#[must_use]`。构造后不用（既不放进 `header` / `body`，也不返回）时会有 warning，和别的 `Spec` 类型一致。

## 5. 版式没有变化

这些不用改调用代码，生成的 PDF 和以前相同。

这一步只是把行收成具名类型。行数、每行的单元格数不变。重复表头的开关现在在 `TableHeader.repeat` 上，默认 `true`。边框收进 `TableStyle.border` 之后，线宽、颜色、零宽不画线的规则都不变。分页、列宽、单元格内边距的错误照旧。

