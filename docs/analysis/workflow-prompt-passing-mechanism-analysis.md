# Workflow 提示词传递机制分析与改进建议

## 📊 当前架构分析

### 1. 核心组件关系图

```
┌─────────────────────────────────────────────────────────────┐
│                    Workflow Execution                        │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │         ConversationSession (消息管理器)              │   │
│  │  - 继承自 MessageHistory                              │   │
│  │  - 管理完整的消息历史（可见 + 不可见）                 │   │
│  │  - Token 统计与限制                                   │   │
│  │  - Batch 可见性控制                                   │   │
│  └──────────────────────────────────────────────────────┘   │
│                           ↑                                  │
│                           │ 共享同一个实例                    │
│  ┌──────────────────────────────────────────────────────┐   │
│  │     WorkflowExecutionEntity.messageHistoryManager     │   │
│  │  - 管理可见消息（用于 LLM 调用）                       │   │
│  │  - 通过 WorkflowStateCoordinator 同步                 │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                           ↓
              ┌────────────────────────┐
              │   Node Handlers        │
              ├────────────────────────┤
              │ • LLM Handler          │
              │ • Agent Loop Handler   │
              └────────────────────────┘
```

### 2. 当前提示词传递流程

#### **LLM 节点流程**

```typescript
// llm-handler.ts
export async function llmHandler(workflowExecution, node, context) {
  const config = node.config as LLMNodeConfig;
  
  // 1. 解析 prompt（从配置或模板）
  const prompt = resolvePrompt(config);  // ← 仅字符串
  
  // 2. 调用 LLMExecutionCoordinator
  await context.llmCoordinator.executeLLM(
    {
      executionId: workflowExecution.id,
      nodeId: node.id,
      prompt: prompt,  // ← 传入字符串
      profileId: config.profileId,
      parameters: config.parameters,
      maxToolCallsPerRequest: config.maxToolCallsPerRequest,
    },
    context.conversationManager,  // ← 传入共享的 ConversationSession
  );
}

// llm-execution-coordinator.ts (core)
async executeLLMLoop(params, conversationState) {
  // 1. 将 prompt 作为 user message 添加到 conversation
  const userMessage = { role: "user", content: prompt };
  conversationState.addMessage(userMessage);  // ← 自动追加到历史
  
  // 2. 获取完整消息历史（包含之前所有节点的消息）
  const messages = conversationState.getMessages();
  
  // 3. 调用 LLM
  await this.llmExecutor.executeLLMCall(messages, ...);
}
```

**关键点：**
- ✅ LLM 节点**自动继承**之前的对话上下文
- ✅ 通过 `ConversationSession` 共享消息历史
- ❌ 只能通过 `prompt` 字符串添加新的 user message
- ❌ 无法直接传入完整的 message 数组

#### **Agent Loop 节点流程**

```typescript
// agent-loop-handler.ts
export async function agentLoopHandler(globalContext, execution, node, context) {
  const config = node.config as AgentLoopNodeConfig;
  
  // 1. 准备初始消息（从 workflow 变量）
  const initialMessages: LLMMessage[] = [];
  const inputPrompt = execution.variableScopes?.workflowExecution?.["input"] 
                   || execution.variableScopes?.workflowExecution?.["prompt"];
  
  if (inputPrompt && typeof inputPrompt === "string") {
    initialMessages.push({ role: "user", content: inputPrompt });  // ← 仅支持字符串
  }
  
  // 2. 创建 AgentLoopCoordinator
  const coordinator = createCoordinator(globalContext, context);
  
  // 3. 执行 Agent Loop（独立的 ConversationSession）
  await coordinator.execute(
    {
      profileId: config.profileId,
      systemPrompt: resolveSystemPrompt(config),
      initialMessages,  // ← 传入消息数组，但仅包含当前节点的输入
      availableTools: config.availableTools,
      maxIterations: config.maxIterations,
    },
    {
      conversationManager: context.conversationManager,  // ← 共享父级 session
      parentExecutionId: execution.id,
      nodeId: node.id,
    },
  );
}
```

