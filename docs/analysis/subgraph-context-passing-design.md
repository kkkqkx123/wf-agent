# Subgraph Context 传递机制设计

## 1. 问题背景

### 1.1 Subgraph 的"黑盒"特性

Subgraph（子工作流）在定义时是独立的，无法感知父工作流的结构：

```
父工作流视角:                    子工作流定义时:
┌─────────────────────┐         ┌─────────────────────┐
│ START → [SUBGRAPH]  │         │ START → LLM → END   │
│          → END      │         │  ↑                   │
└─────────────────────┘         │  不知道会被谁调用     │
                                │  不知道父有哪些context│
                                └─────────────────────┘
```

**核心约束**:
- ✅ Subgraph 是**独立定义**的，编译时不知道父工作流的结构
- ✅ Subgraph 只能使用**传入的参数**（variables + messages）
- ❌ Subgraph 不能假设父工作流存在某个特定的 context ID

### 1.2 Context ID 冲突问题

当父子工作流都定义相同语义的 context 时，会产生冲突：

```toml
# 父工作流
[[nodes]]
id = "prepare"
type = "CONTEXT_PROCESSOR"
[nodes.config]
targetContext = "research-notes"  # ← 创建 research-notes

[[nodes]]
id = "call-subgraph"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "research-agent"

# 子工作流 (research-agent)
[[nodes]]
id = "process"
type = "CONTEXT_PROCESSOR"  
[nodes.config]
targetContext = "research-notes"  # ← 也创建 research-notes!

[[nodes]]
id = "analyze"
type = "LLM"
[nodes.config]
contextRefs = ["research-notes"]  # ← 引用的是哪个?
```

**问题**: 如果共享 Registry，子工作流的 `research-notes` 会**覆盖**父工作流的 `research-notes`，导致父工作流后续节点读到错误的数据。

### 1.3 与节点 ID 问题的相似性

| 维度 | 节点 ID | Context ID |
|------|--------|-----------|
| **定义阶段** | 静态定义在 TOML 中 | 静态定义在 TOML 中（通过 Context Processor） |
| **运行时** | 需要唯一标识 | 需要唯一标识 |
| **冲突场景** | 父子工作流同名节点 | 父子工作流同名上下文 |
| **解决方式** | 预处理添加命名空间前缀 | **应该也添加命名空间前缀** |

---

## 2. 设计方案

### 2.1 核心理念

**扩展 START 节点，支持显式声明输入输出的消息上下文**，类似于函数签名：

```typescript
// 类比函数签名
function researchAgent(
  query: Messages,           // 输入参数
  knowledge?: Messages       // 可选参数
): Messages {                // 返回值
  // ... 子工作流逻辑
}
```

### 2.2 类型定义

#### A. Start 节点配置扩展

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

#### B. Subgraph 节点配置扩展

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
    /** Input mapping: parentContextId → subgraphInputExternalName */
    inputs?: Record<string, string>;
    
    /** Output mapping: subgraphOutputExternalName → parentContextId */
    outputs?: Record<string, string>;
  };
}
```

---

## 3. 使用示例

### 3.1 子工作流定义 (research-agent.toml)

```toml
[workflow]
id = "research-agent"
name = "Research Agent"
type = "DEPENDENT"

# START 节点声明输入输出
[[nodes]]
id = "start"
type = "START"
[nodes.config]
# 声明接收的消息上下文
[[nodes.config.messageInputs]]
externalName = "research_query"
internalName = "query"
required = true
description = "The research question or topic"

[[nodes.config.messageInputs]]
externalName = "background_knowledge"
internalName = "knowledge"
required = false
description = "Optional background information"

# 声明输出的消息上下文
[[nodes.config.messageOutputs]]
internalName = "analysis_result"
externalName = "result"
description = "Analysis results"

# 内部节点使用 internalName
[[nodes]]
id = "analyze"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["query", "knowledge"]  # ← 使用 internalName

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
# 指定哪些 context 作为输出
outputContexts = ["analysis_result"]
```

### 3.2 父工作流调用 (main-workflow.toml)

```toml
[workflow]
id = "main-workflow"
name = "Main Workflow"
type = "STANDALONE"

[[nodes]]
id = "prepare-query"
type = "CONTEXT_PROCESSOR"
[nodes.config]
sourceContext = "current"
targetContext = "my-query"
operationConfig.operation = "TRUNCATE"
operationConfig.truncate.lastN = 5

[[nodes]]
id = "load-knowledge"
type = "CONTEXT_PROCESSOR"
[nodes.config]
sourceContext = "knowledge-base"
targetContext = "kb-context"

