# pdfcraft-pdf 0.7 缺陷修复规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-19 |
| 状态 | **已评审，可开工**。§10 的 Q1 到 Q8 已于 2026-11-19 全部定下 |
| 范围 | `crates/pdfcraft-pdf/src`。不改公共 API，不改其余三包 |
| 版本 | 全部合入后发 `pdfcraft-pdf 0.7.1`。只改行为，不改公共签名，不算 breaking（工程规范 VER-1） |
| 来源 | 2026-11-19 全包审查。7 个缺陷都用公共 API 写探针复现过 |
| 冲突 | 与 [`提案书.md`](提案书.md) 冲突时以提案书为准 |

本文只收能用公共 API 触发、产出错误结果的缺陷。已定提案（提案书 §15）见 [`pdfcraft-pdf-review-impl-spec.md`](pdfcraft-pdf-review-impl-spec.md)，排在本文之后。审查里的其余质量项（静默兜底、死代码、诊断措辞、性能）另立规格，不在本文。行号以 HEAD `05c3efd` 为准。`pdfcraft-pdf` 的代码与 `eec6011` 相同。

---

## 1. 总表

| # | 缺陷 | 触发 | 后果 | 位置 |
| --- | --- | --- | --- | --- |
| B1 | 页脚里的旋转 mark 画错位置 | 页脚 `Mark::Rotated` | 图形画到页面外，无报错 | `layout/frame.rs:99-101`、`:326-331` |
| B2 | 弯引号不受禁则约束 | 正文含 `“ ” ‘ ’` | `”` 落到行首，`“` 留在行尾 | `text/break_class.rs:40-49` |
| B3 | 零高度块多出空白页 | `Spacer(0)`、空 `Inset`、空 `Stack` 挨着 `PageBreak` | 文档首尾多一张空白页 | `layout/paginate.rs:321-342` |
| B4 | 软换行的行尾空格计入行宽 | End、Center、Justify 对齐，或带下划线、删除线的多行段落 | 右缘不齐，装饰线画到空格下 | `text/line_break.rs:373-381` |
| B5 | 负字距遇到组合字符整份失败 | `tracking < 0` 且文本含分解形式的组合字符 | `NegativeTracking`，整份失败 | `layout/line_run.rs:125-134` |
| B6 | GIF、WebP 正文到序列化才解码 | 尺寸不一致或正文损坏的 GIF、WebP | 产出坏 PDF 无报错，或报无位置的 `Backend` | `backend/images.rs:62-91`、`:161-174` |
| B7 | 长度加法溢出成 `inf` | 公共 `Length` 运算，两个接近 `f32::MAX` 的值相加 | 带 `NonNegative` 品牌的 `inf`，相减得 `NaN`，`Eq` 失效 | `geom.rs:249-283`、`:317` |

---

## 2. B1 页脚里的旋转 mark 画错位置

### 2.1 现象

页面 200×200 pt，下边距 30。页脚放一个 10×10 的矩形，绕自身中心旋转 90°。

| | 页面坐标 |
| --- | --- |
| 期望 | `(10,175)-(20,185)` |
| 实际 | `(-330,5)-(-320,15)` |

页眉、`behind`、`front` 不受影响。它们的原点是 `(0,0)`，`draw_fragment` 在原点为零时直接用原矩阵。

### 2.2 根因

`Affine::then(self, next)` 先做 `self` 再做 `next`。`draw_fragment` 把变换写成 `translate(o).then(A)`，作用到点 `q` 上是 `A(q + o)`。内部图元同时被平移成 `p + o`。最终落点是 `A(p + 2o)`。正确值是 `A(p) + o`。

`Fragment::with_offset` 用同样的写法。当前只有 `decorate.rs:333` 产生 `PushTransform`，正文节点不产生，所以这条路径在 flow 里走不到。它是同一个缺陷，一起修。

`check_mark_bounds` 在页脚自身坐标里检查，不受影响，也查不出这个缺陷。

### 2.3 修法

两处都改成共轭：

```rust
Affine::translate(-dx, -dy).then(*affine).then(Affine::translate(dx, dy))
```

内部点已经是 `p + d`。先减 `d` 回到局部坐标，再做 `A`，最后加 `d`，得到 `A(p) + d`。嵌套变换逐层套用仍然正确。

