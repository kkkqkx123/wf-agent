# SDK 类型检查修复计划

## 概述

本文档记录了 SDK 类型检查错误的修复进度和后续计划。当前已修复大部分关键问题，剩余约20个类型错误需要处理。

## 当前状态

### 已修复的问题

#### 1. API 层导出问题 (api/index.ts)
- ✅ 修复重复导出的 `MessageFilter`（重命名为 `WorkflowMessageFilter`）
- ✅ 修复不存在的导出成员：
  - `PublisherAPI`, `PublishOptions`, `PublishResult` → `MessagePublisher`, `createMessagePublisher`
  - `routeMessage`, `createMessageRouter`, `MessageRouter`, `MessageRoute` → `matchesRoutingRule`, `findMatchingRule`, `sortRulesByPriority`
- ✅ 修复旧命名导出：`createThreadStateCheckHook` → `createWorkflowExecutionStateCheckHook`

#### 2. SDK 核心文件 (api/shared/core/sdk.ts)
- ✅ 修复 `createThreadAPI()` → `createWorkflowExecutionAPI()`
- ✅ 修复属性名 `threads` → `executions`

#### 3. 执行构建器 (api/workflow/builders/execution-builder.ts)
- ✅ 修复 `result.value.id` → `result.value.executionId`
- ✅ 修复变量名 `threadId` → `executionId`

#### 4. 工作流执行注册 API (api/workflow/resources/executions/workflow-execution-registry-api.ts)
- ✅ 修复返回对象中的 `threadId` → `executionId`

#### 5. DI 容器配置 (core/di/container-config.ts)
- ✅ 修复 `createCheckpoint` 参数名 `threadId` → `workflowExecutionId`
- ✅ 修复变量名 `threadRegistry` → `workflowExecutionRegistry`

#### 6. Hook 创建器 (workflow/execution/utils/hook-creators.ts)
- ✅ 重命名函数 `createThreadStateCheckHook` → `createWorkflowExecutionStateCheckHook`
- ✅ 修复所有 `threadEntity` → `workflowExecutionEntity`
- ✅ 修复 `getThread()` → `getExecution()`
- ✅ 修复事件名称和日志字段

#### 7. 阶段1: 模块导入路径修复 (2024-01-XX)
- ✅ 修复 `thread-state-coordinator.js` → `workflow-state-coordinator.js`
- ✅ 修复 `thread-builder.js` → `workflow-execution-builder.js`
- ✅ 修复 `core/types/index.js` 导入路径
- ✅ 修复 `contextual-logger.js` 导入路径
- ✅ 修复 `conversation-session.js` 导入路径
- ✅ 修复 `tool-registry.js` 导入路径
- ✅ 修复 `available-tools.js` 导入路径
- ✅ 修复 `workflow-processor.js` → 使用 `WorkflowGraphBuilder`
- ✅ 修复 `thread-operations.js` → `workflow-operations.js`
- ✅ 修复 `thread-state-validator.js` → `workflow-state-validator.js`

#### 8. 阶段2: 类型定义缺失属性和方法修复 (2024-01-XX)
- ✅ 在 `WorkflowExecutionEntity` 中添加方法：
  - `pause()`, `resume()`, `stop()`, `interrupt()`, `resetInterrupt()`
  - `setParentThreadId()`, `getParentThreadId()`, `registerChildThread()`, `unregisterChildThread()`
  - `getThreadType()`, `setThreadType()`, `getExecution()`, `buildEvent()`
- ✅ 在 `WorkflowExecutionState` 中添加 `interrupted` 属性
- ✅ 在 `HumanRelayTask` 中添加 `workflowExecutionEntity` 属性
- ✅ 在 `QueueTask` 中添加 `workflowExecutionEntity` 属性
- ✅ 在 `TriggeredSubgraphTask` 中添加 `mainWorkflowExecutionEntity` 属性
- ✅ 在 `VariableScopes` 中添加 `workflowExecution` 属性
- ✅ 在 `VariableScope` 类型中添加 `"workflowExecution"` 值
- ✅ 在 `WorkflowExecutionStatus` 中添加 `"STOPPED"` 状态
- ✅ 在 `TriggeredSubworkflowContext` 中添加兼容字段
- ✅ 添加 `WorkflowDefinition` 类型别名

