# wf-api infra 基础设施

对应 `src/infra.rs`（16 个模块文件 + 根声明，约 6,000 行）。定位：wf-api 各功能模块的基础设施层——组合根、错误、持久化、事件、订阅/流、插件桥、依赖索引、共享校验骨架。（旧的 `infra/metrics.rs` 已消失，指标能力分散到 `ApiContext.metrics`、`PersistenceLayer::save_metric`、`analysis/llm_metrics.rs`、`analysis/stats.rs`。）

## 1. context.rs（552 行）— 组合根 ApiContext

约 24 个字段的组装根，由 wf-runtime 装配（wf-api 不依赖 wf-runtime）：

- 存储与引擎：`storage: Arc<StorageContext>`、`registries: Arc<ResourceRegistries>`（wf-resource）、`checkpoint_store: Arc<StorageBackend>`（默认取 `storage.checkpoint.store()`）、`llm_gateway`、`tool_registry`、`sandbox`（每上下文编译一次）。
- 事件与指标：`event_bus: Arc<EventBus>`（共享，容量 1024）、`metrics: Option<Arc<MetricsRegistry>>`（默认 None）。
- Live 句柄：`workflow_executions: WorkflowExecutionRegistry`、`agent_loops: Arc<AgentLoopRegistry>`、`template_usage: Arc<DashMap<String,u64>>`（内存计数）、`trigger_state_registry`。
- 写入/持久化：`handlers`（私有内建 NodeHandler map）、`state_manager: ExecutionStateManager`（执行记录统一写点，wf-api 只读）、`persistence: Arc<dyn PersistenceLayer>`（默认 `StorePersistenceLayer::memory()`）、`persistence_bridge`（私有，事件持久化桥任务句柄）。
- 插件/钩子/交互：`plugin_source: Arc<dyn PluginHandlerSource>`（默认 Noop）、`user_interaction_handler`（RwLock<Option<Arc<dyn Handler>>> 单槽）、`hook_handler_registry: Option<Arc<HookHandlerRegistry>>`（透传给引擎，context 本身不触发钩子）。
- 宿主配置与失效标记：`file_checkpoint_manager: Option<FileCheckpointManager>`（私有，with 注入）、`tool_approval: Option<ToolApprovalConfig>`（None = 库默认自动批准）、`stale_workflows: Arc<DashSet<String>>`（Expired 标记的内存载体）。旧字段 `bundles` 已移除。

方法：双构造器（`new`/`from_runtime_parts`，结尾均重启持久化桥）、10 个 builder 式 `with_*`（`with_metrics`/`with_checkpoint_store`/`with_handlers`/`with_persistence`（换层并重启桥）/`with_plugin_source`/`with_agent_loop_registry`/`with_trigger_state_registry`/`with_file_checkpoint_manager`/`with_hook_handler_registry`/`with_tool_approval`）、`resolve_handler`（仅查内建 map）、`plugin_handlers()`（插件不能遮蔽内建节点类型）、`run_middleware(phase, ctx)`（**没有 run_hooks**，钩子走字段透传给引擎）。

**统一执行控制面**（新）：围绕 `LiveExecutionInstance`（agent/workflow 二选一别名）提供 `execution_instance`/`live_execution_status`/`pause_execution`/`resume_execution`/`stop_execution`/`cancel_execution`（workflow 优先，agent 其次；cancel = stop + abort 任务）/`execution_subtree`（按 root/祖先链枚举子树）/`child_depth_allowed`（嵌套深度配额）/`cleanup_terminated_executions`。失效标记 API：`is_stale`/`mark_stale`/`clear_stale`/`list_stale`。

`shutdown()`（同步）：`execution_tasks.abort_all()` + 停持久化桥；`impl Drop` 兜底调用（测试/嵌入式场景）。

## 2. error.rs（171 行）— 统一错误类型

