# 统一边界配置使用示例

本文档展示了如何使用统一的边界数据传递配置（WorkflowStartConfig 和 WorkflowEndConfig）。

## 1. 基本用法

### 1.1 START节点配置变量输入

```typescript
import type { StartNodeConfig } from "@wf-agent/types";

const startConfig: StartNodeConfig = {
  variableInputs: [
    {
      externalName: "apiKey",        // 父工作流中的变量名
      internalName: "api_key",       // 当前工作流内部使用的名称
      required: true,
      description: "API key for authentication"
    },
    {
      externalName: "config",
      internalName: "settings",
      defaultValue: { timeout: 5000 },
      description: "Optional configuration settings"
    }
  ]
};
```

### 1.2 START节点配置消息上下文输入

```typescript
const startConfig: StartNodeConfig = {
  messageInputs: [
    {
      externalName: "main-conversation",   // 调用方使用的上下文ID
      internalName: "chat-history",        // 工作流内部使用的上下文ID
      required: true,
      description: "Main conversation history"
    },
    {
      externalName: "system-context",
      internalName: "system-messages",
      defaultMessages: [
        { role: "system", content: "You are a helpful assistant." }
      ],
      description: "System messages context"
    }
  ]
};
```

### 1.3 END节点配置输出

```typescript
import type { EndNodeConfig } from "@wf-agent/types";

const endConfig: EndNodeConfig = {
  variableOutputs: [
    {
      internalName: "result",          // 工作流内部的变量名
      externalName: "output",          // 返回给调用方的名称
      description: "Processing result"
    }
  ],
  messageOutputs: [
    {
      internalName: "chat-history",    // 工作流内部的上下文ID
      externalName: "updated-chat",    // 返回给调用方的上下文ID
      description: "Updated conversation with AI responses"
    }
  ]
};
```

## 2. Subgraph场景

### 2.1 父工作流中的SUBGRAPH节点配置

```toml
[[nodes]]
id = "subgraph-node"
type = "SUBGRAPH"
name = "Process Data"

[nodes.config]
workflowId = "data-processor-workflow"

# 变量映射：父工作流变量 -> 子图内部变量
variableInputs = [
  { externalName = "input_data", internalName = "data", required = true },
  { externalName = "options", internalName = "opts", defaultValue = {} }
]

# 消息上下文映射：父工作流上下文 -> 子图内部上下文
messageInputs = [
  { externalName = "conversation", internalName = "chat", required = true }
]

# 输出映射：子图内部变量 -> 父工作流变量
variableOutputs = [
  { internalName = "processed_data", externalName = "result" }
]

# 消息上下文输出：子图内部上下文 -> 父工作流上下文
messageOutputs = [
  { internalName = "chat", externalName = "updated_conversation" }
]
```

### 2.2 子图中的START节点配置

```toml
[[nodes]]
id = "start"
type = "START"

[nodes.config]
# 声明接收的输入（与父工作流的variableInputs对应）
variableInputs = [
  { externalName = "input_data", internalName = "data", required = true },
  { externalName = "options", internalName = "opts" }
]

# 声明接收的消息上下文
messageInputs = [
  { externalName = "conversation", internalName = "chat", required = true }
]
```

### 2.3 子图中的END节点配置

```toml
[[nodes]]
id = "end"
type = "END"

[nodes.config]
# 声明返回的输出
variableOutputs = [
  { internalName = "processed_data", externalName = "result" }
]

# 声明返回的消息上下文
messageOutputs = [
  { internalName = "chat", externalName = "updated_conversation" }
]
```

## 3. Triggered Subworkflow场景

### 3.1 START_FROM_TRIGGER节点配置

```toml
[[nodes]]
id = "start_from_trigger"
type = "START_FROM_TRIGGER"

[nodes.config]
# 声明从触发器接收的输入
variableInputs = [
  { externalName = "user_query", internalName = "query", required = true }
]

messageInputs = [
  { externalName = "user_context", internalName = "conversation", required = true }
]
```

