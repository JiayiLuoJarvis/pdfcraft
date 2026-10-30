# pdfcraft-orgchart 结构修复规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-13 |
| 状态 | **修复计划**。行为不另立合同。本文确认后按此改代码 |
| 相关 | [`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md)（可观察行为的唯一权威）；[`workspace-crates-spec.md`](workspace-crates-spec.md)；[`AGENTS.md`](../AGENTS.md) |
| 范围 | 公共面、所有权、标识、字体管线、分页会话、测试放置。不改产品规则 |

0.x 初期，无调用方包袱。结构一次收敛，不留 `#[doc(hidden)]`、弃用别名、双键（字符串 id 与 `NodeId` 并存）或「先能编译再删」的过渡 API。

## 0. 原则

业务逻辑保留。Rust 不专业的部分允许大改，并且应当大改。

| 保留 | 大改 |
| --- | --- |
| 行为规格里的协议、错误码字符串、折叠、框内行、页眉页脚、页角色、常量、评分顺序、驱逐顺序、朝向与舒适带/弹性带 | 模块可见性、标识、容器、缓存、借用、字体所有权、测试所在的 crate 边界 |
| 现有成功夹具的可观察结果：错误码、中文文案、页数、页角色、在**同一测宽器**下的分页结构 | 把「Python 字典 + 类上的 `self.cache`」收成索引、arena、`&mut self` |
| 家族搜索、走线、开槽的**控制流与比较顺序** | 这些函数的参数从 `String` 换成 `Copy` 的 id；记忆化从深拷贝方案换成 arena 下标 |

行为规格 §0 已经要求惯用 Rust。当前实现在 `graph` 做到了，从 `paging` 起又退回字符串字典。本文把那一段补完。补完时不借机改启发式。

两份文档分工：

- 可观察行为以行为规格为准。本文不重写那份合同，也不新增产品能力。
- 代码形状以本文为准。行为规格里的「建议模块名」不是必须保留的文件名。

## 1. 什么算改了业务逻辑

下面任一变化都算改业务，本计划禁止：

- 增删错误码，或改变 `ErrorCode::as_str` 的字符串。
- 改变协议拒绝条件：空白 id、重复、悬空边、自环、环、双锥、跨锥、比例范围、`max_nodes` / `top_n` / 边距 / 节点宽、`__overflow__:` 前缀、`__other__:` 冲突。
- 改变 `top_n` 的折叠对象、排序（比例降序、同比例按 `nodes` 序）、合成名、幂等、投资侧不动、`validate_request` 不把折叠后的节点数算进上限、`generate_pdf` 再校验一次。
- 改变框内行顺序、空白字段省略、比例不进框、公司灰底条件、比例文案 `xx.xx%`、贴边位置。
- 改变页眉、副标题、页脚（含「頁」）、续页虚框、overflow 第二行、`其他股东(k家)` / `其他投资(k家)`。
- 改变行为规格 §12 的常量数值，或 `node_height_limit_mm` 的公式。
- 改变分页偏好：分叉页先横版、纯链页先竖版、先舒适带再弹性带、整图一页的弹性条件、直接层可折行、更深层单行、大族留 stub、overflow 驱逐顺序（人 → 叶子公司 → 小族 → 大族，同类输入序靠后的先走）、至少留一个真实直接孩子、续页 ContCue 的优先方位与撑行距顺序。
- 改变 `PlanScore` / `OverflowScore` 的字段比较顺序。当前顺序是合同的一部分：方向违反、稀疏风险、切开数、重复上下文、页数、输入序；overflow 另有降级数、利用率、前排留下数。
- 改变「搜索预算用尽时停下，返回当时的最优方案」这个控制流。8192 次上限保持。调用方今天不读 `expansions` / `budget_exhausted`，删掉这两个未读字段可以；把用尽改成 `LAYOUT_FAILED` 不行，那会让原本出图的请求变成失败。
- 把算法里的 `continue` 改成错误。放不下的内联、已访问的状态、弹性超高的候选，都是搜索本身。`compute_hops` 里主体走不到的节点名次为 0，是现有规则，保持数值，写成显式分支，不要改成失败。
- 用 krilla `Surface::draw_text` 或 `simple-text` 量宽、画字。
- 为了让「当前误用 2 mm/字打出来的 PDF」保持字节不变，而把生产路径留在假测宽器上。那是实现违背行为规格 §9，不是要保护的业务。

允许改变、而且应当改变的输出只有一类：`generate_pdf` 在 overflow 框、续页虚框、附页升格重折行上，改为使用已经打开的那份 skrifa 度量。单页、且分页阶段不再测字的图，文本与页结构保持现状。多页图的折行、框高、是否切开可以与**今天的错误实现**不同，但必须与行为规格的测字合同相同。不要回调常量去迁就 2 mm/字。

缺框、缺键今天有的路径会 `continue` 或 `Index` panic。这些不是产品规则。重写之后用类型让「有节点必有框」成立，成功路径的 PDF 不变。不要为了「更严格」把现有成功夹具改成 `LAYOUT_FAILED`。若某条夹具只有靠静默丢节点才能出 PDF，停下来标出这条夹具，单独决定；默认不保留静默丢弃。

## 2. 目标公共面

crate 根只导出行为规格 §3 已确认的项：

```rust
pub use error::{Error, ErrorCode};
pub use model::{
    document_title, ChartOptions, ChartRequest, Edge, FontBytes, Node, NodeType,
    MAX_MARGIN_MM, MAX_NODES, MAX_NODE_WIDTH_MM, MIN_NODE_WIDTH_MM,
};

pub fn validate_request(request: &ChartRequest) -> Result<(), Error>;
pub fn generate_pdf(request: &ChartRequest, font: &FontBytes) -> Result<Vec<u8>, Error>;
```

`document_title`、`Node::new`、`Edge::new`、`FontBytes::new`、`ChartRequest::new`、`NodeType::as_str`、`ErrorCode::as_str` 保持。`Error` 仍是公开字段 `code` + `message`，`Display` 仍打印 `message`。

从 crate 根删除，不提供兼容导出：

| 现状 | 处理 |
| --- | --- |
| `pub mod captions, display, layout, measure, page, paging, units` 加 `#[doc(hidden)]` | 改为 `pub(crate)` 模块，或按 §4 合并后仍是 `pub(crate)` |
| `LaidOutNode`（`id: &'static str`，crate 内无引用） | 删除 |
| `DisplayRole` / `LineKind` / `LabelHug` 的 `as_str` | 删除。测试改匹配枚举。这三个类型不进公共面 |
| `paginate`、`paginate_with`、`LayoutEngine`、`Measurer`、`FallbackMeasurer`、`PageGeometry`、`PageSpec`、`NodeBox` | `pub(crate)`。测试在 crate 内部使用 |
| 仅测试使用的 `default_node_width_mm`、`placed_ids`、`build_display` 等 | 随测试迁入，能 `pub(crate)` 就 `pub(crate)` |

`NodeType::as_str` 保留，因为行为规格 §14 把 `PERSON` / `COMPANY` 定为对外记号。没有解析入口，也不要新增 `FromStr`。

示例 `examples/equity_chart.rs` 只使用上述公共入口。

## 3. 目标模块与依赖

文件名可以合并，依赖方向不可反。不依赖 `pdfcraft-pdf`、`pdfcraft-forms`。

```text
model, error, units          无内部业务依赖
graph                        → model, error
prepare                      → graph
caption                      → model          纯字符串。含现在的 captions.rs
display                      → graph, caption
measure                      → display, units, error, model
page                         → measure, units, caption   页模型，pub(crate)
layout                       → page, measure, units, error
family                       → page, error    只搜索「哪些节点同页」，不放置坐标
paging                       → family, layout, measure, graph, display, caption, page
render                       → page, measure, units
lib                          → 上述模块，只为三个公共函数接线
```

禁止：`layout` → `paging`，`family` → `paging`，`measure` → `paging`，`render` → `layout`，任何模块 → `backend`（本 crate 没有这一层，krilla 只出现在 `render`）。

`caption` 与 `display` 可以仍叫 `captions.rs` / `display.rs`。不要为了对照 Python 保留 `measuring.py` 式的对外模块。

## 4. 标识与 arena

校验通过后，内部不再把 `String` 当节点键。

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct NodeId(u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct EdgeId(u32);
```

`NodeId` / `EdgeId` 只由 arena 分配。不提供从裸整数公开构造的方法。调用方的 `String` id 只活在 `ChartRequest` 和错误文案里。

```rust
pub(crate) enum NodeOrigin {
    /// 请求里的节点，含调用方自己写下的 `__other__:` id。
    Input,
    /// `top_n` 折叠生成的股东占位。id 文本仍是 `__other__:{parent}`。
    FoldedHolders { parent: NodeId },
    /// 分页合成。id 文本仍是 `__overflow__:{subject}:{first_moved}`。
    Overflow {
        subject: NodeId,
        first_moved: NodeId,
        moved_count: usize,
    },
}

pub(crate) struct NodeRec {
    pub(crate) id: NodeId,
    pub(crate) origin: NodeOrigin,
    pub(crate) protocol_id: String,
    pub(crate) name: String,
    // 其余展示字段从请求搬来，折叠节点按现在的配方填
}

pub(crate) struct Graph {
    pub(crate) nodes: Vec<NodeRec>,
    pub(crate) edges: Vec<EdgeRec>,
    by_protocol_id: HashMap<String, NodeId>,
    outgoing: Vec<Vec<EdgeId>>,
    incoming: Vec<Vec<EdgeId>>,
    pub(crate) subject: NodeId,
}
```

规则：

- `validate` 的产物就是 `Graph`。邻接表用 `EdgeId`，与现在的 `ValidatedGraph` 一样按下标走，键改成新类型。
- `HashMap<String, NodeId>` 只用于：协议校验、`document_title` 不再经过它（标题仍按请求线性查找，行为不变）、折叠时识别已有 `__other__:`、错误信息里打印调用方 id。布局、走线、分页、绘制不接收 `String` 键。
- 用户输入的 `__overflow__:` 仍在校验时拒绝。用户输入的 `__other__:` 仍允许，来源是 `NodeOrigin::Input`。折叠生成的占位才是 `FoldedHolders`。不要用「字符串前缀」在布局里区分这三件事；绘制虚线、禁止作 ContCue 锚点，看 `NodeOrigin::Overflow`。
- 分页要加 overflow 节点时，向 `nodes` **追加** `NodeRec`，分配新 `NodeId`。不另建一张 `HashMap<String, NodeBox>`。
- 页上的节点集合是 `BTreeSet<NodeId>`。边是 `EdgeId` 或 `{ from, to: NodeId }`。`BTreeSet` 保留现在的确定顺序；元素变 `Copy` 之后，克隆集合不再克隆字符串。
- 节点上限 100，加上折叠与 overflow 仍是很小的图。用 `BTreeSet<NodeId>` 即可，不必上自定义位集。位集如果不能在不改搜索顺序的前提下替换比较结果，就不要做。

`prepare` 仍返回折叠后的 `ChartRequest`，因为 `validate_request` 的合同是「折叠错误能报出，结果丢弃」，`generate_pdf` 的合同是「再 `validate` 一次才套 `max_nodes`」。这次克隆发生在协议边界，保留。`generate_pdf` 拿到第二次 `validate` 的 `Graph` 之后，不再把 id 拷回 `Vec<String>`。

展示行、比例事实、节点框挂在 `NodeId` 上，用 `Vec` 按下标索引。`NodeDisplays::get(&str)` 这类按协议 id 查找的公开方法随公共面一起删除；crate 内直接用 `NodeId`。

## 5. 家族搜索：控制流不动，键和缓存改掉

`family` 仍负责「哪些节点同页」。现在的阶段保留，名字可以改，顺序不能改：

1. `solve_task`：有链则 `solve_chain`，否则 `plan_head` + `enumerate_inlines`，与贪心方案比 `PlanScore`。
2. `plan_head` / `measure_direct_cuts`：按驱逐等级和输入序生成切法，`legal_direct_cuts` 先取舒适解。
3. `greedy_plan`：按单位尝试内联，放得进弹性带才留下。
4. `enumerate_inlines`：子集扩展，预算用尽则 `break`，返回已经收集的页。
5. `solve_chain`：沿链加长，放不下则 `break`，保留当前最优（起点是贪心方案）。
6. `score` / `OverflowScore`：字段与 `Ord` 顺序见 §1。

删除未读的 `SearchResult.expansions` 与 `budget_exhausted`。预算用尽仍是 `break`，不是新错误。`PlacedChild` 若仍表达「stub / 叶子 / 内联」，保留枚举；去掉无用的 `#[allow(dead_code)]`。没有读者的字段删掉，不保留给「以后调试」。

记忆化：

- 键是 `NodeId`、`EdgeId`、`TaskKind`、`BTreeSet<NodeId>`，不是 `String`。
- 方案正文放进 `Vec<SearchPlan>`。memo 的值是下标。返回给调用方时可以克隆一份由 `Copy` id 组成的方案；禁止再为命中缓存而克隆整棵带字符串的页树。
- `layout_cache` / `direct_cache` 同样以页草稿的 id 集合为键，值可以克隆，因为 `LayoutEvaluation` 是几个 `f64` 加一个 `PageGeometry`。

搜索与放置的接口：

```rust
pub(crate) trait PageOracle {
    fn evaluate(&mut self, draft: &PageDraft) -> Result<LayoutEvaluation, Error>;
    fn evaluate_direct(&mut self, draft: &PageDraft) -> Result<DirectFit, Error>;
}
```

`search_family_pages` 改为泛型 `O: PageOracle`，删除 `&mut dyn FnMut`。`paging` 的会话实现这个 trait。泛型是为了让借用检查器看见独占的 `&mut`，不是为了抽象给调用方。

`PageDraft`、`AttachmentTask`、`FamilyEdge`、`OverflowNode` 的身份字段全部改成 `NodeId`。协议 id 字符串只在需要格式化 `__overflow__:` / 错误信息时向 `Graph` 要。

## 6. 分页会话

`Session` 独占数据，方法是 `&mut self`。删除其上全部 `RefCell`：选择缓存、朝向缓存、直接层缓存、overflow 框缓存、两侧续页计数。

这些缓存是「算过的布局」，不是共享状态。`&mut self` 表达这一点。不要为了让回调和 `self` 同时借用而把缓存放回 `RefCell`。

`shareholder_continue` / `investment_continue` 是普通 `usize`。递增发生在 `&mut self` 的标题函数里。计数规则不变：各自从 1 起，续页文案仍由现在的 `caption` 函数生成。

生产入口：

```text
prepare_request
  → validate          得到 Graph
  → display
  → FontMetrics::open 一次
  → build_node_boxes(&mut metrics)
  → paginate(&mut session)   session 持有 &mut metrics
  → 页眉、比例、页脚 ensure_glyphs（应命中字形缓存）
  → render
```

`paginate` 不再内部构造测宽器。`generate_pdf` 把同一份 `FontMetrics` 交进去。overflow 框、升格重折行、虚框尺寸都走这份度量。

固定测宽器（现在的 `FallbackMeasurer`：每字 2 mm，行高 5 mm）只作为 crate 内测试替身，给分页结构测试注入。它不出现在公共 API，`generate_pdf` 不调用它。分页测试继续用它，这样页数、标题、角色的断言与字体文件无关，业务断言保持稳定。

`clone_pack` 删除。需要克隆的布局结果 `#[derive(Clone)]`。

`layout_parent_ends` 每次新建一张可达性缓存，是重复计算，不是业务。`Graph` 上可以有一张按 `(NodeId, NodeId)` 的可达缓存，生命周期跟图一样长。查询结果必须与现在逐次新建缓存相同。

## 7. 布局与绘制

`layout` 的输入改为 `&[NodeBox]` 加 `NodeId` 边，或直接从 arena 切片取。`ranks` / `groups` / `place` / `route` 的算法步骤保留：跳数分层、家族列、行距日程、bus 错开、正交折线。内部映射是 `Vec` 或 `HashMap<NodeId, _>`，不是 `HashMap<String, _>`。

`HashMap` 的 `Index`（`ranks[id]`、`edge_by_id[edge_id]`、`kinds[left]`）在改成下标之后，越界是私有不变量。用带一句理由的 `expect`，符合仓库对已证明不变量的约定。不要改成新的错误码。主体不可达则名次 0 的那条，保持数值。

下面两处「找不到就 `continue`」在类型收紧后应消失，而不是改成调用方可见的新失败：

- `paging` 里给某一页收集框时，草稿里的 `NodeId` 没有框就跳过。
- `render` 里按 id 线性找框，找不到就不画该节点。

目标形状：放置结果里的节点带 `NodeId`，框表是 `Vec<NodeBox>` 按下标对齐，或 `PlacedNode` 直接带上绘制所需的框字段。绘制循环没有「也许没有框」的分支。现有成功页上的节点一个不少。

`cues::required_row_gap` 在锚点不在本页布局里时 `continue`，以及「挡住了但不是邻行就 `continue`」，属于虚框行距算法，保持。编号之后槽位缺失就 `continue` 的检查，若槽位按类型一定存在，删除该分支；若删除会改变「文字超宽则 `LAYOUT_FAILED`」的触发，则保留判断、去掉无意义的跳过。以分页测试的页结构为准，不单独改行距公式。

`render` 仍用 `draw_glyphs`。页原点换算保持 y 向下、页眉在上。链接矩形、圆角裁切、虚线只包 overflow 描边，规则不变。节点与框对齐之后，绘制按 `NodeId` 取框，不再对每节点扫描 `Vec`。

字形 id 与 advance 来自 §8 的同一次缓存。`center_text` 可以先拿 `GlyphRun` 再交给绘制，避免同一次绘制对同一字符串走两遍；宽度数值必须与现在的「先量后画」相同。

## 8. 字体

行为规格：拒绝 `ttcf`、缺字 `MISSING_GLYPH`、行高 = hhea ascent + descent 的绝对值比例尺 + `LINE_GAP_PT`、与绘制同一 glyph id。这些不变。

`FontBytes.data` 是共享的 `Arc<Vec<u8>>`。`FontMetrics` 存下这份 `Arc`。`render` 把这份 `Arc` 克隆进 krilla `Data`。不按页、不按字符串复制字体内容。

```rust
pub(crate) struct FontMetrics {
    bytes: Arc<Vec<u8>>,
    upem: f64,
    ascent: f64,
    descent: f64,
    glyphs: HashMap<char, GlyphUse>,
}
```

`glyph_run(&mut self, text)`：已缓存的字符直接用；本字符串里未缓存的字符，构造**一次** `FontRef` 补齐。禁止每个 `string_width_mm` 都 `FontRef::from_index`。

`Measurer` 的方法改为 `&mut self`。测试替身忽略可变借用。不要在 `FontMetrics` 上用 `RefCell` 假装共享；调用链从 `generate_pdf` 起就是独占的。

`wrap_text` 的切分规则不变：先 `ensure_glyphs`；不超宽或禁止折行则整段；ASCII（`char::is_ascii`，空格断开）成块，其余逐字。`as u32 < 128` 换成 `is_ascii()`，边界与现在相同（ASCII 空格仍单独处理）。折行结果必须与改前在同一度量下逐字相同。缓存只减少解析次数。

不要把 `FontRef` 和自有字节做成自引用结构，也不要为此写 `unsafe`。

## 9. 错误与不变量

公共错误类型不改。`LAYOUT_FAILED` 仍只用于行为规格 §6 已列出的那类失败：非法 overflow、缺锚点、bus 键非法、主体或直接层放不进页、虚框文字超宽等**现有**失败。

内部不变量用类型或 `expect`。公共路径继续不 `unwrap`。

`node_name` 缺 id 时返回空字符串：改成按 `NodeId` 取 `NodeRec::name` 之后，这条分支不存在。升格副标题的文案函数仍接收名字和比例，输出字符串与现在相同。

`cut_caption` 把 `InvalidProtocol` 当成「只写根名」是现有规则（无合法父边时的副标题）。保留这个分支的**结果**。实现上改成 `layout_parent` 返回 `Option`，不要用 `error.code == InvalidProtocol` 做控制流。有父边、无父边、协议上本就没有唯一父边，三种结果与现在的副标题字符串一致。

## 10. 几何

布局继续用 `f64` 毫米。不引入 `Mm` 新类型：它会碰到每一个常量比较，对专业度的收益抵不上行为漂移的风险。

`PageGeometry` 留在 `units`，`pub(crate)`。`cache_key` 可以改成「朝向 + 边距的 `to_bits`」，只要同一页几何仍命中同一缓存项。缓存命中与否不改变最终坐标；它只避免重复放置。

`count_units` 对正有限值向零截断，再封顶 64。钉住的结果是：默认竖版、节点宽 38 mm 为 4 列；默认横版为 6 列；原始截断值大于 64 时为 64；节距小于等于 0 时为 1。列数变化会改变装箱，属于业务。禁止放宽上限，禁止改成四舍五入。

`n64` 的饱和规则保持。

## 11. 测试怎么搬

断言的是可观察结果，不是私有函数名。搬迁时原样保留这些断言：

| 现有文件 | 去向 | 继续断言 |
| --- | --- | --- |
| `tests/validate_request.rs`、`tests/document_title.rs`、`tests/prepare.rs` | 留在 `tests/`，只靠公共 API | 错误码字符串、折叠后的 id / 名称 / 比例、幂等 |
| `tests/generate_pdf.rs` | 留在 `tests/` | `%PDF`、小图的「股权架构图」「主体公司」。`pdftotext` 启动失败则测试失败，禁止 `return` 当成成功 |
| `tests/captions.rs`、`tests/display.rs` | `src` 内 `#[cfg(test)]` | 中文字面量。`as_str()` 改为匹配枚举 |
| `tests/measuring.rs` | `src/measure.rs` 旁 | 缺字、`ttcf`、`NODE_TOO_TALL`、折行、升格重折行。度量仍是真实字体 |
| `tests/paging.rs` | `src/paging.rs` 旁 | 页数、主副标题、角色、附件号。继续注入固定测宽器，不改成思源字体，避免把算法测试和字形捆在一起 |

新增一条，锁住 §1 允许的那处行为修正：

- 用真实字体走 `generate_pdf`，图必须发生附页升格或 overflow（固定测宽器测试里已有这类形状，换成 `FontMetrics`）。
- 断言升格主体或 overflow 框的折行，与直接调用 `promote_to_page_subject` / `overflow_box`（同一 `FontMetrics`）一致。
- 不断言压缩内容流，不断言与「2 mm/字旧实现」的像素或字节相同。

`cargo test -p pdfcraft-orgchart` 在迁完后不依赖 `doc(hidden)` 模块。集成测试编译失败若来自「测试还在引用旧公开模块」，修测试的导入，不把模块重新公开。

## 12. 实施顺序

三步。每一步结束时门禁是绿的。中间不保留双键，也不把旧符号重新导出。

### 12.1 收口公共面

- 模块改为 `pub(crate)`，删 `LaidOutNode` 和内部枚举的 `as_str`。
- 按 §11 搬测试。固定测宽器留在 paging 测试里。
- `generate_pdf` 仍暂时调用现在的 `paginate`。这一步不改测宽，避免和搬测试混在同一个 diff 里难以对照。
- 验收：现有断言全绿；`cargo doc -p pdfcraft-orgchart --no-deps` 的对外页面只有 §2 的类型和三个函数。

### 12.2 字体与生产测宽

- 按 §8 改 `FontMetrics`：一份 `Arc` 字节、字形缓存、`Measurer` 使用 `&mut self`。
- `generate_pdf` 把这份度量传入分页。overflow、虚框、升格走它。
- 加上 §11 的真实字体升格/overflow 测试。
- 分页结构测试仍注入固定测宽器，断言不改。
- 验收：小图 `pdftotext` 与现在相同。多页测试在固定测宽器下与现在相同。真实字体测试证明生产路径没有使用 2 mm/字。

### 12.3 标识、会话、布局一次换键

这一步是大改，放在一个主题里做完。禁止先让 `paging` 用 `NodeId`、`layout` 仍用 `String`。

顺序只约束实现者脑子里的依赖，不作为对外过渡版本：

1. `Graph` / arena / `NodeOrigin` 落地，`prepare` 与协议测试保持通过。
2. `display` 与 `measure` 的表改为按 `NodeId` 索引。
3. `family` 改为 `PageOracle` + id 键 + 方案下标 memo。搜索顺序用现有分页测试看守。
4. `Session` 去掉 `RefCell`，实现 `PageOracle`。
5. `layout` 与 `render` 改为 `NodeId`，删除按字符串找框和静默跳过。
6. 删掉已经没有读者的 `#[allow(dead_code)]`。`units` 里若常量已被使用，去掉多余的 allow。

验收：§11 的全部旧断言，加 §12.2 的字体测试。`rg` 在 `layout`、`family`、`paging`、`render` 中不再出现 `HashMap<String`。协议错误信息里仍可以格式化调用方的 `String` id。

## 13. 门禁

与仓库现有命令相同，在仓库根执行：

```bash
cargo fmt --all -- --check
cargo clippy -p pdfcraft-orgchart --all-targets -- -D warnings
cargo test -p pdfcraft-orgchart
cargo doc -p pdfcraft-orgchart --no-deps
```

`#![forbid(unsafe_code)]` 保持。不新增依赖。`skrifa =0.48.0`、`krilla =0.8.2`、`read-fonts =0.45.0` 的钉版本不在本计划里改。

用户可见的生成结果变化只有 §1 那一处（多页图改用真实字体度量）。实现 §12.2 时记一笔 `CHANGELOG.md`。§12.1 与 §12.3 没有新的对外行为，不必记「重构」。

## 14. 本计划不做

- 不改行为规格里的产品规则，不把本文写成第二份产品合同。
- 不实现行为规格明确不做的能力：环状交叉持股、跨页连线、换纸、换词表、JSON/serde、进程级字体路径、几何截图门禁。
- 不把 `pdfcraft-orgchart` 接到 `pdfcraft-pdf`，不抽公共 geom crate。
- 不保留 Python 模块同名文件作为目标。
- 不把 `RefCell`、`dyn FnMut`、字符串键留在「以后再清」的注释里。
- 不调评分权重、行距、字号、页边，用来抵消换键或换字体缓存带来的差异。出现数值差异时，先判定它属于 §1 的「误用假测宽器」，还是换键改坏了比较顺序。后者修键，不修常量。
