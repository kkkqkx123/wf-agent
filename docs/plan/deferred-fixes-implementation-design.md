# 三项暂缓项的实现设计

## 背景

`engine-design-fix-and-code-sharing.md` 把三项问题列为"暂缓/不做"：M3 restricted 晋升守卫、删除子树竞态、流式 sink 抽象。本文逐一给出**若要实现该怎么做**的落地设计，基于当前 HEAD `00bd8970` 的真实源码，所有结论附 `file:line` 证据。

三项的处置优先级并不相同：M3 与删除竞态是**正确性风险**，只是此前判断触发条件窄或修复有死锁陷阱；流式 sink 抽象是**可维护性优化**，本身不影响正确性。因此本文对前两项给"必须做对"的方案，对第三项给"可做但需克制"的边界。

---

## 一、M3：restricted 晋升守卫

### 1.1 先把语义钉死

`TemporaryMode` 的两档在 **`wf-types/src/agent_execution/temporary.rs:17-34`** 写明，晋升语义是**刻意不对等**的：

| 档位 | 晋升方式 | 源码依据 |
|------|---------|---------|
| `Restricted` | 持久化整个 run（含 checkpoint 链），**同 execution id 去掉 flag 重跑**完成晋升 | `temporary.rs:21-22` |
| `Ephemeral` | checkpoint 链恒空，**禁止重用同 id**，晋升必须新开 execution 并显式传历史 | `temporary.rs:31-32` |

所以"给 restricted 加 id 重用守卫"这个提法本身就是错的——restricted 的同 id 重跑**就是晋升机制**。先前审查报告 M3 的措辞（"缺少 id 重用守卫"）容易误导，真正的风险不是"同 id 重跑"，而是**"在 restricted 仍在运行时用同 id 重跑"**。

### 1.2 真正的风险点

唯一的 id 守卫只覆盖 ephemeral：`reject_ephemeral_id_reuse`（`agent_execution/lifecycle.rs:314-340`）在 `run()`（`:27`）和 `stream()`（`:98`）入口检查：若目标 id 的**持久化记录**的 `loop_config.temporary_mode == Ephemeral`，则拒绝非 ephemeral 的新 run。它对 restricted 完全放行（`:322` 仅挡 Ephemeral）。

危险路径由此打开：

1. 一个 `Restricted` run 处于 `Running`，实体注册在 registry（`wf-agent/src/registry.rs:180`）。
2. 调用方用**同一个 id**、去掉 flag 发起 full run。
3. `reject_ephemeral_id_reuse` 放行（历史记录是 Restricted，不是 Ephemeral）。
4. 新 run 走到 `registry.register()`（`registry.rs:152-165`）：发现同 id 已有实体，直接 `take_gate_permit()` 把旧实体的并发许可**转移**给新实体（`:155-163`），然后覆盖 map 条目（`:164`）。
5. 关键缺陷：`register()` **不 abort 旧实体**。旧 run 的 driver 仍在跑，两个 driver 现在共享一个 id、一个 checkpoint 链、一个 permit 槽。写操作交错，checkpoint 链从两个源头分叉。

这与 `temporary.rs:8-9` 的承诺（"temporary mode never changes the executor"）直接冲突——运行中被替换，执行器语义已经改变。

### 1.3 实现方案：分两层

**第一层（应用层，wf-api）——按状态而非按历史模式拦截**

在 `run()` 和 `stream_with_policy()` 拿到 `agent_loop_id` 之后、构造 coordinator 之前，新增守卫 `reject_running_temporary_reuse`：

- 查 `ctx.agent_loop(id)`（live registry，`agent_execution.rs:176-182` 的 `live_entity` 走的就是它）。
- 若实体存在且状态**非终态**（`Running`/`Paused`，判据 `entity.rs:675-703` 的 `is_running`/`is_paused`），且其 `loop_config.temporary_mode == Some(Restricted)`：
  - 拒绝，返回 `ApiError::Validation`，提示"restricted run still in flight; wait for it to settle, or use a new execution id"。
- 若实体已终态（`Completed`/`Failed`/`Cancelled`），放行——这正是**合法的 restricted 晋升**，checkpoint 链得以延续。

