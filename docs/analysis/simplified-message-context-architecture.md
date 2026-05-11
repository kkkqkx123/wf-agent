# 简化版多上下文管理架构

## 🎯 核心设计理念

### 问题识别

1. **模板系统过于复杂**
   - `prompt` + `promptTemplateId` + `promptTemplateVariables` 三层配置
   - 需要序列化/反序列化，增加开销
   - 继承逻辑（`inheritWorkflowContext`）难以理解和维护

2. **字符串传递效率低**
   - Prompt 需要序列化为字符串再传递给 LLM
   - 无法直接操作消息结构
   - 丢失了消息的元数据（role、metadata 等）

3. **配置冗余**
   - 既要支持 prompt 字符串，又要支持 messages 数组
   - 多种配置方式导致逻辑分支复杂

### 新设计原则

> **"一切皆消息，直接传递 Message[]，消除模板和继承"**

```
传统方式:
┌─────────────┐     模板渲染      ┌──────────┐     序列化      ┌──────┐
│ Config      │ ───────────────→ │ Prompt   │ ─────────────→ │ LLM  │
│ (template)  │                  │ (string) │                │      │
└─────────────┘                  └──────────┘                └──────┘

新方式:
┌─────────────┐                  ┌──────────────┐
│ Context     │ ──生成──→        │ NamedContext │
│ Processor   │   "current"      │              │
└─────────────┘                  └──────┬───────┘
                                        │ 直接引用
                                        ↓
                                ┌──────────────┐
                                │ LLM Node     │
                                │ messages =   │
                                │ ["current"]  │
                                └──────────────┘
```

---

## 📐 简化后的架构

### 1. 命名上下文系统（保持不变）

```typescript
/**
 * 命名消息上下文
 */
export interface NamedMessageContext {
  /** 语义化ID */
  id: string;
  
  /** 消息数组 */
  messages: LLMMessage[];
  
  /** 创建时间戳 */
  createdAt: Timestamp;
  
  /** 最后更新时间戳 */
  updatedAt: Timestamp;
  
  /** 元数据 */
  metadata?: {
    description?: string;
    sourceNodeId?: ID;
    tags?: string[];
    tokenCount?: number;
  };
}

/**
 * 消息上下文注册表
 */
export interface MessageContextRegistry {
  register(context: NamedMessageContext): void;
  get(id: string): NamedMessageContext | undefined;
  update(id: string, messages: LLMMessage[]): void;
  delete(id: string): void;
  listIds(): string[];
  has(id: string): boolean;
}
```

### 2. 内置特殊上下文ID

为了简化使用，定义几个**保留的语义化ID**：

```typescript
/**
 * 内置的特殊上下文ID
 */
export const BUILTIN_CONTEXT_IDS = {
  /** 
   * 当前执行实例的主对话上下文
   * 
   * 自动维护，包含所有历史消息
   */
  CURRENT: 'current',
  
  /** 
   * 系统指令上下文
   * 
   * 通常包含 system role 的消息
   */
  SYSTEM: 'system',
  
  /** 
   * 临时上下文（用于中间结果）
   * 
   * 可以被任何节点覆盖
   */
  TEMP: 'temp',
} as const;
```

**关键特性：**
- ✅ `current` 是默认的主对话上下文，无需显式创建
- ✅ 所有节点默认操作 `current`，除非特别指定
- ✅ 其他命名上下文由 Context Processor 显式创建

### 3. 简化的节点配置

#### A. LLM 节点（极简版）

```typescript
export interface LLMNodeConfig {
  profileId: ID;
  
  /**
   * 【唯一输入方式】消息上下文引用
   * 
   * - 可以是内置ID（如 'current', 'system'）
   * - 可以是自定义ID（由 Context Processor 创建）
   * - 支持多个上下文，按顺序合并
   */
  contextRefs: string[];
  
  /**
   * 可选：是否将LLM响应追加到指定上下文
   * 
   * 默认为 'current'
   */
  outputContext?: string;
  
  parameters?: Record<string, unknown>;
  maxToolCallsPerRequest?: number;
}
```

**使用示例：**

```toml
# 示例 1：最简单的用法（使用 current）
[[nodes]]
id = "llm-simple"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["current"]  # 使用当前主对话

# 示例 2：合并多个上下文
[[nodes]]
id = "llm-merged"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["system", "research-notes", "current"]
# 按顺序拼接：system → research-notes → current

# 示例 3：输出到特定上下文
[[nodes]]
id = "llm-with-output"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["current"]
outputContext = "analysis-result"  # LLM响应保存到 analysis-result
```