`ApiError`（thiserror）八类：`Storage`、`NotFound{entity_type,id}`、`Validation`、`AlreadyExists`、`Execution{message,source}`（`#[source]` 保留类型化 cause）、`ExecutionNotFound`、`Timeout`、`Conflict`。对 9 个来源提供 `From`（ConfigError::Parse/Validation→Validation；LlmError 特例：`ProfileNotFound`→NotFound、ConfigError→Validation、`Timeout(ms)`→Timeout；serde_json::Error→Validation）。`with_timeout(duration, future)`：tokio 超时映射 `Timeout`。稳定分类保证任意传输层渲染一致状态码。

## 3. persistence.rs（996 行）— 持久化层体系

`PersistenceLayer`（async trait）四组能力：生命周期（name/initialize/shutdown/pending_writes/flush/health）、事件（save_event/save_events/query_events/count/clear）、快照（save/load/list/clear_snapshot）、指标（save_metric/query_metrics）。KV 前缀命名空间 `persistence/{event,snapshot,metric}/`。

| 实现 | 行为 |
|------|------|
| `NoOpPersistenceLayer` | 丢弃写入但 `AtomicU64` 计数（health().message 输出 "N writes discarded"——可观测的静默降级） |
| `StorePersistenceLayer` | `StorageBackend` 后端（memory / `sqlite(path)` / `postgres(conn)`）；事件存 JSON blob + 轻量 metadata，查询全前缀扫描 + 内存过滤（复用 `events::filter_events`），解析失败静默跳过 |
| `BufferedPersistenceLayer` | **装饰器**：有界 mpsc + 单 flusher 任务。三类水位缓冲（event 256 / snapshot 64 / metric 64，任一达到即 flush）+ `DEFAULT_FLUSH_INTERVAL_MS=5000` + 队列容量 1024；`WriteOp` 区分尽力数据写（try_send，满则丢弃并计入 `dropped`）与控制操作（Flush/Shutdown 必达 `.await`）；**两级关停**（先 Flush 冲 in-flight，再 Shutdown 终冲退出）；tick `MissedTickBehavior::Skip` + 失败退避到下一 tick（批保留重试）；clear_* 先 `flush_and_wait`（轮询 pending==0）再代理，保证清空前看到干净后端；`pending_writes = pending + queued` 三原子观测 |

模式：trait + 后端实现 + 缓冲装饰器；生产消费批量冲刷；优雅降级。

## 4. events.rs（1071 行，测试占半）— 事件系统查询 API

- **读穿合并**：所有历史/统计/时间线查询经私有 `merge` = `event_bus.recent_events()`（有界内存窗口）∪ `persistence.query_events`（持久层，失败忽略仅用窗口），timestamp 排序 + `dedup_by_key(id)`。
- `DEFAULT_EVENT_LIMIT = 100`；`EventQueryOptions{execution_id, agent_loop_id, workflow_id, event_types, limit}`。
- 能力：`dispatch`（先持久化后发布）、`history`（新→旧）、`timeline`/`agent_timeline`（+ turn/tool-execution 细分）、`stats`/`get_event_stats`、`search_events`（关键字子串）、`get_execution_timeline`（**`PHASE_DEFINITIONS` 6 对阶段**：Execution/Node Execution/Tool Call/Agent Turn/Agent Iteration/Checkpoint，贪心配对 start/end，未闭合 end=None；`determine_status` 倒序找终态）、`execution_timeline_summary`、`execution_listener_stats`、`event_system_health`、`event_history_size`、`event_time_range`、`clear_event_history`、`subscribe`、`wait_for_event`、`get_agent_loop_statistics`。
- `filter_events`（pub(crate)）被 persistence 复用。

## 5. dependency.rs（726 行，新）— 反向依赖索引与更新影响

扫描存储工作流（+ agent 模板注册表）中对共享资源的引用，支撑 update 时影响检查与一键审计。**删除保护仍由 reference.rs 负责，二者不重叠**。

