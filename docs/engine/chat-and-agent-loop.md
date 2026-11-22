# ChatSession 与 agent loop 的职责边界

> 范围：`crates/engine/wf-execution-shared/src/chat.rs`、`crates/app/wf-api/src/chat.rs`、`crates/engine/wf-agent/src/coordinator/lifecycle/`、`crates/foundation/wf-types/src/agent_execution/`、Web 前端 `apps/web-app/src/lib/chat/chat-session.svelte.ts`
> 定位：说明文档。回答三个问题——chat 会话为何"双写"、为何"混入 agent 执行表"、agent loop 的"无工具/临时模式"是否与 ChatSession 重复。
> 结论：**现有设计合理，保留，不改代码**。二者是不同抽象层的两条路径，仅是表层能力（都不调工具）重叠，语义、生命周期、落库归口与晋升方向均不同。补充本说明以消除歧义。

## 0. 文档元信息

| 项 | 值 |
|------|------|
| 状态 | 说明定稿（未改源码） |
| 触发问题 | `ChatSession` 与 agent loop 职责重叠；chat 双写；chat 混入 agent 执行表；agent loop 无工具模式是否等于临时会话 |
| 结论 | 保留现状，仅补文档 |
| 核验方式 | 逐条 `file:line` 引述，均取自本仓库当前 HEAD |
| 代码 diff | 本次无源码改动（见 §10 可选 cross-ref 建议） |

## 1. 背景与问题

系统里存在三种"跟模型说话"的形态，名字相近、外观相似，容易误判为重复实现：

1. `ChatSession`：引擎原语层的无工具多轮聊天。
2. chat 执行链（`/chat` 类入口）：把 `ChatSession` 包装成一条可持久化的会话。
3. agent loop 的"无工具档"：完整 agent 生命周期下把工具暴露收紧（甚至清空）的运行方式。

需要回答：这些是不是重复？chat 为什么要写两处存储？chat 记录为什么落在 `agent_execution` 表里？agent loop 的"无工具模式"是不是就是"临时会话"？

## 2. 核心结论（TL;DR）

| 问题 | 结论 | 依据 |
|------|------|------|
| chat 为何双写 | 消息写 `message` store 存 transcript，会话元数据写 `agent_execution` 存状态/时间/模型/目录；两者职责不同，缺一不可 | `crates/app/wf-api/src/chat.rs:124-151`、`:317-343`、`:347-408` |
| chat 为何混入 agent 执行表 | chat 是 `ExecutionType::Chat`，与 Agent 共用一张表和同一套 registry/list/detail，查询侧无需特例 | `crates/foundation/wf-types/src/execution/hierarchy.rs:3-12`、`crates/app/wf-api/src/execution_listing.rs:125-127`、`:289-294` |
| agent loop 无工具模式是否等于临时会话 | 部分正确但不等同。存在两条独立路径：`explicit empty pool` 的纯 chat run（完整生命周期、无产品入口）；`TemporaryMode`（restricted/ephemeral，服务于草稿围栏/只读快问，可晋升或不可晋升） | `crates/engine/wf-tools/src/tool_exposure.rs:71-83`、`crates/foundation/wf-types/src/agent_execution/temporary.rs:3-34` |
| 是否与 ChatSession 重复 | 表层重叠（都不调工具），本质不同：是否走 agent 生命周期、是否可晋升、落库归口不同。**保留** | 见 §7、§8 |

## 3. 事实：ChatSession 是引擎原语，不是执行域

`crates/engine/wf-execution-shared/src/chat.rs` 的模块头已把定位写死：

> "Tool-free multi-turn chat over a caller-owned session ... There is no tool execution, no checkpointing, no approval, no event publishing and no execution-record persistence. The request template carries no tools field, so a tool-carrying chat request cannot be constructed."（`chat.rs:1-8`）

- `ChatTemplate` 只有 `profile_id / parameters / generation` 三个字段，**没有 tools**（`chat.rs:20-24`）。
- `ChatSession` 只持有 `template + conversation`，注释明确 "nothing is shared with the agent loop or the checkpoint machinery"（`chat.rs:26-31`）。
- `begin_turn` 构造的请求把 `tools`、`tool_call_protocol` 等全部置 `None`（`chat.rs:93-112`，其中 `tools: None` 在 `:98`）。
- 测试 `a_turn_never_carries_tools` 断言请求既无 tools 也无 tool_call_protocol（`chat.rs:331-337`）。

