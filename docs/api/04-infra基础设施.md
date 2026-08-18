# wf-api infra 基础设施

对应 `src/infra.rs`（15 个模块文件 + 根声明），约 4,500 行。定位："Shared infrastructure: error types, context, config parsing, events, streams, subscriptions, persistence and metrics helpers"——wf-api 各功能模块的基础设施层。

## 1. context.rs（300 行）— 组合根 ApiContext

约 20 个字段的组装根，由 wf-runtime 装配（wf-api 不依赖 wf-runtime）：

- 存储：`storage: Arc<StorageContext>`、`checkpoint_store: Arc<StorageBackend>`
- 资源与引擎：`registries: Arc<Registries>`（wf-resource）、`bundles: Arc<BundleRegistry>`、`llm_gateway`、`tool_registry`、`sandbox`（每上下文编译一次）
- 事件与指标：`event_bus: Arc<EventBus>`（共享，1024 容量）、`metrics: Option<Arc<MetricsRegistry>>`
- Live 句柄：`workflow_executions: WorkflowExecutionRegistry`、`agent_loops: Arc<AgentLoopRegistry>`
- 基础设施：`state_manager: ExecutionStateManager`（持久化执行记录统一写点）、`persistence: Arc<dyn PersistenceLayer>`、`plugin_source: Arc<dyn PluginHandlerSource>`、`execution_tasks: Arc<ExecutionTaskRegistry>`、`template_usage` 计数、`user_interaction_handler` 槽

关键方法：双构造器（`new` / `from_runtime_parts`）、builder 式注入（`with_metrics`/`with_checkpoint_store`/`with_handlers`/`with_persistence`/`with_plugin_source`）、`resolve_handler`/`run_hooks`/`run_middleware`（插件贡献）、`shutdown()`（abort 全部 driver 任务 + 停持久化桥，Drop 兜底）。

模式：组合根/服务定位器；trait 对象注入解耦 wf-plugin 与持久化后端。

## 2. error.rs（165 行）— 统一错误类型

`ApiError`（thiserror）八类：`Storage`、`NotFound{entity_type,id}`、`Validation(String)`、`AlreadyExists`、`Execution`（保留类型化 cause）、`ExecutionNotFound`、`Timeout(String)`、`Conflict(String)`。

- 对 8 个下层 crate 错误提供 `#[from]` 转换（特例：LLM `ProfileNotFound`→NotFound、`ConfigError::Parse/Validation`→Validation）。
- `with_timeout(duration, future)`：tokio 超时映射 `ApiError::Timeout`。
- 稳定错误分类保证任意传输层渲染一致状态码。

## 3. persistence.rs（984 行）— 持久化层体系（trait + 装饰器）

`PersistenceLayer`（async trait）三组能力：生命周期（name/initialize/shutdown/pending_writes/flush/health）、事件（save/query/count/clear）、快照（save/load/list/clear）、指标（save/query）。

三个实现：

| 实现 | 行为 |
|------|------|
| `NoOpPersistenceLayer` | 丢弃写入但用 AtomicU64 计数（经 health() 可观测） |
| `StorePersistenceLayer` | KV 存储后端（内存/SQLite，前缀命名空间 `persistence/event/` 等，避免直接 sqlx 依赖） |
| `BufferedPersistenceLayer` | **装饰器**：有界 mpsc 通道 + 单 flusher 任务；`WriteOp` 区分尽力写入（满队列丢弃+计数）与控制操作（始终等待）；两级关停（Flush→Shutdown）、水位触发冲刷、失败重缓冲 + tick 重试、pending/queued/dropped 原子观测 |

模式：经典 trait + 后端实现 + 缓冲装饰器；生产消费批量冲刷；优雅降级（NoOp 带计数）。

## 4. events.rs（1074 行）— 事件系统查询 API

- **读穿合并**：live EventBus 窗口 + 持久化事件按 id 去重合并（`merge`）。
- 能力：`dispatch`（先持久化后发布）、`history`（新→旧）、`timeline`/`agent_timeline`、`stats`、`search_events`（关键字）、`get_execution_timeline`（`PHASE_DEFINITIONS` 6 对阶段：执行/节点执行/工具调用/Agent 回合/迭代/检查点）、`clear_event_history`、`subscribe`、`wait_for_event`、`execution_listener_stats`、`event_system_health`、`execution_timeline_summary`、`event_history_size`、`event_time_range`、`get_agent_loop_statistics`。
- `filter_events`（pub(crate)）被 persistence 复用。

## 5. handler_chain.rs（271 行）— 插件桥/处理器解析

