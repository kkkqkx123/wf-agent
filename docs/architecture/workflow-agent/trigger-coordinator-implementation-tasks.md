# TriggerCoordinator 集成实施任务清单

## 📋 概述

本文档提供将 TriggerCoordinator 正确集成到事件系统中的详细实施步骤。

**目标**: 实现自动化的触发器执行机制，当事件发出时自动检查并执行匹配的触发器。

**预计工期**: 5-7 天

---

## Phase 1: 基础集成（2-3天）

### Task 1.1: 扩展 ExecutionEventEmitter

**文件**: `sdk/core/registry/event-emitter.ts`

**修改内容**:

```typescript
// 1. 添加导入
import type { TriggerCoordinator } from "../../workflow/execution/coordinators/trigger-coordinator.js";

// 2. 添加私有字段
export class ExecutionEventEmitter {
  public readonly executionId: string;
  private triggerCoordinator?: TriggerCoordinator;  // ← 新增
  
  // ... 其他字段 ...
}

// 3. 更新构造函数
constructor(executionId: string, triggerCoordinator?: TriggerCoordinator) {  // ← 新增参数
  if (!executionId) {
    throw new RuntimeValidationError("Execution ID is required", { field: "executionId" });
  }
  this.executionId = executionId;
  this.triggerCoordinator = triggerCoordinator;  // ← 新增
}

// 4. 在 emit() 方法中添加触发器调用
async emit<T extends BaseEvent>(event: T): Promise<void> {
  this.validateNotDisposed();
  
  // ... 现有的事件验证和监听器执行逻辑 ...
  
  // 【新增】执行触发器
  if (this.triggerCoordinator) {
    try {
      await this.triggerCoordinator.handleEvent(event);
    } catch (error) {
      const err = getErrorOrNew(error);
      logger.warn('Trigger execution failed', {
        executionId: this.executionId,
        eventType: event.type,
        error: err.message,
      });
      // 触发器失败不应影响主流程，继续执行
    }
  }
}
```

**验收标准**:
- [ ] ExecutionEventEmitter 可以接受可选的 TriggerCoordinator
- [ ] emit() 方法在触发器存在时会调用 handleEvent()
- [ ] 触发器异常不会中断事件发射流程
- [ ] 添加适当的日志记录

---

### Task 1.2: 更新 EventRegistry

**文件**: `sdk/core/registry/event-registry.ts`

**修改内容**:

```typescript
// 1. 添加导入
import type { TriggerCoordinator } from "../../workflow/execution/coordinators/trigger-coordinator.js";
import type { IdBasedServiceFactory } from "../di/service-factory-types.js";

// 2. 添加字段
class EventRegistry {
  private emitters: Map<string, ExecutionEventEmitter> = new Map();
  private metricsCollector: EventMetricsCollector;
  private triggerCoordinatorFactory?: IdBasedServiceFactory<TriggerCoordinator>;  // ← 新增
  
  constructor() {
    this.metricsCollector = new EventMetricsCollector();
  }
  
  // 3. 新增方法：设置触发器工厂
  setTriggerCoordinatorFactory(factory: IdBasedServiceFactory<TriggerCoordinator>): void {
    this.triggerCoordinatorFactory = factory;
  }
  
  // 4. 更新 getEmitter() 方法
  getEmitter(executionId: string): ExecutionEventEmitter {
    if (!executionId) {
      throw new RuntimeValidationError("Execution ID is required", { field: "executionId" });
    }

    if (!this.emitters.has(executionId)) {
      logger.debug('Creating new ExecutionEventEmitter', { executionId });
      
      // 【新增】为每个 execution 创建对应的 TriggerCoordinator
      let triggerCoordinator: TriggerCoordinator | undefined;
      if (this.triggerCoordinatorFactory) {
        triggerCoordinator = this.triggerCoordinatorFactory.create(executionId);
      }
      
      this.emitters.set(
        executionId, 
        new ExecutionEventEmitter(executionId, triggerCoordinator)  // ← 传入触发器
      );
    }

    return this.emitters.get(executionId)!;
  }
}
```

**验收标准**:
- [ ] EventRegistry 可以接收 TriggerCoordinator 工厂
- [ ] 每个新的 ExecutionEventEmitter 都关联一个 TriggerCoordinator 实例
- [ ] 如果没有设置工厂，emitter 仍然可以正常工作（向后兼容）

---

### Task 1.3: 更新 DI 容器配置

**文件**: `sdk/core/di/container-config.ts`

**修改内容**:

找到 EventRegistry 的绑定位置（大约在第 200 行附近），更新为：

```typescript
// EventRegistry - Event Manager with TriggerCoordinator integration
container
  .bind(Identifiers.EventRegistry)
  .toDynamicValue((c: IContainer): EventRegistry => {
    const eventRegistry = new EventRegistry();
    
    // 获取 TriggerCoordinator 工厂并注入
    const triggerCoordinatorFactory = c.get(Identifiers.TriggerCoordinator);
    eventRegistry.setTriggerCoordinatorFactory(
      triggerCoordinatorFactory as IdBasedServiceFactory<TriggerCoordinator>
    );
    
    return eventRegistry;
  })
  .inSingletonScope();
```

