# 仅对外投资图「整图一页」页眉误用「股权架构图」

- **日期：** 2026-11-04
- **报告方：** `hcsw-pdfcraft`（调用方，仓库 `/mnt/data/dev/hcsw-pdfcraft`）
- **影响 crate：** `pdfcraft-orgchart` ≥ 0.3.0
- **症状层：** 页眉主标题 / 副标题（不是 PDF `/Info`，也不是下载文件名）
- **状态：** 待引擎侧讨论 / 定稿

---

## 1. 现象

调用方把**股东图**和**对外投资图**拆成两次 `Chart::new` + `to_pdf`，再合并进审前套件 PDF。

最终套件里能看到两张业务图（各一页）：

1. 股权架构图页（仅股东锥）— 页眉合理。
2. **对外投资页（仅投资锥）— 页眉错误：**

| 槽位 | 实际看到 | 调用方期望（投资总览角色） |
| --- | --- | --- |
| 主标题 | `股权架构图: 00000955 The Educationist Limited` | `对外投资: 00000955 The Educationist Limited` |
| 副标题 | `各股东/合伙人 → 00000955 The Educationist Limited → 对外投资` | `00000955 The Educationist Limited → 对外投资` |

节点内容本身是对的：主体下方是「长飞光纤…」「长江实业…」，带实缴 / 地点 / 核准日期。问题只在**页眉角色文案**。

> 说明「两页」与「整图一页」不矛盾：套件合并后一共两张图；**每一张图**进引擎时各自判断能否一页装下。对外投资这张图很小，引擎判成「整图一页」，于是套上整图页眉。

---

## 2. 期望行为（调用方立场）

当 `ChartRequest` **只有主体出边（投资）、没有主体入边（股东）**，且整图能装进一页时：

- 主标题应走 **投资总览** 角色：`对外投资: {主体名}`（见 `docs/pdfcraft-orgchart-spec.md` §8.5「投资总览首页」）。
- 副标题应为 `{主体名} → 对外投资`，**不应**以 `各股东/合伙人 →` 开头（图里没有股东）。

若产品刻意规定「任何能一页装下的图都叫股权架构图」，请在规格里写死，并解释无股东时副标题为何仍写「各股东/合伙人」。当前调用方按投资总览角色理解。

---

## 3. 根因定位（调用方已核对）

问题在 **`pdfcraft-orgchart` 分页**，不在调用方组边。

相关代码（引擎）：

- `crates/pdfcraft-orgchart/src/paginate/session/mod.rs`
  - `Session::new`：`whole_main = equity_chart_title(subject)`（恒为「股权架构图」）
  - `whole_sub = whole_chart_subtitle(subject, has_investments)`（`has_investments` 只看出边）
  - `Session::run`：`whole_page_if_elastic` 成功则 `to_page`，**直接返回**，不进入 `pack_side(Side::Investment)`
- `crates/pdfcraft-orgchart/src/captions.rs`
  - `whole_chart_subtitle` 基底恒为 `各股东/合伙人 → {主体}`；有投资再接 ` → 对外投资`
  - `investment_chart_title` / `investment_page_titles` 只有分侧投资页才用

因此：**仅投资、且弹性带内一页装下 → 永远走整图页眉，永远不会出现 `对外投资:`。**

规格对照：

| 文档 | 内容 | 张力 |
| --- | --- | --- |
| `docs/pdfcraft-orgchart-spec.md` §8.5 | 整图 → `股权架构图:`；投资总览 → `对外投资:` | 「仅投资一页」落在哪一格未写清 |
| 同文件 §10.2 | 能放下且不超过弹性高度 → 整图一页；否则分侧 | **不要求**两侧都有节点 |
| 调用方历史设计 2026-09-22 §4.1 | 整图判定写「双侧」 | 与 §10.2「只看能不能放下」不一致 |

`Chart::title()` 恒为 `{主体名}股权架构图`，且页眉不读它（spec §3.1 / Q16）。调用方已不再传旧 `ChartOptions.title`。本问题与 `/Info` 无关。

---

## 4. 调用链路（本项目 → 引擎）

仓库：`/mnt/data/dev/hcsw-pdfcraft`