[[nodes]]
id = "call-research"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "research-agent"
async = false

# 传递消息上下文 (使用 externalName)
[nodes.config.messagePassing]
inputs = {
  research_query = "my-query",           # 父: my-query → 子: query
  background_knowledge = "kb-context"    # 父: kb-context → 子: knowledge
}
# 接收输出的消息上下文
outputs = {
  result = "research-result"  # 子: result → 父: research-result
}

[[nodes]]
id = "use-result"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["current", "research-result"]  # ← 使用父工作流的 context
```

---

## 4. 实现机制

### 4.1 预处理阶段：验证和映射

```typescript
interface MessageContextMapping {
  /** 父工作流 context ID → 子工作流 internalName */
  inputMapping: Map<string, string>;
  
  /** 子工作流 internalName → 父工作流 context ID */
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
  
  // 验证 inputs
  for (const [externalName, parentContextId] of Object.entries(subgraphConfig.messagePassing?.inputs || {})) {
    const inputDef = startConfig.messageInputs?.find(i => i.externalName === externalName);
    
    if (!inputDef) {
      throw new ConfigurationValidationError(
        `Subgraph does not accept input '${externalName}'`,
        { configType: "node", configPath: "messagePassing.inputs" }
      );
    }
    
    if (inputDef.required && !parentContextId) {
      throw new ConfigurationValidationError(
        `Required input '${externalName}' not provided`,
        { configType: "node", configPath: "messagePassing.inputs" }
      );
    }
    
    mapping.inputMapping.set(parentContextId, inputDef.internalName);
  }
  
  // 验证 outputs
  for (const [externalName, parentContextId] of Object.entries(subgraphConfig.messagePassing?.outputs || {})) {
    const outputDef = startConfig.messageOutputs?.find(o => o.externalName === externalName);
    
    if (!outputDef) {
      throw new ConfigurationValidationError(
        `Subgraph does not produce output '${externalName}'`,
        { configType: "node", configPath: "messagePassing.outputs" }
      );
    }
    
    mapping.outputMapping.set(outputDef.internalName, parentContextId);
  }
  
  return mapping;
}
```

### 4.2 运行时：Context 传递

#### Enter Subgraph

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
  
  // 获取子工作流的 START 节点配置
  const subgraphGraph = getSubgraphGraph(workflowId);
  const startNode = subgraphGraph.getNode(subgraphGraph.startNodeId);
  const startConfig = startNode.config as StartNodeConfig;
  
  // 创建消息上下文映射
  const mapping = validateAndMapMessageContexts(subgraphNode, startNode);
  
  // 复制输入 contexts 到子工作流的 internal names
  for (const [parentContextId, internalName] of mapping.inputMapping) {
    const parentContext = registry.get(parentContextId);
    
    if (parentContext) {
      // 在子工作流的 registry 中注册（使用 internalName）
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
      
      logger.debug(`Passed context '${parentContextId}' as '${internalName}' to subgraph`, {
        executionId: executionEntity.id,
        messageCount: parentContext.messages.length,
      });
    } else {
      const inputDef = startConfig.messageInputs?.find(i => i.internalName === internalName);
      
      if (inputDef?.required) {
        throw new RuntimeValidationError(
          `Required context '${parentContextId}' not found in parent workflow`,
          { operation: "enterSubgraph", field: "messagePassing.inputs" }
        );
      }
      
      // 使用默认消息（如果有）
      if (inputDef?.defaultMessages) {
        registry.register({
          id: internalName,
          messages: inputDef.defaultMessages,
          createdAt: now(),
          updatedAt: now(),
        });
      }
    }
  }
  
  await executionEntity.enterSubgraph(workflowId, parentWorkflowId, input);
}
```

#### Exit Subgraph

