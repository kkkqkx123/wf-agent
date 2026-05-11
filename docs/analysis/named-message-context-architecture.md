# 语义化消息ID + 多上下文管理架构设计

## 🎯 核心设计理念

### 当前问题

1. **Subgraph 内置裁剪机制耦合严重**
   - `ContinueFromTriggerNodeConfig.conversationHistoryCallback` 直接在节点配置中定义裁剪规则
   - 裁剪逻辑与子工作流执行逻辑混合，不够优雅
   - 无法复用裁剪结果，每次都需要重新计算

2. **Batch 机制的局限性**
   - 单一消息数组 + 单一批次序列
   - Fork/Join、Agent Loop 等场景需要临时隔离上下文时，只能克隆整个 Session
   - 无法同时维护多个独立的"视图"或"快照"

3. **上下文传递不灵活**
   - LLM 节点只能通过 `prompt` 字符串接收输入
   - Agent Loop 只能从变量中提取简单字符串
   - 无法精确指定使用哪些历史消息

### 新设计目标

> **"将消息管理从节点配置中解耦，通过语义化ID实现声明式的上下文引用"**

```
传统方式:
┌─────────────┐     裁剪规则      ┌──────────┐
│ Subgraph    │ ───────────────→ │ 裁剪逻辑  │ → 传递给子工作流
│ Node Config │                  │ (内嵌)   │
└─────────────┘                  └──────────┘

新方式:
┌─────────────┐                  ┌──────────────┐
│ Context     │ ──产生──→        │ NamedContext │
│ Processor   │   "research-ctx" │ (独立管理)   │
└─────────────┘                  └──────┬───────┘
                                        │ 引用
                    ┌───────────────────┼───────────────────┐
                    ↓                   ↓                   ↓
            ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
            │ LLM Node     │   │ Subgraph     │   │ Agent Loop   │
            │ ctx=         │   │ ctx=         │   │ ctx=         │
            │ research-ctx │   │ research-ctx │   │ research-ctx │
            └──────────────┘   └──────────────┘   └──────────────┘
```

---

## 📐 架构设计

### 1. 核心概念：Named Message Context

```typescript
/**
 * 命名消息上下文
 * 
 * 一个独立的、可引用的消息数组，具有唯一的语义化ID
 */
export interface NamedMessageContext {
  /** 语义化ID（用户定义） */
  id: string;
  
  /** 消息数组 */
  messages: LLMMessage[];
  
  /** 创建时间戳 */
  createdAt: Timestamp;
  
  /** 最后更新时间戳 */
  updatedAt: Timestamp;
  
  /** 元数据（可选） */
  metadata?: {
    /** 描述 */
    description?: string;
    
    /** 来源节点ID */
    sourceNodeId?: ID;
    
    /** 来源执行ID */
    sourceExecutionId?: ID;
    
    /** 自定义标签 */
    tags?: string[];
    
    /** Token 统计 */
    tokenCount?: number;
  };
}

/**
 * 消息上下文注册表
 * 
 * 管理所有命名上下文的生命周期
 */
export interface MessageContextRegistry {
  /** 注册新的命名上下文 */
  register(context: NamedMessageContext): void;
  
  /** 获取命名上下文 */
  get(id: string): NamedMessageContext | undefined;
  
  /** 更新命名上下文 */
  update(id: string, messages: LLMMessage[]): void;
  
  /** 删除命名上下文 */
  delete(id: string): void;
  
  /** 列出所有上下文ID */
  listIds(): string[];
  
  /** 检查上下文是否存在 */
  has(id: string): boolean;
}
```

### 2. Context Processor 节点增强

```typescript
/**
 * Context Processor 节点配置（增强版）
 */
export interface ContextProcessorNodeConfig {
  version?: number;
  
  /** 消息操作配置 */
  operationConfig: MessageOperationConfig;
  
  /** 操作选项 */
  operationOptions?: {
    visibleOnly?: boolean;
    autoCreateBatch?: boolean;
    target?: 'self' | 'parent';
  };
  
  /**
   * 【新增】输出配置：将操作结果保存为命名上下文
   * 
   * 如果提供此配置，操作结果将被保存为一个独立的命名上下文，
   * 而不是直接修改当前的 ConversationSession。
   */
  output?: {
    /** 命名上下文的ID（语义化字符串） */
    contextId: string;
    
    /** 是否覆盖已存在的同名上下文（默认 false） */
    overwrite?: boolean;
    
    /** 元数据（可选） */
    metadata?: {
      description?: string;
      tags?: string[];
    };
  };
}
```

