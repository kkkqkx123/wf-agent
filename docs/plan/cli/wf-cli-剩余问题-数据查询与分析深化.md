# wf-cli 剩余问题 - 数据、查询与分析深化方案

> 状态：方案设计 / 待评审
> 日期：2026-09-02
> 范围：`crates/app/wf-cli` 数据域与分析域（variable / message / task / query / metrics / analysis / search）
> 关联文档：`docs/plan/cli/wf-cli-剩余问题-分阶段深化总览.md`、`docs/plan/wf-cli-api-gap-analysis.md:4.5`、`docs/api/05-查询与分析.md`、`docs/api/01-wf-api-总览与架构.md:分析域`
> 源码锚点：`args.rs:899` `VariableSub` / `cmd/variable.rs:9` / `cmd/message.rs:1` / `cmd/query.rs:11` / `cmd/metrics.rs:1` / `cmd/analysis.rs:1` / `cmd/search.rs:1` / `wf-api/src/analysis/*.rs` / `wf-api/src/entity/variable.rs` / `wf-api/src/entity/message.rs`

---

## 一、现状与剩余问题

### 1.1 已完成

- `variable list/get/set/delete` 4 子命令（`cmd/variable.rs:14-48`）基础 CRUD。
- `message list/search` 2 子命令（`cmd/message.rs:14-35`）基础查询。
- `task list/show/stats/cancel`（`cmd/task.rs:14-53`）已打通。
- `query --status/--workflow-id/--limit/--sort/--offset/--aggregate/--export/--filter`（`cmd/query.rs:11-90`）已支持 `filter eq/neq/gt/gte/lt/lte/in/nin/contains/regex` 与 `aggregate count/sum/avg/min/max/group_by`、导出 `json/csv/xml`。
- `metrics show/export`（`cmd/metrics.rs:13-93`）快照与 `json/prometheus` 导出。
- `analysis performance/bottleneck/errors/compare/progress`（`cmd/analysis.rs:12-94`）与 `cmd/execution.rs:253-332` 的 `execution performance/errors` 二入口。
- `search <query> [--limit]`（`cmd/search.rs`）跨资源全文检索。

### 1.2 剩余问题

| 编号 | 域 | 现状 | 剩余缺口 | 对应 `wf-api` | 影响 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| C1 | variable | 仅 `list/get/set/delete`，缺 `history/statistics/export/import/batch_set/scopes/definitions/latest_checkpoint_variables` | 未暴露 `variable::{history, statistics, export, import, batch_set_variables, variable_scopes, variable_definitions, latest_checkpoint_variables}` | `entity::variable::*` + `variable_history` 等 | 变量无法追溯与批量 |
| C2 | message | 仅 `list/search`，缺 `recent/by_agent_loop/stats/conversation_history/estimate_tokens/search_messages` | 未暴露 `message::{recent, by_agent_loop, stats, conversation_history, estimate_tokens, search_messages}` | `entity::message::*` | 消息面仅执行维度 |
| C3 | task | `list/show/stats/cancel` 已全，缺 `cleanup_tasks` 的批量清理与 `statistics` 的按类型聚合 | 未暴露 `task::cleanup_tasks` 的 `before` 参数 | `entity::task::cleanup_tasks` | 任务 GC 缺失 |
| C4 | query | `filter` 仅单表达式 `field op value`，缺 `distinct/group_by/get_field_value` 与多表达式 `AND/OR` | 未暴露 `query::{get_distinct, group_by_field, get_field_value, evaluate_expression}` 的高级形态 | `query::*` | 高级查询停留在 `filter eq` |
| C5 | metrics | 仅 `workflow/node/agent/tool/error/event` 快照，缺 `top_workflows/top_node_types/agent_stats_by_profile/llm_metrics` | 未暴露 `stats::{top_workflows, top_node_types, agent_stats_by_profile}` + `llm_metrics::{agent_llm_metrics}` | `analysis::stats::*` + `analysis::llm_metrics::*` | 指标无法定位热点 |
| C6 | analysis | `performance/errors/progress` 已通，缺 `llm_metrics/iteration_comparison/decision_graph/path_probability/similar_errors/recovery` 深度 | 未暴露 `analysis::performance::iteration_comparison` + `error_analysis::{get_similar_errors, get_recovery_proposal, similar_errors}` + `llm_metrics` | `analysis::performance::*` + `analysis::error_analysis::*` | 分析仅单执行 |
| C7 | search | 仅 `query + limit`，缺 `type/category/tags` 过滤与分页 | 未暴露 `SearchOptions{types, category, tags, pagination}` | `analysis::search::{search, SearchOptions, SearchResourceType}` | 全文检索无法聚焦 |