```text
POST /v1/internal/audit-engagement-prerequisite-package
  → domain prepare
  → pdfcraft_docs::audit_engagement::render_package_pdf
       ├─ checklist PDF
       ├─ ChartKind::Equity            → build_equity_chart_request
       │                                 → Chart::new → to_pdf   （仅股东边）
       └─ ChartKind::OutwardInvestment → build_outward_investment_request
                                         → Chart::new → to_pdf   （仅投资边）★ 本 issue
  → pdfcraft_post::merge_pdfs
  → SetMetadata { title: subject_title }   // 套件 /Info，不是页眉
```

组图代码：

- `backend/crates/pdfcraft-docs/src/templates/audit_engagement/package.rs`
- `backend/crates/pdfcraft-docs/src/templates/audit_engagement/equity.rs`
  - `SUBJECT_ID = "subject"`
  - 主体显示名 = `"{BRN} {transform(companyNameEn)} {companyNameZh}".trim()`
  - `Educationist Limited -The-` → `The Educationist Limited`
  - 对外投资：`Edge { holder: subject, held: uuid, ratio }` + `Node.paid_in` / `location` / `approval_date`

调用方**不会**把股东和投资塞进同一次 `ChartRequest`（产品决策：两张独立 PDF）。

---

## 5. HTTP 入参（调用方原始 body，可复现）

端点：`POST /v1/internal/audit-engagement-prerequisite-package`

下面是触发问题的完整业务 payload（与线上一致）。与图表无关的大字段已保留，便于端到端复现；引擎侧复现只需第 6 节的 `ChartRequest`。