**要点**：`ChatSession` 自身**不落库**——既不写 message store，也不写 `agent_execution`。持久化是上层 `wf-api` 包装链做的事（§4）。它是供上层复用的纯多轮生成原语。

## 4. 事实：chat 执行链的"双写"与"混入执行表"

chat 处理链在 `crates/app/wf-api/src/chat.rs`：

- 模块头说明会话锚定在 session id，会话本体在 message store，执行器是无工具 chat 原语，"no agent loop events are published"（`chat.rs:1-7`）。
- 每轮用 `ChatSession::new` + `restore(history)` 续接已存会话（`chat.rs:88-89`），`begin_turn` 生成请求（`chat.rs:111-113`）。
- 流式结束后的 driver 做两件事（`chat.rs:124-151`）：
  - `finish_turn(...)` 无条件执行（`:126-134`）；
  - `persist_messages(...)` 仅在 `Completed` 时执行（`:136-142`）。

这就是"**双写**"——两处存储、两种职责：

| 写入 | 函数 | 存储 | 内容 | 行号 |
|------|------|------|------|------|
| ① 会话元数据 | `finish_turn` | `agent_execution` 表 | 一条 `AgentExecution`：状态、起止时间、模型 profile、目录 | `chat.rs:347-408` |
| ② 消息 transcript | `persist_messages` | `message` store | 本轮 user + assistant 消息，按 `ledger_base` 递增时间戳保证全序 | `chat.rs:317-343` |

"**混入 agent 执行表**"指①：chat 会话以 `ExecutionType::Chat` 写进 `agent_execution`，与 agent loop 共用同一张表：

- `ExecutionType` 三值：`Workflow / Agent / Chat`，Chat 注释为 "Tool-free multi-turn question answering"（`crates/foundation/wf-types/src/execution/hierarchy.rs:3-12`）。
- 写入记录时 `execution_type: ExecutionType::Chat`、`definition_id: "@chat"`，并把 `current_iteration / tool_call_count` 置 0、`iteration_history / loop_config` 置 `None`（`chat.rs:378-399`，其中 `:380-381`、`:383-385`、`:393`）。
- 常量 `CHAT_SESSION_DEFINITION_ID = "@chat"`（无 agent 模板，故以执行器命名，`chat.rs:30-32`）。
- 查询侧**无特例**：list 把 Agent 与 Chat 一起查（`crates/app/wf-api/src/execution_listing.rs:125-127`）；detail 注释直言 "chat sessions are `agent_execution` rows"（`execution_listing.rs:289-294`）。测试确认会话记录可被 load 且消息可续接（`chat.rs:519-551`、`:553-599`）。

**为什么要这样设计**：让 chat 会话自动获得与 agent run 统一的列表、详情、层级、审计能力，无需为它另建一套存储与查询分支；代价只是几个字段置零/置空。`definition_id="@chat"` 则把"它没有 agent 模板"这一事实显式编码进去。

## 5. 事实：agent loop 主循环职责

`crates/engine/wf-agent/src/coordinator/lifecycle/run.rs` 是 agent loop 的生命周期模板，`execute / execute_stream / resume_from_checkpoint` 共用它（`:78-90` 注释与 `run_loop` 签名）：

1. 配置校验、注册与父链、起始记录、`BEFORE_USER_PROMPT`、压缩消费者、`execute_inner`、结束记录（`run.rs:83-160`）。
2. `execute_inner`：`start_agent_loop`（`:181`）→ `BEFORE_AGENT` hook 与 lifecycle checkpoint（`:197-215`）→ 组装 iteration/approval/file 协调器（`:217-315`）。
3. 迭代循环 `execution_coordinator.execute(...)`（`:338-343`）。
4. settle：complete / timeout / cancel / fail 及 OnComplete checkpoint（`:345-484`）。

此外 `crates/engine/wf-execution-shared/src/execution_loop.rs:1-8` 提供 agent 与 workflow 共享的循环原语（中断门控、checkpoint 接入、指标），但**不强制统一循环结构**——因为 agent 按迭代、workflow 按节点，粒度不同。

