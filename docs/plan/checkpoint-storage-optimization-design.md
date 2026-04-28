# 检查点存储优化设计方案

## 一、背景与问题分析

### 1.1 当前实现分析

当前检查点存储机制位于以下核心文件：

| 文件 | 职责 |
|-----|------|
| `sdk/core/execution/coordinators/checkpoint-coordinator.ts` | 检查点创建与恢复协调 |
| `sdk/core/execution/managers/checkpoint-state-manager.ts` | 检查点状态管理 |
| `sdk/core/execution/utils/checkpoint-serializer.ts` | 序列化/反序列化 |
| `packages/types/src/checkpoint/snapshot.ts` | 快照类型定义 |

### 1.2 当前存储结构

```typescript
interface ThreadStateSnapshot {
  status: ThreadStatus;
  currentNodeId: ID;
  variables: any[];
  variableScopes: VariableScopes;
  input: Record<string, any>;
  output: Record<string, any>;
  nodeResults: Record<string, NodeExecutionResult>;
  errors: any[];
  conversationState: {
    messages: any[];           // 完整消息历史 - 主要存储开销
    markMap: MessageMarkMap;
    tokenUsage: TokenUsageStats | null;
    currentRequestUsage: TokenUsageStats | null;
  };
  triggerStates?: Map<ID, TriggerRuntimeState>;
  forkJoinContext?: { ... };
  triggeredSubworkflowContext?: { ... };
}
```

### 1.3 问题识别

**核心问题：每次创建检查点都完整存储所有数据**

```
检查点1: [消息1, 消息2, 消息3, ...变量, ...结果]  → 存储 100KB
检查点2: [消息1, 消息2, 消息3, 消息4, ...变量, ...结果]  → 存储 120KB
检查点3: [消息1, 消息2, 消息3, 消息4, 消息5, ...变量, ...结果]  → 存储 140KB
```

**存储开销分析：**

| 数据类型 | 增长特性 | 存储占比（估算） |
|---------|---------|----------------|
| `conversationState.messages` | 线性增长 | 60-80% |
| `nodeResults` | 线性增长 | 10-20% |
| `variables/variableScopes` | 相对稳定 | 5-10% |
| 其他元数据 | 固定大小 | 5-10% |

**具体问题：**

1. **消息历史重复存储**：每条消息在每个检查点中都被完整存储
2. **无差异检测**：未变化的数据仍被完整存储
3. **无压缩机制**：历史消息以原始格式存储
4. **无引用共享**：相同数据无法跨检查点共享

---

## 二、优化方案设计

### 2.1 方案概览

```
┌─────────────────────────────────────────────────────────────────┐
│                    检查点存储优化架构                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐         │
│  │ 增量存储策略 │    │ 消息压缩机制 │    │ 共享引用机制 │         │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘         │
│         │                  │                  │                 │
│         v                  v                  v                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              优化后的检查点存储层                          │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐      │   │
│  │  │ DeltaStorage│  │MessageArchive│  │ SharedRefMgr│      │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 方案一：增量存储策略

#### 2.2.1 设计思路

将检查点分为**基线检查点**和**增量检查点**：

```
基线检查点 (Full Checkpoint)
├── 完整存储所有状态
├── 定期创建（如每10个检查点）
└── 作为后续增量检查点的基准

增量检查点 (Delta Checkpoint)
├── 只存储与前一检查点的差异
├── 消息：只存储新增消息
├── 变量：只存储变化的变量
└── 其他：使用差异编码
```

#### 2.2.2 类型定义

```typescript
/**
 * 检查点类型枚举
 */
export enum CheckpointType {
  FULL = 'FULL',           // 完整检查点
  DELTA = 'DELTA'          // 增量检查点
}

/**
 * 优化后的检查点类型
 */
export interface Checkpoint {
  /** 检查点ID */
  id: ID;
  /** 线程ID */
  threadId: ID;
  /** 工作流ID */
  workflowId: ID;
  /** 时间戳 */
  timestamp: Timestamp;
  /** 检查点类型 */
  type: CheckpointType;
  /** 基线检查点ID（增量检查点需要） */
  baseCheckpointId?: ID;
  /** 前一检查点ID（增量检查点需要） */
  previousCheckpointId?: ID;
  /** 增量数据（增量检查点使用） */
  delta?: CheckpointDelta;
  /** 完整状态（完整检查点使用） */
  fullState?: ThreadStateSnapshot;
  /** 元数据 */
  metadata?: CheckpointMetadata;
}