```typescript
async function exitSubgraph(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: Node,
): Promise<void> {
  const registry = executionEntity.getRegistry();
  const config = subgraphNode.config as SubgraphNodeConfig;
  
  // 获取输出映射
  const subgraphContext = executionEntity.getCurrentSubgraphContext();
  const subgraphGraph = getSubgraphGraph(subgraphContext.workflowId);
  const startNode = subgraphGraph.getNode(subgraphGraph.startNodeId);
  const mapping = validateAndMapMessageContexts(subgraphNode, startNode);
  
  // 将输出 contexts 复制回父工作流
  for (const [internalName, parentContextId] of mapping.outputMapping) {
    const childContext = registry.get(internalName);
    
    if (childContext) {
      // 更新或创建父工作流的 context
      if (registry.has(parentContextId)) {
        registry.update(parentContextId, [...childContext.messages]);
        
        logger.debug(`Returned context '${internalName}' as '${parentContextId}'`, {
          executionId: executionEntity.id,
          messageCount: childContext.messages.length,
        });
      } else {
        registry.register({
          id: parentContextId,
          messages: [...childContext.messages],
          createdAt: now(),
          updatedAt: now(),
          metadata: {
            description: `Output from subgraph '${subgraphContext.workflowId}'`,
            sourceSubgraph: subgraphContext.workflowId,
          },
        });
        
        logger.debug(`Created new context '${parentContextId}' from subgraph output '${internalName}'`, {
          executionId: executionEntity.id,
          messageCount: childContext.messages.length,
        });
      }
    } else {
      logger.warn(`Expected output context '${internalName}' not found in subgraph`, {
        executionId: executionEntity.id,
      });
    }
  }
  
  await executionEntity.exitSubgraph();
}
```

---

## 5. 优势总结

### 5.1 明确的契约

```toml
# 子工作流明确声明接口
[nodes.config]
messageInputs = [
  { externalName = "query", internalName = "q", required = true },
  { externalName = "context", internalName = "ctx", required = false }
]
```

类似函数签名：`function research(q: Messages, ctx?: Messages): Messages`

### 5.2 解耦父子工作流

- ✅ 子工作流不依赖父工作流的 context 命名
- ✅ 父工作流可以自由选择如何传递数据
- ✅ 通过 `externalName/internalName` 映射解耦

### 5.3 类型安全

- ✅ 预处理阶段验证 inputs/outputs 是否匹配
- ✅ 运行时检查 required inputs 是否提供
- ✅ 避免静默失败

### 5.4 灵活性

```toml
# 场景1: 简单传递
inputs = { query = "current" }

# 场景2: 多个输入
inputs = {
  query = "user-question",
  context = "knowledge-base",
  history = "conversation-history"
}

# 场景3: 可选输入（使用 defaultMessages）
messageInputs = [
  { externalName = "optional_context", internalName = "ctx", defaultMessages = [] }
]

# 场景4: 无输出（只读子工作流）
outputs = {}  # 或者不配置 outputs
```

### 5.5 与现有架构一致

- ✅ 复用现有的 MessageContextRegistry
- ✅ 遵循"一切皆消息"的设计理念
- ✅ 与 Context Processor 节点配合使用

---

## 6. 实施路线图

### 阶段 1：类型定义（1-2天）

1. 扩展 `StartNodeConfig` 接口
2. 扩展 `SubgraphNodeConfig` 接口
3. 添加 Zod schema 验证
4. 更新类型导出

### 阶段 2：预处理验证（2-3天）

1. 在图预处理阶段验证 messageInputs/outputs
2. 实现 `validateAndMapMessageContexts` 函数
3. 添加编译时错误提示

### 阶段 3：运行时实现（3-4天）

1. 更新 `enterSubgraph` 函数
2. 更新 `exitSubgraph` 函数
3. 实现 context 复制逻辑
4. 添加日志和错误处理

### 阶段 4：测试与文档（2-3天）

1. 编写单元测试
2. 编写集成测试
3. 更新用户文档
4. 提供迁移指南

---

## 7. 注意事项

### 7.1 性能考虑

- Context 复制使用**浅拷贝**消息数组，避免深拷贝开销
- 对于大上下文，考虑懒加载或引用计数优化

### 7.2 内置 Context 的处理

内置 context（`current`, `system`, `temp`）是否需要特殊处理？

**建议**：不特殊处理，统一通过 messageInputs 传递，保持一致性。

### 7.3 循环引用检测

防止父子工作流之间形成 context 循环依赖：

```toml
# 父工作流
inputs = { query = "result" }  # ← 依赖子的输出

# 子工作流
outputs = { result = "query" }  # ← 依赖父的输入
```

需要在预处理阶段检测并报错。

### 7.4 异步 Subgraph 的限制

对于 `async = true` 的 subgraph：
- inputs 在启动时传递
- outputs 在完成后回调时返回
- 需要确保 context 在异步执行期间不被修改

---

## 8. 未来扩展

### 8.1 Context 转换函数

允许在传递时应用转换：

```toml
inputs = {
  query = {
    source = "current",
    transform = "truncate:lastN:10"  # 传递前裁剪
  }
}
```

### 8.2 Context 合并策略

控制 outputs 如何合并回父工作流：