### 3.2 CONTINUE_FROM_TRIGGER节点配置

```toml
[[nodes]]
id = "continue_from_trigger"
type = "CONTINUE_FROM_TRIGGER"

[nodes.config]
# 声明返回给主工作流的输出
variableOutputs = [
  { internalName = "answer", externalName = "response" }
]

messageOutputs = [
  { internalName = "conversation", externalName = "updated_context" }
]

# 变量回调配置（可选，用于控制哪些变量返回）
variableCallback = { includeVariables = ["answer"] }
```

## 4. Agent Tool Call场景

### 4.1 使用execute_workflow工具

```typescript
// Agent调用工作流时，可以同时传递变量和消息上下文
const toolCall = {
  name: "execute_workflow",
  arguments: {
    workflowId: "my-workflow",
    
    // 变量输入
    input: {
      query: "What is the weather?",
      location: "Beijing"
    },
    
    // 消息上下文输入（新增功能）
    messageContexts: {
      "conversation": [
        { role: "user", content: "Hello" },
        { role: "assistant", content: "Hi! How can I help you?" }
      ],
      "system": [
        { role: "system", content: "You are a weather assistant." }
      ]
    },
    
    waitForCompletion: true
  }
};
```

### 4.2 被调用的工作流配置

```toml
# 工作流定义
[[nodes]]
id = "start_from_trigger"
type = "START_FROM_TRIGGER"

[nodes.config]
# 声明接收的变量
variableInputs = [
  { externalName = "query", internalName = "user_query", required = true },
  { externalName = "location", internalName = "user_location" }
]

# 声明接收的消息上下文
messageInputs = [
  { externalName = "conversation", internalName = "chat_history", required = false },
  { externalName = "system", internalName = "system_context" }
]

# ... 其他节点 ...

[[nodes]]
id = "end"
type = "CONTINUE_FROM_TRIGGER"

[nodes.config]
# 声明返回的变量
variableOutputs = [
  { internalName = "weather_info", externalName = "result" }
]

# 声明返回的消息上下文
messageOutputs = [
  { internalName = "chat_history", externalName = "updated_conversation" }
]
```

## 5. 完整的边界配置示例

### 5.1 使用WorkflowBoundaryConfig类型

```typescript
import type { WorkflowBoundaryConfig } from "@wf-agent/types";

// 完整的工作流接口定义
const myWorkflowInterface: WorkflowBoundaryConfig = {
  start: {
    variableInputs: [
      {
        externalName: "userInput",
        internalName: "input",
        required: true,
        description: "User's input text"
      }
    ],
    messageInputs: [
      {
        externalName: "history",
        internalName: "conversation",
        required: false,
        description: "Previous conversation history"
      }
    ]
  },
  end: {
    variableOutputs: [
      {
        internalName: "response",
        externalName: "output",
        description: "AI's response to the user"
      }
    ],
    messageOutputs: [
      {
        internalName: "conversation",
        externalName: "updated_history",
        description: "Updated conversation including this turn"
      }
    ]
  }
};
```

### 5.2 在工作流元数据中记录接口

```toml
[metadata]
name = "Chat Response Workflow"
version = "1.0.0"
description = "Processes user input and generates AI response"

# 文档化工作流接口
[metadata.interface]
inputs = [
  { name = "userInput", type = "string", required = true },
  { name = "history", type = "message[]", required = false }
]
outputs = [
  { name = "output", type = "string" },
  { name = "updated_history", type = "message[]" }
]
```

## 6. 最佳实践

### 6.1 命名规范

- **externalName**: 使用snake_case，清晰表达业务含义
  - ✅ `user_query`, `api_key`, `conversation_history`
  - ❌ `q`, `key`, `hist`

- **internalName**: 可以使用更简洁的名称，但要保持一致性
  - ✅ `query`, `apiKey`, `chatHistory`
  - ❌ `x`, `k`, `h`

