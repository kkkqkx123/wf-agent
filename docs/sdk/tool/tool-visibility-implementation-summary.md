# 工具可见性架构实现总结

## 实现概述

基于设计文档 `docs/tool-visibility-architecture-design.md`，成功实现了增量式工具可见性声明机制，解决了工具可见性控制的核心问题。

## 核心组件

### 1. 类型定义 (`sdk/core/execution/types/tool-visibility.types.ts`)

定义了工具可见性相关的核心类型：

- `ToolVisibilityContext`: 工具可见性上下文
- `VisibilityDeclaration`: 可见性声明历史记录
- `VisibilityDeclarationStrategy`: 声明策略配置
- `VisibilityChangeType`: 变更类型
- `VisibilityUpdateRequest`: 更新请求
- `defaultVisibilityDeclarationStrategy`: 默认策略

### 2. 工具可见性协调器 (`sdk/core/execution/coordinators/tool-visibility-coordinator.ts`)

核心协调器类，负责：

- 管理工具可见性上下文
- 生成结构化可见性声明消息
- 在作用域切换时触发声明更新
- 支持动态添加工具
- 批量声明优化
- 快照和恢复功能

**关键方法**：
- `initializeContext()`: 初始化可见性上下文
- `updateVisibilityOnScopeChange()`: 作用域切换时更新可见性
- `addToolsDynamically()`: 动态添加工具
- `buildVisibilityDeclarationMessage()`: 构建声明消息
- `isToolVisible()`: 检查工具可见性

### 3. ThreadContext 集成 (`sdk/core/execution/context/thread-context.ts`)

在 ThreadContext 中集成了工具可见性协调器：

- 添加 `toolVisibilityCoordinator` 属性
- 实现 `initializeToolVisibility()` 方法
- 更新 `enterSubgraph()` 和 `exitSubgraph()` 为异步方法
- 更新 `addDynamicTools()` 为异步方法
- 在快照和恢复中包含工具可见性状态

### 4. ToolCallExecutor 增强 (`sdk/core/execution/executors/tool-call-executor.ts`)

在工具执行前添加可见性检查：

- 构造函数接收 `ToolVisibilityCoordinator` 参数
- 在 `executeSingleToolCall()` 中添加可见性验证
- 如果工具不可见，返回明确的错误信息

### 5. 子图处理器更新 (`sdk/core/execution/handlers/subgraph-handler.ts`)

更新子图进入/退出函数为异步：

- `enterSubgraph()`: 改为异步函数
- `exitSubgraph()`: 改为异步函数

### 6. ADD_TOOL 节点处理器增强 (`sdk/core/execution/handlers/node-handlers/add-tool-handler.ts`)

在添加工具时更新可见性：

- 在 `AddToolHandlerContext` 中添加 `threadContext` 参数
- 添加工具后调用 `threadContext.addDynamicTools()` 生成可见性声明

## 集成点更新

### 1. ThreadBuilder (`sdk/core/execution/thread-builder.ts`)

在所有创建 ThreadContext 的地方添加初始化调用：

- `buildThread()`: 添加 `initializeToolVisibility()`
- `copyThread()`: 添加 `initializeToolVisibility()`
- `forkThread()`: 添加 `initializeToolVisibility()`

### 2. CheckpointCoordinator (`sdk/core/execution/coordinators/checkpoint-coordinator.ts`)

在恢复检查点时初始化工具可见性上下文。

### 3. LLMExecutionCoordinator (`sdk/core/execution/coordinators/llm-execution-coordinator.ts`)

在创建 ToolCallExecutor 时传递 ToolVisibilityCoordinator。

### 4. NodeExecutionCoordinator (`sdk/core/execution/coordinators/node-execution-coordinator.ts`)

- 更新子图进入/退出调用为异步
- 在 ADD_TOOL 节点处理器上下文中传递 threadContext

## 工作流程

### 1. 初始化流程

```
ThreadBuilder.buildThread()
  → 创建 ThreadContext（包含 ToolVisibilityCoordinator）
  → initializeToolVisibility()
  → 初始化可见性上下文（THREAD 作用域）
```

### 2. 作用域切换流程