**Handler 实现：**

```typescript
export async function llmHandler(
  workflowExecution: WorkflowExecution,
  node: Node,
  context: LLMHandlerContext,
): Promise<LLMExecutionResult> {
  const config = node.config as LLMNodeConfig;
  
  // 1. 收集所有引用的上下文消息
  let allMessages: LLMMessage[] = [];
  
  for (const contextId of config.contextRefs) {
    const namedContext = context.messageContextRegistry?.get(contextId);
    
    if (!namedContext) {
      throw new RuntimeValidationError(
        `Context '${contextId}' not found. Available contexts: ${
          context.messageContextRegistry?.listIds().join(', ') || 'none'
        }`
      );
    }
    
    allMessages.push(...namedContext.messages);
  }
  
  // 2. 直接使用消息数组调用 LLM（无需序列化）
  const result = await context.llmCoordinator.executeLLM(
    {
      executionId: workflowExecution.id,
      nodeId: node.id,
      messages: allMessages,  // ← 直接传递 Message[]
      profileId: config.profileId,
      parameters: config.parameters,
      maxToolCallsPerRequest: config.maxToolCallsPerRequest,
    },
    context.conversationManager,
  );
  
  // 3. 如果指定了输出上下文，保存响应
  if (config.outputContext && result.responseMessage) {
    const outputContext = context.messageContextRegistry?.get(config.outputContext);
    
    if (outputContext) {
      outputContext.messages.push(result.responseMessage);
      outputContext.updatedAt = now();
    } else {
      // 自动创建新的上下文
      context.messageContextRegistry?.register({
        id: config.outputContext,
        messages: [result.responseMessage],
        createdAt: now(),
        updatedAt: now(),
      });
    }
  } else {
    // 默认追加到 current
    const currentContext = context.messageContextRegistry?.get('current');
    if (currentContext && result.responseMessage) {
      currentContext.messages.push(result.responseMessage);
      currentContext.updatedAt = now();
    }
  }
  
  return result;
}
```

#### B. Context Processor 节点（增强版）

```typescript
export interface ContextProcessorNodeConfig {
  /** 消息操作配置 */
  operationConfig: MessageOperationConfig;
  
  /**
   * 【新增】源上下文ID
   * 
   * - 不指定：默认从 'current' 读取
   * - 指定：从指定的命名上下文读取
   */
  sourceContext?: string;
  
  /**
   * 【新增】目标上下文ID
   * 
   * - 不指定：默认写入 'current'
   * - 指定：写入指定的命名上下文（自动创建）
   */
  targetContext?: string;
  
  /** 操作选项 */
  operationOptions?: {
    visibleOnly?: boolean;
  };
}
```

**使用示例：**

```toml
# 示例 1：从 current 提取最近10条，保存到 research-notes
[[nodes]]
id = "create-research-context"
type = "CONTEXT_PROCESSOR"
[nodes.config.operationConfig]
operation = "TRUNCATE"
[nodes.config.operationConfig.truncate]
lastN = 10

[nodes.config]
sourceContext = "current"       # 从 current 读取
targetContext = "research-notes" # 保存到 research-notes

# 示例 2：过滤 assistant 消息，保存到 temp
[[nodes]]
id = "filter-assistant"
type = "CONTEXT_PROCESSOR"
[nodes.config.operationConfig]
operation = "FILTER"
[nodes.config.operationConfig.filter]
byRole = "assistant"

[nodes.config]
sourceContext = "current"
targetContext = "temp"

# 示例 3：对已有上下文进行二次编辑
[[nodes]]
id = "refine-research"
type = "CONTEXT_PROCESSOR"
[nodes.config.operationConfig]
operation = "FILTER"
[nodes.config.operationConfig.filter]
byRole = "user"

[nodes.config]
sourceContext = "research-notes"  # 从 research-notes 读取
targetContext = "user-only-notes" # 保存到 user-only-notes
```

**Handler 实现：**

