# quire-orgchart 规格（整包迁移）

| 项 | 值 |
| --- | --- |
| 日期 | 2026-10-13 |
| 状态 | **整包行为合同已写全（reference）**。§3 到 §5 与 §14 产品答复已确认。layout 装箱的逐步算法仍以源包实现为对照，本文件钉可观察合同与常量，不逐函数翻译 |
| 相关 | [`workspace-crates-spec.md`](workspace-crates-spec.md)；源包 `/mnt/data/dev/hcsw-pdfcraft/backend/packages/pdfcraft-equity` |
| 意图来源 | `pdfcraft-equity` 行为合同。Rust 惯用重实现，见 §0 |
| 范围 | 公共 API、协议校验、折叠、展示、测量、分页装箱、绘制、错误码、常量、可测切片 |

本文是 **reference**。实现文件名不钉死。

**完整度判定。** 有了本文 + 源包测试，可以实现并验收整包到 `generate_pdf`。layout / paging 的内部搜索与启发式（家族开槽、bus 错开、overflow 链择优）以实现对照 `paging.py` / `layout/*` 为主。规格钉住输入输出、页角色、文案、错误码与数值常量，避免发明产品规则。

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

`quire-orgchart` 接收股权 / 对外投资图的节点边协议与字体字节，输出矢量 PDF 字节。页眉用简体中文角色文案。调用方自己写 PDF `/Info`（若需要）。

不依赖 `quire-pdf` 或 `quire-forms`。测字 `skrifa =0.48.0`。出 PDF `krilla =0.8.2`（`default-features = false`，按需开 feature，关 `simple-text`）。

## 2. 范围

### 2.1 做

- 双锥协议。`from_id` 持有 `to_id`。股东在上，对外投资在下。
- `validate_request`。不加载字体，含 `top_n` 折叠错误。
- `generate_pdf`。prepare → validate → display → measure → paginate → render。
- 错误码字符串与 Python `EquityErrorCode.value` 一致。
- 类型名去掉 `Pdfcraft` 前缀。公共类型缩短为 `ChartRequest` 等（§14）。
- 字体只接受内存字节。

### 2.2 不做

- 依赖或反向依赖 `quire-pdf` / `quire-forms`。
- 环状交叉持股、跨页连线、自定义节点行、换纸张、换词表。
- JSON / serde 作主接口。
- 进程级默认字体路径。
- 几何截图作 v1 门禁（§14 Q10）。

### 2.3 与其它 crate

| | `quire-pdf` | `quire-forms` | `quire-orgchart` |
| --- | --- | --- | --- |
| 输入 | `DocumentSpec` | 已有 PDF + 字段值 | 节点边协议 + 字体字节 |
| PDF 栈 | krilla（仅 backend） | lopdf | krilla（本 crate 直接用） |
| 测字 | skrifa | skrifa | skrifa `0.48.0` |

旧 workspace 规格里「可选依赖 `quire-pdf`」作废。文档后改。

## 3. 公共 API（已确认）

### 3.1 入口

```rust
pub fn validate_request(request: &ChartRequest) -> Result<(), Error>;

pub fn generate_pdf(
    request: &ChartRequest,
    font: &FontBytes,
) -> Result<Vec<u8>, Error>;

pub fn document_title(request: &ChartRequest) -> Result<String, Error>;
```

- `validate_request` 跑 `prepare_request` 并丢弃结果。不读字体。
- `generate_pdf` 见 §7。不写 `/Info`。
- `document_title` 只查主体名。不做其余协议校验。主体缺失 → `SUBJECT_NOT_FOUND`。`options.title` 为 `None` 时回落 `{主体名}股权架构图`。`Some("")` 原样返回。

### 3.2 请求类型

| Python | Rust |
| --- | --- |
| `EquityChartRequest` | `ChartRequest` |
| `EquityNode` | `Node` |
| `EquityEdge` | `Edge` |
| `EquityChartOptions` | `ChartOptions` |
| `NodeType` | `NodeType` |
| `PdfcraftEquityError` | `Error` |
| `EquityErrorCode` | `ErrorCode` |