**使用示例：**

```toml
# 示例 1：创建研究摘要上下文
[[nodes]]
id = "create-research-context"
type = "CONTEXT_PROCESSOR"

# 从当前对话中提取最近 10 条消息
[nodes.config.operationConfig]
operation = "TRUNCATE"
[nodes.config.operationConfig.truncate]
lastN = 10

# 保存为命名上下文 "research-summary"
[nodes.config.output]
contextId = "research-summary"
description = "研究任务的对话摘要"
tags = ["research", "summary"]

# 示例 2：创建过滤后的上下文
[[nodes]]
id = "create-filtered-context"
type = "CONTEXT_PROCESSOR"

# 只保留 assistant 角色的消息
[nodes.config.operationConfig]
operation = "FILTER"
[nodes.config.operationConfig.filter]
byRole = "assistant"

# 保存为命名上下文 "assistant-responses"
[nodes.config.output]
contextId = "assistant-responses"
```

### 3. 支持上下文引用的节点

#### A. LLM 节点

```typescript
export interface LLMNodeConfig {
  profileId: ID;
  
  /** 传统的 prompt 字符串（向后兼容） */
  prompt?: string;
  promptTemplateId?: string;
  promptTemplateVariables?: Record<string, unknown>;
  
  /**
   * 【新增】直接传入消息数组
   */
  messages?: LLMMessage[];
  
  /**
   * 【新增】引用命名上下文
   * 
   * 当提供此字段时：
   * - 忽略 prompt 和 messages
   * - 使用指定的命名上下文作为输入
   * - 可以引用多个上下文并合并
   */
  contextRefs?: {
    /** 引用的上下文ID列表 */
    ids: string[];
    
    /** 合并策略 */
    mergeStrategy?: 'concat' | 'interleave' | 'replace';
    
    /** 是否包含系统消息（默认 true） */
    includeSystemMessages?: boolean;
  };
  
  parameters?: Record<string, unknown>;
  maxToolCallsPerRequest?: number;
}
```

**使用示例：**

```toml
# 示例 1：直接使用命名上下文
[[nodes]]
id = "llm-with-context"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs.ids = ["research-summary"]

# 示例 2：合并多个上下文
[[nodes]]
id = "llm-merged-context"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs.ids = ["research-summary", "code-analysis", "user-preferences"]
contextRefs.mergeStrategy = "concat"

# 示例 3：向后兼容（仍然使用 prompt）
[[nodes]]
id = "llm-simple"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
prompt = "简单问题"
```

#### B. Subgraph 节点

```typescript
export interface SubgraphNodeConfig {
  subgraphId: ID;
  async: boolean;
  
  /**
   * 【新增】上下文传递配置
   * 
   * 替代原有的 conversationHistoryCallback，更加声明式
   */
  contextPassing?: {
    /** 要传递给子工作流的上下文ID列表 */
    contextIds: string[];
    
    /** 传递策略 */
    strategy?: {
      /** 
       * 'clone': 克隆消息数组（子工作流修改不影响父级）
       * 'reference': 引用同一数组（共享可变状态，谨慎使用）
       * 'snapshot': 创建不可变快照
       */
      mode: 'clone' | 'reference' | 'snapshot';
      
      /** 是否合并到子工作流的初始上下文 */
      mergeToInitial?: boolean;
      
      /** 在子工作流中的命名空间前缀（避免ID冲突） */
      namespace?: string;
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
contextIds = ["research-summary"]
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
contextIds = ["research-summary", "code-context", "user-profile"]
strategy.mode = "clone"
strategy.namespace = "parent_"  # 子工作流中变为 "parent_research-summary"
```

#### C. Agent Loop 节点

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
     * 【新增】初始消息配置
     */
    initialMessages?: LLMMessage[];
    
    /**
     * 【新增】引用命名上下文作为初始消息
     */
    contextRefs?: {
      ids: string[];
      mergeStrategy?: 'concat' | 'interleave';
    };
    
    /**
     * 【新增】是否继承 workflow 的完整上下文
     */
    inheritWorkflowContext?: boolean;
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
contextRefs.ids = ["research-summary", "task-spec"]
contextRefs.mergeStrategy = "concat"