---

## 二、修改目标

1. 数据域（variable/message）从"单点读写"补齐至"历史追溯 + 统计 + 批量"可运维。
2. 查询面（query/search）达到 `wf-api` 的 `QueryBuilder` 同等表达力（`distinct/group_by` + 多表达式）。
3. 分析面（metrics/analysis）从快照补齐至热点定位（`top_*`）与根因/相似错误分析。

---

## 三、分阶段修改方案

### 阶段 C1 - Variable 深度

**改动**：

- `args.rs:899` `VariableSub` 扩展：
  - `History { execution, scope, name }` → `variable::history(ctx, execution, scope, name)` 或 `get_variable_history`
  - `Stats { execution }` → `variable::statistics` / `variable_statistics`
  - `Export { execution, output }` → `variable::export` → 文件或 `Json` 信封
  - `Import { execution, file }` → `variable::import`
  - `BatchSet { execution, file }` → `variable::batch_set_variables`（`file` 为 `JSON {scope, name, value}[]`）
  - `Scopes { execution }` → `variable::variable_scopes`
  - `Definitions { scope }` → `variable::variable_definitions`
- `cmd/variable.rs:14` 新增分支，直通 `wf-api::entity::variable::*`，`Text` 模式补充 `scope/name = value` 行。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/variable.rs`

**验收**：`wf variable history --execution <id> --scope default --name x -o json` 返回历史；`wf variable batch-set --execution <id> --file vars.json` 批量成功；`wf variable scopes --execution <id>` 返回 scopes。

---

### 阶段 C2 - Message 深度

**改动**：

- `args.rs:952` `MessageSub` 扩展：
  - `Recent { limit }` → `message::recent`
  - `ByAgentLoop { agent_loop, limit }` → `message::by_agent_loop`
  - `Stats { execution }` → `message::stats` / `get_message_stats`
  - `Conversation { execution, limit }` → `message::conversation_history`
  - `Estimate { execution }` → `message::estimate_tokens`
- `cmd/message.rs:14` 新增分支，直通 `wf-api::entity::message::*`，`Search` 已有，补充 `--role` 过滤透传（`MessageFilter`）。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/message.rs`

**验收**：`wf message stats --execution <id> -o json` 返回统计；`wf message conversation --execution <id> --limit 20` 返回对话；`wf message recent --limit 10` 返回最近消息。

---

### 阶段 C3 - Query 高级能力

**改动**：

- `args.rs:272` `Command::Query` 新增 `--distinct <field>` / `--group-by <field>` / `--having <expr>` / `--select <fields>`（逗号分隔）。
- `cmd/query.rs:11` 扩展：
  - `--distinct` → `query::get_distinct(&records, field).await?` → `success "query-distinct"`
  - `--group-by` → `query::group_by_field(&records, field).await?` → `success "query-group-by"`
  - 多 `--filter` 支持：`filter` 改为 `Vec<String>`（`#[arg(long)] filter: Vec<String>`），每项 `parse_filter_expr` 后 `apply_filter_expressions` 的 `AND` 组合
  - `--having` 在 `aggregate` 后二次 `apply_filter_expressions`（对聚合结果过滤）
  - `--select` 在最终 `records` 上做 `get_field_value` 投影（仅保留指定字段）
- `export` 已有，补充 `export csv` 时的 `RFC 4180` 引号转义校验。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/query.rs`

**验收**：`wf query --distinct status -o json` 返回去重；`wf query --group-by workflow_id -o json` 返回分组；`wf query --filter "status eq completed" --filter "workflow_id eq wf-1"` 多过滤 `AND` 生效。

---

### 阶段 C4 - Metrics 热点定位

**改动**：

- `args.rs:1165` `MetricsSub::Show` 新增 `--top <n> --by-profile --llm`。
- `cmd/metrics.rs:13` `Show` 分支扩展：
  - `--top n` → `stats::top_workflows(registry, n)` + `top_node_types(registry, n)` 聚合到 `data.top`
  - `--by-profile` → `stats::agent_stats_by_profile(registry)`
  - `--llm` → `llm_metrics::agent_llm_metrics(ctx, execution_id).await?`（需 `--execution <id>` 参数）
- `Export` 保持 `json/prometheus`，`Text` 模式补充 `Top workflows:` 表格行（复用 `render.rs` 的表格 helper，若无则 `println!`）。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/metrics.rs`

