# Dynamic Context Integration Design

## 1. Problem Statement

当前 Agent Loop 在每次 LLM 请求时，消息数组仅包含 ConversationSession 中持久化的历史消息（system prompt + 用户消息 + assistant 回复），不包含任何运行时动态信息（如当前时间、TODO 列表、固定文件、工作区文件树等）。

**问题：**
- Agent 无法感知当前的运行时上下文（时间、待办事项等）
- 没有统一的机制在每次 LLM 请求前注入动态信息
- 动态信息如果直接写入 ConversationSession，会污染历史记录并破坏 KV 缓存

## 2. Design: Ephemeral Dynamic Context

### 2.1 Core Principle

动态上下文以 **ephemeral（短暂）** 方式注入：在每次 LLM 请求前附加到消息数组末尾，请求完成后不写回 ConversationSession。

```
ConversationSession（持久化，不变）:
  [system, user_Q1, assistant_A1, user_Q2, assistant_A2, ...]

每次 LLM 请求时 ephemeral 附加:
  [..., user: DynamicContext]

发送给 LLM 的完整消息序列:
  [system, user_Q1, assistant_A1, user_Q2, assistant_A2, ..., user: DynamicContext]
    ────────────── 公共前缀 ──────────────    ─── 新增部分（需重算 KV）───
```

### 2.2 KV Cache Compatibility

依据分析（见 conversation 记录），这种模式对 KV 缓存的影响：

| 部分 | KV 缓存 | 原因 |
|------|---------|------|
| `[system, ..., assistant_A2]` | ✅ 完全复用 | token 序列和位置索引在相邻请求间不变 |
| `[user: DynamicContext]` | ❌ 需重算 | 每次请求重新构建，内容可能变化 |

配合 `enable_prefix_caching=true`（vLLM/SGLang）或 Prompt Caching（Anthropic），公共前缀的 KV 缓存命中率接近 100%。

### 2.3 Cleanup Guarantee

每次请求的流程保证旧的动态上下文自动清理：

```
Turn N:
  1. conversationManager.getMessages() → [system, user_Q1, assistant_A1]
  2. transformContext() → 追加 [user: DC_N]
  3. executeLLMCall([system, user_Q1, assistant_A1, user: DC_N])
  4. conversationManager.addAssistantMessage(response)
     → ConversationSession: [system, user_Q1, assistant_A1, assistant_A2]
     → user: DC_N 没有被持久化

Turn N+1:
  1. conversationManager.getMessages() → [system, user_Q1, assistant_A1, assistant_A2]
     → user: DC_N 已自动消失
  2. transformContext() → 追加 [user: DC_{N+1}]
  3. executeLLMCall([system, user_Q1, assistant_A1, assistant_A2, user: DC_{N+1}])
```

## 3. Architecture: Integration Point

### 3.1 Existing Mechanism: TransformContextFn

