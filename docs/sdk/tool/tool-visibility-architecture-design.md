# 工具可见性架构设计方案

## 1. 设计背景与核心问题

### 1.1 当前系统的问题

当前系统采用"**静态工具描述 + 只增不删**"的策略：

1. **静态初始化**：对话开始时通过系统消息添加初始工具描述
2. **动态增加**：新工具通过 `addDynamicTools()` 动态加入可用集合
3. **执行时拦截**：`ToolCallExecutor` 在执行前检查工具存在性

**核心缺陷**：
- LLM可能基于过期的工具描述生成调用意图
- 作用域切换（子图进入/退出）时，工具可见性未同步更新到LLM上下文
- 执行拦截只能拒绝非法调用，无法预防非法调用意图的生成

### 1.2 LLM工具调用的本质

```
用户输入 → LLM推理 → 生成工具调用请求 → 系统执行工具
                     ↑
               提示词中的工具描述决定
               LLM"知道"哪些工具可用
```

**关键认知**：控制LLM的工具调用行为，必须通过**更新提示词中的工具描述**，而非仅执行时拦截。

## 2. 设计方案：增量式工具可见性声明

### 2.1 核心设计原则

1. **保持KV缓存**：不修改历史消息，避免KV缓存失效
2. **增量声明**：通过新增消息声明当前可用工具集
3. **显式覆盖**：新的声明覆盖旧的声明，形成"有效工具快照"
4. **双重保障**：提示词声明 + 执行拦截，确保安全性

### 2.2 架构组件

#### 2.2.1 ToolVisibilityContext（工具可见性上下文）

```typescript
interface ToolVisibilityContext {
  /** 当前作用域 */
  currentScope: ToolScope;
  
  /** 当前作用域ID（执行实例ID/工作流ID） */
  scopeId: string;  // executionId or workflowId
  
  /** 当前可见工具集合 */
  visibleTools: Set<string>;
  
  /** 可见性声明历史 */
  declarationHistory: VisibilityDeclaration[];
  
  /** 上次声明的消息索引 */
  lastDeclarationIndex: number;
}
```

**注意**：原设计中提到"线程ID/工作流ID"，应改为"执行实例ID/工作流ID"以符合新的命名体系。
interface VisibilityDeclaration {
  timestamp: number;
  scope: ToolScope;
  scopeId: string;
  toolIds: string[];
  messageIndex: number;  // 声明消息在对话中的位置
}
```

#### 2.2.2 ToolVisibilityCoordinator（工具可见性协调器）

**职责**：
- 管理工具可见性上下文
- 生成结构化可见性声明
- 在作用域切换时触发声明更新

**核心方法**：

```typescript
class ToolVisibilityCoordinator {
  /**
   * 初始化可见性上下文
   */
  initializeContext(executionId: string, initialTools: string[]): ToolVisibilityContext;  // 原 threadId
  
  /**
   * 作用域切换时更新可见性
   * 生成并添加新的可见性声明消息
   */
  async updateVisibilityOnScopeChange(
    context: WorkflowExecutionContext,  // 原 ThreadContext
    newScope: ToolScope,
    newScopeId: string,
    availableTools: string[]
  ): Promise<void>;
  
  /**
   * 动态添加工具
   * 生成增量可见性声明
   */
  async addToolsDynamically(
    context: WorkflowExecutionContext,  // 原 ThreadContext
    toolIds: string[],
    scope: ToolScope
  ): Promise<void>;
  
  /**
   * 构建可见性声明消息内容
   */
  buildVisibilityDeclarationMessage(
    scope: ToolScope,
    scopeId: string,
    toolIds: string[],
    changeType: 'init' | 'enter_scope' | 'add_tools' | 'exit_scope'
  ): string;
  
  /**
   * 获取当前有效工具集（用于执行拦截）
   */
  getEffectiveVisibleTools(context: ToolVisibilityContext): Set<string>;
}
```

### 2.3 声明消息格式设计

#### 2.3.1 消息类型选择

**推荐：系统消息（system role）**

理由：
- 工具可见性属于"指令/配置"性质的内容
- 系统消息权重高于用户消息，更易被LLM遵循
- 与现有静态工具描述（系统消息）保持一致

**替代方案：特殊标记的用户消息**

当需要更强烈的上下文切换提示时可用：
```typescript
{
  role: 'user',
  content: '[SYSTEM] 工具可见性更新：...',
  metadata: { type: 'tool_visibility_update' }
}
```

#### 2.3.2 消息内容结构

采用结构化、易解析的格式：

```markdown
## 工具可见性声明