**关键点：**
- ⚠️ Agent Loop 有**独立的** `AgentLoopEntity.conversationManager`
- ⚠️ 通过 `parentExecutionId` 建立父子关系
- ❌ `initialMessages` 只能从 workflow 变量中提取，无法继承完整上下文
- ❌ 不支持直接传入 message 数组来继承之前的对话历史

### 3. 消息继承机制对比

| 特性 | LLM 节点 | Agent Loop 节点 |
|------|---------|----------------|
| **消息管理器** | 共享 Workflow 的 `ConversationSession` | 创建独立的 `AgentLoopEntity.conversationManager` |
| **上下文继承** | ✅ 自动继承（通过共享 session） | ❌ 不自动继承（需要手动构建 initialMessages） |
| **输入方式** | 仅支持 `prompt` 字符串 | 支持 `initialMessages` 数组，但来源受限 |
| **消息隔离** | ❌ 无隔离，所有节点共享同一历史 | ✅ 有隔离，Agent Loop 内部消息独立 |
| **适用场景** | 简单单次 LLM 调用 | 复杂的多轮工具调用循环 |

---

## 🔍 问题分析

### 问题 1：LLM 节点无法灵活控制上下文

**现状：**
```typescript
// 当前只能这样使用
const config: LLMNodeConfig = {
  profileId: "gpt-4",
  prompt: "请分析这段代码",  // ← 只能是字符串
};
```

**问题：**
- 无法传入完整的 message 数组
- 无法精确控制哪些历史消息应该被包含
- 无法在节点级别进行消息过滤、截断等操作
- 必须依赖前面的 `CONTEXT_PROCESSOR` 节点来管理消息

**实际影响：**
```toml
# 需要额外的 CONTEXT_PROCESSOR 节点来准备上下文
[[nodes]]
id = "prepare-context"
type = "CONTEXT_PROCESSOR"
[nodes.config]
operationConfig.operation = "truncate"
operationConfig.count = 10

[[nodes]]
id = "llm-node"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
prompt = "基于上面的对话，请总结"  # ← 无法指定具体使用哪些消息
```

### 问题 2：Agent Loop 节点上下文继承断裂

**现状：**
```typescript
// Agent Loop handler 中
const inputPrompt = execution.variableScopes?.workflowExecution?.["input"];
if (inputPrompt && typeof inputPrompt === "string") {
  initialMessages.push({ role: "user", content: inputPrompt });
}
// ← 只提取了一个字符串，丢失了完整的对话历史
```

**问题：**
- Agent Loop 无法访问 workflow 的完整对话历史
- 即使传入了 `conversationManager`，也只是用于事件同步，不是用于继承上下文
- `initialMessages` 的构建逻辑过于简单，只支持单个字符串

**实际影响：**
```toml
# Workflow 中有多个 LLM 节点对话
[[nodes]]
id = "llm-1"
type = "LLM"
[nodes.config]
prompt = "第一步：分析问题"

[[nodes]]
id = "llm-2"
type = "LLM"
[nodes.config]
prompt = "第二步：深入思考"

[[nodes]]
id = "agent-loop"
type = "AGENT_LOOP"
[nodes.config]
# ← 这里无法继承 llm-1 和 llm-2 的对话历史
# 只能从变量中获取一个简单的字符串
```

### 问题 3：简单提示词场景几乎不存在

**您的观点完全正确！** 

在实际生产场景中：
1. **单轮对话极少**：大多数任务需要多轮交互
2. **上下文至关重要**：LLM 需要理解之前的对话才能给出准确回答
3. **消息管理复杂**：需要截断、过滤、压缩等高级操作

**典型场景示例：**

