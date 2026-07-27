# Execution 模块完整实施计划

## 当前状态

已完成骨架创建：
- `wf-execution-shared` — 基础 trait + 核心类型 + ConditionEvaluator + 测试通过
- `wf-agent` — Entity/State/Executor/Coordinator 骨架，编译通过
- `wf-workflow` — Entity/State/Coordinator/Handler 骨架，编译通过

## Phase 1: wf-execution-shared 完善

### 1.1 中断系统完善

**文件**: `interruption/state.rs`, `interruption/check.rs`, `interruption/handler.rs`

```
InterruptionState
├── pause() -> 发送 Pause 信号
├── stop() -> 发送 Stop 信号
├── check() -> 返回当前中断状态
└── connect_to_parent(parent: Arc<InterruptionState>) -> 级联中断

check_execution_interruption(state: &InterruptionState) -> ExecutionInterruptionCheckResult
├── Continue — 继续执行
├── Paused { iteration } — 已暂停
├── Stopped { iteration } — 已停止
└── Aborted { reason } — 已中止

execute_with_interruption_handling<T, F>(state: &InterruptionState, f: F) -> Result<T>
├── 执行前检查中断
├── 执行操作
└── 执行后检查中断
```

### 1.2 Hook 系统完善

**文件**: `hooks/executor.rs`

```
HookExecutor
├── filter_andSortHooks(hooks, hook_type) -> Vec<BaseHookDefinition>
│   ├── 按 hook_type 过滤
│   ├── 按 enabled=true 过滤
│   └── 按 weight 降序排序
├── evaluateHookCondition(condition, context) -> bool
│   └── 复用 ConditionEvaluator
├── executeSingleHook(hook, ctx) -> HookExecutionResult
│   └── 动态调用 hook handler
└── executeHooks(hooks, ctx, config) -> Vec<HookExecutionResult>
    ├── parallel=true → join_all 并行
    └── parallel=false → 串行执行
```

### 1.3 消息上下文注册表

**文件**: `messaging/message_context_registry.rs`

```
MessageContextRegistry
├── set(name, messages) — 设置命名上下文
├── get(name) -> Option<Vec<Message>>
├── append(name, message) — 追加消息
├── remove(name)
└── list() -> Vec<String>
```

### 1.4 执行池 + 队列

**文件**: `pool/execution_pool.rs`, `pool/execution_queue.rs`

```
ExecutionPool
├── new(max_concurrent) — 基于 Semaphore
├── acquire() -> SemaphorePermit
├── active_count() -> usize
└── available_permits() -> usize

ExecutionQueue<T>
├── push(item)
├── pop() -> Option<T)
├── len() / is_empty()
└── clear()
```

### 1.5 错误链管理器

**文件**: `error_chain/manager.rs`

```
ErrorChainManager
├── record(execution_id, error, node_id)
├── get_records(execution_id) -> Vec<ErrorRecord>
├── get_chain(execution_id) -> Vec<ErrorRecord>
└── clear(execution_id)
```

### 验收标准
- [ ] 所有单元测试通过
- [ ] InterruptionState 级联测试
- [ ] Hook 并行/串行执行测试
- [ ] MessageContextRegistry CRUD 测试

---

## Phase 2: wf-agent 核心实现

### 2.1 AgentLoopEntity 完善

**文件**: `entity/agent_loop.rs`

```
AgentLoopEntity
├── id: Id
├── state: Arc<RwLock<AgentLoopState>>
├── interruption: InterruptionState
├── conversation: Arc<RwLock<ConversationSession>>
├── cancellation: CancellationToken
├── parent_execution_id: Option<Id>
├── child_execution_ids: Arc<RwLock<Vec<Id>>>
├── status() -> ExecutionStatus
├── pause() / resume() / stop() / abort()
├── register_child(child_id) / unregister_child(child_id)
├── get_available_tools() -> Vec<Tool>
└── lock_tool_call_format() -> ToolCallFormat
```

### 2.2 AgentLoopState 完善

**文件**: `state/agent_loop_state.rs`

```
AgentLoopState (已实现基础)
├── status: ExecutionStatus
├── current_iteration: u32
├── tool_call_count: u32
├── iteration_history: Vec<IterationRecord>
├── start_time / end_time: i64
├── error: Option<String>
├── error_records: Vec<ErrorRecord>
├── variable_snapshots: HashMap<String, Value>
├── start() / startIteration() / endIteration()
├── recordToolCall()
├── pause() / resume() / complete() / fail() / cancel()
├── createSnapshot() -> AgentLoopStateSnapshot
└── restoreFromSnapshot(snapshot)
```

### 2.3 AgentIterationCoordinator

**文件**: `coordinator/iteration.rs`