`draw_fragment` 与 `with_offset` 的平移逻辑重复。修复时抽成一个 `FragmentCommand` 平移函数，两处共用，避免以后只改一处。

### 2.4 回归测试

`tests/document.rs` 加一条：页脚 `Mark::Rotated` 90° 矩形，从 `RecordedPage` 取 `PushTransform` 与 `FillPath`，把路径点乘矩阵后断言包围盒是 `(10,175)-(20,185)`。再加一条非对称图形（例如 30° 旋转的长矩形），防止对称图形掩盖错误。

---

## 3. B2 弯引号不受禁则约束

### 3.1 现象

文泉驿正黑 12 pt，栏宽 120 pt。

| 输入 | 第 1 行 | 第 2 行 |
| --- | --- | --- |
| `一二三四五六七八九十”好` | `一二三四五六七八九十` | `”好` |
| `一二三四五六七八九“十好` | `一二三四五六七八九“` | `十好` |
| `一二三四五六七八九十）好`（对照） | `一二三四五六七八九` | `十）好` |

### 3.2 根因

`OP` 与 `CL` 表没有 U+201C、U+201D、U+2018、U+2019。这四个字符落到 `PairClass::Other`。断行表里 `ALLOW[Id][Other]` 与 `ALLOW[Other][Id]` 都是 `true`。

### 3.3 修法

- `“`（U+201C）、`‘`（U+2018）加入 `OP`。
- `”`（U+201D）加入 `CL`。
- `’`（U+2019）按上下文分类，见下。

`’` 也是英文撇号。直接放进 `CL` 会让 `don’t` 在 `’` 与 `t` 之间可断，因为 `ALLOW[Cl][Al]` 是 `true`。现在它是 `Other`，`ALLOW[Other][Al]` 是 `false`，不断。修法是：`’` 前一簇是 `Al` 时保持 `Other`，否则归 `CL`。这需要 `classify` 拿到前一簇的类别，见 §10 Q2。

两端对齐不受影响。`is_fullwidth_op_or_cl` 要求码位不小于 U+3000，所以这四个字符不会成为第二阶段的拉伸簇。汉字与引号之间的间隙仍可拉伸，因为汉字一侧满足条件。这符合 §12.3「不做标点挤压」。

### 3.4 回归测试

- `break_class.rs` 单元测试：四个字符的分类，`don’t` 与 `说’` 的上下文分类。
- `tests/document.rs`：§3.1 前两个输入，断言 `”` 不在行首，`“` 不在行尾。
- 英文 `don’t stop` 在只容得下 `don’t` 的栏宽里不拆开。

---

## 4. B3 零高度块多出空白页

### 4.1 现象

A4，正文 `B` 是一个段落。

| 正文 | 期望页数 | 实际页数 |
| --- | --- | --- |
| `[PageBreak, B]` | 1 | 1 |
| `[Spacer(0), PageBreak, B]` | 1 | 2（第 1 页空白） |
| `[Inset { children: [] }, PageBreak, B]` | 1 | 2（第 1 页空白） |
| `[B, PageBreak, Spacer(0)]` | 1 | 2（第 2 页空白） |

提案书 §2.9 规定：文档开头的 `PageBreak` 不起作用，流结束时挂起页若仍为空就丢掉。

### 4.2 根因

`place_body_node` 的 `Split::Fit` 分支总是把片段推进 `placed`，并把游标设成 `PageCursor::HasBody`。它不看片段是否有效。`flow.rs:135` 已有 `fragment_is_effective`：高度大于 0 或有绘制命令。

### 4.3 修法

`Split::Fit` 分支先判断 `fragment_is_effective`。

- 有效：照旧推入 `placed`，`state.y` 加高度与间距，游标设为 `HasBody`。
- 无效：不推入，不动 `state.y`，不动游标。

`fragment_is_effective` 改成 `pub(crate)`，供 `paginate` 使用。`Split::Partial` 的 head 同样处理。

无效片段不会带锚点。`flow.rs` 的 `stamp_anchor` 只打在有效片段上。实现里加 `debug_assert!(fragment.anchors.is_empty())` 锁住这一点。

### 4.4 回归测试

`tests/document.rs` 加 §4.1 的四种正文，断言页数。再加 `[Spacer(0), PageBreak, PageBreak, B]`，断言 2 页（连续两个 `PageBreak` 产生一张空白页，§2.9）。

