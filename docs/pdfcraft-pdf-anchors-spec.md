# pdfcraft-pdf §12.4 文内跳转与书签规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-19 |
| 状态 | **已定稿。** 产品要点见提案书 §12.4（2026-10-16）；可执行细则经 2026-10-19 讨论钉死（§9）。与提案书冲突时以提案书为准 |
| 范围 | `pdfcraft-pdf`：正文锚点块、文内 / 外链统一 `LinkTarget`、书签树；页眉页脚 `BandSlot::Link` 仍仅 URI |
| 相关 | [`提案书.md`](提案书.md) §12.4、§8 外链 URI；`canvas::LinkTarget`；`spec::{Block, InlineRun, BandSlot}`；`layout` 分页片段；`backend/krilla` 链接注解与 `Document::set_outline` |
| 调用方 | loco-rs `pdfcraft-docs` 对 `InlineRun::Link` 使用 `{ .. }` 通配；字段改名预期可编译。本变更不改 loco-rs / hcsw |
| 作废 | 设计稿「v1 没有文档内部跳转 / 不调用 `set_outline`」——以提案书与本文为准 |

本文是实现合同。按 Feature / poteto playbook 落地；不重开产品讨论。实现中若发现与 krilla 0.8.2 / Canvas / 分页事实冲突，先停下来改提案或本文。

## 1. 产品一句话

正文用 `Block::Anchor` 挂 id（可选书签项）；段落里 `InlineRun::Link` 的目标是外链 URI 或文内锚点 id。跳转落在该锚点 `child` 第一次落页片段的左上角（XYZ，不改缩放）。书签树由带 `outline` 的锚点按文档顺序与 `level` 生成；没有任何 outline 时不写树。

## 2. 范围

### 2.1 做（本主题）

- 公共 `Block::Anchor { id, outline, child }`、`OutlineEntry { title, level }`。
- `LinkTarget` 从 URI newtype 改为枚举：`Uri` / `Anchor` / 布局写入的 `Xyz`（见 §3.1）。
- `InlineRun::Link { target: LinkTarget, runs }`（字段由 `uri: String` 迁到 `target`）。
- 锚点登记、重复表头取第一次、无片段失败、悬空 / 空 / 重复 id 校验（带 `Location`）。
- 书签树：`level` 规则、空 title 失败、无 outline 不调用 `set_outline`。
- `RecordingCanvas` / emit / krilla：URI → `LinkAction`；已解析内部目标 → `Target::Destination(Destination::Xyz(..))`，zoom `None`；有书签时 `Document::set_outline`。
- 测试、`CHANGELOG`、`guide.md` / README 链接与书签说明。

### 2.2 不做

- TOC / `tocItem` / 页码回填目录。
- 按页码跳转（无 `LinkTarget::Page` / 调用方构造页号 API）。
- 行内锚点（锚点只做块级 `Block::Anchor`）。
- 仅图可点、图片专用 link 字段（本主题不扩 `ImageSpec`）。
- `BandSlot::Link` 文内锚点（页眉页脚仍只拼 URI）。
- 命名目的地树（不用 krilla `NamedDestination` 把调用方 id 写入 PDF 名树）。
- `OutlineEntry` 增加 `open` 字段或其它外观定制。
- 改 `pdfcraft-post::merge_pdfs`、forms OTF、Watermark、章跟流走、某页关 header/footer。
- 改 loco-rs / hcsw；不擅自 `git commit` / `push`。

## 3. 公共 API

### 3.1 `LinkTarget`

