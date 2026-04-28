# 错误处理架构分析

## 概述

本文档分析 SDK 中错误处理的架构设计，评估 ErrorService 全局单例的必要性，并提出优化建议。

## 一、当前架构分析

### 1.1 ErrorService 职责

`ErrorService` 当前职责非常简单：

```
ErrorService.handleError(error, context)
  ├── 1. logError() - 根据 severity 记录日志
  └── 2. emitErrorEvent() - 使用 buildErrorEvent() + eventManager.emit()
```

**核心发现**：ErrorService 本质上只是两个操作的简单封装：
1. `logger.error/warn/info()` - 日志记录
2. `safeEmit(eventManager, buildErrorEvent(...))` - 事件触发

### 1.2 当前使用情况

| 模块 | 错误处理方式 | 问题 |
|------|-------------|------|
| **Agent 模块** | `agent-error-handler.ts` → ErrorService | 统一、规范 |
| **Graph 模块** | 直接 `throw new XError()` + 注释 | 未调用 ErrorService |
| **Core 模块** | 直接 `logger.error()` | 绕过 ErrorService |

### 1.3 存在的问题

1. **无效注释泛滥**：大量 `// 抛出 X 错误，由 ErrorService 统一处理` 注释，但实际只是 `throw`，并未调用 ErrorService

2. **Graph 模块未使用 ErrorService**：
   - `variable-coordinator.ts:378` - 抛出 EventSystemError
   - `route-handler.ts:38` - 抛出 BusinessLogicError
   - `checkpoint-state-manager.ts:196` - 抛出 StateManagementError
   - 这些都只是 `throw`，未通过 ErrorService 记录日志和触发事件

3. **直接使用 logger.error**：
   - `triggered-subworkflow-manager.ts:470`
   - `thread-builder.ts:86`
   - `callback-manager.ts:99,107`
   - `task-queue-manager.ts:162`
   - 这些绕过了 ErrorService，无法触发统一的 ERROR 事件

## 二、ErrorService vs Handler 职责分析

### 2.1 两者职责不同

| 组件 | 职责 | 层级 |
|------|------|------|
| **ErrorService** | 基础服务：日志记录 + 事件触发 | 服务层（DI 管理） |
| **agent-error-handler** | 业务处理：上下文构建 + 错误标准化 + 状态管理 | 业务层（函数式） |

**调用链**：`handler` → `ErrorService.handleError()`

### 2.2 ErrorService 存在的价值有限

1. **日志记录**：调用处已有 logger，无需再封装
2. **事件触发**：已有 `safeEmit` + `buildErrorEvent`，无需再封装
3. **唯一价值**：确保所有错误都触发 ERROR 事件 —— 但这可以通过规范而非封装实现

## 三、两种方案对比

### 方案A: ErrorService 全局单例

```typescript
// 当前方式（繁琐）
const container = getContainer();
const errorService = container.get(Identifiers.ErrorService);
await errorService.handleError(standardizedError, context);
```

### 方案B: 无状态函数 + 直接调用

```typescript
// 替代方式（直接）
logError(error, { threadId, nodeId });
await safeEmit(eventManager, buildErrorEvent({ threadId, workflowId, nodeId, error }));
```

### 对比表

| 维度 | 方案A: ErrorService 全局单例 | 方案B: 无状态函数 + 直接调用 |
|------|------------------------------|------------------------------|
| **调用方式** | `container.get(ErrorService).handleError()` | `logError(error); safeEmit(em, buildErrorEvent(...))` |
| **依赖** | 需要 DI 容器获取 | 直接传入 eventManager |
| **代码量** | 封装后调用简单 | 调用处代码稍多 |
| **灵活性** | 固定行为，难以定制 | 可按需定制日志/事件 |
| **一致性** | 强制统一 | 依赖开发者自觉 |
| **测试性** | 需要 mock DI 容器 | 直接 mock 函数 |

## 四、推荐方案

### 4.1 移除 ErrorService，使用无状态函数

**理由**：

1. **符合现有模式**：SDK 已广泛使用 `safeEmit` + `buildXxxEvent` 模式
2. **减少抽象层**：ErrorService 只是对两个简单操作的封装，增加认知负担
3. **提高灵活性**：调用处可根据场景定制日志格式或事件内容
4. **简化依赖**：无需通过 DI 容器获取 ErrorService

### 4.2 实施建议

创建 `sdk/core/utils/error-utils.ts`：