---

## 5. B4 软换行的行尾空格计入行宽

### 5.1 现象

文泉驿正黑 12 pt，`"AAAA BBBB"`，页宽 40 pt。

| 对齐 | 第 1 行墨迹右缘 | 第 2 行墨迹右缘 |
| --- | --- | --- |
| End | 约 36.4 | 40.0 |
| Center | 比第 2 行偏左约 1.8 | |
| Justify（A4 拉丁段落） | 约 591.56，行宽 595.28 | |

Justify 时，行尾空格还在第一阶段分到一份余量。下划线和删除线画到行尾空格下方。

### 5.2 根因

`pack_lines` 在空格之后断行（`line_break.rs:373-381`）。已放进本行的 U+0020 留在 `[line_start, split)` 内。它的宽度进入 `committed`，它的簇进入 `segs`。下游的 `place_line`、`build_roles`、装饰线都把它当成行内内容。

提案书 §12.1 写「行尾丢掉的空格不画」，§12.3 写「行尾丢掉的空格……不参与分配」。设计稿第 7 步写「放进本行的空格留在本行并绘制」。两者冲突，按提案书。

### 5.3 修法

在 `emit_line` 里，从行尾向前跳过连续的 `PairClass::Sp` 原子，得到可见终点 `visible_end`。

- 行宽用 `[line_start, visible_end)` 计算。对齐偏移、Justify 余量都用它。
- `segs` 只覆盖到 `visible_end`。行尾空格不绘制，不进 `build_roles`，不画装饰线，不进链接矩形。
- `visible_end` 前一个字形原本因为后面是空格而带字距间隙。它现在是行尾字形，字距间隙丢掉（§12.1「字距只在后面还是文字时才加」）。
- 断行判定不变。`trial <= avail` 仍按含空格的宽度比较。

适用于所有行尾：软换行、硬换行前、段落末行（§10 Q4）。`CHANGELOG.md` 的 `Fixed` 条目提示：用行尾带下划线的空格做填空横线的调用方改用 `Blank`。

### 5.4 回归测试

- End 与 Center：两行墨迹右缘相等，或居中线相等，误差 `pt_near`。
- Justify：非末行最后一个可见字形的右缘等于框宽。
- 下划线：`FillPath` 或 `StrokePath` 的右端等于最后一个可见字形的右缘。
- 链接跨行时，第 1 行链接矩形的右缘不含行尾空格。

---

## 6. B5 负字距遇到组合字符整份失败

### 6.1 现象

DejaVu Sans，`tracking = -0.5 pt`。

| 输入 | 结果 |
| --- | --- |
| `"café"`（U+00E9 预组合） | 成功 |
| `"cafe\u{301}"`（e 加 U+0301） | `NegativeTracking { advance: 0.0, tracking: -0.5 }` |

两种写法在 Unicode 里等价，粘贴文本常是后一种。

### 6.2 根因

`ink_width` 把字距间隙给簇内最后一个不省略的字形。分解形式里这个字形是组合符，advance 为 0。`0 + (-0.5) < 0`，于是报错。

### 6.3 修法

校验改成整簇：`body + tracking >= 0`，`body` 是簇内全部字形 advance 之和。接收字距的字形不变，仍是最后一个不省略的字形。组合符的绘制宽度变成负值。PDF 的 `TJ` 接受负的调整量。krilla 的 `KrillaGlyph::new` 收 `f32` advance，见 §10 Q5。

`ErrorKind::NegativeTracking` 的 `advance` 字段改填整簇宽度。字段名不变，不算 breaking。

### 6.4 回归测试

- `"cafe\u{301}"` 在 `tracking = -0.5` 下成功，`DrawGlyphRun` 的字形数与预组合版本的差为 1。
- 簇宽 3 pt、`tracking = -4` 仍报 `NegativeTracking`，`advance` 是 3。

---

## 7. B6 GIF、WebP 正文到序列化才解码

### 7.1 现象

| 输入 | `add_image` | `layout_spec` | `to_pdf` |
| --- | --- | --- | --- |
| 头合法、正文是垃圾的 GIF | 成功 | 成功 | `Backend { "cannot serialize the PDF" }`，`at = None` |
| 逻辑屏 200×200、帧 4×4 的 GIF | 成功 | 成功 | 成功。XObject 声明 200×200，数据只有 48 字节，PDF 损坏 |
| 扫描数据损坏的 JPEG | 成功 | 成功 | 成功。坏数据原样嵌入 |

