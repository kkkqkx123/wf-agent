你说得非常对!这是一个关键的架构洞察。让我重新梳理这个问题:

## 🎯 核心问题: Subgraph的"黑盒"特性

### 1. **Subgraph的本质**

```
父工作流视角:
┌─────────────────────────────┐
│ START → [SUBGRAPH] → END   │
│              ↑               │
│         黑盒,内部结构未知     │
└─────────────────────────────┘

子工作流定义时:
┌─────────────────────────────┐
│ START → LLM → END          │
│  ↑                           │
│ 不知道会被谁调用              │
│ 不知道父工作流有哪些context  │
└─────────────────────────────┘
```

**关键约束**:
- ✅ Subgraph是**独立定义**的,编译时不知道父工作流的结构
- ✅ Subgraph只能使用**传入的参数**(variables + messages)
- ❌ Subgraph不能假设父工作流存在某个特定的context ID

---

## 💡 正确的解决方案: Start节点的消息数组传递

### 设计思路

**扩展START节点,支持显式声明输入的消息上下文**,类似于函数签名:

```typescript
/**
 * Start node configuration with message context inputs
 */
export interface StartNodeConfig {
  /**
   * Message context inputs (like function parameters)
   * 
   * Defines what message contexts this subgraph expects to receive.
   * Each input has:
   * - externalName: The name used by caller (parent workflow)
   * - internalName: The name used internally in this subgraph
   */
  messageInputs?: Array<{
    /** External name (used by parent workflow when calling) */
    externalName: string;
    
    /** Internal name (used within this subgraph's nodes) */
    internalName: string;
    
    /** Whether this input is required */
    required?: boolean;
    
    /** Default messages if not provided */
    defaultMessages?: LLMMessage[];
  }>;
  
  /**
   * Output message contexts (like function return values)
   */
  messageOutputs?: Array<{
    /** Internal name (produced by this subgraph) */
    internalName: string;
    
    /** External name (visible to parent workflow) */
    externalName: string;
  }>;
}
```

### 使用示例

#### 子工作流定义 (research-agent.toml)

```toml
[workflow]
id = "research-agent"
name = "Research Agent"

# START节点声明输入输出
[[nodes]]
id = "start"
type = "START"
[nodes.config]
# 声明接收的消息上下文
messageInputs = [
  { externalName = "research_query", internalName = "query", required = true },
  { externalName = "background_knowledge", internalName = "knowledge", required = false }
]
# 声明输出的消息上下文
messageOutputs = [
  { internalName = "analysis_result", externalName = "result" }
]

# 内部节点使用 internalName
[[nodes]]
id = "analyze"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["query", "knowledge"]  # ← 使用internalName

[[nodes]]
id = "generate-report"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["query", "analysis_result"]

[[nodes]]
id = "end"
type = "END"
[nodes.config]
# 指定哪些context作为输出
outputContexts = ["analysis_result"]
```

#### 父工作流调用 (main-workflow.toml)

```toml
[[nodes]]
id = "prepare-query"
type = "CONTEXT_PROCESSOR"
[nodes.config]
sourceContext = "current"
targetContext = "my-query"
operationConfig.operation = "TRUNCATE"
operationConfig.truncate.lastN = 5

[[nodes]]
id = "call-research"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "research-agent"
async = false

# 传递消息上下文 (使用externalName)
[nodes.config.messagePassing]
inputs = {
  research_query = "my-query",           # 父: my-query → 子: query
  background_knowledge = "knowledge-base" # 父: knowledge-base → 子: knowledge
}
# 接收输出的消息上下文
outputs = {
  result = "research-result"  # 子: result → 父: research-result
}

[[nodes]]
id = "use-result"
type = "LLM"
[nodes.config]
contextRefs = ["current", "research-result"]  # ← 使用父工作流的context
```

---

## 🔧 实现机制

### 1. **预处理阶段: 验证和映射**

```typescript
interface MessageContextMapping {
  /** 父工作流context ID → 子工作流internalName */
  inputMapping: Map<string, string>;
  
  /** 子工作流internalName → 父工作流context ID */
  outputMapping: Map<string, string>;
}

function validateAndMapMessageContexts(
  subgraphNode: Node,
  subgraphStartNode: Node,
): MessageContextMapping {
  const subgraphConfig = subgraphNode.config as SubgraphNodeConfig;
  const startConfig = subgraphStartNode.config as StartNodeConfig;
  
  const mapping: MessageContextMapping = {
    inputMapping: new Map(),
    outputMapping: new Map(),
  };
  
  // 验证inputs
  for (const [externalName, parentContextId] of Object.entries(subgraphConfig.messagePassing?.inputs || {})) {
    const inputDef = startConfig.messageInputs?.find(i => i.externalName === externalName);
    
    if (!inputDef) {
      throw new Error(`Subgraph does not accept input '${externalName}'`);
    }
    
    if (inputDef.required && !parentContextId) {
      throw new Error(`Required input '${externalName}' not provided`);
    }
    
    mapping.inputMapping.set(parentContextId, inputDef.internalName);
  }
  
  // 验证outputs
  for (const [internalName, parentContextId] of Object.entries(subgraphConfig.messagePassing?.outputs || {})) {
    const outputDef = startConfig.messageOutputs?.find(o => o.internalName === internalName);
    
    if (!outputDef) {
      throw new Error(`Subgraph does not produce output '${internalName}'`);
    }
    
    mapping.outputMapping.set(internalName, parentContextId);
  }
  
  return mapping;
}
```

