# pdfcraft-orgchart 规格（整包迁移）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-13 |
| 状态 | **整包行为合同已写全（reference）**。§3 到 §5 与 §14 产品答复已确认。layout 装箱的逐步算法仍以源包实现为对照，本文件钉可观察合同与常量，不逐函数翻译 |
| 相关 | [`workspace-crates-spec.md`](workspace-crates-spec.md)；[`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md)；源包 `/mnt/data/dev/hcsw-pdfcraft/backend/packages/pdfcraft-equity` |
| 意图来源 | `pdfcraft-equity` 行为合同。Rust 惯用重实现，见 §0 |
| 范围 | 公共 API、协议校验、折叠、展示、测量、分页装箱、绘制、错误码、常量、可测切片 |

本文是 **reference**。实现文件名不钉死。内部类型、模块和所有权见 [`pdfcraft-orgchart-structure-spec.md`](pdfcraft-orgchart-structure-spec.md)。冲突时：协议、文案、错误码、常量、分页与装箱的可观察结果以本文为准；代码形状以结构规格为准。

**完整度判定。** 有了本文 + 源包测试，可以实现并验收整包到 `Chart::new` / `to_pdf`。layout / paging 的内部搜索与启发式（家族开槽、bus 错开、overflow 链择优）以实现对照 `paging.py` / `layout/*` 为主。规格钉住输入输出、页角色、文案、错误码与数值常量，避免发明产品规则。

## 0. 迁移姿态（非机械翻译）

对齐的是**行为合同**，不是 Python 源码结构。

| 对齐 | 不对齐 |
| --- | --- |
| 双锥规则、错误码字符串、折叠语义、框内/页眉文案、分页角色、可抽取文本与页结构 | 模块拆法、类层次、`dataclass`、可变字典、异常抛掷、reportlab API |
| 测字与绘制同一套 glyph id（skrifa charmap → advance → `draw_glyphs`） | 用 krilla `simple-text` / `Surface::draw_text` 反推宽度 |

实现时用惯用 Rust。

- 所有权与借用。错误用 `Result`。枚举表达角色与错误码。
- 边界解析字体 bytes 与 options 缺省。内部信任已解析类型。
- 图用索引 map 与邻接表。不保留「只为像 Python」的形状。
- 测试断言可观察结果。不断言私有辅助是否同名。

禁止：把 `pdfcraft_equity/*.py` 逐文件改成同名 `.rs` 后称迁移完成。

## 1. 产品一句话

`pdfcraft-orgchart` 接收股权 / 对外投资图的节点边协议与字体字节，输出矢量 PDF 字节。页眉用简体中文角色文案。调用方自己写 PDF `/Info`（若需要）。

不依赖 `pdfcraft-pdf` 或 `pdfcraft-forms`。测字 `skrifa =0.48.0`。出 PDF `krilla =0.8.2`（`default-features = false`，按需开 feature，关 `simple-text`）。

## 2. 范围

### 2.1 做

- 双锥协议。`holder` 持有 `held`。股东在上，对外投资在下。
- `Chart::new`。不加载字体。含 `top_n` 折叠，并在折叠后的 arena 上重跑协议规则。
- `Chart::to_pdf`。`content` 框内行，`font` 测字，`page` 节点框，`paginate` 分页，`render` 绘制。
- 错误码字符串与 Python `EquityErrorCode.value` 一致。
- 公共类型是 `Chart`、`ChartRequest`、`ChartStyle`、`Node`、`Edge`、`PaidIn`、`NodeType`、`Font`、`Error`、`ErrorCode`、`ErrorSubject`（§14）。
- 字体只接受内存字节。`Font::new` 与 `Font::from_shared` 当场解析，返回 `Result`。

### 2.2 不做

- 依赖或反向依赖 `pdfcraft-pdf`、`pdfcraft-forms`。
- 环状交叉持股、跨页连线、自定义节点行、换纸张、换词表。
- JSON、serde 作主接口。
- 进程级默认字体路径。
- 几何截图作 v1 门禁（§14 Q10）。
- 向调用方暴露页结构或中间阶段。

### 2.3 与其它 crate

| | `pdfcraft-pdf` | `pdfcraft-forms` | `pdfcraft-orgchart` |
| --- | --- | --- | --- |
| 输入 | `DocumentSpec` | 已有 PDF + 字段值 | 节点边协议 + `Font` |
| PDF 栈 | krilla（仅 backend） | lopdf | krilla（本 crate 直接用） |
| 测字 | skrifa | skrifa | skrifa `0.48.0` |

不依赖 `pdfcraft-pdf`。

## 3. 公共 API（已确认）

crate 根导出 `Chart`、`Error`、`ErrorCode`、`ErrorSubject`、`Font`、`ChartRequest`、`ChartStyle`、`Edge`、`Node`、`NodeType`、`PaidIn`。每条 `pub use` 带 `#[doc(inline)]`。

### 3.1 入口

```rust
impl Chart {
    pub fn new(request: ChartRequest) -> Result<Self, Error>;
    pub fn to_pdf(&self, font: &Font) -> Result<Vec<u8>, Error>;
    pub fn title(&self) -> String;
    pub fn subject_name(&self) -> &str;
}
```

- `Chart::new` 按值接收请求。它先校验原始请求（§4），再按 `top_n` 折叠（§5），再在折叠后的 arena 上重跑全部协议规则。不读字体。出错时请求随之丢弃。调用方要修正后重试，先克隆请求。
- `Chart::to_pdf` 见 §7。不改变 `Chart`。同一个 `Chart` 配同一份 `Font`，多次出图字节相同。不写 `/Info`。
- `Chart` 不持有字体。
- `title()` 恒为 `{主体名}股权架构图`。名字按节点上存储的文本复制。`subject_name()` 返回该文本。页眉不读 `title()`。

### 3.2 请求类型

| Python | Rust |
| --- | --- |
| `EquityChartRequest` | `ChartRequest` |
| `EquityNode` | `Node` |
| `EquityEdge` | `Edge` |
| 无对应类型 | `ChartStyle`、`PaidIn`、`Font`、`Chart`、`ErrorSubject` |
| `NodeType` | `NodeType` |
| `PdfcraftEquityError` | `Error` |
| `EquityErrorCode` | `ErrorCode` |

```rust
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
    pub fn new(subject_id: impl Into<String>, nodes: Vec<Node>, edges: Vec<Edge>) -> Self;
}

pub struct Node {
    pub id: String,
    pub name: String,
    pub legal_person: Option<String>,
    pub ratio: Option<f64>,
    pub node_type: Option<NodeType>,
    pub paid_in: Option<PaidIn>,
    pub location: Option<String>,
    pub approval_date: Option<String>,
    pub folded_holders_of: Option<String>,
}

impl Node {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self;
}

pub struct PaidIn {
    pub amount: String,
    pub currency: Option<String>,
}

impl PaidIn {
    pub fn new(amount: impl Into<String>) -> Self;
}

pub struct Edge {
    pub holder: String,
    pub held: String,
    pub ratio: Option<f64>,
}

impl Edge {
    pub fn new(holder: impl Into<String>, held: impl Into<String>) -> Self;
}

pub struct ChartStyle {
    pub node_width_mm: f64,
    pub margin_mm: f64,
}

impl ChartStyle {
    pub const MIN_NODE_WIDTH_MM: f64 = 20.0;
    pub const MAX_NODE_WIDTH_MM: f64 = 80.0;
    pub const MAX_MARGIN_MM: f64 = 40.0;
}

pub enum NodeType {
    Person,
    Company,
}

impl Font {
    pub fn new(data: impl Into<Vec<u8>>) -> Result<Self, Error>;
    pub fn from_shared(data: Arc<Vec<u8>>) -> Result<Self, Error>;
}
```

- `ChartRequest::new` 填 `ChartStyle::default()`、`top_n: None`、`max_nodes: ChartRequest::MAX_NODES`。`ChartRequest` 没有 `Default`。
- `ChartStyle::default` 是宽 `38` mm、边距 `8` mm。字段是具体数，不是 `Option`。
- `Node::new`、`Edge::new`、`PaidIn::new` 把其余可选字段设为 `None`。
- `node_type` 对应 Python `type`。对外字符串用 `PERSON`、`COMPANY`，经 `NodeType::as_str`。
- 可选字符串空串或纯空白在展示层当缺省。`id`、`name`、`subject_id` 校验仍要求 strip 后非空。
- 金额与日期格式不校验。`PaidIn.amount` 去掉首尾空白后为空，就不画实缴行。`currency` 为空白时按没有币种处理。
- `folded_holders_of` 声明本节点已是该 id 所指父节点的折叠占位。见 §5。
- `holder` 持有 `held`。
- `Node`、`Edge`、`PaidIn`、`ChartRequest`、`ChartStyle` 保持公有字段，供结构体字面量构造。不标 `#[non_exhaustive]`。`Chart`、`Font`、`Error` 字段私有。

### 3.3 关联常量

crate 根不导出常量。

| 常量 | 值 |
| --- | --- |
| `ChartRequest::MAX_NODES` | `100` |
| `ChartStyle::MIN_NODE_WIDTH_MM` | `20.0` |
| `ChartStyle::MAX_NODE_WIDTH_MM` | `80.0` |
| `ChartStyle::MAX_MARGIN_MM` | `40.0` |

`ChartStyle::default` 的普通宽是 `38` mm，边距是 `8` mm。

### 3.4 错误类型

```rust
pub struct Error { /* 私有：code, message, subject, source */ }