```toml
outputs = {
  result = {
    target = "research-result",
    mergeMode = "append"  # append | replace | smart
  }
}
```

### 8.3 条件传递

基于条件决定是否传递某些 context：

```toml
inputs = {
  optional_context = {
    source = "maybe-exists",
    if = "{{hasContext('maybe-exists')}}"
  }
}
```

---

## 9. 总结

本设计方案通过扩展 START 节点，为 Subgraph 提供了清晰的 context 传递机制：

✅ **明确的接口契约**：类似函数签名的 inputs/outputs 声明  
✅ **解耦父子工作流**：通过 externalName/internalName 映射  
✅ **类型安全**：预处理验证 + 运行时检查  
✅ **灵活性**：支持必填/可选、默认值、多输入输出  
✅ **与架构一致**：复用现有 Registry，遵循“一切皆消息”理念  

该方案从根本上解决了 context ID 冲突问题，同时保持了配置的简洁性和可维护性。

---

## 10. 实施状态

### ✅ 已完成

- [x] 扩展 `StartNodeConfig` 和 `SubgraphNodeConfig` 类型定义
- [x] 更新 Zod Schema 验证模式
- [x] 实现 `validateAndMapMessageContexts` 验证函数
- [x] 重构 `enterSubgraph` 实现消息上下文传递（父→子）
- [x] 重构 `exitSubgraph` 实现消息上下文返回（子→父）
- [x] 更新 `node-execution-coordinator` 传递 subgraphNode 参数
- [x] 编写单元测试验证功能

### 📝 使用示例

#### 子工作流定义 (research-agent.toml)

```toml
[workflow]
id = "research-agent"
name = "Research Agent"
type = "DEPENDENT"

# START 节点声明输入输出
[[nodes]]
id = "start"
type = "START"
[nodes.config]
# 声明接收的消息上下文
[[nodes.config.messageInputs]]
externalName = "research_query"
internalName = "query"
required = true
description = "The research question or topic"

[[nodes.config.messageInputs]]
externalName = "background_knowledge"
internalName = "knowledge"
required = false
description = "Optional background information"

# 声明输出的消息上下文
[[nodes.config.messageOutputs]]
internalName = "analysis_result"
externalName = "result"
description = "Analysis results"

# 内部节点使用 internalName
[[nodes]]
id = "analyze"
type = "LLM"
[nodes.config]
profileId = "gpt-4"
contextRefs = ["query", "knowledge"]

[[nodes]]
id = "end"
type = "END"
[nodes.config]
outputContexts = ["analysis_result"]
```

#### 父工作流调用 (main-workflow.toml)

```toml
[[nodes]]
id = "call-research"
type = "SUBGRAPH"
[nodes.config]
subgraphId = "research-agent"
async = false

# 传递消息上下文 (使用 externalName)
[nodes.config.messagePassing]
inputs = {
  research_query = "my-query",
  background_knowledge = "kb-context"
}
outputs = {
  result = "research-result"
}
```

### 🔍 关键实现细节

1. **浅拷贝策略**：消息数组使用浅拷贝，避免深拷贝开销，同时防止父子工作流相互影响
2. **强制配置**：`messagePassing` 现在是 SUBGRAPH 节点的必需配置，未配置将抛出错误
3. **严格验证**：所有验证失败都会抛出错误而非警告，确保配置正确性
4. **元数据追踪**：复制的 context 会添加 `sourceContext` 和 `passedFromParent` 等元数据，便于调试
5. **必需参数**：`enterSubgraph` 和 `exitSubgraph` 函数的 `subgraphNode` 参数现在是必需的

### ⚠️ Breaking Changes

从向后兼容版本迁移时需要注意：

1. **所有 SUBGRAPH 节点必须配置 messagePassing**
   ```toml
   # ❌ 旧方式（不再支持）
   [[nodes]]
   id = "call-subgraph"
   type = "SUBGRAPH"
   [nodes.config]
   subgraphId = "child"
   
   # ✅ 新方式（必需）
   [[nodes]]
   id = "call-subgraph"
   type = "SUBGRAPH"
   [nodes.config]
   subgraphId = "child"
   [nodes.config.messagePassing]
   inputs = { query = "current" }
   outputs = { result = "result" }
   ```

2. **缺失的 required context 将抛出错误**
   - 以前：记录警告但继续执行
   - 现在：立即抛出错误，终止执行

3. **输出 context 必须存在**
   - 以前：记录警告
   - 现在：如果声明的输出 context 不存在，抛出错误
