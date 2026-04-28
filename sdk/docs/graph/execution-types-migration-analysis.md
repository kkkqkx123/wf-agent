# Execution Types 迁移分析报告

## 一、分析背景

`sdk/graph/execution/types` 目录定义了工作流执行引擎的核心类型。本报告分析这些类型是否应该迁移到 `packages/types` 包，以实现更好的类型共享和模块解耦。

---

## 二、packages/types 包定位分析

### 2.1 包结构概览

`packages/types` 是一个**共享类型定义包**，包含：

| 分类 | 子目录 | 内容 |
|------|--------|------|
| 基础类型 | `/` | ID, Timestamp, Metadata 等通用类型 |
| 图结构 | `graph/` | PreprocessedGraph, Node, Edge 等工作流定义 |
| 线程类型 | `thread/` | Thread, ThreadStatus, ThreadContext 等 |
| 事件类型 | `events/` | 各类事件定义 |
| 工具类型 | `tool/` | Tool, ToolConfig, ToolExecution 等 |
| LLM类型 | `llm/` | LLMRequest, LLMResponse 等 |
| 存储类型 | `storage/` | Storage接口和持久化类型 |
| 错误类型 | `errors/` | 各类错误定义 |

### 2.2 包设计原则

**✅ 应该放在 packages/types 的类型：**
1. **跨模块共享**：被多个包（sdk、apps、packages）使用
2. **纯数据结构**：不依赖具体实现类
3. **稳定接口**：作为公共API的一部分
4. **持久化相关**：需要序列化/反序列化的类型

**❌ 不应该放在 packages/types 的类型：**
1. **SDK内部实现**：仅在sdk内部使用
2. **依赖实现类**：依赖具体的类（如ThreadEntity）
3. **运行时状态**：包含运行时逻辑的类型
4. **高度动态**：频繁变化的内部类型

---

## 三、execution/types 类型详细分析

### 3.1 task.types.ts

#### TaskStatus
```typescript
export type TaskStatus =
  | "QUEUED"
  | "RUNNING"
  | "COMPLETED"
  | "FAILED"
  | "CANCELLED"
  | "TIMEOUT";
```

**使用情况：**
- ✅ `packages/types/src/storage/task-storage.ts` 已定义相同类型
- ✅ `packages/storage` 使用该类型
- ✅ `sdk/graph/services` 使用该类型
- ✅ `sdk/graph/execution/managers` 使用该类型

**迁移建议：⚠️ 已存在重复定义**
- `packages/types` 已有 `TaskStatus` 定义
- `sdk/graph/execution/types/task.types.ts` 中的定义是重复的
- **建议：删除sdk中的定义，统一使用packages/types中的定义**

---

#### TaskInfo
```typescript
export interface TaskInfo {
  id: string;
  threadEntity: ThreadEntity;  // ⚠️ 依赖实现类
  status: TaskStatus;
  submitTime: number;
  startTime?: number;
  completeTime?: number;
  result?: ThreadResult;
  error?: Error;
  timeout?: number;
}
```

**依赖分析：**
- ❌ 依赖 `ThreadEntity`（SDK实现类）
- ❌ 依赖 `ThreadResult`（packages/types）

**迁移建议：❌ 不应迁移**
- 包含对实现类的依赖
- 仅在SDK内部使用
- 是运行时状态管理类型

**替代方案：**
```typescript
// packages/types/src/storage/task-storage.ts 已有更通用的定义
export interface TaskStorageMetadata {
  taskId: ID;
  threadId: ID;  // 使用ID而非ThreadEntity
  workflowId: ID;
  status: TaskStatus;
  submitTime: Timestamp;
  startTime?: Timestamp;
  completeTime?: Timestamp;
  timeout?: number;
  error?: string;
  errorStack?: string;
  tags?: string[];
  customFields?: Record<string, unknown>;
}
```

---