# 示例 2：继承 workflow 上下文 + 额外上下文
[[nodes]]
id = "agent-hybrid"
type = "AGENT_LOOP"
[nodes.config.inlineConfig]
profileId = "gpt-4"
inheritWorkflowContext = true  # 先继承所有历史
contextRefs.ids = ["additional-instructions"]  # 再追加额外指令
```

#### D. Context Processor 节点（二次编辑）

```toml
# 对已有命名上下文进行二次编辑
[[nodes]]
id = "refine-research-context"
type = "CONTEXT_PROCESSOR"

# 引用已有的 "research-summary" 上下文
[nodes.config.operationConfig]
operation = "FILTER"
sourceContextId = "research-summary"  # ← 新字段：指定源上下文

[nodes.config.operationConfig.filter]
byRole = "assistant"

# 保存为新上下文 "assistant-only-summary"
[nodes.config.output]
contextId = "assistant-only-summary"
```

### 4. Workflow 结构定义中的上下文管理

```typescript
/**
 * Workflow 级别的上下文定义
 * 
 * 在 workflow 配置中预定义可用的命名上下文
 */
export interface WorkflowContextDefinition {
  /** 上下文ID */
  id: string;
  
  /** 描述 */
  description?: string;
  
  /** 初始消息（可选） */
  initialMessages?: LLMMessage[];
  
  /** 创建策略 */
  creationStrategy?: {
    /** 
     * 'static': 静态定义，workflow 启动时创建
     * 'dynamic': 动态创建，由 Context Processor 节点生成
     * 'import': 从外部导入（如文件、API）
     */
    type: 'static' | 'dynamic' | 'import';
    
    /** 当 type='import' 时的配置 */
    importConfig?: {
      source: 'file' | 'api' | 'variable';
      path?: string;
      url?: string;
      variableName?: string;
    };
  };
  
  /** 生命周期 */
  lifecycle?: {
    /** 
     * 'workflow': 整个 workflow 执行期间有效
     * 'execution': 单次 execution 期间有效
     * 'temporary': 临时使用，使用后自动删除
     */
    scope: 'workflow' | 'execution' | 'temporary';
    
    /** 是否在 execution 结束后自动清理 */
    autoCleanup?: boolean;
  };
}

/**
 * Workflow 配置增强
 */
export interface WorkflowConfig {
  // ... 现有字段
  
  /**
   * 【新增】预定义的命名上下文
   */
  contexts?: WorkflowContextDefinition[];
}
```

**使用示例：**

```toml
# workflow.toml
id = "research-workflow"
name = "Research Assistant"

# 预定义上下文
[[contexts]]
id = "system-instructions"
description = "系统指令模板"
creationStrategy.type = "static"
[[contexts.initialMessages]]
role = "system"
content = "You are a research assistant..."

[[contexts]]
id = "user-profile"
description = "用户偏好设置"
creationStrategy.type = "import"
[contexts.creationStrategy.importConfig]
source = "variable"
variableName = "userProfile"

[[contexts]]
id = "research-notes"
description = "研究笔记（动态生成）"
creationStrategy.type = "dynamic"
```

---

## 🔧 实现细节

### 1. MessageContextRegistry 实现

```typescript
export class InMemoryMessageContextRegistry implements MessageContextRegistry {
  private contexts: Map<string, NamedMessageContext> = new Map();
  
  register(context: NamedMessageContext): void {
    if (this.contexts.has(context.id)) {
      throw new Error(`Context with id '${context.id}' already exists`);
    }
    this.contexts.set(context.id, context);
  }
  
  get(id: string): NamedMessageContext | undefined {
    return this.contexts.get(id);
  }
  
  update(id: string, messages: LLMMessage[]): void {
    const context = this.contexts.get(id);
    if (!context) {
      throw new Error(`Context with id '${id}' not found`);
    }
    context.messages = messages;
    context.updatedAt = now();
  }
  
  delete(id: string): void {
    this.contexts.delete(id);
  }
  
  listIds(): string[] {
    return Array.from(this.contexts.keys());
  }
  