```
AgentIterationCoordinator::executeIteration(entity) -> IterationResult
├── 1. executeHook(BEFORE_ITERATION)
├── 2. entity.state.startIteration()
├── 3. checkInterruption()
├── 4. executeHook(BEFORE_LLM_CALL)
├── 5. build LLM request (messages + tools)
├── 6. LLM call via LlmExecutionCoordinator
├── 7. checkInterruption()
├── 8. executeHook(AFTER_LLM_CALL)
├── 9. add assistant message to conversation
├── 10. if no tool_calls → return shouldContinue=false
├── 11. if tool_calls:
│    ├── executeHook(BEFORE_TOOL_CALL) per tool
│    ├── ToolExecutionCoordinator.executeToolCalls()
│    ├── executeHook(AFTER_TOOL_CALL) per tool
│    └── check attempt_completion tool
├── 12. entity.state.endIteration()
├── 13. executeHook(AFTER_ITERATION)
└── 14. return IterationResult { should_continue, content }
```

### 2.4 AgentExecutionCoordinator

**文件**: `coordinator/execution.rs`

```
AgentExecutionCoordinator::execute(entity, max_iterations) -> AgentLoopOutput
├── 1. RetryBudget::new(config)
├── 2. loop with retry:
│    └── executeIterationWithRetryAndTimeout()
│        ├── checkInterruption()
│        ├── iterationCoordinator.executeIteration()
│        ├── on success → check should_continue
│        └── on failure → retry if budget allows
├── 3. if max_iterations reached → return output
└── 4. stateTransitor.completeAgentLoop() / failAgentLoop()
```

### 2.5 AgentLoopCoordinator (生命周期)

**文件**: `coordinator/lifecycle.rs`

```
AgentLoopCoordinator
├── execute(config, options) -> AgentLoopOutput
│   ├── 1. buildEntity(config) -> AgentLoopEntity
│   ├── 2. stateTransitor.startAgentLoop()
│   ├── 3. executionCoordinator.execute()
│   ├── 4. stateTransitor.complete/fail()
│   └── 5. return result
├── executeStream(config, options) -> impl Stream<Item=AgentLoopStreamEvent>
├── start(config, options) -> Id (异步)
├── pause(id) / resume(id) / stop(id)
└── continue_execution(id, messages)
```

### 2.6 ToolExecutionCoordinator

**文件**: `coordinator/tool.rs`

```
ToolExecutionCoordinator::executeToolCalls(entity, tool_calls) -> Vec<Message>
├── 1. if no toolApprovalHandler → execute all directly
├── 2. if toolApprovalHandler:
│   ├── processToolBatch() → auto-approve + confirmation-required
│   ├── execute auto-approved tools
│   └── handle confirmation-required tools
├── 3. per tool:
│   ├── executeHook(BEFORE_TOOL_CALL)
│   ├── register tool timeout
│   ├── combine abort signals
│   ├── toolCallExecutor.execute()
│   ├── record result in entity.state
│   └── executeHook(AFTER_TOOL_CALL)
└── 4. return Vec<Message> (tool results)
```

### 2.7 ExecutionCallback 实现

**文件**: `callback/execution_callback.rs`

```
impl ExecutionCallback for AgentLoopExecutor
├── execute_agent_loop(config, input) -> AgentLoopOutput
│   └── AgentLoopCoordinator.execute()
├── execute_workflow(workflow_id, input) -> WorkflowOutput
│   └── 委托给 wf-workflow (通过 trait 对象)
├── query_execution_status(execution_id) -> ExecutionStatus
│   └── 从 ExecutionRegistry 查询
└── cancel_execution(execution_id)
    └── 从 ExecutionRegistry 获取 entity → abort()
```

### 验收标准
- [ ] Agent loop 单次迭代完整执行
- [ ] Hook 在正确时机触发
- [ ] 工具执行结果正确追加到消息历史
- [ ] 中断信号正确传播
- [ ] Checkpoint 可序列化/恢复

---

## Phase 3: wf-workflow 核心实现

### 3.1 WorkflowExecutionEntity 完善

**文件**: `entity/workflow_execution.rs`

```
WorkflowExecutionEntity
├── id: Id
├── workflow_id: Id
├── state: Arc<RwLock<WorkflowExecutionState>>
├── interruption: InterruptionState
├── cancellation: CancellationToken
├── variables: Arc<DashMap<String, Value>>
├── node_results: Arc<DashMap<String, Value>>
├── current_node_id: Option<String>
├── parent_execution_id: Option<Id>
├── child_execution_ids: Arc<RwLock<Vec<Id>>>
├── getVariable(name) / setVariable(name, value)
├── getNodeResult(node_id) / setNodeResult(node_id, value)
├── registerChild(child_id) / unregisterChild(child_id)
└── pause() / resume() / stop() / abort()
```