```typescript
export async function contextProcessorHandler(
  workflowExecution: WorkflowExecution,
  node: Node,
  context: ContextProcessorHandlerContext,
): Promise<ContextProcessorExecutionResult> {
  const config = node.config as ContextProcessorNodeConfig;
  
  // 1. 确定源上下文
  const sourceContextId = config.sourceContext || 'current';
  const sourceContext = context.messageContextRegistry?.get(sourceContextId);
  
  if (!sourceContext) {
    throw new RuntimeValidationError(
      `Source context '${sourceContextId}' not found`
    );
  }
  
  // 2. 执行消息操作
  const result = await executeMessageOperation(
    sourceContext.messages,
    config.operationConfig
  );
  
  // 3. 确定目标上下文
  const targetContextId = config.targetContext || 'current';
  let targetContext = context.messageContextRegistry?.get(targetContextId);
  
  if (!targetContext) {
    // 自动创建新的上下文
    targetContext = {
      id: targetContextId,
      messages: [],
      createdAt: now(),
      updatedAt: now(),
    };
    context.messageContextRegistry?.register(targetContext);
  }
  
  // 4. 更新目标上下文
  targetContext.messages = result.messages;
  targetContext.updatedAt = now();
  
  return {
    operation: config.operationConfig.operation,
    messageCount: result.messages.length,
    executionTime: now() - startTime,
  };
}
```

#### C. Subgraph 节点（简化版）

```typescript
export interface SubgraphNodeConfig {
  subgraphId: ID;
  async: boolean;
  
  /**
   * 【新增】上下文传递配置
   * 
   * 替代原有的 conversationHistoryCallback
   */
  contextPassing?: {
    /** 要传递给子工作流的上下文ID列表 */
    contextIds: string[];
    
    /** 传递策略 */
    strategy?: {
      mode: 'clone' | 'reference' | 'snapshot';
      mergeToInitial?: boolean;
      namespace?: string;  # 避免ID冲突
    };
  };
}
```

**使用示例：**

```toml
# 示例 1：传递单个上下文
[[nodes]]
id = "call-subgraph"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "research-agent"
async = false

[nodes.config.contextPassing]
contextIds = ["research-notes"]
strategy.mode = "clone"
strategy.mergeToInitial = true

# 示例 2：传递多个上下文并添加命名空间
[[nodes]]
id = "call-complex-subgraph"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "multi-task-agent"
async = false

[nodes.config.contextPassing]
contextIds = ["research-notes", "code-context"]
strategy.mode = "clone"
strategy.namespace = "parent_"
# 子工作流中变为 "parent_research-notes", "parent_code-context"
```

#### D. Agent Loop 节点（简化版）

```typescript
export interface AgentLoopNodeConfig {
  agentLoopId?: ID;
  
  inlineConfig?: {
    profileId: string;
    maxIterations?: number;
    availableTools?: AvailableTools;
    
    /**
     * 【新增】初始消息上下文引用
     * 
     * 替代 systemPrompt、initialMessages 等配置
     */
    initialContextRefs?: string[];
    
    /**
     * 【新增】Agent Loop 内部使用的上下文ID
     * 
     * 默认为 'current'
     */
    workingContext?: string;
  };
}
```

**使用示例：**

```toml
# 示例 1：使用命名上下文初始化 Agent Loop
[[nodes]]
id = "agent-with-context"
type = "AGENT_LOOP"
[nodes.config.inlineConfig]
profileId = "gpt-4"
maxIterations = 10
initialContextRefs = ["system", "task-spec"]
workingContext = "agent-main"

# 示例 2：从 current 继承
[[nodes]]
id = "agent-from-current"
type = "AGENT_LOOP"
[nodes.config.inlineConfig]
profileId = "gpt-4"
initialContextRefs = ["current"]  # 直接从 current 开始
```

---

## 🔧 执行实例的上下文管理

### 1. 自动创建的内置上下文

每个 Execution 启动时，自动创建以下上下文：

```typescript
export function initializeExecutionContext(
  execution: WorkflowExecution,
  registry: MessageContextRegistry,
): void {
  // 1. 创建 current 上下文（主对话）
  registry.register({
    id: 'current',
    messages: [],  // 初始为空，或由 workflow 配置指定
    createdAt: now(),
    updatedAt: now(),
    metadata: {
      description: 'Main conversation context',
    },
  });
  
  // 2. 如果 workflow 定义了 system 上下文，创建它
  const systemMessages = execution.workflow.config.systemMessages;
  if (systemMessages && systemMessages.length > 0) {
    registry.register({
      id: 'system',
      messages: systemMessages,
      createdAt: now(),
      updatedAt: now(),
      metadata: {
        description: 'System instructions',
      },
    });
  }
}
```

### 2. Workflow 配置中的上下文定义（简化版）

