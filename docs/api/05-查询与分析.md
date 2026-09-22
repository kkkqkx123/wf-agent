# wf-api 查询、审计与分析

对应 `src/query.rs`（1,036 行）、`src/audit.rs`（1,270 行）、`src/analysis/`（7 个文件约 3,900 行）。全部为**只读查询/分析面**，无运行时状态变更（订阅除外，RAII 管理）。

## 1. query.rs — 执行记录查询 API（TS `QueryAPI`）

### 双层过滤策略（核心设计，未变）

1. **第一层 `FilterCriteria`（基础）**：`workflow_id`/`status` **下推**存储层（`WorkflowExecutionListOptions`）；`start_time_from/to`、`tags`、`custom` 内存 `retain`。
2. **第二层 `FilterExpression`（高级）**：全内存求值——`nin`/`contains`/`regex` 留在内存，避免触碰各后端 SQL 生成。

### 类型

- `FilterOperator`：Eq/Neq/Gt/Gte/Lt/Lte/In/Nin/Contains/Regex（snake_case 序列化）。
- `FilterExpression{field, operator, value}`，`a.b.c` 点路径每段依次尝试 原样 → camelCase → snake_case。
- `SortOptions`/`PaginationOptions`（`DEFAULT_QUERY_LIMIT = 100`）。
- `AggregationType`（Count/Sum/Avg/Min/Max/GroupBy，默认键 count/sum/…/groups）+ `AggregationOp{type, field, group_by, as_}`；结果为动态 `serde_json::Map`。
- `ExportFormat`：Json/Csv/Xml。
- `ExecutionRecord`：`WorkflowExecution` 扁平投影（含派生 `duration = completed_at − started_at`）。
- `QueryBuilder`：13 个方法（new/filter/filter_by/sort/limit/offset/get/first/count/aggregate/export/distinct/group_by）。语义细节：`filter` 逐字段新值覆盖旧值；`count` 忽略分页取全量；**`get` 的表达式在分页之后应用**。

### 语义细节

- 缺失字段：仅匹配 `Neq`，不匹配其他操作符。
- `compare_values` 只对双数字（partial_cmp）与双字符串有效，其余不可比（Gt 等返回 false）；非法 regex 匹配空集。
- 整值 f64 输出整数 JSON number，保证 sum/min/max 整型不退化；GroupBy 用 `BTreeMap` 稳定有序；Min/Max 空集输出 Null。
- 排序：数值优先、`Some` 排 `None` 前、stable sort、descending 整体 reverse。
- 导出：CSV 固定 9 列 camelCase 头 + 引号转义；XML 逐记录缩进 + 转义 + 跳过 null。
- 约 233 行单元测试。

## 2. audit.rs — 执行审计（三维数据源解析链）

### 数据源解析链（每个 facet 同一入口模式）

```
live（运行时注册表实体快照）
→ persisted（存储适配器执行记录）
→ checkpoint 快照（最新检查点经 coordinator 解析 delta 链，记录被清理时的兜底）
```

视图携带 `AuditSource { Live | Persisted | CheckpointSnapshot | Unknown }`；未知执行返回 Unknown 源摘要而非报错。三源字段有差异：live 工具调用只有预聚合 duration（无起止时间戳）、persisted 由 `error.is_none()` 推导 success 且节点视图 node_type 恒空、checkpoint 分支执行起止时间恒 None。

### 域分派与歧义

`audit_summary`/`audit_timeline` 先经 **`entity::execution::resolve_execution`** 判定域：`Ok(domain)` → 该域优先、另一域回退；`Err(Conflict)` → **agent-loop 优先静默回落**；`Err(ExecutionNotFound)` → Unknown 摘要；其他错误传播。checkpoint 计数合并两域 `list_by_entity` 并按 id 去重。

### 能力与类型

- `audit_summary` / `audit_report`（summary + 按 kind 拉 iterations 或 node_executions）/ `audit_timeline`。
- `list_iterations` / `list_tool_calls` / `list_llm_calls`（由迭代扁平化）/ `list_node_executions`（工作流面）。
- `audit_timeline` **离线时间线重建**：`sort_by_key(timestamp, timeline_phase)` 平局规则——phase 0 ExecutionStart → 1 IterationStart → 2 调用类 start/单条 → 3 各种 end → 4 Error → 5 ExecutionEnd；同刻 start 先于 end，零时长调用正确渲染。`AuditTimelineEntryType` **13 个变体**，其中 ExecutionStart/End 仅参与排序、实现中从不产出（实际 emitted 11 种）。tool/llm 调用缺起止时间戳时锚定迭代时间输出合并单条。纯审计视图，不参与恢复。
- 类型：`AuditSummary`（含 entity_kind 与 checkpoint_count）、`ToolCallAuditView`、`LlmCallAuditView`（每迭代 seq、profile_id、模型、token、时长）、`IterationAuditView`、`NodeExecutionAuditView`、`AuditTimelineEntry`。