#### QueueStats, PoolStats, ThreadPoolConfig
```typescript
export interface QueueStats {
  pendingCount: number;
  runningCount: number;
  completedCount: number;
  failedCount: number;
  cancelledCount: number;
}

export interface PoolStats {
  totalExecutors: number;
  idleExecutors: number;
  busyExecutors: number;
  minExecutors: number;
  maxExecutors: number;
}

export interface ThreadPoolConfig {
  minExecutors?: number;
  maxExecutors?: number;
  idleTimeout?: number;
  defaultTimeout?: number;
}
```

**使用情况：**
- 仅在 `sdk/graph/services/thread-pool-service.ts` 使用
- 仅在 `sdk/graph/execution/managers/task-queue-manager.ts` 使用

**迁移建议：❌ 不应迁移**
- 纯SDK内部实现细节
- 不涉及跨模块共享
- 不需要持久化

---

### 3.2 triggered-subworkflow.types.ts

#### TriggeredSubgraphTask
```typescript
export interface TriggeredSubgraphTask {
  subgraphId: ID;
  input: Record<string, any>;
  triggerId: string;
  mainThreadEntity: ThreadEntity;  // ⚠️ 依赖实现类
  config?: {
    waitForCompletion?: boolean;
    timeout?: number;
    recordHistory?: boolean;
    metadata?: any;
  };
}
```

**依赖分析：**
- ❌ 依赖 `ThreadEntity`（SDK实现类）
- ✅ 依赖 `ID`（packages/types）

**迁移建议：❌ 不应迁移**
- 包含对实现类的依赖
- 是运行时任务提交类型

---

#### ExecutedSubgraphResult, TaskSubmissionResult, QueueTask
```typescript
export interface ExecutedSubgraphResult {
  subgraphEntity: ThreadEntity;  // ⚠️ 依赖实现类
  threadResult: ThreadResult;
  executionTime: number;
}

export interface TaskSubmissionResult {
  taskId: string;
  status: TaskStatus;
  message: string;
  submitTime: number;
}

export interface QueueTask {
  taskId: string;
  threadEntity: ThreadEntity;  // ⚠️ 依赖实现类
  resolve: (value: ExecutedSubgraphResult) => void;  // ⚠️ 函数类型
  reject: (error: Error) => void;  // ⚠️ 函数类型
  submitTime: number;
  timeout?: number;
}
```

**迁移建议：❌ 不应迁移**
- `ExecutedSubgraphResult` 和 `QueueTask` 依赖实现类和函数类型
- `TaskSubmissionResult` 虽然是纯数据，但仅SDK内部使用

---

### 3.3 dynamic-thread.types.ts

#### DynamicThreadInfo
```typescript
export interface DynamicThreadInfo {
  id: string;
  threadEntity: ThreadEntity;  // ⚠️ 依赖实现类
  status: TaskStatus;
  submitTime: number;
  startTime?: number;
  completeTime?: number;
  result?: ThreadResult;
  error?: Error;
  parentThreadId?: string;
}
```

**迁移建议：❌ 不应迁移**
- 依赖实现类
- 运行时状态管理类型

---

#### DynamicThreadEvent, DynamicThreadEventType
```typescript
export interface DynamicThreadEvent {
  type: DynamicThreadEventType;
  threadId: string;
  timestamp: number;
  workflowId?: string;
  data?: any;
}

export type DynamicThreadEventType =
  | "DYNAMIC_THREAD_REQUESTED"
  | "DYNAMIC_THREAD_COMPLETED"
  | "DYNAMIC_THREAD_FAILED"
  | "DYNAMIC_THREAD_CANCELLED";
```

**使用情况：**
- 在事件系统中使用
- 可能需要跨模块传递事件

**迁移建议：✅ 应该迁移**
- 纯数据结构，无实现依赖
- 事件类型通常需要跨模块共享
- 符合事件驱动架构的设计原则

**迁移目标位置：**
```
packages/types/src/events/dynamic-thread-events.ts
```

---

