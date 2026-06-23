# Agent 与 Graph 实例管理架构对比

本文档详细分析 SDK 中 Agent 和 Graph 两个模块的实例管理实现及其差异。

## 一、架构概览

### 1.1 核心组件对比

| 组件类型 | Agent 模块 | Graph 模块 |
|---------|-----------|-----------|
| **实体类** | `AgentLoopEntity` | `ThreadEntity` |
| **注册表** | `AgentLoopRegistry` | `ThreadRegistry` |
| **状态管理** | `AgentLoopState` | `ThreadState` + `ExecutionState` |
| **生命周期管理** | `agent-loop-lifecycle.ts` (函数式) | `ThreadLifecycleManager` + `ThreadLifecycleCoordinator` |
| **消息管理** | `ConversationManager` (必需) | `MessageHistoryManager` + `ConversationManager` (可选) |
| **变量管理** | `VariableStateManager` | `VariableStateManager` |
| **Checkpoint** | `AgentLoopCheckpointCoordinator` | `CheckpointCoordinator` |

### 1.2 目录结构对比

```
sdk/agent/                              sdk/graph/
├── entities/                           ├── entities/
│   ├── agent-loop-entity.ts            │   ├── thread-entity.ts
│   └── agent-loop-state.ts             │   ├── thread-state.ts
├── services/                           │   └── execution-state.ts
│   └── agent-loop-registry.ts          ├── services/
├── checkpoint/                         │   ├── thread-registry.ts
│   ├── checkpoint-coordinator.ts       │   ├── graph-registry.ts
│   ├── diff-calculator.ts              │   └── workflow-registry.ts
│   └── ...                             ├── execution/
├── execution/                          │   ├── managers/
│   ├── factories/                      │   │   ├── thread-lifecycle-manager.ts
│   │   └── agent-loop-factory.ts       │   │   └── thread-cascade-manager.ts
│   ├── handlers/                       │   ├── coordinators/
│   │   └── agent-loop-lifecycle.ts     │   │   └── thread-lifecycle-coordinator.ts
│   └── managers/                       │   └── ...
│       └── ...                         └── ...
```

---

## 二、实体类对比

### 2.1 AgentLoopEntity

**文件位置**: `sdk/agent/entities/agent-loop-entity.ts`

**核心职责**:
- 封装 Agent Loop 执行实例的所有数据
- 提供数据访问接口 (getter/setter)
- 持有状态管理器实例

**关键属性**:
```typescript
class AgentLoopEntity {
  readonly id: string;                          // 实例 ID
  readonly config: AgentLoopConfig;             // 循环配置
  readonly state: AgentLoopState;               // 执行状态
  conversationManager: ConversationManager;     // 对话管理器 (必需)
  readonly variableStateManager: VariableStateManager;  // 变量管理器
  abortController?: AbortController;            // 中止控制器
  parentThreadId?: ID;                          // 父 Thread ID
  nodeId?: ID;                                  // 节点 ID
}
```

**设计特点**:
- 纯数据实体，不包含工厂方法
- `ConversationManager` 是必需的，统一管理消息历史
- 支持中断控制 (pause/resume/stop)
- 提供 `cleanup()` 方法用于资源清理

### 2.2 ThreadEntity

**文件位置**: `sdk/graph/entities/thread-entity.ts`

**核心职责**:
- 封装 Thread 执行实例的所有数据
- 提供数据访问接口 (getter/setter)
- 持有状态管理器实例
- 支持子图执行栈管理

**关键属性**:
```typescript
class ThreadEntity {
  readonly id: string;                          // 实例 ID
  private readonly thread: Thread;              // Thread 数据对象 (私有)
  readonly state: ThreadState;                  // 执行状态
  private readonly executionState: ExecutionState;  // 执行状态管理器 (子图栈)
  readonly messageHistoryManager: MessageHistoryManager;  // 消息历史管理器
  readonly variableStateManager: VariableStateManager;    // 变量管理器
  abortController?: AbortController;            // 中止控制器
  conversationManager?: ConversationManager;    // 对话管理器 (可选)
  triggerManager?: any;                         // 触发器管理
  toolVisibilityCoordinator?: any;              // 工具可见性协调器
}
```

**设计特点**:
- 纯数据实体，不包含工厂方法
- `ConversationManager` 是可选的，主要使用 `MessageHistoryManager`
- 支持子图执行栈 (`ExecutionState`)
- 支持 Fork/Join 上下文
- 支持触发子工作流上下文
- 提供 `cleanup()` 方法用于资源清理

### 2.3 实体类差异总结

| 维度 | AgentLoopEntity | ThreadEntity |
|------|-----------------|--------------|
| **消息管理** | `ConversationManager` (必需) | `MessageHistoryManager` + `ConversationManager` (可选) |
| **状态管理** | 单一 `AgentLoopState` | `ThreadState` + `ExecutionState` (子图栈) |
| **执行模型** | 简单循环模型 | 复杂图执行模型 |
| **上下文支持** | 父 Thread ID, 节点 ID | Fork/Join, 子工作流, 子图栈 |
| **事件构建** | 无 | `buildEvent()` 自动填充上下文 |

