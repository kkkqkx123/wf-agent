# Hook 与 Trigger 调试

本文件针对用户需求中的「还需要提供 hook、trigger 的调试功能」，给出复用现有引擎函数的设计。

## 一、Hook 调试

### 1.1 现有事实

- 定义：`HookDefinition { id, hook_type, priority, condition, enabled, payload, handler, create_checkpoint, checkpoint_description }`（`crates/engine/wf-execution-shared/src/hooks/types.rs:11-34`）。
- 上下文：`HookContext { execution_id, hook_type, data: HashMap<String,Value> }`（`types.rs:75-80`）。
- 单次触发：`fire(registry, hooks, hook_type, ctx, event_bus) -> FireSummary`（`crates/engine/wf-execution-shared/src/hooks/fire.rs:85`）。
- 聚合结果：`FireSummary { hook_type, payloads: Vec<Value>, priorities: Vec<i32>, handler_results: Vec<HandlerResult>, duration_ms, outcome }`（`fire.rs:37-45`）。
- 单 handler 结果：`HandlerResult { name, outcome, duration_ms, error }`（`fire.rs:22-29`）。
- 结局：`HookOutcome::Continue | Veto { reason }`（`types.rs:46-56`）。
- 触发点集合：`WORKFLOW_HOOK_TYPES` / `AGENT_HOOK_TYPES`（`crates/engine/wf-execution-shared/src/hooks/types.rs:6-9` 再导出；工作流 `BEFORE_EXECUTE`/`AFTER_EXECUTE`/`ON_ERROR`/`WORKFLOW_BEFORE`/`WORKFLOW_AFTER`，agent `BEFORE_TOOL_CALL`/`AFTER_TOOL_CALL`/`BEFORE_ITERATION`/`AFTER_ITERATION`/`BEFORE_LLM_CALL`/`AFTER_LLM_CALL`/`BEFORE_AGENT`/`AFTER_AGENT`/`BEFORE_USER_PROMPT`/`SUBAGENT_START`/`SUBAGENT_STOP`）。
- 发射器：`WorkflowHookEmitter`（`crates/engine/wf-workflow/src/hook.rs:14`）、`AgentHookEmitter`（`crates/engine/wf-agent/src/hook.rs:33`）。

### 1.2 调试设计（复用 `fire()`）

debugger 不重写触发逻辑，而是：

1. 构造 mock `HookDefinition` 列表（来自 trace 的 `hooks_fired` 或用户手写配置）；
2. 用 mock `HookHandlerRegistry`（`HookHandlerRegistry::fallback()` 或注册录制型 handler——仅记录 `on_point` 入参与返回 `Continue`，不引入副作用）；
3. 构造 mock `HookContext`（含 `data` 中的变量/状态）；
4. 以 `None` 作为 `event_bus` 调用 `fire()`；
5. 直接消费返回的 `FireSummary` 做展示。

### 1.3 展示内容

对每个触发点输出一张表：

| 列 | 来源 |
|----|------|
| 命中的 hook（enabled 且条件通过） | `filter_and_sort_hooks` + `evaluate_hook_condition`（`crates/engine/wf-execution-shared/src/hooks/audit.rs` 再导出） |
| 各条件求值结果 | `evaluate_hook_condition` 的 true/false（失败条件跳过，非失败） |
| 解析后的 payload | `FireSummary.payloads`（由 `resolve_payload_template` 产出） |
| 各 handler 结果 | `FireSummary.handler_results`：`name` / `outcome` / `duration_ms` / `error` |
| 聚合结局 | `FireSummary.outcome`：`Continue` 或 `Veto { reason }` |
| 耗时 | `FireSummary.duration_ms` |

### 1.4 出错/否决的可解释性

- 若某 handler 返回 `Veto`，高亮 `reason` 字符串（实际值）；
- 若 `error` 非空（如 handler 未注册 / 超时），标注「该 handler 未真正生效」；
- 对 gate 点（工作流 `BEFORE_EXECUTE`、agent `BEFORE_TOOL_CALL`）显式提示「veto 会阻断该步」，与非 gate 点（仅记录）区分。

## 二、Trigger 调试

### 2.1 现有事实

触发链路（事件驱动）位于 `crates/engine/wf-workflow/src/trigger/`：