```toml
# 场景 1：代码审查工作流
[[nodes]]
id = "read-code"
type = "SCRIPT"

[[nodes]]
id = "analyze-issues"
type = "LLM"
[nodes.config]
prompt = "分析代码中的潜在问题"

[[nodes]]
id = "suggest-fixes"
type = "AGENT_LOOP"  # ← 需要多轮工具调用来修复问题
[nodes.config]
# ❌ 当前设计：无法继承 analyze-issues 的分析结果和上下文
# ✅ 理想设计：应该能传入完整的 message 数组
```

```toml
# 场景 2：研究助手
[[nodes]]
id = "search-web"
type = "SCRIPT"

[[nodes]]
id = "summarize"
type = "LLM"
[nodes.config]
prompt = "总结搜索结果"

[[nodes]]
id = "deep-research"
type = "AGENT_LOOP"  # ← 需要基于总结进行深入研究
[nodes.config]
# ❌ 当前设计：丢失了 search-web 和 summarize 的完整上下文
```

---

## 💡 改进方案：统一 Message Array 接口

### 核心设计理念

> **"所有 LLM 相关节点都应该支持直接传入 message 数组，以实现灵活的上下文继承"**

### 方案 A：扩展 LLMNodeConfig（推荐）

```typescript
export interface LLMNodeConfig {
  profileId: ID;
  
  /**
   * Prompt 字符串（向后兼容，优先级低于 messages）
   */
  prompt?: string;
  
  /**
   * Prompt 模板 ID
   */
  promptTemplateId?: string;
  promptTemplateVariables?: Record<string, unknown>;
  
  /**
   * 【新增】直接传入消息数组
   * 
   * 当提供此字段时：
   * - 忽略 prompt 和 promptTemplateId
   * - 直接使用这些消息作为 LLM 输入
   * - 可以精确控制包含哪些历史消息
   * - 支持在节点级别进行消息过滤、重组等操作
   * 
   * 消息来源可以是：
   * - 工作流变量：{{workflowExecution.messages}}
   * - 前序节点输出：{{node.output.messages}}
   * - 动态构建：通过 SCRIPT 节点预处理
   */
  messages?: LLMMessage[];
  
  /**
   * 【新增】消息选择策略
   * 
   * 当未提供 messages 时，如何从 ConversationSession 中选择消息：
   * - 'all': 使用所有可见消息（默认）
   * - 'recent': 仅使用最近 N 条消息
   * - 'custom': 使用自定义过滤函数（需要配合 transformContext）
   */
  messageSelection?: {
    strategy: 'all' | 'recent' | 'custom';
    count?: number;  // 当 strategy='recent' 时使用
    filterFn?: (messages: LLMMessage[]) => LLMMessage[];  // 当 strategy='custom' 时使用
  };
  
  parameters?: Record<string, unknown>;
  maxToolCallsPerRequest?: number;
}
```

**使用示例：**

```toml
# 示例 1：直接使用变量中的消息数组
[[nodes]]
id = "llm-with-context"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
# 从工作流变量中获取消息数组
messages = "{{workflowExecution.recentMessages}}"

# 示例 2：使用最近 N 条消息
[[nodes]]
id = "llm-recent-only"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
messageSelection.strategy = "recent"
messageSelection.count = 5

# 示例 3：向后兼容（仍然使用 prompt 字符串）
[[nodes]]
id = "llm-simple"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
prompt = "简单的问题"
```

### 方案 B：扩展 AgentLoopNodeConfig

```typescript
export interface AgentLoopNodeConfig {
  agentLoopId?: ID;
  
  inlineConfig?: {
    profileId: string;
    maxIterations?: number;
    availableTools?: AvailableTools;
    systemPrompt?: string;
    systemPromptTemplateId?: string;
    systemPromptTemplateVariables?: Record<string, unknown>;
    
    /**
     * 【新增】初始消息数组
     * 
     * 替代当前的 initialMessages 构建逻辑
     * 支持从工作流上下文中继承完整的对话历史
     */
    initialMessages?: LLMMessage[];
    
    /**
     * 【新增】是否继承父级 workflow 的对话历史
     * 
     * true: 将 workflow 的 ConversationSession 中的所有消息作为初始上下文
     * false: 仅使用 initialMessages（默认）
     */
    inheritWorkflowContext?: boolean;
  };
}
```

