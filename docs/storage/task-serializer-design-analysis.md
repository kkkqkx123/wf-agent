# Task Serializer 设计分析报告

## 概述

本文档分析 `sdk/core/utils/task/serializer.ts` 的设计合理性，对比项目中其他实体的序列化处理方式，并提出优化建议。

## 一、当前设计分析

### 1.1 文件位置

`sdk/core/utils/task/serializer.ts`

### 1.2 设计优点

| 优点 | 说明 |
|------|------|
| 职责单一 | 专门处理 Task 数据的序列化/反序列化 |
| 纯函数设计 | 序列化函数无副作用，易于测试 |
| 类型安全 | 定义了 `TaskSnapshot` 和 `SerializedThreadResult` 接口 |
| 错误处理完善 | 提供了 `serializeError`/`deserializeError` 处理 Error 对象 |

### 1.3 核心问题

#### 问题 1：数据丢失严重

`SerializedThreadResult` 只保存了摘要信息：

```typescript
export interface SerializedThreadResult {
  output?: Record<string, unknown>;
  nodeResultCount?: number;  // 只保存数量，丢失完整数据
  status?: string;
  error?: string;
}
```

完整的 `ThreadResult` 包含：
- `nodeResults` 数组（完整执行结果）
- `metadata`（元数据信息）

**影响**：从存储恢复后无法重建完整的执行结果。

#### 问题 2：instance 引用无法恢复

在 `task-registry.ts:170` 中：

```typescript
this.tasks.set(taskId, {
  id: snapshot.id,
  instanceType: snapshot.instanceType,
  instance: null as any, // Will be restored on demand  ← 类型安全漏洞
  status: snapshot.status,
  // ...
});
```

**影响**：
- 使用 `null as any` 绕过类型检查，存在运行时风险
- 从存储加载时 `instance` 为 null，无法恢复运行时状态

#### 问题 3：序列化策略不一致

| 数据类型 | 序列化方式 | 问题 |
|----------|-----------|------|
| `ThreadResult` | 简化序列化（只保存摘要） | 数据丢失 |
| `Error` | 完整序列化（message, name, stack） | 正确 |
| `ExecutionInstance` | 不序列化 | 无法恢复 |

---

## 二、项目中其他实体的处理方式

### 2.1 Checkpoint 序列化

**文件**：`sdk/core/utils/checkpoint/serializer.ts`

```typescript
export function serializeCheckpoint(checkpoint: Checkpoint): Uint8Array {
  const json = JSON.stringify(checkpoint, null, 2);
  return new TextEncoder().encode(json);
}

export function deserializeCheckpoint(data: Uint8Array): Checkpoint {
  const json = new TextDecoder().decode(data);
  return JSON.parse(json) as Checkpoint;
}
```

**特点**：
- 直接序列化完整对象
- `Checkpoint` 类型本身就是可序列化的纯数据结构

### 2.2 ThreadStateSnapshot

**文件**：`packages/types/src/checkpoint/graph/snapshot.ts`

```typescript
export interface ThreadStateSnapshot {
  status: ThreadStatus;
  currentNodeId: ID;
  variables: any[];
  variableScopes: VariableScopes;
  input: Record<string, any>;
  output: Record<string, any>;
  nodeResults: Record<string, NodeExecutionResult>;  // 完整数据
  errors: any[];
  conversationState: {
    messages: any[];
    markMap: MessageMarkMap;
    tokenUsage: TokenUsageStats | null;
    currentRequestUsage: TokenUsageStats | null;
  };
  toolApprovalState?: { ... };
  triggerStates?: Map<ID, TriggerRuntimeState>;
  forkJoinContext?: { ... };
  triggeredSubworkflowContext?: { ... };
}
```

**特点**：包含恢复运行时状态所需的**所有信息**。

### 2.3 AgentLoopStateSnapshot

**文件**：`packages/types/src/checkpoint/agent/snapshot.ts`

