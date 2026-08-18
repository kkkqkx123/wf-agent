# Agent 执行实例设计对比分析：Codex vs wf-agent (rs)

> 分析范围说明：**本分析完全基于两个仓库的实际 Rust 源码**（Codex 的 `codex-rs/core/src`，wf-agent 的 `crates/wf-agent/src` 及其相关 crate），**未引用任何 ts 版文档或 `docs/` 下的架构说明**——那些内容描述的是已废弃的 ts 实现，与当前 rs 代码不符。所有结论均带 `文件:行号` 证据。

---

## 1. 「Agent 执行实例」在两套代码里到底是什么

| 维度 | Codex | wf-agent (rs) |
|---|---|---|
| 实例类型 | `CodexThread`（轻量句柄） + `Session`（长生命周期 actor） | `AgentLoopEntity`（有状态结构体，存于 `AgentLoopRegistry`） |
| 实例数量 | 每 thread 一个 `Session`，后台常驻 | 每 run 一个 `AgentLoopEntity`，`run` 结束即不再驱动 |
| 驱动方式 | 后台 `session_loop` 任务收 `Op`，`run_turn` 做单次采样 | `AgentLoopExecutor` 调 `AgentLoopCoordinator` 同步跑或 `spawn` 后台 task |
| 并发/层级控制 | `AgentControl` + `AgentRegistry`（spawn slot + 子 agent 深度限制） | 无等价闸门；`AgentLoopRegistry` 是无上限 `DashMap` |
| 恢复能力 | `recover_turn_if_idle(turn_id)` 按精确 turn 续跑 | 检查点只写不驱动重跑；`pause` 实际 break 出循环无法重入 |

---

## 2. Codex 的执行实例设计（实际代码）

### 2.1 句柄 + Actor：`CodexThread` 与 `Session`

`CodexThread` 本身极薄，只是会话的「门面句柄」：

- `struct CodexThread { session: Arc<Session>, io: SessionIo, ... }`（`codex_thread.rs:149-157`）
- 所有能力都委托给 `session`：`submit(op)`、`start_or_steer_turn`、`recover_turn_if_idle` 等（`codex_thread.rs:194-351`）

真实的执行实例是 `Session`，它由**一个常驻后台任务**承载：

- `tokio::spawn(submission_loop(...))` 在会话创建时启动（`session/mod.rs:777-781`）
- `SessionIo` 内部是 `tx_sub`（发 `Op`）/`rx_event`（收 `Event`）的 **mpsc 通道**（actor 模型）（`session/mod.rs:793-858`）
- 调用方只往通道提交 `Op`，真正的执行在 `submission_loop` 里推进，与调用方解耦

### 2.2 单次采样循环：`run_turn`

`run_turn(sess, turn_context, input, ...)`（`session/turn.rs:153`）负责一次 turn 的完整编排：预压缩 → 注入 hooks/skills/plugins → 捕获 step context → 调用模型 client → 处理流式响应 → 工具调用。它由 `tasks/regular.rs` 中的 `loop` 反复调用，形成「模型→工具→模型」的 agent loop。

### 2.3 硬约束内建：执行容量与子 agent 深度

这是 Codex 设计里最关键的一块，wf-agent 完全没有对等物：

- `AgentControl` 持有 `agent_execution_limiter: Arc<AgentExecutionLimiter>` 与 `state: Arc<AgentRegistry>`（`agent/control.rs:104-119`）
- 每次开启 turn 前都先校验容量：`ensure_execution_capacity_for_turn_start(self)`（`codex_thread.rs:308-310`、`agent/control.rs:219`、`:362`）
- 子 agent 受**数量 + 深度**双重闸门：
  - `AgentRegistry::reserve_spawn_slot(max_threads)`（`agent/registry.rs:81-100`）——超出返回 `AgentLimitReached`
  - `exceeds_thread_spawn_depth_limit(depth, max_depth)`（`agent/registry.rs:76-78`）——递归子 agent 深度限制
  - `next_thread_spawn_depth` / `session_depth`（`agent/registry.rs:64-74`）——depth 沿 `SessionSource::SubAgent(ThreadSpawn{depth})` 传递

### 2.4 精确恢复

`recover_turn_if_idle(request)`（`codex_thread.rs:302-331`）**保留被中断 turn 的 `turn_id`**，空闲时按同一 id 续跑，不丢上下文、不重复计费。

---

## 3. wf-agent (rs) 的执行实例设计（实际代码）

