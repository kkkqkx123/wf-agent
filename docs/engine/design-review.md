# 设计审查：形态边界、共用代码与目录归属

> 范围：chat 会话、single shot、agent loop、临时对话（TemporaryMode）、对话分支树五种形态的源码与文档，重点是 `crates/engine/wf-execution-shared/`、`crates/app/wf-api/` 与 `docs/engine/`。
> 核验基准：本仓库 commit `00bd8970`。结论均附 `file:line` 证据，关键项已逐条复核。
> 性质：分析文档。回答三个问题——设计是否合理、有无问题、有无可共用代码；以及 chat 与对话分支树是否应独立成目录与 agent/workflow 平级。
>
> **当前复核状态（commit `4c21ec3`）**：本文问题清单基于 `00bd8970`，其中多数代码问题已在后续提交修复。请先看文末「六、复核更新」，再读下面的历史分析，避免据旧行号或旧结论做倒退修改。

## 0. 总判断

| 问题 | 结论 |
|------|------|
| 整体设计是否合理 | **分层方向合理，但当前实现有真实技术债**。五种形态的抽象分层（原语/执行域/运行档位）成立，不应合并；但对话分支树存在资源泄漏与竞态，且文档与代码已经漂移 |
| 是否存在问题 | **是**。1 个高危资源泄漏、2 个中高危竞态/一致性问题、3 个中等级问题，另有 3 处文档失真（详见「二」） |
| 是否存在可共用代码 | **是**。至少 4 处高价值重复，其中 `merge_usage` 逐字重复、`LlmRequest` 19 字段构造重复 8+ 处（详见「三」） |
| chat/对话树是否独立成目录 | **文档侧：支持平级（新增 `conversation/`）；源码侧：反对抽 `wf-chat` crate**，建议把对话树纯逻辑下沉到唯一调用方 `wf-api`（详见「四」） |

一句话：**保留形态分层，偿还对话树的技术债，收敛几处明显重复；不要为"目录整齐"制造新的 crate 或依赖环。**

## 一、设计合理之处（保留的理由）

1. **原语与执行域分层正确**。`ChatSession`（`wf-execution-shared/src/chat.rs:28`）零副作用、调用方持有状态；single shot（`single_shot.rs:47`、`:155`）无会话、工具结果不回喂；二者都不该与有状态的 agent loop 合并。详见 `execution-modes.md`。
2. **对话树选"C：独立文档存结构 + 消息 append-only"是对的**。消息是外部 crate 的不可变值类型（`llm-suite-types/src/message.rs`），无法加 `parent_id`；独立树文档让手术成为单文档原子写（`conversation-branch-tree-design.md` 的方案 C）。
3. **脏状态用内容指纹而非布尔传播**，改回即干净、手术往返自动复原（摘要只依赖路径内容序列，不依赖 id/ordinal/时间戳），这是该设计最漂亮的部分（设计文档第 5 节）。
4. **摘要放 `wf-execution-shared` 而非 `wf-api`** 是有意的：让 CLI 与未来桌面端复用同一判定，避免"服务端一套、TUI 一套"。

## 二、存在的问题

### 2.1 高危

**H1. 订阅计数只增不减，空闲回收永不触发（资源泄漏）。**
`conversation/stream.rs:57` 调用 `runtime.add_subscriber()`，但 `runtime.rs:224` 的 `remove_subscriber()` **全仓零调用**，也没有任何 `impl Drop` 做 RAII 回收。而空闲淘汰条件要求 `subscriber_count() == 0`（`runtime.rs:266-267`）。结果：一个会话只要被订阅过一次，订阅计数就永远 ≥1，该 runtime 再也不会被回收，长驻服务里随会话数线性泄漏。此外 `stream.rs:81` 的 `Err(_) => continue` 在广播通道关闭时会无限空转（忙等）。
建议：仿照 agent 侧 `SubscriberGuard` 的 RAII 做法，让 stream 的 guard 在 drop 时 `remove_subscriber`，并让广播 Closed 时退出循环。

### 2.2 中高危