  has(id: string): boolean {
    return this.contexts.has(id);
  }
}
```

### 2. Context Processor Handler 增强

```typescript
export async function contextProcessorHandler(
  workflowExecution: WorkflowExecution,
  node: Node,
  context: ContextProcessorHandlerContext,
): Promise<ContextProcessorExecutionResult> {
  const config = node.config as ContextProcessorNodeConfig;
  
  // 1. 确定操作的消息源
  let sourceMessages: LLMMessage[];
  
  if (config.operationConfig.sourceContextId) {
    // 从命名上下文获取
    const namedContext = context.messageContextRegistry?.get(
      config.operationConfig.sourceContextId
    );
    
    if (!namedContext) {
      throw new RuntimeValidationError(
        `Source context '${config.operationConfig.sourceContextId}' not found`
      );
    }
    
    sourceMessages = namedContext.messages;
  } else {
    // 从当前 ConversationSession 获取（原有逻辑）
    sourceMessages = context.conversationManager.getMessages();
  }
  
  // 2. 执行消息操作
  const result = await executeMessageOperation(sourceMessages, config.operationConfig);
  
  // 3. 如果配置了输出，保存到命名上下文
  if (config.output) {
    const outputContext: NamedMessageContext = {
      id: config.output.contextId,
      messages: result.messages,
      createdAt: now(),
      updatedAt: now(),
      metadata: {
        description: config.output.metadata?.description,
        sourceNodeId: node.id,
        sourceExecutionId: workflowExecution.id,
        tags: config.output.metadata?.tags,
        tokenCount: calculateTokenCount(result.messages),
      },
    };
    
    // 注册到 registry
    if (config.output.overwrite || !context.messageContextRegistry?.has(config.output.contextId)) {
      context.messageContextRegistry?.register(outputContext);
    } else {
      context.messageContextRegistry?.update(config.output.contextId, result.messages);
    }
  } else {
    // 没有配置输出，直接修改当前 ConversationSession（原有逻辑）
    context.conversationManager.setMessages(result.messages);
  }
  
  return {
    operation: config.operationConfig.operation,
    messageCount: result.messages.length,
    executionTime: now() - startTime,
  };
}
```

### 3. LLM Handler 增强

```typescript
export async function llmHandler(
  workflowExecution: WorkflowExecution,
  node: Node,
  context: LLMHandlerContext,
): Promise<LLMExecutionResult> {
  const config = node.config as LLMNodeConfig;
  
  // 1. 确定输入消息
  let inputMessages: LLMMessage[];
  
  if (config.contextRefs && config.contextRefs.ids.length > 0) {
    // 从命名上下文引用
    inputMessages = [];
    
    for (const contextId of config.contextRefs.ids) {
      const namedContext = context.messageContextRegistry?.get(contextId);
      
      if (!namedContext) {
        throw new RuntimeValidationError(
          `Referenced context '${contextId}' not found`
        );
      }
      
      inputMessages.push(...namedContext.messages);
    }
    
    // 根据合并策略处理
    if (config.contextRefs.mergeStrategy === 'interleave') {
      inputMessages = interleaveMessages(inputMessages);
    }
  } else if (config.messages && config.messages.length > 0) {
    // 直接使用消息数组
    inputMessages = config.messages;
  } else {
    // 回退到 prompt 字符串（向后兼容）
    const prompt = resolvePrompt(config);
    inputMessages = [{ role: "user", content: prompt }];
  }
  
  // 2. 添加到 ConversationSession
  for (const msg of inputMessages) {
    context.conversationManager.addMessage(msg);
  }
  
  // 3. 执行 LLM
  const result = await context.llmCoordinator.executeLLM(
    {
      executionId: workflowExecution.id,
      nodeId: node.id,
      prompt: "",  // 不再使用
      profileId: config.profileId,
      parameters: config.parameters,
      maxToolCallsPerRequest: config.maxToolCallsPerRequest,
    },
    context.conversationManager,
  );
  
  return result;
}
```

---

## 🎨 Agent 层设计思考

对于 Agent 层，我建议采用类似但更简化的设计：

### 方案 A：Agent Loop 内部的 Context Registry

```typescript
/**
 * Agent Loop 运行时配置增强
 */
export interface AgentLoopRuntimeConfig {
  // ... 现有字段
  
  /**
   * 命名上下文注册表（可选）
   * 
   * 允许 Agent Loop 内部维护多个独立的上下文
   */
  contextRegistry?: MessageContextRegistry;
  
  /**
   * 默认使用的上下文ID
   * 
   * 如果不指定，使用主 ConversationSession
   */
  defaultContextId?: string;
}
```

**使用场景：**
- Agent Loop 在执行过程中可以创建临时的命名上下文
- 不同的工具调用可以使用不同的上下文
- Hook 可以访问和操作特定的上下文

### 方案 B：Agent Hook 中的上下文引用

```typescript
/**
 * Agent Hook 配置增强
 */
export interface AgentHook {
  // ... 现有字段
  
