# 提示词与消息管理架构分析

## 一、当前架构分析

### 1.1 消息定义

消息定义位于 `packages/types/src/message/message.ts`：

```typescript
// 基础消息接口
export interface Message {
  role: MessageRole;           // 'system' | 'user' | 'assistant' | 'tool'
  content: MessageContent;     // string | 内容块数组
  id?: string;
  timestamp?: number;
  metadata?: Record<string, any>;
}

// LLM消息接口（扩展基础消息）
export interface LLMMessage extends Message {
  thinking?: string;           // 思考内容（Extended Thinking）
  toolCalls?: LLMToolCall[];   // 工具调用
  toolCallId?: string;         // 工具调用ID（tool角色）
}
```

**关键观察**：
- `system` 角色消息是标准消息数组的一部分
- 系统提示词通过 `role: 'system'` 的消息表示
- 消息数组中可以有多个 `system` 消息（用于分段注入）

### 1.2 Graph模块的消息管理

#### 核心组件

```
ThreadEntity (有状态实体)
├── ThreadState (执行状态管理)
├── ExecutionState (子图执行栈)
├── MessageHistoryManager (消息历史)
│   └── 继承自 ConversationManager
│       └── 继承自 MessageHistory
└── 可选：ConversationManager
```

#### 职责分离

| 组件 | 职责 | 设计评价 |
|------|------|----------|
| **ThreadEntity** | 有状态实体，封装所有执行数据 | ✅ 正确，职责清晰 |
| **MessageHistoryManager** | 管理消息历史，扩展Graph特有功能 | ✅ 正确，专门化扩展 |
| **ConversationManager** | 对话管理，Token统计和事件触发 | ✅ 正确，通用功能 |
| **MessageHistory** | 基础消息存储和批次可见性 | ✅ 正确，基础抽象 |
| **LLM Node** | 执行节点，不维护消息历史 | ✅ 正确，单一职责 |

#### 消息流

```
ThreadEntity (持久化存储)
    ↓
Node执行前 → 从 ThreadEntity 获取消息历史
    ↓
LLM Node → 添加新的 user 消息（尾插）
    ↓
LLMExecutionCoordinator → 调用LLM
    ↓
添加 assistant 消息 → 回写到 ThreadEntity
    ↓
下一个Node → 重复上述流程
```

**关键设计**：
- ✅ **LLM Node只负责尾插新消息**：仅添加当前节点的 `user` 消息
- ✅ **上下文由有状态组件维护**：`ThreadEntity` 维护完整消息历史
- ✅ **消息数组传递**：通过 `conversationManager` 共享

#### 代码证据

```typescript
// llm-handler.ts
export async function llmHandler(thread: Thread, node: Node, context: LLMHandlerContext) {
  // 使用传入的 conversationManager（由ThreadEntity维护）
  const result = await context.llmCoordinator.executeLLM(
    { prompt: executionData.prompt, ... },
    context.conversationManager  // ← 不自己创建
  );
}

// llm-execution-coordinator.ts
async executeLLM(request: LLMRequest, conversationState: ConversationManager) {
  // 添加用户消息
  conversationState.addMessage(userMessage);
  
  // 调用LLM（传入完整消息数组）
  const result = await this.llmExecutor.executeLLMCall(
    conversationState.getMessages(),  // ← 获取当前可见消息
    ...
  );
  
  // 添加助手消息
  conversationState.addMessage(assistantMessage);
}
```

### 1.3 Agent-Loop模块的消息管理

#### 核心组件

```
AgentLoopEntity (有状态实体)
├── MessageHistoryManager (Agent专用，简化版)
├── VariableStateManager (变量状态)
└── AgentLoopState (执行状态)

AgentLoopExecutor (执行协调)
└── AgentIterationExecutor (单次迭代)
    └── message-history-helper (初始化工具)
```

#### 职责分析

| 组件 | 职责 | 设计评价 |
|------|------|----------|
| **AgentLoopEntity** | 有状态实体，维护消息历史 | ✅ 正确 |
| **Agent MessageHistoryManager** | 简化版消息管理 | ⚠️ 与Graph不统一 |
| **AgentLoopExecutor** | 执行协调，管理迭代 | ✅ 正确 |
| **message-history-helper** | 初始化消息历史 | ⚠️ 功能单一 |

#### 消息流