/**
 * 增量数据结构
 */
export interface CheckpointDelta {
  /** 新增的消息 */
  addedMessages?: LLMMessage[];
  /** 消息变更（索引 -> 新消息） */
  modifiedMessages?: Map<number, LLMMessage>;
  /** 删除的消息索引 */
  deletedMessageIndices?: number[];
  /** 新增的变量 */
  addedVariables?: Variable[];
  /** 修改的变量 */
  modifiedVariables?: Map<string, any>;
  /** 新增的节点结果 */
  addedNodeResults?: Record<string, NodeExecutionResult>;
  /** 状态变更 */
  statusChange?: {
    from: ThreadStatus;
    to: ThreadStatus;
  };
  /** 当前节点变更 */
  currentNodeChange?: {
    from: ID;
    to: ID;
  };
  /** 其他状态差异 */
  otherChanges?: Record<string, { from: any; to: any }>;
}
```

#### 2.2.3 差异计算算法

```typescript
/**
 * 检查点差异计算器
 */
export class CheckpointDiffCalculator {
  /**
   * 计算两个检查点之间的差异
   */
  calculateDelta(
    previous: ThreadStateSnapshot,
    current: ThreadStateSnapshot
  ): CheckpointDelta {
    const delta: CheckpointDelta = {};

    // 1. 计算消息差异
    delta.addedMessages = this.calculateMessageDelta(
      previous.conversationState.messages,
      current.conversationState.messages
    );

    // 2. 计算变量差异
    const varDiff = this.calculateVariableDelta(
      previous.variables,
      current.variables
    );
    if (varDiff.added.length > 0) delta.addedVariables = varDiff.added;
    if (varDiff.modified.size > 0) delta.modifiedVariables = varDiff.modified;

    // 3. 计算节点结果差异
    delta.addedNodeResults = this.calculateNodeResultsDelta(
      previous.nodeResults,
      current.nodeResults
    );

    // 4. 计算状态变更
    if (previous.status !== current.status) {
      delta.statusChange = { from: previous.status, to: current.status };
    }

    // 5. 计算当前节点变更
    if (previous.currentNodeId !== current.currentNodeId) {
      delta.currentNodeChange = {
        from: previous.currentNodeId,
        to: current.currentNodeId
      };
    }

    return delta;
  }

  /**
   * 计算消息差异（只返回新增消息）
   */
  private calculateMessageDelta(
    previousMessages: LLMMessage[],
    currentMessages: LLMMessage[]
  ): LLMMessage[] {
    // 消息通常是追加操作，返回新增部分
    if (currentMessages.length > previousMessages.length) {
      return currentMessages.slice(previousMessages.length);
    }
    return [];
  }

  /**
   * 计算变量差异
   */
  private calculateVariableDelta(
    previousVars: Variable[],
    currentVars: Variable[]
  ): { added: Variable[]; modified: Map<string, any> } {
    const added: Variable[] = [];
    const modified = new Map<string, any>();

    const prevVarMap = new Map(previousVars.map(v => [v.name, v]));

    for (const currentVar of currentVars) {
      const prevVar = prevVarMap.get(currentVar.name);
      if (!prevVar) {
        added.push(currentVar);
      } else if (!this.deepEqual(prevVar.value, currentVar.value)) {
        modified.set(currentVar.name, currentVar.value);
      }
    }

    return { added, modified };
  }

  /**
   * 计算节点结果差异
   */
  private calculateNodeResultsDelta(
    previous: Record<string, NodeExecutionResult>,
    current: Record<string, NodeExecutionResult>
  ): Record<string, NodeExecutionResult> {
    const added: Record<string, NodeExecutionResult> = {};

    for (const [nodeId, result] of Object.entries(current)) {
      if (!previous[nodeId]) {
        added[nodeId] = result;
      }
    }

    return added;
  }

  private deepEqual(a: any, b: any): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
  }
}
```

#### 2.2.4 增量检查点恢复

```typescript
/**
 * 增量检查点恢复器
 */
export class DeltaCheckpointRestorer {
  /**
   * 从增量检查点恢复完整状态
   */
  async restore(
    checkpointId: string,
    dependencies: CheckpointDependencies
  ): Promise<ThreadStateSnapshot> {
    const checkpoint = await dependencies.checkpointStateManager.get(checkpointId);
    if (!checkpoint) {
      throw new CheckpointNotFoundError('Checkpoint not found', checkpointId);
    }

    // 如果是完整检查点，直接返回
    if (checkpoint.type === CheckpointType.FULL) {
      return checkpoint.fullState!;
    }

    // 如果是增量检查点，需要链式恢复
    return this.restoreDeltaCheckpoint(checkpoint, dependencies);
  }