- `matcher::candidates(templates, event) -> Vec<TriggerTemplate>`（`matcher.rs:23`）：enabled + 条件匹配 + guard 允许 + 有 action + 作用域兼容；
- `guards_allow`（`matcher.rs:72-88`）：拦截内部压缩信号与 `BEFORE_*` hook 点（应改用同步 handler veto）；
- `event_matches`（`matcher.rs:93-151`）：按 `event_type` / `event_name` / `execution_prefix` / `metadata_exists` / `metadata` / 表达式逐项判别；
- `match_key`（`matcher.rs:209`）：调度键 `execution_id:name` 或 `fire_id:name`；
- `arbiter::arbitrate(candidates, event, limits) -> Vec<TriggerTemplate>`（`arbiter.rs:28`）：按竞争作用域（unique / best-win）择优胜者，受 `multi_scope_policy`（keep_first / drop_all）与 `dispatch_burst_limit` 约束；
- `governor::TriggerGovernor::request(key, max_triggers) -> FirePermit(Granted|AlreadyRunning|BudgetExhausted)`（`governor.rs:49`）：重入守卫 + `max_triggers` 预算；
- `TriggerStateRecord { trigger_name, event_id, event_type, status, started_at, completed_at }`（`states.rs:14`）与 `TriggerStateRegistry`（`states.rs:55`）；
- 端口（由 `wf-runtime` 实现）：`TriggerTemplateRegistry` / `SubworkflowRunner` / `TriggerActionRunner`（`ports.rs:18/28/45`）。

Agent 侧子执行：`TriggeredAgentExecutionManager`（`crates/engine/wf-agent/src/trigger.rs`）在 `SUBAGENT_START`/`SUBAGENT_STOP` 触发点运行子 agent。

### 2.2 调试设计（全链路 dry-run）

debugger 对一条 mock `BaseEvent` 跑完整链路，**不真正执行 action**：

1. `templates` 来自 mock `TriggerTemplateRegistry`（或用户给出的模板集）；
2. 调用 `matcher::candidates` → 得到候选模板，并对**每一个未入选**的模板说明落选原因（禁用的 / 条件不匹配 / 被 guard 拦截 / 无 action / 作用域不兼容）；
3. 调用 `arbiter::arbitrate` → 展示竞争作用域分组、best-win 优先级裁决、`multi_scope_policy` 与 `dispatch_burst_limit` 的影响；
4. 调用 `governor::request(key, max_triggers)` → 展示 `FirePermit`（Granted / AlreadyRunning / BudgetExhausted）；
5. 用 mock `TriggerActionRunner`（不执行实际子流程）记录将产生的 `TriggerStateRecord`，写入 `TriggerStateRegistry` 快照。

### 2.3 展示内容

- **候选 vs 落选**：每个模板的匹配判定树（event_type / name / prefix / metadata / 表达式逐条结果，附实际值）；
- **守卫拦截**：若命中压缩信号或 `BEFORE_*` hook 点，明确提示「应改用 hook handler veto」；
- **仲裁结果**：优胜者名单、被 `drop_all`/`keep_first`/burst cap 丢弃者及其原因；
- **许可结果**：`FirePermit` 与预算消耗；
- **状态记录**：形如 `TriggerStateRecord` 的审计条目（`running`/`completed`/`failed`/`aborted`）。

### 2.4 与 hook 的交叉

`HOOK_TRIGGERED` 审计事件本身是 trigger 的常见订阅源（`matcher.rs:369` 演示了 `hook_type` 列表匹配）。debugger 可在同一时间线上并列展示「某 hook 触发 → 其审计事件 → 哪些 trigger 据此匹配/落选」，形成端到端可解释链。

## 三、复用边界小结

| 机制 | 复用的现有函数 | 调试器注入的 mock |
|------|----------------|------------------|
| hook | `fire()`（`fire.rs:85`） | `HookHandlerRegistry`、`HookContext`、`event_bus=None` |
| trigger | `matcher/arbiter/governor`（`matcher.rs:23`/`arbiter.rs:28`/`governor.rs:49`） | `TriggerTemplateRegistry`、`TriggerActionRunner`（no-op） |
| 条件求值 | `ConditionEvaluator`（`route.rs:5`） | 变量快照作为求值上下文 |

下一份文档 `04-formatter-assert.md` 说明格式化输出与断言引擎。
