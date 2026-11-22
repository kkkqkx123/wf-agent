# chat 执行链：流式驱动与双写持久化

> 源码：`crates/app/wf-api/src/chat.rs`（入口 `stream`，常量 `CHAT_SESSION_DEFINITION_ID = "@chat"`）

## 入口与一轮的流程

`stream(ctx, ChatTurnParams) -> ExecutionEventStream` 是 chat 会话的一轮：

1. **校验与解析**：`profile_id` 非空；解析 @提及（`resolve_mentions` 得到注入消息、图片与回执）；校验图片载荷；文本与图片不能同时为空。
2. **恢复历史**：`resolve_session_id` 确定（或新建）会话 id，从 message store 读回 `conversation_history`，用 `ChatSession::restore` 续接；提及消息与回执作为额外上下文注入。
3. **构造请求**：`begin_turn` 追加用户轮次并生成无工具请求（原语保证无 tools 字段）。
4. **流式生成**：`llm_gateway.generate_stream` 起 provider 流，`tokio::spawn` 一个 driver 任务消费它。
5. **收尾双写**（见下）。

## 流式驱动：`drive_turn`

消费 `MessageStreamEvent`，把内容实时转发给前端通道（`ExecutionStreamEvent::LlmDelta` / `ReasoningDelta` / `Usage`），同时累积：

- `answer`：最终回答文本；
- `assistant`：`FinalMessage` / `Message` 事件携带的完整助手消息；
- `usage`：provider 上报的 token 用量（`merge_usage` 合并）；
- `refusal`：拒答快照，单独保存、不混入回答文本。

消费者断开发送帧失败、流内错误、或流结束仍无最终消息时返回 `TurnOutcome::Failed`；`Abort` 返回 `Interrupted`。正常结束调用 `session.record_reply(...)` 后返回 `Completed`。

## 双写持久化

流式结束后的 driver 执行两步，**无条件与有条件的分工**：

| 步骤 | 函数 | 条件 | 存储 | 内容 |
|------|------|------|------|------|
| ① 会话元数据 | `finish_turn` | 无条件（Completed / Failed / Interrupted 都写） | `agent_execution` 表 | 状态、起止时间、模型 profile、目录 |
| ② 消息 transcript | `persist_messages` | 仅 `Completed` | message store | 本轮 user + assistant 消息 |

两处存储职责不同，缺一不可：transcript 支撑下一轮续接与会话回放；执行记录支撑统一的列表/详情/审计查询。

### ① `finish_turn`：混入 `agent_execution` 表

写入一条 `AgentExecution`：

- `execution_type: ExecutionType::Chat`，`definition_id = "@chat"`——chat 没有模板，故以执行器命名，把"没有 agent 模板"显式编码进 id；
- `current_iteration: 0`、`tool_call_count: 0`、`iteration_history: None`、`loop_config: None`、`hierarchy: None`——agent 专属字段全部置零/置空；
- **会话级字段继承**：`started_at` 与 `directory` 从已有记录读取（首轮的值），保证记录横跨整个会话、中途切换工作区不会把早前轮次改归新目录；
- 失败仅 `tracing::warn`，不影响流式结果返回。

### ② `persist_messages`：transcript 全序

- 只持久化本轮新增的消息：`history.len() - 2 - injected`（user + assistant，加上注入的提及上下文偏移）之后的切片；
- 时间戳用 `ledger_base + 1 + index` 递增——因为同一轮内的消息可能共享同一毫秒，`ledger_base` 取历史最大时间戳与当前时间中较大者，保证会话顺序是**全序**。

## 为什么要共用 `agent_execution` 表

`ExecutionType` 三值：`Workflow / Agent / Chat`（`wf-types/src/execution/hierarchy.rs`）。chat 会话与 agent run 共用同一张表、同一套 registry / list / detail API（`execution_listing.rs` 把 Agent 与 Chat 一起查询，detail 注释直言 "chat sessions are `agent_execution` rows"），换来：

- chat 会话自动获得统一的列表、详情、层级、审计能力；
- 查询侧零特例。

代价只是几个 agent 专属字段置零/置空。测试覆盖：一轮完成持久化会话记录与消息、二轮续接已存会话、无 session id 自建会话、提及解析与回执持久化、空白消息与缺失 profile 在模型运行前即被拒绝。

## 事件流形状

返回的 `ExecutionEventStream` 复用 agent 的 `ExecutionStreamEvent`，但语义收敛为：`LlmDelta`（文本增量）、`ReasoningDelta`（思维链增量）、`Usage`（token 用量）、`Completed { result, iterations: 0 }` / `Failed` / `Interrupted`。流结束时会发出 `Completed`，其 `result` 为回答文本字符串、`iterations` 恒为 0。