```
AgentLoopEntity (持久化存储)
    ↓
执行开始 → 创建临时的 MessageHistory
    ↓
initializeMessageHistory() → 注入 systemPrompt + 历史消息
    ↓
第1次迭代 → 调用LLM（传入完整历史）
    ↓
添加 assistant 消息
    ↓
执行工具 → 添加 tool 消息
    ↓
第2次迭代 → 重复（历史已更新）
    ↓
...直到结束
    ↓
同步到 AgentLoopEntity
```

#### 关键差异

```typescript
// Agent-Loop：双层架构（临时 + 持久化）
const messageHistory = createMessageHistory();  // 临时的
initializeMessageHistory(messageHistory, config, entity.getMessages());

// 迭代中使用临时历史
const llmResult = await llmExecutor.executeLLMCall(
  messageHistory.getMessages(),  // 临时历史
  ...
);
messageHistory.addAssistantMessage(...);  // 更新临时历史
entity.addMessage(assistantMessage);       // 同步到持久化

// Graph：单层架构（直接使用持久化）
const result = await llmCoordinator.executeLLM(
  { ... },
  context.conversationManager  // 直接使用ThreadEntity的
);
```

### 1.4 系统提示词处理对比

| 方面 | Graph | Agent-Loop | 评价 |
|------|-------|------------|------|
| **注入时机** | 节点执行时 | 执行开始时 | 不同策略 |
| **注入方式** | 节点配置 | config.systemPrompt | 不统一 |
| **模板支持** | templateId | 字符串 | 不一致 |
| **位置** | 消息数组首条 | 消息数组首条 | ✅ 一致 |

**代码对比**：

```typescript
// Graph：在节点配置中解析
function resolvePrompt(config: LLMNodeConfig): string {
  if (config.promptTemplateId) {
    return templateRegistry.render(config.promptTemplateId, ...);
  }
  return config.prompt || '';
}

// Agent-Loop：在初始化时注入
function initializeMessageHistory(
  messageHistory: MessageHistory,
  config: AgentLoopConfig,
  existingMessages: LLMMessage[]
) {
  if (config.systemPrompt) {
    messageHistory.addSystemMessage(config.systemPrompt);
  }
  if (existingMessages.length > 0) {
    messageHistory.initializeHistory(existingMessages);
  }
}
```

## 二、架构问题诊断

### 2.1 消息管理不统一

```
问题：两个模块各自实现消息管理

Graph:                     Agent-Loop:
  ConversationManager        MessageHistoryManager (Agent版)
  extends MessageHistory     独立实现
  Token跟踪                  无Token跟踪
  工具描述消息               无工具描述消息
  批次管理                   无批次管理
```

**影响**：
- 代码重复
- 行为不一致
- 难以维护

### 2.2 系统提示词处理不一致

```
Graph:                          Agent-Loop:
  - 支持 templateId              - 仅支持字符串
  - 在节点配置中解析             - 在初始化时注入
  - 可使用预定义模板             - 直接使用config.systemPrompt
```

**影响**：
- 用户体验不一致
- 配置方式不同
- 模板系统无法共享

### 2.3 临时vs持久化消息历史

**Agent-Loop的双层架构**：
- ✅ 优点：支持checkpoint（临时历史可丢弃）
- ❌ 缺点：需要同步，增加复杂度

**Graph的单层架构**：
- ✅ 优点：简单直接
- ⚠️ 问题：批次管理实现复杂（markMap）

## 三、正确的架构设计

### 3.1 核心原则

1. **有状态组件维护上下文**：Thread/AgentLoopEntity 维护完整消息历史
2. **执行节点只负责尾插**：LLM Node/Iteration 只添加新的 user 消息
3. **系统提示词统一处理**：通过预定义模板机制统一注入
4. **消息管理统一**：使用统一的 MessageHistory/ConversationManager

### 3.2 推荐的职责分离