```typescript
export interface WorkflowConfig {
  id: ID;
  name: string;
  
  // ... 其他字段
  
  /**
   * 【新增】预定义的系统消息
   * 
   * 自动创建为 'system' 上下文
   */
  systemMessages?: LLMMessage[];
  
  /**
   * 【新增】预定义的静态上下文
   */
  staticContexts?: {
    id: string;
    messages: LLMMessage[];
    description?: string;
  }[];
}
```

**使用示例：**

```toml
# workflow.toml
id = "research-workflow"
name = "Research Assistant"

# 定义系统消息（自动创建为 'system' 上下文）
[[systemMessages]]
role = "system"
content = "You are a research assistant..."

# 定义静态上下文
[[staticContexts]]
id = "user-profile"
description = "User preferences"
[[staticContexts.messages]]
role = "system"
content = "User prefers concise answers..."

[[staticContexts]]
id = "coding-standards"
description = "Project coding standards"
[[staticContexts.messages]]
role = "system"
content = "Follow TypeScript best practices..."
```

---

## ✅ 优势总结

### 1. 极简配置

| 旧设计 | 新设计 |
|--------|--------|
| `prompt` + `promptTemplateId` + `promptTemplateVariables` | `contextRefs = ["current"]` |
| `inheritWorkflowContext: true` | `contextRefs = ["current"]` |
| 复杂的模板渲染逻辑 | 直接传递 Message[] |

### 2. 零序列化开销

```typescript
// 旧方式：需要序列化
const prompt = renderTemplate(templateId, variables);  // → string
await llm.call(prompt);  // string → 内部再解析

// 新方式：直接传递
const messages = getContextMessages(contextRefs);  // → Message[]
await llm.call(messages);  // Message[] → 直接使用
```

### 3. 清晰的依赖关系

```toml
# 一眼就能看出节点使用了哪些上下文
[nodes.config]
contextRefs = ["system", "research-notes", "current"]

# 一眼就能看出上下文从哪里来
[nodes.config]
sourceContext = "current"
targetContext = "research-notes"
```

### 4. 灵活的组合

```toml
# 任意组合多个上下文
contextRefs = ["system", "ctx1", "ctx2", "current"]

# 任意链式处理
Context Processor: current → research-notes
Context Processor: research-notes → filtered-notes
LLM: filtered-notes → result
```

### 5. 统一的模型

- ✅ 所有节点都通过 `contextRefs` 引用上下文
- ✅ 所有上下文都通过 Context Processor 管理
- ✅ 没有特殊的"继承"或"模板"概念

---

## 🚀 实施路线图

### 阶段 1：基础架构（1周）

1. 实现 `NamedMessageContext` 和 `MessageContextRegistry`
2. 定义内置上下文ID（`current`, `system`, `temp`）
3. 实现 Execution 启动时的上下文初始化

### 阶段 2：节点重构（2周）

1. 简化 `LLMNodeConfig`：移除 prompt/template，只保留 `contextRefs`
2. 增强 `ContextProcessorNodeConfig`：添加 `sourceContext` 和 `targetContext`
3. 简化 `AgentLoopNodeConfig`：移除 systemPrompt，使用 `initialContextRefs`
4. 更新所有 Handler 实现

### 阶段 3：Subgraph 迁移（1周）

1. 移除 `conversationHistoryCallback`
2. 添加 `contextPassing` 配置
3. 更新 Subgraph Handler

### 阶段 4：清理与优化（1周）

1. 移除旧的模板渲染代码
2. 移除 prompt 字符串相关逻辑
3. 性能测试和优化

---

## 📝 迁移对比

### 旧配置

```toml
[[nodes]]
id = "llm-node"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
prompt = "Analyze the following code:"
promptTemplateId = "code-analysis-template"
promptTemplateVariables.code = "{{code}}"
inheritWorkflowContext = true
```

### 新配置

```toml
# 步骤 1：准备上下文（如果需要）
[[nodes]]
id = "prepare-code-context"
type = "CONTEXT_PROCESSOR"
[nodes.config.operationConfig]
operation = "FILTER"
[nodes.config.operationConfig.filter]
byRole = "user"

[nodes.config]
sourceContext = "current"
targetContext = "code-context"

# 步骤 2：使用上下文
[[nodes]]
id = "llm-node"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["system", "code-context", "current"]
```

**优势：**
- ✅ 配置更清晰
- ✅ 无模板渲染开销
- ✅ 上下文可复用
- ✅ 易于调试

---

这个简化方案如何？是否符合你的预期？