#### CreateDynamicThreadRequest, DynamicThreadConfig
```typescript
export interface CreateDynamicThreadRequest {
  workflowId: string;
  input: Record<string, any>;
  triggerId: string;
  mainThreadEntity: ThreadEntity;  // ⚠️ 依赖实现类
  config?: DynamicThreadConfig;
}

export interface DynamicThreadConfig {
  waitForCompletion?: boolean;
  timeout?: number;
  recordHistory?: boolean;
  metadata?: any;
}
```

**迁移建议：**
- `CreateDynamicThreadRequest`: ❌ 不应迁移（依赖实现类）
- `DynamicThreadConfig`: ⚠️ 可考虑迁移（纯配置，但仅SDK内部使用）

---

### 3.4 tool-visibility.types.ts

#### ToolVisibilityContext, VisibilityDeclaration
```typescript
export interface ToolVisibilityContext {
  currentScope: ToolScope;
  scopeId: string;
  visibleTools: Set<string>;  // ⚠️ Set类型（运行时）
  declarationHistory: VisibilityDeclaration[];
  lastDeclarationIndex: number;
  initializedAt: number;
}

export interface VisibilityDeclaration {
  timestamp: number;
  scope: ToolScope;
  scopeId: string;
  toolIds: string[];
  messageIndex: number;
  changeType: "init" | "enter_scope" | "add_tools" | "exit_scope" | "refresh";
}
```

**依赖分析：**
- `ToolVisibilityContext` 包含 `Set<string>`（运行时数据结构）
- 依赖 `ToolScope`（定义在SDK内部）

**迁移建议：❌ 不应迁移**
- 包含运行时数据结构（Set）
- 依赖SDK内部类型
- 是运行时状态管理类型

---

## 四、迁移决策矩阵

| 类型 | 当前位置 | 是否迁移 | 目标位置 | 理由 |
|------|---------|---------|---------|------|
| TaskStatus | task.types.ts | ⚠️ 删除重复 | - | packages/types已有定义 |
| WorkerStatus | task.types.ts | ❌ 不迁移 | - | SDK内部实现细节 |
| TaskInfo | task.types.ts | ❌ 不迁移 | - | 依赖ThreadEntity实现类 |
| QueueStats | task.types.ts | ❌ 不迁移 | - | SDK内部实现细节 |
| PoolStats | task.types.ts | ❌ 不迁移 | - | SDK内部实现细节 |
| ThreadPoolConfig | task.types.ts | ❌ 不迁移 | - | SDK内部实现细节 |
| ExecutorWrapper | task.types.ts | ❌ 不迁移 | - | SDK内部实现细节 |
| TriggeredSubgraphTask | triggered-subworkflow.types.ts | ❌ 不迁移 | - | 依赖ThreadEntity |
| ExecutedSubgraphResult | triggered-subworkflow.types.ts | ❌ 不迁移 | - | 依赖ThreadEntity |
| TaskSubmissionResult | triggered-subworkflow.types.ts | ❌ 不迁移 | - | SDK内部使用 |
| QueueTask | triggered-subworkflow.types.ts | ❌ 不迁移 | - | 包含函数类型 |
| DynamicThreadInfo | dynamic-thread.types.ts | ❌ 不迁移 | - | 依赖ThreadEntity |
| DynamicThreadEvent | dynamic-thread.types.ts | ✅ 迁移 | events/ | 纯数据，事件类型 |
| DynamicThreadEventType | dynamic-thread.types.ts | ✅ 迁移 | events/ | 纯数据，事件类型 |
| CreateDynamicThreadRequest | dynamic-thread.types.ts | ❌ 不迁移 | - | 依赖ThreadEntity |
| DynamicThreadConfig | dynamic-thread.types.ts | ⚠️ 可选 | config/ | 纯配置，但仅内部使用 |
| ExecutedThreadResult | dynamic-thread.types.ts | ❌ 不迁移 | - | 依赖ThreadEntity |
| ThreadSubmissionResult | dynamic-thread.types.ts | ❌ 不迁移 | - | SDK内部使用 |
| CallbackInfo | dynamic-thread.types.ts | ❌ 删除 | - | 被GenericCallbackInfo替代 |
| ToolVisibilityContext | tool-visibility.types.ts | ❌ 不迁移 | - | 包含Set，运行时状态 |
| VisibilityDeclaration | tool-visibility.types.ts | ❌ 不迁移 | - | SDK内部使用 |
| VisibilityChangeType | tool-visibility.types.ts | ❌ 不迁移 | - | SDK内部使用 |
| VisibilityUpdateRequest | tool-visibility.types.ts | ❌ 删除 | - | 未使用，过度设计 |