与 ephemeral 守卫互补：ephemeral 查**持久化历史**（防"读空链后接龙"），restricted 查**运行态**（防"并发双 driver"）。两者一个 fail-open（ephemeral，`lifecycle.rs:312-313` 明确）、一个按 live 状态，语义正交。

需要给 `AgentLoopEntity` 暴露只读的 `temporary_mode()`（当前只在创建时用于 `forced_temporary_config`，`entity.rs:23-54`），守卫通过它取值，避免把配置结构整个泄漏出来。

**第二层（引擎层，wf-agent）——堵住 register 的替换语义**

应用守卫只能挡住走 wf-api 的调用；任何直接调 `registry.register()` 的路径仍可并发替换。根治点在 `registry.rs:152-165`：

- 若 existing 实体**非终态**且不是 spawn placeholder，直接返回冲突错误，而不是转移 permit。
- 需要区分正常替换（spawn placeholder → real entity，`registry.rs:149-150` 注释所述）和危险替换。当前代码没有 placeholder 的显式标记，建议引入一个轻量状态位或专门的 `replace_placeholder()` 方法，让普通 `register()` 对"非终态 existing"一律拒绝。
- 这样即便绕过应用层，也无法在旧 run 存活时注册同 id 实体。

**边界与取舍**

- 不要"禁止 restricted 同 id 重用"——那会摧毁 `temporary.rs:21-22` 的晋升语义。
- 守卫必须查 **live 状态**而非持久化记录：命中运行中替换时，持久化记录可能还没落盘（fail-open 的 ephemeral 守卫在这里会漏）。
- 应用层守卫对 `run()`/`stream()` 两个入口都要加，与 ephemeral 守卫并列调用。

---

## 二、删除子树竞态

### 2.1 现状：两条删除路径，一条安全一条有洞

- **`delete_node`**（`surgery.rs:659-681`）走 `mutate()`，而 `mutate()` 在写锁内检查 `runtime.inflight().is_some()`，在飞则直接 `Conflict` 拒绝（`:144-149`）。**这条路径安全**，不参与下面的讨论。
- **`delete_session`**（`surgery.rs:685-710`）：
  1. 拿写锁（`:689`）；
  2. 取 cancel token 并 `cancel()`（`:690-692`）——**仅协作式信号**；
  3. 删所有 tree 文档（`:693`）、删全部 message（`:694-706`）、删 execution record（`:707`）；
  4. `runtime::forget(session_id)`（`:708`）把 runtime 从全局表移除。

### 2.2 根因：cancel ≠ 停止，且没有等待收敛

在飞 driver 的结构是 `Driver::run()`（`turn.rs:342-365`）：`generate_answer()` → `converge_answer()` → `finish_turn()` → `unregister()`。cancel token 只让 `generate_answer` 在事件循环里感知中断（`turn.rs:448-452`），返回 `GeneratedAnswer::interrupted()`，但**后续收敛照常执行**：

- `converge_answer()`（`turn.rs:547-567`）拿写锁、`load_existing()`；
- 若部分文本非空，会 **新建 message 并持久化**（`turn.rs:572-598` 的 `persist_message`）；
- 再 `save_tree()`、`finish_reply`（`:614-663`）。

由此产生三个具体缺陷：

| 缺陷 | 机制 | 证据 |
|------|------|------|
| **删后回写** | delete_session 删完 message 后，在飞 driver 收敛时把部分文本作为新 message 写回，成为孤儿消息 | `surgery.rs:700-706` vs `turn.rs:578-598` |
| **锁内等待死锁** | 若在 delete_session 持写锁期间"等待收敛"，而收敛也要拿同一把写锁（`:556`），必然死锁 | `surgery.rs:689` vs `turn.rs:556` |
| **forget 后新 runtime 并存** | `forget()`（`runtime.rs:304-306`）只删全局表项；旧 driver 仍持有旧 `Arc<SessionRuntime>`（`:244`），同一 id 的新请求 `runtime_for()` 会造全新 runtime，二者各持一把锁、各自写树 | `runtime.rs:295-299` vs `turn.rs:244` |

注意：`converge_answer` 有 `load_existing` 兜底（`:557` 拿不到就早退），所以"树文档删后回写"被挡住了；漏的是**消息持久化**（在 load 之前，`:598`）和**新旧 runtime 并存**。