  /**
   * 链式恢复增量检查点
   */
  private async restoreDeltaCheckpoint(
    deltaCheckpoint: Checkpoint,
    dependencies: CheckpointDependencies
  ): Promise<ThreadStateSnapshot> {
    // 1. 找到基线检查点
    const baseCheckpoint = await this.findBaseCheckpoint(
      deltaCheckpoint,
      dependencies
    );

    // 2. 从基线开始，依次应用增量
    let state = baseCheckpoint.fullState!;
    const deltaChain = await this.buildDeltaChain(
      baseCheckpoint.id,
      deltaCheckpoint.id,
      dependencies
    );

    for (const delta of deltaChain) {
      state = this.applyDelta(state, delta);
    }

    return state;
  }

  /**
   * 应用增量到状态
   */
  private applyDelta(
    state: ThreadStateSnapshot,
    delta: CheckpointDelta
  ): ThreadStateSnapshot {
    const newState = { ...state };

    // 应用消息增量
    if (delta.addedMessages && delta.addedMessages.length > 0) {
      newState.conversationState = {
        ...newState.conversationState,
        messages: [
          ...newState.conversationState.messages,
          ...delta.addedMessages
        ]
      };
    }

    // 应用变量增量
    if (delta.addedVariables) {
      newState.variables = [...newState.variables, ...delta.addedVariables];
    }
    if (delta.modifiedVariables) {
      newState.variables = newState.variables.map(v => {
        const modified = delta.modifiedVariables!.get(v.name);
        return modified ? { ...v, value: modified } : v;
      });
    }

    // 应用节点结果增量
    if (delta.addedNodeResults) {
      newState.nodeResults = {
        ...newState.nodeResults,
        ...delta.addedNodeResults
      };
    }

    // 应用状态变更
    if (delta.statusChange) {
      newState.status = delta.statusChange.to;
    }
    if (delta.currentNodeChange) {
      newState.currentNodeId = delta.currentNodeChange.to;
    }

    return newState;
  }
}
```

#### 2.2.5 存储策略配置

```typescript
/**
 * 增量存储策略配置
 */
export interface DeltaStorageConfig {
  /** 是否启用增量存储 */
  enabled: boolean;
  /** 基线检查点间隔（每N个检查点创建一个基线） */
  baselineInterval: number;  // 默认: 10
  /** 最大增量链长度（超过则创建新基线） */
  maxDeltaChainLength: number;  // 默认: 20
  /** 是否自动压缩旧检查点 */
  autoCompact: boolean;  // 默认: true
  /** 压缩阈值（增量链长度） */
  compactThreshold: number;  // 默认: 15
}
```

---

### 2.4 方案三：共享引用机制

#### 2.4.1 设计思路

通过引用共享避免重复存储相同数据：

```
┌─────────────────────────────────────────────────────────────┐
│                     共享引用存储架构                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  检查点1 ─────┐                                             │
│              │                                              │
│  检查点2 ─────┼──────► 共享数据池                            │
│              │         ├── 消息块1 (ref: msg_block_001)      │
│  检查点3 ─────┘         ├── 消息块2 (ref: msg_block_002)      │
│                        ├── 变量集1 (ref: var_set_001)        │
│                        └── 节点结果集1 (ref: result_set_001)  │
│                                                             │
│  每个检查点只存储：                                           │
│  ├── 引用列表                                                │
│  └── 新增/变更数据                                           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 2.4.2 类型定义

```typescript
/**
 * 共享数据块类型
 */
export enum SharedDataType {
  MESSAGE_BLOCK = 'MESSAGE_BLOCK',
  VARIABLE_SET = 'VARIABLE_SET',
  NODE_RESULT_SET = 'NODE_RESULT_SET',
  CONVERSATION_STATE = 'CONVERSATION_STATE'
}

/**
 * 共享数据块
 */
export interface SharedDataBlock {
  /** 数据块ID */
  id: string;
  /** 数据块类型 */
  type: SharedDataType;
  /** 数据内容 */
  data: any;
  /** 引用计数 */
  refCount: number;
  /** 创建时间 */
  createdAt: Timestamp;
  /** 数据哈希（用于去重） */
  hash: string;
  /** 数据大小（字节） */
  size: number;
}

/**
 * 引用型检查点
 */
export interface ReferencedCheckpoint {
  /** 检查点ID */
  id: ID;
  /** 线程ID */
  threadId: ID;
  /** 工作流ID */
  workflowId: ID;
  /** 时间戳 */
  timestamp: Timestamp;
  /** 消息块引用 */
  messageBlockRefs: string[];
  /** 变量集引用 */
  variableSetRef?: string;
  /** 节点结果集引用 */
  nodeResultSetRef?: string;
  /** 增量数据（新增/变更） */
  delta?: CheckpointDelta;
  /** 元数据 */
  metadata?: CheckpointMetadata;
}
```