```
┌─────────────────────────────────────────────────────────────────┐
│  有状态实体层（Stateful）                                         │
│  ┌─────────────────────┐  ┌─────────────────────┐              │
│  │   ThreadEntity      │  │ AgentLoopEntity     │              │
│  │   ───────────       │  │ ─────────────────   │              │
│  │   维护消息历史      │  │ 维护消息历史        │              │
│  │   管理执行状态      │  │ 管理执行状态        │              │
│  │   提供checkpoint    │  │ 提供checkpoint      │              │
│  └─────────┬───────────┘  └─────────┬───────────┘              │
└────────────┼────────────────────────┼──────────────────────────┘
             │                        │
             ▼                        ▼
┌─────────────────────────────────────────────────────────────────┐
│  消息管理层（Message Management）                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │         ConversationManager（统一）                      │   │
│  │  ┌─────────────────────────────────────────────────┐   │   │
│  │  │  职责：                                         │   │   │
│  │  │  1. 消息存储（MessageHistory基类）              │   │   │
│  │  │  2. Token跟踪                                   │   │   │
│  │  │  3. 批次管理（上下文压缩）                      │   │   │
│  │  │  4. 工具描述消息管理                            │   │   │
│  │  └─────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────────┐
│  执行层（Execution）                                             │
│  ┌─────────────────────┐  ┌─────────────────────┐              │
│  │   LLM Node          │  │ Agent Iteration     │              │
│  │   ────────          │  │ ───────────────     │              │
│  │   从实体获取消息    │  │ 从实体获取消息      │              │
│  │   添加user消息      │  │ 添加user消息        │              │
│  │   调用LLM           │  │ 调用LLM             │              │
│  │   添加assistant消息 │  │ 添加assistant消息   │              │
│  └─────────────────────┘  └─────────────────────┘              │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 系统提示词正确位置

根据 `LLMMessage` 定义，系统提示词应作为消息数组的一部分：

```typescript
// ✅ 正确：系统提示词是消息数组的第一个元素
const messages: LLMMessage[] = [
  { role: 'system', content: systemPrompt },    // ← 系统提示词
  { role: 'user', content: userPrompt },        // ← 用户输入
  { role: 'assistant', content: '...' },        // ← 历史响应
  { role: 'tool', toolCallId: '...', content: '...' },  // ← 工具结果
  // ...
];
```

**多层系统提示词（可选）**：

```typescript
// 如果需要在不同阶段注入系统提示词
const messages: LLMMessage[] = [
  { role: 'system', content: baseSystemPrompt },      // 基础系统提示词
  { role: 'system', content: toolDescription },       // 工具描述（动态）
  { role: 'system', content: contextRules },          // 上下文规则（动态）
  { role: 'user', content: userPrompt },
  // ...
];
```

### 3.4 初始消息处理

**初始化流程**：

```typescript
// 1. 创建有状态实体时，可选传入初始消息
const entity = new ThreadEntity({
  initialMessages: [
    { role: 'system', content: systemPrompt },
    { role: 'user', content: initialUserMessage }
  ]
});

// 2. 或者通过方法设置
entity.setMessages([
  { role: 'system', content: systemPrompt },
  ...historyMessages
]);

// 3. 执行时自动包含初始消息
// Node执行时从entity获取消息历史，包含初始消息
```

**配置方式统一**：

```typescript
// 统一的配置接口
interface ExecutionConfig {
  // 系统提示词（可选，支持多种方式）
  systemPrompt?: string;                    // 直接字符串
  systemPromptTemplateId?: string;          // 模板ID
  systemPromptTemplateVariables?: object;   // 模板变量
  
  // 初始用户消息（可选）
  initialUserMessage?: string;
  initialUserMessageTemplateId?: string;
  
  // 或完整初始消息数组
  initialMessages?: LLMMessage[];
}
```

## 四、具体改进建议

### 4.1 统一消息管理器

**目标**：Graph和Agent-Loop使用统一的消息管理器

```typescript
// 删除 Agent-Loop 的独立 MessageHistoryManager
// 统一使用 ConversationManager

// sdk/core/managers/conversation-manager.ts（已存在）
// 保持当前实现，确保Agent-Loop可以使用
```

**修改点**：
1. Agent-Loop的 `MessageHistoryManager` 改为使用 `ConversationManager`
2. 或让 `AgentLoopEntity` 直接持有 `ConversationManager` 实例

### 4.2 统一系统提示词处理

**目标**：Graph和Agent-Loop使用相同的系统提示词解析逻辑

```typescript
// sdk/core/prompt/system-prompt-resolver.ts
export function resolveSystemPrompt(config: SystemPromptConfig): string {
  // 1. 优先使用 templateId
  if (config.systemPromptTemplateId) {
    const template = getTemplate(config.systemPromptTemplateId);
    if (template) {
      return renderTemplate(template, config.systemPromptTemplateVariables);
    }
  }
  
  // 2. 回退到直接字符串
  if (config.systemPrompt) {
    return config.systemPrompt;
  }
  
  // 3. 使用默认模板
  return getTemplate('system.assistant')?.content || '';
}
```

### 4.3 分离执行实例和消息管理

**当前问题**：
- Agent-Loop的 `message-history-helper` 功能单一
- Graph的 `MessageHistoryManager` 继承层次复杂

**改进方案**：

```typescript
// 1. 有状态实体只负责持有消息管理器引用
class AgentLoopEntity {
  conversationManager: ConversationManager;  // 不自己实现
  