**H2. 落 Pending 与占在飞槽之间有竞态窗口。**
`conversation/turn.rs:208` 先 `save_tree` 写入 `Pending` 节点，到 `:219` 才 `begin_turn` 占用在飞槽。在这段窗口内，若并发请求触发加载，`store.rs` 的加载逻辑会把无在飞驱动的 `Pending` 判定为 `Interrupted` 并保存，而此时生成其实即将启动——可能把正在开始的轮次错误收敛为中断。且这段路径未持会话写锁。
建议：把"落 Pending + 占在飞槽"放进同一个写锁临界区，或先占槽再落库。

**H3. 线性 chat 与对话树双轨并存，数据口径不一致。**
线性 chat（`wf-api/src/chat.rs:55`、`:77`）按时间戳平铺历史，对树结构无感知；对话树走 `conversation/` 一套。同一个 `session_id` 混用两个入口，会串入分支消息或漏掉线性轮次。此外状态口径不同：线性 chat 每轮写终态，对话树 `store.rs` 保留 `Running` 态。设计文档称"线性端点保留原语义、共用存储口径"（第 9.6 节），但现状并不完全成立。
建议：明确会话的"执行类型"标记，入口互斥；或线性入口在检测到树文档时给出明确提示/迁移路径。

### 2.3 中等等级

- **M1. 失败/中断的部分文本不可回放。** `turn.rs:568-587` 把部分文本落库，但 `projection.rs:51-53` 只投影 `Kept` 状态，`/path` 查询取不到；`InflightSnapshot.text` 只在内存。设计文档 F38 承诺"部分文本可见"，断连/重启后失效。
- **M2. 删除子树未等待在飞收敛。** `surgery.rs` 的删除只 cancel 不 await/abort 在飞任务，收敛时仍可能写入已删除的树。
- **M3. restricted 晋升缺少 id 重用守卫。** 只有 ephemeral 有 id 重用保护（`agent_execution/lifecycle.rs:314-337`）；restricted 运行中同 id 去 flag 重跑不会被拒，可能替换实体并转移 permit。
- **M4. 复杂度与场景匹配存疑。** `conversation_tree/` 8 个模块 + `wf-api/src/conversation/` 9 个模块，约数千行支撑"一问一答"；其中 tools/parameters/trace/`OpRecord` 等多为第 7 步（只读工具轮）和 undo（P2）的预留，当前恒空。属于"为未完成步骤预付的复杂度"，可考虑延后引入。

### 2.4 文档失真（应修）

- **D1. 旧设计文档 `docs/plan/conversation-branch-tree-design.md` 基于 commit `d477951`，行号普遍漂移**：如 `chat.rs:98` 的 `tools: None` 现为 `chat.rs:122`；文档写的 `conversation/{mod,...}.rs` 与 AGENTS.md 禁止 mod.rs 冲突（现状是 `conversation.rs` + 子目录）。
- **D2. 文档承诺与代码矛盾**：文档第 8.3 节称"重算同内容时下游自然收敛、forward 提前结束"，但 `conversation_tree/step.rs:16-23` 及其测试明确断言子节点仍脏；文档称 culprit 取"最深祖先"，`dirty.rs` 实际取最浅。
- **D3. 覆盖虚报**：文档第 11.0 节称端到端覆盖断连/崩溃/多观察者，`conversation/tests.rs` 现有用例并未覆盖这些场景。

## 三、可共用的代码

按"重复程度 × 抽取收益"排序。前三项证据已逐字复核。

| 优先级 | 重复项 | 证据 | 建议 |
|--------|--------|------|------|
| 1 | `merge_usage` 用量合并 | `wf-api/src/chat.rs:287-310` 与 `conversation/turn.rs:743-765` **逐字相同**（20 行） | 抽到 `wf-execution-shared`，签名不变，风险极低 |
| 2 | `LlmRequest` 19 字段构造 | `chat.rs:116-137`、`single_shot.rs:263-284`、`title.rs`、`recap.rs`、`agent_request.rs`、`wf-workflow/handler/llm.rs`、`handoff.rs` 等 8+ 处，字段序列一致 | 给 `LlmRequest` 加 `new(profile, messages)` + `with_*` builder；其余默认 None。注意 chat 强制 `tools: None`、agent 需协议对齐、workflow 需 timeout/violation_policy，builder 必须保留显式覆盖 |
| 3 | chat turn 校验三件套 | 空文本+空图 / `validate_user_images` / 空 profile：`chat.rs:73-87`、`wf-api/src/chat.rs:57-73`、`turn.rs:84-98`、`:134-143` | 抽 `validate_chat_turn(text, images, profile)`，统一错误再映射各层错误类型 |
| 4 | 流式驱动消费循环 | `chat.rs:164-282`（drive_turn）与 `turn.rs:368-479`（generate_answer）match 分支同构，约 60 行 | sink 分别是 mpsc 与 `runtime.push_text`+events，完全抽公共 sink 泛型化成本高；**只抽 usage 合并与事件分类**即可，收益/风险更划算 |
| 5 | title/recap 的旁路请求构造 | `title.rs` 与 `recap.rs` 各有一份 request builder（与第 2 项同源） | 随第 2 项一并解决 |