```rust
pub struct ChartRequest {
    pub subject_id: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub options: ChartOptions,
}

pub struct Node {
    pub id: String,
    pub name: String,
    pub legal_person: Option<String>,
    pub ratio: Option<f64>,
    pub node_type: Option<NodeType>,
    pub paid_in_amount: Option<String>,
    pub currency: Option<String>,
    pub location: Option<String>,
    pub approval_date: Option<String>,
}

pub struct Edge {
    pub from_id: String,
    pub to_id: String,
    pub ratio: Option<f64>,
}

pub struct ChartOptions {
    pub node_width_mm: Option<f64>,
    pub margin_mm: Option<f64>,
    pub title: Option<String>,
    pub max_nodes: Option<usize>,
    pub top_n: Option<usize>,
}

pub enum NodeType {
    Person,
    Company,
}

pub struct FontBytes {
    pub data: Vec<u8>,
}
```

- `node_type` 对应 Python `type`。对外字符串用 `PERSON` / `COMPANY`。
- 可选字符串空串或纯空白在展示层当缺省。`id` / `name` / `subject_id` 校验仍要求 strip 后非空。
- 金额与日期格式不校验。

### 3.3 导出常量

| 常量 | 值 |
| --- | --- |
| `MAX_NODES` | `100` |
| `MIN_NODE_WIDTH_MM` | `20.0` |
| `MAX_NODE_WIDTH_MM` | `80.0` |
| `MAX_MARGIN_MM` | `40.0` |

options 字段保持 `Option` 穿过 prepare。缺省只在 `generate_pdf` 代入。普通宽 `38` mm，边距 `8` mm。

### 3.4 错误类型

```rust
pub struct Error { /* code, message */ }
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
```

`ErrorCode::as_str` 映射到 `INVALID_PROTOCOL`、`CYCLE_DETECTED`、`SUBJECT_NOT_FOUND`、`TOO_MANY_NODES`、`FONT_ERROR`、`MISSING_GLYPH`、`NODE_TOO_TALL`、`LAYOUT_FAILED`。

不提供 `to_dict`。HTTP 状态不属于本库。

### 3.5 内部 API（`pub(crate)`）

- `ValidatedGraph`、`validate`、`prepare_request`。
- 保留前缀 `__overflow__:`（用户提交非法）、`__other__:`（折叠合成，校验允许用户自带）。

## 4. 图校验合同

见既有行为。摘要如下。

1. 空白 `subject_id` → `INVALID_PROTOCOL`。
2. options 边界。`max_nodes` ∈ `1..=100`。`top_n` ∈ `0..=cap`。`margin_mm` ∈ `(0, 40]`。`node_width_mm` ∈ `[20, 80]`。
3. `nodes.len() > cap` → `TOO_MANY_NODES`。
4. 节点 id/name、重复、`__overflow__:` 前缀。
5. 主体不在表 → `SUBJECT_NOT_FOUND`。
6. 悬空边、自环、重复边、比例范围。
7. 有向环 → `CYCLE_DETECTED`。
8. 双锥连通与 `SH→INV` 跨锥。
9. 无节点比例时布局父边必须唯一。

双锥与比例解析细则与源包 `graph.py` 一致（股东入边锥、投资出边锥、父边选择四步）。

## 5. prepare 合同（`top_n`）

只折叠股东锥。`None` / `0` 恒等。`>= 1` 时每个股东子都要有展示比例。按比例降序、同比例按 `nodes` 序，保留前 N，其余合成 `__other__:{parent}`，名 `其他股东(k家)`。存活股东重算后删不可达节点。幂等。投资侧不动。

折叠后再 `validate` 计入 `max_nodes`（可出现校验过、生成挂）。

## 6. 错误码触发点（整包）

| 码 | 触发 |
| --- | --- |
| `INVALID_PROTOCOL` | §4 / §5 协议与折叠错误 |
| `CYCLE_DETECTED` | 有向环 |
| `SUBJECT_NOT_FOUND` | 主体不在节点表（含 `document_title`） |
| `TOO_MANY_NODES` | 节点数超上限（含折叠后第二次 validate） |
| `FONT_ERROR` | 字体 bytes 无法当单字体打开、TTC/OTC、解析失败 |
| `MISSING_GLYPH` | 测宽或绘制前 `ensure_glyphs` 缺字 |
| `NODE_TOO_TALL` | 折行后节点高超过 `node_height_limit_mm(肖像可印高)` |
| `LAYOUT_FAILED` | 分页/布局内部不变量破坏（非法 overflow id、缺锚点框、bus 键非法等） |