**使用示例：**

```toml
# 示例 1：继承完整 workflow 上下文
[[nodes]]
id = "agent-with-full-context"
type = "AGENT_LOOP"
[nodes.config.inlineConfig]
profileId = "gpt-4"
inheritWorkflowContext = true
maxIterations = 10

# 示例 2：使用指定的消息数组
[[nodes]]
id = "agent-with-custom-messages"
type = "AGENT_LOOP"
[nodes.config.inlineConfig]
profileId = "gpt-4"
initialMessages = "{{workflowExecution.filteredMessages}}"
maxIterations = 5

# 示例 3：组合使用
[[nodes]]
id = "agent-hybrid"
type = "AGENT_LOOP"
[nodes.config.inlineConfig]
profileId = "gpt-4"
inheritWorkflowContext = true  # 先继承所有历史
initialMessages = "{{additionalContext}}"  # 再添加额外上下文
```

### 方案 C：统一的 Context Builder 模式（最灵活）

引入一个新的配置类型，允许在节点级别定义上下文构建逻辑：

```typescript
/**
 * 上下文构建器配置
 * 允许灵活定义如何为 LLM/Agent 节点构建消息上下文
 */
export interface ContextBuilderConfig {
  /**
   * 上下文来源
   */
  sources: Array<{
    /** 来源类型 */
    type: 'workflow_history' | 'variable' | 'node_output' | 'static';
    
    /** 来源标识（变量名、节点 ID 等） */
    sourceId?: string;
    
    /** 过滤条件 */
    filter?: {
      /** 消息角色过滤 */
      roles?: ('user' | 'assistant' | 'system' | 'tool')[];
      
      /** 时间范围 */
      timeRange?: {
        from?: Timestamp;
        to?: Timestamp;
      };
      
      /** 最大消息数 */
      maxCount?: number;
      
      /** 自定义过滤表达式 */
      expression?: string;
    };
    
    /** 转换函数（可选） */
    transform?: {
      /** 截断策略 */
      truncate?: 'head' | 'tail' | 'middle';
      
      /** 截断数量 */
      truncateCount?: number;
      
      /** 是否压缩 */
      compress?: boolean;
    };
  }>;
  
  /**
   * 合并策略
   */
  mergeStrategy?: 'append' | 'prepend' | 'replace';
}

// 在 LLMNodeConfig 中使用
export interface LLMNodeConfig {
  profileId: ID;
  prompt?: string;
  promptTemplateId?: string;
  
  /**
   * 【新增】上下文构建器配置
   * 当提供时，忽略 prompt，使用此配置构建消息上下文
   */
  contextBuilder?: ContextBuilderConfig;
  
  parameters?: Record<string, unknown>;
  maxToolCallsPerRequest?: number;
}
```

**使用示例：**

```toml
# 复杂场景：从多个来源构建上下文
[[nodes]]
id = "llm-complex-context"
type = "LLM"
[nodes.config]
profileId = "gpt-4"

# 定义上下文构建规则
[nodes.config.contextBuilder]
mergeStrategy = "append"

# 来源 1：最近 5 条 workflow 历史
[[nodes.config.contextBuilder.sources]]
type = "workflow_history"
[nodes.config.contextBuilder.sources.filter]
maxCount = 5
roles = ["user", "assistant"]

# 来源 2：特定变量的消息
[[nodes.config.contextBuilder.sources]]
type = "variable"
sourceId = "research_summary"

# 来源 3：静态系统消息
[[nodes.config.contextBuilder.sources]]
type = "static"
[nodes.config.contextBuilder.sources.transform]
truncate = "tail"
truncateCount = 1
```