**边界提醒**：会话历史装配不是重复——`conversation_tree/projection.rs` 投影 `MessageRef`，`conversation_session/history.rs` 投影消息内容，`turn.rs:501-539` 已把二者组合复用，不要强行合并。agent 的 `RequestUsage::merge_non_zero` 语义相同但类型不同，属"仅概念相同"，统一收益中等。

## 四、chat 与对话分支树的目录归属

分两个层面回答，结论不同。

### 4.1 源码 crate：**不应**抽独立的 `wf-chat`

当前依赖 DAG：`wf-tools ← wf-resource ← wf-execution-shared ← wf-agent ← wf-workflow`，`wf-api` 位于顶端依赖全部五者（`wf-api/Cargo.toml`）。

- **反对理由（决定性）**：chat 执行链的本质是按会话执行器类型分派 `runChat / runAgent / runWorkflow`（`docs/engine/chat/03:13-14`），是**交互外壳**，不是执行内核。若 `wf-chat` 承载执行链，它必须反向依赖 `wf-agent`、`wf-workflow`；而共享层一旦依赖 `wf-chat`，即形成 `wf-chat → wf-workflow → wf-agent → wf-execution-shared → wf-chat` 的环，直接违反 AGENTS.md 的"严格 DAG"约束。
- **关键事实**：`wf-agent`、`wf-workflow` 源码中对 `conversation_tree` 的引用为零。对话树只服务 `wf-api`，并非 agent/workflow 的"共享执行设施"。把它留在 `wf-execution-shared` 本身就是职责错位——该 crate 已有 30 个平级模块（含 chat/conversation_tree/single_shot/recap/title/compaction…），呈杂物抽屉特征。
- **可行建议**：若确要调整，把对话树**纯逻辑**从 `wf-execution-shared` 下沉到唯一调用方 `wf-api/src/conversation/`（无环，因为 `wf-api` 是顶端）；但 `compact`/`hooks`/`token` 等真正共享的部分必须留在 shared。这是一个"按共享性切分"而非"按 chat 名义整体搬走"的调整。

### 4.2 文档目录：**支持**平级

- 现状不对称：`docs/engine/agent/`（7 篇）、`workflow/`（6 篇）、`tools/`（12 篇）各自成目录，而 `chat/` 仅 4 个文件（约 214 行，其中 `03` 还是边界说明）；对话分支树**在 `docs/engine/` 下没有任何文档**，只有 `docs/plan/conversation-branch-tree-design.md`（约 964 行）这份"设计方案"，而规划文档不等于"现状说明"。
- **建议**：在 `docs/engine/` 新增与 agent/workflow 平级的 `conversation/` 目录，把对话树的设计精华整理为现状说明文档；`chat/` 保持平级但补强内容。此调整零代码风险。
- 注意：`execution-modes.md` 作为跨形态总览继续留在 `docs/engine/` 根，承担导航与对比职责，不并入任一子目录。

## 五、行动建议（按优先级）

| 顺序 | 动作 | 类型 |
|------|------|------|
| 1 | 修复 `H1` 订阅计数泄漏（RAII guard）+ 广播 Closed 退出 | 客观 bug，必修 |
| 2 | 修复 `H2` Pending/在飞槽竞态、`M2` 删除竞态 | 客观 bug，必修 |
| 3 | 抽取 `merge_usage`、`LlmRequest` builder、turn 校验三件套 | 低风险去重 |
| 4 | 修正文档失真 D1–D3（行号、收敛承诺、测试覆盖口径） | 文档债 |
| 5 | 决策：文档新增 `conversation/` 平级目录；源码侧评估把对话树纯逻辑下沉到 `wf-api` | 架构取舍 |
| 6 | 重新评估第 7 步只读工具轮与 undo 的预留复杂度是否值得保留 | 可选，产品决策 |

