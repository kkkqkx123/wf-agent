# 四种模型调用形态说明：Chat Session、Single Shot、Agent Loop 与临时对话

> 范围：`crates/engine/wf-execution-shared/src/chat.rs`、`single_shot.rs`、`crates/app/wf-api/src/chat.rs`、`llm.rs`、`crates/engine/wf-agent/src/coordinator/lifecycle/`、`crates/foundation/wf-types/src/agent_execution/temporary.rs`、`crates/foundation/wf-types/src/execution/hierarchy.rs`、`crates/engine/wf-tools/src/tool_exposure.rs`，以及 Web 前端 `apps/web-app/src/lib/chat/chat-session.svelte.ts`。  
> 定位：说明文档。回答一个问题——系统里几种"跟模型说话"的形态究竟差在哪里，是不是重复实现，该如何选型。  
> 结论：**这些形态是刻意分层，保留现状，不改代码**。它们处在抽象层次、执行域、会话状态三个不同维度上；表层能力（如"都不调工具""都只问一次"）的重叠，掩盖不了语义、生命周期、落库归口与晋升方向的不同。原 `docs/engine/chat-and-agent-loop.md` 的完整论证已并入本文档，统一在此说明。

## 0. 文档元信息

| 项       | 值                                                                                                                       |
| ------- | ----------------------------------------------------------------------------------------------------------------------- |
| 状态      | 说明定稿（未改源码）                                                                                                              |
| 触发问题    | ChatSession 与 agent loop 职责是否重叠；chat 为何双写、为何混入 agent 执行表；single shot 与 chat / agent loop 的边界；agent loop 无工具模式是否等于"临时对话" |
| 结论      | 保留现状，四形态各司其职，仅补文档                                                                                                       |
| 核验基准    | 本仓库 commit `00bd8970`，逐条以 `file:line` 引述                                                                                |
| 代码 diff | 无源码改动                                                                                                                   |

## 1. 背景：容易混淆的四种形态

系统里有若干"跟模型说话"的形态，名字相近、外观相似，常被误判为重复实现：

1. **Chat Session（chat 会话）**：无工具、有会话状态的多轮问答。包含引擎原语 `ChatSession` 与把它包装成持久化会话的 `wf-api` chat 执行链。
2. **Single Shot（单次调用）**：一次性的模型调用原语。一次生成，可选地跟一轮有界工具执行，无会话、无第二轮、不落库。
3. **Agent Loop（agent 循环）**：完整生命周期的自主执行域——事件、审批、迭代、checkpoint、执行记录、层级一应俱全。
4. **临时对话（TemporaryMode）**：agent loop 的一种运行档位，通过强制收紧工具暴露（ephemeral 档还关闭 checkpoint）实现"围栏下的快问"，但它仍然是 agent run。

需要回答：这些是不是重复？chat 为什么要写两处存储？chat 记录为什么落在 `agent_execution` 表里？single shot 和 chat / agent loop 的分界在哪？agent loop 的"无工具模式"是不是就是"临时对话"？

## 2. 核心结论（TL;DR）

| 问题                              | 结论                                                                                                                                     | 依据                                                                                                                   |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| chat 为何双写                       | 消息写 message store 存 transcript，会话元数据写 `agent_execution` 存状态/时间/模型/目录；两者职责不同，缺一不可                                                       | `crates/app/wf-api/src/chat.rs:126-142`、`:317-343`、`:347-408`                                                        |
| chat 为何混入 agent 执行表             | chat 是 `ExecutionType::Chat`，与 Agent 共用一张表和同一套 registry/list/detail，查询侧无需特例                                                            | `crates/foundation/wf-types/src/execution/hierarchy.rs:5-12`、`crates/app/wf-api/src/execution_listing.rs:127`、`:293` |
| single shot 是不是简化版 chat / agent | 都不是。它是"一次生成 + 至多一轮工具、工具结果不回喂模型"的无状态原语，服务于评审/压缩/脚本/API 直连                                                                               | `crates/engine/wf-execution-shared/src/single_shot.rs:1-8`、`:47`、`:155`                                              |
| agent loop 无工具模式是否等于临时对话        | 部分正确但不等同。存在两条独立路径：路径 A `explicit empty pool` 纯 chat run（完整生命周期、无产品入口）；路径 B `TemporaryMode`（restricted/ephemeral，服务于草稿围栏/只读快问，可晋升或不可晋升） | `crates/engine/wf-tools/src/tool_exposure.rs:75`、`crates/foundation/wf-types/src/agent_execution/temporary.rs:15-34` |
| 四者是否重复                          | 表层重叠（单次、无工具或受限工具），本质不同：抽象层、会话状态、执行域、落库归口、晋升方向均不同。**保留**                                                                                | 见「职责边界总表」与「设计合理性判断」                                                                                                  |

