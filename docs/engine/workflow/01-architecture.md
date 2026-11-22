# wf-workflow 架构与主循环

> 范围：`src/executor.rs`、`src/coordinator.rs` 及 `coordinator/` 子树，辅以 `src/entity.rs`、`src/state.rs`、`src/execution_callback.rs`。
> 所有 `file:line` 相对 `crates/engine/wf-workflow/src/`。

## 1. 执行入口

### 1.1 WorkflowExecutor（`executor.rs`）

- 结构体（`executor.rs:17-26`）：持有可选 `EventBus`、可选类型化 `InternalSignalBus`、共享 `Arc<LlmGateway>`、可选共享 `SandboxRuntime`。
- 构造器族（`executor.rs:52-100`）：`new` / `with_gateway` / `with_event_bus` / `new_default`（内置 1024 容量 EventBus，`executor.rs:81`）/ `with_sandbox` / `with_signal_bus`。
- `WorkflowRunRequest`（`executor.rs:38-49`）：一次执行的全部入参——`workflow_id`、图结构、`WorkflowExecutionOptions`、工具注册表、可选处理器映射、hook 定义、可选资源注册表、可选变量声明（输入默认值与类型检查）。
- `execute_workflow`（`executor.rs:102-143`）：入口流程——未提供处理器集时用沙箱构建默认 `HandlerRegistry`（`executor.rs:117-121`）；图注册进全局注册中心（`executor.rs:123`）；组装 `WorkflowExecutionParams` 后交给 `WorkflowLifecycleCoordinator::execute_workflow`（`executor.rs:138-142`）。

### 1.2 应用侧回调平面（`execution_callback.rs`）

`WorkflowExecutionCallback`（`execution_callback.rs:39-69`）是面向应用层的注册/执行平面：聚合图注册表、执行注册表、处理器、网关、事件/信号总线、checkpoint 策略与存储、指标、hook（共享 + 按工作流）、沙箱、agent 循环注册表。提供同步执行 `execute_workflow`（`execution_callback.rs:504-525`）、带父执行的 `execute_workflow_with_parent`（`execution_callback.rs:527-553`）、脱离式 `spawn_workflow`（`execution_callback.rs:362-430`）、状态查询 `query_execution_status`（`execution_callback.rs:576-604`）与取消 `cancel_execution`（`execution_callback.rs:606-623`）。

## 2. 生命周期协调层（`coordinator/lifecycle.rs`）

`WorkflowLifecycleCoordinator`（`lifecycle.rs:25-42`）持有事件/信号总线、`StorageBackend`、checkpoint 策略与事件总线、指标、`TriggerStateRegistry`、`HookHandlerRegistry`、`FileCheckpointManager`，经 builder 链注入（`lifecycle.rs:45-115`）。

`execute_workflow`（`lifecycle.rs:141-306`）流程：

1. `input_gate::gate_workflow_request` 请求形状与预算门禁（`lifecycle.rs:159`，见 `input_gate.rs:49-67`：拒绝零/负预算）。
2. `GraphValidator::validate` 图校验（`lifecycle.rs:162-174`）。
3. 驱动 `WorkflowStateMachine` 进入 Started（`lifecycle.rs:176-179`）。
4. `resolve_typed_input` 合并类型化输入并回填默认值（`lifecycle.rs:184-188`）。
5. 创建 `WorkflowExecutionEntity` 并用 `execution_budgets::persist_totals` 冻结预算总量（`lifecycle.rs:190-195`）。
6. 组装共享 `ExecutorContext`（`lifecycle.rs:205-243`）与 `WorkflowCoordinator`（`lifecycle.rs:245-255`），计时执行并 settle 状态机与指标（`lifecycle.rs:257-305`）。

`lifecycle/resume.rs` 提供恢复入口：

- `build_resumed_coordinator`（`resume.rs:22-58`）：取最新 checkpoint 快照重建协调器。
- `build_resumed_coordinator_from_checkpoint`（`resume.rs:64-224`）：按显式 checkpoint id 重建；拒绝终态快照，重建预算继承与错误挂起状态。
- `resume_workflow` / `resume_workflow_from_checkpoint`（`resume.rs:231-259` / `resume.rs:264-292`）：公开恢复 API。
- `resume_suspended_error_branch`（`resume.rs:300-318`）：错误分支挂起点的外部恢复通道。