**验收**：`wf metrics show --top 3 -o json` 返回 `topWorkflows/topNodeTypes`；`wf metrics show --by-profile -o json` 返回按 profile 聚合；`wf metrics show --llm --execution <id>` 返回 LLM 指标。

---

### 阶段 C5 - Analysis 深度

**改动**：

- `args.rs:1182` `AnalysisSub` 新增 `IterationComparison { baseline, compared, iteration }` / `SimilarErrors { id, limit }` / `Recovery { id }` / `DecisionGraph { id }`。
- `cmd/analysis.rs:12` 新增：
  - `IterationComparison` → `performance::iteration_comparison(ctx, baseline, compared, iteration).await?`
  - `SimilarErrors` → `error_analysis::get_similar_errors(ctx, id, limit).await?`
  - `Recovery` → `error_analysis::get_recovery_proposal(ctx, id).await?`
  - `DecisionGraph` → `agent::agent_graph::decision_graph(ctx, id).await?` 或 `analysis::path_probability_analysis`
- `Errors` 已有 `chain/root_cause/recovery`，补充 `--similar --stats` 透传。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/analysis.rs`

**验收**：`wf analysis similar-errors <id> --limit 3 -o json` 返回相似错误；`wf analysis recovery <id> -o json` 返回恢复建议；`wf analysis iteration-comparison <a> <b> --iteration 2` 返回迭代对比。

---

### 阶段 C6 - Search 聚焦

**改动**：

- `args.rs:264` `Search` 已有 `query/limit`，新增 `--type workflow|execution|task|checkpoint|event|agent_loop`（`SearchResourceType` 枚举） / `--category --tags --author` / `--offset`。
- `cmd/search.rs:14` 重构 `SearchOptions`：
  - `types: Option<Vec<SearchResourceType>>` 按 `--type` 解析（逗号分隔）
  - `limit_per_type = limit.map(|l| l/3)` 改为按 `types.len()` 均分，避免单类型饥饿
  - 增加 `--category/tags/author` 的 `WorkflowSearchOptions` 融合（当 `types` 含 `Workflow` 时透传）
- 保持 `render_envelope success "search"`。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/search.rs`

**验收**：`wf search "hello" --type workflow,execution --limit 6 -o json` 返回两类结果；`wf search "x" --type task --limit 3` 仅任务。

---

## 四、依赖与顺序

```
C1 Variable ─┬─► C3 Query 高级 ─► C5 Analysis 深度
C2 Message ──┤
             └─► C4 Metrics 热点 ─► C6 Search 聚焦
```

- C1/C2 可并行，C3 依赖 C1/C2 的数据面稳定（`variable history` 需可查询），C4/C5 可与 C3 并行。

---

## 五、测试

| 用例 | 覆盖 |
| :--- | :--- |
| `tests/variable_message.rs` | `variable set → history → stats → export → import → batch-set → scopes` + `message list → stats → conversation → estimate → search` 双后端 |
| `tests/query_advanced.rs` | `query --distinct --group-by --filter ×2 --aggregate --export csv` 的 `Json/Text` 双格式 |
| `tests/metrics_analysis.rs` | `metrics show --top 2 --by-profile` + `analysis similar-errors/recovery/iteration-comparison` |

---

## 六、风险

| 风险 | 缓解 |
| :--- | :--- |
| `variable batch_set` 的 `value` 为任意 `Json` 的类型安全 | `BatchSet` 的 `file` 解析为 `Vec<{scope,name,value: Value}>`，`value` 直接 `serde_json::Value` 透传，不做 `String` 二次转义 |
| `query --select` 投影与 `aggregate` 互斥 | `validate` 中禁止 `select + aggregate` 共存，返回 `CliError::Arguments` 的 `select cannot be combined with aggregate` |
| `metrics --llm` 需 `execution_id` 的参数依赖 | `validate` 中 `llm` 时必须 `--execution`，缺失返回 `Arguments` 错误而非 `ApiError` |

