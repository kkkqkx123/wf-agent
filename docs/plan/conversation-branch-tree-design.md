# 对话分支树执行实例设计（子树嫁接 + 脏状态步进重算）

> 分析对象：`kkkqkx123/wf-agent` @ `d477951`（递归拉取，含子模块 `crates/infra/llm-suite`、`crates/infra/simvec`）。
> 场景边界：**一问一答的对话树**（知识探索、方案设计、查询型 MCP），只需要编排上下文历史，不需要文件检查点、工作区快照、工具审批与自治多轮循环。
> 本文件是 v2 重写版，`attach` / `detach` 的含义由"观察者绑定"改为**子树嫁接**，并新增"脏状态 + 步进重算"这条主线。

## 0. 结论速览

**修订记录（v1 → v2）**

| 项 | v1 | v2 |
| --- | --- | --- |
| `attach` / `detach` | 观察者订阅 / 退订 | **子树嫁接 / 剥离**（树的搬运操作） |
| 观察者绑定 | 叫 attach / detach | 改称 `subscribe` / `unsubscribe`（`GET /stream` + 断流），与代码既有 `detach` 端点解耦 |
| 前继节点 | 未涉及 | 支持**增加 / 替换 / 移除**前继（含首个问话引入的前提） |
| 改写助手正文 | 允许，仅留档 | 允许，**并使下游全部进入脏状态** |
| 一致性机制 | 无（靠人工判断） | **内容指纹**：脏 = 回复生成时的前缀摘要 ≠ 当前路径前缀摘要 |
| 重算 | 单条"重新生成" | **单步** + **顺序步进**（沿特定路径批量重算脏节点） |

**核心结论**

1. **一问一答式执行实例已经存在，但它不是"活着的实例"。** `ExecutionType::Chat`（`crates/foundation/wf-types/src/execution/hierarchy.rs:5-12`）+ `wf_api::chat::stream`（`crates/app/wf-api/src/chat.rs:55`）+ `ChatSession`（`crates/engine/wf-execution-shared/src/chat.rs:28`）语义成立：一轮 = 一次生成、无工具、无检查点、不发布事件。但会话只是 `agent_execution` 的一行记录（`crates/app/wf-api/src/execution_hierarchy/scan.rs:24-31`），SSE 一断就 `abort()` 驱动任务（`crates/app/wf-api/src/infra/stream.rs:238-244`），而回复正文正写在这个任务里（`crates/app/wf-api/src/chat.rs:317-343`）——断连等于本轮作废，且没有任何可寻址的在飞句柄。
2. **历史没有边。** 存储信封只有 `id / execution_id / agent_loop_id / message`（`crates/foundation/wf-types/src/storage/message.rs:6-14`），读取按 `timestamp` 排序；`Message` 定义在外部子模块 crate（`crates/infra/llm-suite/crates/llm-suite-types/src/message.rs:271-292`），不能加 `parent_id`。所以树结构必须**独立于消息**存放。
3. **"脏"不能用手工布尔标记，必须用内容指纹。** 树上任何一个祖先的输入或输出被改动，其下游的"历史"就变了，理论上答案也应不同。用 `reply.based_on != base(node)`（两者都是前缀摘要）来判定：改动即脏、改回即干净、换分支各算各的，且不需要写时层层传播。
4. **嫁接是否"脏"，由前缀是否物化决定，而不是特判。** 因为摘要是对"段序列"的折叠，把被嫁接节点的祖先链按同样的段序列物化到新树根，折叠结果不变 ⇒ 子树保持新鲜；不物化则整棵子树变脏。跨会话嫁接因此天然全脏，这正是"历史变了"的诚实表达。
5. **树的形状只有 5 个手术动词**：`insert_before`（增前继）、`splice_out`（移除前继/上提）、`reparent`（改父）、`detach_subtree`（剥离）、`attach_subtree`（嫁接）。"替换前继"= `attach(mode=replace)`，即先剥旧再挂新；编辑、弃用、重算都由这几招 + 内容操作组合而成。
6. **原位改 vs 分支改是本设计最关键的一条语义分野**：原位改 = 同一条分支上改主意，旧答案失效 ⇒ 下游全脏；分支改 = 旁边再试一种问法，两套答案各自自洽 ⇒ 不产生脏。前者解决"不成熟前提持续污染"，后者解决"保留对照"。

## 1. 现状核对

### 1.1 已存在的一问一答式执行实例

| 事实 | 证据 |
| --- | --- |
| 执行器类型里已有一档 | `ExecutionType::{Workflow, Agent, Chat}`，Chat 注释为 "Tool-free multi-turn question answering"（`crates/foundation/wf-types/src/execution/hierarchy.rs:5-12`） |
| 一轮的入口是单次调用 | `pub async fn stream(ctx, params: ChatTurnParams)`（`crates/app/wf-api/src/chat.rs:55`），参数含 profile / message / images / session_id / directory（`:36-49`） |
| 会话锚是 session_id，历史每轮从存储重放 | `resolve_session_id`（`:412-419`）→ `conversation_history(ctx, &session_id, None)`（`:77`） |
| 每轮新建内存会话并重新灌入历史 | `ChatSession::new`（`:88`）+ `session.restore(history)`（`:89`） |
| 本轮注入的上下文消息也先灌进历史 | 提及消息与回执逐条 `session.restore(vec![...])`（`:91-109`），回执由 `dynamic_context_message` 生成（`crates/engine/wf-execution-shared/src/agent_prompt/tail_ops.rs:143-150`，实际是一条带 `type` 元数据的 `user` 消息） |
| 一轮只发一次生成请求 | `begin_turn`（`:112`）→ `generate_stream`（`:117`），驱动循环只转发增量 |
| 结构上不可能带工具 | `begin_turn` 内 `tools: None`（`crates/engine/wf-execution-shared/src/chat.rs:98`），模块注释写明 "request template carries no tools field"（`:1-8`） |
| 无检查点、无审批、不发布事件 | 模块注释 "no tools, no iterations, no checkpoints and no agent loop events are published"（`crates/app/wf-api/src/chat.rs:3-7`），文件内无事件总线引用 |
| 落库的两笔 | `persist_messages`（`:317-343`）按 `history.len() - (2 + injected)` 切片写消息；`finish_turn`（`:347-408`）写会话记录 |
| 对外只有一个端点 | `POST /api/v1/chat/stream`（`crates/app/wf-server/src/api/chat.rs:21-25`） |

**结论**：语义上"一问一答"成立；实例上它只活在一个 HTTP 请求的生命周期里——没有注册表、没有状态机、没有可寻址的在飞句柄。

### 1.2 撑不起分支树的五个缺口

1. **没有边**：消息按 `timestamp` 平铺（`crates/app/wf-api/src/storage` 侧 `MessageListOptions` 的排序即时间序，`crates/infra/wf-storage/src/adapter/message.rs:21-47`），无法表达"这一轮从哪继续"。
2. **消息不可变、也不该变**：`Message` 是外部 crate 的值类型（`llm-suite-types/src/message.rs:271-292`），存储适配器只提供 `save / load / list / delete`（`crates/infra/wf-storage/src/adapter/base.rs:20-48`），没有 update。这个约束正好被本设计利用：**内容哈希一次算定、永久有效**。
3. **断连即中止且丢结果**：`ExecutionEventStream::drop` 直接 `task.abort()`（`crates/app/wf-api/src/infra/stream.rs:238-244`），文件头注释也写明 "dropping the stream aborts the driver"。
4. **没有可回放的事件源**：chat 不发事件，即便加了订阅端点也没内容可回放。
5. **无并发护栏**：两轮同时提交会各自读历史、各自追加（`crates/app/wf-api/src/chat.rs:77-110` 无锁）。

### 1.3 命名冲突：代码里的 `attach` / `detach` 是别的意思

仓库已有 `attach` / `detach` 端点与运行时，语义是**观察者绑定**：

* `GET /agent-loops/{id}/attach` 回放事件流（路由 `crates/app/wf-server/src/api/agent/loops.rs:44`，处理器 `crates/app/wf-server/src/api/agent/loops/detached_handlers.rs:177`，回放参数 `:195-200`）。
* `docs/plan/remote-keepalive-and-shutdown-semantics.md:9-11` 明确三分："`detach`（只解绑观察者）、`stop`（终止执行）、`close`（关服务）"。

本方案的 `attach` / `detach` 是**树的搬运操作**，与观察者无关。为免歧义，全文约定：

| 概念 | 本方案用词 | 端点点位 |
| --- | --- | --- |
| 把子树挂到某个父节点下 | `attach`（嫁接） | `POST /chat/sessions/{id}/attach` |
| 把子树从树上剥下来 | `detach`（剥离） | `POST /chat/sessions/{id}/detach` |
| 观察者订阅 / 退订事件流 | `subscribe` / `unsubscribe` | `GET /chat/sessions/{id}/stream` + 断流 |

既有 agent loop 的 `attach` / `detach` 端点**保持原样不动**，只在文档与错误信息中不再复用这两个词指代观察者。

### 1.4 可复用的与不可复用的

| 机制 | 结论 |
| --- | --- |
| 事件总线 + 持久化回放 + `SubscriberGuard`（`crates/app/wf-server/src/api/agent/loops/detached_handlers.rs:177-250`） | **复用**（作为 subscribe 的底座） |
| 空闲回收 / 配额记账（`crates/app/wf-server/src/detached.rs:36-60`、`:142`） | **复用**（会话实例注册表） |
| 在飞任务中止登记（`crates/app/wf-api/src/infra/tasks.rs:18`） | **复用**（键用 `turn_id`） |
| `ChatSession` + `ConversationSession` 的投影（`crates/engine/wf-execution-shared/src/chat.rs:199` `view_messages`） | **复用**（装配路径消息后灌进去） |
| 执行层级树（`crates/foundation/wf-core/src/hierarchy.rs:14-64`） | 不用：节点是"执行"，chat 没有子执行 |
| checkpoint fork（`crates/app/wf-api/src/agent/agent_execution/checkpoint.rs:11-64`） | 不用：复制式分支让公共前缀重复存储，分支一多就是 O(n²) |
| 内存里的分支原语（`crates/engine/wf-execution-shared/src/conversation_session/history.rs:112-139`） | 不用：切的是内存前缀，不解决持久化与实例化 |
| TUI 只读树干（`crates/app/cli/wf-tui/src/session_tree.rs:1-9`） | 需改：其注释断言"message-level branches do not exist, so there is no pi-style leaf pointer to move"，本方案使该前提作废 |

