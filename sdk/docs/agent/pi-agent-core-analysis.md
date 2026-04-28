# pi-agent-core 架构分析与借鉴

本文档分析 [@mariozechner/pi-agent-core](https://github.com/mariozechner/pi-agent-core) 的设计思想，并对比当前 sdk/agent 的实现，提出改进建议。

## 一、pi-agent-core 核心设计思想

### 1. 事件驱动架构

pi-agent-core 提供了清晰的事件流设计，支持细粒度的执行追踪：

```
prompt("Hello")
├─ agent_start
├─ turn_start
├─ message_start   { message: userMessage }
├─ message_end     { message: userMessage }
├─ message_start   { message: assistantMessage }
├─ message_update  { message: partial... }
├─ message_end     { message: assistantMessage }
├─ turn_end        { message, toolResults: [] }
└─ agent_end       { messages: [...] }
```

**事件类型**：

| 事件 | 描述 |
|------|------|
| `agent_start` | Agent 开始处理 |
| `agent_end` | Agent 完成，返回所有新消息 |
| `turn_start` | 新轮次开始（一次 LLM 调用 + 工具执行） |
| `turn_end` | 轮次完成，返回 assistant 消息和工具结果 |
| `message_start` | 任意消息开始（user/assistant/toolResult） |
| `message_update` | 仅 assistant，包含 delta |
| `message_end` | 消息完成 |
| `tool_execution_start` | 工具开始执行 |
| `tool_execution_update` | 工具流式进度 |
| `tool_execution_end` | 工具执行完成 |

### 2. 消息转换管道

```
AgentMessage[] → transformContext() → AgentMessage[] → convertToLlm() → Message[] → LLM
                    (optional)                           (required)
```

- **transformContext**: 可选，用于消息压缩、历史裁剪、上下文注入
- **convertToLlm**: 必需，过滤 UI-only 消息，转换自定义类型为 LLM 格式

**自定义消息类型**（通过声明合并）：

```typescript
declare module "@mariozechner/pi-agent-core" {
  interface CustomAgentMessages {
    notification: { role: "notification"; text: string; timestamp: number };
  }
}
```

### 3. Steering 和 Follow-up 机制

**Steering（转向）**：
- 在工具执行过程中注入中断消息
- 剩余工具被跳过并返回错误结果
- LLM 响应中断消息

```typescript
// 工具执行中途干预
agent.steer({
  role: "user",
  content: "Stop! Do this instead.",
  timestamp: Date.now(),
});
```

**Follow-up（后续）**：
- 在 Agent 完成后自动追加任务
- 仅在无工具调用且无 steering 消息时检查

```typescript
// 完成后追加任务
agent.followUp({
  role: "user",
  content: "Also summarize the result.",
  timestamp: Date.now(),
});
```

### 4. 状态管理

```typescript
interface AgentState {
  systemPrompt: string;
  model: Model<any>;
  thinkingLevel: ThinkingLevel;
  tools: AgentTool<any>[];
  messages: AgentMessage[];
  isStreaming: boolean;
  streamMessage: AgentMessage | null;  // 流式过程中的部分消息
  pendingToolCalls: Set<string>;       // 待完成的工具调用
  error?: string;
}
```

关键设计：
- `streamMessage`: 保存流式过程中的部分消息，便于 UI 更新
- `pendingToolCalls`: 追踪待完成的工具调用（Set 结构）

### 5. 工具定义

```typescript
interface AgentTool {
  name: string;
  label: string;  // UI 显示名称
  description: string;
  parameters: TypeBox schema;
  execute: (toolCallId, params, signal, onUpdate) => Promise<ToolResult>;
}
```

关键设计：
- `label`: UI 友好的显示名称
- `onUpdate`: 流式进度回调
- 错误处理：抛出异常而非返回错误内容

### 6. continue() 方法

从当前上下文继续执行，无需添加新消息。用于错误重试。

```typescript
// 错误后重试
await agent.continue();
```

要求：最后一条消息必须是 user 或 toolResult（不能是 assistant）。

### 7. 动态配置

```typescript
const agent = new Agent({
  // 动态 API Key（支持 OAuth token 刷新）
  getApiKey: async (provider) => refreshToken(),
  
  // 自定义思考预算
  thinkingBudgets: {
    minimal: 128,
    low: 512,
    medium: 1024,
    high: 2048,
  },
  
  // Session ID（Provider 缓存）
  sessionId: "session-123",
});
```

---

## 二、当前 sdk/agent 对比分析

### 架构对比

| 特性 | pi-agent-core | sdk/agent | 差距 |
|------|---------------|-----------|------|
| 事件粒度 | 细粒度（turn/message/tool） | 中粒度 | 缺少 turn 概念 |
| 消息转换 | transformContext + convertToLlm | ConversationSession | 缺少转换管道 |
| Steering | ✅ | ❌ | 缺少中断注入 |
| Follow-up | ✅ | ❌ | 缺少后续队列 |
| 工具流式进度 | onUpdate 回调 | ❌ | 缺少进度回调 |
| continue() | ✅ | ❌（有 resume） | 语义不同 |
| 动态 API Key | getApiKey | profileId | 不支持动态刷新 |
| 自定义消息类型 | 声明合并 | ❌ | 不支持 |
| Checkpoint | ❌ | ✅ | sdk/agent 优势 |

### 当前 sdk/agent 的优势

1. **Checkpoint 机制**：状态快照和恢复，pi-agent-core 没有
2. **分层架构**：Entity/Coordinator/Executor 清晰分层
3. **Graph 集成**：可作为 Graph 节点执行
4. **统一消息管理**：ConversationSession 统一管理消息

---

## 三、改进建议

### 优先级 1：事件系统增强

**目标**：提供细粒度事件，支持 turn 概念

**新增事件类型**：

```typescript
export type AgentStreamEvent = 
  | { type: 'agent_start'; timestamp: number }
  | { type: 'agent_end'; messages: LLMMessage[]; timestamp: number }
  | { type: 'turn_start'; iteration: number; timestamp: number }
  | { type: 'turn_end'; iteration: number; message: LLMMessage; toolResults: ToolResult[] }
  | { type: 'message_start'; message: LLMMessage }
  | { type: 'message_update'; delta: Partial<LLMMessage> }
  | { type: 'message_end'; message: LLMMessage }
  | { type: 'tool_execution_start'; toolCallId: string; toolName: string; args: unknown }
  | { type: 'tool_execution_update'; toolCallId: string; partialResult: unknown }
  | { type: 'tool_execution_end'; toolCallId: string; result: ToolResult }
  | { type: 'iteration_complete'; iteration: number; shouldContinue: boolean };
```

### 优先级 2：消息转换管道

**目标**：支持消息压缩、历史裁剪、自定义类型

**配置扩展**：

```typescript
interface AgentLoopConfig {
  // ... existing fields
  
  /** 消息转换管道（可选） */
  transformContext?: (messages: LLMMessage[], signal?: AbortSignal) => Promise<LLMMessage[]>;
  
  /** 转换为 LLM 格式（可选，用于自定义消息类型） */
  convertToLlm?: (messages: LLMMessage[]) => LLMMessage[];
}
```

### 优先级 3：Steering 和 Follow-up

**目标**：支持执行中断和任务追加

**API 设计**：

```typescript
class AgentLoopEntity {
  private steeringQueue: LLMMessage[] = [];
  private followUpQueue: LLMMessage[] = [];
  
  /** 注入 Steering 消息（中断当前工具执行） */
  steer(message: LLMMessage): void;
  
  /** 注入 Follow-up 消息（完成后追加） */
  followUp(message: LLMMessage): void;
  
  /** 清空队列 */
  clearSteeringQueue(): void;
  clearFollowUpQueue(): void;
  clearAllQueues(): void;
}
```

### 优先级 4：工具流式进度

**目标**：支持工具执行过程中的进度报告

**执行器修改**：

```typescript
interface ToolExecutor {
  execute(
    toolCall: ToolCall,
    signal?: AbortSignal,
    onUpdate?: (partial: ToolResult) => void  // 新增
  ): Promise<ToolResult>;
}
```

### 优先级 5：状态增强

**目标**：支持流式状态追踪

**状态扩展**：

```typescript
class AgentLoopState {
  /** 流式过程中的部分消息 */
  streamMessage: LLMMessage | null = null;
  
  /** 待完成的工具调用 */
  pendingToolCalls: Set<string> = new Set();
}
```

### 优先级 6：continue() 方法

**目标**：支持从当前状态重试

**API 设计**：

```typescript
class AgentLoopCoordinator {
  /** 从当前状态继续执行（用于重试） */
  async continue(id: ID): Promise<AgentLoopResult>;
}
```

---

## 四、实施计划

### Phase 1：事件系统（优先级最高）

1. 扩展 `AgentStreamEvent` 类型定义
2. 修改 `AgentStreamExecutor` 发送细粒度事件
3. 添加 `turn_start/turn_end` 追踪

### Phase 2：消息转换管道

1. 扩展 `AgentLoopConfig` 接口
2. 在 `AgentLoopExecutor` 中集成转换管道
3. 添加默认实现

### Phase 3：Steering 和 Follow-up

1. 在 `AgentLoopEntity` 中添加队列
2. 修改 `AgentIterationExecutor` 检查队列
3. 实现中断逻辑

### Phase 4：工具流式进度

1. 修改 `ToolCallExecutor` 接口
2. 添加 `tool_execution_update` 事件
3. 更新工具定义

### Phase 5：状态增强

1. 扩展 `AgentLoopState`
2. 集成到流式执行流程

### Phase 6：continue() 方法

1. 在 `AgentLoopCoordinator` 中实现
2. 添加状态验证

---

## 五、保留优势

在实施改进时，必须保留 sdk/agent 的现有优势：

1. **Checkpoint 机制**：所有改进必须兼容 Checkpoint
2. **分层架构**：保持 Entity/Coordinator/Executor 分层
3. **Graph 集成**：确保 Agent 可作为 Graph 节点
4. **ConversationSession**：统一消息管理不变