#### 2.4.3 共享数据管理器

```typescript
/**
 * 共享数据管理器
 */
export class SharedDataManager {
  private dataStore: Map<string, SharedDataBlock> = new Map();
  private hashIndex: Map<string, string> = new Map(); // hash -> blockId

  /**
   * 存储数据块（自动去重）
   */
  store(type: SharedDataType, data: any): string {
    // 1. 计算数据哈希
    const hash = this.calculateHash(data);

    // 2. 检查是否已存在相同数据
    const existingId = this.hashIndex.get(hash);
    if (existingId) {
      const block = this.dataStore.get(existingId)!;
      block.refCount++;
      return existingId;
    }

    // 3. 创建新数据块
    const blockId = generateId();
    const block: SharedDataBlock = {
      id: blockId,
      type,
      data,
      refCount: 1,
      createdAt: now(),
      hash,
      size: this.calculateSize(data)
    };

    this.dataStore.set(blockId, block);
    this.hashIndex.set(hash, blockId);

    return blockId;
  }

  /**
   * 获取数据块
   */
  get(blockId: string): SharedDataBlock | undefined {
    return this.dataStore.get(blockId);
  }

  /**
   * 释放数据块引用
   */
  release(blockId: string): boolean {
    const block = this.dataStore.get(blockId);
    if (!block) return false;

    block.refCount--;
    if (block.refCount <= 0) {
      this.dataStore.delete(blockId);
      this.hashIndex.delete(block.hash);
      return true; // 数据块已删除
    }
    return false;
  }

  /**
   * 计算数据哈希
   */
  private calculateHash(data: any): string {
    const json = JSON.stringify(data);
    // 简单哈希实现，实际可使用crypto.subtle.digest
    let hash = 0;
    for (let i = 0; i < json.length; i++) {
      const char = json.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash;
    }
    return hash.toString(16);
  }

  /**
   * 计算数据大小
   */
  private calculateSize(data: any): number {
    return new TextEncoder().encode(JSON.stringify(data)).length;
  }

  /**
   * 获取统计信息
   */
  getStats(): {
    totalBlocks: number;
    totalSize: number;
    totalRefs: number;
    byType: Record<SharedDataType, { count: number; size: number }>;
  } {
    let totalSize = 0;
    let totalRefs = 0;
    const byType: Record<SharedDataType, { count: number; size: number }> = {
      [SharedDataType.MESSAGE_BLOCK]: { count: 0, size: 0 },
      [SharedDataType.VARIABLE_SET]: { count: 0, size: 0 },
      [SharedDataType.NODE_RESULT_SET]: { count: 0, size: 0 },
      [SharedDataType.CONVERSATION_STATE]: { count: 0, size: 0 }
    };

    for (const block of this.dataStore.values()) {
      totalSize += block.size;
      totalRefs += block.refCount;
      byType[block.type].count++;
      byType[block.type].size += block.size;
    }

    return {
      totalBlocks: this.dataStore.size,
      totalSize,
      totalRefs,
      byType
    };
  }
}
```

#### 2.4.4 引用型检查点创建器