## 2. 概念定义与不变量

### 2.1 术语

| 术语 | 含义 |
| --- | --- |
| 会话树 | 一个会话一棵树；节点 = 一轮（`Turn`）或一条前提（`Premise`） |
| 节点 | 一轮的容器：本轮注入的上下文 + 用户消息 + 可选回复（含工具轨迹） |
| 路径 | 根到某节点的节点链，决定发给模型的消息序列 |
| 叶子 | 当前头部；会话级 `active_leaf` 是默认头部 |
| **前继节点** | 路径上位于某节点之前的祖先节点。典型是"第一个问话引入的前提"（`Premise`） |
| **手术** | 对树形状的原子变更；只有 5 个动词（§6.1） |
| **嫁接 attach** | 把一棵子树搬到目标父节点下（同会话内搬迁，或从暂存区/另一会话搬入） |
| **剥离 detach** | 把一棵子树从当前树上切下来，成为独立的 Graft 树（进暂存区） |
| **前缀摘要 base** | 节点在被生成那一刻，其路径上"段序列"的折叠哈希（§5.2） |
| **脏 dirty** | 回复当前正文的生成基准 `based_on` ≠ 节点当前 `base`；即"历史已经变了，这个答案是按旧历史生成的" |
| **单步 step** | 只对指定节点重算一次回复 |
| **顺序步进 forward** | 沿特定路径，从第一条脏节点起逐条向下重算，直到路径干净或触界 |
| 在飞轮 | 正在生成中的一轮（`turn_id`），同一会话同时最多一个 |
| Graft 树 / 暂存区 | 被剥离子树的独立存储单元；会话的 `pool` 列出它 |

### 2.2 不变量

1. **单根**：所有节点的 `parent` 链最终到虚拟根 `root`；`root` 无父、不对应消息。
2. **同树**：`parent` 必须属于同一棵树；跨会话搬运只能 `detach` → `attach` 两步（各自是单文档原子写，天然带审计）。
3. **无环**：目标父节点不得位于被搬运子树的闭包内；插入即校验，并校验深度上限。
4. **`ordinal` 同一父下唯一且单调**；但 **`ordinal` 不参与摘要**（否则手术往返会误判为脏）。
5. **`version` 严格单调递增**；任何手术 +1。
6. **消息 append-only**：编辑一律"写新消息 + 改指针"，旧消息进入 `superseded`，永不覆写、永不物理删除。
7. **`Premise` 节点没有回复**：`kind == Premise ⇒ reply.is_none()`，且不触发生成。
8. **摘要只依赖路径内容**：不依赖节点 id、`ordinal`、`created_at`、消息 id、消息 `timestamp`。

不变量 8 带来一个强性质：**内容相同的路径 ⇒ 摘要相同 ⇒ 手术往返后自动恢复"干净"**（§5.4）。

## 3. 功能清单

优先级：P0 = 本方案必须落地；P1 = 同一轮实施内的自然延伸；P2 = 明确延后。

### 3.1 树结构

| 编号 | 功能 | 行为要点 | 优先级 |
| --- | --- | --- | --- |
| F1 | 树持久化 | 一会话一主树文档；单根、父子边、兄弟序、版本号 | P0 |
| F2 | 树快照 | 节点摘要（角色、首行、`kind`、回复态、脏标记、子数）+ `active_leaf` + `version` + 在飞轮 + 池 | P0 |
| F3 | 路径查询 | 给任意节点返回根到它的有序消息，带 `retained` / `dirty` 标记 | P0 |
| F4 | 线性会话兼容 | 无树文档时按时间戳合成退化树（只读、全干净）；首次手术时落盘 | P0 |
| F5 | 规模上限 | 节点数 / 深度上限，超限拒绝并给明确错误 | P1 |
| F6 | 节点标签 | 节点可命名，便于在树里定位；不影响上下文 | P2 |

### 3.2 手术（前继的增 / 替 / 删 + 嫁接）

| 编号 | 功能 | 行为要点 | 优先级 |
| --- | --- | --- | --- |
| F7 | 增加前继 `insert_before` | 在目标节点与其父之间插入新节点（`Turn` 或 `Premise`）；目标子树自动跟随；下游全脏 | P0 |
| F8 | 移除前继 `splice_out` | 目标节点的子节点上提到其父（保持相对顺序）；目标变无子叶子，仍在树上可原位挂回；下游全脏 | P0 |
| F9 | 替换前继 | `attach(mode=replace)`：先把旧前继子树剥离到池，再把新子树挂上；被替换者可在池里找回 | P0 |
| F10 | 剥离 `detach` | 子树切成独立 Graft 树进池；主树侧该分支消失，`active_leaf` 回退到其父 | P0 |
| F11 | 嫁接 `attach` | 从池 / 同树其他位置 / 另一会话把子树挂到目标父节点下；校验环、深度、节点数 | P0 |
| F12 | 前缀物化 | `carry_prefix=true` 时把祖先链按段序列物化到 Graft 树根，使被嫁接子树保持干净 | P0 |
| F13 | 暂存区列表 | 列出池中 Graft 树（标签、节点数、来源、剥离时间） | P1 |
| F14 | 手术可逆 | 每个手术记录逆向操作参数；`undo` 一键回退（P2 落地，机制在 P0 就绪） | P2 |
| F15 | 手术前置校验 | 环、跨树父、深度、节点上限、`Premise` 无回复、目标已软删 | P0 |

### 3.3 脏状态与步进

| 编号 | 功能 | 行为要点 | 优先级 |
| --- | --- | --- | --- |
| F16 | 内容摘要 | 每条消息写入时算 `hash`，随 `MessageRef` 存进树；摘要折叠为 `base` / `in_digest` | P0 |
| F17 | 脏判定 | `Kept` 且 `based_on != base` ⇒ 脏；纯计算，不落布尔标记 | P0 |
| F18 | 脏清单 | `GET /dirty?leaf=` 返回该路径上的脏节点（按路径序）+ `stale_since_version` + 总数 | P0 |
| F19 | 单步重算 | 对指定节点（默认第一条脏节点）重算一次；旧回复进 `superseded`；下游仍脏 | P0 |
| F20 | 顺序步进 | 从第一条脏节点起沿路径逐条重算，直到干净 / 触界 / 出错 / 取消 | P0 |
| F21 | 步进边界 | `max_steps`（默认 8）、`stop_at`、`policy=keep_human`（不覆盖人工改写正文） | P0 |
| F22 | 步进事件 | `step.started / step.progress / step.completed / step.stopped`，每步复用轮事件 | P0 |
| F23 | 自然收敛 | 重算后若下游 `in_digest` 未变（内容一致）则下游不脏，`forward` 提前结束 | P0 |
| F24 | 确认过期 | `acknowledge`：把当前 `base` 记为已确认，状态从 `Dirty` 降为 `Acknowledged`（不再拦提示） | P1 |
| F25 | 模型漂移 | profile / 参数变化只出 `modelDrift` 提示，**不置脏**（切模型不该让整棵树变脏） | P0 |
| F26 | 脏对提交的影响 | 默认不阻断新轮，仅告警；可选 `block_on_dirty=true` 时 409 + 脏清单 | P1 |

### 3.4 编辑与保留

| 编号 | 功能 | 行为要点 | 优先级 |
| --- | --- | --- | --- |
| F27 | 原位改用户消息 | 目标节点换新用户消息（旧消息进 `superseded`），子树保留 ⇒ 该节点及其下游全脏 | P0 |
| F28 | 分支改用户消息 | 在父节点下新建兄弟节点，原分支一字不动 ⇒ 不产生脏 | P0 |
| F29 | 改写助手正文 | 换新 assistant 消息，旧消息进 `superseded` + `edited_at`；`origin` 转 `Human`；下游全脏 | P0 |
| F30 | 弃用 / 恢复回复 | 节点级开关；弃用后该回复与轨迹不进上下文；**切换会使下游变脏**（历史变了） | P0 |
| F31 | 重新生成 | 等价于"对某节点单步"，只是不改输入 | P0 |
| F32 | 删除节点 / 子树 | 软删（`deleted_at`），消息不物理删除；`active_leaf` 落在被删子树内则回退到最近未删祖先 | P1 |
| F33 | 会话级系统提示 | 会话级 system prompt，随每次路径装配注入（作为虚拟首段，不进树） | P2 |

### 3.5 执行与生命周期

| 编号 | 功能 | 行为要点 | 优先级 |
| --- | --- | --- | --- |
| F34 | 在指定节点下发起一轮 | `parent` + 文本 / 图片；返回 `turn_id` + `node_id`，不等生成 | P0 |
| F35 | 订阅 / 退订 | `GET /stream`：快照帧 + 游标回放 + 实时帧；断流不终止执行 | P0 |
| F36 | 多观察者 | 同会话多端订阅，事件由总线天然广播 | P0 |
| F37 | 同会话单飞 | 一次一个在飞（轮或 forward 作业），否则 409 + 在飞信息 | P0 |
| F38 | 取消 | 取消后节点转 `Interrupted`，部分文本可见；`forward` 取消保留已完成步 | P0 |
| F39 | 实例注册与回收 | 惰性加载；无订阅者 + 无在飞 + 超阈值即淘汰（树已落库） | P0 |
| F40 | 崩溃恢复 | 加载时发现 `Pending` 且无在飞驱动 ⇒ `Interrupted` | P0 |
| F41 | 会话删除级联 | 删会话：主树 + 池中 Graft 树 + 消息一并清理（现有保留期只清检查点与执行记录，见 `crates/app/wf-api/src/retention/service.rs:23-33`） | P1 |

### 3.6 接口与非功能