---

## 三、注册表对比

### 3.1 AgentLoopRegistry

**文件位置**: `sdk/agent/services/agent-loop-registry.ts`

**核心方法**:
```typescript
class AgentLoopRegistry {
  register(entity: AgentLoopEntity): void;
  unregister(id: ID): boolean;
  get(id: ID): AgentLoopEntity | undefined;
  has(id: ID): boolean;
  getAll(): AgentLoopEntity[];
  getAllIds(): ID[];
  size(): number;
  
  // 状态查询
  getByStatus(status: AgentLoopStatus): AgentLoopEntity[];
  getRunning(): AgentLoopEntity[];
  getPaused(): AgentLoopEntity[];
  getCompleted(): AgentLoopEntity[];
  getFailed(): AgentLoopEntity[];
  
  // 清理
  cleanupCompleted(): number;
  clear(): void;
  cleanup(): void;  // 调用每个实体的 cleanup() 方法
}
```

### 3.2 ThreadRegistry

**文件位置**: `sdk/graph/services/thread-registry.ts`

**核心方法**:
```typescript
class ThreadRegistry {
  register(threadEntity: ThreadEntity): void;
  delete(threadId: string): void;
  get(threadId: string): ThreadEntity | null;
  has(threadId: string): boolean;
  getAll(): ThreadEntity[];
  getAllIds(): string[];
  size(): number;
  
  // 工作流查询
  isWorkflowActive(workflowId: string): boolean;
  
  // 状态查询
  getByStatus(status: ThreadStatus): ThreadEntity[];
  getRunning(): ThreadEntity[];
  getPaused(): ThreadEntity[];
  getCompleted(): ThreadEntity[];
  getFailed(): ThreadEntity[];
  getCancelled(): ThreadEntity[];
  
  // 清理
  cleanupCompleted(): number;
  cleanupFailed(): number;
  cleanupCancelled(): number;
  clear(): void;
  cleanup(): void;  // 调用每个实体的 cleanup() 方法
}
```

### 3.3 注册表差异总结

| 维度 | AgentLoopRegistry | ThreadRegistry |
|------|-------------------|----------------|
| **删除方法** | `unregister()` | `delete()` |
| **工作流查询** | 无 | `isWorkflowActive()` |
| **清理方法** | `cleanupCompleted()` | `cleanupCompleted()`, `cleanupFailed()`, `cleanupCancelled()` |
| **命名风格** | `unregister` | `delete` |

---

## 四、生命周期管理对比

### 4.1 Agent 生命周期管理

**文件位置**: `sdk/agent/execution/handlers/agent-loop-lifecycle.ts`

**设计模式**: 函数式导出

**核心函数**:
```typescript
// 创建 Checkpoint
async function createAgentLoopCheckpoint(
  entity: AgentLoopEntity,
  dependencies: AgentLoopCheckpointDependencies,
  options?: AgentLoopCheckpointOptions,
): Promise<string>;

// 清理资源
function cleanupAgentLoop(entity: AgentLoopEntity): void;

// 克隆实体
function cloneAgentLoop(entity: AgentLoopEntity): AgentLoopEntity;
```

**特点**:
- 使用纯函数而非类方法
- 生命周期逻辑与实体类分离
- `cleanupAgentLoop()` 有完整实现，清理 state、conversationManager、variableStateManager

### 4.2 Graph 生命周期管理

**文件位置**: 
- `sdk/graph/execution/managers/thread-lifecycle-manager.ts`
- `sdk/graph/execution/coordinators/thread-lifecycle-coordinator.ts`

**设计模式**: Manager + Coordinator 分层

#### ThreadLifecycleManager

**职责**: 原子状态转换操作

**核心方法**:
```typescript
class ThreadLifecycleManager {
  async startThread(thread: Thread): Promise<void>;
  async pauseThread(thread: Thread): Promise<void>;
  async resumeThread(thread: Thread): Promise<void>;
  async completeThread(thread: Thread, result: ThreadResult): Promise<void>;
  async failThread(thread: Thread, error: Error): Promise<void>;
  async cancelThread(thread: Thread, reason?: string): Promise<void>;
}
```

**特点**:
- 每个方法代表一个完整的状态转换单元
- 不涉及业务逻辑实现细节
- 触发生命周期事件
- 验证状态转换合法性

#### ThreadLifecycleCoordinator

**职责**: 高层流程编排和协调

**核心方法**:
```typescript
class ThreadLifecycleCoordinator {
  async execute(workflowId: string, options: ThreadOptions): Promise<ThreadResult>;
  async pauseThread(threadId: string): Promise<void>;
  async resumeThread(threadId: string): Promise<ThreadResult>;
  async stopThread(threadId: string): Promise<void>;
  async forceSetThreadStatus(threadId: string, status: ThreadStatus): Promise<void>;
  async forcePauseThread(threadId: string): Promise<void>;
  async forceCancelThread(threadId: string, reason?: string): Promise<void>;
}
```

**特点**:
- 无状态设计，不持有实例变量
- 依赖注入，通过构造函数接收依赖
- 流程编排，管理复杂多步操作
- 委托模式，使用 Manager 进行原子状态操作

