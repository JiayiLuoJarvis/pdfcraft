# pdfcraft-orgchart 0.3 公共面与结构重塑规格

| 项 | 值 |
| --- | --- |
| 日期 | 2026-11-04 |
| 状态 | **决定草案**。D3 至 D5、D7 经 2026-11-04 对话选择题确认（D3、D7 按同日审查修订，`folded_holders_of` 的字段名与两条新校验同日确认）；D12、D14 于同日确认；其余在维护者"允许完全重构""按建议撰写"的授权下拟定。提案书 §13 已起草（草案），确认后本文定稿，见 §11 |
| 范围 | 只改 `crates/pdfcraft-orgchart`：公共面、内部标识、模块与文件划分、错误面，以及 PR 0 的绘制顺序修复（DET-1）。算法、文案、常量、错误码字符串不改 |
| 版本 | PR 3 合入后发 `pdfcraft-orgchart 0.3.0`（crates.io 当前 `0.2.0`）。全部 breaking 改动在 PR 3 一个 PR 内完成（VER-3） |
| 相关 | [`pdfcraft-orgchart-spec.md`](pdfcraft-orgchart-spec.md)（行为合同）；[`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md)；[`pdfcraft-pdf-shape-refactor-spec.md`](pdfcraft-pdf-shape-refactor-spec.md)（同层先例）；[`rust-engineering-standard.md`](rust-engineering-standard.md) API-2、API-8、API-9、API-11、API-12、ARC-2、ARC-3、ERR-2、ERR-3、ERR-7、PAN-3、PAN-4、PAN-8、DET-1、TRK-2、TRK-4、TST-1、VER-1 至 VER-3、TOOL-5 |
| 冲突 | 本文与行为规格、结构规格冲突时，以本文为准，旧条文在 §10 列出的 PR 里改写。与提案书冲突时以提案书为准。实现中发现与代码事实冲突，先停下来改本文，不要自行裁决 |

本文是实现合同，不重开产品讨论。行号以 2026-11-04 工作区（HEAD `152a6e9`，orgchart 源码与 `a55cd66` 相同）为准，动手前重新核对。

## 1. 产品一句话

调用方先用 `Chart::new(request)` 把请求编译成一个已校验、已折叠的 `Chart` 值，再用 `chart.to_pdf(&font)` 画成 PDF。字体用 `Font::new(bytes)` 在边界解析一次，可以跨图复用。外观参数收进可复用的 `ChartStyle`。分页结构、页眉页脚文案、框内行、错误码字符串与 `0.2.0` 相同。

## 2. 对标范式

| 层 | 范式 | 在本 crate 的落点 |
| --- | --- | --- |
| 公共面 | 先编译、再使用（`regex::Regex::new` 加 `find`）。编译不需要字体，字体相当于 haystack | `Chart::new` 只做协议校验和折叠。`to_pdf` 才测字、分页、绘制 |
| 请求形状 | 本仓库提案书 §12.7 的字段三分 | 可复用的外观进 `ChartStyle`。每次请求的策略（`top_n`、`max_nodes`）平放在 `ChartRequest` 上。描述性数据是 `Node` 和 `Edge` |
| 内部 | 编译器中间表示的纪律：降级后只在类型化 IR 上变换，不回到源格式 | 校验后只用 `NodeId` 加 `NodeOrigin`。调用方的字符串 id 只出现在请求和错误里 |
| 边界 | ARC-3：外部数据在边界一次解析 | 请求在 `Chart::new` 解析，字体在 `Font::new` 解析。内部不重复校验 |

不采用的方向，以及原因：

- **Compose 或 Flutter 的树与 `children` 命名。** 股权图有多个持股方，是有向图，不是树。
- **builder 和可插拔后端 trait。** krilla 是唯一后端。协议记录用结构体字面量构造就够了。
- **`Mm` 新类型。** 结构规格 §10 已否决。兄弟 crate 之间不共享类型。
- **用调用方的键类型 `K` 当节点 id**（仿 `FontRegistry<K>`）。调用方的 id 本来就是外部数据里的字符串。泛型会让每条错误都要求 `K: Debug`，删不掉任何一条校验。
- **请求按 Vega-Lite 拆成五个子结构。** 只有一个变换（`top_n`）。拆分只加嵌套，删不掉分支。

## 3. 已确认的决定

| # | 决定 |
| --- | --- |
| D1 | 新增 `Chart`。`Chart::new(ChartRequest) -> Result<Chart, Error>`，按值接收请求。删除 `validate_request`、`document_title`、`generate_pdf`，不留过渡（VER-3） |
| D2 | `Chart` 不持有字体。`Chart::to_pdf(&self, &Font) -> Result<Vec<u8>, Error>` |
| D3 | `Chart::new` 折叠后在新 arena 上重跑全部协议规则，包括 `max_nodes`。原来"`validate_request` 通过、`generate_pdf` 报协议错误"的分裂不再存在，见 §6.2 |
| D4 | 删除 `ChartOptions::title`。`Chart::title()` 恒定返回 `{主体名}股权架构图`。另有 `Chart::subject_name()` |
| D5 | 删除 `ChartOptions`。新增 `ChartStyle { node_width_mm: f64, margin_mm: f64 }`，`Default` 为 38 与 8 |
| D6 | `ChartRequest` 上平放 `top_n: Option<NonZeroUsize>` 与 `max_nodes: usize`（默认 `ChartRequest::MAX_NODES`） |
| D7 | 不再保留 id 前缀规则，不拒绝 `__overflow__:`，`__other__:` 不再有特殊含义。折叠占位改由 `Node::folded_holders_of: Option<String>` 声明，折叠保持幂等（行为规格 §5），见 §4.1 与 §6.2 |
| D8 | `FontBytes` 改为 `Font`。`Font::new` 与 `Font::from_shared` 当场解析字体；krilla 侧的字体或序列化失败仍在 `to_pdf` 报 `FONT_ERROR`。前提是提案书 §13 已加入 |
| D9 | `Edge { holder, held, ratio }` 取代 `from_id` 与 `to_id` |
| D10 | `Node` 的 `paid_in_amount` 与 `currency` 合成 `paid_in: Option<PaidIn>`，`PaidIn { amount: String, currency: Option<String> }` |
| D11 | 上限常量改为关联常量：`ChartRequest::MAX_NODES`、`ChartStyle::MIN_NODE_WIDTH_MM`、`ChartStyle::MAX_NODE_WIDTH_MM`、`ChartStyle::MAX_MARGIN_MM`。crate 根不再导出常量 |
| D12 | `Error` 改为私有字段加访问器 `code()`、`message()`、`subject()`、`source()`。新增 `#[non_exhaustive]` 枚举 `ErrorSubject`。`Edge` 的 `index` 只取调用方原始下标，见 §4.4 |
| D13 | 内部不变量用类型消掉（PAN-4）。类型消不掉的继续报 `LAYOUT_FAILED`。不新增生产 `expect`，延续 enterprise-readiness 工作项 E |
| D14 | API-9 新增第五类例外"调用方用字面量构造的协议记录"，点名 `Node`、`Edge`、`PaidIn`、`ChartRequest`。`ChartStyle` 归入第四类，第四类的范围从 `pdfcraft_pdf::spec` 扩到各包的 `XxxStyle`。`Chart`、`Font`、`Error` 字段私有，不需要 `non_exhaustive`。`NodeType`、`ErrorCode` 维持闭集例外 |
| D15 | 模块按领域重组，大文件按职责拆开，测试放在同目录的 `*_tests.rs`，见 §8 |
| D16 | 页面层与布局层的键全部换成 `NodeId`。`NodeId` 与 `EdgeId` 包 `usize`，去掉 `u32::MAX` 哨兵（TRK-2） |

## 4. 公共 API 终态

crate 根只导出下面这些条目，全部加 `#[doc(inline)]`。

```rust
pub use chart::Chart;
pub use error::{Error, ErrorCode, ErrorSubject};
pub use font::Font;
pub use request::{ChartRequest, ChartStyle, Edge, Node, NodeType, PaidIn};
```

### 4.1 请求

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ChartRequest {
    pub subject_id: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub style: ChartStyle,
    pub top_n: Option<NonZeroUsize>,
    pub max_nodes: usize,
}

impl ChartRequest {
    pub const MAX_NODES: usize = 100;
    /// `style` is `ChartStyle::default()`, `top_n` is `None`, `max_nodes` is `MAX_NODES`.
    pub fn new(subject_id: impl Into<String>, nodes: Vec<Node>, edges: Vec<Edge>) -> Self;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub legal_person: Option<String>,
    pub ratio: Option<f64>,
    pub node_type: Option<NodeType>,
    pub paid_in: Option<PaidIn>,
    pub location: Option<String>,
    pub approval_date: Option<String>,
    /// This node already stands in for the holders of that node id folded by `top_n`.
    pub folded_holders_of: Option<String>,
}

impl Node {
    /// Every optional field is `None`.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaidIn {
    pub amount: String,
    pub currency: Option<String>,
}

impl PaidIn {
    pub fn new(amount: impl Into<String>) -> Self;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Holds `held`.
    pub holder: String,
    pub held: String,
    pub ratio: Option<f64>,
}

impl Edge {
    pub fn new(holder: impl Into<String>, held: impl Into<String>) -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartStyle {
    pub node_width_mm: f64,
    pub margin_mm: f64,
}

impl ChartStyle {
    pub const MIN_NODE_WIDTH_MM: f64 = 20.0;
    pub const MAX_NODE_WIDTH_MM: f64 = 80.0;
    pub const MAX_MARGIN_MM: f64 = 40.0;
}

impl Default for ChartStyle { /* 38.0, 8.0 */ }
```

`NodeType` 与 `NodeType::as_str` 不变。

`PaidIn` 的展示规则不变。`amount` 去掉首尾空白后为空，就不画这一行。`currency` 为空白时按 `None` 处理。今天"只有币种、没有金额"会被静默忽略，这种状态在新类型下写不出来。

`folded_holders_of` 取代 `0.2.0` 的 `__other__:{p}` id 约定，语义与之相同：

- 值是另一个节点的 `id`，指向不存在的节点时报 `INVALID_PROTOCOL`。两个节点声明同一个值，或声明的节点没有一条指向该父节点的直接边，也报 `INVALID_PROTOCOL`。这三条是新规则；`0.2.0` 的折叠结果天然满足它们。
- 对该父节点做 `top_n` 排名时，跳过这个节点，它不占名额，也不并入新占位。
- 该父节点仍需折叠时报 `INVALID_PROTOCOL`，`subject` 为声明的节点，与今天 `prepare.rs:106-111` 的冲突相同。
- 展示、测字、分页都把它当作普通调用方节点，名称由调用方提供。它可以持股或被持股，也可以在其他父节点下参与排名；`folded_holders_of` 只影响对所指父节点的排名与冲突检查。折叠只发生在持股方一侧。

### 4.2 编译与绘制

```rust
#[derive(Debug, Clone)]
#[must_use]
pub struct Chart { /* private */ }

impl Chart {
    pub fn new(request: ChartRequest) -> Result<Self, Error>;
    pub fn to_pdf(&self, font: &Font) -> Result<Vec<u8>, Error>;
    /// `{subject name}股权架构图`. The name is copied as stored.
    #[must_use]
    pub fn title(&self) -> String;
    #[must_use]
    pub fn subject_name(&self) -> &str;
}
```

- `Chart::new` 依次执行三步：校验原始请求（行为规格 §4），按 `top_n` 折叠（§5），再在折叠后的 arena 上重跑全部协议规则。理由见 §6.2。
- `Chart::new` 按值接收请求，出错时请求随之丢弃。调用方要修正后重试，需先自行克隆。这一点写进 rustdoc。
- `to_pdf` 不改变 `Chart`。同一个 `Chart` 多次调用 `to_pdf`，输出字节相同（DET-1，依赖 PR 0）。分页会追加 overflow 节点，所以 `to_pdf` 在自己内部克隆 arena。节点不超过 100 个，克隆的代价可以忽略。
- `to_pdf` 不写 `/Info`。页眉不读 `title()`。
- `Chart` 不暴露页结构、节点表或中间阶段。规格 §14 Q2"中间 API 不进稳定公共面"仍成立。

### 4.3 字体

```rust
#[derive(Clone)]
pub struct Font { /* Arc<Vec<u8>>, upem, ascent, descent */ }

impl Font {
    pub fn new(data: impl Into<Vec<u8>>) -> Result<Self, Error>;
    pub fn from_shared(data: Arc<Vec<u8>>) -> Result<Self, Error>;
}

impl fmt::Debug for Font { /* byte length and units per em, never the bytes */ }
```

- `Font::new` 执行今天 `FontMetrics::open` 的全部检查（`measure.rs:82` 起），包括：拒绝 `ttcf`、拒绝空字节、解析 face、读取 `head` 与 `hhea`、upem 必须有限且大于 0、ascent 大于 0、descent 小于 0。
- `Font::new` 之后，`to_pdf` 仍可能报 `FONT_ERROR`：krilla 拒绝字体（`render.rs:44-45`）、`document.finish` 失败（`render.rs:49-53`）。字形没有 advance（`measure.rs:148-152`）在 PR 3 分类，能由 `Font::new` 预检消掉就消掉，否则保留。
- 字形缓存不放进 `Font`。每次 `to_pdf` 新建一份缓存，所以 `Font` 不可变，可以跨图、跨线程共享。
- `Debug` 手写，只输出字节长度和 upem，以 `finish_non_exhaustive()` 结束，避免把整份字体字节打进日志，也满足 `clippy::missing_fields_in_debug`。
- `Font` 不实现 `PartialEq`/`Eq`。今天 `FontBytes` 实现了，所以这是 breaking，列入 §4.5。

### 4.4 错误

```rust
#[derive(Debug, Clone)]
pub struct Error { /* code, message, subject, source */ }

impl Error {
    #[must_use]
    pub fn code(&self) -> ErrorCode;
    #[must_use]
    pub fn message(&self) -> &str;
    #[must_use]
    pub fn subject(&self) -> Option<&ErrorSubject>;
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorSubject {
    /// `index` is the position in `ChartRequest::nodes`.
    Node { index: usize, id: String },
    /// `index` is the position in `ChartRequest::edges`.
    Edge { index: usize, holder: String, held: String },
    /// A request field, such as `"subject_id"`, `"max_nodes"`, `"top_n"`, or `"style.margin_mm"`.
    Field(&'static str),
    /// A character with no glyph in the font.
    Character(char),
}
```

- `ErrorCode` 与 `as_str` 不变，仍是闭集（ERR-7）。
- `Display` 打印 `message`，遵守 ERR-3。`message` 的措辞不是稳定合同。有 `source()` 时，`message` 不再拼接底层错误的文本（今天 `measure.rs:93,99,231` 与 `render.rs:52` 拼了）。
- `source()` 返回字体解析的底层错误，用 `Arc<dyn Error + Send + Sync>` 保存（API-12，登记文字从 `FontBytes.data` 改为本条）。公共签名里不出现 skrifa 的类型（API-11）。
- `Error` 不实现 `PartialEq`/`Eq`（`source` 无法比较）。今天实现了（`error.rs:55`），列入 §4.5。
- `subject` 只指向调用方写下的东西。折叠占位和 overflow 合成节点不作 `subject`，`message` 用文字描述它们，例如 `folded holders of {parent}`。
- `Edge { index }` 取自 `EdgeRec::caller_index`（§6.1）。折叠合成的边没有原始下标，涉及它的错误改报它在调用方一端的节点，记为 `Node`，两端都是合成节点时不给 `subject`。
- `Error`、`Chart`、`Font` 都加 `Send + Sync` 的编译期断言测试（ERR-2）。

各阶段可能返回的错误码：

| 阶段 | 错误码 | 典型 `subject` |
| --- | --- | --- |
| `Chart::new` | `INVALID_PROTOCOL`、`CYCLE_DETECTED`、`SUBJECT_NOT_FOUND`、`TOO_MANY_NODES` | `Node`、`Edge`、`Field` |
| `Font::new`、`Font::from_shared` | `FONT_ERROR` | 无，原因看 `source()` |
| `Chart::to_pdf` | `MISSING_GLYPH`、`NODE_TOO_TALL`、`LAYOUT_FAILED`，以及 krilla 拒绝字体或序列化失败时的 `FONT_ERROR` | `Character`、`Node`、无 |

各项检查给出的 `subject`（PR 3 实现，按现有检查逐条列出；新增检查须在此表补一行）：

| 检查（当前位置） | `subject` |
| --- | --- |
| `subject_id` 为空白（`graph.rs:192`） | `Field("subject_id")` |
| 主体不在节点里（`graph.rs:209`） | `Field("subject_id")` |
| 节点数超过 `max_nodes`，折叠前或折叠后（`graph.rs:199`） | `Field("nodes")` |
| `max_nodes` 超过上限、`top_n` 不小于 `max_nodes`（`graph.rs:246,255`） | `Field("max_nodes")`、`Field("top_n")` |
| `margin_mm`、`node_width_mm` 越界（`graph.rs:263,273`） | `Field("style.margin_mm")`、`Field("style.node_width_mm")` |
| 节点 id 或名称为空白、id 重复（`graph.rs:288,292`） | `Node`，重复时取后出现的一个 |
| `folded_holders_of` 悬空或重复（§4.1 新规则） | `Node` |
| 节点比例越界（`graph.rs:390`，节点与边共用的 helper） | `Node`。helper 改为接收调用点传入的 `ErrorSubject`，不从标签字符串反推 |
| 边悬空、自环、重复、比例越界、跨锥（`graph.rs:349,355,361,390,504`） | `Edge` |
| `folded_holders_of` 指向的父节点没有来自本节点的直接边（§4.1） | `Node` |
| 环（`graph.rs:413`） | `Edge`，取检测到环时的那条边 |
| 节点与主体不连通（`graph.rs:485`） | `Node`，取下标最小的一个；`message` 列出全部 |
| 父边不唯一（`graph.rs:589`） | `Node`，即有多条候选父边的节点 |
| 折叠时持股方缺展示比例（`prepare.rs:72`） | `Node`，即该持股方 |
| 折叠比例之和越界（`prepare.rs:100`） | `Node`，即被折叠的父节点 |
| 父节点已有声明的占位却仍需折叠（`prepare.rs:108`） | `Node`，即声明的占位 |
| 缺字 | `Character` |
| 节点过高 | `Node`；overflow 节点不给 |

不在表中的两处：`graph.rs:298` 的 `__overflow__:` 拒绝在 PR 3 随 D7 删除；`display.rs:315` `hug_for(Subject)` 的 `INVALID_PROTOCOL` 是内部不可达分支，PR 3 由调用结构保证只为非主体生成比例标签，删除该分支。表中"取最小下标""取后出现的一个"这类选择，用 §9 的测试锁住。

### 4.5 调用方迁移（写进 CHANGELOG `Breaking`）

| `0.2.0` | `0.3.0` |
| --- | --- |
| `validate_request(&r)?` | `Chart::new(r)?`（得到的值可以直接用来出图） |
| `generate_pdf(&r, &f)?` | `Chart::new(r)?.to_pdf(&f)?` |
| `document_title(&r)?` | `chart.title()`。需要自定义标题时，调用方自己拼字符串 |
| `FontBytes::new(b)`、`FontBytes::from_shared(a)` | `Font::new(b)?`、`Font::from_shared(a)?` |
| `ChartOptions { node_width_mm, margin_mm, .. }` | `ChartStyle { node_width_mm, margin_mm }`，字段不再是 `Option` |
| `options.top_n: Some(0)` 或 `None` | `top_n: None` |
| `options.max_nodes: None` | `max_nodes: ChartRequest::MAX_NODES` |
| `options.title` | 删除 |
| `Edge { from_id, to_id, .. }` | `Edge { holder, held, .. }` |
| `Node { paid_in_amount, currency, .. }` | `Node { paid_in: Some(PaidIn { amount, currency }), .. }` |
| `err.code`、`err.message` | `err.code()`、`err.message()` |
| `err == other`、`font_bytes == other` | 改比 `err.code()` 等访问器；`Font` 不可比较 |
| 节点 id 写成 `__other__:{p}` 表示已折叠的占位 | id 任意，设 `folded_holders_of: Some(p)` |
| `MAX_NODES` 等 crate 根常量 | `ChartRequest::MAX_NODES`、`ChartStyle::MIN_NODE_WIDTH_MM` 等 |

## 5. 行为变化（CHANGELOG `Changed`）

1. 折叠后违反任一协议规则时，`Chart::new` 直接返回错误。原来 `validate_request` 会通过，要到 `generate_pdf` 才失败。已知两类：节点数超过 `max_nodes`（`TOO_MANY_NODES`），以及折叠删边后父边选择落到下一档、出现多条候选（`INVALID_PROTOCOL`，例子见 §6.2）。
2. 节点 id 可以以 `__overflow__:` 开头。id 为 `__other__:{p}` 的节点不再有特殊含义，按普通持股方参与排名；要声明折叠占位，改用 `folded_holders_of`（§4.1）。
3. 字体解析错误出现在 `Font::new`。krilla 侧的字体或序列化失败仍在 `to_pdf` 报 `FONT_ERROR`。
4. 所有原本能出图的请求，按 §4.5 迁移后，在同一份字体下，PDF 的可抽取文本、页数、页角色和附件编号都与 `0.2.0` 相同。折叠仍然幂等：把折叠结果按 §4.5 写回请求再编译，结果不变。

以上是全部行为变化。PR 0 只改变 PDF 内的绘制顺序，不属于本节，记在 CHANGELOG `Fixed`。错误 `message` 的措辞可以变，它不是合同。

## 6. 内部合同

### 6.1 arena 与身份

```rust
pub(crate) struct NodeId(usize);
pub(crate) struct EdgeId(usize);

pub(crate) enum NodeOrigin {
    /// From the request. `index` is the position in `ChartRequest::nodes`.
    /// `stands_in_for` comes from `Node::folded_holders_of`.
    Caller { index: usize, id: String, stands_in_for: Option<NodeId> },
    /// `top_n` stand-in for the holders folded under `parent`.
    FoldedHolders { parent: NodeId, count: usize },
    /// Paging stand-in for direct children moved off one page.
    Overflow { subject: NodeId, first_moved: NodeId, moved_count: usize },
}

pub(crate) struct EdgeRec {
    pub(crate) id: EdgeId,
    pub(crate) from: NodeId,
    pub(crate) to: NodeId,
    pub(crate) ratio: Option<f64>,
    /// Position in `ChartRequest::edges`; `None` for an edge added by folding.
    pub(crate) caller_index: Option<usize>,
}
```

- 调用方的字符串 id 只存在 `NodeOrigin::Caller` 里，同时用于 `ErrorSubject` 和错误文案。
- 某节点"已是 `parent` 的折叠占位"，指 `Caller { stands_in_for: Some(parent), .. }` 或 `FoldedHolders { parent, .. }`。折叠排名排除这两种节点，冲突检查也以此判断（§6.2）。
- 删除下列代码（时机见 §10：PR 2 删除 arena 内部的字符串判断，请求边界的前缀规则到 PR 3 才删）：
  - `OVERFLOW_ID_PREFIX`、`overflow_node_id`、`folded_origin`。
  - `"__other__:"` 字面量。
  - `Graph::to_request`、`prepare::rebuild`、`prepare_request`。
- 判断"是不是 overflow""是不是折叠占位""是不是主体"，一律读 `NodeOrigin` 或与 `graph.subject` 比较 `NodeId`。不比较字符串，也不读独立的 bool。
- `NodeId` 只由 arena 分配。下标访问由类型保证（PAN-8）。

### 6.2 折叠在 arena 上进行

`fold(graph, top_n) -> Result<Graph, Error>` 从旧 arena 构建新 arena，不经过 `ChartRequest`。新 arena 的顺序必须与今天 `rebuild` 产出的请求顺序完全相同（`prepare.rs:184-264`）：

1. 存活节点按原顺序排列。
2. 折叠占位按 `kept` 的顺序追加在末尾。
3. 存活边按原顺序排列，折叠边随后追加。

下游按输入序打破平局的规则（折叠排名、overflow 驱逐、家族搜索的 `rank`）都依赖这个顺序。存活边保留 `caller_index`，折叠边为 `None`。

`Chart::new` 折叠后在新 arena 上重跑全部协议规则，相当于今天 `generate_pdf` 的第二次 `validate`（`lib.rs:150-151`）。不能只复核节点数：父边按四档选取（`graph.rs:527-573`），折叠删掉一条边后，节点可能落到下一档并出现多条候选。2026-11-04 审查中的反例：A（节点比例 40）、B（30）、X（无节点比例），边为 A→root、B→root、X→root、X→A、X→B，`top_n = 2`。`validate_request` 返回 `Ok`，`generate_pdf` 返回 `INVALID_PROTOCOL`（`ambiguous parent edges for X count=2`）。§9 把它列为夹具。

折叠保持幂等。某父节点下已有占位（§6.1）时，排名跳过它；剩余持股方不超过 `top_n` 就不折叠；仍超过则报冲突。这与今天 `prepare.rs:64` 和 `prepare.rs:106-111` 依据 `__other__:{p}` id 的行为相同。

### 6.3 页面层换键

| 位置（当前） | 字符串字段 | 改为 |
| --- | --- | --- |
| `measure.rs:258-260` `NodeBox` | `protocol_id`、`is_overflow` | `id: NodeId`，origin 从 arena 读 |
| `measure.rs:278` `InboxNode` | `protocol_id` | 删除 |
| `measure.rs:390` | 用字符串比较判断主体 | 与 `subject: NodeId` 比较 |
| `measure.rs:564`、`cues.rs:20,26,48` | `anchor_id`、`node_id` | `NodeId` |
| `page.rs:42` `PlacedNode` | `protocol_id` | 删除 |
| `page.rs:70` `CueSlot`、`page.rs:229` `ContCue` | `anchor_id` | `NodeId` |
| `page.rs:236` `NodePageLink` | `node_id` | `NodeId` |
| `page.rs:199-204` `PreferredParent`、`display.rs` `ParentEdge` | `from_id`、`to_id` | 复用 `EdgeRef`（`page.rs:13-18`，字段 `from_id`、`to_id` 已是 `NodeId`） |
| `family.rs:80` `OverflowNode::protocol_id` | 拼接出的 id | 删除 |
| `layout/place.rs:702,736` 构造 `PlacedNode` | `protocol_id` | 随 `PlacedNode` 删除 |
| `render.rs:88,103,126,280,292`、`ratios.rs:53,152` | 按字符串查框或比例 | 按 `NodeId` 查 |
| `paging.rs:90,194,276,325,333,762,928` | 字符串键的表、`protocol()` 查询 | `NodeId`；文案需要时从 `NodeOrigin` 取 |
| `measure.rs:592-619` | overflow 框的拼接 id | 删除 |
| `page.rs:131,284-351` `check()` | 用前缀和字符串判断 overflow | 读 `NodeOrigin` |
| `cues.rs:85,112,342,379-391,430` | 字符串键 | `NodeId` |
| `display.rs:91-102` | 字符串键 | `NodeId` |

上表按 2026-11-04 的 `rg protocol_id\|anchor_id\|node_id\|from_id\|to_id` 结果整理。PR 2 动手前重跑一次，表外命中按同一规则处理。

`cues.rs:162` 的平局规则按 `anchor_id` 字符串比较，换成比较 `NodeId`。理由：排序的前两级比较 `(y, x)`，已放置的节点框互不重叠，所以两个不同的锚点不会在前两级打平；同一个锚点在两种比较下都相等。2026-11-04 审查中，在现有测试集上加探针验证过：不同锚点在 `(y, x)` 上打平 0 次，换键后全部测试结果与基线相同。这只覆盖现有夹具。换键后如果有分页测试断言变化，说明这条推断不成立，此时停下来查原因，不要改常量。

### 6.4 内部不变量与 `LAYOUT_FAILED`

`ErrorCode::LayoutFailed` 在 `error.rs` 之外有 50 处，分布在 10 个文件。这个数字用 `rg -c "ErrorCode::LayoutFailed" --glob '!*_tests.rs' --glob '!error.rs' crates/pdfcraft-orgchart/src` 统计；内联测试模块里没有命中。PR 3 逐处分类，分类表写进 PR 描述：

- **由类型消掉。** 指换键以后不可能再发生的情况。例如"放置的节点没有框"（`render.rs:88`）、"overflow 框与链接不是一一对应"、"续页锚点是 overflow"（`page.rs:283-351`）。做法是从同一份数据构造，或者用 `NodeId` 下标访问。删除对应分支。
- **保留。** 产品上可以合理遇到的失败，例如主体或直接层放不进页面、续页框的文字超出宽度。这一类继续报 `LAYOUT_FAILED`。
- **改码。** `display.rs:284` 在出图路径上调用 `ratio_source`，今天可能报 `INVALID_PROTOCOL`。D3 之后这类错误都在 `Chart::new` 报出，这里就成了内部不变量，按前两类处理。

`graph.rs:113` `layout_parent_optional` 用 `unwrap_or_default` 吞掉错误，同样在 PR 3 归类：要么由 D3 证明不会出错、改用类型表达，要么把错误传上去。

不新增生产 `expect`（D13）。

## 7. 不动

- 分页、家族搜索、走线、开槽的控制流与比较顺序（结构规格 §1、§5）。
- 行为规格 §8 的文案、§9 的测字合同、§10 的分页合同（§10.4 只改 overflow 节点 id 的文字描述，见 PR 4）、§12 的常量。
- `ErrorCode` 的变体与 `as_str` 字符串。
- `krilla =0.8.2`、`skrifa =0.48.0`、`read-fonts =0.45.0` 钉版。不新增依赖。
- `#![forbid(unsafe_code)]`。

## 8. 模块与文件

### 8.1 目标模块图

本 crate 的模块依赖必须是 DAG（ARC-2）。下图每条边都对应 PR 1 搬运后实际存在的 `use`。

```text
error                       无内部依赖
units                       无内部依赖
captions                    无内部依赖                    纯字符串函数：标题、页脚、"其他股东"等文案
request   → error（PR 1、PR 2 期间另有 → captions，供 document_title 用；PR 3 删除）
font      → error, units                                 Font、字形缓存、Measurer、wrap_text
graph     → request, captions, error                     arena、协议规则、折叠、比例解析
content   → graph, request, captions, error              框内行、比例文本
page      → graph, content, captions, font, request, units, error 节点框与尺寸（含 WrappedLine）、放置结果、页模型、续页框尺寸。→ request 来自 measure.rs:392,493 读节点宽上限
layout    → page, graph, units, error                    ranks、groups、place、route（算法不动）
paginate  → layout, page, content, captions, graph, request, font, units, error
render    → page, content, captions, graph, font, units, error  唯一出现 krilla 的地方
chart     → paginate, render, page, content, captions, graph, font, request, units, error
lib       → chart, error, font, request                  只做重导出
```

`chart` 放在最上层，因为 `Chart::to_pdf` 要调用 `paginate` 和 `render`。arena 放在下层的 `graph` 里，避免出现环。`captions` 单独放在最底层：`model.rs:205` 的 `document_title` 和 `prepare.rs:271` 的折叠都要用它，放进 `content` 会形成 `request → content → request` 与 `graph → content → graph` 两个环。

目录模块一律用 `<name>/mod.rs`，不写成 `<name>.rs` 加 `<name>/`，因为根 `Cargo.toml:66` 把 `clippy::self_named_module_files` 设为 deny。

### 8.2 文件去向

| 现有文件 | 去向 |
| --- | --- |
| `lib.rs` 的出图管线 | `chart.rs` |
| `model.rs` | `request.rs`。`FontBytes` 并入 `font/mod.rs` |
| `error.rs` | `error.rs` |
| `ids.rs`；`graph.rs` 的 `Graph` 与访问器 | `graph/arena.rs` |
| `graph.rs` 的 `validate`、选项边界、环检测、双锥 | `graph/check.rs` |
| `graph.rs` 的比例解析与父边选择（约第 527 到 662 行，含 `ratio_source`） | `graph/ratio.rs` |
| `prepare.rs` | `graph/fold.rs` |
| `measure.rs` 的 `FontMetrics`、`Measurer`、`wrap_text`、`GlyphUse`、`line_height_mm` | `font/mod.rs`（超过约 600 行时拆出 `font/metrics.rs` 与 `font/wrap.rs`） |
| `measure.rs` 的 `WrappedLine`、`NodeBox`、`InboxNode`、`build_node_boxes`、`join_segments`、`overflow_box`、`promote_to_page_subject`、续页框尺寸 | `page/boxes.rs` |
| `display.rs` | `content/lines.rs`，测试见 §8.3 |
| `captions.rs` | `captions.rs`（位置不变，见 §8.1），测试见 §8.3 |
| `page.rs` | `page/model.rs` |
| `paging.rs` 的 `Session` 骨架：`new`、`run`、`pack_side`、`search_page_to_spec`、`materialize`、`to_page`、`unnumbered`、`protocol`、`sorted`，以及 `task_key`、`task_index`、`has_fork` | `paginate/session.rs` |
| `paging.rs` 选版式与朝向的方法：`key`、`standalone_key`、`evaluate`、`evaluate_direct`、`layout_pack`、`choose_layout`、`choose_direct_layer`、`whole_page_if_elastic`、`orientation_pair`、`as_evaluation`、`layout_at`、`pack_geometry`、`layout_root`，以及 `SideOracle` | `paginate/fit.rs` |
| `paging.rs` 组页的方法：`boxes_for_page`、`overflow_for`、`family_graph`，以及 `investment_id_refs` | `paginate/assemble.rs` |
| `paging.rs` 标题与附页说明：`next_shareholder_titles`、`next_investment_titles`、`cut_caption`、`node_name` | `paginate/titles.rs` |
| `paging.rs` 的 `FallbackMeasurer`（已带 `#[cfg(test)]`） | `font/fixed.rs`，在 `font/mod.rs` 里以 `#[cfg(test)] mod fixed;` 挂载 |
| `family.rs` 的草稿类型、`PageOracle`、`Context` 骨架与图查询方法、`store_plan`、`plan_at`、`make_draft` | `paginate/family/mod.rs` |
| `family.rs` 的 `PlanScore`、`OverflowScore`、`score`、`sparse_risk`、`page_utilization`、`sparse_buckets`、`repeated_context` | `paginate/family/score.rs` |
| `family.rs` 的 `solve_task`、`expand_page`、`page_for`、`classify` | `paginate/family/solve.rs` |
| `family.rs` 的 `plan_head`、`compute_head`、`measure_direct_cuts`、`overflow_cut`、`run_for_cut`、`chosen_direct_run`、`direct_run_score`、`build_head`、`legal_direct_cuts`、`head_draft`、`accepted_layout`、`over_elastic` | `paginate/family/head.rs` |
| `family.rs` 的 `greedy_plan`、`enumerate_inlines` | `paginate/family/inline.rs` |
| `family.rs` 的 `chain_path`、`solve_chain` | `paginate/family/chain.rs` |
| `cues.rs` | `paginate/cues.rs` |
| `layout/*` | 文件不搬。PR 0 改 `layout/mod.rs:110` 的收集顺序；PR 2 改 `layout/place.rs:702,736` 的 `PlacedNode` 构造 |
| `render.rs` 的 `render`、`draw_page`、`origin`、`draw_ratios` | `render/mod.rs` |
| `render.rs` 的页眉页脚：`draw_titles`、`header_slots`、`fit_header_line`、`draw_footer` | `render/chrome.rs` |
| `render.rs` 的节点、续页框、引线：`draw_node`、`paint_node_lines`、`draw_cues`、`leader_*`、`stroke_leader`、`fill_leader_arrow` | `render/marks.rs` |
| `render.rs` 的 krilla 图元：`fill_round`、`stroke_*`、`round_path`、`arrow`、颜色、`center_text`、`draw_text`、`krilla_glyphs`、`rect_pt` | `render/paint.rs` |
| `ratios.rs` | `render/ratio_labels.rs` |

同一类型的 `impl` 块可以分写在同一模块树的多个文件里。拆 `Session` 和 `Context` 时只搬方法，不改签名、不改控制流。表中的方法名以 PR 1 当时的代码为准。新增或改名的方法按同一职责归类。

### 8.3 测试放置

- 单元测试使用 `pub(crate)` 条目，留在 `src/`。按 TST-1，可以内联 `#[cfg(test)] mod tests`，也可以放在同目录的 `<name>_tests.rs` 里，用 `#[cfg(test)] #[path = "..."] mod tests;` 挂载。不挪进 `tests/`，因为 `tests/` 只能用公共 API。
- 内联测试比生产代码还长的，挪到 `*_tests.rs`：`prepare.rs`（测试 339 行，生产代码 273 行）、`ratios.rs`（311 行对 215 行）。
- 同一文件有两个测试模块的，合并成一个 `*_tests.rs`：`display.rs`（内联 198 行加 `display_tests.rs`）、`captions.rs`（内联 83 行加 `captions_tests.rs`）。
- 其余内联测试（`units`、`error`、`graph`、`model`、`cues`、`render`、`family`）跟着所在代码搬，不强制挪出。
- `paging_tests.rs`（1113 行）跟着 `paginate/` 的拆分，按主题分成多个文件。`measure_tests.rs` 跟着 `font` 和 `page/boxes` 拆分。
- `tests/` 下的集成测试只使用 §4 的公共面，在 PR 3 重写。

## 9. 测试

- **纯搬运的 PR（PR 1）。** 测试数量不变，断言一行不改。
- **换键的 PR（PR 2）。** 分页测试的页数、主副标题、角色、附件编号断言不改。`tests/generate_pdf.rs` 的 `pdftotext` 断言不改。
- **PR 0。** 同一请求连续出图两次，字节相同。用一张会分多页的夹具（审查时用的是 33 页的图）。
- **新增测试。**
  - 对每个会折叠的夹具，在折叠后的 arena 上重跑全部协议规则，结果都应通过。
  - §6.2 的反例夹具：`Chart::new` 返回 `INVALID_PROTOCOL`，`subject` 为 `Node`（X）。
  - 折叠后的节点顺序、占位名称、比例，与 `0.2.0` 的 `prepare_request` 输出逐项一致。用现有 `prepare.rs` 测试里的期望值作字面量。
  - 幂等：把折叠结果写回请求（占位设 `folded_holders_of`），再次 `Chart::new` 加 `to_pdf`，字节与第一次相同。取代 `prepare.rs` 的 `fold_is_idempotent_and_keeps_the_investment`。
  - 父节点已有声明的占位却仍需折叠，返回 `INVALID_PROTOCOL`；`folded_holders_of` 悬空、重复，或声明节点没有指向父节点的边，返回 `INVALID_PROTOCOL`。
  - `Chart::new` 在"折叠后超过 `max_nodes`"时返回 `TOO_MANY_NODES`。改写 `tests/prepare.rs:94` 的预期。
  - 节点 id `__overflow__:x` 可以出图。id 为 `__other__:root` 但未设 `folded_holders_of` 的节点，按普通持股方参与 `root` 的排名。
  - `Font::new` 对 `ttcf`、空字节和坏表返回 `FONT_ERROR`，`source()` 不为空，`message` 不含 `source` 的文本。`Debug` 输出不包含字节内容。
  - 同一个 `Chart` 调用两次 `to_pdf`，字节相同。同一个 `Font` 给两张图用，结果与各自新建 `Font` 相同。
  - `ErrorSubject` 按 §4.4 的对照表逐行覆盖，包括折叠合成边涉及的错误给出 `Node`。
  - `Error`、`Chart`、`Font` 满足 `Send + Sync`。
- 不断言压缩内容流。不断言错误 `message` 的措辞。

## 10. PR 切分

顺序固定。每个 PR 都能单独通过 TOOL-5 门禁。

| PR | 内容 | 版本与 CHANGELOG |
| --- | --- | --- |
| (0) | `layout/mod.rs:110` 的 `laid_out.into_values().collect()` 遍历 `HashMap`（`layout/place.rs:678`），绘制顺序不确定，同一请求两次出图字节不同（DET-1）。改为收集后按 `NodeId` 排序（`sort_unstable_by_key`）。几何、文本、像素不变。2026-11-04 复核时在临时 worktree 做过正反对照：48 层多页请求，改前两次出图字节不同，改后相同。可以先于本文其余部分合入 | 不升版本。`Unreleased` 写 `Fixed`，随 `0.3.0` 发布 |
| (1) | 按 §8 搬文件、拆文件、挪测试。在 `docs/module-dependency.md` 补 orgchart 的模块图。不改任何逻辑 | 不变，不记 |
| (2) | §6.1 到 §6.3：arena 身份、在 arena 上折叠、页面层换键、`NodeId(usize)`、`EdgeRec::caller_index`。三个公共函数的行为原样保留，包括"校验通过、出图失败"的分裂。前缀规则（`__overflow__:` 拒绝、`__other__:{p}` 的排名排除与冲突）改为在请求边界把字符串翻译成 `NodeOrigin`，arena 内部不再比较字符串；字面量到 PR 3 才删。翻译在完整的节点索引建好之后进行，以 `prepare.rs:57-64,106-111` 的排名排除与冲突行为为准；`graph.rs:306-307` 边插入边查找、要求父节点先出现的做法是实现细节，不作为合同 | 不变，不记 |
| (3) | §4.1 到 §4.3 与 §5：`Chart`、`Font`、`ChartStyle`、`Edge` 与 `Node` 改形状（含 `folded_holders_of`）、关联常量、D3、D7。按 D14 在工程规范 API-9 与 disposition 登记例外，API-12 的登记文字从 `FontBytes.data` 改为 `Font`。同步示例、crate 文档、`docs/guide.md`、`tests/`。版本号同步两份 README（crate `README.md` 的安装片段与根 `README.md:64` 的状态行，`scripts/check_readme_identity.py` 是门禁），以及 `AGENTS.md:17`、提案书第 661 行。`AGENTS.md:83` 的 `FontBytes` 改为 `Font`。同一 PR 内另做 §4.4 与 §6.4：`Error` 字段私有、`ErrorSubject`、`source()`、ERR-3 去重复文本、`LAYOUT_FAILED` 分类。**开工前提案书 §13 已确认（§11）** | 升到 `0.3.0`。写 `Breaking`（§4.5，含访问器、`Error` 不再可比较）和 `Changed`（§5）。合入后可发 `0.3.0` |
| (4) | 文档终态：行为规格 §2、§3、§4、§5、§6、§7、§9.1、§10.4（overflow id 不再是字符串）、§13、§14；结构规格 §1、§2、§3、§4、§9；`workspace-crates-spec.md` 第 44 行；工程规范 TRK-2 与 TRK-4 的 orgchart 反例、ERR-1 对 orgchart 错误的描述；disposition 中 M-STRONG-TYPES 改为 done | 不变 |

PR 3 把公共面和错误面的全部 breaking 改动放在一起，因为 VER-3 要求在同一个 PR 内完成；而且每一项都要重写同一批 doctest、示例和集成测试，分开做就要重写多遍。PR 内分两组提交，每次提交门禁都绿：先做公共面（§4.1 到 §4.3），再做错误面（§4.4 与 §6.4，约 80 处 `Error::new`）。这样评审时可以单独看错误面那组提交。

crate README 的示例代码不受门禁编译，PR 3 人工核对（今天在 `crates/pdfcraft-orgchart/README.md:34-61`）。

## 11. 文档同步

- **提案书。** 新增一条（建议编号 §13，"`pdfcraft-orgchart` 公共值形状"），写法与 §12.7 相同，列出 D1 到 D14 的要点，指向本文。§13 须写明：orgchart 的字体入参由 `FontBytes` 改为预先解析的 `Font`，取代提案书 §9（第 663 行）与确认总表 §4.1（第 928 行）里关于 orgchart 的表述；`pdfcraft-forms` 的 `FontBytes`（借用 `&[u8]`）不受影响。第 623 行里 orgchart 的 `FontBytes.data`、`FontBytes::new`、`FontBytes::from_shared` 同步改为 `Font` 内部持有 `Arc<Vec<u8>>`、`Font::new`、`Font::from_shared`。这一条已于 2026-11-04 起草为草案，维护者确认后再更新提案书抬头与文末确认总表，本文的状态同时改为"产品条已定稿"。它不放进任何实现 PR，PR 3 以它的确认为前提。
- **行为规格 §14 新增和改写的条目。**
  - 改写 Q5：不保留前缀，身份由枚举表达，已折叠的占位由 `folded_holders_of` 声明。
  - 改写 Q11：在 `Chart::new` 内折叠后计数。
  - 改写 Q12：类型是 `Option<NonZeroUsize>`。
  - 改写 Q13：外观缺省在 `ChartStyle::default`。
  - 新增 Q15 到 Q21，分别对应 D1 与 D2、D4、D5、D8、D9 与 D10、D12、D14。
- **历史规格不改。** `shared-font-bytes-spec.md` 与 `pdfcraft-orgchart-draw-fix-spec.md` 是已落地的历史合同，保留原文。

## 12. 不做

- 改算法、启发式、评分顺序、常量、文案。
- 新的产品能力：换纸张、换词表、环状持股、跨页连线、自定义节点行。
- serde 或 JSON 主接口、builder、`FromStr`。
- `Percent` 或 `Mm` 新类型。
- 让 `Chart` 暴露页结构，或开放测宽器替身。
- 修改 hcsw 工程。hcsw 钉在 `=0.1.1`，由它自己决定何时迁移。
- 擅自 `git commit` 或 `push`。