**要点**：agent loop 的职责是"带完整生命周期的一次 agent 执行"——事件、审批、迭代、checkpoint、执行记录、层级。这些是 `ChatSession` 完全没有的。

## 6. 事实：两条"无工具"路径

所谓"agent loop 无工具模式"实际上是两条互相独立的路径，不能混为一谈。

### 6.1 路径 A：`explicit empty pool` 纯 chat run

判定谓词 `is_explicit_no_tools`：`available` 为**显式空列表**且 `initial`、`discoverable` 都为空（`crates/engine/wf-tools/src/tool_exposure.rs:71-83`）。注意 `None`（未设置=继承全部）不算纯 chat（`:85-94`）。

- `AgentLoopEntity::is_explicit_no_tools()`：`entity.rs:248-256`。
- 请求构造走 `tools: None` 分支（`crates/engine/wf-agent/src/agent_request.rs:51-70`）。
- 一致性处理：拒绝"空池 + 非空 initial/discoverable"的矛盾配置（`crates/engine/wf-agent/src/validation.rs:533-542`）；历史归一跳过空池解析（`crates/engine/wf-agent/src/coordinator/lifecycle/entity.rs:83-93`）；冻结空 tool snapshot（`:262-268`）；提示块不渲染工具（`crates/engine/wf-execution-shared/src/agent_prompt/exposure.rs:17-39`）；workflow 内 agent_loop 节点同样跳过解析（`crates/engine/wf-workflow/src/handler/agent_loop/conversation.rs:100-107`）。
- **重要事实**：全仓搜索 `available_tool_names = []` 仅命中测试，**没有产品入口主动构造这条路径**。它是一条被各入口一致支持的内部/兼容档位。

这条路径**仍走完整 agent loop**（§5 的事件、审批、迭代、checkpoint、执行记录），只是请求里没有工具。

### 6.2 路径 B：`TemporaryMode`（restricted / ephemeral）

定义于 `crates/foundation/wf-types/src/agent_execution/temporary.rs:3-34`：

> "`None` ... means a full agent run. A temporary mode never changes the executor: the row stays `execution_type = agent`, the checkpoint and history machinery stays the same shape, only the exposure is forced and, for the ephemeral tier, checkpointing is switched off."（`temporary.rs:5-9`）

两档语义：

| 档位 | 工具暴露 | checkpoint | 持久化 | 晋升 | 行号 |
|------|----------|-----------|--------|------|------|
| `Restricted` | 强制临时白名单（只读检查、交互、网络查询） | 正常 | 与完整 agent run 相同 | 同 `execution id` 去掉 flag 重跑即晋升 | `temporary.rs:17-23` |
| `Ephemeral` | 强制 ephemeral 白名单（只读检查+交互，无网络/无副作用） | 完全关闭（interval/lifecycle/hook/file history session/manual 创建均禁） | 消息与执行记录仍存，transcript 可重放 | 不复用同一 id，另起新执行并带显式历史 | `temporary.rs:24-33` |

支撑点：

- 配置字段 `temporary_mode: Option<TemporaryMode>`，注释强调"引擎侧强制覆盖、不合并，客户端无法扩宽自己的笼子"（`crates/foundation/wf-types/src/agent_execution/loop_config.rs:113-119`）。
- 强制逻辑 `forced_temporary_config`：把 available 换成档位白名单，清空 initial/discoverable，过滤 activated，隐藏表追加 denied，关闭 `general` 代理（`crates/engine/wf-agent/src/coordinator/lifecycle/entity.rs:11-54`）。
- ephemeral 关 checkpoint：`checkpoint_integration_for_config` 直接返回 `None`（`crates/engine/wf-agent/src/coordinator/lifecycle/checkpoint.rs:46-56`）；压缩消费者不建 checkpoint（`run.rs:53-63`）；不留 file history session（`run.rs:263-275`）；hook checkpoint 跳过（`run.rs:266`）；手动创建被拒（`crates/engine/wf-agent/src/checkpoint/coordinator.rs:281-309`）。
- 入口：server 层严格解析 wire 名（未知值拒绝，`crates/app/wf-server/src/api/agent/loops/lifecycle.rs:51-56`、`:138-150`）；Web 前端 `/fenced → restricted`、`/readonly → ephemeral`（`apps/web-app/src/lib/chat/chat-session.svelte.ts:740-748`），`askFenced` 设置一次性 scope（`:611-624`），请求带 `temporaryMode`（`:959-969`），非 agent 执行类型会清空该 scope（`:596`）。CLI 侧恒为 `None`，无 CLI 入口（`crates/app/cli/wf-cli-shared/src/turn.rs:115-122`）。

