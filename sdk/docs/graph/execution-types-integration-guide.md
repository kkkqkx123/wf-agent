# Execution Types 优化集成指南

## 一、变更概述

本次优化对 `sdk/graph/execution/types` 目录进行了以下改进：

1. **消除重复定义**：删除SDK中的 `TaskStatus` 定义，统一使用 `packages/types` 中的版本
2. **枚举常量化**：将 `WorkerStatus` 和 `DynamicThreadEventType` 改为 `const enum`
3. **类型安全增强**：为 `QueueStats` 添加返回类型声明
4. **冗余类型清理**：删除 `CallbackInfo` 和 `VisibilityUpdateRequest`

---

## 二、变更详情

### 2.1 TaskStatus 统一化

**变更前：**
```typescript
// sdk/graph/execution/types/task.types.ts
export type TaskStatus =
  | "QUEUED"
  | "RUNNING"
  | "COMPLETED"
  | "FAILED"
  | "CANCELLED"
  | "TIMEOUT";
```

**变更后：**
```typescript
// sdk/graph/execution/types/task.types.ts
import type { TaskStatus } from "@modular-agent/types";

// Re-export TaskStatus for convenience
export { TaskStatus } from "@modular-agent/types";
```

**影响范围：**
- 所有使用 `TaskStatus` 的文件现在从 `@modular-agent/types` 导入
- 保持向后兼容，仍可从 `task.types.ts` 导入

---

### 2.2 WorkerStatus 枚举化

**变更前：**
```typescript
export type WorkerStatus =
  | "IDLE"
  | "BUSY"
  | "SHUTTING_DOWN";
```

**变更后：**
```typescript
export const enum WorkerStatus {
  IDLE = "IDLE",
  BUSY = "BUSY",
  SHUTTING_DOWN = "SHUTTING_DOWN",
}
```

**使用方式变更：**
```typescript
// 变更前
wrapper.status = "IDLE";

// 变更后
wrapper.status = WorkerStatus.IDLE;
```

**影响文件：**
- `sdk/graph/services/thread-pool-service.ts`

---

### 2.3 DynamicThreadEventType 枚举化

**变更前：**
```typescript
export type DynamicThreadEventType =
  | "DYNAMIC_THREAD_REQUESTED"
  | "DYNAMIC_THREAD_COMPLETED"
  | "DYNAMIC_THREAD_FAILED"
  | "DYNAMIC_THREAD_CANCELLED";
```

**变更后：**
```typescript
export const enum DynamicThreadEventType {
  REQUESTED = "DYNAMIC_THREAD_REQUESTED",
  COMPLETED = "DYNAMIC_THREAD_COMPLETED",
  FAILED = "DYNAMIC_THREAD_FAILED",
  CANCELLED = "DYNAMIC_THREAD_CANCELLED",
}
```

**使用方式变更：**
```typescript
// 变更前
type: "DYNAMIC_THREAD_REQUESTED"

// 变更后
type: DynamicThreadEventType.REQUESTED
```

**影响文件：**
- `sdk/graph/execution/utils/event/dynamic-thread-events.ts`

---

### 2.4 QueueStats 类型声明

**变更前：**
```typescript
getQueueStats() {
  return {
    pendingCount: this.pendingQueue.length,
    runningCount: this.runningTasks.size,
    completedCount: this.taskRegistry.getStats().completed,
    failedCount: this.taskRegistry.getStats().failed,
    cancelledCount: this.taskRegistry.getStats().cancelled,
  };
}
```

**变更后：**
```typescript
getQueueStats(): QueueStats {
  return {
    pendingCount: this.pendingQueue.length,
    runningCount: this.runningTasks.size,
    completedCount: this.taskRegistry.getStats().completed,
    failedCount: this.taskRegistry.getStats().failed,
    cancelledCount: this.taskRegistry.getStats().cancelled,
  };
}
```

---

### 2.5 删除冗余类型

**删除的类型：**
1. `CallbackInfo` - 被 `GenericCallbackInfo<T>` 替代
2. `VisibilityUpdateRequest` - 未使用，过度设计

---

## 三、迁移指南

### 3.1 更新导入语句

**TaskStatus 导入：**
```typescript
// 推荐方式（从 packages/types 导入）
import type { TaskStatus } from "@modular-agent/types";

// 兼容方式（从 task.types.ts 导入）
import { TaskStatus } from "../types/task.types.js";
```

**WorkerStatus 导入：**
```typescript
import { WorkerStatus } from "../types/task.types.js";

// 使用枚举常量
const status = WorkerStatus.IDLE;
```

**DynamicThreadEventType 导入：**
```typescript
import { DynamicThreadEventType } from "../types/dynamic-thread.types.js";

// 使用枚举常量
const eventType = DynamicThreadEventType.REQUESTED;
```

---

### 3.2 代码迁移示例

**示例1：使用 WorkerStatus 枚举**
```typescript
// ❌ 错误：使用字符串字面量
wrapper.status = "IDLE";

// ✅ 正确：使用枚举常量
wrapper.status = WorkerStatus.IDLE;
```

**示例2：使用 DynamicThreadEventType 枚举**
```typescript
// ❌ 错误：使用字符串字面量
const event = {
  type: "DYNAMIC_THREAD_REQUESTED",
  threadId: "123",
  timestamp: Date.now(),
};

// ✅ 正确：使用枚举常量
const event = {
  type: DynamicThreadEventType.REQUESTED,
  threadId: "123",
  timestamp: Date.now(),
};
```

**示例3：类型安全的返回值**
```typescript
// ❌ 缺少类型声明
getQueueStats() {
  return { /* ... */ };
}

// ✅ 添加类型声明
getQueueStats(): QueueStats {
  return { /* ... */ };
}
```

---

## 四、验证步骤