```typescript
/**
 * 引用型检查点创建器
 */
export class ReferencedCheckpointCreator {
  private sharedDataManager: SharedDataManager;
  private messageBlockSize: number;  // 每个消息块包含的消息数量

  constructor(sharedDataManager: SharedDataManager, messageBlockSize: number = 10) {
    this.sharedDataManager = sharedDataManager;
    this.messageBlockSize = messageBlockSize;
  }

  /**
   * 创建引用型检查点
   */
  create(
    threadId: string,
    workflowId: string,
    state: ThreadStateSnapshot,
    previousCheckpoint?: ReferencedCheckpoint
  ): ReferencedCheckpoint {
    const checkpoint: ReferencedCheckpoint = {
      id: generateId(),
      threadId,
      workflowId,
      timestamp: now(),
      messageBlockRefs: [],
      metadata: undefined
    };

    // 1. 处理消息：分块存储
    const messages = state.conversationState.messages;
    checkpoint.messageBlockRefs = this.storeMessageBlocks(messages);

    // 2. 处理变量集
    if (state.variables.length > 0) {
      checkpoint.variableSetRef = this.sharedDataManager.store(
        SharedDataType.VARIABLE_SET,
        state.variables
      );
    }

    // 3. 处理节点结果集
    if (Object.keys(state.nodeResults).length > 0) {
      checkpoint.nodeResultSetRef = this.sharedDataManager.store(
        SharedDataType.NODE_RESULT_SET,
        state.nodeResults
      );
    }

    return checkpoint;
  }

  /**
   * 分块存储消息
   */
  private storeMessageBlocks(messages: LLMMessage[]): string[] {
    const refs: string[] = [];

    for (let i = 0; i < messages.length; i += this.messageBlockSize) {
      const block = messages.slice(i, i + this.messageBlockSize);
      const ref = this.sharedDataManager.store(
        SharedDataType.MESSAGE_BLOCK,
        {
          messages: block,
          startIndex: i,
          endIndex: Math.min(i + this.messageBlockSize, messages.length)
        }
      );
      refs.push(ref);
    }

    return refs;
  }

  /**
   * 恢复检查点状态
   */
  restore(checkpoint: ReferencedCheckpoint): ThreadStateSnapshot {
    // 1. 恢复消息
    const messages: LLMMessage[] = [];
    for (const ref of checkpoint.messageBlockRefs) {
      const block = this.sharedDataManager.get(ref);
      if (block) {
        messages.push(...block.data.messages);
      }
    }

    // 2. 恢复变量
    let variables: any[] = [];
    if (checkpoint.variableSetRef) {
      const block = this.sharedDataManager.get(checkpoint.variableSetRef);
      if (block) {
        variables = block.data;
      }
    }

    // 3. 恢复节点结果
    let nodeResults: Record<string, any> = {};
    if (checkpoint.nodeResultSetRef) {
      const block = this.sharedDataManager.get(checkpoint.nodeResultSetRef);
      if (block) {
        nodeResults = block.data;
      }
    }

    return {
      status: 'PAUSED',
      currentNodeId: '',
      variables,
      variableScopes: { global: {}, thread: {}, local: [], loop: [] },
      input: {},
      output: {},
      nodeResults,
      errors: [],
      conversationState: {
        messages,
        markMap: { currentBatch: 0, batchBoundaries: [0], originalIndices: [], boundaryToBatch: [] },
        tokenUsage: null,
        currentRequestUsage: null
      }
    };
  }
}
```

---

## 三、整合方案

### 3.1 统一架构