## 3. 一图看清分层

四种形态并不在同一层次上，把它们画成一条线比较是混淆的根源。它们分属三个维度：

```
抽象层（原语，无执行域副作用）
  ├─ ChatSession          有会话状态 / 无工具 / 多轮生成
  └─ Single Shot          无会话状态 / 单次生成 + 至多一轮有界工具

执行域（落 execution_type、走持久化与查询）
  ├─ chat 执行链          ExecutionType::Chat / 包装 ChatSession / 双写
  ├─ agent loop           ExecutionType::Agent / 完整生命周期 / 多轮迭代
  └─ workflow             ExecutionType::Workflow / 图编排 / 按节点推进（见 §14）

运行档位（挂在 agent loop 上，不改执行域）
  └─ TemporaryMode        restricted（可晋升） / ephemeral（关 checkpoint）
```

- ChatSession 与 Single Shot 是**引擎原语**：调用方持有状态，自身不落库、不挂 checkpoint、不发事件。
- chat 执行链与 agent loop 是**执行域**：落 `agent_execution` 表，进入统一的 list/detail/审计体系。
- TemporaryMode 不是独立执行域，而是 **agent loop 的一个开关**：跑在 `execution_type = agent` 上，只强制改变工具暴露（ephemeral 另关 checkpoint）。

## 4. 形态一：Chat Session（chat 会话）

### 4.1 引擎原语 `ChatSession`

`crates/engine/wf-execution-shared/src/chat.rs` 的模块头把定位写死：

> "Tool-free multi-turn chat over a caller-owned session ... There is no tool execution, no checkpointing, no approval, no event publishing and no execution-record persistence. The request template carries no tools field, so a tool-carrying chat request cannot be constructed."

- `ChatTemplate`（`chat.rs:20`）只有 `profile_id / parameters / generation` 三个字段，**没有 tools**。
- `ChatSession`（`chat.rs:28`）只持有 `template + conversation`，注释明确 "nothing is shared with the agent loop or the checkpoint machinery"。
- `begin_turn` 构造的请求把 `tools` 等工具相关字段全部置 `None`（`chat.rs:122`）。
- 测试 `a_turn_never_carries_tools`（`chat.rs:356`）断言请求既无 tools 也无 tool_call_protocol。

**要点**：`ChatSession` 自身**不落库**——既不写 message store，也不写 `agent_execution`。持久化是上层 `wf-api` 包装链做的事（见「chat 的双写与混入执行表」）。它是介于"单次无状态生成"和"完整 agent loop"之间的有会话状态原语。

### 4.2 chat 执行链

`crates/app/wf-api/src/chat.rs` 把原语包装成可持久化的会话（`stream` 入口，`chat.rs:55`）：每轮 `ChatSession::new` + `restore(history)` 续接已存会话，`begin_turn` 生成无工具请求，流式生成结束后做双写收尾。它复用 agent 的 `ExecutionStreamEvent`，但语义收敛为 `LlmDelta` / `ReasoningDelta` / `Usage` / `Completed { iterations: 0 }`，**不发布任何 agent loop 事件**。

## 5. 形态二：Single Shot（单次调用）

### 5.1 定位

`crates/engine/wf-execution-shared/src/single_shot.rs:1-8` 的模块头：

> "The middle primitive between a pure text generation (no tools) and the full agent loop: exactly one model generation followed by at most one bounded round of tool executions. There is no second model round, no conversation persistence and no checkpoint attribution — the reviewer runs read-only."

它是**无状态的单次调用原语**，两个函数：