- `DependencyKind { Tool, Script, Profile, SubWorkflow, Trigger }`（5 种）；`DependentEntry{workflow_id, workflow_name, node_id, field}`，node_id 哨兵：workflow 级工具 `"(workflow-level)"`、agent 模板 `"(agent-template)"`（其 workflow_id 前缀 `"agent:"`）。
- 引用扫描键：Profile→`config.profile_id` + inline `config.inline_definition.config.profile_id`；Tool→`tool_ids[]/tool_id/tool_name` + `available_tools` 四桶 + workflow 顶层 available_tools（顶层命中即跳过节点扫描）+ agent 模板；Script→`script_name`；SubWorkflow→`subgraph_id/embed_id`；Trigger→`trigger_id/trigger_template_id/triggered_workflow_id`。工具/脚本/触发模板按 **id 与 name 双候选**匹配（`resolve_candidates`）。
- `check_update_impact(kind, id)`：对每个依赖者重跑 publish 校验 → `ImpactLevel{Pass|Warning|Error}` 汇总 `UpdateImpactReport`；**任何 Error 且非 agent 前缀 → `ctx.mark_stale()`（Expired 标记的唯一置位者）**。
- `audit_all_workflows`：全库每个正式工作流跑 publish 校验，只返回有 error/warning 的。
- `request_async_revalidation(ctx, kind, id)`：spawn 后台重验证 + warn，供高频写路径不付同步成本。

## 6. validation.rs（244 行，新）— 共享校验骨架

