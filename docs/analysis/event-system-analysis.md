# 事件系统实现分析

## 概述

项目采用分层事件系统架构，核心由三部分组成：

1. **事件类型定义**（packages/types/src/events）
2. **事件构建器**（sdk/core/utils/event/builders）
3. **事件注册表**（sdk/core/registry/event-registry）

该系统支持执行范围和全局监听器，提供自动清理机制，避免内存泄漏。

## 核心文件与功能

### 1. 事件类型定义（packages/types/src/events）

- **base.ts**: 定义基础事件接口（BaseEvent）和事件类型（EventType）枚举
- **agent-events.ts**: 定义代理生命周期事件（AGENT_STARTED、AGENT_COMPLETED、AGENT_TURN_STARTED等）
- **workflow-execution-events.ts**: 定义工作流执行事件（WORKFLOW_EXECUTION_STARTED、NODE_COMPLETED等）
- **tool-events.ts**: 定义工具调用事件（TOOL_CALL_STARTED、TOOL_CALL_COMPLETED）
- **interaction-events.ts**: 定义交互事件（HUMAN_RELAY_REQUESTED、TOOL_APPROVAL_REQUESTED）
- **checkpoint-events.ts**: 定义检查点事件（CHECKPOINT_CREATED、CHECKPOINT_RESTORED）
- **index.ts**: 统一导出所有事件类型，形成Event联合类型

### 2. 事件构建器（sdk/core/utils/event/builders）

- **common.ts**: 提供createBuilder工具函数，用于生成事件构建器
- **agent-events.ts**: 提供buildAgentStartedEvent、buildAgentCompletedEvent等构建函数
- **workflow-execution-events.ts**: 提供buildWorkflowExecutionStartedEvent、buildNodeCompletedEvent等构建函数
- **tool-events.ts**: 提供buildToolCallStartedEvent、buildToolCallCompletedEvent等构建函数
- **index.ts**: 统一导出所有构建器函数

构建器函数采用工厂模式，确保事件格式一致性，自动填充id和timestamp。

### 3. 事件注册表（sdk/core/registry/event-registry.ts）

- **EventRegistry类**: 核心事件管理器
- **执行范围监听器**: 通过getEmitter(executionId)创建隔离的执行环境
- **全局监听器**: onGlobal()方法注册跨执行的监听器
- **自动清理**: cleanupExecutionListeners()在执行结束时自动清理监听器
- **指标收集**: EventMetricsCollector收集跨执行的事件统计信息
- **事件发射**: emit()方法支持重试、延迟、条件发射等高级功能

## 事件发射机制

- **emit()**: 基础事件发射，失败抛出异常
- **emitBatch()**: 批量发射，失败收集错误
- **emitBatchParallel()**: 并行批量发射，提高性能
- **emitIf()**: 条件发射，仅在满足条件时触发
- **emitDelayed()**: 延迟发射
- **emitWithRetry()**: 带重试机制的发射（默认3次）
- **emitAndWaitForCallback()**: 发射后等待回调事件

## 架构特点

1. **执行隔离**: 每个执行有独立的EventEmitter实例，避免事件交叉污染
2. **自动清理**: 执行结束时自动清理相关监听器，防止内存泄漏
3. **高性能**: 支持并行批量发射，适用于高吞吐场景
4. **可监控**: 提供完整的指标收集和诊断API
5. **灵活性**: 支持全局和执行范围两种监听模式
6. **健壮性**: 内置重试、超时、条件发射等容错机制

## 使用示例

```typescript
// 注册执行范围监听器
const emitter = eventRegistry.getEmitter(executionId);
emitter.on('AGENT_STARTED', (event) => {
  console.log('Agent started:', event.agentLoopId);
});

// 发射事件
await emit(eventRegistry.getEmitter(executionId), buildAgentStartedEvent({
  agentLoopId: 'agent-123',
  maxIterations: 5,
  initialMessageCount: 1
}));

// 注册全局监听器
const unsubscribe = eventRegistry.onGlobal((event) => {
  console.log('Global event:', event.type);
});

// 带重试的事件发射
await emitWithRetry(eventRegistry.getEmitter(executionId), event, 3, 1000);
```

## 总结

该事件系统设计成熟，具有生产级可靠性。通过清晰的分层结构、自动清理机制和丰富的发射选项，为复杂的工作流执行提供了强大的监控和协调能力。