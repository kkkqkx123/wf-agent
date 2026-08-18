# Hook 注册-通知机制设计思想

> 状态：已落地（Rust 侧，Stage 1-4 完成，2026-08）
> 背景：`docs/plan/hook-注册通知机制设计方案.md` 的实现回顾与设计思想整理。
> 配套分析：`docs/analysis/hooks-comparison-analysis.md`（Codex / dsh / wf-agent 对比，记录了重构前的「观察型副作用」形态）。

---

## 1. 核心思想（一句话）

**Hook 不是事件广播，而是「注册 + 同步通知」**：引擎在 hook 点停下来，同步 `await` 注册的接收方完成——引擎等待的是语义边界（已评估、已通知、已接管），而不是一次无回执的投递。

三个设计原则：

1. **语义对齐**：hook 点是同步评估与拦截点，引擎调用形态 `registry.dispatch(hooks, hook_type, ctx, bus).await`——通知完成前不前进；
2. **机制 = 注册 + 通知**：静态定义表达「何时、按什么条件、带什么数据」，动态接收方表达「收到后做什么」，二者解耦；
3. **防护是硬约束**：每个接收方调用包超时（默认 3s，可配置）+ 跳过 + 记错，接收方**永不阻塞或失败引擎**。

### 1.1 为什么不是事件广播 / 异步管道

| 机制 | 语义 | 问题 |
|---|---|---|
| 事件广播（旧实现） | 送达即完成，无人等待 | 延迟不定、Lagged 丢信号、无回执，与「边界拦截」语义不符 |
| mpsc + oneshot 管道 | 投递后等回执 | 仍是「投递-消费」模型，多一跳任务调度；回执只表达「收到」 |
| **注册 + 同步通知（现实现）** | 引擎在边界等待接收方完成 | 最贴合语义；实现即直接 await，无中间跳 |

- **同步是语义，异步是实现**：引擎侧直接 `await`，不引入中间调度；接收方内部可自由使用异步原语（如压缩服务接管后自行 `spawn` 摘要子 workflow），这是实现细节，不影响边界语义。

## 2. 两级结构：静态定义（评估）与动态接收方（行为）

- **静态**：`BaseHookDefinition`（来自配置）——纯数据，评估是同步纯函数（condition / enabled / weight 过滤，payload 模板解析）。
- **动态**：`HookReceiver`（运行时注册）——`Send + Sync` 的异步 trait，`on_hook(&HookContext) -> HookOutcome`，携带稳定名字（注册去重 / 注销）。
- **解耦规则**：`BaseHookDefinition` 增加可选 `receiver` 字段（名字解析到注册表）；**未配置时默认落到「审计接收方」**（发事件 + 记日志，即旧行为）。这样：
  - 用户 hook 配置保持声明式，不引入逻辑；
  - 引擎内置服务（压缩）以接收方身份注册到内置信号点；
  - 插件/上层可注册新接收方，配置经 `receiver` 字段引用复用——不依赖事件匹配。

## 3. 事件总线的角色降级

- **保留**：审计记录、持久化、外部订阅、用户 trigger 规则匹配、跨层回写（如压缩完成的版本校验广播）。
- **降级**：不再承担 hook 功能链路的送达职责。`HOOK_TRIGGERED` 事件由 dispatch 作为**审计副本**发布（metadata 含 `hook_type` / `payloads` / `receivers`（name / outcome / duration_ms / error）/ `duration_ms`）；trigger 模板体系（用户自定义「事件 → 动作」规则）整体保留不动。

## 4. 模块与核心抽象（`crates/wf-execution-shared/src/hooks/`）

| 文件 | 职责 |
|---|---|
| `types.rs` | `BaseHookDefinition`（含 `receiver` 字段）、`HookContext`（execution_id + hook_type + data）、`HookOutcome`（Continue / Intercept{reason}，Intercept 仅机制预留） |
| `receiver.rs` | `HookReceiver` trait（`name()` + `on_hook()`） |
| `registry.rs` | `HookRegistry`：`register`（按名去重）/ `unregister` / `get`（按名解析）/ `for_type`（hook_type → 按 weight 降序）/ `contains` / `with_timeout` / `notify`（超时守卫） |
| `dispatch.rs` | 统一入口：评估 → 解析 payload → 按序通知 → 汇总 → 审计发布；`DispatchSummary` / `ReceiverResult` |
| `emit.rs` | 降级为审计发布通道：`filter_and_sort_hooks` / `evaluate_hook_condition` / `publish_hook_audit_event`（旧 `emit_hook_events` 已删除） |
| `template.rs` | payload 模板解析（`{{path}}` 取值） |

### 4.1 dispatch 流程（同步屏障）

```
1. 静态评估：对目标 hook_type 的 BaseHookDefinition 集合做 condition / enabled / weight 过滤
2. 解析 payload：模板解析失败记 warn，用 null 兜底
3. 按序通知：先静态定义中显式 receiver 的（weight 序），后动态注册的（weight 降序）
4. 汇总 HookOutcome：首个 Intercept 生效（引擎不消费，仅记录）
5. 审计发布：payloads 或 receiver 结果非空时发布 HOOK_TRIGGERED（无 bus 则跳过）
```