### 4.1 类型检查

```bash
# 运行类型检查
pnpm typecheck
```

**预期结果：** 无类型错误

---

### 4.2 构建测试

```bash
# 运行构建
pnpm build
```

**预期结果：** 构建成功

---

### 4.3 单元测试

```bash
# 运行相关测试
cd sdk
pnpm test graph/execution/types
pnpm test graph/services/thread-pool-service
pnpm test graph/execution/managers/task-queue-manager
```

**预期结果：** 所有测试通过

---

## 五、向后兼容性

### 5.1 兼容性保证

✅ **完全兼容：**
- `TaskStatus` 仍可从 `task.types.ts` 导入
- 枚举值与原字符串值完全相同
- 运行时行为不变

⚠️ **需要注意：**
- `const enum` 在编译时内联，JavaScript运行时无法访问枚举对象
- 如果需要在运行时遍历枚举值，应使用普通 `enum`

---

### 5.2 破坏性变更

❌ **已删除的类型：**
- `CallbackInfo` - 使用 `GenericCallbackInfo<T>` 替代
- `VisibilityUpdateRequest` - 未使用，直接删除

**迁移方式：**
```typescript
// CallbackInfo 替代方案
import type { GenericCallbackInfo } from "../managers/callback-manager.js";
import type { ExecutedThreadResult } from "../types/dynamic-thread.types.js";

type CallbackInfo = GenericCallbackInfo<ExecutedThreadResult>;
```

---

## 六、最佳实践

### 6.1 使用枚举常量

**推荐：**
```typescript
// 使用枚举常量，类型安全
if (status === WorkerStatus.IDLE) {
  // ...
}
```

**不推荐：**
```typescript
// 使用字符串字面量，容易拼写错误
if (status === "IDLE") {
  // ...
}
```

---

### 6.2 导入规范

**推荐：**
```typescript
// 从源头导入，路径明确
import { WorkerStatus } from "../types/task.types.js";
import { DynamicThreadEventType } from "../types/dynamic-thread.types.js";
```

**不推荐：**
```typescript
// 从 index.ts 导入，增加间接层
import { WorkerStatus } from "../types/index.js";
```

---

### 6.3 类型声明完整性

**推荐：**
```typescript
// 所有公共方法都应有明确的返回类型
getQueueStats(): QueueStats {
  // ...
}

getPoolStats(): PoolStats {
  // ...
}
```

---

## 七、故障排查

### 7.1 类型错误

**问题：** `Cannot find name 'WorkerStatus'`

**解决方案：**
```typescript
// 确保正确导入
import { WorkerStatus } from "../types/task.types.js";
```

---

### 7.2 枚举值错误

**问题：** `Type '"IDLE"' is not assignable to type 'WorkerStatus'`

**解决方案：**
```typescript
// 使用枚举常量而非字符串
status: WorkerStatus.IDLE  // ✅
status: "IDLE"             // ❌
```

---

### 7.3 运行时错误

**问题：** `Cannot read property 'IDLE' of undefined`

**原因：** `const enum` 在编译时内联，运行时不存在枚举对象

**解决方案：**
```typescript
// 如果需要运行时访问枚举对象，改用普通 enum
export enum WorkerStatus {  // 注意：去掉了 const
  IDLE = "IDLE",
  BUSY = "BUSY",
  SHUTTING_DOWN = "SHUTTING_DOWN",
}
```

---

## 八、性能影响

### 8.1 const enum 优势

✅ **编译时内联：**
```typescript
// 源代码
if (status === WorkerStatus.IDLE) { }

// 编译后
if (status === "IDLE") { }
```

✅ **零运行时开销：**
- 不生成额外的枚举对象
- 与字符串字面量性能相同

✅ **类型安全：**
- 编译时检查枚举值正确性
- IDE自动补全支持

---

### 8.2 包大小影响

**优化前：**
- 生成枚举对象代码
- 增加bundle大小

**优化后：**
- 完全内联，无额外代码
- 减少bundle大小

---

## 九、后续优化建议

### 9.1 类型文档化

为所有类型添加完整的JSDoc注释：
```typescript
/**
 * Worker Status
 * 
 * Represents the operational state of a thread executor in the pool.
 * 
 * @example
 * ```typescript
 * const status = WorkerStatus.IDLE;
 * if (status === WorkerStatus.BUSY) {
 *   console.log('Executor is busy');
 * }
 * ```
 */
export const enum WorkerStatus {
  /** Executor is idle and available for new tasks */
  IDLE = "IDLE",
  /** Executor is currently executing a task */
  BUSY = "BUSY",
  /** Executor is shutting down */
  SHUTTING_DOWN = "SHUTTING_DOWN",
}
```

---

### 9.2 类型测试

添加类型测试文件确保类型正确：
```typescript
// __tests__/types.test.ts
import { WorkerStatus } from "../types/task.types.js";
import { DynamicThreadEventType } from "../types/dynamic-thread.types.js";

// 类型检查测试
const _workerStatus: WorkerStatus = WorkerStatus.IDLE;
const _eventType: DynamicThreadEventType = DynamicThreadEventType.REQUESTED;
```

---

## 十、变更历史

- **2026-04-06**：完成类型优化
  - 删除TaskStatus重复定义
  - WorkerStatus改为const enum
  - DynamicThreadEventType改为const enum
  - 为QueueStats添加返回类型声明
  - 删除CallbackInfo和VisibilityUpdateRequest冗余类型
  - 更新所有导入语句

---

## 十一、参考文档

- [Execution Types 优化设计方案](./execution-types-optimization-design.md)
- [Execution Types 迁移分析报告](./execution-types-migration-analysis.md)
- [TypeScript const enum 文档](https://www.typescriptlang.org/docs/handbook/enums.html#const-enums)