---

## 五、迁移实施方案

### 5.1 Phase 1: 删除重复定义（优先级：高）

**任务：删除sdk中的TaskStatus定义**

```typescript
// 删除文件：sdk/graph/execution/types/task.types.ts 中的 TaskStatus 定义

// 更新导入：所有使用TaskStatus的文件改为从 @modular-agent/types 导入
import { TaskStatus } from "@modular-agent/types";
```

**影响范围：**
- `sdk/graph/services/task-registry.ts`
- `sdk/graph/services/thread-pool-service.ts`
- `sdk/graph/execution/managers/dynamic-thread-manager.ts`
- `sdk/graph/execution/managers/task-queue-manager.ts`
- `sdk/graph/execution/types/triggered-subworkflow.types.ts`
- `sdk/graph/execution/types/dynamic-thread.types.ts`

---

### 5.2 Phase 2: 迁移事件类型（优先级：中）

**任务：迁移DynamicThreadEvent相关类型**

**步骤1：在packages/types中创建新文件**
```typescript
// packages/types/src/events/dynamic-thread-events.ts

import type { ID, Timestamp } from "../common.js";

/**
 * Dynamic Thread Event Types
 */
export const enum DynamicThreadEventType {
  REQUESTED = "DYNAMIC_THREAD_REQUESTED",
  COMPLETED = "DYNAMIC_THREAD_COMPLETED",
  FAILED = "DYNAMIC_THREAD_FAILED",
  CANCELLED = "DYNAMIC_THREAD_CANCELLED",
}

/**
 * Dynamic Thread Event
 */
export interface DynamicThreadEvent {
  /** Event type */
  type: DynamicThreadEventType;
  /** Thread ID */
  threadId: ID;
  /** Event timestamp */
  timestamp: Timestamp;
  /** Workflow ID (optional) */
  workflowId?: ID;
  /** Event data (optional) */
  data?: unknown;
}
```

**步骤2：更新packages/types导出**
```typescript
// packages/types/src/events/index.ts
export * from "./dynamic-thread-events.js";
```

**步骤3：更新SDK中的导入**
```typescript
// sdk/graph/execution/types/dynamic-thread.types.ts
// 删除 DynamicThreadEvent 和 DynamicThreadEventType 定义

// 所有使用处更新导入
import { 
  DynamicThreadEvent, 
  DynamicThreadEventType 
} from "@modular-agent/types";
```

**影响范围：**
- `sdk/graph/execution/managers/dynamic-thread-manager.ts`
- `sdk/graph/execution/managers/callback-manager.ts`
- `sdk/graph/execution/utils/event/dynamic-thread-events.ts`

---

### 5.3 Phase 3: 删除冗余类型（优先级：低）

**任务：删除未使用和冗余的类型**

1. 删除 `CallbackInfo`（被GenericCallbackInfo替代）
2. 删除 `VisibilityUpdateRequest`（未使用）
3. 删除 `types/index.ts`（未被使用）

---

## 六、迁移收益评估

### 6.1 正向收益

**类型一致性：**
- 消除 `TaskStatus` 的重复定义
- 统一事件类型的定义位置
- 减少类型混淆和维护成本

**跨模块共享：**
- 事件类型可在apps和packages中使用
- 便于实现跨模块的事件监听和处理

**包依赖清晰：**
- packages/types 作为唯一的类型定义源
- 减少SDK对外的类型暴露

### 6.2 潜在风险

