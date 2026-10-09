# 候选 1 取舍说明

## 选定的形状

新建 `src/layout/hline.rs::HLineNode`，split 照 `ImageNode` 写：宽先于高；高于整页 → `UnsplittableTallerThanFrame`；高于剩余 → `MoveToNextPage`；对齐取 x。公共面加一个 `HLine` 构建器，形态与 `ImageBox` 相同（`Default` + `&mut self` setter + 私有 `lower`），入口是 `Flow::hline(|h| …)` 和 `Cell::hline(|h| …)`。`check_content` 把 HLine 并进 Spacer 那一支。

选它的理由：
- **分页协议是现成的。** image 已经证明这套 split 判定加上“失败早于 `start_page`”能成立，分隔线没有任何新的分页语义。
- **调用面最小。** 只有一个类型，setter 和提案 §2.3 表里的项一一对应。列表项和栏不需要新 API，单元格只加一个构造器。
- **组装期不会失败。** 字段类型（`Positive` / `NonNegative` / `Color`）挡住了所有无效输入，所以 `lower` 是全函数。只依赖框尺寸的两项检查留在 split。
- **宽度延迟解析**（`None` = `avail.width`），同一个节点放进正文、栏、单元格、项内都正确。这是和 ImageNode 唯一的结构差别，也是必须有的差别。

## 考虑过并否决的方案

| 方案 | 否决原因 |
| --- | --- |
| `pen(Pen)` 一次设色和线粗 | 颜色和线粗绑成一组，各自的默认值丢了；只改色也得指定线粗 |
| 节点放进 `flow.rs`，与 `SpacerNode` 同居 | flow.rs 已经承担协议和组合子；线的判定和单测体量与 image 相当，应该和 image 一样有独立文件 |
| 构建时把宽解析成定值（完全照抄 ImageNode 的 `Size`） | 构建时不知道框宽，无法满足“默认 = W”和“栏内只画到栏宽” |
| `space_after` 放进 `Fragment.spacing_after` | 页底会被 stack 吞掉，与提案“计入节点高度、整条不可拆”冲突 |
| 无闭包重载 `doc.hline_default()` / `doc.hline()` 零参 | 多一个入口；`|_| {}` 已有 image 先例 |
| 通过 `Layer` / behind 绘制线 | 提案明确不是逃逸路径，不参与分页 |
| 通用单元格 stack（`Cell::flow(|f| …)`），让线自然进格 | 范围膨胀，§2.6 没要求；本刀只为线开 `Cell::hline` |

## 需要裁决者留意的地方

- 默认对齐是 **Center**，和 ImageBox 的 Start 相反，所以 setter 是 `start()` / `end()`，没有 `center()`。
- 默认 1 pt、黑色是实现自定（提案没钉），已在 design.md 的 Open questions 里列出。
- `space_after` 在页底不被吞掉。这是有意的，与段落间距行为不同。