**验收标准**:
- [ ] EventRegistry 在创建时自动获得 TriggerCoordinator 工厂
- [ ] DI 容器正确解析所有依赖
- [ ] 没有循环依赖问题

---

### Task 1.4: 编写单元测试

**文件**: `sdk/__tests__/core/registry/event-emitter-trigger.int.test.ts`

**测试用例**:

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import { ExecutionEventEmitter } from '../../../core/registry/event-emitter.js';
import { TriggerCoordinator } from '../../../workflow/execution/coordinators/trigger-coordinator.js';
import { buildNodeCompletedEvent } from '../../../core/utils/event/builders/node-events.js';

describe('ExecutionEventEmitter with TriggerCoordinator', () => {
  let emitter: ExecutionEventEmitter;
  let mockTriggerCoordinator: any;
  
  beforeEach(() => {
    mockTriggerCoordinator = {
      handleEvent: vi.fn().mockResolvedValue(undefined),
    };
    
    emitter = new ExecutionEventEmitter('test-exec-1', mockTriggerCoordinator);
  });
  
  it('should call trigger coordinator when emitting event', async () => {
    const event = buildNodeCompletedEvent({
      executionId: 'test-exec-1',
      workflowId: 'test-workflow',
      nodeId: 'node-1',
      nodeType: 'llm',
      output: { result: 'success' },
    });
    
    await emitter.emit(event);
    
    expect(mockTriggerCoordinator.handleEvent).toHaveBeenCalledWith(event);
  });
  
  it('should continue even if trigger fails', async () => {
    mockTriggerCoordinator.handleEvent.mockRejectedValue(new Error('Trigger failed'));
    
    const event = buildNodeCompletedEvent({
      executionId: 'test-exec-1',
      workflowId: 'test-workflow',
      nodeId: 'node-1',
      nodeType: 'llm',
      output: { result: 'success' },
    });
    
    // Should not throw
    await expect(emitter.emit(event)).resolves.not.toThrow();
  });
  
  it('should work without trigger coordinator', async () => {
    const emitterWithoutTrigger = new ExecutionEventEmitter('test-exec-2');
    
    const event = buildNodeCompletedEvent({
      executionId: 'test-exec-2',
      workflowId: 'test-workflow',
      nodeId: 'node-1',
      nodeType: 'llm',
      output: { result: 'success' },
    });
    
    // Should not throw
    await expect(emitterWithoutTrigger.emit(event)).resolves.not.toThrow();
  });
});
```

**验收标准**:
- [ ] 所有测试用例通过
- [ ] 覆盖正常流程和异常流程
- [ ] 测试隔离性良好

---

## Phase 2: 触发器注册集成（1-2天）

### Task 2.1: 在 WorkflowExecutionBuilder 中注册触发器

**文件**: `sdk/workflow/execution/factories/workflow-execution-builder.ts`

**修改内容**:

在 `build()` 方法中添加触发器注册逻辑：

```typescript
async build(workflowId: string, options: WorkflowExecutionOptions = {}) {
  // ... 现有代码：获取 workflow graph ...
  
  // Step 5: Create WorkflowExecution
  const workflowExecution: WorkflowExecution = {
    id: generateId(),
    workflowId,
    status: 'PENDING',
    input: options.input || {},
    createdAt: now(),
    updatedAt: now(),
  };
  
  // ... 创建 execution entity ...
  
  // 【新增】Step 8: Register triggers from workflow graph
  const triggers = workflowGraph.triggers || [];
  if (triggers.length > 0) {
    const triggerCoordinator = this.getTriggerCoordinator();
    for (const trigger of triggers) {
      triggerCoordinator.register(trigger, workflowId);
    }
    
    logger.info('Registered triggers for workflow execution', {
      executionId: workflowExecution.id,
      triggerCount: triggers.length,
    });
  }
  
  // ... 返回结果 ...
}

// 新增辅助方法
private getTriggerCoordinator(): TriggerCoordinator {
  if (!this.globalContext) {
    throw new Error("GlobalContext not initialized");
  }
  const factory = this.globalContext.container.get(Identifiers.TriggerCoordinator);
  // 这里需要获取 executionId，可能需要调整
  return (factory as IdBasedServiceFactory<TriggerCoordinator>).create(
    workflowExecution.id  // 注意：此时 execution 已创建
  );
}
```

**验收标准**:
- [ ] 工作流中的触发器定义被正确注册
- [ ] 每个 execution 有独立的触发器状态
- [ ] 日志记录清晰

---

### Task 2.2: 确保触发器状态清理

**文件**: `sdk/workflow/execution/coordinators/workflow-state-transitor.ts`

在 workflow execution 结束时清理触发器状态：

```typescript
async completeExecution(executionId: string): Promise<void> {
  // ... 现有清理逻辑 ...
  
  // 【新增】清理触发器状态
  const triggerCoordinator = this.getTriggerCoordinator(executionId);
  if (triggerCoordinator) {
    triggerCoordinator.clear();
  }
}
```

**验收标准**:
- [ ] execution 完成后触发器状态被清理
- [ ] 没有内存泄漏

---

## Phase 3: 验证与测试（1-2天）

### Task 3.1: 运行现有测试套件

```bash
# 运行所有相关测试
pnpm test sdk/__tests__/core/registry/
pnpm test sdk/__tests__/workflow/execution/coordinators/
```

**验收标准**:
- [ ] 所有现有测试通过
- [ ] 没有引入回归问题

---

### Task 3.2: 编写集成测试

**文件**: `sdk/__tests__/workflow/execution/trigger-integration.int.test.ts`

创建端到端测试，验证：
1. 触发器在事件发出时被执行
2. 触发动作正确执行（如停止、暂停工作流）
3. 多个触发器可以同时工作

**验收标准**:
- [ ] 集成测试通过
- [ ] 覆盖主要使用场景

---

### Task 3.3: 手动测试

创建一个简单的测试工作流，包含触发器：

```toml
# test-workflow-with-trigger.toml
[workflow]
id = "test-trigger-workflow"
name = "Test Trigger Workflow"

