# 触发子工作流重构方案

## 文档信息

- **创建时间**: 2025-01-XX
- **状态**: 待实施
- **相关文件**:
  - `sdk/types/node.ts`
  - `sdk/types/trigger.ts`
  - `sdk/core/execution/handlers/trigger-handlers/execute-triggered-subgraph-handler.ts`
  - `sdk/core/execution/handlers/triggered-subgraph-handler.ts`

## 问题描述

### 原始问题

1. `triggered-subgraph-handler.ts` 文件是否应该导入 `StartFromTriggerNodeConfig` 和 `ContinueFromTriggerNodeConfig`？
2. 保存主工作流状态的逻辑是否多余？

### 分析结论

经过深入分析，发现当前实现存在设计不一致的问题：

1. **节点配置冗余**：`StartFromTriggerNodeConfig` 定义了 `subgraphId`, `inputMapping`, `outputMapping`, `async` 配置，但执行时完全忽略这些配置
2. **概念混淆**：当前实现混淆了"触发子工作流"和"普通子工作流"的概念
3. **状态保存多余**：触发子工作流是异步执行的，不需要保存和恢复主工作流状态

## 推荐的设计方案

### 1. 触发子工作流的结构

```
触发子工作流（例如：提示词压缩工作流）
├── START_FROM_TRIGGER 节点（空配置，仅作为标识）
├── 业务节点1（例如：压缩逻辑）
├── 业务节点2（例如：清理逻辑）
└── CONTINUE_FROM_TRIGGER 节点（空配置，仅作为标识）
```

**特点**：
- 触发子工作流包含实际的业务逻辑
- START_FROM_TRIGGER 节点作为入口标识，不包含业务逻辑
- CONTINUE_FROM_TRIGGER 节点作为出口标识，不包含业务逻辑
- 触发子工作流与主工作流完全独立，可以单独定义和测试

### 2. 节点配置

#### StartFromTriggerNodeConfig

```typescript
/**
 * 从触发器开始的节点配置
 * 专门用于标识由触发器启动的孤立子工作流的起始点
 * 空配置，仅作为标识
 */
export interface StartFromTriggerNodeConfig {
  // 空配置，仅作为标识
}
```

#### ContinueFromTriggerNodeConfig

```typescript
/**
 * 从触发器继续的节点配置
 * 用于在子工作流执行完成后恢复到主工作流的执行位置
 * 空配置，仅作为标识
 */
export interface ContinueFromTriggerNodeConfig {
  // 空配置，仅作为标识
}
```

### 3. 触发器动作配置

#### ExecuteTriggeredSubgraphActionConfig

```typescript
/**
 * 执行触发子工作流动作配置
 * 用于触发器启动孤立的子工作流执行
 */
export interface ExecuteTriggeredSubgraphActionConfig {
  /** 触发子工作流ID（包含 START_FROM_TRIGGER 节点的工作流） */
  triggeredWorkflowId: ID;
  /** 是否等待完成（默认false，异步执行） */
  waitForCompletion?: boolean;
}
```

**说明**：
- `triggeredWorkflowId`: 指定要执行的触发子工作流ID
- `waitForCompletion`: 是否等待子工作流完成（默认false，异步执行）

### 4. 执行流程

```
1. 触发器被触发
   ↓
2. 获取触发子工作流定义（通过 triggeredWorkflowId）
   ↓
3. 从主线程上下文获取所有执行上下文
   - getAllVariables(): 所有变量
   - getOutput(): 输出数据
   - getInput(): 输入数据
   ↓
4. 创建子工作流上下文
   - 将主线程上下文作为输入
   - 标记为触发子工作流执行
   ↓
5. 执行触发子工作流
   - 从 START_FROM_TRIGGER 节点开始
   - 执行业务节点
   - 到达 CONTINUE_FROM_TRIGGER 节点结束
   ↓
6. 执行完成后
   - 如果 waitForCompletion=true: 将新的执行上下文传递给主线程
   - 如果 waitForCompletion=false: 异步执行，不阻塞主线程
```

### 5. 上下文传递

#### 输入（主线程 → 子工作流）

```typescript
const input = {
  variables: mainThreadContext.getAllVariables(),
  output: mainThreadContext.getOutput(),
  input: mainThreadContext.getInput()
};
```

#### 输出（子工作流 → 主线程）

```typescript
// 如果 waitForCompletion=true
const subgraphOutput = subgraphContext.getOutput();
const subgraphVariables = subgraphContext.getAllVariables();

// 更新主线程上下文
mainThreadContext.setOutput(subgraphOutput);
// 合并变量
Object.assign(mainThreadContext.getVariableValues(), subgraphVariables);
```

