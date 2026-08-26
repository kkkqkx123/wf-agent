# Workflow 模块对比分析：Codex vs wf-agent（rs 版，基于真实代码）

> 分析范围：仅依据两个仓库 **当前 Rust 源码**（`codex/codex-rs/core`、`wf-agent/crates/wf-workflow`、`wf-agent/crates/wf-core`），**不采信任何 `docs/`（均为已废弃 ts 版描述）**。
> 前置分析见 `agent-execution-instance-analysis.md`（Agent 执行实例对比），本文聚焦 **Workflow / DAG 编排** 这一层。

---

## 1. 核心结论速览

| 维度 | Codex | wf-agent (rs / wf-workflow) |
|------|-------|------------------------------|
| 是否有 DAG 引擎 | **无**（grep 全仓无 topolog/DAG/scheduler） | **有**（`GraphTraversal` + `WorkflowCoordinator`） |
| 编排范式 | **动态、LLM 驱动**：agent loop + `spawn_agent`/`wait`/`message`/`plan` 工具 | **静态、声明式**：先定义图，再按节点驱动 |
| 并行模型 | LLM 自行 `spawn` 多个子 agent 再 `wait` | 顶层**线性单指针**，仅 `FORK`/`JOIN` 节点并行 |
| 容量闸门 | `AgentControl` spawn slot + 子 agent 深度限制 | 无全局闸门；`TaskScheduler` 存在但**未被使用** |
| 检查点/恢复 | 无 workflow 级检查点（按 turn 续跑） | **真实落地且测试覆盖**（`resume_workflow`） |
| 状态机守卫 | 多层校验 | 高层 `WorkflowStateMachine` 有守卫；**运行态 `WorkflowExecutionState` 裸赋值** |

---

## 2. Codex 的"Workflow"：没有工作流引擎，靠工具编排

Codex 在 `codex-rs/core/src` 中**不存在 DAG / 拓扑排序 / workflow 图引擎**（已全文 grep 确认）。其"多步编排"完全依赖：

- **Agent Loop 本身**：`session_loop` 后台任务 + `run_turn` 单次采样循环（见前置分析）。
- **多 Agent 编排工具**（`tools/handlers/multi_agents_v2/`）：
  - `spawn.rs` → `spawn_agent`（派生子 agent，含 `next_thread_spawn_depth` 深度控制，`spawn.rs:62`）
  - `wait.rs` → `wait`（等待某个子 agent 完成）
  - `send_message.rs` / `message_tool.rs` → 跨 agent 消息
  - `interrupt_agent.rs` / `list_agents.rs` / `followup_task.rs`
- **Plan 工具**（`tools/handlers/plan.rs` + `plan_spec.rs`）：让模型先产出分步计划，再逐步执行。

设计取向：编排是**涌现式**的——模型靠工具调用自己决定何时分叉、何时汇聚、何时等待。好处是灵活、无需预定义图；代价是不确定性高、无法静态校验依赖、难以暂停/恢复整张"图"。

---

## 3. wf-agent (rs) 的 Workflow 执行实例设计

### 3.1 实体：`WorkflowExecutionEntity`（`entity.rs:12-29`）

与 Agent Loop 的 `AgentLoopEntity` 同源，是一个**富状态执行实体**，实现 `ExecutionEntity`：

```text
id / workflow_id
state:            Arc<RwLock<WorkflowExecutionState>>   // 运行态
interruption:     InterruptionState                      // 暂停/取消信号
cancellation:     CancellationToken                     // 取消令牌
variables:        Arc<DashMap<String, Value>>           // 可读写变量
node_results:     Arc<DashMap<String, Value>>           // 各节点输出
current_node_id:  Arc<RwLock<Option<String>>>
parent/child_execution_ids                              // 层级关系（目前 inert）
output:           Arc<RwLock<Option<Value>>>             // 最终结果
retry_budget:     Option<Arc<RetryBudget>>              // 全局重试预算
```

### 3.2 执行链路

```text
WorkflowExecutor.execute_workflow        (executor.rs)      —— 入口、注册图、装配 handlers
  └─ WorkflowLifecycleCoordinator.execute_workflow (coordinator/lifecycle.rs)
        ├─ GraphValidator::validate        —— 图结构校验
        ├─ WorkflowStateMachine::start      —— 高层状态机（有守卫）
        ├─ 构建 WorkflowExecutionEntity
        ├─ 装配 WorkflowCoordinator (+ checkpoint)
        └─ coordinator.execute()            —— 主循环
              └─ while let Some(node_id) = self.current_node_id   // 单指针线性遍历
                    ├─ NodeCoordinator.execute_node
                    │     └─ NodeHandler::execute   （20+ 节点类型分派）
                    └─ determine_next_node          // 选「下一条」边，单一后继
```

### 3.3 图遍历：`GraphTraversal`（`graph.rs`）