```rust
/// Hyperlink target. Callers construct [`Uri`](Self::Uri) or [`Anchor`](Self::Anchor).
/// [`Xyz`](Self::Xyz) is written only by layout after resolving an anchor id.
#[non_exhaustive]
pub enum LinkTarget {
    /// External URI (same rules as §8 / former newtype).
    Uri(String),
    /// In-document anchor id (must match some `Block::Anchor.id`).
    Anchor(String),
    /// Resolved page destination. Not a caller-facing “jump to page N” API.
    Xyz(LinkXyz),
}

/// Page-local XYZ point in pdfcraft page space (origin top-left, y down).
/// Fields are crate-private so external crates cannot construct page jumps.
pub struct LinkXyz { /* page_index: usize, point: Point */ }

impl LinkXyz {
    pub(crate) fn new(page_index: usize, point: Point) -> Self { /* … */ }
    pub fn page_index(&self) -> usize { /* … */ }
    pub fn point(&self) -> Point { /* … */ }
}

impl LinkTarget {
    /// Non-empty URI. Empty → `ErrorKind::EmptyLink`. Spaces kept.
    pub fn uri(uri: &str) -> Result<Self, Error> { /* … */ }

    /// Non-empty anchor id. Empty → `ErrorKind::EmptyAnchorId`.
    /// Spaces kept (same emptiness rule as URI).
    pub fn anchor(id: &str) -> Result<Self, Error> { /* … */ }

    pub(crate) fn xyz(page_index: usize, point: Point) -> Self {
        Self::Xyz(LinkXyz::new(page_index, point))
    }
}
```

| 项 | 约定 |
| --- | --- |
| 空串 | `Uri("")` → `EmptyLink`；`Anchor("")` → `EmptyAnchorId`；`" "` / `"\t"` **合法**（与现 URI 一致，不 trim） |
| `as_str` | **删除**（或不再作为唯一目标表示）。需要 URI 文本时 match `Uri(s)` 或提供 `uri_str() -> Option<&str>` |
| Breaking | 原 `struct LinkTarget(String)` 与 `as_str()` 调用方必须改；穷尽 match 须处理新变体（`#[non_exhaustive]` 下外部宜用 `_` 或显式三臂） |
| 不做 | 公开「按页码」构造器 |

### 3.2 `InlineRun::Link`

```rust
pub enum InlineRun<K> {
    Text { text, font, style },
    Link {
        /// `Uri` or `Anchor` at the call site. Layout may replace `Anchor` with `Xyz` before paint.
        target: LinkTarget,
        runs: Vec<Self>,
    },
    Blank(BlankSpec),
}
```

- §8：库只写入 URI 字符串，不访问、不校验 scheme；空 URI → `EmptyLink`。
- `BandSlot::Link` **不变**：仍是 `uri: Vec<TextRun>` + 可见 `text`；绘制时拼成 URI，走 `LinkTarget::uri`。

### 3.3 `Block::Anchor` 与 `OutlineEntry`

```rust
pub enum Block<K> {
    // …existing variants…
    /// Marks a destination for in-document links and optional outline entries.
    Anchor {
        /// Document-unique, non-empty id (byte-non-empty; spaces allowed).
        id: String,
        /// When `Some`, contributes one outline item in document order.
        outline: Option<OutlineEntry>,
        /// Wrapped flow content. Destination is the first placed fragment of this child.
        child: Box<Self>,
    },
}

pub struct OutlineEntry {
    /// Non-empty bookmark title (byte-non-empty; spaces allowed).
    pub title: String,
    /// Outline depth. First outline item in the document must be `1`.
    /// Each following item may be at most one deeper than the previous (`level <= prev + 1`),
    /// and may jump up by any number of levels (e.g. `3` → `1`).
    pub level: u8,
}
```

- `child` 用 `Box` 打断递归枚举大小。
- 正文内任何可放 `Block` 的位置都可包 `Anchor`（含表单元格、列表项、栏内、`KeepTogether` / `Styled` 内等）。
- 允许嵌套 `Anchor`；每个 id 仍须全文唯一。
- 未被任何 `LinkTarget::Anchor` 引用的锚点允许存在（可只供书签）。

### 3.4 调用示例