### 2.3 实现方案：先收敛、后删除，全程不持锁等待

核心原则：**cancel 之后必须等在飞 driver 真正跑完，再拿锁删库**。等待不能在持锁状态下进行。

**步骤 1：给 `SessionRuntime` 增加收敛通知**

`SessionRuntime`（`runtime.rs` 当前字段：写锁 / slot / 订阅计数 / last_used）增加一个 `tokio::sync::Notify`（或 `watch::Sender<bool>`）。`finish_turn()`（`:203`）在清空 slot 后调用 `notify_waiters()`。新增 `async fn settle(&self)`：若 `inflight().is_none()` 立即返回，否则 `notified().await`。

**步骤 2：重写 `delete_session` 的时序**

1. 拿写锁，只做两件事：取 cancel token、`abort_handle`（若能从 `execution_tasks` 按 turn_id 取到）→ **释放锁**。
2. `token.cancel()`——让生成阶段尽快停。
3. `runtime.settle().await`——**不持锁**等待 driver 走完 `finish_turn`；给一个超时上限（如 agent 超时 `agent_timeout_ms` 同级），超时则硬 abort `execution_tasks.abort(turn_id)`（`infra/tasks.rs:40-48` 已有现成 API）。
4. 重新拿写锁，再删 documents / messages / record，`forget()`。

这样：收敛写入（步骤 3 之前完成）不会被删后回写覆盖；删库发生在无在飞任务之后；`forget()` 时机后移，杜绝新旧 runtime 并存窗口。

**步骤 3（可选，加固）：在 driver 侧加删除代次校验**

若担心 `forget()` 后仍有漏网 task，给 runtime 加 `generation: AtomicU64`，`forget`/删除时递增；`converge_answer` 在持久化前对比入口捕获的 generation，不一致则放弃写回。这是防御性双保险，步骤 1-2 已能解决主路径，可按需加入。

**为什么此前判"风险高"**：因为朴素地"持锁 await 收敛"会死锁。本方案用 `Notify` + "锁外等待"绕开，复杂度可控。若追求最小改动，也可退而求其次——在 delete_session 里先 `token.cancel()`、再 `execution_tasks.abort(turn_id)`（按 `runtime.inflight()` 拿 turn_id，`runtime.rs:224`），接受 slot 残留（反正随后 `forget`），代价是部分文本不落库（与 M1 的既有取舍一致）。

---

## 三、流式 sink 抽象

### 3.1 三处驱动的真实差异

| 维度 | chat `drive_turn` | conversation `generate_answer` | agent forwarder |
|------|------------------|-------------------------------|-----------------|
| 位置 | `chat.rs:157-264` | `conversation/turn.rs:372-483` | `infra/stream.rs:70-139+` |
| 消费的事件 | `MessageStreamEvent` | `MessageStreamEvent` | `AgentStreamEvent` |
| sink 目标 | `mpsc::Sender<ExecutionStreamEvent>`，经 `send_frame`（`chat.rs:386`） | `runtime.push_text/push_reasoning` + `events::turn_*`（`:401-424`） | `mpsc::Sender<ExecutionStreamEvent>`，1:1 映射字段 |
| 文本/usage 累积 | 有（`answer` + `merge_usage`） | 有（`answer` + `merge_usage`） | **无**（纯转发） |
| consumer 断开 | 检测，`send_frame` 失败即返回 Failed（`:170-189`） | 无 consumer mpsc | detach 策略（keep/cancel） |
| 取消检查 | 无 | 每事件后 `cancel.is_cancelled()`（`:448-452`） | 无 |
| 产出 | `TurnOutcome`（`:149-153`） | `GeneratedAnswer`（含 `base`/stale 检测 `:620-628`） | 无（转发即结束） |

结论：**chat 与 conversation 的"`MessageStreamEvent` 分类 + 文本/usage 累积"高度同构**（约 60 行），但副作用、断开、取消、base 四处不同；**agent forwarder 是不同事件类型的纯转发，不该纳入同一抽象**。

### 3.2 实现方案：只抽"纯累加"，不抽"整个循环"

在 `wf-execution-shared` 新增 `TurnAccumulator`（chat 与 conversation 都已依赖该 crate）：