**生效时间**：2024-01-15T10:30:00Z  
**当前作用域**：WORKFLOW(data-analysis-workflow)  
**变更类型**：进入子图

### 当前可用工具清单

| 工具ID | 工具名称 | 说明 |
|--------|----------|------|
| calculator | 计算器 | 执行数学计算 |
| database_query | 数据库查询 | 执行SQL查询 |

### 重要提示

1. **仅可使用上述清单中的工具**，其他工具调用将被拒绝
2. 工具参数必须符合schema定义
3. 如需更多工具，请完成当前任务后退出当前作用域
```

**简化版**（用于token敏感场景）：

```markdown
[TOOL_VISIBILITY]
scope: WORKFLOW(data-analysis-workflow)
tools: calculator,database_query
timestamp: 2024-01-15T10:30:00Z
[/TOOL_VISIBILITY]

**注意**：请仅使用上述工具，其他调用将被系统拒绝。
```

### 2.4 作用域切换流程

#### 2.4.1 进入子图（Subgraph）

```
当前状态：
- 作用域：MAIN_WORKFLOW
- 可见工具：[toolA, toolB, toolC]

执行 enterSubgraph(subgraphId)：
  1. ToolVisibilityCoordinator 接收作用域切换事件
  2. 查询子图的可用工具：[toolD, toolE]
  3. 构建声明消息："进入子图，当前可用：[toolD, toolE]"
  4. 将声明消息添加到对话（系统消息）
  5. 更新可见性上下文
  6. 后续LLM调用只能看到 toolD、toolE
```

#### 2.4.2 退出子图

```
执行 exitSubgraph()：
  1. ToolVisibilityCoordinator 接收退出事件
  2. 恢复父作用域的工具集：[toolA, toolB, toolC]
  3. 构建声明消息："退出子图，恢复工具集：[toolA, toolB, toolC]"
  4. 将声明消息添加到对话
  5. 更新可见性上下文
```

#### 2.4.3 动态添加工具

```
执行 addDynamicTools([toolF])：
  1. 将 toolF 加入当前可见工具集
  2. 构建声明消息："新增工具：[toolF]，当前可用：[toolA, toolB, toolF]"
  3. 将声明消息添加到对话
```

### 2.5 与现有组件的集成

#### 2.5.1 WorkflowExecutionContext 集成（原 ThreadContext）

```typescript
class WorkflowExecutionContext {  // 原 ThreadContext
  private toolVisibilityCoordinator: ToolVisibilityCoordinator;
  private toolVisibilityContext: ToolVisibilityContext;
  
  /**
   * 进入子图时更新工具可见性
   */
  enterSubgraph(workflowId: ID, parentWorkflowId: ID, input: any): void {
    // 原有逻辑：变量作用域切换
    this.variableCoordinator.enterLocalScope(this);
    this.executionState.enterSubgraph(workflowId, parentWorkflowId, input);
    
    // 新增：工具可见性切换
    const subgraphTools = this.getSubgraphAvailableTools(workflowId);
    await this.toolVisibilityCoordinator.updateVisibilityOnScopeChange(
      this,
      'LOCAL',  // 原 WORKFLOW，改为 LOCAL 表示局部/子图作用域
      workflowId,
      subgraphTools
    );
  }
  
  /**
   * 退出子图时恢复工具可见性
   */
  exitSubgraph(): void {
    // 原有逻辑
    this.executionState.exitSubgraph();
    this.variableCoordinator.exitLocalScope(this);
    
    // 新增：恢复父作用域工具可见性
    const parentScopeId = this.executionState.getParentScopeId();
    const parentTools = this.getAvailableToolsForScope(parentScopeId);
    await this.toolVisibilityCoordinator.updateVisibilityOnScopeChange(
      this,
      'EXECUTION',  // 原 THREAD，改为 EXECUTION 表示执行实例级别
      parentScopeId,
      parentTools
    );
  }
  