### 4.3 生命周期管理差异总结

| 维度 | Agent | Graph |
|------|-------|-------|
| **设计模式** | 函数式导出 | Manager + Coordinator 分层 |
| **复杂度** | 简单，3 个函数 | 复杂，Manager + Coordinator |
| **状态转换** | 无独立管理 | `ThreadLifecycleManager` 专门管理 |
| **流程编排** | 无 | `ThreadLifecycleCoordinator` 负责 |
| **事件触发** | 无 | 自动触发生命周期事件 |
| **状态验证** | 无 | `validateTransition()` 验证合法性 |
| **级联操作** | 无 | `ThreadCascadeManager` 支持级联取消 |

---

## 五、Checkpoint 对比

### 5.1 Agent Checkpoint

**文件位置**: `sdk/agent/checkpoint/`

**核心组件**:
- `AgentLoopCheckpointCoordinator` - 协调器
- `AgentLoopDiffCalculator` - 差异计算器
- `AgentLoopDeltaRestorer` - 增量恢复器
- `AgentLoopCheckpointResolver` - 配置解析器

**状态类型**: `AgentLoopStateSnapshot`

**特点**:
- 独立模块，完整实现
- 静态方法，依赖通过参数传递
- 支持增量 checkpoint (delta)

### 5.2 Graph Checkpoint

**文件位置**: `sdk/graph/execution/coordinators/` 和 `sdk/graph/execution/utils/`

**核心组件**:
- `CheckpointCoordinator` - 协调器
- `CheckpointDiffCalculator` - 差异计算器
- `DeltaCheckpointRestorer` - 增量恢复器
- `GraphCheckpointConfigResolver` - 配置解析器

**状态类型**: `ThreadStateSnapshot`

**特点**:
- 与 execution 紧密耦合
- 支持更复杂的状态 (Fork/Join, 子工作流)
- 支持节点级 checkpoint
- 支持从 checkpoint 恢复子线程

### 5.3 Checkpoint 差异总结

| 维度 | Agent Checkpoint | Graph Checkpoint |
|------|------------------|------------------|
| **目录位置** | `sdk/agent/checkpoint/` | `sdk/graph/execution/` |
| **模块独立性** | 独立模块 | 与 execution 耦合 |
| **状态复杂度** | 简单 | 复杂 (Fork/Join, 子工作流) |
| **节点级支持** | 无 | `createNodeCheckpoint()` |
| **子线程恢复** | 无 | 支持 |

---

## 六、消息管理对比

### 6.1 Agent 消息管理

**管理器**: `ConversationManager` (必需)

**特点**:
- 统一管理消息历史
- 支持 Token 统计
- 支持事件触发
- 消息标记 (markMap)

### 6.2 Graph 消息管理

**管理器**: `MessageHistoryManager` + `ConversationManager` (可选)

**特点**:
- `MessageHistoryManager` 作为主要管理器
- `ConversationManager` 可选集成
- 消息同步到两个管理器
- 支持消息历史标准化

### 6.3 消息管理差异总结

| 维度 | Agent | Graph |
|------|-------|-------|
| **主要管理器** | `ConversationManager` | `MessageHistoryManager` |
| **ConversationManager** | 必需 | 可选 |
| **消息同步** | 无 | 同步到两个管理器 |
| **Token 统计** | 内置 | 需要 ConversationManager |

---

## 七、设计原则差异

### 7.1 Agent 设计原则

1. **简洁性**: 执行模型简单，生命周期管理使用函数式导出
2. **独立性**: Checkpoint 作为独立模块
3. **统一性**: 使用统一的 `ConversationManager` 管理消息

### 7.2 Graph 设计原则

1. **分层性**: Manager + Coordinator 分层，职责清晰
2. **可扩展性**: 支持子图、Fork/Join、子工作流等复杂场景
3. **事件驱动**: 自动触发生命周期事件
4. **高内聚**: Checkpoint 与 execution 紧密耦合

---

## 八、总结

### 8.1 核心差异

| 维度 | Agent | Graph |
|------|-------|-------|
| **执行模型** | 简单循环 | 复杂图执行 |
| **生命周期管理** | 函数式 (简单) | Manager + Coordinator (复杂) |
| **状态管理** | 单一状态 | 状态 + 子图栈 |
| **消息管理** | ConversationManager (必需) | MessageHistoryManager + ConversationManager (可选) |
| **Checkpoint** | 独立模块 | 与 execution 耦合 |
| **上下文支持** | 基础 | Fork/Join, 子工作流, 子图栈 |

### 8.2 设计决策

1. **不强制架构对称**: Agent 和 Graph 的执行模型本质不同，不需要强制对称
2. **按需复杂度**: Agent 保持简单，Graph 按需增加复杂度
3. **高内聚低耦合**: Graph 的 Checkpoint 与 execution 紧密耦合，符合高内聚原则

### 8.3 最佳实践

- **Agent**: 适用于简单的 LLM 循环场景
- **Graph**: 适用于复杂的工作流编排场景
- **选择依据**: 根据执行模型复杂度选择合适的模块