---

## 🎯 推荐实施方案

### 阶段 1：快速改进（立即实施）

**目标：** 解决最紧迫的上下文继承问题

1. **扩展 LLMNodeConfig**
   ```typescript
   export interface LLMNodeConfig {
     // ... 现有字段
     
     /** 直接传入消息数组（优先级高于 prompt） */
     messages?: LLMMessage[];
   }
   ```

2. **扩展 AgentLoopNodeConfig**
   ```typescript
   export interface AgentLoopNodeConfig {
     inlineConfig?: {
       // ... 现有字段
       
       /** 初始消息数组 */
       initialMessages?: LLMMessage[];
       
       /** 是否继承 workflow 上下文 */
       inheritWorkflowContext?: boolean;
     };
   }
   ```

3. **更新 Handler 实现**
   - `llm-handler.ts`: 优先使用 `config.messages`，否则回退到 `prompt`
   - `agent-loop-handler.ts`: 根据 `inheritWorkflowContext` 决定是否复制 workflow 消息

### 阶段 2：增强灵活性（中期规划）

**目标：** 提供更强大的上下文管理能力

1. **引入 Context Processor 增强功能**
   - 支持更复杂的消息过滤、转换操作
   - 支持将处理结果保存到变量供后续节点使用

2. **引入消息模板系统**
   - 允许定义可复用的消息模板
   - 支持动态渲染消息内容

### 阶段 3：统一抽象（长期愿景）

**目标：** 实现完全的上下文管理抽象

1. **引入 ContextBuilder 模式**
   - 统一的上下文构建接口
   - 声明式的上下文配置

2. **引入消息管道（Message Pipeline）**
   - 类似 Unix pipe 的消息处理链
   - 支持插件化的消息转换器

---

## 📝 实施细节

### 1. 类型定义修改

```typescript
// packages/types/src/node/configs/execution-configs.ts
export interface LLMNodeConfig {
  profileId: ID;
  prompt?: string;
  promptTemplateId?: string;
  promptTemplateVariables?: Record<string, unknown>;
  
  /** 【新增】直接传入消息数组 */
  messages?: LLMMessage[];
  
  parameters?: Record<string, unknown>;
  maxToolCallsPerRequest?: number;
}

// packages/types/src/node/configs/agent-loop-configs.ts
export interface AgentLoopNodeConfig {
  agentLoopId?: ID;
  
  inlineConfig?: {
    profileId: string;
    maxIterations?: number;
    availableTools?: AvailableTools;
    systemPrompt?: string;
    systemPromptTemplateId?: string;
    systemPromptTemplateVariables?: Record<string, unknown>;
    
    /** 【新增】初始消息数组 */
    initialMessages?: LLMMessage[];
    
    /** 【新增】是否继承 workflow 上下文 */
    inheritWorkflowContext?: boolean;
  };
}
```

### 2. Handler 实现修改

```typescript
// sdk/workflow/execution/handlers/node-handlers/llm-handler.ts
export async function llmHandler(workflowExecution, node, context) {
  const config = node.config as LLMNodeConfig;
  
  // 1. 确定输入消息
  let messagesToUse: LLMMessage[];
  
  if (config.messages && config.messages.length > 0) {
    // 优先使用直接传入的消息数组
    messagesToUse = config.messages;
  } else {
    // 回退到 prompt 字符串（向后兼容）
    const prompt = resolvePrompt(config);
    messagesToUse = [{ role: "user", content: prompt }];
  }
  
  // 2. 将消息添加到 conversation（如果尚未存在）
  for (const msg of messagesToUse) {
    if (!context.conversationManager.getAllMessages().includes(msg)) {
      context.conversationManager.addMessage(msg);
    }
  }
  
  // 3. 执行 LLM
  const result = await context.llmCoordinator.executeLLM(
    {
      executionId: workflowExecution.id,
      nodeId: node.id,
      prompt: "",  // 不再使用，消息已通过 conversation 管理
      profileId: config.profileId,
      parameters: config.parameters,
      maxToolCallsPerRequest: config.maxToolCallsPerRequest,
    },
    context.conversationManager,
  );
  
  return result;
}
```

