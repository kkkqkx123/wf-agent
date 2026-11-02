# wf-metrics

infra 层指标子系统：为运行时提供统一的指标采集、聚合、持久化、导出与报告能力。核心是一把可复用的 `BaseMetricCollector`（无锁缓冲 + 直方图/摘要状态），其上叠放 17 个领域采集器，由 `MetricsRegistry` 统一持有；对外支持 Prometheus 文本与 JSON 双格式渲染、可选的持久化 sink（由 `wf-runtime` 注入实现，保持 crate 依赖 DAG 无环），以及注册表级的聚合报告与异常检测。TUI 与 server 共享的基础使用量追踪（屏幕切换、操作时长、错误类型）也在本 crate（不记录任何用户身份数据）。

## 依赖

`wf-types`（`config::metrics` 配置类型）、`wf-common`（时间、重试预算事件类型），以及 dashmap/tokio/serde/serde_json/thiserror/tracing/async-trait。持久化通过 `MetricsSink` trait 抽象，`wf-runtime` 用 `MetricsStorageAdapter`（memory/Sqlite/PostgreSQL）接线——本 crate 不依赖 `wf-storage`。基准测试在 `benches/record_bench.rs`（criterion）。

## 模块结构

### 基础设施

| 模块 | 职责 |
|---|---|
| `metric.rs` | 基础数据模型：`MetricType`（Counter/Gauge/Histogram/Summary）、`Metric` 运行时记录（直方图/摘要附带 buckets/percentiles/sum/count）、`MetricFilter`/`MetricQueryResult`/`TimeRange`/`TimePoint`、`labels()` 快捷构造 |
| `collector.rs` | `BaseMetricCollector`：所有领域采集器的公共底座。无锁记录（DashMap 系列 + 原子直方图桶计数）、缓冲批量 flush 到 sink（失败批次进入有界重试队列，超限丢弃并计入 `InternalMetrics::drop_count`）、过期清理（内存保留窗口，默认 1 小时）、查询（内存 + 持久化合并，重启后可从持久化点重建直方图快照）、快照导出。`CollectorConfig`（缓冲大小、flush/报告间隔、保留窗口、启用开关、标签治理）从 `wf-types::config::metrics` 解析，注册表级全局保留窗口可被每采集器配置覆盖。`InternalMetrics` 提供自监控快照（缓冲压力、flush/query 耗时、丢弃与错误计数、估算内存） |
| `collector_math.rs` | 纯函数聚合数学：把排干的批次转为可持久化点（直方图持久化累积桶快照以便重启重建；摘要展开为 `{name}_p{...}` gauge 点）、桶→分位数、时间序列聚合。无锁无 sink，独立于采集器运行时 |
| `sink.rs` | `MetricPoint`（可持久化指标点，直方图携带累积桶/sum/count）与 `MetricsSink` trait（save_batch/query/delete_old） |
| `labels.rs` | 标签治理：每采集器可选键白名单；违规默认仅告警放行，strict 模式才拒录，存量采集器不受影响 |

### 领域采集器（collectors/）

全部基于 `BaseMetricCollector` 组合而成，指标名常量集中在 `constants.rs`（workflow/node/tool/token/agent/agent_loop/event/error/subgraph/template/timeout/retry/checkpoint/compaction/config/http 等命名空间），Prometheus HELP 描述集中在 `descriptions.rs`（无表项的指标自动人名化回退）：

| 采集器 | 覆盖范围 |
|---|---|
| `workflow.rs` | 工作流执行次数/时长/成功失败/活跃数/错误/重试/超时 |
| `node.rs` | 节点执行记录（开始/成功/失败/时长/输入输出大小/token 用量） |
| `subgraph.rs` | 子图执行 |
| `agent.rs` / `agent_loop.rs` | agent 层与 agent 循环（迭代次数等） |
| `event.rs` | 事件计数 |
| `tool.rs` | 工具调用次数/时长/错误/参数与结果体积/发现与激活 |
| `token/` | LLM token 用量与成本。`token.rs` 定义 `TokenCallLabels`（model/profile/provider/format，缺省维度省略以保持系列稀疏）与 `TokenUsageRecord`（原始 provider 计数与归一化计数两视图）；子文件 `tracking.rs`（记账 + 有界的按执行累加器，上限 1024）、`stats.rs`（聚合统计如 `TokenUsageStats`/`ExecutionTokenUsage`）、`export.rs` |
| `error.rs` | 错误发生计数（按类型/来源） |
| `timeout.rs` | 超时计数与统计 |
| `retry_budget.rs` | 重试预算：消费/耗尽/退避/超时错误、按消费者活跃重试追踪；监听 `wf_common` 的 `RetryBudgetEvent` 回调 |
| `checkpoint.rs` | checkpoint 创建/失败等 |
| `compaction.rs` | 上下文压缩指标 |
| `config.rs` | 配置子系统访问/耗时/失败 |
| `resource.rs` | 资源采样（`ResourceSample`） |
| `template.rs` | 模板渲染用量 |

### 注册表与导出

| 模块 | 职责 |
|---|---|
| `registry.rs` | `MetricsRegistry`：每运行时一个，持有全部领域采集器（typed accessor 获取）；执行路径持有 `Option<Arc<MetricsRegistry>>`，缺省即静默跳过（可选注入为常态）。按 `MetricsConfig` 构造（每采集器 retention 覆盖全局值）；内建异常检测（`AnomalyThresholds`：错误风暴 >100、工作流成功率 <0.8、工具失败率 >0.2、checkpoint 失败 >10）；支持报告订阅回调列表 |
| `formatter.rs` | Prometheus 文本格式（HELP/TYPE、`_total` 后缀、直方图桶与摘要分位，零外部依赖）与 JSON 快照导出；`format_internal_prometheus` 把采集器自监控指标（缓冲压力、丢弃、flush 错误）作为独立 series 一并暴露，使管道健康可被外部观测 |
| `render_cache.rs` | `RenderCache`：一次 `refresh` 单趟收集快照并同帧渲染双格式缓存，高频抓取不重复采样；渲染在锁外完成 |
| `report.rs` | 注册表级聚合报告 `generate_report`：总量/按类型/按分类汇总、Top 指标、趋势（需给时间范围，对 14 个关键指标计算方向与变化百分比）、异常列表；报告通过订阅回调推送 |
| `usage.rs` | 基础使用量追踪（`UsageMetricsCollector`）：屏幕切换、操作时长、错误类型三类计数，供 TUI 与 server 共享；仅服务内部优化优先级，标签集合有界 |

## 语义约定

- **可选注入**：指标是可观测性旁路，所有采集器经由 `Option<Arc<MetricsRegistry>>` 访问，缺失不阻断任何执行路径；每采集器也有 `enabled` 开关，禁用时不分配 series 状态直接丢弃。
- **持久化契约**：counter/gauge 原样持久；直方图持久累积桶快照（value=sum + buckets/sum/count）以便重启重建分布（M5）；摘要按分位展开为 `{name}_p{...}` gauge 点（M4）。CLI 在无 server 特性时通过 `counter_total_with_history` / `latest_with_history` 把内存值与本地持久化合并。
- **双计数视图**：token 指标同时保留 provider 原始计数（prompt/completion/reported_total）与归一化计数（input/output/total），reported_total 仅用于与归一化 total 对账。

## 测试

各模块内联 `#[cfg(test)]`（大文件如 `collector/` 独立 `tests.rs`），另有 criterion 基准 `benches/record_bench.rs` 覆盖记录热路径。
