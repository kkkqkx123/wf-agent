# wf-workflow 触发器子系统

> 范围：`src/trigger/`（listener、matcher、arbiter、governor、subscription、ports、internal、states）。
> 所有 `file:line` 相对 `crates/engine/wf-workflow/src/`。

引擎自持事件驱动触发器的**监听核心**：工作流与 agent 事件流经同一个 `EventBus`，但具体动作执行器位于 `wf-runtime`（`TriggerActionRouter`：子工作流 / agent / 创建 / 上下文内）。`wf-agent` 只拥有子执行目标（`TriggeredAgentExecutionManager`），不直接运行本监听器。

## 1. 事件扇入（`subscription.rs`）

- `subscribed_types`（`subscription.rs:28-50`）：计算至少有一个模板匹配的去重事件类型集合；`event_type` 不可解析时回退通用通道（空列表）——**不存在通配订阅**。
- `EventFanIn`（`subscription.rs:84-117`）：在 `new` 中同步构建（`subscription.rs:94-111`），保证广播接收器先于监听循环任务存在——spawn 窗口内发布的事件被缓冲而非丢失。
- 每个类型化订阅一个转发任务，复制进共享无界 mpsc 通道（`spawn_forwarder`，`subscription.rs:123-158`）；可观测类型上的 lag 记 debug，请求/变更类型记 warn（`subscription.rs:136-153`）。

## 2. 编排循环（`listener.rs`）

- `TriggerEventListener`（`listener.rs:59-78`）：持有 `Arc<EventBus>`、模板注册表、动作执行器、共享 `TriggerGovernor`、可选并发闸门、运行时限额、关闭令牌。
- 双循环（`listener.rs:144-203`）：主事件循环 select 关闭/扇入，**每个事件派生一个匹配任务**；命中的批次经无界通道进入后台**分发循环**（`listener.rs:152-169`），使事件循环保持响应。
- 单事件管线（`listener.rs:207-218`）：`registry.templates()` → `candidates()` → `arbitrate()` → `dispatchable()`。
- `execute_action`（`listener.rs:285-362`）：对无执行上下文的事件做冷启动防御检查（`listener.rs:295-302`）；按分发键向门控申请许可（`listener.rs:308-326`）；在可选并发闸门下派生执行器（`listener.rs:331-361`），结束后必释放门控键。尽力而为：失败记日志、不传播（`listener.rs:351-354`）。
- 内建压缩动作的分发键按 `(目标数组, 发射版本)` 细化（`listener.rs:230-253`、`369-381`）。
- 匹配与分发分离；(执行, 模板) 的每个命中对独立成任务，使在飞守卫与预算有意义（`listener.rs:43-57` 模块文档）。

## 3. 候选匹配（`matcher.rs`）

- `candidates`（`matcher.rs:23-53`）：启用 + 条件 + 守卫 + 事件匹配 + 动作；无执行上下文的事件只匹配"冷启动"类动作（`matcher.rs:42-49`）。
- 运行期守卫 `guards_allow`（`matcher.rs:77-101`）：压缩信号保留给内建 `ExecuteContextCompression` 动作；`BEFORE_*` hook 点对触发模板关闭。
- `event_matches`（`matcher.rs:106-164`）：事件类型、可选 `event_name`、`execution_prefix`、元数据映射/存在性，条件经 `wf_core::condition::ConditionEvaluator` 求值（`matcher.rs:171-214`）。
- `match_key`（`matcher.rs:222-234`）：作用域事件为 `execution_id:name`；创建事件为 `fire_id:name`（回退 `name:timestamp`）——同一键同时驱动在飞守卫与 `max_triggers` 预算。
- `value_matches`（`matcher.rs:248-271`）：数值 `>=/<=/>/<`、`^prefix` 前缀、精确相等；数组逐元素匹配。

## 4. 竞争裁决（`arbiter.rs`）

`arbitrate`（`arbiter.rs:28-110`）把候选按竞争范围分组：

- `Unique` 范围：贡献唯一订阅者。
- `BestWin` 范围：贡献显式优先级最高者（`best_win_winner`，`arbiter.rs:116-151`；优先级缺失/重复则响亮报错）。
- 同时候中多个范围：经 `dispatch_burst_limit` 限流（`arbiter.rs:76-87`）或多范围策略 `KeepFirst`/`DropAll`（`arbiter.rs:88-103`）裁决。

## 5. 开火门控（`governor.rs`）

- `TriggerGovernor`（`governor.rs:29-34`）：`in_flight: DashMap` 重入守卫 + `Mutex<HashMap>` 开火计数，均按分发键。
- `request`（`governor.rs:49-70`）：先原子认领（已存在 → `AlreadyRunning`），再查预算（`Some(0)` 永不授予；预算拒绝时归还认领）。`FirePermit { Granted, AlreadyRunning, BudgetExhausted }`（`governor.rs:16-24`）。
- `release`（`governor.rs:73-75`）清认领；预算计数在释放后**继续存在**（跨事件累计）。监听器测试覆盖每模板与每执行的预算行为（`listener.rs:657-765`）。

## 6. 端口：运行时服务注入（`ports.rs`）

`TriggerTemplateRegistry`（`ports.rs:18-21`）、`SubworkflowRunner`（`ports.rs:27-34`）、`TriggerActionRunner`（`ports.rs:44-48`）均由 `wf-runtime` 在装配时实现；压缩链本身不再经过监听器（`ports.rs:36-43`）。

## 7. 保留协议与开火记录

### `internal.rs`

- `__` 前缀保留变量：`__completed_{node_id}`（`internal.rs:25-27`）、结果槽 `__trigger_subworkflow_result` / `__trigger_script_result` / `__trigger_agent_result`（`internal.rs:30-35`）。
- 类型化 `InternalSignalBus` 发布器：stop/pause/resume/skip 信号与子工作流/脚本/agent 结果（`internal.rs:43-132`）——触发动作由此影响运行中执行的协调器循环。

### `states.rs`

- `TriggerStateRecord`（`states.rs:15-51`，status：`running|completed|failed|aborted`）与 `TriggerStateRegistry`（`states.rs:55-104`），按父执行 id 键控；重复 `event_id` 替换而非追加（`states.rs:72-82`）。
- `snapshot_for`（`states.rs:97-103`）：喂给 checkpoint 的 `trigger_states` 审计字段。

## 8. 节点内触发上下文

图节点（尤其是消息节点）内的触发动作执行见 [02-node-handlers.md](02-node-handlers.md) 第 9 节（`handler/trigger/` 的 `TriggerContext` 与 `TriggerCoordinator`）。两级触发互为补充：监听器面向事件总线驱动的跨执行触发，节点内协调器面向工作流图内的同步动作执行。
