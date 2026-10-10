模型家族：GPT-5

# quire geom slice 1 — Trail Review

以下“第 N 行”按 TSV 数据行计数，不含表头。

## 1. 决策证据薄弱或缺失

### 高：第 3、9 行的产品依据不成立

- 第 3 行称实现依据是“proposal §2.0”，并称提案书推翻了 bare `f32` 和 72 pt 默认边距；但仓库中的 `docs/提案书.md` 没有“§2.0”，抬头仍是“逐条确认中，未开始实现”，§2 明写元素尚未取舍，§7 对默认 72 pt 边距仍写“要你定的是”。因此 branded `Length`、零默认边距、正宽 `Stroke`、非空 `Path` 都没有已确认产品条目支撑。
- 第 3 行引用的 `.audit/geom-slice1-how.md` 当前不存在；`.audit/` 中只有 TSV。第 9 行的 12/12 评分只能证明候选符合自定 rubric，不能替代提案确认。
- 实际提交又在 `src/lib.rs` 根级重导出了整套类型。结合 `AGENTS.md`“提案仍逐条确认时不要提前写公共 API”，这是本次最需要用户复核的决策。

### 中：第 9 行的“parent agrees”没有独立证据

`CROSS_JUDGE.md` 有评分与推荐，但没有父 agent 的复核记录、异议处理或明确批准。TSV 把“读过评分后同意”写成结论，证据列只指向评分文件本身。

### 中：第 8、11 行没有真正比较迟到的 C3

第 8 行按 dropout 继续，缺少 dropout 规则出处、C3 任务 ID及“当时为空”的耐久证据；第 11 行 C3 到达后仅以“avoid churn mid-implement”拒绝换基线，没有补评分或与 C2 的差异复核。当前 C3 文件已经有内容，无法从现存材料重建第 8 行当时的状态。

## 2. 验证声称有、但 trail 没有证明

### 高：第 12、13 行没有保存四个 gate 的输出

- 第 12 行只是“delegate reported four gates green”。
- 第 13 行写了“all exit 0; 12 tests pass”，证据却只是 `cargo test+clippy+fmt+doc` 的标签，没有精确命令、逐项退出码、stdout/stderr、Rust/Cargo 版本，也没有说明验证对象就是干净的 `01fdfbb`。
- 现存提交确实包含 12 个单元测试，但这只能核实测试数量，不能证明当时四条命令都执行成功。没有 PR/CI，因而也没有独立 CI 记录补足这一点。

### 中：关键“不可能状态”只有正向单元测试

第 9 行把“only widening”“路径不能空、不能以 `LineTo` 开头、`Close` 后不能追加”作为满分依据；实现测试只验证允许的 widening 和几个合法路径。没有外部集成测试或 `compile_fail` doctest 证明非法构造在 crate 外确实编译失败。有限 mm 换算溢出到非有限值的分支也没有边界测试。

### 低：第 14 行的证据只覆盖认证，不覆盖 remote

`gh auth status` 能支持 token 无效，但不能证明“no git remote”；trail 应另记 `git remote -v`。当前仓库检查确实没有 remote，所以结论看起来正确，缺的是可复核记录。

## 3. 事后看风险较高的选择

### 高：提交夹带未记录的包名变更

`01fdfbb` 除 geom/error 外还把 `Cargo.toml` 的包名从 `quire` 改为 `quire-pdf`。第 2 行退出条件、第 9 行选型、第 12 行落地记录都没提此事；提案 §9 仍说工作名是 `quire`，设计稿 PR 计划也写“建立 `quire` 包”。这会改变下游依赖/导入名，且不属于 Length brands 的必要改动。

### 中：公共语义过早冻结

零默认边距、严格正 `Stroke`、严格正 `Size`、非空且 consuming-builder 的 `Path` 都是合理候选，但当前权威文档没有确认。把它们直接根级重导出，比先保持 crate-private 更难回退。

## 4. casual skim 容易漏掉的缺口

- 第 9 行明确说要 graft C1 的“module split + `pt_xywh`”，实际 `01fdfbb` 只有 `pt_xywh`/`pt_trbl`，全部实现仍在单个 `src/geom.rs`；trail 没有记录放弃 module split。
- `.audit/` 当前是未跟踪目录；候选与 `CROSS_JUDGE.md` 又只在 `/tmp`。即使本地文件现在存在，也没有 commit、hash 或仓库内快照保证证据不会丢失或被改写；缺失的 `geom-slice1-how.md` 已经展示了该风险。
- 提交增加大量公共行为且改了包名，但没有更新 `CHANGELOG.md`；这与 `AGENTS.md`“有公共行为起记”的规则不符，也不在退出条件或验证清单中。
- 第 13 行只报“12 tests pass”，没有区分 unit tests、doctests、集成测试；仓库目前没有 geom 集成测试，因此根级重导出和外部调用形状没有被真实消费者编译验证。

## 结论

代码门禁结论目前可被摘要和提交内容部分佐证，但 trail 不足以证明“依据已确认、四门禁已留证、选型完整复核”。在开 PR 前，至少应补：权威产品决策或撤回公共导出、包名变更决策、四 gate 原始结果、C3 补评，以及把审计材料放入耐久位置。