| 函数                                              | 语义                                  | 校验与边界                                                                                                                                                                |
| ----------------------------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `generate_text_once`（`single_shot.rs:155`）      | 恰好一次纯文本生成，就是 `gateway.generate` 的封装 | 无消息报错；请求若声明任何工具则报错（`:165-169`），文本调用不得带工具                                                                                                                             |
| `generate_with_tools_once`（`single_shot.rs:47`） | 一次生成 + 至多一轮有界工具执行                   | 请求必须声明工具且 allow-list 非空（`:61-71`）；模型必须返回 tool_call（`:81-85`）；调用数上限 `MAX_SINGLE_SHOT_TOOL_CALLS = 4`（`:20`、`:86-91`）；越界工具直接报错不执行（`:105-109`）；参数必须是合法 JSON（`:110-119`） |

### 5.2 与 agent loop 的分水岭：工具结果不回喂模型

这是 single shot 最容易被忽略、也最关键的一点。`generate_with_tools_once` 执行完那一轮工具后，把结果放进 `SingleShotOutcome { generation, executions }`（`single_shot.rs:144-147`）**直接返回给调用方，不再发起第二次模型生成**。工具执行用 `ToolExecutionContext::new(execution_id)`（`single_shot.rs:99-100`），不建 checkpoint session，因此不会把文件/shell 副作用记成 agent 编辑。

对比之下，agent loop 是 `model → tools → model → …` 的**迭代闭环**：工具结果作为新一轮消息回喂模型，驱动下一次生成，直到收敛或终止。single shot 是 `model → tools（一次）` 的**开放端点**。这一条把两者彻底分开：single shot 不是"只有一轮的 agent loop"，而是没有迭代闭环的原语。

### 5.3 与 ChatSession 的区别

| 维度   | ChatSession           | Single Shot              |
| ---- | --------------------- | ------------------------ |
| 会话状态 | 有，`restore` 续接历史、多轮累积 | 无，一次调用即结束                |
| 工具   | 类型层禁止（请求无 tools 字段）   | 可选：一轮 allow-list 内工具（≤4） |
| 工具结果 | 不适用                   | 返回调用方，不回喂模型              |
| 落库   | 上层执行链负责双写             | 原语自身不落库，调用方决定            |

一个管"多轮无工具问答"，一个管"单次可带工具的判定/生成"，互补而不重叠。

### 5.4 谁在用 single shot

| 场景           | 调用点                                                                                                                               | 用途                                                |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| 审批评审 verdict | `crates/app/wf-api/src/checkpoint/approval.rs` 的 `run_review_round`（调 `generate_with_tools_once`，`:168`）                          | allow-list 只有 `REVIEW_VERDICT_TOOL`，要求模型恰好吐一个判定调用 |
| 工具审批判定       | `crates/app/wf-runtime/src/tool_approval.rs:274`                                                                                  | `generate_text_once` 做一次只读判定                      |
| 压缩交接文档       | `crates/engine/wf-agent/src/coordinator/iteration/handoff.rs:159`、`:204`                                                          | `generate_text_once` 生成/重试 handoff 文档             |
| 交互式脚本        | `crates/engine/wf-execution-shared/src/interactive_script_session/driver.rs:132`                                                  | `generate_text_once` 单次生成                         |
| API 直连       | `crates/app/wf-api/src/llm.rs`：`generate`（`:19`）、`generate_with_tools_once`（`:36`）、`generate_text`（`:65`）、`generate_batch`（`:77`） | 对外暴露的一次性 LLM 调用                                   |

补充：`llm.rs` 的 `generate`（`:19`）是最底层的 `gateway.generate` 封装，允许带 tools 声明但只生成、不执行工具；`generate_text`（`:65`）才走 `generate_text_once` 并强制无工具。二者都属于"一次性、无状态"调用家族。single shot 自身不产生 `execution_type`、不写 `agent_execution`，传入的 `execution_id` 仅用于工具执行上下文。

## 6. 形态三：Agent Loop（agent 循环）

`crates/engine/wf-agent/src/coordinator/lifecycle/run.rs` 是 agent loop 的生命周期模板，`execute / execute_stream / resume_from_checkpoint` 共用它：