| 编号 | 功能 | 行为要点 | 优先级 |
| --- | --- | --- | --- |
| F42 | HTTP 端点族 | 见 §10.1；OpenAPI 快照与 TS 类型同步 | P0 |
| F43 | 写序列化 + 版本冲突 | 同会话串行；变更可带 `if_version`，不匹配 409 + 当前版本 | P0 |
| F44 | 能力硬约束 | 树会话不得静默获得写文件 / 检查点 / 审批能力 | P0 |
| F45 | web-app 树交互 | 树侧栏、手术拖拽、脏徽标、"顺序重算"按钮 | P0 |
| F46 | TUI 树视图 | `session_tree` 从线性树干升级为树（只读优先） | P1 |
| F47 | Rust 客户端 | `wf-client` 端点常量 + 调用封装 | P1 |

## 4. 数据模型

### 4.1 结构放在哪里

| 方案 | 做法 | 评价 |
| --- | --- | --- |
| A. 给消息加父指针 | 在 `Message` 上加 `parent_id` | **不可行**：`Message` 属外部子模块 crate（`llm-suite-types/src/message.rs:271-292`），改动外溢到整个 LLM 层 |
| B. 在存储信封上加父指针 | 给 `MessageStorageMetadata`（`wf-types/src/storage/message.rs:6-14`）加 `parent_id / node_id` | 可行但结构分散：一次"上提子树 + 改叶子"要写 N 条记录，非原子；头部指针无处安放 |
| C. 独立树文档引用消息 | 一会话一文档，节点只存消息 id 与 **内容哈希** | **采用**：一次变更一笔原子写；消息保持 append-only；与现有消息读路径正交 |

代价是"结构 + 内容"两处写，故必须定死写序与恢复语义（§4.8）。

### 4.2 树文档 `ConversationTree`

主键 `tree_id`（ULID）。会话 ↔ 主树是 1:1，池中的 Graft 树是同一实体的另一种 `kind`。

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `tree_id` | String | 主键；主树与池中 Graft 树同级 |
| `session_id` | String | 所属会话锚（= chat 的 session id） |
| `kind` | `TreeKind { Session, Graft }` | `Graft` = 被剥离的子树 |
| `root` | NodeId | 虚拟根 `"root"`，不对应消息 |
| `nodes` | `BTreeMap<NodeId, Node>` | 按 id 有序，序列化稳定 |
| `active_leaf` | Option<NodeId> | 默认头部；空 = 空会话 |
| `next_ordinal` | u64 | 兄弟序分配器 |
| `version` | u64 | 每次变更 +1 |
| `origin` | Option<GraftOrigin> | 仅 `Graft`：来源会话 / 树 / 节点 / 剥离时间 / `prefix_segments` |
| `pool` | Vec<PoolEntry> | 仅 `Session`：本会话剥出去的 Graft 树索引 |
| `profile_id` | Option<String> | 会话默认模型 |
| `schema_version` | u16 | 文档版本，便于迁移 |
| `created_at` / `updated_at` | i64 | ms |

### 4.3 节点 `Node`

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | NodeId | ULID，独立于消息 id（改内容后身份不变） |
| `parent` | NodeId | 父节点；根的直系子父为 `"root"` |
| `ordinal` | u64 | 兄弟序；**不参与摘要** |
| `kind` | `NodeKind { Turn, Premise }` | `Premise` = 纯前提注入，无回复 |
| `context` | Vec<MessageRef> | 本轮注入的动态上下文（提及回执等），先于用户消息进路径 |
| `user` | MessageRef | 用户消息（`Premise` 亦为一条 `user`/`system` 文本） |
| `superseded_user` | Vec<MessageRef> | 被原位改掉的旧用户消息（留档，只读） |
| `reply` | Option<Reply> | 回复；`Premise` 恒为 `None` |
| `label` | Option<String> | 分支标签（F6） |
| `deleted_at` | Option<i64> | 软删（F32） |
| `base_version` | u64 | 最近一次使 `base` 变化的树版本；用于"自 vN 起过期"与定位 `culprit`（不是摘要本身，摘要不落库） |
| `created_at` / `updated_at` | i64 | |

`MessageRef` 带哈希，这是本设计的关键：

```rust
struct MessageRef {
    id: String,     // 消息存储主键
    hash: String,   // blake3(canonical_json(message) minus id/timestamp)，截断 128bit hex
}
```

**为什么要冗余一份哈希**：树快照要给每个节点算脏/干净，而消息正文在另一张表里。若每次都去读全部消息，2000 节点的会话要拉 4000 条。消息不可变（§2.2 不变量 6），内容哈希写时算一次、永久有效，之后摘要计算**只读树文档、零 IO**。

### 4.4 回复 `Reply`

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `message` | MessageRef | 当前生效的回复消息 |
| `trace` | Vec<MessageRef> | 工具轨迹（`assistant.tool_calls` / `tool` 结果），有序 |
| `state` | `ReplyState` | 见 §4.5 |
| `origin` | `ReplyOrigin { Model, Human }` | 人工改写后为 `Human` |
| `based_on` | String | **生成基准摘要**（= 生成时的 `base(node)`） |
| `based_on_version` | u64 | 生成时的树版本，用于 `stale_since` 展示 |
| `params` | `ParamsFingerprint` | profile id + 影响输出的参数 + 工具白名单；只用于 `modelDrift` |
| `acknowledged` | Option<String> | 用户确认过的摘要（F24） |
| `superseded` | Vec<MessageRef> | 被取代的旧回复（重生成 / 改写），只读留档 |
| `edited_at` | Option<i64> | 人工改写时间 |
| `generated_at` | i64 | |

### 4.5 回复状态机

```
Pending ──成功──> Kept ──弃用──> Discarded ──恢复──> Kept
   │                │
   ├──失败──> Failed└──重新生成/改写──> Superseded（旧回复），节点换新回复
   └──取消──> Interrupted
```

| 状态 | 进上下文 | 参与下游摘要 | 可脏 | 说明 |
| --- | --- | --- | --- | --- |
| `Pending` | 否 | 否（按"无回复"折叠） | 否 | 在飞；同会话其余提交被拒 |
| `Kept` | 是 | 是 | **是** | 默认终态 |
| `Discarded` | 否 | 否 | 否 | "决定不保留这条 AI 回复"；恢复后可能立刻变脏 |
| `Interrupted` | 否 | 否 | 否 | 断流 / 取消；部分文本可查看 |
| `Failed` | 否 | 否 | 否 | 无内容；可重试（= 单步） |
| `Superseded` | 否 | 否 | 否 | 留档，只读 |

"参与下游摘要"这一列是脏传播的开关：**任何改变"是否参与"的操作（弃用 / 恢复 / 改正文 / 重生成）都会改下游的 `base`，从而让下游变脏**。

### 4.6 脏 vs 确认（两个正交标记）

| 派生标记 | 计算 | 用途 |
| --- | --- | --- |
| `dirty` | `state == Kept && reply.based_on != base(node)` | 需要重算 |
| `acknowledged` | `reply.acknowledged == Some(base(node))` | 已知过期但用户选择保留；`dirty=false` 且 UI 降级显示 |
| `modelDrift` | `reply.params != 当前会话 params` | 只是提示"这条不是当前模型生成的"，不置脏 |

`acknowledged` 与 `base` 解耦的意义：先确认，之后再改上游，`base` 变了 ⇒ 重新变脏，确认不粘住。

### 4.7 存储注册

沿用既有实体注册：在 `define_storage_entities!` 清单（`crates/infra/wf-storage/src/context.rs:292`）中追加一条（照 `Message` 的形状，`:397-401`）：

```
ConversationTree,
conversation_tree,
"conversation_tree",
ConversationTreeStorage,
EntityIndexes::new(&["sessionId", "kind"], &[], Some("updatedAt")),
```

* 信封定义放 `wf-types`（与 `storage/message.rs` 同风格），适配器放 `wf-storage/src/adapter/conversation_tree.rs`。
* 按 `sessionId` 建索引是为了"列出会话的所有树（主树 + 池）"。
* 单文档体积由节点上限约束（建议 2000 节点 / 深度 512）；节点记录只有几个 32 字节哈希与 id，约 200 B。

### 4.8 写序（崩溃安全）

1. 先写消息（用户 / 上下文 / 回复），拿到 id；**写入时算 `hash`**。
2. 再写树文档（节点入树 / 指针更新 / 摘要更新 / `version+1`）。

崩溃在 1 与 2 之间 ⇒ 孤儿消息，无引用，无害（不影响任何路径）。
崩溃在 2 之后、生成完成前 ⇒ 树里留 `Pending`，加载时按 F40 收敛为 `Interrupted`。

手术只在第 2 步发生（不改消息），所以**手术是单文档原子写**——这是选方案 C 的核心收益。

## 5. 新鲜度模型（内容指纹）

### 5.1 为什么不用布尔传播

直觉做法是"改一个节点，把它所有后代的 `dirty` 置 true"。三个问题：

1. **不可逆**：把改动改回去（改错字后又改回来、手术往返），布尔标记不会自己变干净，用户得手动清。
2. **跨分支误伤 / 漏伤**：哪些后代受影响，取决于路径投影（弃用回复不进上下文、`Premise` 只有输入），布尔传播得把这些规则重写一遍，两处规则必然漂移。
3. **无法解释**："这条为什么脏"只能记个时间戳，说不清是哪次改动。

内容指纹把这三件事一次解决：脏是**算出来的**，不是**记下来的**。

### 5.2 摘要定义

把路径看作**段序列**，每个节点贡献一段：

```
segment(n) = ( ctx_hashes(n), user_hash(n), reply_part(n) )

ctx_hashes(n) = [ hash(m) for m in n.context ]
user_hash(n)  = hash(n.user)
reply_part(n) = if 回复参与下游（state == Kept）:
                    ("r", hash(reply.message), [hash(t) for t in reply.trace])
                else:
                    ("r", "-")

fold(H, s) = blake3("seg\x1f" || H || "\x1f" || ctx_hashes || "\x1f" || user_hash || "\x1f" || reply_part)

base(n)     = fold( in_digest(parent(n)), segment(n) 去掉 reply_part )   // 回复的生成基准
in_digest(n)= fold( in_digest(parent(n)), segment(n) )                   // 子节点的上游摘要
in_digest(root) = H0 = blake3("wf.cvtree.v1:root")
```

于是：