wf-agent 采用 **「重实体 + 多层 Coordinator + 注册表」** 模型，实例本身（`AgentLoopEntity`）承载了大量状态。

### 3.1 实例本体：`AgentLoopEntity`

`entity.rs:13-36` 定义的字段远多于 Codex 的句柄：

- `id` / `definition_id`（每次 run 生成新 id，`agent_id` 只是定义 id）
- `state: Arc<RwLock<AgentLoopState>>` —— 状态机
- `interruption: InterruptionState` —— 暂停/停止信号
- `conversation: Arc<RwLock<ConversationSession>>` —— 会话
- `cancellation: CancellationToken` —— 取消令牌
- `parent_execution_id` / `child_execution_ids` —— 父子层级
- `timeout_manager: AgentTimeoutManager` —— 超时（含 pause 超时）

它直接实现了 `IExecutionEntity`（`entity.rs:243-315`），`pause/resume/stop` 同时翻转 `interruption` 与 `state.status`。**注意 `get_hierarchy_depth()` 永远返回 `0`**（`entity.rs:308-310`）——层级深度追踪是桩代码。

### 3.2 三层 Coordinator 驱动

```
AgentLoopExecutor (executor.rs)
  └─ AgentLoopCoordinator (lifecycle.rs)        构建 entity + 生命周期
       └─ AgentExecutionCoordinator (execution.rs)  迭代循环 + 重试 + 超时 + 检查点
            └─ AgentIterationCoordinator (iteration.rs) 单次 LLM 往返 + 工具调用 + 流式
                 └─ ToolExecutionCoordinator (coordinator/tool.rs)
```

- **入口 `AgentLoopExecutor`**（`executor.rs:23-162`）：`execute` 同步跑（`:75-89`），`spawn_agent_loop` 后台 `tokio::spawn`（`:99-161`）。spawn 会**预注册占位 entity**（`:109-110`）使实例可被立即查询/取消，并通过父 `CancellationToken` 实现父子取消级联（`:145-154`）。
- **`AgentLoopCoordinator::execute`**（`lifecycle.rs:226-259`）：`build_entity` → 注册 → 起始持久化 → 启动会话压缩消费 → `execute_inner`。
- **`execute_inner`**（`lifecycle.rs:270-391`）：`AgentLoopStateTransitor::start_agent_loop` → 建 `AgentIterationCoordinator` + `AgentExecutionCoordinator` → `execution_coordinator.execute(entity, max_iterations, max_execution_time)`（`:327-329`）→ 终态转移（complete/fail）。
- **`AgentExecutionCoordinator::run_iterations`**（`execution.rs:103-181`）：`for iteration in 0..max_iterations` 循环，每轮先检查 `is_running()`（`:110-114`，非 running 即 break）；带 `FailurePolicyManager` 重试（`:198-244`）与 wall-clock 超时（`:75-91`）。
- **`AgentIterationCoordinator::execute_iteration`**（`iteration.rs:223-542`）：BEFORE_ITERATION hook → 构建 LLM 请求 → Blocking/Streaming 调模型 → token 追踪 → AFTER_LLM_CALL hook → 工具调用 → 中断检查 → 终态聚合。

### 3.3 状态机与快照

`AgentLoopState`（`state.rs:109-122`）显式持有 `status`、`iteration_history`、`tool_discovery`、`pending_tool_calls`、`completed_tool_results`（用于重放幂等）。实现了 `StateManager` 的 `create_snapshot` / `restore_from_snapshot`（`state.rs:377-413`）。

### 3.4 注册表：`AgentLoopRegistry`

`registry.rs:51-61` 是三个 `DashMap`：`entities` / `results` / `tasks`。提供 `register`/`get`/`query`/`store_result`/`register_task`/`abort_task`/`cleanup_terminated`——**无任何容量上限**。

### 3.5 检查点

`AgentCheckpointIntegration::create_checkpoint`（`checkpoint/coordinator.rs:49-86`）把 entity 快照写入 `StorageBackend`，但 **`restore_checkpoint`（`:88-91`）定义的 `_entity` 变量从未被使用**，且执行循环里只有 `create_checkpoint` 调用，没有从检查点重驱动 loop 的代码路径。

---

## 4. wf-agent 的主要不足（代码实证）

### 不足 1：状态机转移无源状态校验（state transitor bug 的根因）

`AgentLoopState` 的转移函数是**裸赋值**，没有任何源状态前置检查：