```rust
body: vec![
    Block::Anchor {
        id: "sec-pay".into(),
        outline: Some(OutlineEntry {
            title: "付款".into(),
            level: 1,
        }),
        child: Box::new(Block::Paragraph(ParagraphSpec {
            runs: vec![InlineRun::Text {
                text: "付款条款".into(),
                font: MyFont::Song,
                style: RunStyle::default(),
            }],
            ..ParagraphSpec::default()
        })),
    },
    Block::Paragraph(ParagraphSpec {
        runs: vec![
            InlineRun::Text { text: "见".into(), font: MyFont::Song, style: RunStyle::default() },
            InlineRun::Link {
                target: LinkTarget::anchor("sec-pay").unwrap(),
                runs: vec![InlineRun::Text {
                    text: "付款".into(),
                    font: MyFont::Song,
                    style: RunStyle::default(),
                }],
            },
            InlineRun::Link {
                target: LinkTarget::uri("https://example.test/pay").unwrap(),
                runs: vec![InlineRun::Text {
                    text: "网办".into(),
                    font: MyFont::Song,
                    style: RunStyle::default(),
                }],
            },
        ],
        ..ParagraphSpec::default()
    }),
],
```

### 3.5 Breaking 摘要

| 面 | 变化 |
| --- | --- |
| `LinkTarget` | newtype → `#[non_exhaustive]` 枚举；删/改 `as_str` |
| `InlineRun::Link` | `uri: String` → `target: LinkTarget` |
| `Block` | 新变体 `Anchor`（穷尽 match 须补臂） |
| 新增 | `OutlineEntry`、`LinkXyz`、若干 `ErrorKind` |
| loco-rs | `InlineRun::Link { .. }` 通配预期仍过；若有绑 `uri` 字段名则需改 |

## 4. 算法与接线

### 4.1 锚点目标

1. `layout_spec` / 分页过程中，每个 `Block::Anchor` 在其 `child` **第一次真正落页**的 `PlacedFragment` 上登记：
   - `page_index`：该页在文档中的 0-based 序号（与 krilla `XyzDestination::new` 的 `page_index` 一致）；
   - `point`：该片段在**页坐标**下的原点（左上；y 向下）——即 `PlacedFragment.origin`。
2. 写成 PDF 时使用 krilla `XyzDestination::new(page_index, point)`，zoom 参数为 `None`（不改变阅读器缩放）。
3. **重复表头**：同一锚点随表头重复出现多次时，**只保留第一次**登记的页+点。
4. **跨页拆分**：`child` 首片落在第 N 页，则目标为该头片原点（不是整块逻辑起点的「尚未放置」状态）。
5. **无有效片段 → 失败**（`AnchorWithoutFragment`），包括但不限于：
   - `child` 为空容器（空 `Styled` / 空 `KeepTogether` / 空 `Stack` 等最终未落页）；
   - 仅 `PageBreak`（或等价地从未产生内容片段）；
   - 零高且 `Fragment::empty()`（例如当前实现下**无行**的空段落、`Spacer(0)`）；
   - 其它最终没有任何「落页片段」的 child。
6. **有高度、无画令的片段仍算有效**：例如 `Spacer(>0)` 的首部落页片——跳转到该空白带顶。

### 4.2 链接解析顺序

1. **Lower / 校验入口**：构造或降低 `LinkTarget::uri` / `anchor` 时拒绝空串；`BandSlot` 拼出的 URI 仍走 `EmptyLink`。
2. **分页完成后**得到完整锚点表 `id → LinkXyz`。
3. **绘制正文链接前**（或等价的 fragment 物化阶段）：将 `FragmentCommand::Link` / 活跃链接上的 `LinkTarget::Anchor(id)` **替换为** `LinkTarget::Xyz(..)`；找不到 id → `UnknownAnchor`（`Location` 落在该链接）。
4. `annotate_link(rect, target)` 收到的已是 `Uri` 或 `Xyz`（不再带未解析 `Anchor`）。
5. **emit（krilla）**：
   - `Uri(s)` → `Target::Action(LinkAction::new(s))`（现状）；
   - `Xyz(xyz)` → `Target::Destination(Destination::Xyz(XyzDestination::new(xyz.page_index(), krilla_point(xyz.point()))))`；
   - 坐标映射与现有几何一致：pdfcraft 页左上、y 向下 → 适配器恒等送入 krilla（krilla 内部再翻到 PDF 空间）；**不要**在适配器里对 XYZ 点再预翻转一次。