`ValidationContext`/`ValidationError`/`ValidationResult` 及 `validate_tool_reference/validate_tool_list/validate_profile_reference/…` **本体在 wf-types，本模块只做再导出**；本地真正新增 `build_validation_context(ctx)`：聚合 tool_names/disabled_tools、profile_ids/**profile_formats**、script_names、workflow_ids、trigger_ids、agent_ids（registry + storage 双源合并），workflow_graphs/trigger_templates 留空由上层补（`workflow::validation::build_reference_context` 是唯一补充者）。workflow/agent/trigger 三个域校验器共享同一骨架。

## 7. reference.rs（420 行）— 引用感知删除

`ReferenceKind { Tool, Script, Trigger }`；`DeleteReference{workflow_id, workflow_name, node_id}`。`collect_node_references` 通用骨架（扫 workflow 节点 config 字符串匹配）被 tool/script 复用；Trigger 私有扫描（仅 Route/Start/End 节点的 trigger 键）。`delete_with_reference_check(ctx, kind, id, force)`：非 force 且有引用 → `Conflict`（列出 `wf#node`）；删除分派：Tool→`llm::tool::delete_tool`、Script→`llm::script::delete_script`、Trigger→`storage.trigger_template.delete`（**不清 registry，已知漂移**）。

## 8. handler_chain.rs（275 行）— 插件桥/处理器解析

- 契约 trait（wf-api 定义、wf-runtime 实现）：`PluginNodeExecutor`、`PluginMiddlewareBridge`、`PluginHandlerSource`（`node_executor`/`middleware` 必选；`plugin_node_types`/`llm_provider_names`（为 `LlmFormat::Custom` 预留）/`event_handler_event_types` 默认空）+ `NoopPluginHandlerSource`。
- `PluginNodeAdapter`：插件执行器 → 引擎 `NodeHandler`（错误映射为 `NodeExecutionFailed("plugin node error: …")`）。
- `TemplateSubgraphHandler`：设计的第三级解析链（内建 handler → 插件执行器 → 存储节点模板作子图经 `WorkflowCoordinator` 执行）。**现状：定义并导出但全 workspace 无构造点，实际生效仅两级**。
- `node_type_name`：规范 SCREAMING_SNAKE_CASE 名（插件注册表用）。

## 9. state_tracker.rs（353 行）— 执行状态记录（记录侧）

读侧在 execution_state.rs；本模块把点位快照经 `PersistenceLayer` snapshot 存储持久化，workflow/agent 共用（各提供 `ExecutionStateAccessor::capture() -> StatePoint`）。追加式日志键 `state:{execution_id}:{sequence:016}`（16 位零填充保证字典序=数值序，sequence 从 1 起取现存最大+1）。能力：`record_state`（**故意不从 crate 根再导出**——记录发生在引擎侧）、`clear_state`、`list_state_records`、`get_state_at_iteration`、`get_variable_snapshot`（as-of）、`get_variable_history`、`get_most_changed_variables`（不同序列化值数降序）、`get_variable_mutation_count`、`get_call_stack`、`get_memory_usage`、`get_peak_memory_usage`。

## 10. stream.rs（293 行）— 执行流（SSE/WS 友好）

- `ExecutionStreamEvent` 引擎无关协议枚举，**13 个变体**：`Engine(BaseEvent)`（总线生命周期统一包装进入）、`IterationStart`、`LlmDelta`、`ReasoningDelta`、`ToolStart`、`ToolEnd`、`Usage{prompt/completion_tokens,cost}`、`IterationEnd`、`Interrupted`、`Completed{result,iterations}`、`Failed{error}`、`SubAgentStarted`、`SubAgentEnded`。协议词汇不含引擎名（agent/workflow 是对等引擎）。
- `ExecutionEventStream`（mpsc 容量 256 + 可选 driver 句柄）：Drop abort（断连即取消运行中执行）；实现 `Stream`。
- `spawn_execution_stream(bus, id)`：**返回前同步完成订阅**（零事件丢失）+ 转发；`ExecutionStreamSink`（Clone）`completed/failed`。
- `from_agent_stream`：wf-agent 内部 `AgentStreamEvent`（12 变体）→ 协议事件适配，引擎类型不出现在协议中。
- **慢消费者统一策略**（本文件与 subscription.rs）：终态事件 `send().await` 必达，非终态 `try_send` 满即丢（广播滞后语义，永不阻塞总线/引擎）。

## 11. subscription.rs（265 行）— 过滤式事件订阅

`EventSubscriptionOptions`（execution/agent_loop/workflow id + event_types，四条件 AND）、`for_execution`、`matches`、`is_terminal`（终态集 6 个：WorkflowCompleted/Failed/Cancelled + AgentCompleted/Failed/Cancelled）。`EventSubscription`：接收端 + 后台转发任务，`next/try_next`，Stream，Drop abort；多消费者各有独立通道。`wait_for_event(bus, opts, timeout)`：有界等待首个匹配，超时/关闭都 `Ok(None)`。

## 12. 其余模块

| 文件 | 行数 | 功能 |
|------|------|------|
| `diagnostics.rs` | 162 | 存储健康报告：遍历 `storage.named_backends()`（**探测集来自 context 注册表本身，新实体自动覆盖**；内存 context 恰 20 个 backend），单存储失败降级为该 store `healthy:false` 而非整体失败；`health`/`diagnose`（+持久层健康+事件数）/`item_counts` |
| `config.rs` | 235 | `wf-config` 薄门面：parse/validate/transform/export/装配五组再导出；本地 `parse_workflow(_file)`/`validate_*`/`export_*` 把 ConfigError 映射进 `ApiResult`；分层 merge 原语不外泄，宿主只走 `ConfigOrchestrator` |
| `event_persistence.rs` | 125 | 事件持久化桥：订阅共享 EventBus，把约 120 种引擎事件经 PersistenceLayer 落盘（镜像 `wf_execution_shared::EventMetricsBridge`）；`Lagged(n)` warn 继续、写失败 warn；由 ApiContext 启停/重启 |
| `tasks.rs` | 117 | `ExecutionTaskRegistry`（DashMap<String, AbortHandle>）：register/unregister/abort/abort_all，供关停与断连硬取消 detached driver |
| `util.rs` | 6 | `round2` |

## 13. 关键设计要点

1. **持久化三件套**：trait + 缓冲装饰器（水位/两级关停/尽力写 vs 控制写）+ NoOp 兜底（带计数），pending/queued/dropped 原子可观测。
2. **依赖影响闭环**：保存正式工作流即触发 impact 检查；上游资源更新 → 重验证失败 → `mark_stale`（Expired）→ 查询面 warn 不阻断、重存清除。
3. **插件桥**：接口隔离解耦 wf-plugin；三级 handler 链中模板兜底已定义未接线。
4. **订阅/流一致策略**：慢消费者丢非终态、必达终态。
5. **观察者→持久化桥**：引擎零改动接入约 120 种事件持久化。
6. **读穿合并**：有界总线窗口 + 持久化支撑历史查询。
7. **组合根 + Drop 兜底 teardown**，`shutdown()` 为权威路径（任务持 `Arc<ApiContext>`，仅靠 Drop 触达不到存活任务）。
