# 工具作用域、描述和可用性处理逻辑分析

## 一、工具作用域处理逻辑

### 1. 作用域类型定义
系统支持三种工具作用域（[`ToolContextManager.ts:23`](sdk/core/execution/managers/tool-context-manager.ts:23)）：
- **EXECUTION**：执行实例作用域，仅当前工作流执行实例可用（原THREAD）
- **LOCAL**：局部作用域，当前子图或局部上下文可用
- **GLOBAL**：全局作用域，所有执行实例可用

**命名问题分析与改进**：

原有命名存在严重问题：
1. **THREAD** - 与新的`WorkflowExecution`概念不一致，容易引起混淆
2. **LOCAL** - 语义模糊，不清楚是相对于什么的"局部"
3. 作用域层次不清晰

**推荐的新命名方案**：

| 当前命名 | 建议命名 | 说明 |
|---------|---------|------|
| THREAD | EXECUTION | 工作流执行实例级别，与WorkflowExecution对应 |
| LOCAL | SUBGRAPH 或 CONTEXT | 更明确表示子图或上下文级别 |
| GLOBAL | GLOBAL | 保持不变，语义清晰 |

或者采用更清晰的三层架构：
- **INSTANCE** - 执行实例级别（最具体）
- **WORKFLOW** - 工作流定义级别（中等范围）
- **GLOBAL** - 全局级别（最广泛）

### 2. 作用域管理架构
核心管理器：[`ToolContextManager`](sdk/core/execution/managers/tool-context-manager.ts:66)

**数据结构**（[`ToolContextManager.ts:42-49`](sdk/core/execution/managers/tool-context-manager.ts:42-49)）：
```typescript
interface ToolContext {
  executionTools: Map<string, ToolMetadata>;    // 执行实例作用域工具（原threadTools）
  localTools: Map<string, ToolMetadata>;        // 局部作用域工具
  globalTools: Map<string, ToolMetadata>;       // 全局作用域工具
}
```