## 3. 主循环协调层（`coordinator/workflow.rs` + `workflow/`）

### 3.1 结构与初始化

`WorkflowCoordinator`（`workflow.rs:29-75`）持有执行上下文、实体、`GraphTraversal`、处理器映射、当前/已完成节点、节点输出、hook、死循环计数器（`navigation_count`、`loop_navigation_count`，`workflow.rs:42-47`）、checkpoint 集成、状态管理器、fork 分支进度发布点、插件处理器、信号接收端与错误分支作用域（`error_scope`，`workflow.rs:74`）。

`new`（`workflow.rs:83-106`）先跑 `preprocess::preprocess_graph`（展开 EMBED_GRAPH，见 `preprocess.rs:29-38`，最大嵌入深度 `MAX_EMBED_DEPTH = 16`，`preprocess.rs:9`）；`new_preprocessed`（`workflow.rs:114-185`）为每个 FORK 节点预建 `ForkRegistry`、解析起始节点、计算导航上限倍率（默认 5，`workflow.rs:136`）、向变量表发布 `loop_max_iterations_cap`（`workflow.rs:141-146`）并订阅信号总线。

### 3.2 主循环 `execute_inner`（`workflow.rs:313-504`）

`execute`（`workflow.rs:301-311`）外围：`persist_start` → `WORKFLOW_BEFORE` 作用域 hook → `execute_inner` → `WORKFLOW_AFTER` hook → `persist_final`（两条退出路径都落库）。

每次迭代（`workflow.rs:343-477`）依次：

1. `check_interruption_and_timeout` 中断/超时检查（`workflow.rs:344`，实现见 `persistence.rs:114-241`）。
2. `apply_pending_variable_injections` 排水节点边界变量注入（`workflow.rs:350`）。
3. `max_steps` 总步数上限（`workflow.rs:352-359`）与 `check_navigation_backstop` 死循环防护（`workflow.rs:361`）。
4. 跳过已完成节点（checkpoint 恢复场景，`workflow.rs:363-368`）与信号请求的节点跳过（`workflow.rs:370-375`）。
5. 解析节点类型与 checkpoint 配置/强制标志（`workflow.rs:387-393`），打节点前 checkpoint（`workflow.rs:395-398`）。
6. 构建节点上下文，经 `NodeAttempt`/`NodeOutcome` 执行单节点（`workflow.rs:413-431`）。
7. 成功：`record_node_success` + `determine_next_node`；失败：`record_node_failure` + `route_node_failure`（返回 Continue/Suspended/Interrupt，`workflow.rs:433-469`）。
8. `process_trigger_effects` 处理触发器副作用（`workflow.rs:476`）。

循环退出后：`compute_final_output`、丢弃错误作用域、实体 `complete()`、发布 `WorkflowExecutionCompleted`、`cp.on_workflow_end`（`workflow.rs:479-503`）。

### 3.3 死循环防护

- 双计数器：`navigation_count`（非循环 DAG 环）与 `loop_navigation_count`（循环体内节点数，进出 LOOP_START/LOOP_END 时重置）；任一超过 `total_node_count × max_navigation_multiplier` 即报错（`navigation.rs:25-76`，上限计算 `workflow.rs:136`）。
- 运行期迭代上限 `loop_max_iterations_cap` 发布给循环处理器（`workflow.rs:141-146`）；`options.max_steps` 限制已完成节点总数。

### 3.4 节点执行与记账（`workflow/node_exec.rs`）

- `execute_node_once`（`node_exec.rs:136-250`）：内建 → 插件处理器解析，构建 `NodeCoordinator`，解析超时，将执行包进超时与 panic 捕获。
- `record_node_execution`（`node_exec.rs:82-118`）：追加封顶的 `NodeExecutionRecord`；`record_workflow_error`（`node_exec.rs:122-132`）持久化结构化错误；`record_node_success` / `record_node_failure`（`node_exec.rs:254-365` / `369-420`）做事件、指标、checkpoint 与执行记录收尾。
- 变量注入排水（`variable_injection.rs:18-108`）：`NextBoundary` 每个节点边界排水、`OnResume` 每次显式恢复排水一次；按变量 last-writer-wins 写入实体并发布 `WorkflowVariableInjected` 审计事件（含合并计数与新旧值）。错误作用域激活期间跳过注入（`variable_injection.rs:27-29`）。