1. 配置校验、注册与父链、起始记录、压缩消费者、`execute_inner`、结束记录（`run.rs:83` 的 `run_loop`）。
2. `execute_inner`（`run.rs:154`、`:174`）：`start_agent_loop`（`:181`）→ `BEFORE_AGENT` hook 与 lifecycle checkpoint（`:204`）→ 组装 iteration/approval/file 协调器。
3. 迭代循环：模型与工具多轮往返，直到完成/超时/取消/失败。
4. settle：终态结算（`:461`、`:488`），写入执行记录。

此外 `crates/engine/wf-execution-shared/src/execution_loop.rs:1-8` 提供 agent 与 workflow 共享的循环原语（中断门控、checkpoint 接入、指标），但**不强制统一循环结构**——因为 agent 按迭代、workflow 按节点，粒度不同。

**要点**：agent loop 的职责是"带完整生命周期的一次 agent 执行"——事件、审批、迭代、checkpoint、执行记录、层级、压缩。这些是 `ChatSession` 与 single shot 都没有的。

## 7. 形态四：临时对话（TemporaryMode）

### 7.1 定义：仍是 agent run，只改工具暴露

`crates/foundation/wf-types/src/agent_execution/temporary.rs:15-34`：

> "`None` ... means a full agent run. A temporary mode never changes the executor: the row stays `execution_type = agent`, the checkpoint and history machinery stays the same shape, only the exposure is forced and, for the ephemeral tier, checkpointing is switched off."（`temporary.rs:5-9`）

配置字段 `temporary_mode: Option<TemporaryMode>`（`crates/foundation/wf-types/src/agent_execution/loop_config.rs:119`）由引擎**强制覆盖、不合并**，客户端无法扩宽自己的笼子；强制逻辑 `forced_temporary_config`（`crates/engine/wf-agent/src/coordinator/lifecycle/entity.rs:23`）把 available 换成档位白名单、清空 initial/discoverable、过滤 activated、隐藏表追加 denied、关闭 `general` 代理。

### 7.2 两档语义

| 档位           | 工具暴露                               | checkpoint                                                               | 持久化                      | 晋升                             | 行号                   |
| ------------ | ---------------------------------- | ------------------------------------------------------------------------ | ------------------------ | ------------------------------ | -------------------- |
| `Restricted` | 强制临时白名单（只读检查、交互、网络查询）              | 正常                                                                       | 与完整 agent run 相同         | 同 `execution id` 去掉 flag 重跑即晋升 | `temporary.rs:17-23` |
| `Ephemeral`  | 强制 ephemeral 白名单（只读检查+交互，无网络/无副作用） | 完全关闭（`checkpoint_integration_for_config` 直接返回 `None`，`checkpoint.rs:46`） | 消息与执行记录仍存，transcript 可重放 | 不复用同一 id，另起新执行并带显式历史           | `temporary.rs:24-33` |

ephemeral 的 checkpoint 全关：`run.rs` 的压缩消费者不建 checkpoint、不留 file history session、hook checkpoint 跳过、手动创建被拒。但消息与执行记录照常落库，只是 checkpoint 链为空——崩溃后没有快照可恢复，调用方需重新提问而非续跑（`temporary.rs:11-14`）。

### 7.3 入口

- server 层严格解析 wire 名，未知值拒绝（`crates/app/wf-server/src/api/agent/loops/lifecycle.rs:141-146`、`:180`）。
- Web 前端 `/fenced → restricted`、`/readonly → ephemeral`（`apps/web-app/src/lib/chat/chat-session.svelte.ts:746-747`）；`askFenced`（`:616`）设置一次性 scope、随该轮清除，请求带 `temporaryMode`（`:966`），非 agent 执行类型清空该 scope（`:596`）。
- CLI 侧 `temporary_mode` 恒为 `None`，无 CLI 入口（`crates/app/cli/wf-cli-shared/src/turn.rs`）。

**要点**：它之所以被叫作"临时对话/临时会话"，是因为注释用它描述草稿受限首轮（`temporary.rs:18-22`）和只读快问（`:24-33`），Web 入口也叫 fenced/readonly。但它跑在 `execution_type = agent` 上、走完整 agent 生命周期——与 chat 的 `ExecutionType::Chat` 是两条不同的执行域。

## 8. chat 的双写与混入执行表

### 8.1 双写：两处存储、两种职责

chat 流式结束后的 driver 执行两步（`crates/app/wf-api/src/chat.rs:124-142`），`finish_turn` 无条件、`persist_messages` 仅在 `Completed` 时：