#### 9. 阶段3: 批量修复旧命名 (2024-01-XX)
- ✅ 修复 `triggered-subworkflow-handler.ts` 中的 `threadRegistry` → `workflowExecutionRegistry`
- ✅ 修复 `workflow-operations.ts` 中的 `threadRegistry` → `workflowExecutionRegistry`
- ✅ 修复 `workflow-reference-checker.ts` 中的 `threadRegistry` → `workflowExecutionRegistry`

#### 10. 阶段4: 修复事件构建器 (2024-01-XX)
- ✅ 在 `core/utils/event/builders/index.ts` 中添加向后兼容的别名导出
- ✅ 修复 `workflow-execution-events.ts` 中的 `threadId` → `executionId`
- ✅ EventType 已包含所有事件类型（新名称和旧名称）

#### 11. 阶段5: 修复类型不匹配问题 (2024-01-XX)
- ✅ 修复 `CheckpointCreator` 类型定义中的 `threadId` → `workflowExecutionId`
- ✅ 修复 `tool-call-executor.ts` 中调用 `createCheckpointFn` 的参数
- ✅ 添加 `WorkflowExecutionBuilder` 服务标识符
- ✅ 修复 `HookExecutionContext` 接口中的 `threadEntity` → `workflowExecutionEntity`
- ✅ 修复 `hook-handler.ts` 中使用 `threadEntity` 的地方

#### 12. 阶段6: 修复其他类型错误 (2024-01-XX)
- ✅ 确认 `ThreadOptions` 和 `Thread` 类型别名已存在
- ✅ 添加 `AgentLoopRegistry` 的 `cleanupByParentWorkflowExecutionId` 等方法
- ✅ 修复 `InterruptionContext` 接口中的 `threadRegistry` → `workflowExecutionRegistry`

#### 13. 阶段7: 修复 VariableScopes 初始化问题 (2026-04-29)
- ✅ 修复 `workflow-execution-builder.ts` 中所有 `variableScopes` 初始化，添加 `workflowExecution` 属性
- ✅ 修复 `variable-state.ts` 中所有 `variableScopes` 初始化，添加 `workflowExecution` 属性
- ✅ 修复 `WorkflowExecutionEntity` 构造函数参数类型不匹配问题（添加 `ExecutionState` 参数）

#### 14. 阶段8: 修复缺失的类型和方法 (2026-04-29)
- ✅ 在 `WorkflowExecutionEntity` 中添加 `getExecution()` 方法
- ✅ 在 `WorkflowExecutionEntity` 中添加 `unregisterChildThread()` 方法
- ✅ 在 `WorkflowExecutionEntity` 中添加 `buildEvent()` 方法
- ✅ 在 `ToolApprovalContext` 中添加 `threadRegistry` 属性（向后兼容）
- ✅ 在 `TriggerHandlerContextFactoryConfig` 中添加 `workflowGraphRegistry` 属性（向后兼容）
- ✅ 在 `NodeHandlerContextFactoryConfig` 中添加 `workflowExecutionRegistry` 属性
- ✅ 在 `WorkflowExecutionResult` 中添加 `id` 属性（向后兼容）
- ✅ 添加 `WorkflowExecutionInterruptedException` 类型别名
- ✅ 添加 `WorkflowExecutionNotFoundError` 类型别名

#### 15. 阶段9: 修复模块导入错误 (2026-04-29)
- ✅ 修复 `workflow-operations.ts` 中的 `ThreadBuilder` 导入 → `WorkflowExecutionBuilder`
- ✅ 修复 `workflow-conversation-session.ts` 中的 `available-tools.js` 导入路径