`message` 尽量贴近 Python，不保证逐字。

## 7. `generate_pdf` 管线

```text
ChartRequest
  → prepare_request → ChartRequest'
  → validate → ValidatedGraph
  → build_display + ratio_facts
  → 解析 margin / node_width 缺省
  → FontMetrics(font bytes)
  → build_node_boxes（用肖像可印高做 NODE_TOO_TALL）
  → paginate（可横竖切换、overflow、cue、升格附页）
  → ensure_glyphs（页眉、cue、页脚文案）
  → render → PDF bytes
```

不写 `/Info`。不用 `document_title` / `options.title` 覆盖页眉。

## 8. 展示合同（display + captions）

### 8.1 角色

每个节点恰好一个角色。`SUBJECT`、`SHAREHOLDER`（入边锥）、`INVESTMENT`（出边锥）。

### 8.2 框内行

空白字段 strip 后为空则跳过该行。比例**不进框**。

| 角色 | 行顺序 |
| --- | --- |
| `SUBJECT` / `SHAREHOLDER` | 名称。法人姓名（无「法定代表人：」前缀） |
| `INVESTMENT` | 名称。有实缴金额则 `实缴 {金额}` 或 `实缴 {金额} {币种}`（仅币种不画行）。所在地原文一行。核准日期原文一行。不画法人 |

`__other__:` 节点按股东配方，通常只有名称一行。实线，不可点击。

### 8.3 线旁比例

非主体且解析出展示比例时，文案 `xx.xx%`（两位小数）。

- 股东。`LabelHug::Bottom`（框下沿外）。
- 投资。`LabelHug::Top`（框上沿外）。
- 贴 stub 右侧 `RATIO_LABEL_OFFSET_X_MM`（1.2 mm）。
- 主体与升格后的页主体不画。
- 无贴节点 stub 则不画。标签不进布局包围盒，不参与 fits。

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

续页虚框两行。股东侧 `股东详见` + `附件{N}`。投资侧 `对外投资详见` + `附件{N}`。

overflow 框内第二行定稿后为 `详见附件{N}`。合成名 `其他股东(k家)` / `其他投资(k家)`。

## 9. 测量合同（skrifa）

### 9.1 字体

- 入参 `FontBytes`。拒绝 TTC/OTC → `FONT_ERROR`。
- 无法解析 → `FONT_ERROR`。
- 测宽前对每个字符 ensure glyph。缺字 → `MISSING_GLYPH`（消息可含文本前缀）。
- 行高用 ascent+descent 比例尺 + `LINE_GAP_PT`。与绘制共用同一 charmap / advance 管线。禁止 krilla `draw_text` 反推宽度。

### 9.2 节点框

- 普通宽 = options 或 `38` mm。主体宽 = `min(width * 2.55, 80)` mm。
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

- id `__overflow__:{主体}:{首个被挪走的孩子}`。
- 名称 `其他股东(k家)` 或 `其他投资(k家)`。
- 虚线描边。整框可点到续页。不是 ContCue。
- 若「主体 + 一个真实孩子 + 合成位」两朝向弹性都放不下，该页退回只要求物理 fits。

overflow 链择优（页数、降级、续页利用率 0.35、留下孩子数等）对照源包。

### 10.5 续页 ContCue

- 仅在有锚点时占虚框。锚点是大族 stub，以及附页上仍被切开的 stub。
- 股东优先锚点上方。投资优先下方。
- 虚框与节点、边一并进包围盒与 fits。不得与节点/其它虚框/边折线相交。上下各留 `CONT_CUE_GAP_MM`。
- 上下因挡道放不下时先撑行距，再排。仍放不下则左右放置并画指向锚点的虚线箭头。
- overflow 节点不挂 ContCue。
- 股东侧与投资侧都有页时，股东第 1 页主体下方额外画「对外投资详见 / 附件{N}」（N = 投资侧首页）。整图一页不画。股东续页不重复。

### 10.6 切开附页

从该股东起画上游树，不再重复原主体。该股东升为页主体（主体框尺寸、不显示占股比例、按新宽重折行）。

## 11. 绘制合同（krilla）