* `base(n)` = 折叠到"本轮用户消息为止"的摘要 ⇒ 回复应当基于它生成。
* `in_digest(n)` = 再折叠本轮回复 ⇒ 子节点的上游摘要。
* `dirty(n) ⇔ reply.state == Kept && reply.based_on != base(n)`。

**`hash(m)` 的输入**：`blake3(canonical_json(message) 去掉 id / timestamp / 用量类 metadata)`，截断为 128bit hex。

* 剔除 `id` 与 `timestamp` 是为了满足不变量 8：换个 id 写同样内容的消息（手术往返、嫁接搬运）摘要必须不变。
* 剔除用量类 metadata（`usage` / `cost` / 计时）是因为它们不参与回放；否则"补写用量"会凭空让整棵子树变脏。
* 其余 metadata（含提及回执的 `type` 标记）**进**哈希：保守取向，宁可多脏，不可少脏。
* 语义上 `hash` 覆盖的就是"会被回放给模型的内容"，`thinking` / `thinking_signature` 自然在内——若某 profile 不回放 thinking，代价是"thinking 变了也判脏"，属于安全方向的误判。

### 5.3 为什么"改写前继 ⇒ 下游全脏"自然成立

`base(n)` 是继承链上的折叠。改动任意祖先的 `ctx / user / reply_part` ⇒ 该祖先的 `in_digest` 变 ⇒ 一切后代 `base` 变 ⇒ 所有 `Kept` 后代 `based_on != base` ⇒ 脏。不需要任何传播代码，拓扑上必然。

同理，"移除前继（上提）"后前缀少一段，"增加前继"后前缀多一段，都是折叠值变化。

### 5.4 手术往返自动恢复干净

摘要只依赖"路径内容的有序序列"，与节点 id、`ordinal`、消息 id、时间戳无关（不变量 8）。因此：

```
把 N 上提（splice_out）→ 把 N 原位挂回（insert_before）→ 折叠序列与原来逐段相同 → 摘要相同 → 全部恢复干净
```

这是内容指纹相对布尔标记最实用的收益：手术敢试，试错可退。也正因为如此，`ordinal` **绝不能**进摘要。

### 5.5 计算成本与存放位置

| 项 | 做法 |
| --- | --- |
| 摘要是否落库 | **不落库**。树文档只存每段的 `hash`；`base` / `in_digest` 在实例加载时单趟折叠重建（O(n)，零 IO） |
| 变更时 | 只重算受影响子树（新父的子树的根以下），O(subtree) |
| 读快照时 | 直接从内存 `DigestIndex` 读，O(1) 取 `dirty` |
| 哈希算法 | `blake3`（`crates/engine/wf-execution-shared/Cargo.toml:31` 已依赖；`wf-types` 无哈希依赖，所以折叠逻辑放 `wf-execution-shared`，`wf-types` 只存字符串） |

不落库的理由：摘要一旦落库就会与内容漂移（改哈希输入、改折叠规则、迁移脚本漏算），而重建成本是线性的、无 IO 的。宁可每次算，也不要存一份可能过期的真相。

### 5.6 模型 / 参数漂移单列

`ParamsFingerprint`（profile id + temperature 等 + 工具白名单）**不进摘要**。若进了，切一次模型整棵树立刻全脏，而"脏"的本意是"历史内容变了"，不是"换了个模型"。所以：

* 换模型 ⇒ `modelDrift` 提示（"这条由 `gpt-4o` 生成"），不拦、不脏。
* 若用户真想要"按新模型全部重算"，用 `forward(max_steps=全部)` 或 `regenerate_all`，是他主动发起，而不是系统替他判定。

### 5.7 走查：不成熟前提的替换与步进

初始树（`#xx` 为摘要短号；`in` = `in_digest`，`base` = 该回复的生成基准）：

```
root ── N1(Premise「项目用 Vim 编辑」)   in=#a1
        └─ N2(Q「推荐插件」)  R2 Kept(base=#a2, in=#a3)
           └─ N3(Q「怎么配 lint」) R3 Kept(base=#a4, in=#a5)
```

用户发现前提错了（其实用 VSCode）。执行 `POST /nodes/N1/content`（原位改前提文本）——注意 `base(N2) = fold(in_digest(N1), …)`，改 N1 的文本只动 `in_digest(N1)`：

| 步 | 动作 | `in_digest(N1)` | `base(N2)` | N2.dirty | N3.dirty |
| --- | --- | --- | --- | --- | --- |
| 0 | 改前 | `#a1` | `#a2` | false | false |
| 1 | N1 文本替换 | `#a1'` | `#a2'` | **true** | **true** |
| 2 | `step forward` 第 1 步：重算 N2 → R2' | `#a1'` | `#a2'`（不变） | false | **true** |
| 3 | `step forward` 第 2 步：重算 N3 → R3' | `#a1'` | `#a2'` | false | false |

（`base(N2)` 在第 2 步不变，是因为改的是 N1；变的是 `in_digest(N2)` = `#a3'`，它才是 `base(N3)` 的输入。）

若用户只想先看一眼第一步（`mode=single`），停在步 2：N2 干净、N3 仍脏，UI 显示"N3 有 1 条待重算"。这就是"单步"的用途。

再叠加一次"移除前继"：`splice_out(N1)` ⇒ N2 上提到 `root`，前缀少一段 ⇒ N2 / N3 重新变脏；随后 `insert_before(N2, N1)` 挂回 ⇒ 段序列复原 ⇒ 两者立刻恢复干净（§5.4）。整个往返不需要任何"清理脏标记"的动作。

## 6. 树手术

### 6.1 五个动词与它们的组合

| 动词 | 语义 | 端点 | 摘要后果 |
| --- | --- | --- | --- |
| `insert_before(target, new)` | 在 `target` 与其父之间插入 `new` | `POST /nodes/{target}/insert-before` | `target` 子树全脏（前缀变长） |
| `splice_out(node)` | `node` 的子节点上提到 `node.parent` | `POST /nodes/{node}/splice` | 上提子树全脏（前缀变短） |
| `reparent(node, new_parent)` | 改父（同树内搬运的最小形式） | `POST /nodes/{node}/reparent` | 子树全脏（前缀不同） |
| `detach_subtree(node)` | 子树切成 Graft 树进池 | `POST /sessions/{id}/detach` | 主树侧该分支消失；Graft 侧按 `carry_prefix` 决定 |
| `attach_subtree(src, new_parent)` | 子树挂到 `new_parent` 下 | `POST /sessions/{id}/attach` | 被搬子树全脏（除非前缀折叠值恰好相同） |

**其余操作都是组合**：

| 需求 | 组合 |
| --- | --- |
| 替换前继 | `attach(mode=replace, target=旧前继的父, replace=旧前继)` = `detach(旧前继)` + `attach(新子树)` |
| 增加前提（不生成） | `insert_before(target, Premise)` |
| 增加前提（并立即重算） | `insert_before` + `step(single, target)` |
| 改问法但保留旧答案 | 分支改（§6.7） |
| 改问法并让下游重算 | 原位改（§6.7） |

### 6.2 `insert_before`（增加前继）

输入：`target` + 内容（文本 / 图片）+ `kind`（默认 `Turn`；`Premise` 不触发生成）。

动作：

1. 新建节点 `P`，`P.parent = target.parent`，`ordinal = next_ordinal++`，`P.reply = None`（若 `kind=Premise`）或 `Pending`（若 `kind=Turn` 且 `generate=true`）。
2. `target.parent = P`（**`target` 的子树整体跟随**，不需要逐节点改）。
3. 折叠 `P` 的 `in_digest`（`Premise` 的 `reply_part` 记作 `-`，与"无回复"同形）；`target` 的 `base` 随之变 ⇒ `target` 子树全脏。
4. `active_leaf` 若在 `target` 子树内，保持不变（路径仍然通）。

注意 `mode=siblings`（把同父下 `target` 之后的所有兄弟都挂到 `P` 下）只作为可选参数，默认不做——多数场景"前提只约束这一条链"。

### 6.3 `splice_out`（移除前继）

输入：`node` + `mode`（默认 `lift`）。

动作：

1. 取 `node` 的子节点（按 `ordinal` 升序），逐个把 `parent` 改为 `node.parent`，并重新分配 `ordinal`（追加到目标父的末尾或紧随 `node` 的序位，两者都不影响摘要）。
2. `node` 变成无子节点，仍在树上——它是"曾被采用但现在不进入任何路径"的一轮，UI 折叠显示。
3. `node` 自身的回复**不改状态**（仍是 `Kept`），但它已不在任何路径上，`dirty` 对它无意义（不进上下文就不需要重算）。
4. `active_leaf` 若 == `node`，上提到 `node.parent`。

可逆性：`insert_before(任一被上提的子节点, node)` 可以把 `node` 挂回该子节点之上；若当初有多个子节点，只有被指定的那条链恢复，其余保持上提状态——部分可逆，文档明说。

### 6.3b `reparent`（改父，同树内搬运的特例）

`reparent` 就是 `attach` 的同树内特例：只搬一棵子树、不跨树、不进池、不动物理文档，只把 `parent` 指针与 `ordinal` 改掉，再折叠被搬子树。单独留一个端点是因为它是 UI 拖拽的高频动作，语义比"通用嫁接"窄，报错也更好写（不允许跨会话、不允许 `mode=replace`）。

判定是否真的产生脏：搬家后按新父折叠，若新前缀折叠值恰好等于旧值（例如把节点搬到与原父摘要相同的另一父下），则保持干净——这不是特判，还是同一套判定。

### 6.4 替换前继

两条路径，按"要不要保留旧内容"选：

| 场景 | 做法 | 结果 |
| --- | --- | --- |
| 换掉内容，旧内容不必再挂在树上 | `POST /nodes/{old}/content`（原位改） | 旧正文进 `superseded`，`old` 位置不变，下游全脏 |
| 换成另一棵已有子树 | `POST /attach {mode:"replace", old, src}` | `old` 自动剥离到池（可找回），`src` 子树挂到 `old.parent` 下，下游全脏 |

### 6.5 `detach`（剥离 = 子树嫁接的第一个半程）

输入：`node` + `carry_prefix: bool`（默认 `true`）+ `label`。

动作：