- 契约 trait：`PluginNodeExecutor`、`PluginHookBridge`、`PluginMiddlewareBridge`、`PluginHandlerSource`（注入源，由 wf-runtime 提供）、`NoopPluginHandlerSource` 默认空实现。
- 适配器：`PluginNodeAdapter`（插件执行器 → `wf_workflow::handler::NodeHandler`）、`TemplateSubgraphHandler`（模板子图节点 → 经 WorkflowCoordinator 执行子图，含父执行关联）。
- **三级解析链**：内建 handler → 插件执行器 → 模板子图兜底。
- `node_type_name`：规范 SCREAMING_SNAKE_CASE 节点名（插件注册表用）。

## 6. event_persistence.rs（123 行）— 事件持久化桥

后台 watcher 订阅共享 EventBus，把约 120 种引擎事件经 `PersistenceLayer` 持久化（`EventPersistenceBridge::new` + `spawn`）。引擎发布者不感知 wf-api 层；`Lagged` 跳过与写入失败以 warning 呈现；由 `ApiContext` 启停/重启。

## 7. subscription.rs（264 行）— 过滤式事件订阅

- `EventSubscriptionOptions`（execution/agent_loop/workflow id + event_types），`for_execution` 构造、`matches` 谓词、静态 `is_terminal`。
- `EventSubscription`：mpsc 接收端 + 后台转发任务；实现 `Stream`；Drop 即 abort。
- **慢消费者策略**：终态事件总是等待发送（保证订阅结束），非终态 try_send 满即丢（广播滞后语义，永不阻塞总线）。
- `wait_for_event(bus, options, timeout)`：有界等待首个匹配。

## 8. stream.rs（178 行）— 执行流（SSE/WS 友好）

- `ExecutionStreamEvent` 标记枚举：`Engine(BaseEvent)` / `Agent(AgentStreamEvent)` / `Completed{result, iterations}` / `Failed{error}`。
- `ExecutionEventStream`（Stream）：mpsc 接收端 + 可选 driver 句柄，Drop 时 abort（断连即取消运行中工作流）。
- `spawn_execution_stream`：同步订阅（零事件丢失）+ 转发；终态保证送达、非终态满即丢。
- `from_agent_stream`：AgentEventStream → 统一流。

## 9. state_tracker.rs（351 行）— 执行状态记录

- `ExecutionStateAccessor`（async trait：`capture() -> StatePoint`）：每实体种类一个薄适配器。
- `StatePoint`：iteration/status/variables/call_stack_depth/memory_usage。
- `ExecutionStateRecord`：execution_id + 单调 sequence + 时间戳。
- 能力：`record_state`（1 基序号，键 `state:{id}:{seq:016}`）、`clear_state`、`list_state_records`、`get_state_at_iteration`、`get_variable_snapshot`（as-of）、`get_variable_history`、`get_most_changed_variables`、`get_variable_mutation_count`、`get_call_stack`、`get_memory_usage`、`get_peak_memory_usage`。
- 追加式快照日志 + 序号，派生式读分析。工作流与 Agent 执行共用一套实现。

## 10. 其余模块

| 文件 | 行数 | 功能 |
|------|------|------|
| `diagnostics.rs` | 188 | 存储健康报告：探针 19 个适配器（probe! 宏），单存储失败降级为 `healthy: false` 而非整体失败；`health`/`diagnose`/`item_counts` |
| `reference.rs` | 212 | 引用感知删除：`ReferenceKind`（Tool/Script）、`DeleteReference`（workflow#node）；非 force 且有引用时拒绝 `Conflict` |
| `config.rs` | 218 | `wf-config` 门面：Parse/Validate/Transform/Export 分组再导出 + wf-api 自有聚合校验（`validate_workflow`/`validate_node` 聚合 `NodeConfigIssue`） |
| `metrics.rs` | 26 | 指标适配器薄包装（save/query/delete_old） |
| `util.rs` | 6 | `round2` 数值工具 |
| `tasks.rs` | 117 | `ExecutionTaskRegistry`（DashMap<String, AbortHandle>）：register/unregister/abort/abort_all，供关停与断连硬取消 detached driver 任务 |

## 11. 关键设计要点

1. **持久化三件套**：trait + 缓冲装饰器 + NoOp 兜底，健康可观测。
2. **插件桥**：接口隔离解耦 wf-plugin，三级 handler 解析链（内建 → 插件 → 模板子图）。
3. **订阅/流一致策略**：慢消费者丢非终态、必达终态。
4. **观察者→持久化桥**：约 120 种事件零改动接入持久化。
5. **读穿合并**：有界总线窗口 + 持久化存储支撑历史查询。
6. **Drop 保证的 teardown**：builder 式组装 + `Drop` 防御性兜底。