| 写入              | 函数                 | 存储                  | 内容                                               | 行号                |
| --------------- | ------------------ | ------------------- | ------------------------------------------------ | ----------------- |
| ① 会话元数据         | `finish_turn`      | `agent_execution` 表 | 一条 `AgentExecution`：状态、起止时间、模型 profile、目录        | `chat.rs:347-408` |
| ② 消息 transcript | `persist_messages` | message store       | 本轮 user + assistant 消息，按 `ledger_base` 递增时间戳保证全序 | `chat.rs:317-343` |


`persist_messages` 的时间戳用 `ledger_base + 1 + index` 递增——同一轮内消息可能共享同一毫秒，`ledger_base` 取历史最大时间戳与当前时间中较大者，保证会话顺序是**全序**。

### 8.2 混入 agent 执行表

- `ExecutionType` 三值：`Workflow / Agent / Chat`，Chat 注释为 "Tool-free multi-turn question answering"（`crates/foundation/wf-types/src/execution/hierarchy.rs:5-12`）。
- 写入记录时 `execution_type: ExecutionType::Chat`、`definition_id = "@chat"`（`chat.rs:380-381`），并把 `current_iteration / tool_call_count` 置 0、`iteration_history / loop_config / hierarchy` 置 `None`。常量 `CHAT_SESSION_DEFINITION_ID = "@chat"`（`chat.rs:32`）——chat 没有 agent 模板，故以执行器命名，把"没有 agent 模板"显式编码进 id。
- 会话级字段继承：`started_at` 与 `directory` 从已有记录读取（首轮的值），保证记录横跨整个会话，中途切换工作区不把早前轮次改归新目录。
- 查询侧**无特例**：list 把 Agent 与 Chat 一起查（`execution_listing.rs:127`）；detail 注释直言 "chat sessions are `agent_execution` rows"（`execution_listing.rs:293`）。

**为什么这样设计**：让 chat 会话自动获得与 agent run 统一的列表、详情、层级、审计能力，无需另建存储与查询分支；代价只是几个字段置零/置空。

## 9. agent loop 的两条"无工具/受限"路径

所谓"agent loop 的无工具模式"实际上是两条互相独立的路径，不能混为一谈，也不能等同于 ChatSession 或 single shot。

### 9.1 路径 A：显式空池纯 chat run

判定谓词 `is_explicit_no_tools`（`crates/engine/wf-tools/src/tool_exposure.rs:75`）：`available` 为**显式空列表**且 `initial`、`discoverable` 都为空（`None` = 未设置=继承全部，不算）。

- 请求构造走 `tools: None` 分支；一致性处理：拒绝"空池 + 非空 initial/discoverable"的矛盾配置、历史归一跳过空池解析、冻结空 tool snapshot、提示块不渲染工具、workflow 内 agent_loop 节点同样跳过解析。
- **重要事实**：全仓搜索 `available_tool_names = []` 仅命中测试，**没有产品入口主动构造这条路径**。它是一条被各入口一致支持的内部/兼容档位。

它**仍走完整 agent loop**（事件、审批、迭代、checkpoint、执行记录），只是请求里没有工具。

### 9.2 路径 B：TemporaryMode

即「形态四」。restricted 服务于草稿围栏首轮且可原地晋升，ephemeral 服务于只读快问且不复用 id。它仍是 agent run，与 ChatSession 的 `ExecutionType::Chat` 是两条执行域。

## 10. 职责边界总表