1. 新建 `Graft` 树文档，`tree_id` 新分配；把 `node` 及其全部后代搬入（从主树删除这些节点条目）。
2. `carry_prefix=true`：把 `node` 的祖先链**按段序列**写入 `origin.prefix_segments`（每段只有 `ctx_hashes / user_hash / reply_part`，几条哈希，不拷消息）。
3. `carry_prefix=false`：不写前缀；Graft 树从 `H0` 起折叠 ⇒ 整棵子树立刻全脏。
4. 主树侧：`node.parent` 少一个子；`active_leaf` 若在 `node` 子树内 ⇒ 回退到 `node.parent`。
5. 主树 `pool` 追加 `PoolEntry { graft_id, label, node_count, detached_at, origin_node }`。

**关键点**：`carry_prefix=true` 时 Graft 树根的 `in_digest` = 折叠物化后的段序列 = 原位置的 `in_digest(node.parent)` ⇒ 逐段相同 ⇒ **整棵子树保持干净**。不是特判，是折叠函数的结合性。

### 6.6 `attach`（嫁接 = 第二个半程）

输入：`src`（`graft_id` 或同树内的 `node_id` 或另一会话的 `graft_id`）+ `new_parent` + `mode {link, replace}`。

前置校验（F15）：

| 校验 | 失败 |
| --- | --- |
| `new_parent` 存在且未软删 | 422 |
| `new_parent` 不在被搬子树的闭包内（防环） | 422 |
| 合并后节点数 ≤ 上限、深度 ≤ 上限 | 422 |
| `src.kind == Graft` 且 `src.session_id` 兼容（跨会话时校验 profile 兼容性，不兼容仅告警） | 422 / 警告 |
| `mode=replace` ⇒ 被替换节点允许被剥离 | 409 |

动作：

1. `mode=replace`：先对 `old` 做一次内联 `detach`（进池，可找回）。
2. 把 `src` 的节点并入目标树（`tree_id` 统一，节点 id 保持），`src_root.parent = new_parent`，`ordinal = next_ordinal++`。
3. 折叠被搬子树：从 `in_digest(new_parent)` 开始逐段重算 ⇒ `base` 依次更新 ⇒ `based_on` 不等的回复置脏。
4. 若 `src` 是池中 Graft 树 ⇒ 删除该 Graft 文档与 `pool` 条目（搬走了）。
5. 返回 `moved_nodes` / `dirty_count` / 新 `version`。

**跨会话嫁接必然全脏**：目标前缀与来源前缀几乎不可能折叠出同一个摘要。这不是缺陷——把一段对话搬到另一段历史下面，原本的答案当然要重算。用户随后用 `forward` 一次跑完即可。

### 6.7 原位改 vs 分支改（本设计最关键的分野）

| | 原位改 `POST /nodes/{id}/content` | 分支改 `POST /nodes/{id}/edit` |
| --- | --- | --- |
| 树形状 | 不变，`id` 不变 | 父节点下新增兄弟节点 |
| 旧用户消息 | 进 `node.superseded_user` | 原地不动 |
| 旧回复 | 进 `reply.superseded`，节点回 `Pending` | 原地不动，整条旧分支自洽 |
| 下游 | **全脏** | **不脏**（旧分支的历史没变） |
| 解决的问题 | "这个前提是错的，别让它继续污染后面所有回答" | "同一个问题换个问法，两套答案都留着对比" |
| 类比 | `git commit --amend` + 后续全部 rebase | `git branch 试一下` |

两者都 P0，UI 上必须**显式二选一**，不要藏在一个"编辑"按钮后面：默认给分支改（安全），Advanced 菜单给原位改（连带重算）。这条列为开放点 O2。

### 6.8 拒绝清单（宁可报错，不要静默降级）

1. `Premise` 节点要求生成回复 ⇒ 422（前提是输入，不是一轮问答）。
2. `attach` 形成环 / 目标在被搬子树内 ⇒ 422。
3. 超过节点数或深度上限 ⇒ 422，附当前计数与上限。
4. `splice_out` 一个 `root` 的直系子节点且它没有子节点 ⇒ 允许（等价于"把唯一一问摘掉，会话回到空前缀"）。
5. 手术期间会话有在飞轮 ⇒ 手术本身允许（只改结构），但**顺序步进**要求无在飞轮，否则 409。
6. `mode=replace` 但 `old` 是被软删节点 ⇒ 409。

### 6.9 可逆性与撤销

每个手术记一条 `OpRecord`（环形缓冲，上限 200 条）到树文档：`{op, params, version_before, version_after}`。逆操作：

| 正向 | 逆向 |
| --- | --- |
| `insert_before(target, P)` | `splice_out(P)` |
| `splice_out(N)` | `insert_before(每个原子里节点, N)` |
| `detach(N)` | `attach(graft_id, 原父)` |
| `attach(src, parent)` | `detach` 回池（或 `reparent` 回原位） |
| 原位改 / 改正文 | 指针换回 `superseded` 里的那个 `MessageRef` |
| 弃用 | 恢复 |

因为消息不可变、摘要按内容算，**逆操作不需要重算任何东西就能恢复"干净"**（§5.4）。`undo` 端点列为 P2，但 `OpRecord` 在 P0 就写，避免将来补不了历史。

## 7. 上下文装配与保留

### 7.1 路径投影

从目标节点沿 `parent` 上溯到 `root`，反转为节点链，然后逐节点按顺序投影：

| 节点情况 | 投影内容 |
| --- | --- |
| `Premise` | `context` → `user`（就这些，无回复） |
| `Turn`，`reply.state == Kept` | `context` → `user` → `trace`（原序） → `reply.message` |
| `Turn`，其它状态 | `context` → `user`（跳过回复与轨迹） |
| 任意，`deleted_at` 非空 | **该节点自身跳过**，其子树节点各自判断（软删一个中间节点不应连带删掉挂在它下面的分支） |

关键推论：**非当前路径的分支天然不进上下文**。"把某条分支挂到实例上 / 从上下文摘掉"在本模型下就是"切换 `active_leaf`"或"手术"。这也是"嫁接 vs 观察者"两种 attach 语义曾经纠缠的根源——现在二者已分家：切换头部不叫 attach，叫 `POST /leaf`。

### 7.2 脏回复是否进上下文

默认**进**（否则整条链断掉，用户没法继续对话），但：

* 快照里如实标注 `dirty: true`；
* 提交新轮时附带 `dirty_count` 警告；
* `block_on_dirty = true`（会话级开关，默认 false）时直接 409 + 脏清单。

理由：脏只意味着"答案可能不是按最新历史生成的"，不意味着"内容不可用"。把它当作提示而不是门禁，符合"探索型对话"的实际使用方式。

### 7.3 保留（弃用）语义

* 弃用不改变树形状：子节点仍挂在原节点下，路径从该节点的用户消息继续。
* 弃用会让**下游变脏**（因为 `reply_part` 从 `("r", hash, ...)` 变成 `("r", "-")`，折叠值变）——这正是"把某条不成熟的 AI 回复从历史里摘掉，后面的回答得重来"。
* 恢复同样改折叠值 ⇒ 再脏一次（或恢复原状 ⇒ 自动干净）。
* 弃用状态不计入 token 预算。

### 7.4 token 与用量

当前 chat 不设预算（`ConversationSession::new()` → `token_limit = 0`），也不持久化用量。树会话：

* 预算按**路径**估（不是整棵树），`Premise` 计入。
* 每轮用量写进回复消息 `metadata`，会话级用量是读时求和；`forward` 的每步独立记账，`metadata.step_of` 关联到触发它的版本。用量类 metadata 不进 `hash`（§5.2），所以后补用量不会让下游变脏。
* 不改 `AgentExecution` 结构。

### 7.5 只读工具轮（查询型 MCP）

场景里明确要"部分查询型 MCP"，所以保留一个**极窄**的工具通道：

| 约束 | 取值 |
| --- | --- |
| 生成次数 | 最多 2 次（第 1 次可带工具，第 2 次强制无工具） |
| 工具往返 | 最多 1 次 |
| 单轮调用数 | ≤ 4（沿用 `MAX_SINGLE_SHOT_TOOL_CALLS`，`crates/engine/wf-execution-shared/src/single_shot.rs:20`） |
| 白名单外 | 声明与执行两侧都拒（同 `single_shot.rs:105-109`） |
| 检查点 / 审批 / 工作区写入 | **无**（不建 checkpoint 会话，不产生文件观察项） |
| 轨迹归属 | 作为节点 `reply.trace`，不成为树节点 |

不能直接复用现有两件：`ChatSession` 结构上无 `tools` 字段（`crates/engine/wf-execution-shared/src/chat.rs:98`）；`generate_with_tools_once` 要求模型必须返回工具调用否则报错（`single_shot.rs:81-86`），且缺"工具后二次生成"。所以新增一个窄驱动，`ChatSession` 保持原样。

白名单来源：会话级 `allowed_tools: Vec<String>`（可被单轮覆盖，默认空 = 纯文本）。现状没有只读能力位（`ToolType` 只有 `Stateless / Stateful / Rest / BuiltIn / Mcp`，`crates/foundation/wf-types/src/tool/state.rs:5-12`），MCP 的 `readOnlyHint` 未被采集，所以第一版按名称白名单，能力位自动判定列 P2。

## 8. 步进重算

### 8.1 脏清单

`GET /chat/sessions/{id}/dirty?leaf=`：

```json
{
  "version": 42,
  "leaf": "n3",
  "nodes": [
    { "node_id": "n2", "index": 1, "state": "kept", "based_on_version": 30, "stale_since_version": 41, "culprit": "n1" },
    { "node_id": "n3", "index": 2, "state": "kept", "based_on_version": 30, "stale_since_version": 41, "culprit": "n1" }
  ],
  "total": 2,
  "other_branches_dirty": 3
}
```

* `index` = 该节点在路径上的序号（1 起，跳过 `Premise`），UI 可以直接画"第 2、3 条待重算"。
* `culprit` = 最深的、`base_version` 等于该节点 `stale_since_version` 的祖先，用来说明"是因为改了 N1"。
* `other_branches_dirty` = 非当前路径上的脏节点计数（只统计，不参与步进）。
* 全部按 `leaf` 指定路径算，这就是"特定路径的脏状态"。