### 3.2 WorkflowExecutionState 完善

**文件**: `state/workflow_execution_state.rs`

```
WorkflowExecutionState
├── status: ExecutionStatus
├── current_node_id: Option<String>
├── completed_nodes: Vec<String>
├── start_time / end_time: i64
├── error: Option<String>
├── error_records: Vec<ErrorRecord>
├── operation_state: Option<OperationState>
├── start() / pause() / resume() / complete() / fail() / cancel()
├── createSnapshot() -> WorkflowExecutionStateSnapshot
└── restoreFromSnapshot(snapshot)
```

### 3.3 NodeCoordinator

**文件**: `coordinator/node.rs`

```
NodeCoordinator::executeNode(entity, node, handler) -> NodeExecutionResult
├── 1. emit NodeStarted event
├── 2. executeHook(BEFORE_EXECUTE)
├── 3. checkpointStrategy.maybeCreateCheckpoint()
├── 4. executeWithRetry()
│   ├── handler.execute(ctx)
│   ├── on failure → retry if budget allows
│   └── on success → return result
├── 5. executeHook(AFTER_EXECUTE)
├── 6. emit NodeCompleted / NodeFailed event
└── 7. return result
```

### 3.4 WorkflowCoordinator 完善

**文件**: `coordinator/workflow.rs`

```
WorkflowCoordinator::execute() -> Value
├── 1. emit WorkflowExecutionStarted
├── 2. while current_node_id.isSome():
│   ├── checkInterruption()
│   ├── skip completed nodes (resume support)
│   ├── node = graph.get_node(current_node_id)
│   ├── handler = handlers.get(node.type)
│   ├── nodeCoordinator.executeNode(entity, node, handler)
│   ├── on success:
│   │   ├── store node output
│   │   ├── insert metadata to variables
│   │   └── navigateToNextNode()
│   │       ├── handler-specified next_node_ids
│   │       ├── single edge → follow
│   │       ├── multiple edges → evaluate conditions
│   │       └── no edges → stop
│   └── on failure:
│       ├── emit NodeFailed
│       └── return error
├── 3. computeFinalOutput()
└── 4. emit WorkflowExecutionCompleted
```

### 3.5 节点 Handler 实现

**文件**: `handler/` 下各文件

| Handler | 实现要点 |
|---------|---------|
| `start_end.rs` | 直接透传 input → output |
| `route.rs` | 评估 branches 条件，返回 with_next_nodes |
| `variable.rs` | 解析 assignments，写入 variables |
| `loop.rs` | LoopStart 设置计数器，LoopEnd 递增+条件回跳 |
| `fork_join.rs` | Fork 并行执行分支，Join 聚合结果 |
| `agent_loop.rs` | 桥接 wf-agent，创建 AgentLoopCoordinator |
| `llm.rs` | 从 context 取消息，调用 LLM，写回 context |
| `subgraph.rs` | 递归调用 WorkflowCoordinator |
| `context_processor.rs` | set/remove/transform 变量操作 |
| `sync.rs` | 透传 |
| `script.rs` | 调用 wf-sandbox 执行脚本 |
| `user_interaction.rs` | 等待外部输入事件 |
| `tool_visibility.rs` | 透传（stub） |
| `embed.rs` | 透传（stub） |
| `trigger.rs` | 透传（stub） |

### 3.6 AgentLoopNodeHandler (桥接)

**文件**: `handler/agent_loop.rs`

```
AgentLoopNodeHandler::execute(ctx) -> NodeExecutionResult
├── 1. resolveRuntimeConfig(node.config)
│   ├── agentLoopId → 从 registry 获取配置
│   └── inlineConfig → 直接使用
├── 2. collectInitialMessages(messageInputs)
│   └── 从 MessageContextRegistry 读取
├── 3. map dataInputs → internal variables
├── 4. create AgentLoopCoordinator
├── 5. coordinator.execute(config, options)
│   └── parentExecutionId = ctx.execution_id
│   └── nodeId = ctx.node_id
├── 6. syncMessageOutputs() → 写回 MessageContextRegistry
├── 7. syncCompletionData() → 写回 VariableManager
└── 8. return NodeExecutionResult { finalResponse, iterationCount, toolCallCount }
```

### 3.7 Fork/Join 完整实现

**文件**: `handler/fork_join.rs`, `barrier.rs`

```
ForkNodeHandler::execute(ctx) -> NodeExecutionResult
├── 1. 读取 branches 配置
├── 2. 创建 SyncBarrier(expected_count)
├── 3. 为每个 branch 创建子执行:
│   ├── 构建子图上下文
│   ├── tokio::spawn 并行执行
│   └── 每个分支完成后 notify_branch_completed()
├── 4. wait_for_all() 或 join_all
├── 5. 收集 BranchResult
├── 6. failureStrategy.evaluate() → ForkOutcome
└── 7. 返回聚合结果或错误

JoinNodeHandler::execute(ctx) -> NodeExecutionResult
├── 1. 读取 join strategy
├── 2. 收集所有入边输出
├── 3. 按策略聚合:
│   ├── first → 取第一个
│   ├── last → 取最后一个
│   ├── merge → 合并所有
│   └── aggregate → 自定义聚合
└── 4. 返回聚合结果
```

