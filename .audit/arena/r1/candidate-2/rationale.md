# R1 候选 2：取舍理由

## 为什么是内容盒再偏移

§2.10 的水平语义是整页宽一条带子，缩进是调用方给出的一对长度，不是左右页边距，也不是「先垫两边、剩下的分给槽」。

所以 lower 的不变量是：

1. 内容盒宽度先算出来：`content_w = page_width − inset_left − inset_right`。
2. 槽的 Start / End 只相对这个盒子。单槽占满盒子；双槽在盒内等分，gap 0。
3. 盒子整体平移 `inset_left`。页面绘制原点保持 x = 0，因为 `measure_band` 仍把片段放在 `(0, y)`，平移写在片段命令的 x 里。

槽宽分配和现在一致（双槽各一半、单槽占满可用宽），变的是可用宽的定义：从「页宽减去自动 margin 垫片」变成「页宽减去调用方 inset」。默认 0，槽贴页边。

外层列在需要平移或收窄时才出现，而且外层只有 `Fixed`。槽用的 `Ratio(1)` 关在内容盒里，不和 inset 抢 `resolve_column_widths` 的剩余宽。以后 Ratio / Auto 的分配怎么改，页眉缩进的宽度仍是那一次减法。

## 平移助手的缺口

`Fragment::with_offset(dx, dy)` 能平移字形和链接矩形，但是它作用在已经 split 的 `Fragment` 上，并且是 `layout::flow` 的内部方法。`Band::lower` 在 `band_content` 里运行，签名是 `Result<Vec<FlowNode>>`。测量、`LayoutCtx` 和 `Frame { width: page_width }` 都属于 `measure_band`。

从 `document.rs` 调用 `with_offset` 只有三条路，都不在本刀里：

- 在 lower 里 split。lower 没有字体上下文，并且会把 `measure_band` 的工作做第二遍。
- 加一个带水平偏移的 `FlowNode`。这是 `layout/` 的新语义。
- 把 `measure_band` 的 frame 改成 `content_w`、把 `PlacedFragment.origin.x` 改成 `inset_left`。工单要求 `measure_band` 的 x = 0 原点不动，lowering 留在 `document.rs`。

现成的、且 lower 已经在用的平移是 `ColumnsNode::split` 里的 `fragment.with_offset(x, 0)`。前导空 `Fixed(inset_left)` 只为了把这条 x 光标拨到内容盒的起点。它不是一条「左边距轨」。`inset_left == 0` 时这条轨不存在。

右缩进优先写成内容盒变窄之后的框内余量。`inset_left > 0` 时外层是两条 Fixed：`F(inset_left)` 空轨 + `F(content_w)` 槽盒。fixed 之和是 `page_width − inset_right`，求解器允许 rest 留下不分配。右缘来自盒子右端（单槽 End，或右半盒的 End），再加上已经平移的 `inset_left`，等于 `page_width − inset_right`。

只在 `inset_left == 0` 且 `inset_right > 0` 时追加尾部空 `Fixed(inset_right)`。原因是局部的：`ColumnsNode::new` 要求至少两列，而 `Fixed` 不能是 0，做不出「宽度为 0 的前导偏移」。尾轨宽度等于减法之后的余量，槽节点是内容轨的子节点，不是尾轨的兄弟。左右都大于 0 时没有这条尾轨。

## 和候选 1（双侧空轨）的差别

候选 1 的轨是 `spacer(inset_left) ++ 槽 Ratio ++ spacer(inset_right)`。两侧空 `Fixed` 和槽是同一层兄弟。槽宽是求解器在减掉两块垫片（和 gap）之后的剩余，再按 `Ratio(1)` 平分。左垫片同时承担「往右推」和「从可用宽里扣掉」。右垫片是扣宽的另一块输入。哪一侧 inset 非 0，哪一侧就有一条空轨。

候选 2 的槽不在那一层。槽只出现在一条宽度已经是 `content_w` 的 Fixed 轨里面。外层没有 Ratio，所以 inset 不经过「Fixed 垫片 + Ratio 吃剩余」这条公式。

同一组 inset 的绘制坐标应当相同：双槽各 `content_w / 2`，左原点 `inset_left`，右缘 `page_width − inset_right`。结构不同的地方是右缩进的身份。

- 候选 1：`inset(l, r)` 且两边都大于 0 时，轨是 `F(l) 空 + R + R + F(r) 空`（有槽的那些 R）。
- 候选 2：同一输入的轨是 `F(l) 空 + F(page_width − l − r) [槽盒]`。没有 `F(r)`。`r` 是页框里内容盒右边的空白。