**要点**：`TemporaryMode` 之所以被看成"临时会话"，是因为注释用它描述草稿受限首轮（`temporary.rs:18-22`）和只读快问（`:24-33`），且 Web 入口就叫 fenced/readonly。但它跑在 `execution_type = agent` 上，走完整 agent 生命周期——这与 `ChatSession` 的 `ExecutionType::Chat` 是两条不同的执行域。

## 7. 重叠分析与职责边界

| 维度 | ChatSession（原语） | chat 执行链（`ExecutionType::Chat`） | agent loop 路径 A（空池纯 chat） | agent loop 路径 B（TemporaryMode） |
|------|------|------|------|------|
| 抽象层 | 引擎原语 | 执行域 | agent 运行档位 | agent 运行档位 |
| `execution_type` | 无（不落库） | `Chat` | `Agent` | `Agent` |
| 工具 | 无（请求无 tools 字段） | 无 | 无（空池） | 受限白名单（非"无工具"） |
| 生命周期（事件/审批/迭代/checkpoint/层级） | 无 | 无 | 有 | 有（ephemeral 关 checkpoint） |
| 落库 | 无（上层负责） | message store + agent_execution | agent loop 全链路 | agent loop 全链路 |
| 是否可晋升为完整 agent run | 不适用 | 否（终态） | 不适用（换配置即可） | restricted 可、ephemeral 不可 |
| 产品入口 | 被上层调用 | `/chat` 类 | 无（仅测试构造） | Web `/fenced`、`/readonly` |

真正的重叠只有一格："都不调用工具"。但：

- `ChatSession`/chat 执行域是**永久无工具的问答执行域**，终态，不晋升。
- `TemporaryMode` 是**带围栏的 agent run**，restricted 明确服务于"草稿首轮"并可原地晋升，ephemeral 服务于"只读快问"但不复用 id。它的价值恰恰在于"仍是 agent run"——保留审计、事件、层级、可晋升语义。

因此这不是冗余实现，而是有意的分层：一个面向纯问答的轻量执行域，一个面向 agent 的受限运行档。

## 8. 设计合理性判断（保留理由）

1. **抽象分层正确**：`ChatSession` 是 library primitive，调用方拥有状态、零副作用零落库（`chat.rs:1-8`、`:26-31`）；把它与有状态的 agent loop 合并会污染原语的纯粹性。
2. **落库归口统一**：chat 以 `ExecutionType::Chat` 混入 `agent_execution` 并双写 message store，换来统一的 list/detail/hierarchy 能力且查询侧零特例（`execution_listing.rs:125-127`、`:289-294`），收益明确、代价可控。
3. **临时模式语义独立**：`TemporaryMode` 的"不改 executor、只强制 exposure"（`temporary.rs:5-9`）保证它天然继承 agent 的全部治理能力；restricted/ephemeral 的晋升差异（`temporary.rs:17-33`）是 chat 执行域无法表达的。
4. **一致性已内建**：空池纯 chat 在校验、历史归一、请求构造、提示渲染、workflow 节点等各入口都有统一处理（§6.1），说明它是被有意维护的档位，而非遗留死角。

**结论**：保留现状。本次仅补充本文档；不改动任何源码。

## 9. 选型准则（给开发者）

| 场景 | 用哪个 |
|------|--------|
| 纯文本多轮问答、不需要工具/审批/checkpoint/agent 事件 | chat 执行链（底层 `ChatSession`） |
| 想要 agent run 的审计/层级，但首轮先围栏只读、之后可继续同一执行 | agent loop + `TemporaryMode::Restricted` |
| 一次性只读快问，不要 checkpoint、不打算复用该执行 id | agent loop + `TemporaryMode::Ephemeral`（Web `/readonly`） |
| 显式空工具池的 agent run | 路径 A（内部档位，当前无产品入口，见开放点①） |