| 维度 | ChatSession 原语 | chat 执行链 | Single Shot | Agent Loop（完整/路径 A） | 临时对话（路径 B） |
|------|------|------|------|------|------|
| 抽象层 | 引擎原语 | 执行域 | 引擎原语 | 执行域 / agent 运行档 | agent 运行档 |
| 会话状态 | 有（多轮、restore） | 有（持久化多轮） | 无（一次调用） | 有（多轮迭代/压缩） | 有（同 agent loop） |
| `execution_type` | 无（不落库） | `Chat` | 无（不落执行记录） | `Agent` | `Agent` |
| 工具 | 无（类型层禁止） | 无 | 可选：一轮 allow-list 有界工具（≤4） | 完整工具池 / 显式空池 | 受限/临时白名单（非"无工具"） |
| 模型往返次数 | 每轮一次 | 每轮一次 | 恰好一次 | 多轮直到收敛 | 多轮（同 agent loop） |
| 工具结果回喂 | 不适用 | 不适用 | 否（返回调用方） | 是（驱动下一轮） | 是 |
| 生命周期（事件/审批/迭代/checkpoint/层级） | 无 | 无 | 无 | 有（路径 A 也完整） | 有（ephemeral 关 checkpoint） |
| 落库 | 无（上层负责） | message store + `agent_execution` | 无（调用方决定） | agent loop 全链路 | agent loop 全链路（消息/记录仍存） |
| 可否晋升为完整 agent run | 不适用 | 否（终态） | 不适用（一次性） | 本身就是 | restricted 可、ephemeral 否 |
| 产品入口 | 被上层调用 | `/chat` 类 | 内部评审/压缩/脚本 + API 直连 | `/agent` 执行 | Web `/fenced`、`/readonly` |

真正的重叠只有几格："都不调完整工具""都只问一次或少数几次"。但：

- ChatSession / chat 执行域是**永久无工具的问答执行域**，终态，不晋升。
- Single Shot 是**无会话、无迭代闭环的一次性原语**，工具结果不回喂，服务于判定型/生成型子任务。
- 临时对话是**带围栏的 agent run**：restricted 明确服务于"草稿首轮"并可原地晋升，ephemeral 服务于"只读快问"但不复用 id。它的价值恰恰在于"仍是 agent run"——保留审计、事件、层级、可晋升语义。

## 11. 设计合理性判断（保留理由）

1. **抽象分层正确**：ChatSession 是 library primitive，调用方拥有状态、零副作用零落库（`chat.rs:1-8`、`:28`）；Single Shot 是无状态原语，工具结果交给调用方（`single_shot.rs:1-8`）。把它们与有状态的 agent loop 合并会污染原语的纯粹性。
2. **落库归口统一**：chat 以 `ExecutionType::Chat` 混入 `agent_execution` 并双写 message store，换来统一的 list/detail/hierarchy 能力且查询侧零特例（`execution_listing.rs:127`、`:293`），收益明确、代价可控。
3. **临时模式语义独立**：`TemporaryMode` 的"不改 executor、只强制 exposure"（`temporary.rs:5-9`）保证它天然继承 agent 的全部治理能力；restricted/ephemeral 的晋升差异（`temporary.rs:17-33`）是 chat 执行域无法表达的。
4. **一致性已内建**：空池纯 chat 在校验、历史归一、请求构造、提示渲染、workflow 节点等各入口都有统一处理（`tool_exposure.rs:75`），single shot 在 allow-list、调用数上限、JSON 校验上也有严格边界（`single_shot.rs:20`、`:105-119`），说明它们都是被有意维护的档位，而非遗留死角。

**结论**：保留现状。本次仅补充本文档，不改动任何源码。

## 12. 选型准则（给开发者）

| 场景 | 用哪个 |
|------|--------|
| 纯文本多轮问答，不需要工具/审批/checkpoint/agent 事件 | chat 执行链（底层 `ChatSession`） |
| 一次性判定/生成，最多跑一轮受控工具且不需要把结果回喂模型（如评审 verdict、压缩摘要、单次脚本） | Single Shot（`generate_text_once` / `generate_with_tools_once`，或 `wf-api` 的 `llm::generate*`） |
| 想让模型自主多轮调用工具、迭代到收敛，并要完整审计/层级 | Agent Loop（完整档） |
| 要 agent 审计/层级，但首轮先围栏只读、之后继续同一执行 | Agent Loop + `TemporaryMode::Restricted`（Web `/fenced`） |
| 一次性只读快问，不要 checkpoint、不复用执行 id | Agent Loop + `TemporaryMode::Ephemeral`（Web `/readonly`） |
| 显式空工具池的 agent run | 路径 A（内部档位，无产品入口） |

## 13. 事实核验索引

按形态列出关键 `file:line`，均取自 commit `00bd8970`。