**不建议**：为追求目录整齐而新增 `wf-chat` crate（制造依赖环或薄壳），或把 chat/conversation 与 agent/workflow 强行统一抽象（抹平本质的抽象层差异）。

---

## 六、复核更新（commit `4c21ec3`）

> 本节是对上文历史分析的现状复核，逐条在当前 commit 上验证。结论：上文标注的多数代码问题已经修复，重复代码已经抽取；仅文档失真（D1、D3 部分）仍然成立，架构取舍（M4 与目录归属）仍待产品决策。

### 6.1 已修复的问题（不要再按上文改动）

| 旧编号 | 上文结论 | 当前实现（`4c21ec3`） |
|--------|---------|----------------------|
| H1 订阅泄漏 | `remove_subscriber` 零调用，无 RAII，广播关闭忙等 | 已修复。`conversation/runtime.rs:65` 新增 `SubscriptionGuard`，其 `Drop`（`:76`）调用 `remove_subscriber`（`:269`）；`stream.rs:58` 用 guard 持有订阅，`stream.rs:84` 对 `ChannelClosed` 直接 `return` 退出循环 |
| H2 Pending/在飞槽竞态 | 先落库后占槽，存在误判窗口 | 已修复。`conversation/turn.rs:203-214` 改为先 `runtime.begin_turn` 占槽再 `save_tree`，且持会话写锁（`:98`），注释明确关闭误判窗口 |
| M2 删除未等在飞收敛 | 只 cancel 不 await/abort | 已修复。`conversation/surgery.rs:690-712` 的 `delete_session` 先 cancel token，释放锁后 `wait_for_task` 等待收敛，超时再 `abort` |
| M3 restricted 晋升无守卫 | 只有 ephemeral 有 id 重用保护 | 已修复。`agent_execution/lifecycle.rs:28`、`:112` 调用 `reject_live_restricted_reuse`（`:356`），运行中 restricted 用同 id 晋升会被拒绝 |
| M1 部分文本不可回放 | 部分文本仅在内存 | 已改善。`turn.rs:548-561` 将中断/失败但已有文本的消息落库为 `ReplyState::Interrupted`；`projection.rs` 仍只投影 `Kept`（属"不进模型上下文"的有意设计，UI 可经 transcript 查看） |

### 6.2 已抽取的重复代码（第「三」节建议已落地）

| 旧优先级 | 上文建议 | 当前实现 |
|---------|---------|---------|
| 1 `merge_usage` 逐字重复 | 抽到 `wf-execution-shared` | 已抽取为 `TurnAccumulator`（`wf-execution-shared/src/turn_accumulator.rs`），`chat.rs:164`、`conversation/turn.rs:403` 共用 |
| 2 `LlmRequest` 19 字段构造 | 加 builder | 已抽取 `new_llm_request`（`wf-types/src/llm/request_builder.rs:20`） |
| 3 chat turn 校验三件套 | 抽 `validate_chat_turn` | 已抽取 `validate_chat_input`（`wf-api/src/infra/validation.rs:24`），`chat.rs:66`、`turn.rs:92` 共用 |
| 4 流式驱动消费循环 | 抽 usage 合并与事件分类 | 已抽取 `TurnAccumulator::consume` 统一事件分类与用量折叠，各层只保留 sink |

### 6.3 仍然成立的项

- **D1 文档失真（仍成立）**：`docs/plan/conversation-branch-tree-design.md` 仍以 `d477951` 为基准，`chat.rs:98` 的 `tools: None` 现为 `chat.rs:122`。本文件第「三」「五」节的旧行号同样失效，阅读时以 code 为准。
- **M4 复杂度与场景匹配（待产品决策）**：对话树 `conversation_tree/` 8 模块 + `wf-api/conversation/` 9 模块的预留复杂度是否值得，属取舍而非 bug。
- **目录归属（待架构决策）**：第「四」节结论不变——不加 `wf-chat` crate；源码侧是否把对话树纯逻辑下沉到 `wf-api` 仍待决策。

### 6.4 复核结论

上文「保留形态分层、偿还技术债、收敛重复」的方向仍然有效，且**代码侧的技术债与重复已基本偿还完毕**。当前无需再按第「五」节顺序改动源码；下一步仅剩文档行号校对与两项架构/产品决策。