```json
{
  "data": {
    "item": {
      "companyData": {
        "companyNameZh": "",
        "companyNameEn": "Educationist Limited -The-",
        "businessRegistrationNumber": "00000955",
        "principalOperatingAddress": "香港尖沙咀漆咸道南45-51號其士大廈803室"
      },
      "roughFinancialInformation": {
        "period": { "from": "2025", "to": "2026" }
      },
      "accountingInformation": {
        "annualReview": {
          "startDate": "2025-07-01",
          "endDate": "2026-03-31"
        },
        "closedAt": "2026-09-30",
        "annualReturnDate": "03-31"
      },
      "form": {
        "basic": {
          "auditCondition": "firstCooperationAndFirstAnnualReview",
          "accountingType": "generalTrade",
          "monthlyStatement": "completeAnnual",
          "firstCooperationAndFirstAnnualReview": {
            "businessModel": "hongKongOperation",
            "taxableProfit": "0",
            "localCurrency": "HKD"
          }
        },
        "businessProcessModel": {
          "mainProductsAndServices": "db",
          "model": ["traditionalTrade", "eCommerce"],
          "traditionalTrade": {
            "supplierDeliveryMethod": "supplierDeliveryToCustomer",
            "mainSupplier": [
              {
                "companyName": "sdfb",
                "countryName": "中国香港",
                "deliveryMethod": "supplierDeliveryToCustomer"
              }
            ],
            "mainCustomer": [
              { "companyName": "sfb s", "countryName": "中国香港" }
            ]
          },
          "relatedCompany": [
            { "companyName": "清空数据库, 按照昨天早上的, 做一次薄播种" }
          ],
          "eCommerce": {
            "mainSupplier": [
              {
                "companyName": "清空数据库, 按照昨天早上的, 做一次薄播种",
                "countryName": "中国香港",
                "deliveryMethod": "designatedReceivingLocation"
              }
            ],
            "eCommerceTradingPlatform": [
              { "tradingPlatform": "amazon" },
              { "tradingPlatform": "aliexpress" },
              { "tradingPlatform": "independentStation" },
              { "tradingPlatform": "alibaba" }
            ]
          }
        },
        "shareholder": [
          {
            "shareholderType": "legalPerson",
            "uuid": "648f7ee3-78b5-4a6e-8815-4402d932ab39",
            "companyName": "太古股份有限公司",
            "legalPersonName": "张三",
            "registeredLocation": "中国",
            "shareRatio": "40",
            "shareholder": [
              {
                "shareholderType": "noOtherShareholder",
                "uuid": "76eaa621-54fa-4f12-8ea4-6be3bbc638ab"
              }
            ]
          },
          {
            "shareholderType": "legalPerson",
            "uuid": "89052f22-ff05-4378-8992-46679ceddec5",
            "companyName": "中创新航科技集团股份有限公司",
            "legalPersonName": "李四",
            "registeredLocation": "中国",
            "shareRatio": "20",
            "shareholder": [
              {
                "shareholderType": "noOtherShareholder",
                "uuid": "fbaf148f-9d5d-4c94-86b1-73fbe5314b49"
              }
            ]
          },
          {
            "shareholderType": "naturalPerson",
            "uuid": "65a307b5-a356-4b9b-8e65-b9f6d30191f5",
            "name": "于文乐",
            "shareRatio": "30"
          },
          {
            "shareholderType": "naturalPerson",
            "uuid": "944c2959-91dd-4169-8427-ef185f893b0f",
            "name": "刘静瑜",
            "shareRatio": "10"
          }
        ],
        "outwardInvestment": [
          {
            "companyName": "长飞光纤科技股份有限公司",
            "shareRatio": "80",
            "paidInvestmentAmount": "800000",
            "currency": "HKD",
            "location": "中国",
            "approvalDate": "2026-06-06",
            "uuid": "0e4855d7-8e82-43f6-828e-7a2c7535bce3"
          },
          {
            "companyName": "长江实业有限公司",
            "shareRatio": "100",
            "paidInvestmentAmount": "10000000",
            "currency": "HKD",
            "location": "香港",
            "approvalDate": "2026-05-06",
            "uuid": "e64ab3d3-dda7-41d2-8a39-3205edd957df"
          }
        ]
      },
      "submitted": 4
    },
    "company": {
      "id": 102844260,
      "type": "私人股份有限公司",
      "nameZh": "教育有限公司",
      "nameEn": "Educationist Limited",
      "addressZh": {
        "part1": "香港",
        "part2": "尖沙咀",
        "part3": "加連威老道77號",
        "part4": "恩浩國際中心開麟中心8樓A2号"
      },
      "addressEn": {
        "part1": "No. A2, 8/F, Kolling Centre,",
        "part2": "77 Granville Road,",
        "part3": "Tsim Sha Tsui,",
        "part4": "HONG KONG"
      },
      "comment": "",
      "activeStatus": "仍注册",
      "chargeRegister": "",
      "dissolutionDate": null,
      "incorporationDate": "2025-07-28",
      "businessRegistrationNumber": "00000955"
    },
    "order": [],
    "banks": [{ "id": 180, "showBankName": "中国银行股份有限公司" }],
    "taxReturns": [],
    "directors": [
      { "id": 987, "showName": "于文乐", "gender": 1 },
      { "id": 995, "showName": "刘静瑜", "gender": 2 }
    ],
    "shareholders": {
      "naturalPerson": [
        {
          "showName": "于文乐",
          "fullNameZh": "于文乐",
          "fullNameEn": "WEN LE YI",
          "shareholder": { "sharesPercent": 0.55 }
        },
        {
          "showName": "刘静瑜",
          "fullNameZh": "刘静瑜",
          "fullNameEn": "JING YU LIU",
          "shareholder": { "sharesPercent": 0.1 }
        }
      ],
      "corporateBody": [
        {
          "nameZh": "太古股份有限公司",
          "nameEn": "SWIRE PACIFIC LTD",
          "showName": "太古股份有限公司",
          "shareholder": { "sharesPercent": 0.15 }
        },
        {
          "nameZh": "中创新航科技集团股份有限公司",
          "nameEn": "",
          "showName": "中创新航科技集团股份有限公司",
          "shareholder": { "sharesPercent": 0.2 }
        }
      ]
    },
    "filters": {
      "genders": [
        { "id": 0, "value": "" },
        { "id": 1, "value": "男" },
        { "id": 2, "value": "女" }
      ]
    }
  },
  "encrypt": false
}
```

套件因此会生成 **两张** orgchart PDF 再合并。本 issue 只针对其中 **OutwardInvestment** 那一次调用。

---

## 6. 本项目实际传给引擎的 `ChartRequest`（OutwardInvestment）

由 `build_outward_investment_request` 从上面 `form.outwardInvestment` 生成。等价于：