```rust
pub fn pause(&mut self)   { self.status = ExecutionStatus::Paused; }   // state.rs:343-345
pub fn resume(&mut self)  { self.status = ExecutionStatus::Running; }  // state.rs:347-349
pub fn complete(&mut self){ self.status = ExecutionStatus::Completed; self.end_time=... } // :351-354
pub fn fail(&mut self, e) { self.status = ExecutionStatus::Failed; ... } // :356-360
pub fn cancel(&mut self)  { self.status = ExecutionStatus::Cancelled; ... } // :362-365
```

`AgentLoopStateTransitor`（`state_transitor.rs:11-76`）每个方法只是 `entity.state.write().await.xxx()` + 发事件，纯薄包装，本身也没有守卫。

> 后果：允许非法跃迁（如 `Completed → Paused`、`Failed → Running`、`Paused → Paused` 重复触发 timeout 逻辑）。这正是 Agent Loop 测试里「state transitor」相关用例失败的根因。对比 Codex：turn 状态由 `session_loop` 单一所有者推进，且在 turn boundary 做容量/空闲校验，不存在多方随意改状态的情况。

### 不足 2：检查点「只写不恢复」

执行循环里 `AgentExecutionCoordinator` 仅调用 `create_checkpoint`（`lifecycle.rs:130, 173, 221` 及出错时 `:278`）。`restore_checkpoint` 虽然存在（`checkpoint/coordinator.rs:88-91`），但其结果赋给 `_entity` 后**既未驱动新 loop，也未被任何调用方使用**——它是死 API。

> 后果：实例**无法从检查点真正恢复续跑**。对比 Codex 的 `recover_turn_if_idle(turn_id)`（保留 turn_id 续跑），wf-agent 有快照能力却无「重驱动」闭环，检查点形同审计存档。

### 不足 3：缺执行容量闸门，子 agent 无数量/深度配额

`AgentLoopRegistry` 是无上限 `DashMap`，`AgentLoopExecutor::spawn_agent_loop` 无并发上限（`executor.rs:99-161`）。wf-core 只有 `scheduler.rs` 的 `max_concurrent`，那是 **workflow DAG 节点调度**的并发控制，**不约束 agent loop 实例**，更没有子 agent 递归深度限制。

> 对比 Codex：`reserve_spawn_slot` + `agent_execution_limiter` + `exceeds_thread_spawn_depth_limit` 三层闸门（见 §2.3）。wf-agent 在子 agent 爆炸式递归时缺乏任何背压。

### 不足 4：pause 会永久 break 出循环，无法 resume 重入

`run_iterations` 每轮开头检查 `is_running()`，非 running 即 `break`（`execution.rs:110-114`）。当外部调用 `entity.pause()`，`state.status` 变 `Paused` → `is_running()` 为 false → 循环 break 并**返回**，整个执行函数结束。

`entity.resume()`（`entity.rs:286-291`）只会把 `status` 翻回 `Running`，但**没有任何机制重新进入 `execute_inner` 的迭代循环**——`AgentLoopExecutor` 只暴露 `execute`/`spawn_agent_loop`/`execute_stream`，没有 `resume`/`continue` 入口，也没有 `execution_coordinator` 的重驱动调用。

> 后果：wf-agent 的 pause/resume 实际是「软停止」，而非「挂起-恢复」。一旦 pause，该 run 即终结，resume 只改状态位而循环已死。这正是 Pause/Resume 类测试无法真正通过的本质原因。

### 不足 5：同步/异步两套路径的实例可见性不对称

- `spawn_agent_loop`：**预注册**占位 entity（`:109-110`），所以从 `t=0` 起实例可查、可取消。
- `execute`（同步）**只 `store_result`，从不 `register` entity**（`executor.rs:75-89`）。即便注入了 shared registry，该路径也从未把 entity 写进 registry。

> 后果：同步执行的实例**完全无法被 `query_execution_status` / `cancel_execution` 寻址**（除非拿到内部结果槽）。单元测试 `test_sync_execution_registers_and_is_queryable` 期望同步实例能被查到 `completed`，但当前代码下该实例从未进入 registry——该测试正是「1/6 通过」之外的失败项之一，印证了这一不对称。

### 不足 6：流式与非流式逻辑大面积重复（DRY 违反）

