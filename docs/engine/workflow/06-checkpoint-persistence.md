# wf-workflow Checkpoint、持久化与可观测性

> 范围：`src/checkpoint.rs` 及 `checkpoint/`、`src/persistence.rs`、`src/execution_budgets.rs`、`src/execution_context.rs`、`src/interaction.rs`、`src/interactive_script_session.rs`、`src/registry.rs`、`src/hook.rs`、`src/degradation.rs`。
> 所有 `file:line` 相对 `crates/engine/wf-workflow/src/`。

## 1. 检查点（`checkpoint.rs` + `checkpoint/`）

### 1.1 节点级策略（`checkpoint/strategy.rs:51-249`）

- `NodeCheckpointStrategy` 包装共享的 `CadencedCheckpointStrategy<WorkflowCheckpointTiming>`；预设 `never / always / on_error / every_node / every_n_nodes / for_depth`（`strategy.rs:62-131`）。
- `resolve(node_config)`（`strategy.rs:159-222`）：节点级配置在显式处优先；工作流作用域的时机（start/end）不受节点控制；节奏（cadence）只作用于 Before/After，错误 checkpoint 永远触发（`strategy.rs:192-206`）。

### 1.2 集成协调器（`checkpoint/coordinator.rs:30-50`）

`WorkflowCheckpointIntegration` 持有内部协调器、策略、存储、触发器状态注册表、fork 注册表与事件总线。入口：

- `on_node_completed` / `on_node_before` / `on_node_failed`（`coordinator.rs:238-278` / `285-324` / `326-363`）。
- 手动 / start / end / **interruption / pause / timeout**（`coordinator.rs:369-473`）：中断、暂停、超时 checkpoint 无条件持久化，保证已停止的运行可恢复（`coordinator.rs:413-415`、`436`、`454-456`）。
- `create_checkpoint`（`coordinator.rs:475+`）：重复进度闸门将相同坐标的快照并回最新行（`coordinator.rs:484-508`）；前驱未变检查带重建/重试（`coordinator.rs:537-580`）；持久化后经 `event_bus` 发布（`coordinator.rs:592+`）。

集成点的构建见生命周期层 `build_checkpoint_integration`（`coordinator/lifecycle.rs:120-139`），未配置策略时为 `None`。

## 2. 执行记录落库（`persistence.rs`）

- `build_workflow_execution`（`persistence.rs:26-149`）：把活体实体转为 `WorkflowExecution` 持久化记录——快照失败响亮降级为裸 `Created` 记录（`persistence.rs:36-59`）；过滤引擎内部变量（`persistence.rs:63-78`）；节点结果与节点执行历史交叉核对（`persistence.rs:80-104`）；层级经 `build_persisted_hierarchy` 构建（`persistence.rs:151-166`）。
- 写盘时机：主循环的 `persist_start` / `persist_final` 在执行开始与每次终态退出写 `WorkflowExecution`（`coordinator/workflow/persistence.rs:45-59` / `64-109`），由应用层经 `ExecutionStateManager` 接线；恢复场景由 `persistence_rebuild.rs` 测试覆盖。
- 统一事件发射 `emit_event`（`coordinator/workflow/persistence.rs:243-303`）：向执行事件总线发布 `BaseEvent`。

## 3. 执行预算跨挂起/恢复（`execution_budgets.rs`）

设计原则（模块文档 `execution_budgets.rs:1-6`）：checkpoint 存**冻结总量 + 已耗时间 `elapsed_ms`**，绝不存"剩余毫秒"——停在存储里的时间不消耗预算。

- `persist_totals`（`execution_budgets.rs:25-39`）：冻结总量写入实体变量映射（`BUDGETS_VAR`，`execution_budgets.rs:15`），恢复时据此重算余量。
- 审批等待不计费：`begin/end_approval_wait` 配合 `active_elapsed_ms`（`execution_budgets.rs:93-124`），人工响应时间不消耗 wall-clock 预算。
- `snapshot_config` / `snapshot_elapsed_ms`（`execution_budgets.rs:147-164` / `249-269`）：预算经 checkpoint 往返。
- `continuation_options`（`execution_budgets.rs:170-245`）：为恢复的运行重建剩余 wall-clock 与其他限额；**预算耗尽拒绝恢复**。