| 形态 | 关键位置 |
|------|----------|
| ChatSession 原语 | `wf-execution-shared/src/chat.rs:1-8`（模块头）、`:20`（ChatTemplate）、`:28`（ChatSession）、`:122`（tools: None）、`:356`（无工具测试） |
| chat 执行链 | `wf-api/src/chat.rs:32`（`@chat`）、`:55`（stream）、`:126-142`（双写调用）、`:317-343`（persist_messages）、`:347-408`（finish_turn）、`:380-381`（execution_type/definition_id） |
| 执行类型 | `wf-types/src/execution/hierarchy.rs:5-12`；`wf-api/src/execution_listing.rs:127`、`:293` |
| Single Shot | `wf-execution-shared/src/single_shot.rs:1-8`、`:20`（上限）、`:47`（generate_with_tools_once）、`:99-100`（工具上下文）、`:105-119`（allow-list/JSON）、`:144-147`（outcome）、`:155`（generate_text_once）；调用点见「谁在用 single shot」 |
| Agent Loop | `wf-agent/src/coordinator/lifecycle/run.rs:83`（run_loop）、`:154`/`:174`（execute_inner）、`:181`（start_agent_loop）、`:204`（BEFORE_AGENT）、`:461`/`:488`（settle）；`wf-execution-shared/src/execution_loop.rs:1-8` |
| 临时对话 | `wf-types/src/agent_execution/temporary.rs:5-9`、`:15-34`；`loop_config.rs:119`；`wf-agent/.../entity.rs:23`（forced_temporary_config）；`checkpoint.rs:46`；`wf-server/.../lifecycle.rs:141-146`、`:180`；Web `chat-session.svelte.ts:596`、`:616`、`:746-747`、`:966` |
| 空池路径 A | `wf-tools/src/tool_exposure.rs:75`（is_explicit_no_tools） |

## 14. 补充：Workflow（图编排执行域）

前文四种形态都是"跟模型说话"的单位；workflow 是把它们组织起来的**图编排执行域**：按预定义的节点图推进，模型调用发生在具体节点内部，而不是 workflow 本身直接跟模型对话。这也是标题说"四种形态"而本节单独补充的原因——它不在同一抽象层面。

### 14.1 定位

- `ExecutionType::Workflow`（`crates/foundation/wf-types/src/execution/hierarchy.rs:6`）是独立执行域，与 Agent / Chat 共用 `agent_execution` 表与统一的 list/detail/审计体系。
- 引擎位于 `crates/engine/wf-workflow`：图校验与分析（`validation.rs`、`analysis.rs`）、节点 handler 注册表（`handler.rs`）、生命周期协调（`coordinator/`）、checkpoint 与状态持久化。
- 与 agent loop 共享 `wf-execution-shared` 的循环原语（中断门控、checkpoint 接入、指标，见 §6），但不强制统一循环结构——agent 按迭代推进，workflow 按节点推进。

### 14.2 模型调用发生在节点内部

节点是模型调用的载体，主要三类：

| 节点 | 语义 | 位置 |
|------|------|------|
| LLM 节点 | 单次流式生成，节点内可执行工具，但结构固定、无自主迭代闭环 | `crates/engine/wf-workflow/src/handler/llm.rs` |
| AGENT_LOOP 节点 | 在节点内嵌入完整 agent loop（复用 `wf-agent` 注册表），即"图中的一个自主执行点" | `crates/engine/wf-workflow/src/handler/agent_loop.rs:18` |
| 非模型节点 | script、route、fork/join、subgraph、trigger、user_interaction 等，负责控制流与副作用 | `crates/engine/wf-workflow/src/handler.rs:1-21` |

对比：LLM 节点接近 single shot 的"有限工具一轮"语义（区别在于它把工具结果回喂当轮上下文，且受图的编排约束）；AGENT_LOOP 节点则完整复用形态三的生命周期。

### 14.3 选型准则

| 场景 | 用哪个 |
|------|--------|
| 流程结构预先已知：分支、并行、循环由图定义，模型只在固定点位介入 | Workflow + LLM 节点 |
| 流程中某一步需要模型自主多轮迭代到收敛 | Workflow + AGENT_LOOP 节点 |
| 没有预定义结构，整个执行就是一次自主迭代 | Agent Loop（形态三） |

**要点**：workflow 不与前四种形态竞争，而是它们的编排层——单次生成落在 LLM 节点、完整 agent loop 落在 AGENT_LOOP 节点，chat 则不参与 workflow 编排。