  /**
   * 上下文引用（可选）
   * 
   * Hook 可以指定操作哪个命名上下文
   */
  contextRef?: {
    /** 上下文ID */
    id: string;
    
    /** 操作类型 */
    operation: 'read' | 'write' | 'append';
  };
}
```

**使用示例：**

```toml
# Agent 配置
id = "research-agent"

[[hooks]]
type = "BEFORE_LLM_CALL"
action = "COMPRESS_CONTEXT"
contextRef.id = "main-conversation"
contextRef.operation = "write"

[[hooks]]
type = "AFTER_TOOL_CALL"
action = "LOG_TOOL_RESULT"
contextRef.id = "tool-execution-log"
contextRef.operation = "append"
```

---

## ✅ 优势总结

### 1. 解耦与复用

- ✅ **消息管理与节点逻辑解耦**：Context Processor 专注于消息操作，其他节点专注于业务逻辑
- ✅ **上下文可复用**：一次生成，多次引用
- ✅ **消除重复计算**：避免在每个子工作流中重新裁剪

### 2. 声明式配置

- ✅ **语义化ID**：`research-summary` 比复杂的裁剪规则更易理解
- ✅ **清晰的依赖关系**：通过 `contextRefs` 明确看到节点使用了哪些上下文
- ✅ **易于调试**：可以单独检查某个命名上下文的内容

### 3. 灵活性

- ✅ **多种合并策略**：concat、interleave、replace
- ✅ **灵活的传递模式**：clone、reference、snapshot
- ✅ **支持二次编辑**：可以对已有上下文进行进一步处理

### 4. 性能优化

- ✅ **按需加载**：只在需要时获取上下文
- ✅ **内存管理**：可以设置上下文的生命周期，自动清理
- ✅ **减少克隆开销**：通过 reference 模式避免不必要的深拷贝

### 5. 可扩展性

- ✅ **支持外部导入**：可以从文件、API、变量导入上下文
- ✅ **支持动态创建**：Context Processor 可以动态生成上下文
- ✅ **Agent 层扩展**：同样的设计可以应用到 Agent Loop

---

## 🚀 实施路线图

### 阶段 1：基础架构（1-2周）

1. 实现 `NamedMessageContext` 和 `MessageContextRegistry`
2. 增强 `ContextProcessorNodeConfig` 支持 `output` 配置
3. 更新 `ContextProcessorHandler` 支持保存到命名上下文

### 阶段 2：节点支持（2-3周）

1. 增强 `LLMNodeConfig` 支持 `contextRefs`
2. 增强 `SubgraphNodeConfig` 支持 `contextPassing`
3. 增强 `AgentLoopNodeConfig` 支持 `contextRefs`
4. 更新对应的 Handler 实现

### 阶段 3：Workflow 集成（1-2周）

1. 在 `WorkflowConfig` 中添加 `contexts` 字段
2. 实现 Workflow 启动时的上下文初始化
3. 实现上下文的生命周期管理

### 阶段 4：Agent 层扩展（2-3周）

1. 在 `AgentLoopRuntimeConfig` 中添加 `contextRegistry`
2. 实现 Agent Hook 中的上下文引用
3. 实现 Agent 内部的上下文管理

### 阶段 5：优化与文档（1-2周）

1. 性能优化（缓存、懒加载等）
2. 编写详细的使用文档和示例
3. 迁移指南（从旧配置到新配置）

---

## 📝 迁移示例

### 旧配置（Subgraph 内置裁剪）

```toml
[[nodes]]
id = "call-research-subgraph"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "research-agent"
async = false

# 复杂的裁剪配置
[nodes.config.continueFromTrigger.conversationHistoryCallback]
operation = "TRUNCATE"
[nodes.config.continueFromTrigger.conversationHistoryCallback.truncate]
lastN = 10
```

### 新配置（语义化上下文）

```toml
# 步骤 1：创建命名上下文
[[nodes]]
id = "prepare-research-context"
type = "CONTEXT_PROCESSOR"
[nodes.config.operationConfig]
operation = "TRUNCATE"
[nodes.config.operationConfig.truncate]
lastN = 10

[nodes.config.output]
contextId = "research-context"

# 步骤 2：使用命名上下文
[[nodes]]
id = "call-research-subgraph"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "research-agent"
async = false

[nodes.config.contextPassing]
contextIds = ["research-context"]
strategy.mode = "clone"
```

**优势：**
- 更清晰的责任分离
- `research-context` 可以被多个节点复用
- 更容易理解和维护

---

这个设计方案如何？是否符合你的预期？我可以立即开始实施阶段 1 的基础架构部分。