### 8.2 单步

`POST /chat/sessions/{id}/step` body `{ "mode": "single", "leaf": "n3", "node_id": "n2" }`

1. 校验：无在飞、`node.reply.state == Kept` 且确实脏（不脏则 200 + `noop`）。
2. 旧回复 → `superseded`；节点 `reply = Pending`；置在飞。
3. 以当前 `base(node)` 为基准装配路径并存下 `based_on`，生成。
4. 收敛：写回复消息 + `hash` → 树指针 + `based_on` + `base_version = version+1` → 事件。
5. 下游**通常**仍脏（`in_digest(node)` 变了）；除非新回复内容与原回复逐字节相同。这就是单步的设计目的：**看一步，确认再继续**。

`node_id` 省略时等价于"路径上第一条脏节点"。`policy=keep_human`（默认开）时，若该节点 `origin == Human`，单步拒绝（409），避免覆盖人工内容。

### 8.3 顺序步进

`POST /chat/sessions/{id}/step` body `{ "mode": "forward", "leaf": "n3", "max_steps": 8, "stop_at": null }`

```
plan = 路径上所有脏节点（按路径序，开始时快照一次）
for node in plan:
    if steps >= max_steps:            stop(reason=max_steps)
    if node.id == stop_at:            stop(reason=stop_at)      // 到达但不重算
    if not dirty(node):               skip(node); continue      // 已被上游重算顺手改干净
    重算 node（同 8.2 的第 2-4 步）
    steps += 1; progress(steps, total)
    if 取消:                          stop(reason=cancelled)
    if 失败:                          stop(reason=failed, node)
stop(reason=done)
```

性质：

* **计划顺序固定、脏判定每步重做**：`plan` 只在开始时算一次（避免"重算导致脏集合抖动"引起死循环），但每一步执行前重新判 `dirty`。所以上游重算若顺手把下游改干净，下游会被跳过而不再烧一次生成。
* **自然收敛**：重算 N 后若新回复内容与原回复逐字节相同，`in_digest(N)` 不变 ⇒ N 的下游全部不脏 ⇒ 循环空转结束。这是内容指纹给的免费优化，布尔传播做不到。
* **一次 forward 占一个在飞槽**（与提交轮互斥，F37），可取消（F38），取消后已完成步保留 `Kept`，未做步仍脏。
* 默认 `max_steps = 8`：一次点"顺序重算"通常是改了个前提、下游两三条，8 留足余量又不会失控。

### 8.4 失败 / 中断 / 取消

| 情况 | 处理 |
| --- | --- |
| 单步生成失败 | 节点 `Failed`，`forward` 立即停止并返回失败节点；剩余仍脏，可重试 |
| 单步被取消 | 节点 `Interrupted`（部分文本保留）；`forward` 停止，`reason=cancelled` |
| 进程崩溃 | 加载时 `Pending` → `Interrupted`（F40）；脏清单照常给出，用户可继续步进 |
| 工具轮越界 | 硬错误，整步失败（宁可失败也不静默降级成无工具对话） |

失败**不**写 `based_on`：因为没产出可用内容，节点保持"脏"（`Failed` 状态本身不进上下文，但重试时仍需按当前 `base` 生成）。

### 8.5 事件

| 帧 | 关键字段 |
| --- | --- |
| `step.started` | `planLength, leaf, version` |
| `turn.*` | 复用轮事件（每步一组） |
| `step.progress` | `done, total, nodeId, nextNodeId` |
| `step.completed` | `steps, remainingDirty` |
| `step.stopped` | `reason, steps, remainingDirty, nodeId?` |

### 8.6 与提交轮的关系

* 共用同一个在飞槽 ⇒ 提交轮与 `forward` 不能并行（避免两条写路径同时改树）。
* 提交轮不要求路径干净（§7.2）；若用户执意在脏路径上继续追问，新轮自身的 `based_on` 就是当前 `base`，它是干净的，脏的仍是上游那几条。
* `forward` 结束后 `active_leaf` 不变。

## 9. 执行实例与流

### 9.1 为什么必须把"提交"和"订阅"分开

现状是提交即订阅（`POST /chat/stream` 返回 SSE 并 `with_task`），而 `Drop` 会 abort 驱动（`crates/app/wf-api/src/infra/stream.rs:238-244`）。要让"断连续跑、重连续看"成立，驱动任务必须由实例持有、由注册表管理，与任何连接的生存期解耦——这正是 agent loop 已有的 dispatch / attach 拆分（dispatch `crates/app/wf-server/src/api/agent/loops/detached_handlers.rs:76`，attach `:177-250`）。**注意**：这一节的 `attach` 是历史命名，本方案一律称"订阅"。

### 9.2 会话实例

`ConversationSessionHandle`（`wf-api` 内，按会话 id 惰性创建）：

| 字段 | 作用 |
| --- | --- |
| `session_id` / `directory` / `profile_id` | 身份与归属 |
| `tree: RwLock<ConversationTree>` | 主树（含池索引） |
| `digests: RwLock<DigestIndex>` | 每节点的 `base` / `in_digest` / `dirty` 派生缓存 |
| `write_lock: tokio::Mutex<()>` | 手术 + 提交 + 步进的串行化 |
| `inflight: RwLock<Option<InflightJob>>` | 在飞轮或 `forward` 作业：`turn_id`、`node_id`、累计文本、`CancellationToken` |
| `subscribers: AtomicUsize` | 观察者计数（空闲回收判定） |
| `last_event_ms: AtomicI64` | 最近事件时间 |

`ConversationSessionRegistry`：`DashMap<String, Arc<Handle>>` + 每主体配额（复用 `DetachedRunRegistry` 的记账方式，`crates/app/wf-server/src/detached.rs:142`）。

不复用 `AgentLoopRegistry`（`crates/engine/wf-agent/src/registry.rs:56-76`）：它的实体带迭代计数、容量许可、结果槽与层级管理器，chat 一个都不需要，`pause/resume` 对单次生成也没有意义。

### 9.3 一轮的生命周期

1. **提交**：校验 profile / 父节点 / 空闲 / 工具白名单 → 写上下文消息与用户消息（带 `hash`）→ 写树（节点入树，`reply = Pending`，`version+1`）→ 置 `inflight` → abort 句柄登记进 `ExecutionTaskRegistry`（`crates/app/wf-api/src/infra/tasks.rs:18`，键 `turn_id`）→ 返回 `turn_id / node_id / version`。
2. **生成**：装配路径（§7.1）→ 一次生成（可带工具轮，§7.5）→ 增量发布到总线。
3. **收敛**：写回复消息（含轨迹，带 `hash`）→ 写树（`Kept` / `Failed` / `Interrupted`，记 `based_on = base(node)`）→ 折叠该节点 `in_digest`（下游 `dirty` 自动重算）→ 清 `inflight` → 终态帧 → 写会话记录（沿用 `crates/app/wf-api/src/chat.rs:347-408` 的字段口径）。
4. **取消**：令牌触发 → 生成中断 → 部分文本作为 `Interrupted` 落库（可为空）。

### 9.4 订阅 / 退订

* **订阅** = `GET /chat/sessions/{id}/stream`。顺序：`session.snapshot`（`version`、`active_leaf`、`inflight`、脏汇总）→ 游标回放 → 实时帧。
* 回放复用现成设施：`events::history` 按 `execution_id` 查最近事件 + `events::subscribe` + `SubscriberGuard`（`crates/app/wf-server/src/api/agent/loops/detached_handlers.rs:177-250`，回放窗口 500 / 取 200）。
* 回放与订阅之间的空窗用 `version` 兜底：客户端发现 `tree.changed` 的 `version` 跳号即重取快照，不依赖帧完整性。
* **退订** = 断流。驱动在实例里，不随流存活；`SubscriberGuard` 记账，空闲回收时"无订阅者 + 无在飞 + 超阈值"即从内存淘汰（树已落库，下次访问重载），口径同 `StreamRuntime`（`crates/app/wf-server/src/detached.rs:36-60`）。
* **多观察者**：每个订阅各自向总线订阅（按 `execution_id` 过滤），广播是天然的。

### 9.5 崩溃与重启

* 加载时 `inflight` 为空但树里存在 `Pending` ⇒ 置 `Interrupted` 并写回。
* 在飞累计文本不落库，重启后以最后写入的回复消息为准（通常为空），UI 呈现"被打断，可重算"。
* `DigestIndex` 每次加载重建（§5.5），所以重启后脏判定与崩溃前完全一致——这也是不落库的另一个好处。

### 9.6 与线性 `/chat/stream` 的关系

线性端点保留原语义，调用方零改动。树会话走新端点族；两者共用消息存储与会话记录口径，所以列表、历史、标题、统计都不需要分叉。线性会话被首次访问树端点时按时间戳合成退化树（F4，只读、全干净），首次手术时才落盘。

## 10. 接口与客户端

### 10.1 HTTP 端点（`/api/v1/chat/sessions`）

