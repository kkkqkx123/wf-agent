# 状态、持久化与资源管理

> 范围：`crates/engine/wf-agent/src/state.rs`、`persistence.rs`、`registry.rs`、`capacity.rs`、`timeout.rs`。

## 1. 运行时状态：`AgentLoopState`

`AgentLoopState`（`src/state.rs:159-192`）是每个 agent loop 的内存状态机；`AgentLoopStateSnapshot`（`state.rs:109-157`）是其可序列化快照，字段一一对应：

- **生命周期**：`status: ExecutionStatus` + 当前迭代数、工具调用计数、起止时间、错误信息（`state.rs:110-116`）。
- **执行轨迹**：`iteration_history: Vec<IterationRecord>`（每次迭代的起止时间、工具调用、LLM 调用、响应内容，`state.rs:38-53`）、`error_records`（`state.rs:117`）。
- **跨迭代预算/治理**：`retry_totals`（按错误类别的跨迭代重试计数，`state.rs:121`）、`permanently_failed_tools`（仅作报告，`state.rs:150`）、`scope_violation_streak`（连续全越界工具轮次，`state.rs:156`）、`timeout_count`（`state.rs:146`）。
- **重放幂等表**：`pending_tool_calls`（快照时仍在飞的工具调用 id，恢复后重新执行，`state.rs:133`）；`completed_tool_results`（已完成调用 id → 缓存结果，恢复后重放相同 id 直接返回缓存、不再执行工具，`state.rs:138`）。
- **其他**：`variable_snapshots` + `variable_history`（`state.rs:128/174`）、`tool_discovery: ToolDiscoveryState`（`state.rs:74-98`）、`locked_tool_call_protocol`（首次组装 LLM 请求后锁定工具调用协议格式，保证恢复后一致，`state.rs:143`）、`interruption_records` / `event_records` 审计（`state.rs:124-127`）、流式缓冲（`state.rs:189-191`）。

**状态转移**：`transition()` + 静态合法转移表 `transition_allowed()`（`state.rs:462-502`）强制执行合法迁移；派生方法 `start/pause/resume/complete/fail/cancel/timeout`（`state.rs:533-567`）。

**快照机制**：实现 `StateManager<AgentLoopStateSnapshot>` —— `create_snapshot()` / `restore_from_snapshot()` / `cleanup()` / `size()`（`state.rs:729-797`），往返保留重试预算、pending/completed 工具调用等（测试 `state.rs:863-939`）。`AgentStateCoordinator` 包装 `ConversationSession` 提供对话消息的快照/恢复（`state.rs:800-833`）。

## 2. 执行记录落库（`persistence.rs`）

- 入口 `build_agent_execution(entity)`（`persistence.rs:19`）：把实体内存中的 `AgentLoopState` 转成可持久化的 `AgentExecution` 记录，按 per-run 的 `agent_loop_id` 键控。
- 落库内容：状态、当前迭代/工具调用计数、完整 `iteration_history`（tool_call_id 优先复用 LLM 的调用 id，保证审计轨迹与对话、checkpoint 记录一致，`persistence.rs:41-56`）、LLM 调用轨迹（`persistence.rs:61-65`）、错误、永久失败工具、运行时配置 `AgentRuntimeConfig`（`persistence.rs:91-119`）与执行层级 `ExecutionHierarchy`（`persistence.rs:125-140`）。
- 落库时机：每次迭代后由 `AgentRecordPersister::persist_iteration` 持久化（`coordinator/lifecycle/run.rs:32-36`），终态时再次落库（`run.rs:170-172`）。

## 3. 实体注册中心：`AgentLoopRegistry`

所有 agent-loop 实体的运行期注册中心（`src/registry.rs:56-74`）：

- **注册/注销/查询**：`register`（终端状态实体的占位符会被替换，`registry.rs:152-182`）、`unregister`、`get`、`query`（按 `AgentExecutionFilter` 过滤，`registry.rs:269-300`）、`execution_records`（`registry.rs:304-320`）、`cleanup_terminated`（`registry.rs:324-346`）。
- **容量闸门**：限制最大并发（`max_concurrent` / `available_permits` / `gate_stats`，`registry.rs:124-141`）与子代理嵌套深度（`depth_allowed`，`registry.rs:193-196`）；实体到达终态时释放容量。
- **结果与任务句柄**：`store_result` / `result` 结果槽（`registry.rs:225-232`）、`register_task` / `abort_task` / `unregister_task`（`registry.rs:236-253`）。

## 4. 容量门（`capacity.rs`）

`AgentCapacityGate`（`capacity.rs:9-11`）：并发准入门，包装 `ConcurrencyGate`；`try_acquire()` 预留一个槽位（`capacity.rs:39-41`），超限或门关闭则失败；`set_max_concurrent()` 原地重建门（`capacity.rs:21-23`）。permit 生命周期归 registry（存于实体、终态时释放），与 registry 解耦便于独立测试。

## 5. 超时管理（`timeout.rs`）

`AgentTimeoutManager`（`timeout.rs:12-14`）提供整个 loop 的墙钟超时 + 暂停超时（`max_pause_duration`）+ **可暂停句柄**——审批等待期间不消耗超时预算（`timeout.rs:7-10`）：

- `register(id, duration, on_timeout)`（`timeout.rs:111-174`）：spawn 倒计时任务，用 `watch` 通道跟踪暂停状态，暂停期间剩余时间冻结（`saturating_sub` 扣除活跃时间），恢复时通过 `Notify` 唤醒；`TimeoutHandle` 取消标志可阻止回调执行（`timeout.rs:41-66`）。
- `AgentPauseGuard`（`timeout.rs:178-186`）：RAII 审批暂停守卫，drop 时自动 resume。