```
进入子图：
  NodeExecutionCoordinator.executeNodeLogic()
    → enterSubgraph() (异步)
      → ToolVisibilityCoordinator.updateVisibilityOnScopeChange()
        → 生成可见性声明消息
        → 添加到对话历史
        → 更新可见性上下文

退出子图：
  NodeExecutionCoordinator.executeNodeLogic()
    → exitSubgraph() (异步)
      → ToolVisibilityCoordinator.updateVisibilityOnScopeChange()
        → 恢复父作用域工具集
        → 生成可见性声明消息
```

### 3. 动态添加工具流程

```
ADD_TOOL 节点执行：
  addToolHandler()
    → ToolContextManager.addTools()
    → ThreadContext.addDynamicTools() (异步)
      → ToolVisibilityCoordinator.addToolsDynamically()
        → 批量声明或立即声明
        → 生成可见性声明消息
```

### 4. 工具执行流程

```
LLM 生成工具调用：
  ToolCallExecutor.executeSingleToolCall()
    → 检查工具是否存在
    → 检查工具是否在可见性上下文中
      → 如果不可见，返回错误
    → 执行工具调用
```

## 声明消息格式

生成的可见性声明消息采用结构化格式：

```markdown
## 工具可见性声明

**生效时间**：2024-01-15T10:30:00Z
**当前作用域**：WORKFLOW(data-analysis-workflow)
**变更类型**：进入子图

### 当前可用工具清单

| 工具名称 | 工具ID | 说明 |
|----------|--------|------|
| calculator | calculator | 执行数学计算 |
| database_query | database_query | 执行SQL查询 |

### 重要提示

1. **仅可使用上述清单中的工具**，其他工具调用将被拒绝
2. 工具参数必须符合schema定义
3. 如需更多工具，请完成当前任务后退出当前作用域
```

## 关键特性

### 1. 增量式声明

- 不修改历史消息，保持 KV 缓存
- 通过新增系统消息声明当前工具集
- 新声明覆盖旧声明

### 2. 双重保障

- **提示词声明**：LLM 在生成调用前就知道可用工具
- **执行拦截**：执行时再次验证工具可见性

### 3. 批量优化

- 支持批量合并动态添加的工具
- 可配置批量等待时间
- 作用域切换时强制声明

### 4. 快照支持

- 支持创建和恢复可见性上下文快照
- 与检查点机制集成

## 配置选项

通过 `VisibilityDeclarationStrategy` 可配置：

```typescript
{
  minDeclarationInterval: 1000,      // 最小声明间隔（毫秒）
  batchDeclarations: true,           // 是否批量合并声明
  maxBatchWaitTime: 500,            // 最大批量等待时间（毫秒）
  forceDeclarationOnScopeChange: true,  // 作用域切换时强制声明
  refreshInterval?: number          // 定期刷新间隔（轮数）
}
```

## 向后兼容性

所有修改保持了向后兼容性：

- `enterSubgraph()` 和 `exitSubgraph()` 改为异步，但调用处已更新
- `addDynamicTools()` 改为异步，但调用处已更新
- 新增的 `threadContext` 参数在 `AddToolHandlerContext` 中是可选的

## 测试建议

建议添加以下测试场景：

1. **作用域切换测试**
   - 进入子图时工具可见性更新
   - 退出子图时工具可见性恢复
   - 多层子图嵌套

2. **动态添加工具测试**
   - 单个工具添加
   - 批量工具添加
   - 批量声明优化

3. **执行拦截测试**
   - 调用不可见工具被拒绝
   - 错误信息包含当前可用工具列表

4. **快照恢复测试**
   - 创建可见性上下文快照
   - 从快照恢复可见性上下文

5. **边界情况测试**
   - 上下文压缩后的声明恢复
   - 长对话中的定期刷新
   - FORK/JOIN 场景

## 总结

本次实现完整地按照设计文档完成了工具可见性架构的改进，通过增量式声明机制实现了：

1. ✅ 动态、准确的工具可见性控制
2. ✅ 保持 KV 缓存性能
3. ✅ 双重安全保障（声明+拦截）
4. ✅ 清晰的 LLM 体验
5. ✅ 完整的快照支持

所有核心功能已实现并集成到现有系统中，可以开始测试和验证。