提供图校验（节点/边/起止/可达性）与查询 API，其中：

- `find_ready_nodes(completed)`（`graph.rs:121`）：**正确的 DAG 就绪原语**（依赖全部完成即可运行）。
- ⚠️ **但该方法是死代码**——顶层 `WorkflowCoordinator` 从未调用它，而是用单一 `current_node_id` 指针线性推进（`coordinator/workflow.rs:501`）。

### 3.4 并行模型：**顶层线性，FORK/JOIN 才并行**

这是 wf-workflow 最关键的设计事实：

- 顶层 `WorkflowCoordinator.execute_inner` 是 `while let Some(node_id) = self.current_node_id` 的**单指针循环**（`coordinator/workflow.rs:501`）。
- `determine_next_node`（`workflow.rs:1064-1108`）对当前节点的出边**只挑一条**：无条件的取第一条，有条件的取第一条命中的。**它不会同时激活多个后继**。
- 真正的并行仅发生在 `FORK` 节点：`ForkHandler`（`handler/fork_join.rs:153`）把每个分支子图抽出来，用 `tokio::task::JoinSet` 并发 `execute_branch`（`fork_join.rs:287-348`），每个分支内部再递归一个 `WorkflowCoordinator`；`JoinHandler` 负责聚合输出。

> **菱形合并缺陷（无 FORK/JOIN 时）**：图 `A→B, A→C, B→D, C→D` 在顶层会被线性化——A 完成后只走 B→D，C 分支**静默丢弃**。要让并行被正确处理，作者必须显式使用 FORK/JOIN 节点；普通 DAG 的扇出/汇聚在顶层不被支持。

`SUBGRAPH`/`EMBED` 节点（`handler/subgraph.rs:146`、`handler/embed.rs:119`）也都是**同步串行** `coordinator.execute().await`，不并行。

### 3.5 状态机：两层分裂

| 层 | 类型 | 守卫 | 位置 |
|----|------|------|------|
| 高层 | `wf_core::WorkflowStateMachine` | ✅ `is_valid_transition` / `is_terminal`（`state.rs:285`），非法转移返回 `CoreError::InvalidStateTransition` | `lifecycle.rs:126` |
| 节点/运行态 | `WorkflowExecutionState`（entity 内部） | ❌ `pause/resume/complete/fail/cancel` 全是裸 `self.status = ...`（`state.rs:148-175`） | entity 持有 |
| 节点级 | `NodeStateMachine` | ✅ 有守卫（`state.rs:107-120`） | **但未被 coordinator 接线使用** |

即：高层流程有守卫，但执行实体的运行时状态仍走裸赋值——与 Agent Loop 的 `AgentLoopState` 问题同源，只是多了一层未被连接的"正确实现"。

### 3.6 重试 / 超时 / 失败（相对成熟）

- `NodeRetryConfig`（`workflow.rs:96-193`）：解析顺序 节点配置 > 类型默认 > 全局；`LLM`/`AGENT_LOOP` 默认 `retry(3)` + 指数退避。
- `on_failure`：`retry` / `continue` / `fallback`（带 `fallback_output`）。
- 节点超时：`tokio::time::timeout` 包裹（`workflow.rs:647`）。
- 全局墙钟超时 `max_execution_time`、步数上限 `max_steps`、死循环保护 `navigation_count > 节点数*5`。
- 重试预算 `RetryBudget`：fork 分支 `allocate_branch_budgets` 切片共享。
- ⚠️ **重试路径与主路径大量重复**：`workflow.rs:678-912` 中节点成功/失败后的记账（写 node_results、mark_node_completed、record_node_execution、emit_event、determine_next）在 retry 块里几乎整段重抄，违反 DRY。

### 3.7 检查点 / 恢复：**真实且测试覆盖**（优于 Agent Loop）

- `WorkflowCheckpointIntegration` 钩子：`on_workflow_start` / `on_node_before` / `on_node_completed` / `on_node_failed` / `on_workflow_end` / `on_interruption`。
- `WorkflowLifecycleCoordinator::resume_workflow`（`lifecycle.rs:241`）加载最新快照 → 重建 entity → `coordinator.resume_from(snapshot)` 注入已完成节点输出 → 从检查点续跑。
- `lifecycle.rs` 内有 9+ 个 `#[tokio::test]`（含 `test_execute_then_resume`、`test_resume_from_before_node_checkpoint` 等），**证明恢复闭环是活的**（对比 Agent Loop 的 `restore_checkpoint` 是死代码）。

### 3.8 触发器：`process_trigger_effects`（`workflow.rs:944-967`）

节点内写 `__trigger_stop` / `__trigger_pause` 标记变量 → 循环末尾翻译成 entity 的 `interruption`。这带来与 Agent Loop **相同的双暂停语义分歧**：`status == Paused` 与 `InterruptionState::Pause` 并存。