| 方法 | 路径 | 作用 | 返回 |
| --- | --- | --- | --- |
| POST | `/chat/sessions` | 创建会话 | `{session_id, tree_id, version}` |
| GET | `/chat/sessions/{id}` | 树快照（F2，含 `dirtyCount`） | `{version, activeLeaf, inflight, nodes[], pool[]}` |
| GET | `/chat/sessions/{id}/path?leaf=` | 路径消息（F3） | `[{message, nodeId, retained, dirty}]` |
| GET | `/chat/sessions/{id}/dirty?leaf=` | 脏清单（§8.1） | 见 8.1 |
| POST | `/chat/sessions/{id}/turns` | 发起一轮（F34） | 202 `{turnId, nodeId, version}` |
| POST | `/chat/sessions/{id}/turns/{turnId}/cancel` | 取消 | 200 |
| GET | `/chat/sessions/{id}/stream?since=` | 订阅（F35） | SSE |
| POST | `/chat/sessions/{id}/step` | 单步 / 顺序步进（F19/F20） | 202 `{jobId, planLength}` |
| POST | `/chat/sessions/{id}/leaf` | 切换头部 | 200 `{version}` |
| POST | `/chat/sessions/{id}/nodes/{n}/insert-before` | 增前继（F7） | 202 `{nodeId, dirtyCount}` |
| POST | `/chat/sessions/{id}/nodes/{n}/splice` | 移除前继（F8） | 200 `{dirtyCount}` |
| POST | `/chat/sessions/{id}/nodes/{n}/reparent` | 改父（§6.3b） | 200 `{dirtyCount}` |
| POST | `/chat/sessions/{id}/nodes/{n}/content` | 原位改用户消息（F27） | 202 `{turnId, dirtyCount}` |
| POST | `/chat/sessions/{id}/nodes/{n}/edit` | 分支改用户消息（F28） | 202 `{nodeId, turnId}` |
| POST | `/chat/sessions/{id}/nodes/{n}/reply` | 改写助手正文（F29） | 200 `{version, dirtyCount}` |
| POST | `/chat/sessions/{id}/nodes/{n}/reply/discard` | 弃用（F30） | 200 `{version, dirtyCount}` |
| POST | `/chat/sessions/{id}/nodes/{n}/reply/restore` | 恢复 | 200 `{version, dirtyCount}` |
| POST | `/chat/sessions/{id}/nodes/{n}/acknowledge` | 确认过期（F24） | 200 `{version}` |
| POST | `/chat/sessions/{id}/detach` | 剥离子树（F10） | 201 `{graftId, dirtyCount}` |
| POST | `/chat/sessions/{id}/attach` | 嫁接子树（F11） | 200 `{movedNodes, dirtyCount, version}` |
| GET | `/chat/sessions/{id}/grafts` | 暂存区（F13） | `[{graftId, label, nodeCount, detachedAt}]` |
| DELETE | `/chat/sessions/{id}/nodes/{n}` | 软删节点及其整棵子树（F32） | 200 `{version, dirty}` |
| DELETE | `/chat/sessions/{id}` | 删除会话（树 + 池 + 消息级联） | 200 |

约束：所有端点先校验会话属于 `ExecutionType::Chat`，否则 400；所有变更端点接受可选 `if_version`；所有手术端点返回 `dirtyCount`，让 UI 立刻知道"这次改动要重算几条"。

**实现位置**：`crates/app/wf-server/src/api/conversation.rs`（24 个端点全部实现并挂载，见 §11.0）。处理器已登记 utoipa，`openapi.json` 与 `schema.d.ts` 已同步。

### 10.2 SSE 帧

| 帧 | 关键字段 | 说明 |
| --- | --- | --- |
| `session.snapshot` | `version, activeLeaf, inflight{turnId,nodeId,text}, dirtyCount` | 订阅首帧，权威快照 |
| `tree.changed` | `version, nodeId, change` | 结构变更（手术 / 编辑 / 弃用 / 叶子切换） |
| `dirty.changed` | `version, total, nodes[]` | 脏集合变化（手术或重算后） |
| `turn.started` | `turnId, nodeId, parent` | 一轮开始 |
| `turn.delta` / `delta` | `turnId, content` | 复用现有增量帧名，前端累加器无需改 |
| `turn.reasoning` | `turnId, content` | 推理增量 |
| `turn.usage` / `usage` | 用量字段 | 复用现有用量帧 |
| `turn.tool` | `turnId, toolCallId, name, phase` | 工具往返（P1） |
| `turn.completed` | `turnId, nodeId, messageId, usage` | 终态 |
| `turn.failed` / `turn.cancelled` | `turnId, error` | 终态 |
| `step.*` | 见 §8.5 | 步进 |
| `session.error` | `error` | 会话级错误（版本冲突等） |

事件镜像沿用 agent 的做法：驱动把帧映射成 `BaseEvent`（`execution_id = session_id`，`metadata` 携带 `turn_id / node_id / content`）发布到总线（参照 `crates/engine/wf-agent/src/stream.rs:180-221`），持久化桥负责落盘。新增 `EventType`：`ConversationTurnStarted / ConversationTurnCompleted / ConversationTurnFailed / ConversationTreeChanged / ConversationDirtyChanged / ConversationStepStarted / ConversationStepCompleted`；增量复用 `LlmStreamChunk`，用量复用 `LlmStreamDone`。

### 10.3 客户端顺序

```
1. GET  /chat/sessions/{id}/stream      → snapshot（含 dirtyCount）
2. GET  /chat/sessions/{id}/dirty       → 脏清单（画"待重算"徽标）
3. POST /chat/sessions/{id}/turns       → turnId（框内立即显示用户消息）
4. 流上收 turn.*                         → 增量渲染
5. 网络中断 → 重做 1，snapshot.inflight.text 即"已生成的部分"
```

先订阅后提交，保证不漏帧；提交返回后到首帧之间的空窗由 `turn.started` 补齐。

### 10.4 web-app 改动

* `apps/web-app/src/lib/services/streaming.ts`（`streamChat`）拆成 `subscribeChatSession` + `submitChatTurn`；现有增量/用量分支可原样复用。
* 新增树侧栏（`apps/web-app/src/lib/components/chat/` 下，与 `ChatSessionRail.svelte` 并列）：父子关系、当前叶子、`Premise` 图钉、脏徽标、拖拽手术。
* `ChatTranscript.svelte` 按路径渲染，脏回复加"待重算"条带 + `单步 / 顺序重算` 两个按钮。
* `MessageBubble.svelte` 增加节点级动作：编辑（默认分支改）、原位改（高级）、改写正文、弃用 / 恢复、确认过期。
* `routes/chat/+page.svelte` 增加"分支"入口与当前路径指示。

### 10.5 TUI 改动

* `crates/app/cli/wf-tui/src/session_tree.rs:1-9` 的"没有 message 级分支"前提作废，`assemble_tree` 按树结构装配（保留 `SESSION_TREE_NODE_CAP` 截断与最近检查点字段，检查点对树会话恒为空）。
* 只读优先（F46），脏节点用前缀标记（如 `~`）区分于 `*`（当前叶子）。

### 10.6 生成物同步

* `crates/app/wf-server/openapi.json` 重新生成——快照测试会因路由漂移失败。当前新端点未加 utoipa 注解，快照不漂移，但文档缺这 23 条；补注解后再重生成。
* `apps/web-app/src/lib/api/schema.d.ts` 由 `tools/openapi-codegen` 重新生成。
* `docs/app/server/11-chat.md` 端点清单同步（新增 23 个操作，原 `/chat/stream` 保留）。

## 11. 改动清单、实施、测试与待拍板

### 11.0 实施状态

按 §11.3 的 8 步推进，**第 1–6 步与第 8 步的后端/API/前端最小可用部分已落地并通过编译与测试**，第 7 步（只读工具轮）仍未做。

| 步 | 内容 | 状态 | 验证方式 |
| --- | --- | --- | --- |
| 1 | 类型 + 摘要 + 投影 + 不变量（纯函数） | **完成** | `cargo check -p wf-types`、`-p wf-execution-shared` 通过 |
| 2 | 存储实体 + 线性升级 + 树/路径/脏清单 | **完成** | `cargo check -p wf-storage` 通过；adopt 读路径走通 |
| 3 | 会话实例 + 提交/订阅分离 + 取消 + 单飞 | **完成** | `conversation::tests` 端到端通过（mock provider） |
| 4 | 内容操作 + 脏传播 | **完成** | 原位改 ⇒ 下游全脏；分支改 ⇒ 原分支不脏 |
| 5 | 步进（single / forward） | **完成** | `single` 后下游仍脏（用例断言脏数 2→1） |
| 6 | 手术（insert / splice / reparent / detach / attach + 池） | **完成** | 往返复原、`carry_prefix` 保新鲜、成环被拒 |
| 7 | 只读工具轮 + 白名单 | **未做** | 保持 `ChatSession` 无工具边界，等待单独的窄驱动设计 |
| 8 | web-app + TUI + codegen + 文档 | **部分完成** | 端点/OpenAPI/TS 类型/文档与最小树栏已落地；TUI 只读树装配已提供入口，深度交互未做 |

测试实测：`cargo test -p wf-execution-shared --lib conversation_tree` → **19 passed**；
`cargo test -p wf-api --lib conversation` → **7 passed**（含 1 个既有 `chat::` 用例）。
编译与测试均在 `devs` 编译服务器执行，日志见 `docs/plan/conversation-branch-tree-remaining-work.md`。

### 11.0b 落地过程中修正的四个实现事实

这四条是写代码时才暴露的，原设计没有写对，已按"修代码 + 记录"处理：

| # | 事实 | 影响 | 处理 |
| --- | --- | --- | --- |
| R1 | `EventType` 的 `as_str()` 是**穷尽 match**（`crates/foundation/wf-types/src/events/base.rs:209` 起） | 只加枚举变体会直接编译失败 | 7 个新变体在同一文件 `:378-384` 补 `as_str` 分支 |
| R2 | `detach` 只把节点移进 Graft 文档，**没有重设子树根的 `parent`** | Graft 里子树的根仍指向留在原树的父节点 ⇒ Graft 不是合法树，`subtree_ids` 数不到它，`attach` 搬 0 个节点 | `surgery.rs` 的 detach 循环里对被剥离的根重设 `parent = graft.root` |
| R3 | 摘要折叠**没有跳过软删节点**，而投影会跳过 | 删节点后投影少了一段、摘要没变 ⇒ "删了但不脏"，脏判定与真实上下文脱节 | `digest::refold` / `refold_node` 对 `deleted` 节点取透明值（`base = out = 父 out`），删节点因此自然让下游变脏 |
| R4 | `is_dirty` 对"没有回复"的节点返回 `false` | 新插入的前继（turn 无 reply）永远不会被步进生成 | `is_dirty` 改为：无 reply 的 `Turn` 视为待生成，`Premise` 与软删节点不算 |

另有一条与写入顺序有关的实现约束：助手回复可能由 provider 返回**不带 id** 的消息，而树只能指向已落库的消息，所以收敛时必须"先落库拿 id，再改指针"（`conversation/turn.rs` 的 `converge_answer`）。

### 11.1 模块归属与改动清单（以实际落地为准）

