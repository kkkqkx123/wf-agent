# wf-agent 架构与主循环

> 范围：`crates/engine/wf-agent/src/coordinator.rs` 及 `coordinator/` 子树。

## 1. 三层协调器

协调器由三层组成，外层驱动内层、逐层收窄职责：

| 层 | 类型 | 文件 | 职责 |
|---|---|---|---|
| 生命周期层 | `AgentLoopCoordinator` | `src/coordinator/lifecycle.rs:40` | 一次 agent run 的完整生命周期：建实体、注册、hook、settle 终态、checkpoint |
| 执行层 | `AgentExecutionCoordinator` | `src/coordinator/execution.rs:28` | 迭代外循环：信号排水、暂停/恢复、跨迭代重试、迭代间 checkpoint |
| 迭代层 | `AgentIterationCoordinator` | `src/coordinator/iteration.rs:81` | 单次迭代骨架：请求组装 → LLM 调用 → 工具执行 → 结果落地 |

另有状态机辅助 `AgentLoopStateTransitor`（`src/coordinator/state_transitor.rs:8`），封装全部合法状态迁移与终态事件发布。

## 2. 生命周期层

### 2.1 入口与恢复

- `execute(config, input)`（`lifecycle.rs:292`）：构建实体后以阻塞模式驱动 `run_loop`。
- `execute_stream(config, input)`（`lifecycle.rs:562`）：在 spawned task 中以流式模式驱动 `run_loop`，通过 mpsc channel 返回 `AgentEventStream`。
- 三种恢复入口（`lifecycle.rs:311` / `:394` / `:463`）：
  - `resume_from_checkpoint`：分支恢复——分配全新 execution id，与源执行建立 parent 链接（`lifecycle.rs:349-362`）；
  - `resume_from_checkpoint_in_place`：沿用源 execution id 原地续跑，要求源执行已不在注册表中存活（`lifecycle.rs:406-417`）；
  - `auto_resume_from_checkpoint`：崩溃后自动恢复，使用 checkpoint 中声明的 loop_config，不追加用户消息（`lifecycle.rs:478-500`）。
- 所有恢复都还原 state 快照 + 完整会话（`lifecycle.rs:376-380`）。

### 2.2 统一模板 `run_loop`（`lifecycle/run.rs:83`）

同步、流式、恢复三种路径共用一个模板：

1. 校验 checkpoint 配置、注册实体到 entity registry，失败则回滚父链接（`run.rs:91-106`）；建立父子层级关联（`run.rs:110-121`）。
2. 持久化起始记录 `persist_agent`（`run.rs:125`）。
3. 触发 `BEFORE_USER_PROMPT` hook（`run.rs:136-144`）。
4. 启动压缩消费者 `spawn_compression_consumer`（`run.rs:46-76`），循环结束后 abort（`run.rs:149-157`）。
5. 执行体 `execute_inner`（`run.rs:174`）。
6. 结束后再次 `persist_agent` 落库（`run.rs:161-162`）。

### 2.3 执行体 `execute_inner`（`run.rs:174-485`）

- `AgentLoopStateTransitor::start_agent_loop` 置 Running（`run.rs:181`）。
- `BEFORE_AGENT` hook + Manual 生命周期 checkpoint（`run.rs:202-215`）。
- 组装 `AgentIterationCoordinator`（`run.rs:217-302`）：注入审批、token 跟踪、hook、迭代内 checkpoint、消息间隔 checkpoint、压缩限额、文件作用域等；流式时挂 event sink 并向实体发布 subagent 观察者（`run.rs:291-301`）。
- 组装 `AgentExecutionCoordinator` 并执行迭代循环（`run.rs:307-343`）。
- 成功路径：settle 为 Completed，再打 OnComplete 生命周期 checkpoint，触发 `AFTER_AGENT` hook（`run.rs:346-389`）。
- 失败路径：先按 `settle_kind` settle 终态，再按 settle 后状态选择 checkpoint trigger（OnTimeout/OnCancel/OnStopped/OnFailure/OnError，`run.rs:449-458`），同样触发 `AFTER_AGENT`（`run.rs:476-484`）。settle 失败只记日志，不覆盖原始错误（`run.rs:439-443`）。

### 2.4 settle 分支（`lifecycle/settle.rs`）

`settle_kind(err, active_shutdown)`（`settle.rs:15`）：

- 宿主主动关机 → Cancel（`settle.rs:16-18`）；
- `ExecutionTimeout` → Timeout（`settle.rs:18-19`）；
- `Cancelled` / `LlmError::Cancelled` → Cancel（`settle.rs:20-24`）；
- 其余 → Fail。

settle 通过 `AgentLoopStateTransitor` 落到状态机并发布 `AgentFailed`/`AgentCancelled`/`ExecutionTimeoutExpired` 事件（`state_transitor.rs:56-104`）。

## 3. 执行层：迭代外循环（`execution.rs`）