  /**
   * 动态添加工具
   */
  async addDynamicTools(toolIds: string[]): Promise<void> {
    // 更新可用工具集合
    toolIds.forEach(id => this.availableTools.add(id));
    
    // 新增：生成可见性声明
    await this.toolVisibilityCoordinator.addToolsDynamically(
      this,
      toolIds,
      this.getCurrentToolScope()
    );
  }
}
```

**重要说明**：
- 原设计中的 `THREAD` 作用域应改为 `EXECUTION`，与 `WorkflowExecution` 概念对应
- 原设计中的 `WORKFLOW` 作用域应改为 `LOCAL`，更准确表示局部/子图范围
- 这样命名更符合层次结构：EXECUTION > LOCAL > GLOBAL

#### 2.5.2 ConversationManager 集成

```typescript
class ConversationManager {
  /**
   * 添加工具可见性声明消息
   */
  addToolVisibilityDeclaration(content: string): void {
    // 使用系统消息添加声明
    const message: LLMMessage = {
      role: 'system',
      content: content,
      metadata: {
        type: 'tool_visibility_declaration',
        timestamp: Date.now()
      }
    };
    
    this.addMessage(message);
  }
  
  /**
   * 获取最后一次工具声明后的消息
   * 用于上下文压缩时的参考
   */
  getMessagesSinceLastToolDeclaration(): LLMMessage[] {
    const lastDeclarationIndex = this.findLastToolDeclarationIndex();
    return this.messages.slice(lastDeclarationIndex);
  }
  
  private findLastToolDeclarationIndex(): number {
    for (let i = this.messages.length - 1; i >= 0; i--) {
      if (this.messages[i]?.metadata?.type === 'tool_visibility_declaration') {
        return i;
      }
    }
    return 0;
  }
}
```

#### 2.5.3 ToolCallExecutor 集成

执行拦截逻辑保持不变，但增加可见性检查：

```typescript
class ToolCallExecutor {
  private toolVisibilityCoordinator: ToolVisibilityCoordinator;
  
  async executeSingleToolCall(
    toolCall: { id: string; name: string; arguments: string },
    // ...其他参数
  ): Promise<ToolExecutionResult> {
    // 前置检查1：工具是否存在
    let toolConfig: Tool | undefined;
    try {
      toolConfig = this.toolService.getTool(toolCall.name);
    } catch (error) {
      return this.buildErrorResult(toolCall, `工具 '${toolCall.name}' 不存在`);
    }
    
    // 前置检查2：工具是否在可见性上下文中
    const visibilityContext = this.toolVisibilityCoordinator.getContext(threadId);
    if (!visibilityContext.visibleTools.has(toolCall.name)) {
      return this.buildErrorResult(
        toolCall, 
        `工具 '${toolCall.name}' 在当前作用域不可用。` +
        `当前可用工具：[${Array.from(visibilityContext.visibleTools).join(', ')}]`
      );
    }
    
    // 执行工具调用...
  }
}
```

### 2.6 声明消息频率控制

避免过于频繁的声明消息：

```typescript
interface VisibilityDeclarationStrategy {
  /** 最小声明间隔（毫秒） */
  minDeclarationInterval: number;
  
  /** 是否批量合并声明 */
  batchDeclarations: boolean;
  
  /** 最大批量等待时间 */
  maxBatchWaitTime: number;
  
  /** 作用域切换时强制声明 */
  forceDeclarationOnScopeChange: boolean;
}

// 默认策略
const defaultStrategy: VisibilityDeclarationStrategy = {
  minDeclarationInterval: 1000,  // 1秒内不重复声明
  batchDeclarations: true,        // 批量合并动态添加的工具
  maxBatchWaitTime: 500,         // 最多等待500ms批量
  forceDeclarationOnScopeChange: true  // 作用域切换必须声明
};
```

## 3. 完整流程示例

### 3.1 主工作流执行

```
对话消息流：
1. [System] 初始工具描述：可用工具 [toolA, toolB, toolC]
2. [User] 请分析数据
3. [Assistant] 我需要调用 toolA...
4. [Tool] toolA 执行结果...

当前可见性：toolA, toolB, toolC
```

### 3.2 进入子图（数据分析子工作流）

```
执行 enterSubgraph(data-analysis-workflow)：

5. [System] ## 工具可见性声明
          **当前作用域**：WORKFLOW(data-analysis-workflow)
          **变更类型**：进入子图
          **可用工具**：calculator, database_query
          **注意**：仅可使用上述工具

6. [Assistant] 在数据分析子工作流中，我使用 database_query...

当前可见性：calculator, database_query（覆盖之前的声明）
```

### 3.3 子图内动态添加工具

```
执行 addDynamicTools([chart_generator])：

7. [System] ## 工具可见性声明
          **当前作用域**：WORKFLOW(data-analysis-workflow)
          **变更类型**：新增工具
          **可用工具**：calculator, database_query, chart_generator

8. [Assistant] 现在我可以使用 chart_generator 生成图表...

当前可见性：calculator, database_query, chart_generator
```

### 3.4 退出子图

```
执行 exitSubgraph()：