| 层 / crate | 文件（已落地） | 改动 |
| --- | --- | --- |
| `wf-types` | 新增 `src/conversation.rs` + `src/conversation/tree.rs` | `ConversationTree` / `Node` / `Reply` / `ReplyState` / `ReplyOrigin` / `NodeKind` / `MessageRef` / `ParamsFingerprint` / `PrefixSegment` / `GraftOrigin` / `PoolEntry` / `OpRecord`；**只存哈希字符串，不做哈希计算** |
| `wf-types` | `src/events/base.rs` | 新增 7 个 `EventType`（Turn started/completed/failed、Tree changed、Dirty changed、Step started/completed）**并补 `as_str` 分支**（见 R1） |
| `wf-types` | `src/lib.rs` | 导出 `ConversationTree` 与节点/回复类型 |
| `wf-execution-shared` | 新增 `src/conversation_tree.rs` + `conversation_tree/{digest,dirty,projection,surgery,invariants,step,adopt,test_support}.rs` | 摘要折叠、脏判定、路径投影、5 个手术动词 + 内容操作 + `delete_subtree`、不变量、步进计划、线性历史升级；`blake3` 已在依赖内 |
| `wf-execution-shared` | `src/chat.rs` | 新增 `continue_turn` / `build_request`：按已存档的 user 消息续生成，不追加新消息（树驱动需要） |
| `wf-storage` | 新增 `src/adapter/conversation_tree.rs` | `ConversationTreeListOptions`（`sessionId` / `kind` 过滤）+ `ConversationTreeStorageAdapter`（`list_by_session` / `load_session_tree`） |
| `wf-storage` | `src/adapter.rs`、`src/adapter/adapter_impls/macros.rs`、`.../adapters.rs` | 模块注册、`make_base_adapter!(ConversationTreeStorage, ...)`、两个读方法的实现 |
| `wf-storage` | `src/entity_impl.rs`、`src/context.rs` | `Entity` 实现（索引字段取自 metadata）+ 表注册（`conversation_tree`，索引 `sessionId`/`kind`，排序 `updatedAt`） |
| `wf-api` | 新增 `src/conversation/{mod,store,runtime,events,query,turn,step,surgery,stream,tests}.rs` | 会话树服务：加载/adopt、会话级写锁与在飞槽、事件发布、快照/路径/脏清单、提交轮与驱动、步进作业、手术与内容操作、订阅（replay + live） |
| `wf-api` | `src/lib.rs` | `pub mod conversation` |
| `wf-server` | 新增 `src/api/conversation.rs`；`src/api.rs`、`src/router.rs` | 23 个端点 + SSE 处理器；路由挂在 `/api/v1/chat/sessions/*` |
| `wf-server` | `src/openapi.rs` 的 `paths(...)` 清单 | 24 个处理器已登记 utoipa；`openapi.json` 快照与 `schema.d.ts` 已重生成 |
| `wf-api` | **未做**：`src/infra/context.rs` 增字段 | 运行时表改用进程内 `OnceLock<DashMap>`（`conversation/runtime.rs`），不动 `ApiContext` 构造点 |
| `wf-runtime` | **未做** | 无需注入：配额/锁都在进程内表里，默认值即上线值 |
| `wf-client` / `web-app` / `wf-tui` | 已落地最小集成 | 端点常量、订阅/提交拆分、树侧栏、脏/模型漂移徽标、单步/顺序重算入口、TUI 只读树装配入口 |

依赖方向不变（箭头 = 依赖）：`wf-storage → wf-types`、`wf-server → wf-api → wf-execution-shared → wf-types`，无新增环。摘要在 `wf-execution-shared` 而非 `wf-api`，是为了让 CLI 与未来的桌面端复用同一套判定，避免"服务端一套、TUI 一套"。`runtime.rs` 用进程内表而非 `ApiContext` 字段，是刻意的取舍：多进程部署本就需要分布式锁，进程内表不假装解决它，也避免改动 `ApiContext` 的每个构造点（`wf-api` 里有多处）。

### 11.2 测试策略

* **纯函数（无 IO）**：
  * 摘要：同内容不同 id / `ordinal` / 时间戳 ⇒ 同摘要；段序变化 ⇒ 不同；`Premise` 折叠与"无回复"折叠可区分。
  * 脏：改祖先内容 ⇒ 后代全脏；改回 ⇒ 自动干净；弃用 / 恢复 ⇒ 下游脏；换模型 ⇒ 不脏但有 `modelDrift`。
  * 手术：`insert_before` + `splice_out` 往返 ⇒ 摘要复原；`attach` 成环被拒；`detach(carry_prefix=true)` ⇒ Graft 树干净；`false` ⇒ 全脏。
  * 投影：路径跳过 `Discarded` / `Interrupted` / `Superseded`；`Premise` 无回复段。
* **服务层（内存存储 + mock LLM）**：提交→生成→落库→树状态；单飞拒绝；`if_version` 冲突；`forward` 自然收敛（重算出同样内容时提前结束）；`max_steps` 截断；失败中断保留已完成步。
* **并发与生命周期**：提交后立刻丢弃 SSE → 轮仍完成且落库；重新订阅拿 snapshot（含在飞累计文本）；多观察者各收一份帧。
* **崩溃恢复**：构造 `Pending` 树 + 空注册表 → 加载即 `Interrupted`；`DigestIndex` 重建后脏集合与崩溃前一致。
* **工具轮**：白名单外被拒且整轮失败；工具失败降级为 `tool_result` 后仍出答案；超 4 次被拒；无工具时只发一次生成。
* 单测随源文件（`#[cfg(test)]`），大文件用 `test.rs`，服务层集成测试放 `tests/`（遵循 `AGENTS.md`）。

### 11.3 实施顺序（每步可独立编译与验收）

| 步 | 内容 | 验收 |
| --- | --- | --- |
| 1 | 类型 + 摘要 + 投影 + 不变量（纯函数） | 纯函数单测全绿，`cargo clippy --all-targets --all-features` 通过 |
| 2 | 存储实体 + 线性升级 + 树 / 路径 / 脏清单（只读） | 老会话读出与原来一致的路径，且全干净 |
| 3 | 会话实例 + 提交/订阅分离 + 取消 + 单飞 | 断连续跑、重连续看、取消转 `Interrupted` |
| 4 | 内容操作（原位改 / 分支改 / 改正文 / 弃用恢复）+ 脏传播 | 改一条 ⇒ 下游脏计数正确；改回 ⇒ 归零 |
| 5 | 步进（single / forward） | 走查场景（§5.7）逐行可复现；自然收敛用例通过 |
| 6 | 手术（insert / splice / reparent / detach / attach + 池） | 往返复原；跨会话嫁接全脏；环被拒 |
| 7 | 只读工具轮 + 白名单 | 白名单内外分野清晰，检查点数量为 0 |
| 8 | web-app + TUI + codegen + 文档 | `openapi.json` 快照测试通过，`schema.d.ts` 重生成，端点文档同步 |

顺序理由：摘要在第 1 步就得定死，它是第 4、5、6 步的共同地基；手术放第 6 步是因为它依赖脏计算但不被脏计算依赖。

### 11.4 验收清单

* 单根、不变量、版本冲突、单飞、断连续跑、回放、编辑隔离、弃用可逆、工具白名单、`Pending` 收敛——全有用例。
* **脏状态**：改任意前继的用户消息 / 助手正文 / 弃用态，路径上的脏计数 = 受影响节点数；改回后归零。
* **步进**：`single` 后下游仍脏；`forward` 后路径归零；`max_steps` 截断时 `remainingDirty` 正确。
* **手术**：`insert_before` + `splice_out` 往返后脏计数回到 0（不靠任何"清理标记"）；`detach(carry_prefix=true)` 后 Graft 树脏计数为 0。
* 不新增文件检查点、不触发工具审批、不产生执行层级节点（断言：跑一轮树会话后 checkpoint 数量为 0）。
* 线性 `/chat/stream` 行为不变（现有用例保持通过）；agent loop / workflow 的消息读路径无行为变化。

### 11.5 待拍板开放点

| 编号 | 问题 | 本方案默认取向 | 若改变的影响 |
| --- | --- | --- | --- |
| O1 | 改写助手正文后，下游是否必然全脏 | 是（`reply_part` 参与折叠） | 若希望"改文字不算改历史"，需把 `reply_part` 从折叠里摘出——但那样弃用/重生成也不会传播，与需求冲突 |
| O2 | 编辑用户消息的默认行为：分支改 or 原位改 | UI 默认分支改（安全），高级菜单给原位改 | 反过来会让用户误伤整条分支；建议保持双入口 |
| O3 | 嫁接是否允许跨会话 | 允许，但必须 `detach` → `attach` 两步 | 一步跨会话要跨两个文档事务；当前取舍是牺牲便利换原子性 |
| O4 | `carry_prefix` 默认值 | `true`（被嫁接子树保持干净） | 默认 `false` 更"诚实"（历史确实变了），但会让每次挪动都触发长链重算 |
| O5 | `forward` 默认步数上限 | 8 | 过小会频繁截断，过大则一次点击可能很贵 |
| O6 | 脏回复是否阻断新提交 | 不阻断，仅告警 | 阻断更"纯净"，但会打断探索式对话 |
| O7 | `acknowledge`（确认过期）是否要 | P1，要（避免反复提示） | 不要的话脏徽标会一直挂着 |
| O8 | `Premise` 是否允许带工具轨迹 | 不允许（纯输入） | 允许就等于引入"提前检索"，会破坏一问一答的边界 |
| O9 | 单会话节点上限 / 深度 | 2000 / 512 | 影响单文档体积与树渲染性能 |
| O10 | 工具白名单粒度 | 会话级默认 + 每轮可覆盖 | 全局粒度省配置但缺灵活性 |
| O11 | 树视图先做 web 还是 TUI | web 先行，TUI 只读跟进 | 取决于主要使用面 |
| O12 | `undo` 是否进 P0 | P2（但 `OpRecord` 在 P0 就写） | 提前做会拖慢落地，完全不记录则将来补不了 |
| O13 | 首段虚拟 system prompt（会话级） | P2，且作为"虚拟段"进折叠不进树 | 若进树，改它会让整棵树脏——这其实是正确语义，但成本高 |

O1、O2、O4 会改变对外语义与用户手感，建议最先定；其余可在实施中按用例反馈调整，不阻塞第 1 步。