### 2. **运行时: Context传递**

```typescript
async function enterSubgraph(
  executionEntity: WorkflowExecutionEntity,
  workflowId: string,
  parentWorkflowId: string,
  input: Record<string, unknown>,
  subgraphNode: Node,
): Promise<void> {
  const registry = executionEntity.getRegistry();
  const config = subgraphNode.config as SubgraphNodeConfig;
  
  // 获取子工作流的START节点配置
  const subgraphGraph = getSubgraphGraph(workflowId);
  const startNode = subgraphGraph.getNode(subgraphGraph.startNodeId);
  const startConfig = startNode.config as StartNodeConfig;
  
  // 创建消息上下文映射
  const mapping = validateAndMapMessageContexts(subgraphNode, startNode);
  
  // 复制输入contexts到子工作流的internal names
  for (const [parentContextId, internalName] of mapping.inputMapping) {
    const parentContext = registry.get(parentContextId);
    
    if (parentContext) {
      // 在子工作流的registry中注册 (使用internalName)
      registry.register({
        id: internalName,
        messages: [...parentContext.messages],  // 浅拷贝
        createdAt: now(),
        updatedAt: now(),
        metadata: {
          ...parentContext.metadata,
          sourceContext: parentContextId,
          passedFromParent: true,
        },
      });
    } else if (startConfig.messageInputs?.find(i => i.internalName === internalName)?.required) {
      throw new Error(`Required context '${parentContextId}' not found in parent workflow`);
    }
  }
  
  await executionEntity.enterSubgraph(workflowId, parentWorkflowId, input);
}

async function exitSubgraph(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: Node,
): Promise<void> {
  const registry = executionEntity.getRegistry();
  const config = subgraphNode.config as SubgraphNodeConfig;
  
  // 获取输出映射
  const subgraphGraph = getSubgraphGraph(executionEntity.getCurrentSubgraphContext().workflowId);
  const startNode = subgraphGraph.getNode(subgraphGraph.startNodeId);
  const mapping = validateAndMapMessageContexts(subgraphNode, startNode);
  
  // 将输出contexts复制回父工作流
  for (const [internalName, parentContextId] of mapping.outputMapping) {
    const childContext = registry.get(internalName);
    
    if (childContext) {
      // 更新或创建父工作流的context
      if (registry.has(parentContextId)) {
        registry.update(parentContextId, [...childContext.messages]);
      } else {
        registry.register({
          id: parentContextId,
          messages: [...childContext.messages],
          createdAt: now(),
          updatedAt: now(),
        });
      }
    }
  }
  
  await executionEntity.exitSubgraph();
}
```

---

## 📋 完整的类型定义

### 1. **Start节点配置扩展**

```typescript
// packages/types/src/node/configs/control-configs.ts

export interface StartNodeConfig {
  /**
   * Message context inputs
   * 
   * Defines the message contexts that this workflow (especially subgraphs)
   * expects to receive from the caller.
   */
  messageInputs?: Array<{
    /** Name used by the caller (parent workflow) */
    externalName: string;
    
    /** Name used internally within this workflow */
    internalName: string;
    
    /** Whether this input is required */
    required?: boolean;
    
    /** Description for documentation */
    description?: string;
    
    /** Default messages if not provided by caller */
    defaultMessages?: LLMMessage[];
  }>;
  
  /**
   * Message context outputs
   * 
   * Defines the message contexts that this workflow produces for the caller.
   */
  messageOutputs?: Array<{
    /** Name used internally within this workflow */
    internalName: string;
    
    /** Name visible to the caller (parent workflow) */
    externalName: string;
    
    /** Description for documentation */
    description?: string;
  }>;
}
```

### 2. **Subgraph节点配置扩展**

```typescript
// packages/types/src/node/configs/subgraph-configs.ts

export interface SubgraphNodeConfig {
  subgraphId: ID;
  async: boolean;
  
  /**
   * Message context passing configuration
   * 
   * Maps parent workflow contexts to subgraph inputs,
   * and subgraph outputs back to parent workflow contexts.
   */
  messagePassing?: {
    /** Input mapping: parentContextId → subgraphInputName */
    inputs?: Record<string, string>;
    
    /** Output mapping: subgraphOutputName → parentContextId */
    outputs?: Record<string, string>;
  };
}
```

---

## 🎯 优势总结

### 1. **明确的契约**

```toml
# 子工作流明确声明接口
[nodes.config]
messageInputs = [
  { externalName = "query", internalName = "q", required = true },
  { externalName = "context", internalName = "ctx", required = false }
]
```

类似函数签名: `function research(q: Messages, ctx?: Messages): Messages`

### 2. **解耦父子工作流**

- 子工作流不依赖父工作流的context命名
- 父工作流可以自由选择如何传递数据
- 通过`externalName/internalName`映射解耦

### 3. **类型安全**

- 预处理阶段验证inputs/outputs是否匹配
- 运行时检查required inputs是否提供
- 避免静默失败

### 4. **灵活性**

```toml
# 场景1: 简单传递
inputs = { query = "current" }

# 场景2: 多个输入
inputs = {
  query = "user-question",
  context = "knowledge-base",
  history = "conversation-history"
}

# 场景3: 可选输入 (使用defaultMessages)
messageInputs = [
  { externalName = "optional_context", internalName = "ctx", defaultMessages = [] }
]
```

---

你觉得这个设计方案如何?是否需要调整某些细节?