  constructor(config: AgentLoopConfig) {
    this.conversationManager = new ConversationManager({
      initialMessages: buildInitialMessages(config)
    });
  }
}

// 2. 提取初始消息构建逻辑
function buildInitialMessages(config: AgentLoopConfig): LLMMessage[] {
  const messages: LLMMessage[] = [];
  
  // 添加系统提示词
  const systemPrompt = resolveSystemPrompt(config);
  if (systemPrompt) {
    messages.push({ role: 'system', content: systemPrompt });
  }
  
  // 添加初始用户消息
  if (config.initialUserMessage) {
    messages.push({ role: 'user', content: config.initialUserMessage });
  }
  
  // 或添加历史消息
  if (config.initialMessages) {
    messages.push(...config.initialMessages);
  }
  
  return messages;
}
```

### 4.4 LLM Node职责明确

**LLM Node应该**：
1. 从有状态实体获取当前消息历史
2. 添加当前节点的用户提示词（尾插）
3. 调用LLM执行
4. 将响应添加回消息历史

**LLM Node不应该**：
1. ❌ 自己创建消息历史
2. ❌ 维护完整上下文
3. ❌ 处理系统提示词注入（由实体初始化时处理）

```typescript
// ✅ 正确的LLM Node实现
export async function llmHandler(
  thread: Thread,
  node: Node,
  context: LLMHandlerContext
) {
  const config = node.config as LLMNodeConfig;
  
  // 1. 获取当前消息历史（从有状态实体）
  const conversationManager = context.conversationManager;
  
  // 2. 添加当前节点的用户消息（尾插）
  const userPrompt = resolvePrompt(config);
  conversationManager.addUserMessage(userPrompt);
  
  // 3. 调用LLM（传入完整消息数组）
  const result = await context.llmCoordinator.executeLLM({
    messages: conversationManager.getMessages(),
    ...
  });
  
  // 4. 添加助手响应
  conversationManager.addAssistantMessage(result.content, result.toolCalls);
  
  return result;
}
```

## 五、总结

### 5.1 当前架构的优点

1. ✅ **有状态组件维护上下文**：ThreadEntity和AgentLoopEntity正确维护了消息历史
2. ✅ **LLM Node只负责尾插**：节点只添加新的用户消息，不管理完整历史
3. ✅ **消息定义清晰**：LLMMessage接口定义完善
4. ✅ **批次管理支持**：Graph的MessageHistory支持上下文压缩

### 5.2 需要改进的地方

1. ⚠️ **消息管理器不统一**：Graph和Agent-Loop各自实现
2. ⚠️ **系统提示词处理不一致**：template支持程度不同
3. ⚠️ **初始化逻辑分散**：初始化消息构建逻辑分散在各处
4. ⚠️ **提示词模板系统孤立**：预定义模板未被充分利用

### 5.3 推荐实施顺序

| 优先级 | 任务 | 影响范围 |
|--------|------|----------|
| P0 | 统一系统提示词解析逻辑 | Agent + Graph |
| P1 | Agent-Loop使用ConversationManager | Agent |
| P2 | 提取初始消息构建工具函数 | Agent + Graph |
| P3 | 统一提示词模板系统 | 全局 |

### 5.4 架构验证标准

改进后的架构应该满足：

1. ✅ Graph和Agent-Loop使用相同的系统提示词处理逻辑
2. ✅ 两者使用统一的消息管理器（ConversationManager）
3. ✅ 有状态实体（Thread/AgentLoop）维护完整消息历史
4. ✅ 执行节点（LLM Node/Iteration）只负责尾插新消息
5. ✅ 系统提示词作为消息数组的第一个元素
6. ✅ 提示词模板系统被充分利用