### 6.2 Required字段的使用

```typescript
// ✅ 好的做法：明确标注必填项
variableInputs: [
  { externalName: "apiKey", internalName: "key", required: true },
  { externalName: "timeout", internalName: "to", defaultValue: 5000 }
]

// ❌ 避免：所有字段都设为required
variableInputs: [
  { externalName: "apiKey", internalName: "key", required: true },
  { externalName: "timeout", internalName: "to", required: true }  // 有默认值就不需要required
]
```

### 6.3 描述文档

```typescript
// ✅ 好的做法：提供清晰的描述
messageInputs: [
  {
    externalName: "conversation",
    internalName: "chat",
    required: true,
    description: "Full conversation history including user queries and AI responses"
  }
]

// ❌ 避免：缺少描述或使用模糊的描述
messageInputs: [
  {
    externalName: "conversation",
    internalName: "chat",
    description: "messages"  // 太模糊
  }
]
```

### 6.4 错误处理

```typescript
// 在handler中验证必填参数
if (config?.messageInputs) {
  for (const inputDef of config.messageInputs) {
    const messages = triggerInput.messageContexts?.[inputDef.externalName];
    
    if (!messages && inputDef.required) {
      throw new Error(
        `Required message context '${inputDef.externalName}' ` +
        `(mapped to '${inputDef.internalName}') is missing`
      );
    }
  }
}
```

## 7. 迁移指南

### 7.1 从旧配置迁移到新配置

如果你之前使用了特殊的配置类型，可以平滑迁移到统一配置：

**旧代码**:
```typescript
import type { WorkflowStartConfig } from "@wf-agent/types";

// 运行时特有字段单独管理
const runtimeMetadata = {
  originalSubgraphNodeId: "...",
  namespace: "...",
  depth: 1
};

const config: WorkflowStartConfig = {
  variableInputs: [...],
  messageInputs: [...]
};
```

**新代码**:
```typescript
import type { WorkflowStartConfig } from "@wf-agent/types";

// 边界配置部分
const boundaryConfig: WorkflowStartConfig = {
  variableInputs: [...],
  messageInputs: [...]
};

// 运行时特有字段单独管理
const runtimeMetadata = {
  originalSubgraphNodeId: "...",
  namespace: "...",
  depth: 1
};
```

### 7.2 兼容性说明

- 所有旧的配置类型仍然可用（标记为@deprecated）
- 新代码建议使用统一配置类型
- 旧配置将在v2.0版本中移除

## 8. 常见问题

### Q1: 什么时候使用variableInputs，什么时候使用messageInputs？

**A**: 
- **variableInputs**: 用于传递结构化数据（字符串、数字、对象等）
- **messageInputs**: 用于传递LLM对话历史（消息数组）

### Q2: externalName和internalName可以相同吗？

**A**: 可以，如果不需要重命名，可以设置为相同的值：
```typescript
{
  externalName: "query",
  internalName: "query",  // 保持相同
  required: true
}
```

### Q3: 如何处理可选的输入？

**A**: 设置`required: false`并提供`defaultValue`：
```typescript
{
  externalName: "options",
  internalName: "opts",
  required: false,
  defaultValue: { timeout: 5000 }
}
```

### Q4: 消息上下文的浅拷贝安全吗？

**A**: 对于大多数场景是安全的，因为消息对象通常是不可变的。如果需要深度隔离，可以在handler中使用深拷贝。

## 9. 总结

统一的边界配置提供了：

1. ✅ **一致性**: 所有场景使用相同的配置接口
2. ✅ **清晰度**: 明确的输入/输出契约
3. ✅ **灵活性**: 支持变量和消息的独立管理
4. ✅ **可维护性**: 减少重复定义，降低认知负担
5. ✅ **向后兼容**: 平滑迁移路径

通过遵循这些示例和最佳实践，你可以更好地利用统一的边界数据传递配置。