9. [System] ## 工具可见性声明
          **当前作用域**：WORKFLOW(main-workflow)
          **变更类型**：退出子图
          **可用工具**：toolA, toolB, toolC
          **注意**：已退出子图，恢复主工作流工具集

10. [Assistant] 回到主工作流，我使用 toolA...

当前可见性：toolA, toolB, toolC（恢复主工作流工具集）
```

### 3.5 非法调用拦截示例

```
场景：LLM错误地尝试调用已不可用的 toolD

11. [Assistant] 调用 toolD（此工具已不在当前作用域）

执行拦截：
- ToolCallExecutor 检查可见性上下文
- 发现 toolD 不在 visibleTools 中
- 返回错误：工具 'toolD' 在当前作用域不可用。当前可用工具：[toolA, toolB, toolC]

12. [Tool] 错误：工具 'toolD' 在当前作用域不可用...

13. [Assistant] 抱歉，toolD 已不可用，我改用 toolA...
```

## 4. 边界情况处理

### 4.1 上下文压缩场景

**问题**：工具声明消息可能被压缩，导致LLM丢失可见性信息

**解决方案**：
1. **保留关键声明**：上下文压缩时保留最后一次工具可见性声明
2. **声明恢复**：压缩后添加恢复声明
3. **元数据标记**：声明消息添加特殊元数据，压缩算法识别并保留

```typescript
// 上下文压缩时
function compressContext(messages: LLMMessage[]): LLMMessage[] {
  // 找出最后一次工具声明
  const lastDeclaration = findLastToolDeclaration(messages);
  
  // 执行压缩
  const compressed = performCompression(messages);
  
  // 在压缩后上下文开头添加声明恢复
  if (lastDeclaration) {
    compressed.unshift({
      role: 'system',
      content: `[上下文压缩] 工具可见性恢复：当前可用 [${lastDeclaration.tools.join(', ')}]`,
      metadata: { type: 'tool_visibility_recovery' }
    });
  }
  
  return compressed;
}
```

### 4.2 长对话场景

**问题**：对话过长时，早期的工具声明可能被LLM"遗忘"

**解决方案**：
1. **定期刷新**：每N轮对话后重新声明当前工具集
2. **关键节点声明**：在LLM可能混淆的节点前主动声明

```typescript
// 定期刷新策略
if (conversationTurn % refreshInterval === 0) {
  await toolVisibilityCoordinator.refreshDeclaration(threadContext);
}
```

### 4.3 多工作流并行（FORK/JOIN）

**问题**：FORK后多个分支可能有不同的工具集

**解决方案**：
1. **分支独立上下文**：每个分支维护独立的 `ToolVisibilityContext`
2. **合并时统一声明**：JOIN时合并工具集并重新声明

```typescript
// FORK时
forkBranches.forEach(branch => {
  branch.toolVisibilityContext = toolVisibilityCoordinator.forkContext(
    parentContext,
    branch.id
  );
});

// JOIN时
const mergedTools = mergeToolSets(forkedContexts);
await toolVisibilityCoordinator.updateVisibilityOnScopeChange(
  workflowExecutionContext,  // 原 threadContext
  'EXECUTION',  // 原 THREAD，改为 EXECUTION
  executionId,  // 原 threadId
  mergedTools
);
```

## 5. 实现优先级

### Phase 1：基础框架（MVP）
1. 实现 `ToolVisibilityCoordinator` 核心类
2. 实现声明消息生成与添加
3. 集成到 `ThreadContext` 的作用域切换
4. 基础执行拦截

### Phase 2：增强功能
1. 声明频率控制与批量优化
2. 上下文压缩集成
3. 节点级工具可见性配置

### Phase 3：高级特性
1. 智能声明（根据LLM行为预测）
2. 工具使用统计驱动的动态可见性
3. 可视化调试工具

## 6. 总结

本设计方案通过**增量式可见性声明**机制，在不破坏KV缓存的前提下，实现了动态、准确的工具可见性控制：

| 方面 | 原方案 | 新方案 |
|------|--------|--------|
| **更新方式** | 静态初始化，不更新 | 增量声明，动态更新 |
| **KV缓存** | 不受影响 | 不受影响 |
| **实现复杂度** | 低 | 中等 |
| **可见性准确性** | 低 | 高 |
| **安全性** | 依赖执行拦截 | 声明+拦截双重保障 |
| **LLM体验** | 可能混淆 | 清晰明确 |

此方案在保持系统性能的同时，有效解决了工具可见性的核心问题。