### 验收标准
- [ ] START→END 两节点 workflow 可执行
- [ ] 含变量、条件路由的 workflow 可执行
- [ ] Fork/Join 并行执行正确
- [ ] Agent loop 节点桥接成功
- [ ] 节点 hook 正确触发
- [ ] 中断级联传播正确

---

## Phase 4: Checkpoint 集成

### 4.1 Agent Checkpoint

**文件**: `wf-agent/src/checkpoint/coordinator.rs`

```
AgentCheckpointCoordinator
├── createCheckpoint(entity) -> Checkpoint
│   ├── 构建 config layers
│   ├── 创建 AgentLoopState 快照
│   └── 通过 storage adapter 持久化
├── restoreFromCheckpoint(checkpoint_id, config) -> AgentLoopEntity
│   ├── 加载序列化状态
│   ├── 重建 managers
│   └── 恢复执行
└── shouldCreateCheckpoint(strategy, entity) -> bool
```

### 4.2 Workflow Checkpoint

**文件**: `wf-workflow/src/checkpoint/coordinator.rs`

```
WorkflowCheckpointCoordinator
├── createCheckpoint(entity) -> Checkpoint
├── restoreFromCheckpoint(checkpoint_id) -> WorkflowExecutionEntity
└── NodeCheckpointStrategy
    ├── BeforeNode / AfterNode
    ├── OnError
    └── Interval(u32)
```

### 验收标准
- [ ] Agent 执行中断后可从 checkpoint 恢复
- [ ] Workflow 执行中断后可从 checkpoint 恢复
- [ ] Delta diff 正确计算
- [ ] 恢复后变量状态一致

---

## Phase 5: 集成测试 + 端到端

### 5.1 测试场景

| 场景 | 覆盖 |
|------|------|
| 简单 workflow | START → Variable → END |
| 条件路由 | START → ROUTE → (分支A / 分支B) → END |
| 循环 | START → LOOP_START → ... → LOOP_END → END |
| Fork/Join | START → FORK → (并行分支) → JOIN → END |
| Agent 节点 | START → AGENT_LOOP → END |
| 嵌套子图 | START → SUBGRAPH → END |
| 中断恢复 | 执行中暂停 → 恢复 → 完成 |
| Checkpoint | 执行中 checkpoint → 恢复 → 完成 |

### 5.2 集成测试文件

```
crates/wf-agent/tests/
├── agent_loop_test.rs
└── checkpoint_test.rs

crates/wf-workflow/tests/
├── simple_workflow_test.rs
├── route_workflow_test.rs
├── loop_workflow_test.rs
├── fork_join_workflow_test.rs
├── agent_node_test.rs
├── subgraph_test.rs
├── interruption_test.rs
└── checkpoint_test.rs
```

---

## Phase 6: 性能优化 + 完善

### 6.1 性能优化

- Handler 热点路径使用 `match` 静态分发
- 变量解析缓存（避免重复插值）
- 事件发射异步化（broadcast channel）
- 并行分支使用 JoinSet 管理

### 6.2 错误处理完善

- 错误链跨层传播
- 超时错误精确到节点
- 工具执行失败重试策略
- 优雅降级（fallback output）

### 6.3 可观测性

- 执行追踪（tracing span）
- 指标收集（执行时间、迭代次数、工具调用次数）
- 结构化日志

---

## 依赖关系总结

```
Phase 1 (wf-execution-shared)
    ↓
Phase 2 (wf-agent) ← 依赖 Phase 1 完成
    ↓
Phase 3 (wf-workflow) ← 依赖 Phase 1 + Phase 2 完成
    ↓
Phase 4 (Checkpoint) ← 依赖 Phase 2 + Phase 3 完成
    ↓
Phase 5 (集成测试) ← 依赖 Phase 4 完成
    ↓
Phase 6 (优化) ← 依赖 Phase 5 完成
```

## 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| async-trait 性能 | 热点路径延迟 | 关键路径使用具体类型 |
| 中断级联复杂 | 死锁/泄漏 | 使用 CancellationToken 树 |
| Fork/Join 竞态 | 数据竞争 | DashMap + Notify 模式 |
| Checkpoint 恢复不一致 | 状态丢失 | SHA256 完整性校验 |
| Trait 对象跨 crate | 类型不匹配 | 共享 trait 定义在 shared |