6. **不用** `NamedDestination` / `register_named_destination`：调用方 id 只活在布局表里，不进 PDF 名树。

### 4.3 书签树

1. 按文档顺序收集所有 `outline: Some(entry)` 的锚点（与锚点首次落页成功绑定；无片段的锚点已失败，不会进树）。
2. 若收集结果为空：**不**调用 `Document::set_outline`，不写 outline 字典。
3. 校验（失败带该 `Block::Anchor` 的 `Location`）：
   - `title` 字节空 → `EmptyOutlineTitle`；
   - 第一项 `level != 1` → `InvalidOutlineLevel`；
   - 任一项 `level == 0` → `InvalidOutlineLevel`；
   - 任一项 `level > prev_level + 1` → `InvalidOutlineLevel`；
   - `level < prev_level`（含一次上跳多层，如 3→1）**合法**。
4. 由扁平 `(title, level, destination)` 建树：
   - 维护栈：当前路径上的节点；
   - `level == prev + 1`：新节点挂为栈顶之子；
   - `level <= prev`：弹出至深度 `level - 1`，新节点作为该层兄弟。
5. krilla：`Outline` + `OutlineNode::new(title, xyz)`；**不**调用 `with_open(true)`（默认折叠，`open=false`）。
6. 每个书签项的目的地与该锚点的链接目标相同（首次片段左上 XYZ）。

### 4.4 `RecordingCanvas` / Canvas

- `CanvasCommand::AnnotateLink { rect, target: LinkTarget }` 继续存在；测试可对 `Uri` 与 `Xyz` 分别断言。
- 布局测优先：`RecordingCanvas` 上锚点链接已是 `Xyz`，核对 `page_index` 与点坐标（`pt_near`）。
- 不在 `RecordingCanvas` 上模拟完整 outline 树，除非后续加专用命令；书签行为以 `to_pdf` + 解析 PDF outline / 适配器单元测覆盖（见 §5.3）。

### 4.5 模块边界

| 层 | 职责 |
| --- | --- |
| `spec` | `Block::Anchor`、`OutlineEntry`、`InlineRun::Link { target }`；lower 时登记待解析锚点 / 链接；**禁止** → `backend` |
| `layout` | 分页时写锚点表；物化链接为 `Xyz`；outline 扁平列表 |
| `canvas` | `LinkTarget` / `LinkXyz` 类型；`annotate_link`；Recording 记录 |
| `backend` | URI / XYZ 注解；`set_outline`；krilla 类型关在适配器内 |

不新增依赖。`pdfcraft-post` 不改。

## 5. 错误面

| `ErrorKind` | 何时 | `Location` |
| --- | --- | --- |
| `EmptyLink` | URI 空串（正文或 band） | 链接处；band 保持现有 Header/Footer 区域 |
| `EmptyAnchorId` | `Block::Anchor.id` 或 `LinkTarget::anchor("")` 为空 | 该 Anchor 或该链接 |
| `DuplicateAnchorId` | 同一 `id` 第二次出现 | **后出现**的 Anchor |
| `AnchorWithoutFragment` | child 无落页片段 | 该 Anchor |
| `UnknownAnchor` | `LinkTarget::Anchor` 在表中无成功登记 | 该链接 |
| `EmptyOutlineTitle` | `outline.title` 空串 | 该 Anchor |
| `InvalidOutlineLevel` | 首项 ≠1、`level==0`、或加深超过 +1 | 该 Anchor |

- 未引用锚点：不报错。
- `EmptyLink` 文案保持「link URI is empty」；新码各自写清 Display。
- 依赖 / 后端失败仍经 `source()`；XYZ 页码越界属实现 bug（krilla 会 panic）——布局保证只引用已封闭页。

## 6. 实现约束

### 6.1 推荐改动落点

