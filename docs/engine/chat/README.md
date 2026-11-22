# Chat Session 文档

> 范围：chat session 的完整实现链路——引擎原语、API 执行链、前端控制器，以及它与 agent loop 的职责边界。

## 文档索引

| 文档 | 内容 |
|------|------|
| [01-session-primitive.md](01-session-primitive.md) | `ChatSession` 引擎原语：无工具多轮聊天的最小实现，不落库 |
| [02-execution-chain.md](02-execution-chain.md) | `wf-api` chat 执行链：流式驱动、双写持久化、混入 `agent_execution` 表 |
| [03-frontend-and-agent-boundary.md](03-frontend-and-agent-boundary.md) | 前端 `ChatSessionController`，以及 chat 与 agent loop / TemporaryMode 的职责边界 |

## 一图总览

```
前端 ChatSessionController (apps/web-app/src/lib/chat/chat-session.svelte.ts)
        │  sendText → runChat / runAgent / runWorkflow
        ▼
wf-api chat::stream (crates/app/wf-api/src/chat.rs)
        │  restore(history) → begin_turn → llm_gateway.generate_stream
        │  drive_turn（流式转发）→ finish_turn + persist_messages（双写）
        ▼
ChatSession 原语 (crates/engine/wf-execution-shared/src/chat.rs)
        │  无工具多轮生成，调用方持有状态，零副作用零落库
        ▼
LlmGateway（wf-llm）

持久化：message store（transcript） + agent_execution 表（会话元数据，ExecutionType::Chat）
```

## 核心要点

- **ChatSession 是原语，不是执行域**：无工具、无 checkpoint、无审批、无事件发布、不落库（`chat.rs` 模块头）。
- **chat 执行链负责持久化（双写）**：消息写 message store 存 transcript；会话元数据以 `ExecutionType::Chat` 写入 `agent_execution` 表，`definition_id = "@chat"`，与 agent run 共用同一套 list/detail/层级 API，查询侧零特例。
- **agent 的"无工具模式"不是 chat**：agent loop 的空工具池与 `TemporaryMode`（restricted/ephemeral）都跑在 `execution_type = agent` 上、走完整 agent 生命周期，与 chat 是两条不同的执行域。详见 03。
- 选型准则见 03 文档末节。
