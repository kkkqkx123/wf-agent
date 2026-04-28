# MessageHistory Batch Memory Optimization Design

## 一、现状分析

### 1.1 当前消息历史管理机制

当前项目使用 `MessageHistory` 类管理 LLM 对话消息，核心机制如下：

```typescript
// sdk/core/messages/message-history.ts
class MessageHistory {
  protected messages: LLMMessage[] = [];  // 所有消息存储在内存中
  protected markMap: MessageMarkMap;       // 批次标记映射
}

interface MessageMarkMap {
  originalIndices: number[];    // 原始索引映射
  batchBoundaries: number[];    // 批次边界索引
  boundaryToBatch: number[];    // 边界到批次的映射
  currentBatch: number;         // 当前批次号
}
```

### 1.2 Batch 可见性控制机制

通过 `startNewBatch()` 方法创建新批次，历史批次消息变为"不可见"但仍保留在内存中：

```typescript
// 启动新批次
startNewBatch(boundaryIndex?: number): number {
  const index = boundaryIndex ?? this.messages.length;
  this.markMap = startNewBatch(this.markMap, index);
  return this.markMap.currentBatch;
}

// 获取可见消息（当前批次边界之后的消息）
getMessages(): LLMMessage[] {
  return getVisibleMessages(this.messages, this.markMap);
}

// 获取不可见消息（历史批次的消息）
getInvisibleMessages(): LLMMessage[] {
  return getInvisibleMessages(this.messages, this.markMap);
}
```

### 1.3 存在的问题

#### 问题1：内存持续增长

```typescript
// 场景：长对话，每10轮启动一个新批次
for (let i = 0; i < 100; i++) {
  messageHistory.addUserMessage(`Message ${i}`);
  messageHistory.addAssistantMessage(`Response ${i}`);
  
  if (i % 10 === 0) {
    messageHistory.startNewBatch();  // 历史消息仍保留在内存
  }
}
// 结果：内存中保留了所有100轮对话的消息
```

#### 问题2：Checkpoint 冗余存储

```typescript
// sdk/graph/execution/coordinators/checkpoint-coordinator.ts
private static extractThreadState(...) {
  const conversationState = conversationManager
    ? {
        messages: conversationManager.getAllMessages(),  // 保存所有消息
        markMap: conversationManager.getMarkMap(),
        // ...
      }
    : { /* ... */ };
}
```

每次创建 checkpoint 都会保存完整的消息历史，即使历史批次的消息在当前上下文中不可见。

## 二、优化方案设计

### 2.1 核心思路

采用**延迟加载（Lazy Loading）**模式：
1. `startNewBatch()` 时将当前批次消息保存到 checkpoint
2. 清空内存中的不可见消息，只保留引用（checkpoint ID）
3. 需要访问历史消息时，从 checkpoint 加载

### 2.2 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                    MessageHistory (内存)                     │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐      ┌──────────────────────────────┐ │
│  │ 当前批次消息    │      │ 历史批次消息引用 (checkpoint) │ │
│  │ (visible)       │      │ (invisible, 可从存储恢复)     │ │
│  └─────────────────┘      └──────────────────────────────┘ │
│           │                              │                  │
│           ▼                              ▼                  │
│    getMessages()              getMessagesFromCheckpoint()   │
└─────────────────────────────────────────────────────────────┘
           │                              │
           ▼                              ▼
┌─────────────────────────────────────────────────────────────┐
│              Checkpoint Storage (SQLite)                     │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  checkpoint_metadata                                │   │
│  │  - id, thread_id, timestamp, message_count...       │   │
│  ├─────────────────────────────────────────────────────┤   │
│  │  checkpoint_blob                                    │   │
│  │  - checkpoint_id, blob_data (compressed)            │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 2.3 数据结构修改

#### 2.3.1 扩展 MessageMarkMap

```typescript
// packages/types/src/messages/message.types.ts
interface MessageMarkMap {
  originalIndices: number[];
  batchBoundaries: number[];
  boundaryToBatch: number[];
  currentBatch: number;
  
  // 新增：批次到 checkpoint 的映射
  batchToCheckpoint?: Array<{
    batchId: number;
    checkpointId: string | null;  // null 表示消息仍在内存中
    messageCount: number;
  }>;
  
  // 新增：内存中保留的消息范围
  memoryRange?: {
    startBatch: number;
    endBatch: number;
  };
}
```

#### 2.3.2 新增 BatchCheckpointInfo 类型

```typescript
// packages/types/src/messages/message.types.ts
interface BatchCheckpointInfo {
  batchId: number;
  checkpointId: string;
  boundaryIndex: number;
  messageCount: number;
  timestamp: number;
}
```

### 2.4 核心方法设计

#### 2.4.1 带 Checkpoint 的新批次启动