### 3.5 超时与中断（`workflow/timeout.rs`、`workflow/persistence.rs`）

- 节点超时优先级：节点 `timeout_seconds` > options 默认（ms）> 引擎兜底 `DEFAULT_NODE_TIMEOUT_MS = 30s`（`timeout.rs:15`、`timeout.rs:32-49`）；agent 循环、子图、交互类长任务豁免。
- `check_interruption_and_timeout`（`persistence.rs:114-241`）：Stop → 记录中断 + `cp.on_interruption` checkpoint + 返回取消；Pause → `cp.on_pause` + 返回暂停；wall-clock 超时按 `execution_budgets::active_elapsed_ms` 计时（审批等待不消耗预算）。

## 4. 单节点协调层（`coordinator/node.rs`）

`NodeCoordinator::execute_node`（`node.rs:119-338`）运行单个节点：

1. 发布 `NodeStarted`（`node.rs:143-153`）。
2. 触发 `BEFORE_EXECUTE` 门禁 hook，支持否决（veto → 走失败路径）；hook 选择性 checkpoint 在触发后进行（`node.rs:161-201`）。
3. `execute_with_interruption_handling` 将处理器执行与 Stop 信号竞速（`node.rs:207-227`）。
4. 成功：标记节点完成、触发 `AFTER_EXECUTE`（含 checkpoint，`node.rs:229-259`）、发布 `NodeCompleted`。

失败统一走 `fail_node`（`node.rs:347-407`）：ON_ERROR hook、`NodeFailed` 事件、带类别与 `hook_veto` 来源的结构化 `NodeFailureInfo`。

## 5. 错误分支流转（`coordinator/workflow/error_branch_flow.rs`）

- `enter_error_branch`（`error_branch_flow.rs:21-70`）：冻结主变量表，在"入口快照 + 错误命名空间"覆盖层上运行分支，发布 `WorkflowErrorBranchTaken`/`Resumed`。
- `suspend_error_branch`（`error_branch_flow.rs:79-131`）：在错误挂定点停车，类型化恢复记录写入实体状态（非业务变量）。
- `merge_error_scope` / `discard_error_scope`（`error_branch_flow.rs:136-176` / `181-185`）：显式 JOIN 合并回刷覆盖层；未合并到终点则丢弃。
- `route_node_failure`（`error_branch_flow.rs:248-331`）：节点失败进入错误分支机制，返回 `Continue`/`Suspended`/`Interrupt`。详见 [04-validation-and-error-branches.md](04-validation-and-error-branches.md)。

## 6. 实体与状态

- `WorkflowExecutionEntity`（`entity.rs:127-153`）：活体运行对象——共享 `state: Arc<RwLock<WorkflowExecutionState>>`、中断状态、取消令牌、共享变量与节点结果 `DashMap`、`pending_injections` 注入缓冲（不入快照，`entity.rs:138-142`）、层级管理器、一次性 output。变量注入 API（`entity.rs:311-392`）、`pause/resume/stop/abort`（`entity.rs:498-522`）。工厂 `WorkflowExecutionBuilder`（`factory.rs:4-67`）。
- `WorkflowExecutionState`（`state.rs:70-84`）：自有状态机——状态、当前/已完成节点、节点执行历史、错误记录、中断与事件审计、`error_suspend` 挂起状态。迁移 `start/pause/resume/complete/fail/cancel/timeout`（`state.rs:208-237`）带合法性检查（`state.rs:263-285`）；快照 `create_snapshot`/`restore_from_snapshot`（`state.rs:393-432`）。

## 7. 输入门禁与预算（`input_gate.rs`、`execution_budgets.rs`）

- `gate_workflow_request`（`input_gate.rs:49-67`）：请求形状与运行时预算校验；`resolve_typed_input`（`input_gate.rs:72-108`）回填声明默认值并执行类型/必填检查，未声明变量直通。
- 预算跨挂起/恢复：checkpoint 只存冻结总量与 `elapsed_ms`，不存剩余值；`persist_totals`（`execution_budgets.rs:25-39`）落冻结总量，审批等待经 `begin/end_approval_wait` 不计入预算（`execution_budgets.rs:93-124`），`continuation_options`（`execution_budgets.rs:170-245`）重算余量并在预算耗尽时拒绝恢复。详见 [06-checkpoint-persistence.md](06-checkpoint-persistence.md)。