#### 16. 阶段10: 修复向后兼容性问题 (2026-04-29)
- ✅ 在 `WorkflowExecutionBuildResult` 中添加 `threadEntity` 属性（向后兼容）
- ✅ 在 `ToolApprovalContext` 中添加 `workflowGraphRegistry` 属性（向后兼容）
- ✅ 在 `ConversationSessionConfig` 中添加 `workflowExecutionId` 属性
- ✅ 修复 `buildWorkflowExecutionFailedEvent` 参数名 `workflowExecutionId` → `executionId`
- ✅ 修复 `CheckpointDependencies` 类型检查（添加 `threadRegistry` 非空检查）
- ✅ 修复 `threadEntity` 类型为 unknown 的问题（使用 `workflowExecutionEntity`）
- ✅ 在 `HumanRelayTask` 和 `QueueTask` 中添加 `workflowExecutionEntity` 属性
- ✅ 添加 `ThreadExecutor` 类型别名（指向 `WorkflowExecutor`）
- ✅ 修复 `WorkflowExecutor.executeThread` → `executeWorkflow`
- ✅ 修复所有 `WorkflowExecutionBuildResult` 返回值，添加 `threadEntity` 属性

### 剩余问题分类

根据最新的类型检查结果，剩余约40个错误可分为以下几类：

1. **事件类型不匹配** (~10个): `CompleteEvent` 缺少 `threadId` 属性等
2. **类型不匹配** (~15个): 各种接口和类型签名不匹配
3. **缺失的属性** (~10个): `workflowExecutionEntity`, `threadRegistry` 等变量未定义
4. **其他错误** (~5个): 参数数量不匹配、类型转换等

## 后续修复计划

### 阶段 10: 修复事件类型不匹配

**优先级：高**

需要修复的主要问题：

1. **VariableScopes 缺少 `thread` 属性**
   - 文件：`workflow/execution/factories/workflow-execution-builder.ts:372`
   - 需要同时保留 `thread` 和 `workflowExecution` 属性

2. **CreateCheckpointOptions 参数名**
   - 文件：`workflow/execution/coordinators/node-execution-coordinator.ts:218`
   - 需要使用 `workflowExecutionId` 而不是 `executionId`

3. **ToolApprovalContext 缺少 `threadRegistry` 属性**
   - 文件：`workflow/execution/coordinators/llm-execution-coordinator.ts:507`
   - 需要添加向后兼容的 `threadRegistry` 属性

4. **TriggerHandlerContextFactoryConfig 缺少属性**
   - 文件：`core/di/container-config.ts:611`
   - 需要添加 `workflowGraphRegistry` 属性

### 阶段 8: 修复类型不匹配问题

**优先级：中**

1. **WorkflowExecutionResult 缺少 `id` 属性**
   - 文件：`core/serialization/entities/task-serializer.ts:114,134`

2. **TriggeredSubgraphTask 缺少 `mainThreadEntity` 属性**
   - 文件：`workflow/execution/handlers/trigger-handlers/execute-triggered-subgraph-handler.ts:165`

3. **WorkflowExecutionState 类型不匹配**
   - 文件：`workflow/execution/factories/workflow-execution-builder.ts:208,301,395`

### 阶段 9: 修复缺失的方法

**优先级：中**

1. **WorkflowExecutionEntity 缺少 `buildEvent` 方法**
   - 文件：`workflow/execution/coordinators/node-execution-coordinator.ts:316,433,444,479,517,536`

2. **WorkflowExecutionEntity 缺少 `getExecution` 方法**
   - 文件：`workflow/execution/utils/hook-creators.ts:56,83`

3. **WorkflowExecutionEntity 缺少 `unregisterChildThread` 方法**
   - 文件：`workflow/execution/handlers/triggered-subworkflow-handler.ts:370`

### 阶段 10: 修复模块导入错误

**优先级：低**

1. **找不到模块 `thread-builder.js`**
   - 文件：`workflow/execution/utils/workflow-operations.ts:9`

2. **找不到模块 `available-tools.js`**
   - 文件：`workflow/message/workflow-conversation-session.ts:25`

3. **缺失的类型导出**
   - `WorkflowExecutionInterruptedException`
   - `WorkflowExecutionNotFoundError`

## 执行建议