```typescript
export interface AgentLoopStateSnapshot {
  status: AgentLoopStatus;
  currentIteration: number;
  toolCallCount: number;
  startTime: number | null;
  endTime: number | null;
  error: any;
  messages: Message[];      // 完整消息历史
  variables: Record<string, any>;  // 完整变量
  config?: any;
}
```

**特点**：包含恢复 AgentLoop 所需的**完整状态**。

### 2.4 设计模式对比

| 实体 | 序列化方式 | 是否可恢复 |
|------|-----------|-----------|
| **TaskSnapshot** | 简化序列化（摘要） | ❌ 不可完全恢复 |
| **Checkpoint** | 完整序列化 | ✅ 可恢复 |
| **ThreadStateSnapshot** | 完整状态快照 | ✅ 可恢复 |
| **AgentLoopStateSnapshot** | 完整状态快照 | ✅ 可恢复 |

---

## 三、Checkpoint 系统的设计模式

Checkpoint 系统采用了完善的分层设计：

```
┌─────────────────────────────────────────────────────────────┐
│                    Runtime Layer                            │
│  ThreadEntity / AgentLoopEntity                             │
│  (包含复杂对象、方法、运行时状态)                              │
└─────────────────────┬───────────────────────────────────────┘
                      │ createSnapshot()
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                    Snapshot Layer                           │
│  ThreadStateSnapshot / AgentLoopStateSnapshot               │
│  (纯数据结构，可序列化，包含完整状态)                          │
└─────────────────────┬───────────────────────────────────────┘
                      │ serialize()
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                    Storage Layer                            │
│  Uint8Array (持久化存储)                                     │
└─────────────────────────────────────────────────────────────┘
```

**关键设计原则**：
1. Entity 类不直接序列化
2. 通过 Snapshot 机制提取可序列化状态
3. Snapshot 包含恢复所需的所有信息
4. 支持增量更新（Delta）

---

## 四、优化建议

### 4.1 方案一：对齐 Checkpoint 的 Snapshot 模式（推荐）

重新设计 `TaskSnapshot`，包含完整可恢复信息：

```typescript
export interface TaskSnapshot {
  /** 序列化版本号 */
  _version: 1;
  
  /** Task ID */
  id: string;
  
  /** Execution instance type */
  instanceType: "agent" | "thread";
  
  /** Instance ID */
  instanceId: string;
  
  /** Workflow ID */
  workflowId: string;
  
  /** Thread ID (for thread instances) */
  threadId?: string;
  
  /** Task Status */
  status: TaskStatus;
  
  /** Timestamps */
  submitTime: number;
  startTime?: number;
  completeTime?: number;
  
  /** Timeout */
  timeout?: number;
  
  /** 完整执行结果（而非摘要） */
  result?: ThreadResult;
  
  /** 完整错误信息 */
  error?: {
    message: string;
    name: string;
    stack?: string;
    cause?: unknown;
  };
  
  /** 用于恢复 instance 的上下文信息 */
  instanceContext?: {
    // Thread 相关
    currentNodeId?: string;
    input?: Record<string, unknown>;
    output?: Record<string, unknown>;
    // Agent 相关
    config?: AgentLoopConfig;
  };
}
```

### 4.2 方案二：分离存储与运行时模型

```typescript
// 存储模型（纯数据）
interface TaskStorageModel {
  id: string;
  instanceType: "agent" | "thread";
  instanceId: string;
  workflowId: string;
  threadId?: string;
  status: TaskStatus;
  submitTime: number;
  startTime?: number;
  completeTime?: number;
  timeout?: number;
  result?: ThreadResult;
  error?: SerializedError;
}

// 运行时模型（包含 Entity 引用）
interface TaskRuntimeModel {
  id: string;
  instance: ExecutionInstance;
  storageModel: TaskStorageModel;
}

// 转换器
class TaskModelConverter {
  toStorage(runtime: TaskRuntimeModel): TaskStorageModel;
  toRuntime(storage: TaskStorageModel, instance: ExecutionInstance): TaskRuntimeModel;
}
```

### 4.3 方案三：引入恢复策略模式