## 3. analysis 模块

### 3.1 error_analysis.rs（1,142 行）— 工作流错误根因分析（TS `WorkflowErrorAnalysisAPI` 家族）

基于 `wf-common::error_chain::ErrorRecord` 与 `RecoveryAction`。**两源链（live 结构化链 → persisted 文本合成），无 checkpoint 兜底**（与 audit 三源不同）。

- 11 个能力：`workflow_error_stats`、`recovery_recommendations`、`similar_errors`（normalize_message 聚类跨执行，limit=0 默认 20）、`get_error_chain`（根→指定 id 切片，时间升序）、`analyze_root_cause`、`error_context`/`error_context_chain`（错误时刻变量快照/调用栈/内存，经 state_tracker，未录制则为空）、`get_advanced_error_analysis`（热点节点 top-10、最成问题节点 top-5、时间模式、趋势）、`get_recovery_proposal`、`stream_error_chain`（Stream，根优先）、`subscribe_to_errors`（事件总线 Error 过滤 + `ErrorSubscription` RAII abort）。
- 启发式：severity（Retry/Fallback→Warning，ManualIntervention/Abort→Critical，否则可恢复性判定）；`suggest_action` 类型→动作映射（ToolError→retry/fallback、Timeout→retry、Validation→skip/fallback）；**`estimate_likelihood` = 基础 50 + 可恢复 30 + Warning 20 + 动作（retry 15 / fallback 10 / skip 20），clamp 0–100**；`estimate_recovery_time`：retry 1100ms / fallback 600 / skip 200。
- 时间模式：间隔取首末各 3 个均值，recent < early×0.7 → accelerating、> ×1.3 → decelerating，否则 steady（<3 条 none）。注意命名：间隔变小=更密集=accelerating。
- 已知退化：`error_trend` 比较的是前后半的**记录条数比**而非速率，奇数长度恒倾向 Decreasing。

### 3.2 error_common.rs（66 行）— 两面共享 helper

`analysis::error_analysis`（workflow 面）与 `agent::agent_error_analysis`（agent 面）共用：`record_view`（ErrorRecord→可序列化视图）、`action_name`（RecoveryAction→snake 字符串）、`normalize_message`（小写折叠空白 + 剥除尾部括号后缀再聚类）、`minimal_record`（持久化纯文本合成最小记录维持分析可用）。

### 3.3 performance.rs（816 行）— 性能剖析（TS `PerformanceMetricsAPI`）

常量：`BOTTLENECK_HIGH_RATIO=0.5`/`MEDIUM=0.2`/`MAX_BOTTLENECKS=10`（profile 结构体内截断 5）、`SLOW_NODE_THRESHOLD_MS=5000`、`FAST_TIER_MS=5000`/`NORMAL_TIER_MS=30000`、`TREND_IMPROVE/DEGRADE=0.8/1.2`。

- `profile`/`analyze_performance`：单执行时间线（live 节点历史按 start_time 排序，兜底持久化）；total 兜底 = 最大节点 end − start；share = duration/total。
- `identify_bottlenecks`：占比 High/Medium/Low，分母塌缩为 0 时退化为节点时长求和；排序 duration 降 → location 升。
- `get_performance_summary`：`PerformanceTier`（≤5s Fast / ≤30s Normal / Slow）+ 三条建议（慢节点 >5s、成功率 <1、ops/s <1）。
- `get_iteration_comparison`：前后半平均，0.8/1.2 趋势比（i64 整数除法）。
- `compare`：`improvement_rate` 截断 ±1，`improved = rate > 0`。

### 3.4 progress.rs（533 行）— 执行进度跟踪（TS `progress.ts` 移植）