## 5. Hook 点位全景

### 5.1 Agent 侧（`crates/wf-agent/`）

| 点位 | 时机 | 携带数据 | 位置 |
|---|---|---|---|
| `BEFORE_AGENT` | 循环开始前 | execution_id / current_iteration / status | `coordinator/lifecycle.rs` |
| `AFTER_AGENT` | 循环结束后（**成功与失败两分支均发射**） | 成功：`success=true` + `total_iterations`；失败：`success=false` + `error` 摘要 | `coordinator/lifecycle.rs`（sync + streaming 两条路径） |
| `BEFORE_ITERATION` / `AFTER_ITERATION` | 每轮迭代边界 | — | `coordinator/iteration.rs` |
| `BEFORE_LLM_CALL` / `AFTER_LLM_CALL` | LLM 调用边界 | AFTER 带 finish_reason 等 | `coordinator/iteration.rs` |
| `BEFORE_TOOL_CALL` / `AFTER_TOOL_CALL` | 工具调用边界（含**并行**路径） | — | `coordinator/tool.rs` |
| `BEFORE_USER_PROMPT` | 用户输入进入循环（输入提交后、start 事件前） | `prompt` 摘要 | `coordinator/lifecycle.rs` `execute` 入口 |
| `SUBAGENT_START` | 子实体创建注册后（同步/异步两分支统一） | `child_execution_id` / `agent_id` / `prompt` / `wait_for_completion` | `trigger.rs` `TriggeredAgentExecutionManager` |
| `SUBAGENT_STOP` | `execute_child` 返回后（成功/失败/超时）与异步 spawn 任务结束后 | `success` / `error` / `result` | 同上 |

- **SUBAGENT 挂载点 = 父实体的 hook 配置**（子实体由 trigger 创建、无独立 hook 配置，经 `parent_execution_id` 关联）；通过 `with_hook_registry` + `with_event_bus` 注入 manager。

### 5.2 Workflow 侧（`crates/wf-workflow/`）

| 点位 | 时机 | 位置 |
|---|---|---|
| `BEFORE_EXECUTE` / `AFTER_EXECUTE` / `ON_ERROR` | 每个节点执行边界 | `coordinator/node.rs` |
| `WORKFLOW_BEFORE` / `WORKFLOW_AFTER` | 整次执行前后 | `coordinator/workflow.rs` |

### 5.3 内部信号点（非用户配置词表）

- `CONTEXT_COMPRESSION_REQUESTED`（= `wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE` = `wf_types::hook::CONTEXT_COMPRESSION_SIGNAL`），在 `INTERNAL_SIGNAL_TYPES` 中、`is_known_hook_type` 认可但不在用户 hook 词表。
- 词表常量集中在 `crates/wf-types/src/hook.rs`（`AGENT_HOOK_TYPES` / `WORKFLOW_HOOK_TYPES` / `INTERNAL_SIGNAL_TYPES` / 具名常量）。

## 6. 压缩链：引擎内置接收方示例（Stage 2）

### 6.1 改前 vs 改后

- **改前**：引擎检测超限 → bus 发布 `CONTEXT_COMPRESSION_REQUESTED` → listener 匹配预置模板 → `CompressionTriggerRunner` 跑摘要子 workflow（事件广播 + 模板匹配，多一跳、语义为「投递」）。
- **改后**：`CONTEXT_COMPRESSION_REQUESTED` 成为引擎内置信号点，`CompressionService`（原 runner 逻辑迁入，实现 `HookReceiver`，名字 `context_compression`）在运行时组装时注册为接收方。引擎检测到超限/强制压缩时同步 `dispatch`。

### 6.2 接管即时（核心语义）

- 接收方立即做**版本幂等检查**（`execution_id:target_context_id` → 已处理版本号），通过后 `spawn` 摘要子 workflow **随即返回**——引擎等待的是「已接管」，一次 await，无中间调度。
- 压缩完成的回写链路**不变**：`CONTEXT_COMPRESSION_COMPLETED` 仍走事件总线 + 版本校验（agent 会话自消费、workflow 经 `ExecutionContextRegistry` 写回，广播语义正确）。
- 幂等、审计、trigger_states 记录、持久化 ledger 全部随迁至 `CompressionService`。
- 数据契约：`KEY_TARGET_CONTEXT_ID` / `KEY_TOKENS_USED` / `KEY_TOKEN_LIMIT` / `KEY_MESSAGE_COUNT` / `KEY_ARRAY_VERSION` / `KEY_FORCED` / `KEY_MESSAGES`，外加 `agent_loop_id`（仅 agent 目标携带，用于区分自消费与注册表写回）。
- 预置 `context_compression_trigger` 模板已从 trigger 体系移除（`wf-resource/predefined/triggers.rs` 删除）；摘要子 workflow 仍由预置资源提供，插件可注册替代 workflow。

### 6.3 引擎检测点

- agent：`coordinator/iteration.rs`（超限检测 + `publish_forced_compression` 强制路径）→ publish REQUESTED 审计副本 + `dispatch_compression`；
- workflow：`handler/llm.rs`（`emit_token_usage_events` + 强制路径）→ publish 审计副本 + `dispatch_compression_signal`（经 `NodeExecutionContext.hook_registry`）。

