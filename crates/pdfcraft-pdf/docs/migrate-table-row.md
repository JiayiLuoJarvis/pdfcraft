# 迁到表格行与边框类型

这份说明给还在用当前已提交 API 的调用方。对照的是仓库 `HEAD`（`header: Vec<Vec<CellSpec>>`、`border_width` / `border_color`）和工作树上的未发布改动。版本号仍是 `0.1.0`。`pdfcraft-forms` 和 `pdfcraft-orgchart` 的类型不变。

按下面的顺序改。每一步都对应一类编译错误。改完再看文末的说明。版式没有变化。

## 1. 把行包进 `TableRow`

`header` 和 `body` 从 `Vec<Vec<CellSpec<K>>>` 改成 `Vec<TableRow<K>>`。每行包一层 `TableRow { cells: ... }`。

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
use pdfcraft_pdf::spec::TableRow;

TableSpec {
    header: vec![TableRow {
        cells: vec![
            CellSpec {
                content: vec![CellBlock::Text { text: "Item".into(), font: () }],
                ..CellSpec::default()
            },
        ],
    }],
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
    .map(|item| {
        TableRow {
            cells: vec![CellSpec {
                content: vec![CellBlock::Text { text: item.name.clone(), font: () }],
                ..CellSpec::default()
            }],
        }
    })
    .collect();
```

## 2. 把边框字段收进 `TableBorder`

`TableSpec` 上的 `border_width` 和 `border_color` 两个字段合并成一个 `border: TableBorder { width, color }`。不要边框时写 `TableBorder::default()`（宽度为零，不画线）。

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
use pdfcraft_pdf::spec::TableBorder;

TableSpec {
    border: TableBorder {
        width: NonNegative::pt(0.5).unwrap(),
        color: Color::rgb(0, 0, 0),
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
        cells: vec![CellSpec {
            content: vec![CellBlock::Text { text: "Name".into(), font: () }],
            ..CellSpec::default()
        }],
    }
}
```

已有 `Vec<CellSpec>` 想复用时，用 `TableRow { cells }` 包一下：

```rust
let cells: Vec<CellSpec<()>> = build_cells(&item);
TableRow { cells }
```

已有行要追加单元格时，改的是 `row.cells`：

```rust
row.cells.push(CellSpec {
    content: vec![CellBlock::Text { text: extra, font: () }],
    ..CellSpec::default()
});
```

## 4. FRU 语法的变化

`TableSpec` 仍实现 `Default`，`..TableSpec::default()` 继续可用。`TableRow` 也实现 `Default`（空行），所以 `TableRow { cells }` 的简写仍然成立。

`TableBorder` 实现 `Default` 而不是要求全写。非默认边框用全写 `TableBorder { width, color }`，不要只写一半再补 `..Default::default()`。宽度为零时不看颜色，不画线。

`TableRow` 是 `#[must_use]`。构造后不用（既不放进 `header` / `body`，也不返回）时会有 warning，和别的 `Spec` 类型一致。

## 5. 版式没有变化

这些不用改调用代码，生成的 PDF 和以前相同。

行只是多了一层具名类型，`header` / `body` 的行数、每行的单元格数、`repeat_header` 的重复行为都不变。边框只是把两个字段收进一个 struct，线宽、颜色、零宽不画线的规则都不变。分页、列宽、单元格内边距的错误照旧。