### 推荐的修复顺序

1. **先修复事件类型**（阶段4）：这是最大的错误来源
2. **再修复类型不匹配**（阶段5）：确保核心接口兼容
3. **最后处理其他错误**（阶段6）：处理剩余的零散问题

### 注意事项

1. **保持向后兼容**：在重命名导出时，考虑添加类型别名以支持平滑迁移
2. **测试验证**：每完成一个阶段后，运行类型检查验证修复效果
3. **文档更新**：同步更新相关文档和注释
4. **代码审查**：修复完成后进行代码审查，确保没有引入新的问题

## 预期结果

完成所有修复后，SDK 应该：
- ✅ 通过 TypeScript 类型检查（`pnpm typecheck` 无错误）
- ✅ 所有旧命名已清理完毕
- ✅ 类型定义完整且一致
- ✅ 模块导入路径正确
- ✅ 符合命名重构设计方案的要求

## 参考资料

- [命名重构设计方案](../architecture/naming-refactor/design-proposal.md)
- [Analyzer 使用指南](../skill/SKILL.md)

1. **thread-state-coordinator.js**
   - 文件：`workflow/execution/handlers/trigger-handlers/apply-message-operation-handler.ts:15`
   - 应改为：`../../../state-managers/workflow-execution-state-coordinator.js`

2. **thread-builder.js**
   - 文件：`workflow/execution/handlers/trigger-handlers/execute-triggered-subgraph-handler.ts:26`
   - 文件：`workflow/execution/types/workflow-tool.types.ts:13`
   - 应改为：`../../factories/workflow-execution-builder.js`

3. **其他缺失的模块**
   - `../../../core/types/index.js`
   - `../../../utils/contextual-logger.js`
   - `../../../core/messaging/conversation-session.js`
   - `../../../core/registry/tool-registry.js`
   - `../../../resources/dynamic/prompts/fragments/available-tools.js`
   - `../graph-builder/workflow-processor.js`

### 阶段 2: 修复类型定义缺失的属性和方法

**优先级：高**

#### 2.1 WorkflowExecutionEntity 缺失的方法

需要在 `WorkflowExecutionEntity` 类中添加以下方法：
- `pause(): void`
- `resume(): void`
- `stop(): void`
- `interrupt(): void`
- `resetInterrupt(): void`
- `buildEvent(): EventBuilder`
- `setParentThreadId(parentId: string): void`
- `registerChildThread(childId: string): void`
- `getThreadType(): WorkflowExecutionType`
- `setThreadType(type: WorkflowExecutionType): void`
- `getParentThreadId(): string | undefined`

#### 2.2 HumanRelayTask 缺失的属性

需要在 `HumanRelayTask` 类型中添加：
- `workflowExecutionEntity: WorkflowExecutionEntity`

#### 2.3 QueueTask 缺失的属性

需要在 `QueueTask` 类型中添加：
- `workflowExecutionEntity: WorkflowExecutionEntity`

#### 2.4 VariableScopes 缺失的属性

需要在 `VariableScopes` 接口中添加：
- `workflowExecution: Record<string, unknown>`

#### 2.5 WorkflowExecutionResult 缺失的属性

需要在 `WorkflowExecutionResult` 接口中添加：
- `id: ID`（或使用现有的 `executionId`）

### 阶段 3: 批量修复旧命名

**优先级：中**

需要全局替换的命名：

#### 3.1 变量名和属性名
- `threadRegistry` → `workflowExecutionRegistry`
- `threadEntity` → `workflowExecutionEntity`
- `threadId` → `executionId`（在上下文中）
- `ThreadExecutor` → `WorkflowExecutor`

#### 3.2 方法名
- `getThread()` → `getExecution()`
- `getThreadType()` → `getExecutionType()`
- `setThreadType()` → `setExecutionType()`

#### 3.3 配置接口属性
- `WorkflowConversationSessionConfig.threadId` → `executionId`
- `NodeHandlerContextFactoryConfig.workflowExecutionRegistry`（需添加）
- `TriggerHandlerContextFactoryConfig.threadRegistry` → `workflowExecutionRegistry`