**循环依赖风险：**
- 迁移事件类型需要确保不引入循环依赖
- **缓解措施**：事件类型应只依赖基础类型（ID, Timestamp）

**破坏性变更：**
- 导入路径变更可能影响现有代码
- **缓解措施**：渐进式迁移，保持向后兼容

**类型膨胀：**
- packages/types 可能变得过于庞大
- **缓解措施**：只迁移真正需要共享的类型

---

## 七、不迁移的理由总结

### 7.1 依赖实现类

以下类型依赖 `ThreadEntity` 等SDK实现类：
- `TaskInfo`
- `TriggeredSubgraphTask`
- `ExecutedSubgraphResult`
- `QueueTask`
- `DynamicThreadInfo`
- `CreateDynamicThreadRequest`
- `ExecutedThreadResult`

**原因：**
- packages/types 应该只包含纯数据结构
- 实现类依赖会导致循环依赖
- 这些类型是运行时状态，不需要跨模块共享

### 7.2 SDK内部实现细节

以下类型是SDK内部实现细节：
- `WorkerStatus`
- `QueueStats`
- `PoolStats`
- `ThreadPoolConfig`
- `ExecutorWrapper`
- `TaskSubmissionResult`
- `ThreadSubmissionResult`
- `ToolVisibilityContext`
- `VisibilityDeclaration`

**原因：**
- 仅在SDK内部使用
- 不涉及跨模块通信
- 暴露这些类型会增加API表面积

### 7.3 包含运行时数据结构

以下类型包含运行时数据结构：
- `ToolVisibilityContext`（包含 `Set<string>`）
- `QueueTask`（包含函数类型）

**原因：**
- Set、Function等不是可序列化的纯数据
- 不适合放在共享类型包中

---

## 八、最终建议

### 8.1 应该执行的操作

1. ✅ **删除重复定义**：删除sdk中的 `TaskStatus`，统一使用packages/types中的定义
2. ✅ **迁移事件类型**：将 `DynamicThreadEvent` 和 `DynamicThreadEventType` 迁移到packages/types
3. ✅ **删除冗余类型**：删除 `CallbackInfo`、`VisibilityUpdateRequest`、`types/index.ts`

### 8.2 不应该迁移的类型

**保持现状的理由充分：**
- 大部分类型依赖SDK实现类
- 是运行时状态管理类型
- 仅在SDK内部使用
- 不需要跨模块共享

**packages/types 的定位：**
- 共享的、稳定的、纯数据的类型定义
- 不应包含实现细节和运行时状态

### 8.3 架构设计原则

**类型分层原则：**
```
packages/types (共享层)
  ↓
sdk/graph/execution/types (SDK层)
  ↓
sdk/graph/execution/managers (实现层)
```

**每一层的职责：**
- **共享层**：跨模块共享的纯数据类型
- **SDK层**：SDK内部的类型定义和接口
- **实现层**：具体的实现类和运行时状态

---

## 九、实施时间表

| 阶段 | 任务 | 预计时间 | 优先级 |
|------|------|---------|--------|
| Phase 1 | 删除TaskStatus重复定义 | 0.5小时 | 高 |
| Phase 2 | 迁移DynamicThreadEvent类型 | 1小时 | 中 |
| Phase 3 | 删除冗余类型 | 0.5小时 | 低 |
| Phase 4 | 更新文档和测试 | 1小时 | 低 |

**总计：3小时**

---

## 十、验收标准

### 10.1 功能验收

- [ ] 所有测试通过
- [ ] 类型检查无错误
- [ ] 构建成功
- [ ] 无循环依赖

### 10.2 架构验收

- [ ] TaskStatus 统一从 @modular-agent/types 导入
- [ ] DynamicThreadEvent 在 packages/types 中定义
- [ ] 无重复类型定义
- [ ] 无未使用的类型

### 10.3 文档验收

- [ ] 更新类型定义文档
- [ ] 更新导入指南
- [ ] 添加迁移说明

---

## 十一、变更历史

- **2026-04-06**：初始版本，完成迁移分析和方案设计