## 7. 接收方注册 API（Stage 4，`crates/wf-runtime/src/hook_receiver.rs`）

- `register_hook_receiver(registry, hook_type, receiver, weight)`：校验 hook 类型（`is_known_hook_type`）→ 注册；未知类型 / 名字重复返回 `HookReceiverError`。
- `register_plugin_hook_receivers(registry, mapping, receivers, weight)`：按插件 manifest `hooks` 映射（插件 hook 名 → 引擎 hook 类型）批量注册，逐条返回结果——部分无效不阻塞整体。
- 运行时组装：`register_compression_receiver`（`trigger_listener.rs`）以 weight 1000 注册内置压缩接收方（优先接管）。

## 8. 接线与运行时组装

- **注册表形态**：`Option<Arc<HookRegistry>>` 贯穿所有 coordinator/handler（`with_hook_registry(...)` 注入）；`ExecutorContext` / `NodeExecutionContext` 也携带，供 handler 层（如 workflow LLM 节点）dispatcher。
- **兜底语义**：`registry_or_default`（`wf-agent/src/hook.rs`、`wf-workflow/src/hook.rs`）——无注入注册表时退到进程级 `OnceLock<HookRegistry>` 默认实例，保证测试与最小嵌入仍可观察到审计事件（行为退化为旧「审计接收方」）。
- **bootstrap**（`crates/wf-runtime/src/bootstrap.rs`）创建**共享** `HookRegistry`：
  - 注入 `AgentLoopExecutor` / `WorkflowExecutionCallback` / `ApiContext`（API 执行的 agent 与 workflow 走同一信号点）；
  - `start_trigger_listener_with_parts` 共享 sub-workflow runner + shutdown token 给 `CompressionService`（运行期关闭时一并停掉在途摘要子 workflow）；
  - `AgentTriggerRunner::with_hook_context` 把注册表 + bus 传给 `TriggeredAgentExecutionManager`（SUBAGENT 点位）。

## 9. 边界与不做事项

- **控制语义不接线**：阻断工具调用、改写入参、注入上下文、权限决策、重试均不通过 hook 体系实现（approval 与 workflow 机制承担）；`HookOutcome::Intercept` 仅作机制预留。
- **不引入压缩 hook 类型**：`PRE_COMPACT` / `POST_COMPACT` 不新增；压缩完全托管于压缩子 workflow。
- **事件总线不删除**：审计/持久化/外部订阅/用户 trigger 规则通道保留；trigger 用户模板体系（`SubworkflowActionRunner`）保留不动。
- **hook 不承载逻辑**：行为始终在注册的接收方。
- **TS 层 `packages/` 不做任何改动**。

## 10. 测试覆盖（验收对照）

| 验收项 | 用例 |
|---|---|
| 注册接收方被同步通知、outcome 汇总可见 | `hooks/dispatch.rs`、`hooks/registry.rs` 单测 |
| 接收方超时不阻塞引擎 | `timeout_receiver_does_not_block_engine`、`slow_receiver_times_out_and_is_reported` |
| `HOOK_TRIGGERED` 审计事件仍发布且含结果摘要 | `audit_event_published_with_results_summary`、`publish_hook_audit_event` 相关 |
| 压缩链 e2e 全绿 + 接管即时 | `context_compression_chain_end_to_end`、`no_compression_event_when_named_array_within_limit`、`compression_dispatch_takes_over_immediately`（wf-runtime）；`over_limit_named_array_flows_through_compression_chain` 等（wf-workflow/tests/ 两个 e2e 文件，已迁移为 dispatch 模型） |
| SUBAGENT 时机与数据、失败路径 AFTER_AGENT | `trigger.rs` 三个 SUBAGENT 用例（顺序 START→child-ran→STOP / 失败 STOP / 异步 STOP）；`executor.rs` `test_after_agent_fires_on_failure_path_with_error_details`（排空重试预算后断言 `success=false` + error） |
| 注册 API 对外化 | `hook_receiver.rs` 4 个单测（未知类型 / 注册通知 / 名字去重 / 插件映射） |

## 11. 关键决策记录（回顾要点）

| 决策 | 理由 |
|---|---|
| 同步屏障而非异步管道 | 语义最贴合「边界等待」；实现即直接 await |
| 接收方超时 + 跳过而非传播 | 引擎永不因 hook 失败；超时按「跳过 + 记错」处理 |
| 审计事件由 dispatch 发布 | 总线降级为记录通道，hook 功能链路不依赖送达 |
| 压缩走内置接收方而非 trigger 模板 | 接管即时（一次 await）；模板体系留给用户规则 |
| SUBAGENT 挂父实体 hook 配置 | 子实体无独立配置，经 parent_execution_id 关联 |
| `Option<Arc<HookRegistry>>` + 默认兜底 | 最小嵌入/测试可运行；显式注入时才真正 dispatch |
| 压缩回写链路不变 | COMPLETED 广播 + 版本校验对 agent/workflow 均语义正确 |