[[nodes]]
id = "start"
type = "start"

[[nodes]]
id = "llm_node"
type = "llm"
prompt = "Say hello"

[[triggers]]
id = "stop_after_llm"
name = "Stop after LLM node"
condition.eventType = "NODE_COMPLETED"
condition.nodeName = "llm_node"
action.type = "stop_workflow_execution"
```

执行该工作流，验证触发器是否生效。

**验收标准**:
- [ ] 触发器按预期工作
- [ ] 日志输出清晰可追踪

---

## Phase 4: 文档与优化（1天）

### Task 4.1: 更新架构文档

**文件**: `docs/architecture/workflow-agent/trigger-system.md`

添加以下内容：
- 触发器系统的工作原理
- 事件流向图
- 最佳实践和注意事项

---

### Task 4.2: 添加使用示例

在 SDK 文档中添加触发器使用示例：

```typescript
// 示例：创建带触发器的工作流
const workflow = {
  id: 'my-workflow',
  nodes: [/* ... */],
  triggers: [
    {
      id: 'auto-stop',
      condition: {
        eventType: 'NODE_COMPLETED',
        nodeName: 'final-node',
      },
      action: {
        type: 'stop_workflow_execution',
      },
    },
  ],
};
```

---

### Task 4.3: 性能优化（可选）

如果发现有性能问题，可以考虑：

1. **快速路径优化**: 在没有触发器时跳过检查
2. **缓存匹配结果**: 避免重复计算哪些触发器匹配事件
3. **批量处理**: 如果有多个触发器匹配，考虑批量执行

---

## 📊 进度跟踪

| Phase | 任务 | 状态 | 负责人 | 预计完成日期 |
|-------|------|------|--------|--------------|
| Phase 1 | Task 1.1: 扩展 ExecutionEventEmitter | ⏳ Pending | - | - |
| Phase 1 | Task 1.2: 更新 EventRegistry | ⏳ Pending | - | - |
| Phase 1 | Task 1.3: 更新 DI 容器配置 | ⏳ Pending | - | - |
| Phase 1 | Task 1.4: 编写单元测试 | ⏳ Pending | - | - |
| Phase 2 | Task 2.1: 注册触发器 | ⏳ Pending | - | - |
| Phase 2 | Task 2.2: 清理触发器状态 | ⏳ Pending | - | - |
| Phase 3 | Task 3.1: 运行现有测试 | ⏳ Pending | - | - |
| Phase 3 | Task 3.2: 编写集成测试 | ⏳ Pending | - | - |
| Phase 3 | Task 3.3: 手动测试 | ⏳ Pending | - | - |
| Phase 4 | Task 4.1: 更新文档 | ⏳ Pending | - | - |
| Phase 4 | Task 4.2: 添加示例 | ⏳ Pending | - | - |
| Phase 4 | Task 4.3: 性能优化 | ⏳ Pending | - | - |

---

## 🎯 成功指标

- ✅ 所有单元测试通过
- ✅ 所有集成测试通过
- ✅ 手动测试验证触发器正常工作
- ✅ 没有性能退化
- ✅ 文档完整清晰

---

## ⚠️ 风险与缓解

### 风险 1: 循环依赖
**描述**: EventRegistry 和 TriggerCoordinator 可能形成循环依赖

**缓解**: 
- 使用工厂模式延迟创建
- 确保 DI 容器正确配置依赖顺序

### 风险 2: 触发器失败影响主流程
**描述**: 触发器执行失败可能导致事件发射失败

**缓解**:
- 在 ExecutionEventEmitter 中使用 try-catch
- 记录错误但不抛出
- 提供配置选项控制行为

### 风险 3: 性能问题
**描述**: 每个事件都检查触发器可能影响性能

**缓解**:
- 只在有触发器时才检查
- 添加快速路径优化
- 监控性能指标

---

## 📝 备注

- 本计划基于方案 A（ExecutionEventEmitter 集成）
- 如需调整方案，请更新本文档
- 每个任务完成后应更新进度跟踪表

---

**创建日期**: 2026-05-16
**最后更新**: 2026-05-16
**状态**: 待开始
