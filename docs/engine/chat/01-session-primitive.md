# ChatSession 引擎原语

> 源码：`crates/engine/wf-execution-shared/src/chat.rs`

## 定位

`ChatSession` 是介于"单次无状态生成"和"完整 agent loop"之间的轻量原语：**调用方拥有的无工具多轮聊天会话**。模块头把边界写死：

> "There is no tool execution, no checkpointing, no approval, no event publishing and no execution-record persistence. The request template carries no tools field, so a tool-carrying chat request cannot be constructed."

所有状态都活在 `ChatSession` 实例内部，"nothing is shared with the agent loop or the checkpoint machinery"。它自身不落库——既不写 message store，也不写 `agent_execution`；持久化由上层 `wf-api` 包装链负责（见 02）。

## 结构

```rust
pub struct ChatTemplate {
    pub profile_id: String,
    pub parameters: Option<serde_json::Value>,
    pub generation: Option<LlmGenerationParams>,
}

pub struct ChatSession {
    template: ChatTemplate,
    conversation: ConversationSession,
}
```

- `ChatTemplate` 只有 `profile_id / parameters / generation` 三个字段，**刻意不含 tools**。
- `conversation` 复用通用的 `ConversationSession`，负责消息序列与 token 用量统计。

## 关键方法

| 方法 | 职责 |
|------|------|
| `new(profile_id)` / `with_template` | 构造会话 |
| `restore(messages)` | 注入既有历史（如从持久化会话恢复的消息、@提及注入的上下文），成为下一轮的历史 |
| `begin_turn(text, images, execution_id)` | 校验并追加用户轮次，构造**无工具**的 `LlmRequest` |
| `record_reply(message, usage)` | 记录助手回复；把 provider 上报的 token 用量折入会话 |
| `send(...)` | 单轮组合：`begin_turn` + 网关调用 + `record_reply`（非流式便捷路径） |

## 无工具约束的实现

`begin_turn` 构造请求时把工具相关字段全部置 `None`（`tools`、`tool_call_protocol`、`locked_tool_call_protocol`、`violation_policy`），因此**类型层面就无法构造出携带工具的 chat 请求**。测试 `a_turn_never_carries_tools` 断言了这一点。

`begin_turn` 的入参校验：

- 纯空白文本且无图片 → 拒绝；
- 图片载荷非法 → 拒绝；
- `profile_id` 为空 → 拒绝。

## 在分层中的位置

| 层 | 形态 |
|----|------|
| 单次生成 | 无状态 `LlmGateway` 调用 |
| **ChatSession** | **有会话状态的原语，无任何执行域副作用** |
| agent loop | 完整生命周期：事件、审批、迭代、checkpoint、执行记录、层级 |

`ChatSession` 是引擎原语层（engine 层 `wf-execution-shared`）的组件，可被任何上层复用——目前的使用方是 `wf-api` 的 chat 执行链（02 文档）。