仅右缩进时候选 2 才会出现一条尾部空 Fixed，而且槽盒仍是 `F(content_w)`，不是一条被右侧垫片挤窄的 Ratio。候选 1 在这种输入下是 `R… + F(r)`，槽自己就是那条 Ratio。

## 考虑过、没选的

### A. 改 `measure_band`：框宽改成 `content_w`，原点 x 改成 `inset_left`

绘制坐标直接满足验收，但 inset 必须从 `FlowNode` 再传到 `decorate`，`measure_band` 的全页宽和 x = 0 都要改。工单明确这两处不动。否决。

### B. 新的 `FlowNode` 偏移节点，lower 包一层

这就是「内容盒再 `with_offset`」的直接形状，但偏移节点住在 `layout/`，本刀不允许改 layout 语义，也不许为 R1 加公共或内部协议。否决。用列的 x 光标是 document.rs 里能调用到的同一效果。

### C. 候选 1 的双侧垫片，只把宽度来源从 margin 换成 inset

diff 更小，绘制坐标也对。它让 inset 继续当空轨，内容宽是剩余宽的导出值。本候选要的是内容盒当源头：双 inset 都大于 0 时右侧不出现空轨，槽 Ratio 不和外层 Fixed 一起求解。否决作为本候选的形状。

### D. 两侧都用空 Fixed，中间再套一条 `Fixed(content_w)`

`F(l) + F(content_w) + F(r)` 把右缩进又做成了轨，和「右缩进是余量」矛盾，也比两条 Fixed 多一条轨。否决。尾部空轨只留给「左缩进为 0、否则 `ColumnsNode` 建不出来」这一种。

### E. 单行盒里左 Start、右 End 叠在同一 `TextNode`

一个 `TextNode` 只有一个 `align`。两个 Text 放进 `Stack` 会上下排，不会并排。并排的现成节点就是 gap 0 的两列。工单写的是槽在「页宽减去 inset」里 Start / End，并允许两条 Ratio。保持现在的半盒模型，不让左右文字重叠。否决叠放。

### F. `inset` 缺省仍跟 margin，或 `Option` 表示「未设置则用 margin」

§2.10 要求画带时无视左右 margin。验收是不写 `inset` 时左槽 x = 0。默认必须是 0。否决。

### G. 字段做成 `pub`，或 `inset` 收 `f32` / 返回 `Result`

调用面是 `b.inset(m.left(), m.right())`。`Length<NonNegative>` 与 margin 访问器一致，负值和 NaN 在 `pt` / `mm` 构造期拒绝。字段公开会比 `Band` 其余数据多一条读路径，工单示例没有读字段。否决。

### H. `content_w <= 0` 时丢给 `Positive::pt` 或等 `ColumnWidths`

`Positive::pt(0)` 的错误是 `NonPositiveLength`，读起来像长度构造失败，数值还是减出来的内容宽。候选 1 可以等求解器对垫片求和报 `ColumnWidths`，因为那里的槽是 Ratio、垫片是 Fixed。本候选必须先有一条正的 `Fixed(content_w)`，减完不是正数时没有轨可建。映射到已有的 `DecorationWiderThanFrame { width: inset_left + inset_right, avail: page_width }`，不新增错误变体。无槽不报这条。

### I. 用 `default_column_gap()`（12pt）或非 0 gap 把右缩进编码成间距

gap 加在列与列之间，并且每多一列就减一次，不能表示「盒子右端到页边」的一段。带内槽间距今天就是 0。外层和盒内都继续 `NonNegative::ZERO`。否决。

## 风险

- 旧调用方在左右 margin 非 0 时，页眉会从对齐正文变成贴页边。这是 §2.10 和本刀的目标。crate 仍是 `0.x`，`publish = false`。
- 仅右缩进比两侧都缩进多一条尾部空轨。绘制坐标与「内容盒在 x = 0、宽 `content_w`」相同，但轨的个数不对称。这是 `ColumnsNode` 最少两列、又不能从 document.rs 做片段平移的结果，不是第二套缩进规则。
- 嵌套列多一层（外层 Fixed 内容盒，内层双槽 Ratio）。`check_content` 已经允许带内列、列内再列。空轨高度为 0，带高仍是槽的行盒高度。
- `content_w` 用与页宽、inset 相同的 `as_pt()` 减法，不加 epsilon。与列求解器的浮点习惯一致。`!(content_w > 0.0)` 把 NaN 和小于等于 0 一并拒掉。