反方向（推理，未探针）：逻辑屏写 1×1、帧很大的 GIF 能通过 `ImageEdge` 与 `ImagePixels` 检查，大帧在 `finish` 时才解码，绕过 `Limits`。

### 7.2 根因

`ImageTable::register` 只对 PNG 调 `validate_png_pixels` 做完整解码。krilla 0.8.2 的 `Image::from_gif`、`from_webp`、`from_jpeg` 只读文件头，正文在 `Document::finish` 时解码。`image.size()` 对 GIF 返回逻辑屏尺寸，不是帧尺寸。

### 7.3 修法

- **GIF**：`register` 里用 `gif =0.14.2` 解码首帧。帧的位置不是 `(0,0)` 或宽高不等于逻辑屏时报 `ErrorKind::Image`。先按帧尺寸查 `ImageEdge` 与 `ImagePixels`，再解像素，解码失败报 `ErrorKind::Image`。
- **WebP**：`register` 里用 `image-webp =0.2.4` 完整解码一次，失败报 `ErrorKind::Image`。
- 规则统一为：除 JPEG 外，位图在 `add_image` 时完整校验，与 PNG 一致（§10 Q6）。两条新直依都已在依赖图里（krilla 传递），按 DEP-2 / DEP-3 钉 `=` 版本，`default-features = false`。PR 6 同步更新 `AGENTS.md`「字形管线与依赖」里的直依清单，以及 `Cargo.lock`。
- **JPEG**：DCT 数据原样嵌入是 PDF 的正常做法，不在库里解码。在 `ImageFormat::Jpeg` 与 `Resources::add_image` 的 rustdoc 里写明「只校验文件头」。
- **兜底**：`emit.rs` 把 `document.finish()` 返回的 `KrillaError::Image` 映射成 `ErrorKind::Image`，不再是 `Backend`。

### 7.4 回归测试

测试代码里按字节拼 GIF，不新增二进制夹具。

- 帧尺寸不等于逻辑屏：`add_image` 返回 `ErrorKind::Image`。
- 逻辑屏 1×1、帧超 `max_image_pixels`：`add_image` 返回 `LimitExceeded { kind: ImagePixels }`。
- 正文 LZW 数据损坏：`add_image` 返回 `ErrorKind::Image`。

---

## 8. B7 长度加法溢出成 inf

### 8.1 现象

`NonNegative::pt(f32::MAX)? + NonNegative::pt(f32::MAX)?` 得到 `inf`，类型仍是 `Length<NonNegative>`。两个这样的值相减得到 `NaN` 的 `Length<Finite>`。`Length`、`Point`、`Rect`、`Size` 都实现了 `Eq`，`NaN` 不等于自身，违反 `Eq` 的约定。`geom` 模块文档写「invalid numbers fail at construction」，与此不符。

下游会拦住：`inf` 的 `Spacer` 报 `NonFiniteLength`，`inf` 页边距报 `BodyFrame`。所以不会产出坏 PDF，但类型给出的保证不成立。

### 8.2 根因

`geom.rs:249-283` 的 `Add`、`AddAssign` 与 `geom.rs:317` 的 `Sum` 直接相加后调 `from_trusted_pt`，不检查结果。`Sub` 的输入都是有限值，只有输入已是 `inf` 时才产出 `NaN`。`Point::new` 只收 `Length<Finite>`，`NaN` 都来自 `inf - inf`。

### 8.3 修法

三处加法在 `f32::MAX` 处饱和：`Self::from_trusted_pt((self.pt + rhs.pt).min(f32::MAX))`。`Sum` 逐项用饱和加法。公共签名不变（§10 Q8）。

饱和后的值仍是有限非负数，品牌成立。这么大的长度在下游照样以「高于页面」「超出框宽」之类的错误被拒绝。

### 8.4 回归测试

`geom.rs` 单元测试：

- `MAX + MAX` 等于 `MAX`，`as_pt().is_finite()` 为真。
- `[MAX, MAX, MAX].into_iter().sum()` 等于 `MAX`。
- `(MAX + MAX) - (MAX + MAX)` 等于 `0`。

---

## 9. 实施顺序

