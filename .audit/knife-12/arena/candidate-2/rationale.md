# Rationale

## Problem

§2.3 要的是当前框里一条不可拆的水平线：默认吃满框宽，可以更短并按 Start / Center / End 放置，高度是上下空白加线粗，Fragment 里一条水平 `Stroke`。仓库里已经有水平描边（表边框、`Layer::line`）和两个叶子流节点（可拆的 `SpacerNode`、不可拆的 `ImageNode`），但 `FlowNode` 没有这条线。单元格的 `CellContent` 只有 `Text` 和 `Image` 两个工厂臂，而 §2.6 允许格内同时放段落、分隔线和 spacer。线粗与线色已经有公共类型 `Pen`。本候选把节点放进 `flow.rs`，把公共名字定成 `rule`，并用一条嵌套流补上格内组合，而不是再添一个 `hline` 模块和一组 `Cell::*` 工厂。

## Usage (caller's view)

```rust
use quire_pdf::{document, Color, NonNegative, PageSize, Pen, Positive, Rule};

document(PageSize::A4, |doc| {
    doc.paragraph(|p| p.text("正文"));
    doc.rule(Rule::default());
    doc.rule(Pen::new(Color::rgb(180, 180, 180), Positive::pt(0.5)?));
    doc.rule(
        Rule::new(Pen::new(Color::rgb(0, 0, 0), Positive::pt(1.0)?))
            .width(Positive::pt(72.0)?)
            .start()
            .space_before(NonNegative::pt(6.0)?)
            .space_after(NonNegative::pt(6.0)?),
    );
    doc.ul(|list| {
        list.item(|flow| {
            flow.paragraph(|p| p.text("附件"));
            flow.rule(Rule::default());
        });
    });
    doc.table(|table| {
        table.row([
            Cell::new("名称"),
            Cell::flow(|flow| {
                flow.paragraph(|p| p.text("说明"));
                flow.spacer(NonNegative::pt(4.0).unwrap());
                flow.rule(Rule::default());
            }),
        ]);
    });
    Ok(())
})?;
```

导入 `Rule` 与现有的 `Pen`、长度和颜色。`flow.rule` 返回 `()`。列表沿用 `item`。格子沿用 `row([...])`，组合内容进 `Cell::flow`。类型草图在 `design.md`，与上面三段调用一致。

## Shape

`Rule` 是唯一的公共配置值：一支 `Pen`、可选定长、对齐、两段非负空白。`Flow::rule(impl Into<Rule>)` 是唯一的推进方法；`From<Pen>` 覆盖「只改笔、其余默认」。降级是字段搬家，不会失败。`HLineNode` 与 `SpacerNode` 同文件，`FlowNode::HLine` 在 `split` 时把 `LayoutCtx` 丢掉，只把 `Frame` 交给节点。

框宽默认、中线 y、不可拆、超宽先于超高，都封在 `HLineNode::split`。调用方看不到 `Fragment` 和 `MoveToNextPage`。线宽、线色、空白在类型边界上已经是 `Positive` / `NonNegative` / `Color`；「定长是否长过这块框」要等 `Frame` 出现，所以检查放在 `split`，不放在 setter（boundary-discipline）。高度是三项之和，节点内现算，不另存一份会漂移的高度字段。`spacing_after` 恒为 0，空白只存在于这个和里。

公共面是 `Rule` 的几个 setter、`Flow::rule`、`Cell::flow`。藏起来的是分页策略、描边几何，以及「栏、格、列表项、Band 与 spacer 同一允许臂」。`Cell::flow` 复用整份 `Flow`：格内能写什么由已有的 `check_content(TableCell)` 决定，不为单元格再列一份方法表。`Band` 仍只有 `left` / `right`，公共 API 写不进线；允许矩阵不给 Band 单开例外。

对齐公式在 `image.rs` 和 `paragraph.rs` 里各有一份。这里再写一遍三行，不抽公共函数，避免这刀改到图片和段落。

## Synthesis decision

Arena 在比较候选后填写本节。本文件是 candidate 2，不选定底稿。

## Tradeoffs accepted

- 接受 `Cell` 失去 `Clone` / `PartialEq` / `Eq`，以及 `Cell::flow` 的闭包是 `'static`，以换格内与正文共用一套 `Flow` 方法。
- 接受继续保留 `Cell::new` 与 `Cell::image`，以换这刀不去改已经落地的文字格和图片格。
- 接受内部类型名 `HLineNode` 与公共方法名 `rule` 并列，以换节点能挨着 `SpacerNode`，同时公共名字不与 `Layer::line` 抢同一个词。
- 接受默认黑、1 pt（§2.3 没写死这个数），以换 `Rule::default()` 和 `flow.rule(pen)` 不用先填线粗。
- 接受 `check_content` 在 Band 里放行 `HLine`，以换它和 spacer 共用一条允许臂；Band 的公共方法仍构造不出这条线。
- 接受上下空白算进不可拆高度（空白过大会 `UnsplittableTallerThanFrame`），以换一个高度公式，并且空白不会像段后距那样被丢掉。
- 接受第四份对齐三行，以换不碰 `image.rs` 和 `paragraph.rs`。

## Alternatives considered

- 新建 `hline.rs`，公共方法 `flow.hline(|h| …)`，再加 `Cell::hline`。模块只转发一种叶子 `split`，调用链变长，格内每多一种孩子就多一个工厂。`rule` 与嵌套流把配置和格内组合都收进已有的 `Flow`。
- 给 `CellContent` 做 `CellOp` 枚举（段落 / spacer / rule / 图片）以保住 `Cell: Eq`。每增加一个 `Flow` 方法就要改这一份清单，和 `check_content` 的节点清单手写同步。闭包臂让新方法自动进入格子，拒绝仍由矩阵负责。
- `flow.rule(|r| r.width(…))` 做成和 `ImageBox` 一样的闭包 builder。图片闭包要带着文档令牌解码；`Rule` 降级不需要文档上下文，闭包只是把一个 `Copy` 值又包了一层。链式值一种写法就够。

## Open questions and risks

- 默认 1 pt 黑是否就作为 §2.3「默认实线」的实现默认？
- `Cell` 去掉 `Eq` 之后，若有调用方把格子放进需要相等比较的容器，是否必须在这刀改回可比较的表示？
- `Cell::flow` 要求 `'static`。调用方是否需要在格子闭包里借用 `document` 闭包的局部引用？需要的话，推迟闭包这条路要改成在 `row` 时就拿到父 `Flow`。
- 高度和非有限时用 `InvalidGeometry`，是否够用，还是要单独的错误变体？

## Next implementation step

在 `src/layout/flow.rs` 加上 `HLineNode`、`FlowNode::HLine` 和 `split` 单测，通过后再改 `document.rs` 的 `Rule` 与 `Cell::flow`。