## 需要修改的文件

### 1. sdk/types/node.ts

**修改位置**: 第 369-432 行

**修改内容**:

```typescript
/**
 * 从触发器开始的节点配置
 * 专门用于标识由触发器启动的孤立子工作流的起始点
 * 空配置，仅作为标识
 */
export interface StartFromTriggerNodeConfig {
  // 空配置，仅作为标识
}

/**
 * 从触发器继续的节点配置
 * 用于在子工作流执行完成后恢复到主工作流的执行位置
 * 空配置，仅作为标识
 */
export interface ContinueFromTriggerNodeConfig {
  // 空配置，仅作为标识
}
```

**影响**:
- 需要更新相关的验证器
- 需要更新测试用例

### 2. sdk/types/trigger.ts

**修改位置**: 第 191-214 行

**修改内容**:

```typescript
/**
 * 执行触发子工作流动作配置
 * 用于触发器启动孤立的子工作流执行
 */
export interface ExecuteTriggeredSubgraphActionConfig {
  /** 触发子工作流ID（包含 START_FROM_TRIGGER 节点的工作流） */
  triggeredWorkflowId: ID;
  /** 是否等待完成（默认false，异步执行） */
  waitForCompletion?: boolean;
}
```

**说明**:
- 移除 `subgraphId` 参数（不再需要）
- 移除 `inputMapping` 参数（不再需要）
- 移除 `config` 参数（简化为 `waitForCompletion`）

### 3. sdk/core/execution/handlers/trigger-handlers/execute-triggered-subgraph-handler.ts

**修改位置**: 第 88-181 行

**修改内容**:

```typescript
/**
 * 执行触发子工作流处理函数
 * @param action 触发动作
 * @param triggerId 触发器ID
 * @param executionContext 执行上下文
 * @returns 执行结果
 */
export async function executeTriggeredSubgraphHandler(
  action: TriggerAction,
  triggerId: string,
  executionContext?: ExecutionContext
): Promise<TriggerExecutionResult> {
  const startTime = Date.now();
  const context = executionContext || ExecutionContext.createDefault();

  try {
    const parameters = action.parameters as ExecuteTriggeredSubgraphActionConfig;
    const { triggeredWorkflowId, waitForCompletion = false } = parameters;

    if (!triggeredWorkflowId) {
      throw new Error('Missing required parameter: triggeredWorkflowId');
    }

    // 获取主工作流线程上下文
    const threadRegistry = context.getThreadRegistry();
    const threadId = context.getCurrentThreadId();

    if (!threadId) {
      throw new NotFoundError('Current thread ID not found in execution context', 'ThreadContext', 'current');
    }

    const mainThreadContext = threadRegistry.get(threadId);

    if (!mainThreadContext) {
      throw new NotFoundError(`Main thread context not found: ${threadId}`, 'ThreadContext', threadId);
    }

    // 获取工作流注册表
    const workflowRegistry = context.getWorkflowRegistry();
    const triggeredWorkflow = workflowRegistry.get(triggeredWorkflowId);

    if (!triggeredWorkflow) {
      throw new NotFoundError(`Triggered workflow not found: ${triggeredWorkflowId}`, 'Workflow', triggeredWorkflowId);
    }

    // 从主线程上下文获取所有执行上下文
    const input = {
      variables: mainThreadContext.getAllVariables(),
      output: mainThreadContext.getOutput(),
      input: mainThreadContext.getInput()
    };

    // 创建事件协调器
    const eventCoordinator = new EventCoordinator(context.getEventManager() || eventManager);

    // 创建 ThreadExecutor 实例（作为 SubgraphContextFactory 和 SubgraphExecutor）
    const threadExecutor = new ThreadExecutor(
      context.getEventManager(),
      workflowRegistry
    );

    // 创建触发子工作流任务
    const task: TriggeredSubgraphTask = {
      subgraphId: triggeredWorkflowId,
      input,
      triggerId,
      mainThreadContext,
      config: {
        waitForCompletion,
        timeout: 30000,
        recordHistory: true,
      }
    };

    // 执行触发子工作流
    await executeSingleTriggeredSubgraph(
      task,
      threadExecutor, // 作为 SubgraphContextFactory
      threadExecutor, // 作为 SubgraphExecutor
      eventCoordinator
    );

    const executionTime = Date.now() - startTime;

    return createSuccessResult(
      triggerId,
      action,
      {
        message: `Triggered subgraph execution initiated: ${triggeredWorkflowId}`,
        triggeredWorkflowId,
        input,
        waitForCompletion,
        executed: true,
        completed: waitForCompletion,
      },
      executionTime
    );
  } catch (error) {
    const executionTime = Date.now() - startTime;
    return createFailureResult(triggerId, action, error, executionTime);
  }
}
```