## 4. 压缩回写注册表（`execution_context.rs`）

事件驱动压缩链把压缩后的消息数组回写进**活体执行**的命名消息数组（数组本身由工作流变量映射持有，见 [03-variables-and-context.md](03-variables-and-context.md)）：

- `ContextWriter` trait（`execution_context.rs:57-80`）+ `VariableMapWriteBack`：对活执行变量映射的带版本写回；版本不符（`VersionMismatch`）即丢弃——**并发追加优先于陈旧压缩结果**（`execution_context.rs:99-104`）。
- `ExecutionContextRegistry`（`execution_context.rs:147-154`）按执行 id 键控；`register_workflow`/`unregister` 在执行开始/结束时挂接（`execution_context.rs:162-181`）；`write_context_with_tail` 保留最近若干轮可见（`execution_context.rs:109-131`）。
- 边界：只有工作流变量映射注册于此；agent 会话自消费 `CONTEXT_COMPRESSION_COMPLETED` 事件（`execution_context.rs:10-13`）。

## 5. 交互挂起与恢复

- `interaction.rs:7-10` re-export `wf_execution_shared::interaction` 的实现：进程级 `OnceLock` 注册表（`interaction.rs:17`，`DashMap<String, oneshot::Sender<Value>>`）、`register_interaction`（`:83`）、drop-safe 的 `InteractionWait`（`:98`）、`complete_interaction`（`:125-127`）。工作流侧的等待/超时/清理逻辑在 USER_INTERACTION 处理器（[02-node-handlers.md](02-node-handlers.md) 第 8 节）。
- `interactive_script_session.rs:4-12` re-export 交互式脚本会话共享设施（`InteractiveScriptSessionEntity`、`drive_session`、`SessionRegistry`、`SessionPhase`、`parse_confirmation`、`replay_inputs`），本 crate 不含本地逻辑。

## 6. 注册中心（`registry.rs`）

- 类型别名（`registry.rs:9-11`）：`WorkflowGraphRegistry` / `WorkflowExecutionRegistry` / `WorkflowScriptFlowRegistry`，基于 `wf_core::registry::ConcurrentRegistry`。
- `ScriptRegistry`（`registry.rs:21-120`）：完整 `ScriptDefinition` 蓝图的 DashMap。
- `WorkflowRegistry`（`registry.rs:128-203`）：图 + 脚本 + 流的组合注册表；进程级单例 `WorkflowRegistry::global()`（`OnceLock`，`registry.rs:144-147`）；**图重注册原地替换**，更新无需重启即生效（`registry.rs:158-163`）；自由函数（`register_graph` / `lookup_flow` 等）委托全局实例（`registry.rs:213-250`）；工厂 `create_graph_registry` / `create_execution_registry`（`registry.rs:252-258`）。

## 7. 可观测性

### Hook（`hook.rs`）

- `WorkflowHookEmitter`（`hook.rs:14-98`）：`fire_workflow_point` / `fire_point` 构建 `HookContext` 调共享 `fire`，并在事件总线发布 `HOOK_TRIGGERED` 审计事件（`hook.rs:22-67`）；返回 `FireSummary`（可否决）。
- `maybe_hook_checkpoint`（`hook.rs:77-97`）：仅为**实际触发且选择加入**的定义创建 hook checkpoint，尊重总开关但不受触发器列表限制。

### 数据降级事件（`degradation.rs`）

fail-open 场景的可观测性：解析失败导致引擎带降级/空数据继续时，`emit_data_degradation`（`degradation.rs:21-58`）发布携带 `data_degraded` 标记 + 失败点 + 详情的 `NODE_CUSTOM_EVENT`，让审计与触发模板能感知"数据被静默丢弃"；无事件总线时为 no-op，失败只记日志不传播。

### 事件总线使用点

触发器扇入订阅（`subscription.rs:94-111`）、checkpoint 完成与执行状态变更事件（`checkpoint/coordinator.rs:592+`、`:113-116`）、hook 审计事件（`hook.rs:39-46`）、降级事件（`degradation.rs:44-57`）。事件发布失败路径由 `tests/event_publish_negative.rs` 覆盖。