一个缺陷一个 PR。每个 PR 带 §2 到 §8 的回归测试，`CHANGELOG.md` 写进 `Fixed`，提交前跑工程规范 TOOL-5 的完整门禁。

| PR | 缺陷 | 依赖 |
| --- | --- | --- |
| 1 | B1 | 无 |
| 2 | B3 | 无 |
| 3 | B5 | 无 |
| 4 | B2 | 无 |
| 5 | B4 | 提案书 §12.1 已补「行尾空格在所有行尾丢掉」（2026-11-19） |
| 6 | B6 | 无 |
| 7 | B7 | 无 |
| 8 | 发 `0.7.1` | 1 到 7 |

---

## 10. 待决问题

**Q1（B1）平移逻辑是否收成一个函数。** **已定（2026-11-19）：收。**`draw_fragment` 改成「平移每条命令后调 canvas」，与 `with_offset` 共用 `translate_command(cmd, dx, dy)`。不引入 `Group` 帧树，那是架构改动，不属于本规格。

**Q2（B2）`’` 怎样区分撇号与右引号。** **已定（2026-11-19）：方案 A。**

| 方案 | 做法 | 代价 |
| --- | --- | --- |
| A（建议） | 前一簇是 `Al` 时归 `Other`，否则归 `CL` | `classify` 要拿到前一簇类别，改 `cluster.rs` 一处调用 |
| B | 一律归 `CL`，另把 `ALLOW[Cl][Al]` 对 `’` 特判为 `false` | 断行表出现字符级特例 |
| C | 一律保持 `Other`，只修 `“ ” ‘` | `说’` 的 `’` 仍能落到行首 |

**Q3（B2）是否同时补其他常见禁则字符。** 候选：`…`（U+2026）、`—`（U+2014）成对不拆，`、` 以外的半角 `,.!?` 不落行首。**已定（2026-11-19）：本规格只修四个引号，其余走提案讨论。** 半角标点不能直接归 `CL`，否则 `3.14`、`e.g.` 会在点后可断，需要像 `’` 一样按上下文分类。

**Q4（B4）哪些行丢行尾空格。** **已定（2026-11-19）：方案 A。**

| 方案 | 范围 |
| --- | --- |
| A（建议） | 所有行尾：软换行、硬换行前、段落末行 |
| B | 只软换行 |

提案书「行尾丢掉的空格」没有限定换行种类。方案 A 与 CSS `white-space: normal` 一致，用户写在 `\n` 前或段末的空格不再影响右对齐。

**Q5（B5）负 advance 是否被后端接受。** **已查（2026-11-19）：接受。** `line_run.rs:252-255` 把 `draw - body` 加到接收字形上，不钳位。`canvas.rs` 与 `backend/` 不校验 `advance_norm` 的符号。krilla 0.8.2 `content.rs:622-646` 把 `x_advance - font_advance` 写成 `TJ` 调整量，不要求非负。`draw_widths` 只在整簇层面钳到 0，修复后的校验保证整簇不为负，不会触发钳位。PR 3 里仍用 `to_pdf` 加 poppler `pdftotext -bbox` 核对一次组合符位置。

**Q6（B6）GIF 与 WebP 怎样解码。** **已定（2026-11-19）：方案 B。**

| 方案 | 做法 | 代价 |
| --- | --- | --- |
| A | GIF 自写块解析，只读到首帧描述符。WebP 完整解码 | GIF 正文损坏仍留到 `to_pdf`，报无位置的错误 |
| B（已选） | GIF 直依 `gif =0.14.2`，WebP 直依 `image-webp =0.2.4`，都完整解码 | 两条新直依，依赖图不新增 crate。多一次解码，与 PNG 相同 |
| C | 只做兜底映射，不提前解码 | 坏 GIF 仍能产出坏 PDF |

**Q7（版本）** **已定（2026-11-19）：全部合入后一次发 `0.7.1`。** B4 会改变右对齐、居中、两端对齐与带装饰线的行的排版结果，批量输出与 `0.7.0` 不再逐字节相同。`CHANGELOG.md` 在 `Fixed` 里写明这一点。

**Q8（B7）长度加法怎样防溢出。** **已定（2026-11-19）：在 `f32::MAX` 处饱和，公共签名不变。** 不改成返回 `Result` 的 `checked_add`。那样调用方的长度运算处处要 `?`，且属于 breaking。