```typescript
// sdk/workflow/execution/handlers/node-handlers/agent-loop-handler.ts
export async function agentLoopHandler(globalContext, execution, node, context) {
  const config = node.config as AgentLoopNodeConfig;
  
  // 1. 构建初始消息
  let initialMessages: LLMMessage[] = [];
  
  if (config.inlineConfig) {
    // 如果配置了继承 workflow 上下文
    if (config.inlineConfig.inheritWorkflowContext) {
      // 复制 workflow 的所有可见消息
      const workflowMessages = context.conversationManager.getMessages();
      initialMessages = [...workflowMessages];
    }
    
    // 如果配置了初始消息数组，追加到后面
    if (config.inlineConfig.initialMessages) {
      initialMessages.push(...config.inlineConfig.initialMessages);
    }
    
    // 如果没有配置任何消息，回退到从变量中提取（向后兼容）
    if (initialMessages.length === 0) {
      const inputPrompt = execution.variableScopes?.workflowExecution?.["input"];
      if (inputPrompt && typeof inputPrompt === "string") {
        initialMessages.push({ role: "user", content: inputPrompt });
      }
    }
  }
  
  // 2. 执行 Agent Loop
  const coordinator = createCoordinator(globalContext, context);
  const result = await coordinator.execute(
    {
      profileId: config.inlineConfig?.profileId,
      systemPrompt: resolveSystemPrompt(config),
      initialMessages,
      availableTools: config.inlineConfig?.availableTools,
      maxIterations: config.inlineConfig?.maxIterations,
    },
    {
      conversationManager: context.conversationManager,
      parentExecutionId: execution.id,
      nodeId: node.id,
    },
  );
  
  return result;
}
```

### 3. Schema 验证更新

```typescript
// packages/types/src/node/configs/execution-configs-schema.ts
import { z } from "zod";
import { LLMMessageSchema } from "../../message/index.js";

export const LLMNodeConfigSchema = z.object({
  profileId: z.string(),
  prompt: z.string().optional(),
  promptTemplateId: z.string().optional(),
  promptTemplateVariables: z.record(z.unknown()).optional(),
  
  // 【新增】消息数组
  messages: z.array(LLMMessageSchema).optional(),
  
  parameters: z.record(z.unknown()).optional(),
  maxToolCallsPerRequest: z.number().int().positive().optional(),
});
```

---

## ✅ 优势总结

### 改进后的优势

1. **✅ 灵活的上下文控制**
   - 可以精确指定使用哪些消息
   - 支持从多个来源组合消息
   - 节点级别的上下文隔离或共享

2. **✅ 向后兼容**
   - 现有的 `prompt` 字符串方式仍然有效
   - 渐进式迁移，无需一次性重构所有工作流

3. **✅ 符合实际场景**
   - 支持复杂的多轮对话场景
   - 适应需要上下文继承的工作流
   - 满足生产环境的真实需求

4. **✅ 统一的设计**
   - LLM 节点和 Agent Loop 节点采用相似的接口
   - 降低学习成本
   - 提高代码可维护性

5. **✅ 可扩展性**
   - 为未来的 ContextBuilder 模式奠定基础
   - 支持更高级的上下文管理功能

---

## 🚀 下一步行动

1. **立即开始**：实施方案 A 的类型定义修改
2. **更新 Handler**：修改 `llm-handler.ts` 和 `agent-loop-handler.ts`
3. **添加测试**：编写单元测试验证新功能
4. **更新文档**：补充使用示例和最佳实践
5. **收集反馈**：在实际工作流中验证效果

您觉得这个分析和改进方案如何？是否需要我立即开始实施方案 A？