- 持有 `answer: String`、`usage: Option<TokenUsageStats>`、`assistant`、`refusal`、`base`。
- 提供 `consume(&mut self, event: MessageStreamEvent) -> AccAction`，其中 `AccAction` 是描述"这一事件要求 sink 做什么"的小枚举：`Text(String)` / `Reasoning(String)` / `Usage(TokenUsageStats)` / `Keep` / `Stop(StopReason)`，把 usage 合并统一收进累加器（复用已抽取的 `merge_usage_non_zero`）。
- 两个调用方各自实现"执行 `AccAction`"：

| AccAction | chat 的执行 | conversation 的执行 |
|-----------|------------|-------------------|
| `Text(s)` | `send_frame(LlmDelta)`，失败→Failed | `push_text` + `events::turn_delta` |
| `Reasoning(s)` | `send_frame(ReasoningDelta)` | `push_reasoning` + `events::turn_reasoning` |
| `Usage(u)` | `send_frame(Usage{...})` | `events::turn_usage` |
| `Stop` | 映射为 `TurnOutcome` | 映射为 `GeneratedAnswer` |

**刻意不抽的部分**：sink 副作用、断开检测、取消检查、base/stale 检测全部留在调用方。这些是两处行为分叉的根源，硬塞进抽象只会得到一堆 `if is_chat`。

### 3.3 边界与收益判断

- **收益**：消除约 60 行逐字重复的事件分类；新增 LLM 事件类型时只改累加器一处（与 `wf-types` 的 `MessageStreamEvent` 枚举保持同步更容易）。
- **成本**：引入一个 `AccAction` 中间层和一次匹配分发；两处仍需各自写 sink 执行分支。
- **风险**：若把断开/取消/base 也塞进抽象，会因语义不一致而劣化。**必须克制在"纯累加"边界内**。
- **net**：中等偏低。建议在**下一次因新增事件类型而改动这两处时顺手做**，不单独排期；优先级低于前两项。

---

## 四、优先级与落地建议

| 项 | 性质 | 建议 | 触发条件 |
|----|------|------|---------|
| M3 restricted 守卫 | 正确性（并发写分叉） | 做，且**两层都做**（应用层按状态拦截 + register 拒绝非终态替换） | 一旦允许 API 直接传 `agent_loop_id` 发起 full run |
| 删除竞态 | 正确性（删后回写/双 runtime） | 做，用 `Notify` 锁外等待方案 | 一旦 `delete_session` 暴露给前端在高并发场景调用 |
| 流式 sink 抽象 | 可维护性 | 顺手做，克制在纯累加边界 | 下次新增 `MessageStreamEvent` 变体时 |

三项共同的经验：**先分清"设计取舍"和"实现漏洞"**。M3 的矛盾不是"要不要守卫"，而是"守卫该按历史模式还是按运行态"；删除竞态的矛盾不是"要不要等收敛"，而是"在锁内还是锁外等"；sink 抽象的矛盾不是"抽不抽"，而是"抽到事件分类还是抽到副作用"。定位准了，方案就收敛。

---

## 附：本文引用的关键源码索引

- `crates/foundation/wf-types/src/agent_execution/temporary.rs:17-34`：两档晋升语义
- `crates/app/wf-api/src/agent/agent_execution/lifecycle.rs:27,98,314-340`：ephemeral 守卫及两个调用入口
- `crates/engine/wf-agent/src/registry.rs:149-165`：实体替换与 permit 转移
- `crates/engine/wf-agent/src/coordinator/lifecycle/entity.rs:23-54`：`forced_temporary_config`
- `crates/app/wf-api/src/conversation/surgery.rs:144-149,659-681,685-710`：两条删除路径
- `crates/app/wf-api/src/conversation/turn.rs:342-365,547-567,572-598,620-628`：driver 收敛与写回
- `crates/app/wf-api/src/conversation/runtime.rs:203,224,295-306`：finish_turn / inflight / forget
- `crates/app/wf-api/src/infra/tasks.rs:40-48`：`ExecutionTaskRegistry::abort`
- `crates/app/wf-api/src/chat.rs:157-264,386`：`drive_turn` 与 `send_frame`
- `crates/app/wf-api/src/infra/stream.rs:70-139`：agent 事件转发