1. **`canvas.rs`**：`LinkTarget` 枚举 + `LinkXyz`；更新文档与单测。
2. **`error.rs`**：六个新 `ErrorKind` + Display。
3. **`spec/mod.rs`**：`Block::Anchor`、`OutlineEntry`；`InlineRun::Link` 字段迁移。
4. **`spec/lower.rs`**：降低 Anchor / 新 Link 形状；收集 outline 与锚点 id；band 仍只 URI。
5. **`layout/flow` + paginate`**：Anchor 包装节点；首次 `PlacedFragment` 登记；表头重复忽略第二次。
6. **绘制路径**：Anchor 链接 → `Xyz` 后 `annotate_link`。
7. **`backend/emit.rs`**：分支 URI vs XYZ；`Document::set_outline`。
8. 更新所有 `InlineRun::Link { uri, .. }` / `LinkTarget::uri` / `as_str` 测试与示例。

### 6.2 测试要求

优先 `RecordingCanvas`；`to_pdf` 冒烟 `%PDF`，必要时用未压缩 / pretty 设置观察注解与 outline（与现 backend 测风格一致），不断言无关压缩流细节。

至少覆盖：

- 外链 URI 行为不回退（空 URI、跨行矩形）。
- `LinkTarget::Anchor` → 记录为 `Xyz`，页码与左上点正确（可用短文+已知 margin）。
- 重复表头内锚点：两次绘制，目标仍是第一次。
- `AnchorWithoutFragment`：空 child / 仅 PageBreak / 零高 spacer。
- `DuplicateAnchorId`、`UnknownAnchor`、`EmptyAnchorId`、`EmptyOutlineTitle`、`InvalidOutlineLevel`（首项≠1、跳级加深）。
- level 上跳 3→1 建树成功；无任何 outline 时 PDF 无 outline。
- `BandSlot::Link` 仍仅 URI；空 band URI 仍 `EmptyLink` + Header Location。
- `Spacer(>0)` 作 child 的锚点可链到（目标在空白带顶）。
- 穷尽 match / 示例：`hello` / `bill` 等若匹配 `Block` / `LinkTarget` 则补臂。

### 6.3 文档与门禁

- `CHANGELOG.md`：breaking（`LinkTarget`、`InlineRun::Link`）+ 锚点 / 书签行为。
- `crates/pdfcraft-pdf/docs/guide.md`、crate README：链接一节写清 URI vs 锚点；书签与 `OutlineEntry.level` 规则；点明 band 仅 URI。
- 设计稿「无内部跳转 / 无 outline」加一句「已由提案书 §12.4 取代」可另 PR；本主题不强制大段改设计稿。
- 提交前跑 AGENTS.md 门禁（至少 `pdfcraft-pdf` 的 module check / fmt / clippy / test / doc）。

## 7. PR 切分建议

单主题可一 PR。若 diff 过大：

1. `LinkTarget` 枚举 + `InlineRun::Link { target }` + URI 路径回归（尚无 Anchor 行为）。
2. `Block::Anchor` + 登记 / 解析 / `Xyz` 注解 + 错误码 + 测试。
3. 书签树 + `set_outline` + 测试 / changelog / guide。

不要把 forms OTF、post Watermark、搁置项塞进本主题。

## 8. 验收清单

- [ ] 提案书 §12.4 / §8 与本文一致；未超出 §2.2「不做」
- [ ] `Block::Anchor`、`OutlineEntry`、`LinkTarget::{Uri,Anchor,Xyz}` 已按 §3 落地
- [ ] 锚点目标 = 首次落页片段左上 XYZ，zoom 不变；表头重复取第一次
- [ ] 无片段 / 空 id / 重复 id / 悬空链接 / outline 规则失败且 Location 符合 §5
- [ ] 无 outline 不写树；有 outline 时 level 规则与默认折叠正确
- [ ] `BandSlot::Link` 仍仅 URI
- [ ] RecordingCanvas / emit / krilla 接线符合 §4；未用 NamedDestination
- [ ] §6.2 测试覆盖；门禁通过
- [ ] CHANGELOG + guide/README 已更新

## 9. 本会话已钉的产品细则（相对提案原文的可执行化）

提案原文未写死、经本会话确认后写入上文的：

| 点 | 决定 |
| --- | --- |
| `LinkTarget` 形状 | `#[non_exhaustive] enum { Uri, Anchor, Xyz(LinkXyz) }`；公开构造器仅 `uri` / `anchor` |
| `InlineRun::Link` | 字段 `target: LinkTarget`（取代 `uri: String`） |
| `Block::Anchor.child` | `Box<Block<K>>` |
| `OutlineEntry.level` | `u8`；可上跳多层；加深最多 +1；首项必须为 1；禁止 0 |
| 空 id / 空 title | 仅 `""` 失败；空白字符保留（对齐 `EmptyLink`） |
| 首片段 | 分页后首次落页的 `PlacedFragment`；原点即页坐标左上目标点 |
| 无片段 | 空容器 / 仅 PageBreak / 零高 empty → `AnchorWithoutFragment`；`Spacer(>0)` 有效 |
| Canvas 接线 | 绘制前解析为 `Xyz`；emit 写 `Destination::Xyz`；不用 NamedDestination |
| 书签展开 | 全部默认折叠（不调 `with_open(true)`；不设 `open` 字段） |
| ErrorKind | `EmptyAnchorId` / `DuplicateAnchorId` / `UnknownAnchor` / `AnchorWithoutFragment` / `EmptyOutlineTitle` / `InvalidOutlineLevel`；`EmptyLink` 仍专指空 URI |
| Location | 重复 id → 后出现者；悬空 → 链接处；outline / 无片段 / 空 id → 该 Anchor |
| Band | 仍仅 URI |