`AgentLoopRuntimeConfig` 已经定义了 `TransformContextFn`（[context.ts](file:///d:/项目/agent/wf-agent/packages/types/src/agent-execution/context.ts#L33-L47)）：

```typescript
export type TransformContextFn = (
  messages: LLMMessage[],
  signal?: AbortSignal,
) => Promise<LLMMessage[]>;
```

**但当前 `AgentExecutionCoordinator` 从未调用它。** 这导致该机制形同虚设。

### 3.2 Integration in Coordinator

在 [agent-execution-coordinator.ts](file:///d:/项目/agent/wf-agent/sdk/agent/execution/coordinators/agent-execution-coordinator.ts) 中，两个执行路径都需要插入 transform 步骤：

```
Before (current):
  const llmResult = await this.llmExecutor.executeLLMCall(
    conversationManager.getMessages(), ...);

After (proposed):
  let messages = conversationManager.getMessages();
  
  // Transform context (for dynamic context injection, compression, etc.)
  if (entity.config.transformContext) {
    messages = await entity.config.transformContext(messages, abortSignal);
  }
  
  const llmResult = await this.llmExecutor.executeLLMCall(messages, ...);
```

### 3.3 Wiring in Application Layer

调用方（如 CLI App、Web App、或 SDK API 层）在构造 `AgentLoopRuntimeConfig` 时提供 `transformContext`：

```typescript
const config: AgentLoopRuntimeConfig = {
  profileId: "default",
  systemPrompt: "You are a helpful assistant.",
  transformContext: async (messages, signal) => {
    const dynamicMessages = buildDynamicContextMessages(dynamicConfig, runtime);
    if (dynamicMessages.length > 0) {
      return [...messages, ...dynamicMessages];
    }
    return messages;
  },
};
```

## 4. Type Simplification

### 4.1 dynamic-context.ts

保留（被实际消费）：
- `DynamicContextConfig` — SDK 中 `context.ts` 和 `composer.ts` 使用
- `DynamicRuntimeContext` — 同上
- `DynamicContextMessage` — 同上

删除（零引用）：
- `DynamicPromptResult` — 废弃，无任何引用
- `DynamicContextOptions` — 废弃，无任何引用
- `DynamicContextFragment` — 废弃，无任何引用

### 4.2 user-config.ts

保留（被实际消费）：
- `PinnedFileItem` — `dynamic-context.ts` 使用
- `SkillConfigItem` — `dynamic-context.ts` 使用

删除（零引用）：
- `PinnedFilesConfig` — 废弃
- `SkillsConfig` — 废弃
- `UserConfigType` — 废弃
- `UserConfigItem` — 废弃

### 4.3 todo.ts

保留：
- `TodoItem` — todo-list.ts 和 update-todo-list/handler.ts 使用
- `TodoStatus` — 同上
- `TodoPriority` — todo-list.ts 使用

删除：`TodoWriteArgs`、`TodoUpdateOp`、`TodoUpdateArgs`、`TodoStats`、`TodoWriteResult`、`TodoUpdateResult`、`TodoConfig`、`TodoValidationError`、`TodoOperationError`（均已确认零引用）

## 5. Implementation Plan

### Phase 1: Type Cleanup (已完成)
- [x] Simplify `todo.ts` — 删除 9 个未使用类型
- [ ] Simplify `dynamic-context.ts` — 删除 3 个未使用类型
- [ ] Simplify `user-config.ts` — 删除 4 个未使用类型

### Phase 2: Coordinator Integration
- [ ] In `executeIteration()`: 在调用 `executeLLMCall` 前调用 `transformContext`
- [ ] In `executeIterationStream()`: 在流式调用前调用 `transformContext`

### Phase 3: Application Wiring
- [ ] 应用层提供 `transformContext` 实现，调用 `buildDynamicContextMessages()`

## 6. DynamicContextMessage 的 role 扩展

当前 `DynamicContextMessage.role` 固定为 `"user"`。从设计上看，动态上下文有两种可能的注入方式：

| 方式 | role | 适用场景 |
|------|------|---------|
| 追加到历史末尾 | `"user"` | 作为额外的上下文信息，LLM 将其视为用户提供的背景信息 |
| 作为系统级上下文 | `"system"` | 直接在 system prompt 后追加（但会破坏 system prompt 的缓存） |

推荐使用 `"user"`，因为：
1. 放在末尾不会破坏 llama.cpp/vLLM 等框架对 system prompt 的 KV 缓存
2. 语义上更清晰：这是运行时的"用户上下文"而非系统指令

## 7. Future Considerations

### 7.1 Token Budget Management
当动态上下文过大时，应该限制其 token 数，或根据剩余 token 预算动态裁剪。

### 7.2 Selective Context Injection
不是所有 LLM 调用都需要动态上下文（如 tool call 结果的后续 LLM 调用），可以通过 `DynamicContextConfig` 控制。

### 7.3 Streaming Support
流式模式下同样需要注入动态上下文，但注入时机应在流式响应开始之前。

### 7.4 ConversationSession 的 AddAssistantMessage
注意：`executeLLMCall` 返回后，assistant 消息通过 `conversationManager.addAssistantMessage()` 存入历史。其中**不包含**动态上下文消息，所以动态上下文不会污染历史记录。