### 阶段 4: 修复事件构建器

**优先级：中**

需要重命名的事件构建器函数（在 `core/utils/event/builders/index.ts` 中）：

- `buildThreadStartedEvent` → `buildWorkflowExecutionStartedEvent`
- `buildThreadCompletedEvent` → `buildWorkflowExecutionCompletedEvent`
- `buildThreadFailedEvent` → `buildWorkflowExecutionFailedEvent`
- `buildThreadPausedEvent` → `buildWorkflowExecutionPausedEvent`
- `buildThreadResumedEvent` → `buildWorkflowExecutionResumedEvent`
- `buildThreadCancelledEvent` → `buildWorkflowExecutionCancelledEvent`
- `buildThreadStateChangedEvent` → `buildWorkflowExecutionStateChangedEvent`
- `buildThreadForkStartedEvent` → `buildWorkflowExecutionForkStartedEvent`
- `buildThreadForkCompletedEvent` → `buildWorkflowExecutionForkCompletedEvent`
- `buildThreadJoinStartedEvent` → `buildWorkflowExecutionJoinStartedEvent`
- `buildThreadJoinConditionMetEvent` → `buildWorkflowExecutionJoinConditionMetEvent`
- `buildThreadCopyStartedEvent` → `buildWorkflowExecutionCopyStartedEvent`
- `buildThreadCopyCompletedEvent` → `buildWorkflowExecutionCopyCompletedEvent`

### 阶段 5: 修复类型不匹配问题

**优先级：中**

#### 5.1 CheckpointCreator 类型不匹配
- 文件：`core/di/container-config.ts:292`
- 需要调整 `createCheckpoint` 函数签名以匹配 `CheckpointCreator` 类型

#### 5.2 WorkflowExecutionBuilder 类型不匹配
- 文件：`core/di/container-config.ts:640`
- 需要调整 `WorkflowExecutionBuilder` 的 `build` 方法签名

#### 5.3 HookExecutionContext 类型不匹配
- 文件：`workflow/execution/coordinators/node-execution-coordinator.ts:361,387`
- 需要确保传递给 hook 的上下文对象符合 `HookExecutionContext` 接口

### 阶段 6: 修复其他类型错误

**优先级：低**

#### 6.1 缺失的类型导出
- `WorkflowExecutionInterruptedException`（应在 `@wf-agent/types` 中定义）
- `WorkflowExecutionNotFoundError`（应在 `@wf-agent/types` 中定义）

#### 6.2 AgentLoopRegistry 缺失的方法
- `cleanupByParentWorkflowExecutionId(parentId: string): Promise<void>`

#### 6.3 ToolApprovalContext 缺失的属性
- `threadRegistry` → `workflowExecutionRegistry`

## 执行建议

### 推荐的修复顺序

1. **先修复类型定义**（阶段2）：确保所有必要的类型和接口都已定义
2. **再修复导入路径**（阶段1）：确保所有模块都能正确导入
3. **然后批量修复命名**（阶段3）：使用全局搜索替换
4. **接着修复事件构建器**（阶段4）：确保事件系统正常工作
5. **最后处理类型不匹配**（阶段5-6）：处理复杂的类型兼容性问题

### 注意事项

1. **保持向后兼容**：在重命名导出时，考虑添加类型别名以支持平滑迁移
2. **测试验证**：每完成一个阶段后，运行类型检查验证修复效果
3. **文档更新**：同步更新相关文档和注释
4. **代码审查**：修复完成后进行代码审查，确保没有引入新的问题

## 预期结果

完成所有修复后，SDK 应该：
- ✅ 通过 TypeScript 类型检查（`pnpm typecheck` 无错误）
- ✅ 所有旧命名已清理完毕
- ✅ 类型定义完整且一致
- ✅ 模块导入路径正确
- ✅ 符合命名重构设计方案的要求

## 参考资料

- [命名重构设计方案](../architecture/naming-refactor/design-proposal.md)
- [Analyzer 使用指南](../skill/SKILL.md)