---

## 4. wf-workflow 的主要不足（均带 `文件:行` 实证）

### P0 — 正确性问题

1. **顶层 DAG 不支持并行/汇聚，菱形结构静默丢分支**
   `coordinator/workflow.rs:501`（`while let Some(node_id)` 单指针）+ `:1064-1108`（`determine_next_node` 只取单边）。`GraphTraversal::find_ready_nodes`（`graph.rs:121`）本是可用的就绪队列原语，但**未被调用**。后果：非 FORK/JOIN 的扇出图执行结果错误。

2. **运行态状态机无守卫**
   `state.rs:148-175` 的 `pause/resume/complete/fail/cancel` 是裸赋值；`NodeStateMachine`（`wf-core/src/state.rs:107`）的守卫实现存在却没被 `WorkflowCoordinator` 接线。与 Agent Loop 的 state transitor bug 同根。

3. **`get_hierarchy_depth()` 恒返回 0**（`entity.rs:220-222`）
   `parent/child_execution_ids` 有字段但层级深度从不计算，父子执行关系实际上是 inert 的。

### P1 — 设计/一致性问题

4. **`TaskScheduler` 投入却闲置**
   `wf-core/src/scheduler.rs` 实现了优先级 + 公平 + `max_concurrent` 的任务调度器，但 `wf-workflow` 内 grep **零引用**（`handler/fork_join.rs` 只用 `JoinSet`，无并发预算）。顶层节点并发完全无调度，与这份"重型基础设施"投入矛盾。

5. **双暂停语义并存**
   `workflow.rs:944-967`（`__trigger_*` → `InterruptionState`）与 `state.rs` 的 `status == Paused` 两套机制共存，易在恢复/查询路径分歧。

6. **节点间耦合靠 `Any::downcast`**
   `handler/fork_join.rs:22-35`（`resolve_handlers`）、`subgraph.rs:18-33` 通过 `ctx.handler_registry.downcast::<HashMap<...>>()` 与 `ctx.graph_structure.downcast::<WorkflowGraphStructure>()` 取回上下文——脆弱的运行时类型转换，改类型即崩。

### P2 — 可维护性

7. **重试路径大面积复制主路径**
   `workflow.rs:678-912`，节点完成记账（node_results / mark_node_completed / record_node_execution / determine_next）在 retry 块里整段重抄，且 `determine_next_node` 与 `determine_next_node_without_output` 逻辑高度重叠。

8. **节点类型爆炸、handler 各自接线**
   `parse_node_type`（`workflow.rs:48-74`）枚举 **20+ 静态节点类型**（LLM/SCRIPT/AGENT_LOOP/ROUTE/LOOP/SUBGRAPH/…），每个 handler 都重复一遍 context 装配，扩展性靠堆 handler。

---

## 5. 改进建议（按优先级）

| 优先级 | 动作 | 对应不足 |
|--------|------|----------|
| P0 | 顶层改用 `find_ready_nodes` 驱动就绪队列，激活所有入依赖满足的节点（或在 FORK/JOIN 之外补一个"并行扇出"通道） | #1 |
| P0 | 让 coordinator 走 `NodeStateMachine`/`WorkflowStateMachine` 的守卫转移，消除裸赋值 | #2 |
| P0 | 真实计算 `get_hierarchy_depth`（沿 `parent_execution_id` 上溯计数） | #3 |
| P1 | 把 `TaskScheduler` 接入顶层并发（或在 FORK 之外提供全局并发预算），别让基础设施空转 | #4 |
| P1 | 统一暂停语义：只保留 `InterruptionState` 单一真相源 | #5 |
| P1 | 用 trait/泛型替换 `downcast`，或将 handler 上下文改为强类型 | #6 |
| P2 | 抽取 `run_node_and_record` 复用主/重试路径；合并两个 `determine_next_*` | #7 |
| P2 | 用 macro/注册表收敛 20+ 节点类型装配 | #8 |

---

## 6. 与 Agent Loop 模块的交叉观察

- **检查点成熟度倒挂**：Agent Loop 的 `restore_checkpoint` 是死代码（前篇已述），而 Workflow 的 `resume_workflow` **真实可用且测试充分**——说明两个模块并非统一进度，Workflow 反而更成熟。
- **状态机问题同源**：`WorkflowExecutionState` 与 `AgentLoopState` 都采用无守卫裸赋值；`wf-core` 里其实已有带守卫的 `WorkflowStateMachine`/`NodeStateMachine`，但两个执行模块的 coordinator 都没接上——这是**共享基础设施未被复用的系统性问题**，建议优先统一。
- **层级深度 stub 同源**：`AgentLoopEntity.get_hierarchy_depth()` 与 `WorkflowExecutionEntity.get_hierarchy_depth()` 都返回 0，父子关系在两套执行实例里都是 inert 的。