```typescript
  /**
   * 创建优化检查点
   */
  async createCheckpoint(
    threadId: string,
    dependencies: CheckpointDependencies,
    metadata?: CheckpointMetadata
  ): Promise<string> {
    // 1. 获取线程状态
    const threadEntity = dependencies.threadRegistry.get(threadId);
    if (!threadEntity) {
      throw new ThreadContextNotFoundError('Thread not found', threadId);
    }

    const currentState = this.extractState(threadEntity);

    // 2. 获取上一个检查点
    const previousCheckpoints = await this.checkpointStateManager.list({ threadId });
    const previousCheckpoint = previousCheckpoints.length > 0
      ? await this.checkpointStateManager.get(previousCheckpoints[0])
      : null;

    // 3. 决定检查点类型
    const checkpointType = this.determineCheckpointType(previousCheckpoints.length);

    // 4. 创建检查点
    if (checkpointType === CheckpointType.FULL) {
      return this.createFullCheckpoint(threadId, currentState, metadata);
    } else {
      return this.createDeltaCheckpoint(
        threadId,
        currentState,
        previousCheckpoint?.threadState,
        metadata
      );
    }
  }

  /**
   * 决定检查点类型
   */
  private determineCheckpointType(checkpointCount: number): CheckpointType {
    if (!this.config.delta.enabled) {
      return CheckpointType.FULL;
    }

    if (checkpointCount === 0) {
      return CheckpointType.FULL;
    }

    if (checkpointCount % this.config.delta.baselineInterval === 0) {
      return CheckpointType.FULL;
    }

    return CheckpointType.DELTA;
  }

  /**
   * 创建完整检查点
   */
  private async createFullCheckpoint(
    threadId: string,
    state: ThreadStateSnapshot,
    metadata?: CheckpointMetadata
  ): Promise<string> {
    // 应用消息压缩
    const compressedState = this.applyCompression(state);

    // 使用共享引用存储
    const referencedCheckpoint = this.createReferencedCheckpoint(
      threadId,
      compressedState
    );

    return this.checkpointStateManager.create({
      id: generateId(),
      threadId,
      workflowId: referencedCheckpoint.workflowId,
      timestamp: now(),
      type: CheckpointType.FULL,
      fullState: compressedState,
      metadata
    } as Checkpoint);
  }

  /**
   * 创建增量检查点
   */
  private async createDeltaCheckpoint(
    threadId: string,
    currentState: ThreadStateSnapshot,
    previousState?: ThreadStateSnapshot,
    metadata?: CheckpointMetadata
  ): Promise<string> {
    if (!previousState) {
      return this.createFullCheckpoint(threadId, currentState, metadata);
    }

    // 计算差异
    const delta = this.diffCalculator.calculateDelta(previousState, currentState);

    return this.checkpointStateManager.create({
      id: generateId(),
      threadId,
      workflowId: '',
      timestamp: now(),
      type: CheckpointType.DELTA,
      delta,
      metadata
    } as Checkpoint);
  }

  /**
   * 应用消息压缩
   */
  private applyCompression(state: ThreadStateSnapshot): ThreadStateSnapshot {
    if (!this.config.compression.enabled) {
      return state;
    }

    const compressedConversation = this.messageCompressor.compress(
      state.conversationState.messages
    );

    return {
      ...state,
      conversationState: {
        ...state.conversationState,
        messages: compressedConversation.recentMessages,
        compressedBlocks: compressedConversation.compressedBlocks,
        archivedRefs: compressedConversation.archivedRefs
      } as any
    };
  }
```

### 3.2 存储效果预估

| 场景 | 原始存储 | 优化后存储 | 节省比例 |
|-----|---------|-----------|---------|
| 10个检查点，每检查点100条消息 | 10 × 100KB = 1MB | 100KB + 9 × 10KB = 190KB | 81% |
| 100个检查点，消息持续增长 | 100 × 平均150KB = 15MB | 约2MB | 87% |
| Fork子线程检查点 | 完整拷贝 | 引用共享 | 90%+ |

---

## 四、实施计划

### 4.1 阶段一：增量存储（优先级：高）

**目标**：实现基础增量存储能力

**任务清单**：
1. 定义增量检查点类型
2. 实现差异计算器
3. 实现增量恢复器
4. 修改CheckpointCoordinator支持增量模式
5. 添加配置选项

**涉及文件**：
- `packages/types/src/checkpoint/checkpoint.ts` - 新增类型
- `sdk/core/execution/utils/checkpoint-diff-calculator.ts` - 新建
- `sdk/core/execution/utils/checkpoint-delta-restorer.ts` - 新建
- `sdk/core/execution/coordinators/checkpoint-coordinator.ts` - 修改

### 4.3 阶段二：共享引用（优先级：中）

**目标**：避免跨检查点重复存储

**任务清单**：
1. 定义共享数据块类型
2. 实现共享数据管理器
3. 实现引用型检查点创建器
4. 实现引用计数和垃圾回收
5. 集成到存储层

**涉及文件**：
- `sdk/core/execution/utils/shared-data-manager.ts` - 新建
- `sdk/core/execution/utils/referenced-checkpoint-creator.ts` - 新建
- `sdk/core/storage/checkpoint-storage-callback.ts` - 修改

---

## 五、风险与缓解措施

| 风险 | 影响 | 缓解措施 |
|-----|------|---------|
| 增量恢复链过长导致恢复慢 | 恢复时间增加 | 限制最大链长，定期创建基线 |
| 共享引用管理复杂 | 内存泄漏风险 | 实现引用计数和自动垃圾回收(注意不要引入反模式，例如不要试图直接gc) |
| 向后兼容性 | 旧检查点无法恢复 | 实现版本检测和迁移逻辑 |

---

## 六、总结

本设计方案通过三个核心策略优化检查点存储：

1. **增量存储**：只存储差异，减少重复数据
2. **消息压缩**：对历史消息进行摘要压缩
3. **共享引用**：跨检查点共享不变数据

预期可将存储开销降低**70-90%**，同时保持完整的恢复能力。

建议优先实施增量存储策略，可快速获得显著收益，后续逐步引入消息压缩和共享引用机制。