```typescript
// sdk/core/messages/message-history.ts
class MessageHistory {
  /**
   * 启动新批次并将历史消息保存到 checkpoint
   * @param checkpointStorage Checkpoint 存储接口
   * @param boundaryIndex 边界索引（可选）
   * @returns 新批次号
   */
  async startNewBatchWithCheckpoint(
    checkpointStorage: CheckpointStorageCallback,
    boundaryIndex?: number
  ): Promise<number>;
  
  /**
   * 从 checkpoint 加载指定批次的消息
   * @param batchId 批次 ID
   * @param checkpointStorage Checkpoint 存储接口
   * @returns 该批次的消息数组
   */
  async loadBatchFromCheckpoint(
    batchId: number,
    checkpointStorage: CheckpointStorageCallback
  ): Promise<LLMMessage[]>;
  
  /**
   * 获取所有消息（包括从 checkpoint 恢复的）
   * @param checkpointStorage Checkpoint 存储接口
   * @returns 完整的消息数组
   */
  async getAllMessagesWithRestore(
    checkpointStorage: CheckpointStorageCallback
  ): Promise<LLMMessage[]>;
}
```

#### 2.4.2 批次管理工具函数扩展

```typescript
// sdk/core/utils/messages/batch-management-utils.ts

/**
 * 启动新批次并创建 checkpoint 映射
 */
export function startNewBatchWithCheckpoint(
  markMap: MessageMarkMap,
  boundaryIndex: number,
  checkpointId?: string
): MessageMarkMap;

/**
 * 获取需要从内存释放的批次
 */
export function getBatchesToRelease(
  markMap: MessageMarkMap,
  keepInMemory: number  // 保留在内存中的批次数量
): number[];

/**
 * 重建消息数组（合并内存中的消息和 checkpoint 引用）
 */
export function rebuildMessagesWithCheckpoint(
  messages: LLMMessage[],
  markMap: MessageMarkMap,
  loadedBatches: Map<number, LLMMessage[]>
): LLMMessage[];
```

### 2.5 集成到现有系统

#### 2.5.1 ConversationManager 集成

```typescript
// sdk/core/managers/conversation-manager.ts
class ConversationManager extends MessageHistory {
  private checkpointStorage?: CheckpointStorageCallback;
  
  /**
   * 设置 checkpoint 存储接口
   */
  setCheckpointStorage(storage: CheckpointStorageCallback): void {
    this.checkpointStorage = storage;
  }
  
  /**
   * 启动新批次（支持自动 checkpoint）
   */
  async startNewBatchWithAutoCheckpoint(boundaryIndex?: number): Promise<number> {
    if (!this.checkpointStorage) {
      // 没有配置 storage，使用普通模式
      return this.startNewBatch(boundaryIndex);
    }
    
    return this.startNewBatchWithCheckpoint(this.checkpointStorage, boundaryIndex);
  }
}
```

#### 2.5.2 Graph MessageHistoryManager 集成

```typescript
// sdk/graph/execution/managers/message-history-manager.ts
class MessageHistoryManager extends ConversationManager {
  /**
   * 启动新批次并添加初始工具描述
   * 支持 checkpoint 自动保存
   */
  async startNewBatchWithInitialToolsAndCheckpoint(
    boundaryIndex?: number
  ): Promise<number> {
    // 1. 启动新批次并保存到 checkpoint
    const newBatch = await this.startNewBatchWithAutoCheckpoint(boundaryIndex);
    
    // 2. 添加工具描述（原有逻辑）
    if (!this.hasToolDescriptionMessage()) {
      const toolDescMessage = this.getInitialToolDescriptionMessage();
      if (toolDescMessage) {
        this.addMessage(toolDescMessage);
      }
    }
    
    return newBatch;
  }
}
```

## 三、实现步骤

### 3.1 第一阶段：基础类型定义

1. 修改 `packages/types/src/messages/message.types.ts`
   - 扩展 `MessageMarkMap` 接口
   - 新增 `BatchCheckpointInfo` 接口

### 3.2 第二阶段：批次管理工具函数

1. 修改 `sdk/core/utils/messages/batch-management-utils.ts`
   - 添加 `startNewBatchWithCheckpoint`
   - 添加 `getBatchesToRelease`
   - 添加 `rebuildMessagesWithCheckpoint`

### 3.3 第三阶段：MessageHistory 核心实现

1. 修改 `sdk/core/messages/message-history.ts`
   - 添加 `startNewBatchWithCheckpoint`
   - 添加 `loadBatchFromCheckpoint`
   - 添加 `getAllMessagesWithRestore`
   - 修改 `createSnapshot` 支持 checkpoint 引用

### 3.4 第四阶段：集成到上层管理器

1. 修改 `sdk/core/managers/conversation-manager.ts`
2. 修改 `sdk/graph/execution/managers/message-history-manager.ts`

### 3.5 第五阶段：测试和验证

1. 单元测试：批次管理和 checkpoint 恢复
2. 集成测试：长对话内存占用测试
3. 性能测试：checkpoint 加载性能

## 四、注意事项

### 4.1 向后兼容性

- `MessageMarkMap` 的新字段是可选的，不影响现有数据
- 提供降级方案：没有 checkpoint storage 时使用原有逻辑

### 4.2 性能考虑

- 添加缓存机制避免频繁 IO
- 支持异步批量加载多个批次
- checkpoint 压缩存储减少磁盘占用

### 4.3 错误处理

- checkpoint 加载失败时提供降级方案
- 保持内存中至少一个批次的消息作为安全备份
- 提供手动触发 checkpoint 的接口

## 五、预期收益

1. **内存占用降低**：长对话场景内存占用减少 60-80%
2. **启动速度提升**：无需加载完整历史消息即可启动
3. **可扩展性增强**：支持更长的对话历史
4. **与现有系统无缝集成**：复用现有 checkpoint 机制