`execute_stream`（`lifecycle.rs:529-746`）几乎**整段复制**了 `execute` 的 build_entity + 注册 + 起始持久化 + 检查点 + 循环驱动（`:226-259`），两套分支各自维护一份实体构建与生命周期编排。

> 后果：两路径易漂移（如流式版在 `start_agent_loop` 失败时少发了 AFTER_AGENT hook 的等价处理、缺迭代级持久化 `with_iteration_persist`），且任何状态转移 bug 都要修两处。

### 不足 7：抽象层过多、存在空壳类型

- `AgentLoopFactory`（`factory.rs:3-9`）：`create(id)` 只是 `AgentLoopEntity::new(id)`——一个 9 行的透传，价值存疑。
- `AgentStateCoordinator`（`state.rs:424-458`）：仅包装 `ConversationSession` 的 add/messages/snapshot/restore，而 entity 已直接持有 `conversation`，该类型冗余。
- `AgentLoopStateTransitor`（见不足 1）：每方法 3 行，职责已被 `entity` 自身的 `IExecutionEntity` 实现覆盖。
- `get_hierarchy_depth()` 恒返回 `0`（`entity.rs:308-310`）：层级深度追踪未实现，与 Codex 的实时 `session_depth`/`next_thread_spawn_depth` 形成反差。

> 后果：类型数量膨胀（单 agent loop 就 10+ 核心类型），认知负担大，且空壳类型让人误以为有独立职责，实际只是转发。

### 不足 8：两套暂停语义并存，易分歧

暂停通过**两条独立通道**表达：`AgentLoopState.status == Paused`（`state.rs:161`）与 `InterruptionState` 的 `Pause` 信号（`entity.rs:279-284`）。`pause()` 同时翻转两者，但「paused 超时」逻辑只走 `interruption.stop()`（`entity.rs:320-343`），而 `status` 仍停留在 `Paused` 直到 `resume()` 把它改回 `Running`。

> 后果：判断「是否暂停」有两个真相来源（state.status vs interruption.check()），不同代码路径可能读到不一致的结论（如超时 stop 后 status 仍是 Paused），加剧状态机脆弱性。

---

## 5. 改进建议（按优先级）

| 优先级 | 项 | 要点 |
|---|---|---|
| **P0** | 修复状态机守卫 | 在 `AgentLoopState` 的每个转移函数加源状态前置校验（状态矩阵），非法跃迁返回 `AgentError`；`AgentLoopStateTransitor` 承担守卫而非仅转发 |
| **P0** | 修复同步实例可见性 | `execute` 路径也应 `register` entity（与 spawn 一致），或让 `query_execution_status` 同时查 `results` 与已注册 entity |
| **P0** | 实现真正的 pause/resume 重入 | 引入 `resume_loop` 入口，使 `resume()` 后重新进入 `execute_inner` 的迭代循环；用 `InterruptionState` 而非 break 来挂起 |
| **P1** | 打通检查点恢复闭环 | 让 `restore_checkpoint` 真正重建 entity + 重驱动 loop；或明确其为只读审计并移除误导性的 public API |
| **P1** | 加执行容量闸门 | 在 `AgentLoopExecutor` 引入 `max_concurrent` + 子 agent 深度上限，对齐 Codex 的 spawn slot 模型 |
| **P1** | 收敛 DRY | 把 `execute` 与 `execute_stream` 合并为「构建 entity + 启动驱动任务」单一入口，流式仅作为迭代 coordinator 的 mode |
| **P2** | 清理空壳类型 | 合并 `AgentLoopFactory`/`AgentStateCoordinator` 进 entity 或 coordinator；实现 `get_hierarchy_depth` 或删除 |
| **P2** | 统一暂停语义 | 以 `InterruptionState` 为唯一暂停真相，移除 `status == Paused` 的平行判定，只在边界做快照 |

---

## 6. 一句话结论

**Codex 把「执行实例」做成一个轻量句柄 + 常驻 actor，并把并发/层级/恢复等硬约束内建进 `AgentControl`/`AgentRegistry`/turn 模型；wf-agent (rs) 把重状态塞进 `AgentLoopEntity` 并用多层 Coordinator 驱动，但状态机无守卫、检查点只写不恢复、缺容量闸门、pause 无法重入、同步/异步可见性不对称——这些问题大多源于「把复杂状态直接裸赋值、缺少单一权威的状态所有者与恢复闭环」，而非缺少功能点。** 修复应优先补状态机守卫与 pause/resume 重入闭环，再补容量闸门与 DRY 收敛。