## 10. 权威与冲突

1. [`提案书.md`](提案书.md) §12.4（及 §8 URI 规则；§12 总序 12.1→12.2→12.3→12.4）
2. 本文
3. [`AGENTS.md`](../AGENTS.md)（模块 DAG、错误面、禁止项）
4. 设计稿未确认或已作废部分（含「v1 无内部跳转 / 无 outline」）

冲突时按上序。实现中若发现提案与 krilla 0.8.2 / Canvas / 分页事实冲突，停下来改提案或本文，不要 silently 歪实现。

## 11. 下一次实现会话提示词（`/poteto-mode`）

复制到新会话即可。**勿重开产品讨论**；合同以本文为准。

```text
/poteto-mode

## Playbook
feature（pstack poteto-mode / playbooks/feature.md）

## Goal
按已定稿合同实现 pdfcraft-pdf §12.4：Block::Anchor、LinkTarget::{Uri,Anchor,Xyz}、InlineRun::Link { target }、书签树与校验。

## Authority (in order)
1. docs/提案书.md §12.4（及 §8 URI；§12 顺序）
2. docs/pdfcraft-pdf-anchors-spec.md  ← 可执行合同；勿重开产品取舍
3. AGENTS.md（DAG、门禁、禁止项）
4. docs/pdfcraft-design.md 仅作 krilla/Canvas 技术事实；冲突以提案与合同为准

## Done when
- 合同 §3–§5 与 §8 验收清单全部满足
- RecordingCanvas / 集成测覆盖合同 §6.2
- CHANGELOG + guide/README 已更新
- AGENTS.md pdfcraft-pdf 门禁通过：
  python3 scripts/check_pdfcraft_pdf_modules.py
  cargo fmt --all -- --check
  cargo clippy -p pdfcraft-pdf --all-targets --features test-utils -- -D warnings
  cargo test -p pdfcraft-pdf --features test-utils
  cargo doc -p pdfcraft-pdf --no-deps

## Hard constraints
- 不重开 §12.4 产品讨论；发现与 krilla/分页事实冲突则停写并改提案或 anchors-spec
- 不做 TOC / 按页码跳转 / 行内锚点 / 仅图可点 / Band 文内锚点 / NamedDestination
- 不改 loco-rs / hcsw；不改 pdfcraft-post merge；不擅自 git commit / push

## Out of scope
- forms OTF、pdfcraft-post Watermark、章跟流走、某页关 header/footer、Justify/Latin 回归以外的无关重构

## Report when finished
- 改动摘要、测试证据、与合同验收清单对照、已知风险
```