**关键特性**：
- 执行实例隔离：每个工作流执行实例维护独立的工具上下文（[`ToolContextManager.ts:68`](sdk/core/execution/managers/tool-context-manager.ts:68））
- 原子操作：提供增删改查的原子化方法
- 快照支持：支持保存和恢复工具上下文快照（[`ToolContextManager.ts:343-368`](sdk/core/execution/managers/tool-context-manager.ts:343-368））

### 3. 作用域操作方法

**添加工具**（[`ToolContextManager.ts:96-144`](sdk/core/execution/managers/tool-context-manager.ts:96-144)）：
```typescript
addTools(
  executionId: string,      // 原threadId
  workflowId: ID,
  toolIds: string[],
  scope: ToolScope = 'EXECUTION',  // 原THREAD
  overwrite: boolean = false,
  descriptionTemplate?: string,
  customMetadata?: Record<string, any>
): number
```

**查询工具**（[`ToolContextManager.ts:153-177`](sdk/core/execution/managers/tool-context-manager.ts:153-177)）：
- 支持按作用域查询
- 支持查询所有作用域的工具（合并返回）
- 返回工具ID集合

**移除工具**（[`ToolContextManager.ts:220-263`](sdk/core/execution/managers/tool-context-manager.ts:220-263)）：
- 支持按作用域移除
- 支持从所有作用域移除

### 4. 作用域优先级
在查询工具时，系统会按以下优先级查找：
1. EXECUTION 作用域（最高优先级，原THREAD）
2. LOCAL 作用域（局部/子图级别）
3. GLOBAL 作用域（最低优先级）

## 二、工具描述处理方式

### 1. 描述定义
工具描述在配置文件中定义（[`calculator.toml:4`](configs/tools/stateless/calculator.toml:4)）：
```toml
description = "A tool for performing basic mathematical calculations"
```

### 2. 描述模板支持
系统支持动态描述模板（[`ToolContextManager.ts:32`](sdk/core/execution/managers/tool-context-manager.ts:32)）：
```typescript
interface ToolMetadata {
  toolId: string;
  descriptionTemplate?: string;  // 描述模板
  customMetadata?: Record<string, any>;
  addedAt: number;
}
```

### 3. 描述验证
**静态验证**（[`tool-static-validator.ts:117`](sdk/core/validation/tool-static-validator.ts:117)）：
- 工具注册时验证描述字段必须存在且非空
- 使用 Zod schema 进行验证

**验证规则**（[`tool-static-validator.ts:111-142`](sdk/core/validation/tool-static-validator.ts:111-142)）：
```typescript
const toolSchema = z.object({
  id: z.string().min(1, 'Tool ID is required'),
  name: z.string().min(1, 'Tool name is required'),
  type: z.custom<ToolType>(...),
  description: z.string().min(1, 'Tool description is required'),  // 必填
  parameters: toolParametersSchema,
  metadata: toolMetadataSchema.optional(),
  config: z.any().optional(),
});
```

### 4. 描述使用场景
1. **工具搜索**（[`tool-registry.ts:149-159`](sdk/core/tools/tool-registry.ts:149-159)）：
   - 按描述关键词搜索工具
   - 支持模糊匹配

2. **LLM调用**（[`llm-executor.ts:30`](sdk/core/execution/executors/llm-executor.ts:30)）：
   - 将工具描述传递给LLM
   - 支持动态工具配置和描述模板

## 三、工具可用性处理方式

### 1. 配置层可用性控制
**启用标志**（[`calculator.toml:5`](configs/tools/stateless/calculator.toml:5)）：
```toml
enabled = true  # 控制工具是否启用
```

**工具集注册**（[`__registry__.toml:40-48`](configs/tools/__registry__.toml:40-48)）：
```toml
[tool_sets.basic_tools]
description = "基础工具集"
enabled = true
tools = ["calculator", "fetch", "weather", "time_tool"]
```

### 2. 注册层可用性验证
**静态验证器**（[`tool-static-validator.ts`](sdk/core/validation/tool-static-validator.ts)）：
- 验证工具定义的完整性
- 验证参数schema的有效性
- 验证配置结构的正确性

**验证流程**（[`tool-service.ts:55-62`](sdk/core/services/tool-service.ts:55-62)）：
```typescript
registerTool(tool: Tool): void {
  // 静态验证工具定义
  const result = this.staticValidator.validateTool(tool);
  if (result.isErr()) {
    throw result.error[0];
  }
  this.registry.register(tool);
}
```

### 3. 执行层可用性检查
**运行时验证器**（[`tool-runtime-validator.ts`](sdk/core/validation/tool-runtime-validator.ts)）：
- 验证参数值的类型
- 验证参数值的格式
- 验证枚举值的有效性

**执行前检查**（[`tool-service.ts:169-189`](sdk/core/services/tool-service.ts:169-189)）：
```typescript
// 运行时验证参数
try {
  this.runtimeValidator.validate(tool, parameters);
} catch (error) {
  if (error instanceof RuntimeValidationError) {
    return err(new ToolError(...));
  }
  return err(new ToolError(...));
}
```

### 4. 工具存在性检查
**工具查询**（[`tool-service.ts:86-95`](sdk/core/services/tool-service.ts:86-95)）：
```typescript
getTool(toolId: string): Tool {
  const tool = this.registry.get(toolId);
  if (!tool) {
    throw new ToolNotFoundError(
      `Tool with id '${toolId}' not found`,
      toolId
    );
  }
  return tool;
}
```

**存在性检查**（[`tool-service.ts:137-139`](sdk/core/services/tool-service.ts:137-139)）：
```typescript
hasTool(toolId: string): boolean {
  return this.registry.has(toolId);
}
```

### 5. 执行器可用性
**执行器映射**（[`tool-service.ts:43-49`](sdk/core/services/tool-service.ts:43-49)）：
```typescript
private initializeExecutors(): void {
  this.executors.set('STATELESS', new StatelessExecutor());
  this.executors.set('STATEFUL', new StatefulExecutor());
  this.executors.set('REST', new RestExecutor());
  this.executors.set('MCP', new McpExecutor());
}
```

**执行器检查**（[`tool-service.ts:159-167`](sdk/core/services/tool-service.ts:159-167)）：
```typescript
const executor = this.executors.get(tool.type);
if (!executor) {
  return err(new ToolError(
    `No executor found for tool type '${tool.type}'`,
    toolId,
    tool.type,
    { parameters }
  ));
}
```

## 四、总结

### 工具作用域处理
- 采用三层作用域架构（THREAD/WORKFLOW/GLOBAL）
- 通过 `ToolContextManager` 实现线程隔离和原子操作
- 支持作用域优先级查询和快照恢复

### 工具描述处理
- 配置文件定义静态描述
- 支持描述模板动态生成
- 注册时进行静态验证
- 用于工具搜索和LLM调用

### 工具可用性处理
- 配置层：通过 `enabled` 标志控制
- 注册层：静态验证工具定义完整性
- 执行层：运行时验证参数有效性
- 执行器层：检查工具类型对应的执行器是否存在

整个系统通过分层验证和多层管理机制，确保工具的正确注册、安全执行和有效管理。



# Workflow可见域分析与LLM工具可用性架构改进方案

## 一、当前Workflow可见域逻辑分析

### 1.1 可见域层级结构
系统当前采用三层作用域架构：

| 作用域 | 范围 | 管理组件 |
|--------|------|----------|
| **EXECUTION** | 当前执行实例隔离（原THREAD） | `ToolContextManager.executionTools` |
| **LOCAL** | 局部/子图内共享 | `ToolContextManager.localTools` |
| **GLOBAL** | 全局共享 | `ToolContextManager.globalTools` |

**命名问题说明**：
原有THREAD/WORKFLOW/LOCAL命名存在严重问题，建议改为EXECUTION/LOCAL/GLOBAL或INSTANCE/SUBGRAPH/GLOBAL

### 1.2 当前可见域处理流程
1. **工具注册阶段**：通过 `ToolService.registerTool()` 注册到全局注册表
2. **上下文初始化**：`ThreadContext` 在构造时从 `PreprocessedGraph` 加载可用工具到 `availableTools` 集合
3. **工具描述添加**：`ConversationManager.getInitialToolDescriptionMessage()` 构建系统消息，静态描述可用工具
4. **执行时检查**：`ToolCallExecutor.executeSingleToolCall()` 调用前通过 `ToolService.getTool()` 验证工具存在性

### 1.3 核心问题识别

**问题1：可见性控制点错位**
- 当前系统仅在**工具调用执行阶段**进行权限检查（`ToolService.execute()`）
- 但LLM在**生成响应阶段**就已经决定调用哪些工具
- 执行阶段的检查只能拒绝非法调用，无法阻止LLM生成非法调用意图

**问题2：消息数组静态化**
- 工具描述以**系统消息**形式在对话开始时静态添加（`ConversationManager:553-582`）
- 一旦添加，不会随作用域变化动态调整
- 子图切换、FORK/JOIN等场景下工具可见性已变，但消息数组未更新

**问题3：缺乏作用域感知的工具过滤**
- `LLMExecutionRequestData` 虽然支持传入 `tools` 参数（`llm-executor.ts:30`）
- 但实际调用时未根据当前作用域动态过滤
- 所有注册工具都可能被传递给LLM

## 二、LLM工具可用性本质问题

### 2.1 LLM工具调用的决策机制
```
用户输入 → LLM推理 → 生成工具调用请求 → 系统执行工具
                     ↑
               工具schema决定
               LLM"知道"哪些工具
```

**关键点**：LLM是否调用某工具，取决于**提示词中是否包含该工具的schema**，而非执行时的权限检查。

### 2.2 当前架构的缺陷
```
当前流程：
1. 系统消息：包含所有工具描述（静态）
2. LLM调用：看到所有工具 → 可能调用任何工具
3. 执行检查：验证工具是否存在/可用（后置检查）
                    ↓
            问题：LLM已生成非法调用意图
```

**本质问题**：工具可见性控制应该在**LLM调用前**完成，而不是**工具执行时**。


@/docs/tool-visibility-architecture-design.md 基于该文档给出修改方案，并执行代码修改任务