```typescript
interface TaskRecoveryStrategy {
  canRecover(snapshot: TaskSnapshot): boolean;
  recover(snapshot: TaskSnapshot): Promise<ExecutionInstance>;
}

class ThreadRecoveryStrategy implements TaskRecoveryStrategy {
  async recover(snapshot: TaskSnapshot): Promise<ThreadEntity> {
    // 使用 ThreadBuilder 从 snapshot 重建 ThreadEntity
  }
}

class AgentRecoveryStrategy implements TaskRecoveryStrategy {
  async recover(snapshot: TaskSnapshot): Promise<AgentLoopEntity> {
    // 使用 AgentLoopFactory 从 snapshot 重建 AgentLoopEntity
  }
}

// 策略注册表
class TaskRecoveryRegistry {
  private strategies: Map<string, TaskRecoveryStrategy> = new Map();
  
  register(type: string, strategy: TaskRecoveryStrategy): void;
  recover(snapshot: TaskSnapshot): Promise<ExecutionInstance>;
}
```

---

## 五、具体改进建议

### 5.1 确保 ThreadResult 类型可序列化

检查 `ThreadResult` 中是否有不可序列化的字段：

```typescript
// 检查项：
// - 函数类型 ❌
// - Symbol ❌
// - 循环引用 ❌
// - Map/Set → 转换为 Object/Array
// - Date → 转换为 ISO string
```

如果有不可序列化字段，定义 `SerializableThreadResult` 类型。

### 5.2 修复类型安全问题

**当前代码**：
```typescript
instance: null as any  // ❌ 类型不安全
```

**改进方案**：
```typescript
// 方案 A：使用可选类型
instance?: ExecutionInstance;

// 方案 B：使用联合类型
instanceRef: 
  | { type: 'loaded'; instanceId: string }
  | { type: 'runtime'; instance: ExecutionInstance };

// 方案 C：使用占位符
instance: ExecutionInstance | { _placeholder: true; instanceId: string };
```

### 5.3 增加版本控制

```typescript
interface TaskSnapshot {
  _version: 1;  // 序列化版本号
  // ...
}

// 反序列化时检查版本
function deserializeTaskSnapshot(data: Uint8Array): TaskSnapshot {
  const json = new TextDecoder().decode(data);
  const snapshot = JSON.parse(json) as TaskSnapshot;
  
  if (snapshot._version !== 1) {
    // 执行版本迁移
    return migrateSnapshot(snapshot);
  }
  
  return snapshot;
}
```

---

## 六、总结

### 6.1 问题严重程度评估

| 问题 | 严重程度 | 影响范围 |
|------|---------|---------|
| `SerializedThreadResult` 数据丢失 | **高** | 无法恢复完整执行结果 |
| `instance` 无法恢复 | **高** | 无法恢复运行时状态 |
| 类型安全漏洞 (`null as any`) | **中** | 潜在运行时错误 |
| 缺少版本控制 | **低** | 未来迁移困难 |

### 6.2 最佳实践参考

项目中 Checkpoint 系统的 `ThreadStateSnapshot`/`AgentLoopStateSnapshot` 设计模式是最佳实践：

1. **专门为序列化设计**：Snapshot 类型是纯数据结构
2. **包含完整状态**：恢复所需的所有信息
3. **支持增量更新**：Delta 机制减少存储开销
4. **版本控制**：便于未来迁移

### 6.3 推荐行动

1. **短期**：修复 `null as any` 类型安全漏洞
2. **中期**：保存完整 `ThreadResult` 而非摘要
3. **长期**：引入恢复策略模式，对齐 Checkpoint 系统设计

---

## 附录：相关文件

- `sdk/core/utils/task/serializer.ts` - Task 序列化工具
- `sdk/graph/services/task-registry.ts` - Task 注册表
- `sdk/core/utils/checkpoint/serializer.ts` - Checkpoint 序列化工具
- `packages/types/src/checkpoint/graph/snapshot.ts` - ThreadStateSnapshot 定义
- `packages/types/src/checkpoint/agent/snapshot.ts` - AgentLoopStateSnapshot 定义