```typescript
/**
 * Error Utilities - 错误处理工具函数
 * 提供无状态的错误处理能力
 *
 * 设计原则：
 * - 纯函数：所有方法都是纯函数，无副作用
 * - 无状态：不依赖 DI 容器，直接传入依赖
 * - 灵活性：调用处可按需定制
 */

import type { EventManager } from '../../managers/event-manager.js';
import type { SDKError } from '@modular-agent/types';
import { buildErrorEvent } from '../event/builders/index.js';
import { safeEmit } from '../event/event-emitter.js';
import { logger } from '../../utils/logger.js';

/**
 * 记录错误日志（根据 severity 选择级别）
 *
 * @param error SDKError 对象
 * @param context 额外的上下文信息
 */
export function logError(
  error: SDKError,
  context?: Record<string, any>
): void {
  const logData = {
    errorType: error.constructor.name,
    errorMessage: error.message,
    severity: error.severity,
    ...context
  };

  switch (error.severity) {
    case 'error':
      logger.error(error.message, logData);
      break;
    case 'warning':
      logger.warn(error.message, logData);
      break;
    case 'info':
      logger.info(error.message, logData);
      break;
  }
}

/**
 * 触发错误事件
 *
 * @param eventManager 事件管理器
 * @param params 事件参数
 */
export async function emitErrorEvent(
  eventManager: EventManager | undefined,
  params: {
    threadId: string;
    workflowId: string;
    nodeId?: string;
    error: Error;
  }
): Promise<void> {
  await safeEmit(eventManager, buildErrorEvent(params));
}

/**
 * 统一错误处理（日志 + 事件）
 * 便捷函数，同时执行日志记录和事件触发
 *
 * @param eventManager 事件管理器
 * @param error SDKError 对象
 * @param params 事件参数
 */
export async function handleError(
  eventManager: EventManager | undefined,
  error: SDKError,
  params: {
    threadId: string;
    workflowId: string;
    nodeId?: string;
  }
): Promise<void> {
  // 记录日志
  logError(error, params);

  // 触发事件
  await emitErrorEvent(eventManager, { ...params, error });
}
```

### 4.3 调用示例

```typescript
import { handleError, logError, emitErrorEvent } from '@modular-agent/core/utils/error-utils';

// 方式1：使用便捷函数（日志 + 事件）
await handleError(eventManager, error, { threadId, workflowId, nodeId });

// 方式2：分开调用（更灵活）
logError(error, { threadId, nodeId, additionalContext });
await emitErrorEvent(eventManager, { threadId, workflowId, nodeId, error });
```

## 五、迁移计划

### 阶段一：创建无状态工具函数

1. 创建 `sdk/core/utils/error-utils.ts`
2. 实现 `logError`、`emitErrorEvent`、`handleError` 函数
3. 编写单元测试
4. 更新 `sdk/core/utils/index.ts` 导出

### 阶段二：重构 Agent 模块

1. 修改 `agent-error-handler.ts` 使用无状态函数
2. 保留模块特定的上下文构建和状态管理
3. 更新测试用例

### 阶段三：重构 Graph 和 Core 模块

1. 替换直接 `logger.error` 调用为 `logError`
2. 在关键位置添加 `emitErrorEvent` 调用
3. 移除无效的 ErrorService 注释

### 阶段四：移除 ErrorService

1. 从 DI 容器配置中移除 ErrorService 绑定
2. 删除 `sdk/core/services/error-service.ts`
3. 更新相关文档

### 阶段五：清理和验证

1. 运行完整的测试套件
2. 进行类型检查：`pnpm typecheck`
3. 检查是否有遗漏的错误处理点

## 六、总结

| 结论 | 说明 |
|------|------|
| **ErrorService 不必要** | 只是对 logger + safeEmit 的简单封装 |
| **推荐无状态函数** | 符合 SDK 现有模式，减少抽象层 |
| **保留 buildErrorEvent** | 已有的事件 builder 设计合理 |
| **移除无效注释** | 当前大量 `// 由 ErrorService 统一处理` 注释是误导性的 |

## 七、相关文件

- `sdk/core/services/error-service.ts` - ErrorService 实现（待移除）
- `sdk/core/utils/event/builders/error-events.ts` - 错误事件 builder
- `sdk/core/utils/event/event-emitter.ts` - safeEmit 等事件触发工具
- `sdk/agent/execution/handlers/agent-error-handler.ts` - Agent 错误处理
- `sdk/docs/error-handling-refactor-plan.md` - 之前的重构计划（可合并）