**说明**:
- 修改参数解析逻辑
- 移除 `inputMapping` 处理
- 直接传递主线程的所有上下文
- 简化配置选项

### 4. sdk/core/execution/handlers/triggered-subgraph-handler.ts

**修改位置**: 已完成

**修改内容**:
- ✅ 已移除 `SavedMainThreadState` 接口
- ✅ 已移除 `saveMainThreadState` 函数
- ✅ 已移除 `restoreMainThreadState` 函数
- ✅ 已简化 `executeSingleTriggeredSubgraph` 函数
- ✅ 已更新文件注释

## 需要更新的验证器

### 1. sdk/core/validation/node-validation/start-from-trigger-validator.ts

**修改内容**:

```typescript
import { z } from 'zod';
import { NodeType } from '../../../../types/node';
import { ValidationError } from '../../../../types/errors';

/**
 * START_FROM_TRIGGER 节点配置 schema
 * 空配置，仅作为标识
 */
const startFromTriggerNodeConfigSchema = z.object({});

/**
 * 验证 START_FROM_TRIGGER 节点
 * @param node 节点定义
 * @throws ValidationError 如果节点配置无效
 */
export function validateStartFromTriggerNode(node: Node): void {
  if (node.type !== NodeType.START_FROM_TRIGGER) {
    throw new ValidationError(`Invalid node type for start-from-trigger validator: ${node.type}`, `node.${node.id}`);
  }

  const result = startFromTriggerNodeConfigSchema.safeParse(node.config || {});
  if (!result.success) {
    throw new ValidationError('START_FROM_TRIGGER node must have no configuration', `node.${node.id}.config`);
  }
}
```

### 2. sdk/core/validation/node-validation/continue-from-trigger-validator.ts

**保持不变**，已经是空配置验证。

## 需要更新的测试用例

### 1. sdk/core/validation/__tests__/node-validator-subgraph.test.ts

**修改内容**:
- 更新 START_FROM_TRIGGER 节点验证测试
- 移除对 `subgraphId`, `inputMapping`, `outputMapping`, `async` 的测试
- 添加空配置验证测试

### 2. sdk/core/validation/__tests__/workflow-validator.test.ts

**修改内容**:
- 更新触发子工作流验证测试
- 确保测试用例使用空配置

## 设计优点

1. **简化设计**: 节点配置简化，不需要复杂的 inputMapping 和 outputMapping
2. **独立性**: 触发子工作流与主工作流完全独立，可以单独定义和测试
3. **灵活性**: 触发子工作流可以访问主工作流的所有变量和状态
4. **一致性**: 与普通工作流的执行方式一致，只是触发方式不同
5. **可维护性**: 减少了配置冗余，更容易理解和维护
6. **类型安全**: 简化的配置减少了类型错误的可能性

## 执行计划

### 阶段1: 类型定义修改
1. 修改 `sdk/types/node.ts` 中的 `StartFromTriggerNodeConfig`
2. 修改 `sdk/types/trigger.ts` 中的 `ExecuteTriggeredSubgraphActionConfig`
3. 运行类型检查: `cd sdk && tsc --noEmit`

### 阶段2: 验证器更新
1. 修改 `sdk/core/validation/node-validation/start-from-trigger-validator.ts`
2. 更新相关测试用例
3. 运行验证测试: `cd sdk && npm test -- node-validator-subgraph.test.ts`

### 阶段3: 执行逻辑重构
1. 修改 `sdk/core/execution/handlers/trigger-handlers/execute-triggered-subgraph-handler.ts`
2. 更新相关测试用例
3. 运行执行测试: `cd sdk && npm test -- execute-triggered-subgraph-handler.test.ts`

### 阶段4: 集成测试
1. 运行完整测试套件: `cd sdk && npm test`
2. 验证端到端功能
3. 更新文档

## 注意事项

1. **向后兼容性**: 这是一个破坏性变更，需要更新所有使用触发子工作流的代码
2. **测试覆盖**: 确保所有测试用例都更新到新的设计
3. **文档更新**: 需要更新用户文档和API文档
4. **迁移指南**: 提供从旧设计迁移到新设计的指南

## 参考资料

- [SDK Architecture](../plans/sdk/sdk-architecture.md)
- [SDK Implementation Plan](../plans/sdk/sdk-implementation-plan.md)
- [Node Types](../types/node.ts)
- [Trigger Types](../types/trigger.ts)
- [Workflow Validator](../core/validation/workflow-validator.ts)