### 11.1 画什么

每页依次（概念顺序）：页眉主副标题、边（含箭头）、节点填充与描边、框内文字、比例标签、ContCue、页脚。overflow / ContCue 挂可点链接到目标页。

### 11.2 节点外观

- 圆角半径 `NODE_CORNER_RADIUS_MM`（2.2 mm）。填充与描边各画一次。虚线只包描边（overflow）。
- 文字裁切用同一圆角。链接命中区用外接矩形。
- 框内每一行在框宽内**水平居中**（折行宽仍按 `内容宽 = 框宽 − 2 * PADDING_X_MM` 测量）。
- 主体描边更粗（`SUBJECT_BORDER_PT` 1.5 vs `NORMAL_BORDER_PT` 0.8）。

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
| `FOOTER_TEXT_PT` | 9 |
| `CONT_CUE_TEXT_PT` | 6 |
| `CONT_CUE_GAP_MM` | 3 |
| `CONT_CUE_PAD_MM` | 2 |
| `CONT_CUE_MIN_WIDTH_MM` | 18 |
| `CONT_CUE_LINE_COUNT` | 2 |
| `PAGE_NUMBER_INSET_MM` | 5 |

## 13. 可测迁移切片

每个 §13 切片结束须可测。目标架构一次收敛，不为过渡态留兼容层（outcome-oriented）。

| § | 交付 | 验证 |
| --- | --- | --- |
| §13.1 | 协议类型、`Error`、`validate`、`prepare`、`validate_request`、`document_title` | 移植 `test_graph` / `test_prepare` 行为。`cargo test -p quire-orgchart` |
| §13.2 | display 框内行、`ratio_facts`、captions 字符串纯函数 | 对照 `test_display` / captions 期望字面量 |
| §13.3 | `FontBytes` + skrifa 测宽、`build_node_boxes`、`MISSING_GLYPH` / `NODE_TOO_TALL` / `FONT_ERROR` | `test_measuring` 行为。缺字样张 |
| §13.4 | paginate + layout 引擎、overflow、cue、附页升格 | `test_paging` / `test_layout` / cue 测试的可观察页结构 |
| §13.5 | krilla `render` + `generate_pdf` | `%PDF` 冒烟。`pdftotext` 抽页眉/附件号/关键节点名。页数与附件编号与 Python 对照（非像素门禁） |

Python 对照命令：

```bash
cd /mnt/data/dev/hcsw-pdfcraft/backend
uv run pytest packages/pdfcraft-equity/tests -q
```

Rust 门禁：

```bash
cargo fmt --all -- --check
cargo clippy -p quire-orgchart --all-targets -- -D warnings
cargo test -p quire-orgchart
cargo doc -p quire-orgchart --no-deps
```

### 13.1 建议 crate 模块（非强制文件名）

`error`、`model`、`graph`、`prepare`、`display`、`captions`、`measure`、`layout`、`paging`、`render`、`lib` 重导出稳定面。

## 14. 已确认答复

| 题 | 答复 | 来源 |
| --- | --- | --- |
| Q1 命名 | 缩短为 `ChartRequest` / `Node` / `Edge` / `ChartOptions` / `Error` / `ErrorCode` | 口头 |
| Q2 中间 API | 不进稳定公共面 | 口头 |
| Q3 `to_dict` | 不提供 | 默认 |
| Q4 `NodeType` 字符串 | `PERSON` / `COMPANY` | 默认 |
| Q5 `__other__:` | 校验允许，折叠冲突报错 | 默认 |
| Q6 字体 | bytes。拒 TTC/OTC。OTF 能开则开 | 默认 |
| Q7 文案 | 简体词表。页脚「頁」保留 | 默认 |
| Q8 `/Info` | 不写 | 默认 |
| Q9 `message` | 只稳定 `code` | 默认 |
| Q10 验收 | 文本可抽取 + 页结构。无几何截图门禁 | 默认 |
| Q11 `max_nodes` | 折叠后再计 | 口头 |
| Q12 `top_n` | `None` 与 `Some(0)` 都不折叠 | 默认 |
| Q13 options | 保持 `Option`，缺省在 `generate_pdf` 代入 | 默认 |
| Q14 纸张朝向 | 固定 A4，启发式朝向 | 默认 |

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