- `get_progress`：live 已完成节点 + 持久化时间戳/状态（缺失降级 now/"running"）+ 图节点总数（`resolve_graph`）。tool_calls = node_type 含 "TOOL" 计数。
- `compute_progress_metrics`（纯函数）：ETA = 平均迭代时长 × 剩余迭代，**confidence 固定 0.8**，未知时 remaining = −1。
- `ProgressTracker`：轮询（interval 下限 50ms + CancellationToken `select!` biased）、监听器列表（`Unsubscribe` RAII Drop 移除）、**仅 iteration/status 变化才发事件**（Complete/Failed/Paused/Resumed）、stop→start 可重入（换新 token）。
- `format_progress`：`Progress: x% | Iteration i/n | ETA: HH:MM:SS`。

### 3.5 search.rs（697 行）— 跨资源统一搜索（TS `SearchAPI`）

- 资源 6 类：Workflow/Execution/Task/Checkpoint/Event/AgentLoop（Event 源是 `event_bus.recent_events()` 而非存储）。
- `search()`：`futures::join_all` **并行扇出**，单类型失败 warn 降级为空而非整体失败。
- 评分：id 命中 +2、其他字段 +1（小写子串）；`matched_fields` 记录实际命中值。
- 确定性排序：类型内 score 降 → id 升；全局 score 降 → type 升 → id 升，`by_type` 用 BTreeMap；输出不依赖注册表迭代顺序（有专项测试）。
- `SearchOptions`：types / limit_per_type（默认 20）/ limit_total（默认 100）；`truncated` 按 `>` 判定后截断。

### 3.6 llm_metrics.rs（198 行）— LLM token/成本指标（TS `getAgentLlmMetrics`）

读取共享 `MetricsRegistry` token 收集器 5 个指标（TOTAL/PROMPT/COMPLETION_TOKENS、COST、REQUEST_COUNT），`by_model` 按 total_tokens 降序，label 缺失记 "unknown"；`flush(ctx)` 冲刷持久层 pending；未装配 metrics 时返回全零 `default()`（优雅降级，与 stats.rs 的报错策略相反）。

### 3.7 stats.rs（464 行）— 指标注册表统计门面

- **11 个 collector 薄包装**：workflow/node/agent/tool/error/event/timeout/checkpoint/template/config/http_stats；**6 个 `*_with_history` 异步版**合并持久化指标存储。
- top-N：`top_workflows`/`agent_stats_by_profile`（total 降 → id 升，零用量省略，需外部传入 id 集）、`top_node_types`（按 value 降序，不做零过滤）。
- 导出/报告：`export_json`、`export_prometheus`、`subscribe`/`unsubscribe`（ReportCallback 生命周期）、`generate_report(ReportOptions)`（时间范围 + include_trends）。
- `registry(ctx)`：未配置 metrics 时**报错**（`ApiError::execution`）——全模块唯一缺数据抛错的门面。
- 测试占比约 53%（含历史合并 mock-sink 集成测试）。

## 4. 横切要点

- **降级策略总体一致、有明确例外**：live → persisted →（仅 audit 额外 checkpoint），无数据返回空/零/Unknown；例外是 `stats::registry` 报错与 `error_context` 未知 id not_found。
- **数据源层数差异**：audit 三源；error_analysis/performance/progress 两源（无 checkpoint）。
- **纯查询面**：全部 `&ApiContext`；RAII 守卫（`Unsubscribe`、`ErrorSubscription`）。
- 测试占比 21–53% 不等，重点覆盖启发式、降级路径与排序确定性。

## 5. audit 与 checkpoint 命令面的歧义解析差异（有意为之）

同一 execution id 同时存在于 agent-loop 与 workflow 两域时，两采取不同策略（实现载体：`entity/execution.rs::resolve_execution` 与 `checkpoint/record.rs` 的域护栏）：

| 面 | 策略 | 理由 |
|---|------|------|
| **audit**（只读聚合） | 静默回落：resolve 判域决定优先序，Conflict 亦按 agent-loop 优先尝试，最终兜底 Unknown 摘要而非报错 | 目的是"尽力返回可读时间线/摘要"；歧义属边缘情形，不阻断查看 |
| **checkpoint/执行命令**（变更面） | 要求显式：resolver 双命中返回 `Conflict` 强制调用方指定 `--domain`（HTTP 端点本身已隐含域，用 `ensure_execution_domain`/`ensure_checkpoint_domain` 断言，消息直接指引正确端点） | 变更/恢复选错域后果不可逆；决策权留给用户 |

统一方案应在引入带类型的 `ExecutionRef` 时一并讨论，而非在 resolver 层单方面改动。