impl Error {
    pub fn code(&self) -> ErrorCode;
    pub fn message(&self) -> &str;
    pub fn subject(&self) -> Option<&ErrorSubject>;
}

pub enum ErrorCode {
    InvalidProtocol,
    CycleDetected,
    SubjectNotFound,
    TooManyNodes,
    FontError,
    MissingGlyph,
    NodeTooTall,
    LayoutFailed,
}

#[non_exhaustive]
pub enum ErrorSubject {
    Node { index: usize, id: String },
    Edge { index: usize, holder: String, held: String },
    Field(&'static str),
    Character(char),
}
```

- `ErrorCode::as_str` 映射到 `INVALID_PROTOCOL`、`CYCLE_DETECTED`、`SUBJECT_NOT_FOUND`、`TOO_MANY_NODES`、`FONT_ERROR`、`MISSING_GLYPH`、`NODE_TOO_TALL`、`LAYOUT_FAILED`。`ErrorCode` 是闭集，调用方按 `code()` 穷尽匹配。
- `Display` 打印 `message`。`message()` 给日志。措辞不是合同。有 `source()` 时，`message` 不拼接底层错误的文本。
- `std::error::Error::source()` 返回字体解析的底层错误。公共签名里不出现 skrifa 类型。
- `Error` 不可比较。`Font` 不可比较。
- `ErrorSubject` 标 `#[non_exhaustive]`。`Node.index` 是 `ChartRequest::nodes` 里的位置。`Edge.index` 是 `ChartRequest::edges` 里的位置。二者只指向调用方写下的节点和边。折叠占位与 overflow 合成节点不作 `subject`。`Field` 是请求字段，例如 `"subject_id"`、`"nodes"`、`"max_nodes"`、`"top_n"`、`"style.margin_mm"`、`"style.node_width_mm"`。`Character` 是字体里缺的那个字符，可能来自库自带的文案。
- 不提供 `to_dict`。HTTP 状态不属于本库。

### 3.5 内部 API（`pub(crate)`）

- 身份是 `NodeId(usize)` 与 `NodeOrigin`。变体是 `Caller`、`FoldedHolders`、`Overflow`。见结构规格 §4。
- 节点 id 不按前缀分类。折叠占位只认 `folded_holders_of` 与 `NodeOrigin`。

## 4. 图校验合同

见既有行为。摘要如下。全部发生在 `Chart::new`。

1. 空白 `subject_id` → `INVALID_PROTOCOL`。`subject` 为 `Field("subject_id")`。
2. 边界。`max_nodes` ∈ `1..=ChartRequest::MAX_NODES`（100）。`top_n` 为 `None` 时不折叠。`Some(n)` 的 `n` ∈ `1..=max_nodes`。`top_n == max_nodes` 合法。`margin_mm` 为有限值且 ∈ `(0, 40]`。`node_width_mm` 为有限值且 ∈ `[20, 80]`。越界 → `INVALID_PROTOCOL`。
3. `nodes.len() > max_nodes` → `TOO_MANY_NODES`。折叠前查一次，折叠后再查一次。
4. 节点 id 或名称为空白、id 重复 → `INVALID_PROTOCOL`。节点 id 不按前缀分类。
5. 主体不在表 → `SUBJECT_NOT_FOUND`。
6. 悬空边、自环、重复边、比例超出 `0..=100` → `INVALID_PROTOCOL`。
7. 有向环 → `CYCLE_DETECTED`。
8. 双锥连通，以及股东锥到投资锥的跨锥边 → `INVALID_PROTOCOL`。
9. 无节点比例时布局父边必须唯一 → `INVALID_PROTOCOL`。
10. `folded_holders_of` 指向不存在的节点，两个节点声明同一个父节点，或声明的节点没有一条指向该父节点的直接边 → `INVALID_PROTOCOL`。

`Chart::new` 的顺序是校验原始请求、折叠、在新 arena 上重跑全部协议规则。折叠后节点数超限，或折叠删边后父边不唯一，都在 `Chart::new` 失败。

双锥与比例解析细则与源包 `graph.py` 一致（股东入边锥、投资出边锥、父边选择四步）。

## 5. prepare 合同（`top_n`）

只折叠股东锥。`top_n` 为 `None` 时图不变。`Some(n)` 时，每个被排名的股东子都要有展示比例。按比例降序，同比例按 `nodes` 序，保留前 `n` 个，其余合成一个占位。占位身份是 `NodeOrigin::FoldedHolders { parent, count }`。展示名是 `其他股东(k家)`。

调用方用 `folded_holders_of` 声明某节点已是该父节点的占位。对该父节点排名时跳过此节点。此节点不占名额，也不并入新占位。该父节点仍需折叠时报 `INVALID_PROTOCOL`，`subject` 为声明的节点。占位的展示、测字、分页与普通调用方节点相同，名称由调用方提供。

被折掉的比例用与 CPython `math.fsum` 相同的有限和。结果须落在 `0..=100`，否则 `INVALID_PROTOCOL`。六笔 `100/6` 的和是 100，可以折叠。存活股东重算后删不可达节点。折叠幂等。投资侧不动。

折叠在 arena 上构建新 arena。`Chart::new` 随后在这张新 arena 上重跑全部协议规则，包括 `max_nodes`。

## 6. 错误码触发点（整包）

| 码 | 阶段 | 触发 |
| --- | --- | --- |
| `INVALID_PROTOCOL` | `Chart::new` | §4 与 §5 的协议和折叠错误 |
| `CYCLE_DETECTED` | `Chart::new` | 有向环 |
| `SUBJECT_NOT_FOUND` | `Chart::new` | 主体不在节点表 |
| `TOO_MANY_NODES` | `Chart::new` | 折叠前或折叠后，节点数超过 `max_nodes` |
| `FONT_ERROR` | `Font::new`、`Font::from_shared`、`Chart::to_pdf` | 打不开单字体、字体集合、解析失败。`to_pdf` 里还有 krilla 拒绝字体、序列化失败、字形没有 advance |
| `MISSING_GLYPH` | `Chart::to_pdf` | 测宽或绘制前 `ensure_glyphs` 缺字 |
| `NODE_TOO_TALL` | `Chart::to_pdf` | 折行后节点高超过 `node_height_limit_mm`（肖像可印高） |
| `LAYOUT_FAILED` | `Chart::to_pdf` | 分页或布局无法完成。缺锚点框、主体或直接层放不进页、续页文字超宽。出图路径上的协议错误也记这个码 |

`message` 的措辞不是合同。调用方用 `code()` 取 `ErrorCode`，再用 `as_str` 取稳定字符串。

## 7. `to_pdf` 管线

```text
ChartRequest
  → Chart::new
      graph 校验原始请求
      → graph 按 top_n 在 arena 上折叠
      → graph 在折叠后的 arena 上重跑协议规则
  → Chart
Font::new 或 Font::from_shared
  → Chart::to_pdf
      content 框内行
      → font 测字
      → page 节点框（肖像可印高，NODE_TOO_TALL）
      → paginate（横竖切换、overflow、cue、升格附页）
      → ensure_glyphs（页眉、cue、页脚文案）
      → render → PDF bytes
```

不写 `/Info`。页眉不读 `title()`。

## 8. 展示合同（display + captions）

### 8.1 角色

每个节点恰好一个角色。`SUBJECT`、`SHAREHOLDER`（入边锥）、`INVESTMENT`（出边锥）。

### 8.2 框内行

空白字段 strip 后为空则跳过该行。比例**不进框**。

| 角色 | 行顺序 |
| --- | --- |
| `SUBJECT` / `SHAREHOLDER` | 名称。法人姓名（无「法定代表人：」前缀） |
| `INVESTMENT` | 名称。有实缴金额则 `实缴 {金额}` 或 `实缴 {金额} {币种}`（仅币种不画行）。所在地原文一行。核准日期原文一行。不画法人 |

折叠占位按股东配方，通常只有名称一行。实线，不可点击。

### 8.3 线旁比例

非主体且解析出展示比例时，文案 `xx.xx%`（两位小数）。

- 同一节点只留第一条 fact。
- 股东。`LabelHug::Bottom`（框下沿外）。基线是 `node.y + height + ascent(9 pt) + RATIO_LABEL_GAP_Y_MM`。
- 投资。`LabelHug::Top`（框上沿外）。基线是 `node.y - descent(9 pt) - RATIO_LABEL_GAP_Y_MM`。这里的 descent 是基线以下的正距离，不含 `LINE_GAP_PT`。禁止用整段行高。
- 贴 stub 右侧 `RATIO_LABEL_OFFSET_X_MM`（1.2 mm）。先找竖向 stub。股东再找侧向 stub。投资没有侧向 stub。贴边容差 0.05 mm。
- 主体与升格后的页主体不画。
- 无贴节点 stub 则不画。节点不在本页放置结果里也不画。标签不进布局包围盒，不参与 fits。

### 8.4 公司底色

灰底当 `node_type == Company`，或 `node_type` 为空且 `legal_person` strip 后非空。`Person` 一律白底。投资锥不画法人行，但灰底仍看原始 `node_type` / `legal_person`。

### 8.5 页眉与页脚文案

主标题（居中）。

| 页角色 | 主标题 |
| --- | --- |
| 整图 / 股东总览首页 | `股权架构图: {图表主体}` |
| 投资总览首页 | `对外投资: {图表主体}` |
| 同侧续页（侧内序号 ≥ 2） | `续页{n}: {图表主体}` |
| 切开附页 | `附件{N}: {本页根}`（N 为 PDF 页码，与「详见附件 N」相同） |

副标题。

| 页角色 | 副标题 |
| --- | --- |
| 整图一页 | `各股东/合伙人 → {主体}`。有投资时再接 ` → 对外投资` |
| 股东总览 / 续页 | `各股东/合伙人 → {主体}` |
| 投资总览 / 续页 | `{主体} → 对外投资` |
| 切开附页 | `{根}持有{上一层} 10.00%` 或 `{上一层}投资{根} 10.00%`。无比例则无百分号。无父边则只写根名 |

页脚。`第 {i} / {n} 頁`（「頁」保持与 Python 相同）。

主标题槽顶在页边距（`MAIN_TITLE_TOP_GAP_MM` 为 0）。主标题行与副标题槽顶之间是 `TITLE_LINE_GAP_MM`（1.5 mm）。页眉超出可印宽度时按 Unicode 标量二分截断，并在末尾加 U+2026。省略号本身宽于可印宽度时，该行为空字符串。

续页虚框两行。股东侧 `股东详见` + `附件{N}`。投资侧 `对外投资详见` + `附件{N}`。

overflow 框内第二行定稿后为 `详见附件{N}`。合成名 `其他股东(k家)` / `其他投资(k家)`。

## 9. 测量合同（skrifa）

### 9.1 字体

- 入参 `Font`。内部持有 `Arc<Vec<u8>>`。`Font::new` 包住调用方拥有的字节并当场解析。`Font::from_shared` 保存已有的 `Arc` 并当场解析。解析失败 → `FONT_ERROR`，原因在 `source()`。拒绝 TTC/OTC。
- `Font::new` 只做打开单字体时已有的检查。空字节、打不开 face、读不到 `head` 或 `hhea`、upem 不是有限正数、ascent 不大于 0、descent 不小于 0，都在这里失败。字形没有 advance 仍在 `to_pdf` 报 `FONT_ERROR`。krilla 拒绝字体或序列化失败也在 `to_pdf` 报 `FONT_ERROR`。
- `Font` 不可变，可以跨图复用。`Debug` 只输出字节长度和 units per em。`Font` 不可比较。
- 测宽前对每个字符 ensure glyph。缺字 → `MISSING_GLYPH`。`subject` 为 `Character`。
- 行高用 ascent+descent 比例尺 + `LINE_GAP_PT`。与绘制共用同一 charmap / advance 管线。禁止 krilla `draw_text` 反推宽度。

### 9.2 节点框

- 普通宽 = `style.node_width_mm`。缺省 `38` mm。主体宽 = `min(width * 2.55, 80)` mm。
- 内容宽 = 框宽 − `2 * PADDING_X_MM`。名称可折行。名称字号主体 `12` pt，普通 `10` pt。法人/详情 `9` pt。
- 高度 = 上下 padding + 各折行行高，再与两行名称下限取 max。主体用 `SUBJECT_PADDING_Y_MM`。
- `NODE_TOO_TALL` 比较对象是**肖像**可印高导出的 `node_height_limit_mm = (printable_h - LAYER_SPACING_MM) / 2`。升格附页主体时 Python 不再做该检查。
- overflow 合成框。装箱时第二行按名称字号预留空行。页序定后写入 `详见附件{N}`，不改几何。

## 10. 分页与装箱合同

算法细节对照 `paging.py` 与 `layout/*`。下列为必须保持的产品行为。

### 10.1 纸张与朝向

- 固定 A4。`210 × 297` mm。边距默认 `8` mm。页眉高 `18` mm，页脚高 `6` mm。
- 可印宽/高 = 页尺寸减双边距再减页眉页脚。
- 舒适带 = 可印高 × `0.85`。弹性带 = 可印高 × `0.95`。容差 `FIT_EPSILON_MM`（0.5）。
- 分叉页偏好横版。纯链页偏好竖版。先舒适带再弹性带。每带先试偏好朝向。

### 10.2 整图一页 vs 两侧分页

某朝向同时物理放下且不超过弹性高度 → 可整图一页。否则股东侧与投资侧分别分页。

### 10.3 直接层与家族

- 页内主体（原始或升格）的直接关系一层允许折行。更深层级默认单行，高度不超过弹性带。
- 每个直接孩子按「主体 + 该孩子完整子锥」单独量。超弹性高 → 大族。本页留 stub，整锥进附页。否则小族整锥内联。
- 可印宽够时，有子族的父按子族宽以 `FAMILY_GAP_MM` 开槽。同高 X 重叠的异目标横 bus 以 `BUS_STAGGER_MM` 错开。
- 股东竖 stub 至少 `SHAREHOLDER_STUB_MIN_MM`（6.5 mm）。

### 10.4 overflow 合成节点

直接层弹性高度放不下时，沿「人 → 叶子公司 → 小族 → 大族」挪走直接孩子。同类里输入序靠后的先走。每步至少留一个真实直接孩子。

插入合成节点。

- 身份是 `NodeOrigin::Overflow { subject, first_moved, moved_count }`。
- 名称 `其他股东(k家)` 或 `其他投资(k家)`。
- 虚线描边。整框可点到续页。不是 ContCue。
- 若「主体 + 一个真实孩子 + 合成位」两朝向弹性都放不下，该页退回只要求物理 fits。

overflow 链择优（页数、降级、续页利用率 0.35、留下孩子数等）对照源包。稀疏分桶取 `gap = 0.35 - utilization`。`gap <= 0` 得 0。否则对 `gap / 0.05` 向零截断。`0.35 / 0.05` 的截断结果是 6。禁止按 0.05 逐次累加。

### 10.5 续页 ContCue

- 仅在有锚点时占虚框。锚点是大族 stub，以及附页上仍被切开的 stub。
- 股东优先锚点上方。投资优先下方。
- 虚框与节点、边一并进包围盒与 fits。不得与节点/其它虚框/边折线相交。上下各留 `CONT_CUE_GAP_MM`。
- 上下因挡道放不下时先撑行距，再排。仍放不下则左右放置并画指向锚点的虚线箭头。引线两端是相对边的中点。颜色与边线相同（0.2 灰）。线宽 1.0 pt。虚线节距 4 和 3。箭头长 4.5 pt，两侧各 30 度，填充同色，不描边。
- overflow 节点不挂 ContCue。
- 股东侧与投资侧都有页时，股东第 1 页主体下方额外画「对外投资详见 / 附件{N}」（N = 投资侧首页）。整图一页不画。股东续页不重复。

### 10.6 切开附页

从该股东起画上游树，不再重复原主体。该股东升为页主体（主体框尺寸、不显示占股比例、按新宽重折行）。

## 11. 绘制合同（krilla）

### 11.1 画什么

每页依次（概念顺序）：页眉主副标题、边（含箭头）、节点填充与描边、框内文字、比例标签、ContCue、页脚。overflow / ContCue 挂可点链接到目标页。

### 11.2 节点外观

- 圆角半径 `NODE_CORNER_RADIUS_MM`（2.2 mm）。填充与描边各画一次。虚线只包描边（overflow 与续页框、续页引线）。
- 节点框与续页框描边是黑色。边线、边箭头填充、续页引线和引线箭头填充是 0.2 灰（字节 51）。框内文字是黑色。公司填充是 `COMPANY_SHADE`（247/255）。个人填充是白色。
- 文字裁切用同一圆角。链接命中区用外接矩形。
- 框内每一行在框宽内**水平居中**（折行宽仍按 `内容宽 = 框宽 − 2 * PADDING_X_MM` 测量）。续页框文字在槽内水平居中。
- 主体描边更粗（`SUBJECT_BORDER_PT` 1.5 vs `NORMAL_BORDER_PT` 0.8）。续页框另用 `CONT_CUE_LINE_WIDTH_PT` 1.0 与 `CONT_CUE_CORNER_MM` 1.0，虚线节距 `CONT_CUE_DASH_ON_PT` / `CONT_CUE_DASH_OFF_PT`（4 / 3）。

### 11.3 坐标

Python/reportlab 与 krilla 坐标约定可能不同。Rust 侧在适配层做一次映射，使**视觉上**页眉在上、股东在主体上方、投资在下方，与 Python 样张页结构一致。不在公共 API 暴露后端坐标。

测字与绘制必须同一 glyph 管线。

## 12. 版式常量（须对齐）

来源 `units.py`。实现可 `pub(crate)`，不必全部导出。

| 名 | 值 |
| --- | --- |
| `PAGE_WIDTH_MM` | 210 |
| `PAGE_HEIGHT_MM` | 297 |
| `NODE_WIDTH_MM` | 38 |
| `SUBJECT_WIDTH_FACTOR` | 2.55 |
| `SUBJECT_PADDING_Y_MM` | 3.5 |
| `SUBJECT_NAME_FONT_PT` | 12 |
| `MARGIN_MM` | 8 |
| `HEADER_HEIGHT_MM` | 18 |
| `FOOTER_HEIGHT_MM` | 6 |
| `PADDING_X_MM` | 2 |
| `PADDING_Y_MM` | 1.5 |
| `NAME_FONT_PT` | 10 |
| `DETAIL_FONT_PT` | 9 |
| `RATIO_FONT_PT` | 9 |
| `NODE_CORNER_RADIUS_MM` | 2.2 |
| `RATIO_LABEL_OFFSET_X_MM` | 1.2 |
| `RATIO_LABEL_GAP_Y_MM` | 0.4 |
| `SHAREHOLDER_STUB_MIN_MM` | 6.5 |
| `PAID_IN_LABEL` | `实缴` |
| `LINE_GAP_PT` | 2 |
| `LAYER_SPACING_MM` | 18 |
| `NODE_SPACING_MM` | 6 |
| `ROW_GAP_MM` | 8 |
| `FAMILY_GAP_MM` | 12 |
| `FIT_EPSILON_MM` | 0.5 |
| `BUS_STAGGER_MM` | 2 |
| `COMFORT_HEIGHT_RATIO` | 0.85 |
| `ELASTIC_HEIGHT_RATIO` | 0.95 |
| `MAX_NODES` | 100 |
| `RATIO_MIN` / `RATIO_MAX` | 0 / 100 |
| `MAX_MARGIN_MM` | 40 |
| `MIN_NODE_WIDTH_MM` / `MAX_NODE_WIDTH_MM` | 20 / 80 |
| `EDGE_WIDTH_PT` | 0.7 |
| `ARROW_SIZE_PT` | 6 |
| `SUBJECT_BORDER_PT` / `NORMAL_BORDER_PT` | 1.5 / 0.8 |
| `MAIN_TITLE_PT` / `SUB_TITLE_PT` | 16 / 8 |
| `MAIN_TITLE_TOP_GAP_MM` | 0 |
| `TITLE_LINE_GAP_MM` | 1.5 |
| `EDGE_GRAY` | 0.2（通道字节 51） |
| `INK_BYTE` | 0 |
| `COMPANY_SHADE` | 247/255 |
| `CONT_CUE_LINE_WIDTH_PT` | 1.0 |
| `CONT_CUE_CORNER_MM` | 1.0 |
| `CONT_CUE_DASH_ON_PT` / `CONT_CUE_DASH_OFF_PT` | 4 / 3 |
| `LEADER_ARROW_LENGTH_FACTOR` | 0.75（箭头长 4.5 pt） |
| `LEADER_ARROW_HALF_ANGLE_DEG` | 30 |
| `FOOTER_TEXT_PT` | 9 |
| `CONT_CUE_TEXT_PT` | 6 |
| `CONT_CUE_GAP_MM` | 3 |
| `CONT_CUE_PAD_MM` | 2 |
| `CONT_CUE_MIN_WIDTH_MM` | 18 |
| `CONT_CUE_LINE_COUNT` | 2 |
| `PAGE_NUMBER_INSET_MM` | 5 |

## 13. 可测切片

每个切片结束须可测。

| § | 交付 | 验证 |
| --- | --- | --- |
| §13.1 | `request`、`error`、`graph` 的协议与折叠、`Chart::new` | 错误码与折叠结果。`cargo test -p pdfcraft-orgchart` |
| §13.2 | `content` 框内行、比例事实、`captions` 字符串 | 对照框内行与文案的期望字面量 |
| §13.3 | `font` 的 skrifa 测宽、`page` 的节点框、`MISSING_GLYPH`、`NODE_TOO_TALL`、`FONT_ERROR` | 缺字样张与打不开的字体 |
| §13.4 | `paginate` 与 `layout`、overflow、cue、附页升格 | 页数、主副标题、角色、附件号 |
| §13.5 | `render` 与 `Chart::to_pdf` | `%PDF` 冒烟。`pdftotext` 抽页眉、附件号、关键节点名。页数与附件编号与 Python 对照（非像素门禁） |

Python 对照命令：

```bash
cd /mnt/data/dev/hcsw-pdfcraft/backend
uv run pytest packages/pdfcraft-equity/tests -q
```

Rust 门禁：

```bash
cargo fmt --all -- --check
cargo clippy -p pdfcraft-orgchart --all-targets -- -D warnings
cargo test -p pdfcraft-orgchart
cargo doc -p pdfcraft-orgchart --no-deps
```

### 13.1 模块

`error`、`units`、`captions`、`request`、`font`、`graph`、`content`、`page`、`layout`、`paginate`、`render`、`chart`。`lib` 只重导出 §3 的公共类型。依赖方向见结构规格 §3。

## 14. 已确认答复

| 题 | 答复 | 来源 |
| --- | --- | --- |
| Q1 命名 | `Chart`、`ChartRequest`、`ChartStyle`、`Node`、`Edge`、`PaidIn`、`Font`、`Error`、`ErrorCode`、`ErrorSubject` | 提案书 §13 |
| Q2 中间 API | 不进稳定公共面 | 口头 |
| Q3 `to_dict` | 不提供 | 默认 |
| Q4 `NodeType` 字符串 | `PERSON`、`COMPANY` | 默认 |
| Q5 折叠身份 | 身份由 `NodeOrigin` 表达。已有占位由 `folded_holders_of` 声明。id 前缀没有特殊含义 | 提案书 §13 |
| Q6 字体 | bytes。拒 TTC/OTC。OTF 能开则开 | 默认 |
| Q7 文案 | 简体词表。页脚「頁」保留 | 默认 |
| Q8 `/Info` | 不写 | 默认 |
| Q9 `message` | 只稳定 `code()`。`message()` 给日志 | 提案书 §13 |
| Q10 验收 | 文本可抽取 + 页结构。无几何截图门禁 | 默认 |
| Q11 `max_nodes` | 在 `Chart::new` 内，折叠之后计数，并重跑全部协议规则 | 提案书 §13 |
| Q12 `top_n` | 类型是 `Option<NonZeroUsize>`。`None` 不折叠。`Some(n)` 的 `n` 在 `1..=max_nodes`。`top_n == max_nodes` 合法 | 提案书 §13 |
| Q13 外观缺省 | `ChartStyle::default` 为宽 38 mm、边距 8 mm。字段不是 `Option` | 提案书 §13 |
| Q14 纸张朝向 | 固定 A4，启发式朝向 | 默认 |
| Q15 编译与出图 | `Chart::new(ChartRequest) -> Result<Chart, Error>` 按值接收请求。`Chart::to_pdf(&self, &Font)` 出 PDF。`Chart` 不持有字体 | 提案书 §13 |
| Q16 标题 | `Chart::title()` 恒为 `{主体名}股权架构图`。另有 `Chart::subject_name()` | 提案书 §13 |
| Q17 外观类型 | `ChartStyle { node_width_mm, margin_mm }`。`Default` 为 38 与 8 | 提案书 §13 |
| Q18 字体类型 | `Font::new` 与 `Font::from_shared` 返回 `Result`。解析失败是 `FONT_ERROR` | 提案书 §13 |
| Q19 边与实缴 | `Edge { holder, held, ratio }`。`Node.paid_in` 是 `Option<PaidIn>`。`PaidIn { amount, currency }` | 提案书 §13 |
| Q20 错误访问器 | `Error` 字段私有。访问器是 `code()`、`message()`、`subject()`、`source()`。`ErrorSubject` 标 `#[non_exhaustive]` | 提案书 §13 |
| Q21 字面量例外 | `ChartStyle` 是样式结构体。`Node`、`Edge`、`PaidIn`、`ChartRequest` 是协议记录，公有字段，不标 `non_exhaustive`。`Chart`、`Font`、`Error` 字段私有 | 提案书 §13 |

## 15. 实现时仍须打开的源文件

规格不替代下列算法源码。实现对应切片时对照。

| 主题 | 源文件 |
| --- | --- |
| 校验 / 折叠 | `graph.py`、`prepare.py` |
| 展示 | `display.py`、`captions.py` |
| 测量 | `measuring.py` |
| 分页 | `paging.py`、`_family_search.py`、`cont_cues.py`、`cue_slots.py` |
| 布局 | `layout/engine.py`、`ranks.py`、`groups.py`、`place.py`、`route.py`、`wrap_detect.py` |
| 绘制 / 链接 | `render.py`、`annotations.py`、`geometry.py` |
| 常量 | `units.py` |