- `execute`（`execution.rs:80`）：构建 `FailurePolicyManager`，注册 wall-clock 超时（到点置 timeout_flag 并触发 interruption.stop()，`execution.rs:97-128`），然后进入 `run_iterations`。
- `run_iterations` 主循环（`execution.rs:149-403`），每次迭代边界处理：
  1. 排水内部信号（`execution.rs:409-445`）：Stop/Pause/Resume 信号翻译成 interruption 状态，保证停在当前迭代边界。
  2. 暂停时持久化 `cp.on_pause`（`execution.rs:166-168`）；悬停门 `wait_for_resume`（`execution.rs:171`）。
  3. 注入本轮缓存的 steering 输入为 user message 并发 `AgentSteeringInjected` 事件（`execution.rs:179-237`）——不打断当前响应，只为下一轮提供上下文。
  4. 调用 `execute_iteration_with_retry`，记录迭代耗时指标（`execution.rs:240-249`）。
- 迭代间收尾：`AfterExecute` checkpoint（`execution.rs:253-268`）；`persist_iteration` 持久化执行记录（`execution.rs:272-274`）。若本轮不再继续：Pause → continue（可恢复）；Stop → 退出；否则注入 followup 输入（`AgentFollowupQueued` 事件）后 continue；没有 followup 才算真正完成 → `cp.on_agent_complete` 后返回（`execution.rs:290-366`）。
- 达到 `max_iterations`：返回 `MaxIterationsReached` 并触发 `on_agent_complete`（`execution.rs:382-403`）。
- 跨迭代重试 `execute_iteration_with_retry`（`execution.rs:486-562`）：失败时先做结构化错误分析并写入 state，再在同一把 state 写锁内做重试决策。`Grant`：记账防重复退还、错误 checkpoint、按退避延迟重试；`Stop`：原样返回错误；`Trip`（断路器）：同类错误重复超限——压缩后仍复发的 context-length 拒绝归类为 `ContextBudgetExhausted` 而非断路器模式。

## 4. 迭代层：单次迭代（`iteration.rs` + `iteration/`）

`AgentIterationCoordinator::execute_iteration`（`iteration.rs:422-718`）分五个阶段：

**① 前置 hook 边界**：`BEFORE_ITERATION` → `start_iteration` → 流式 `IterationStart` → 中断检查 → `BEFORE_LLM_CALL`（`iteration.rs:428-468`）。

**② 压缩 settle 与请求组装**：`settle_compression_flight` 等待在飞的压缩写回（`iteration.rs:476-479`）；连续超预算 streak 达阈值 → park 而非无限压缩（`iteration.rs:486-520`）；`build_agent_request` 组装请求（`iteration.rs:523-530`）；上下文预算估算仅作警告，每会话最多一次（`iteration.rs:541-567`）。

**③ LLM 调用**：阻塞路径 `blocking_llm_call`（`iteration/llm_call.rs:65`）或流式路径 `stream_llm_call`（`iteration/streaming.rs:52`）；调后 `record_usage_and_compression` 做双轨 token 计账与压缩触发（`iteration/compression.rs:250`）。

**④ 工具执行**：assistant 消息入会话；有工具调用时打 `ToolBefore` 边界 checkpoint（`iteration.rs:611-627`）；无工具调用 → 直接 `finish_iteration`（`iteration.rs:629-632`）；有则执行工具并检查 `attempt_completion` 完成数据（`iteration.rs:658-663`）。ingest 预处理（`iteration/ingest.rs:18-90`）：超大工具输出蒸馏、按内容哈希去重文件读取，原件归档到 checkpoint session 并留指针。连续全为文件范围违规的轮次达 3 次直接报 Validation 错误（`iteration.rs:78`、`iteration.rs:686-699`）。

**⑤ 收尾**：`finish_iteration`（`iteration/outcome.rs:67`）：`end_iteration_with_content`、流式 `IterationEnd`、`AFTER_ITERATION` hook，返回 `IterationResult`。

### 迭代内支撑

- 迭代内 checkpoint（`iteration/checkpoint.rs`）：`boundary_checkpoint` 走策略门控，失败只 warn 不破坏迭代（`checkpoint.rs:9`）；`maybe_message_checkpoint` 水位线机制，新增消息达到 `message_interval` 时补打 Interval checkpoint（`checkpoint.rs:43`）。
- 压缩协作（`iteration/compression.rs`）：压缩写回落到当前会话版本才放行，失败统一 park；`publish_forced_compression` 在 provider 以 context-length-exceeded 拒绝时强制发出压缩请求作为安全网（`compression.rs:141`）。
- handoff（`iteration/handoff.rs`）：`enter_handoff_flow` 生成交接文档并用 KEEP 事实校验，缺漏带 `HANDOFF_RETRY_NOTE` 重试；随后归档 block 源与索引、裁掉热视图；文档生成失败 park 而非静默降级（`handoff.rs:95`、`:91-94`）。

## 5. 关键设计要点

1. **一条调用链**：`execute` / `execute_stream` / `resume_*` → `run_loop`（生命周期模板）→ `execute_inner`（组装两个子协调器）→ 执行层外循环 → 迭代层单次迭代。
2. **checkpoint 三个层次**：迭代间（`AfterExecute` + iteration persist）、迭代内（`ToolBefore`/`ToolAfter`/`Interval`/`BeforeCompression` 边界）、生命周期（`Manual`/`OnComplete`/终态），全部经由同一策略门控。
3. **暂停/恢复/停止统一走 interruption + 信号排水**；压缩失败与超预算升级复用 pause 语义实现"外部可感知的 park"。
4. **重试分两级**：迭代内由 retry policy 控制；跨迭代重试与断路器区分一般重复错误与 context 预算耗尽。