```rust
use pdfcraft_orgchart::{ChartRequest, Edge, Node, NodeType, PaidIn};

let subject_name = "00000955 The Educationist Limited";

let request = ChartRequest::new(
    "subject",
    vec![
        Node {
            node_type: Some(NodeType::Company),
            ..Node::new("subject", subject_name)
        },
        Node {
            ratio: Some(80.0),
            node_type: Some(NodeType::Company),
            paid_in: Some(PaidIn {
                amount: "800000".into(),
                currency: Some("HKD".into()),
            }),
            location: Some("中国".into()),
            approval_date: Some("2026-06-06".into()),
            ..Node::new(
                "0e4855d7-8e82-43f6-828e-7a2c7535bce3",
                "长飞光纤科技股份有限公司",
            )
        },
        Node {
            ratio: Some(100.0),
            node_type: Some(NodeType::Company),
            paid_in: Some(PaidIn {
                amount: "10000000".into(),
                currency: Some("HKD".into()),
            }),
            location: Some("香港".into()),
            approval_date: Some("2026-05-06".into()),
            ..Node::new(
                "e64ab3d3-dda7-41d2-8a39-3205edd957df",
                "长江实业有限公司",
            )
        },
    ],
    vec![
        Edge {
            holder: "subject".into(),
            held: "0e4855d7-8e82-43f6-828e-7a2c7535bce3".into(),
            ratio: Some(80.0),
        },
        Edge {
            holder: "subject".into(),
            held: "e64ab3d3-dda7-41d2-8a39-3205edd957df".into(),
            ratio: Some(100.0),
        },
    ],
);
// style / top_n / max_nodes 均为 ChartRequest::new 默认：
// ChartStyle { node_width_mm: 38.0, margin_mm: 8.0 }, top_n: None, max_nodes: 100
```

结构摘要：

| 字段 | 值 |
| --- | --- |
| `subject_id` | `"subject"` |
| 入边（股东） | **无** |
| 出边（投资） | 2 条：`subject → 长飞…`、`subject → 长江…` |
| `top_n` | `None` |
| `style` | default 38 / 8 |

同一套件里的 **Equity** 请求（对照，页眉正常）只含股东入边，**不含**上述投资边；不在本 issue 复现路径内。

引擎侧最小复现：

```text
Chart::new(上述 request)?.to_pdf(&font)?
→ 抽取第 1 页主/副标题
→ 实际：股权架构图: … / 各股东/合伙人 → … → 对外投资
→ 期望：对外投资: … / … → 对外投资
```

---

## 7. 建议讨论点

1. **产品文案：** 仅投资且整图一页，主/副标题应是投资总览，还是继续叫「股权架构图」？
2. **实现：** 若走投资总览，建议在 `whole_page_if_elastic` 成功后按锥形态分支，例如：
   - 无入边、有出边 → `investment_chart_title` + `investment_overview`
   - 有入边（无论有无出边）→ 现有 `equity_chart_title` + `whole_chart_subtitle`
   - 或：无入边时**不要**走整图捷径，直接 `pack_side(Investment)`（行为需与现有弹性整页测例对齐）
3. **规格：** 统一 §8.5 / §10.2 与历史「双侧」表述；补「零股东、仅投资、一页」标题金样测试（当前 `whole_page_tests` 未锁此例）。
4. **调用方约束：** `hcsw-pdfcraft` 按 AGENTS 约定不改本仓库源码；页眉无法在应用层诚实修复。请在本仓定稿后发版，调用方再 bump。

---

## 8. 非目标 / 已排除

- 不是套件把两张图合成一张双锥图。
- 不是节点 / 实缴 / 边方向组错。
- 不是下载文件名（调用方仍用 `… 对外投资.pdf` 后缀）。
- 不是套件 `SetMetadata.title`（那是合并后 `/Info`，值为主体名，不是页眉）。
- 不是 `Chart::title()`（恒为 `{主体}股权架构图`，页眉不读）。

---

## 9. 联系

讨论请回本文件或引擎仓库 issue。调用方仓库路径：`/mnt/data/dev/hcsw-pdfcraft`。引擎仓库路径：`/mnt/data/libs/pdfcraft`。
