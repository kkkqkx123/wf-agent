# 消息数组重构设计方案

## 1. 设计原则

### 1.1 核心目标
1. **消息管理独立化**：将消息管理从 LLM 类型定义中完全分离，建立独立的类型定义模块
2. **操作开销优化**：通过批次快照机制减少操作开销，避免维护过度延长的数组
3. **操作语义清晰**：明确区分不同操作类型的批次行为，删除 createNewBatch 配置
4. **绝对统一来源**：所有消息操作类型定义集中在单一模块，确保一致性

### 1.2 关键设计决策
- **删除 createNewBatch**：消息数组自身逻辑保证不丢失信息，无需手动控制批次创建
- **引入 APPEND 操作**：专门用于尾插，不创建新批次
- **INSERT 仅支持中间插入**：删除 -1 位置选项，始终创建新批次
- **批次快照机制**：通过快照数组存储历史状态，避免数组无限增长

## 2. 独立类型定义模块

### 2.1 模块结构
```
packages/types/src/
├── message/                    # 新增：消息管理独立模块
│   ├── index.ts               # 导出所有类型
│   ├── message.ts             # 消息基础类型
│   ├── message-array.ts       # 消息数组类型
│   ├── message-operations.ts  # 消息操作类型
│   └── batch-snapshot.ts      # 批次快照类型
└── llm.ts                     # LLM 类型（移除消息操作相关定义）
```

### 2.2 消息基础类型（message.ts）

```typescript
/**
 * 消息角色
 */
export type MessageRole = 'system' | 'user' | 'assistant' | 'tool';

/**
 * 消息内容类型
 */
export type MessageContent = string | Array<{
  type: 'text' | 'image' | 'tool_use' | 'tool_result';
  text?: string;
  image_url?: { url: string };
  tool_use?: {
    id: string;
    name: string;
    input: Record<string, any>;
  };
  tool_result?: {
    tool_use_id: string;
    content: string | Array<{ type: string; text: string }>;
  };
}>;

/**
 * 消息基础接口
 */
export interface Message {
  /** 消息角色 */
  role: MessageRole;
  /** 消息内容 */
  content: MessageContent;
  /** 消息ID（可选） */
  id?: string;
  /** 消息时间戳（可选） */
  timestamp?: number;
  /** 其他元数据 */
  metadata?: Record<string, any>;
}
```

### 2.3 消息数组类型（message-array.ts）

```typescript
import type { Message } from './message';
import type { BatchSnapshot } from './batch-snapshot';

/**
 * 消息数组状态
 */
export interface MessageArrayState {
  /** 完整的消息数组（包含所有批次） */
  messages: Message[];
  /** 批次快照数组 */
  batchSnapshots: BatchSnapshot[];
  /** 当前批次索引 */
  currentBatchIndex: number;
  /** 消息总数 */
  totalMessageCount: number;
}

/**
 * 消息数组统计信息
 */
export interface MessageArrayStats {
  /** 总消息数 */
  totalMessages: number;
  /** 当前批次消息数 */
  currentBatchMessages: number;
  /** 批次总数 */
  totalBatches: number;
  /** 当前批次索引 */
  currentBatchIndex: number;
}
```

### 2.4 消息操作类型（message-operations.ts）

```typescript
import type { Message } from './message';

/**
 * 消息操作类型
 */
export type MessageOperationType = 
  | 'APPEND'      // 尾插消息（不创建新批次）
  | 'INSERT'      // 中间插入消息（创建新批次）
  | 'REPLACE'     // 替换消息（创建新批次）
  | 'TRUNCATE'    // 截断消息（创建新批次）
  | 'CLEAR'       // 清空消息（创建新批次，快照为空）
  | 'FILTER'      // 过滤消息（创建新批次）
  | 'ROLLBACK';   // 回退到指定批次

/**
 * 消息操作配置基础接口
 */
export interface MessageOperationConfig {
  /** 操作类型 */
  operation: MessageOperationType;
}

/**
 * APPEND 操作配置
 */
export interface AppendMessageOperation extends MessageOperationConfig {
  operation: 'APPEND';
  /** 要追加的消息数组 */
  messages: Message[];
}

/**
 * INSERT 操作配置
 */
export interface InsertMessageOperation extends MessageOperationConfig {
  operation: 'INSERT';
  /** 插入位置（相对于当前批次，0 <= position <= currentBatchMessages.length） */
  position: number;
  /** 要插入的消息数组 */
  messages: Message[];
}

/**
 * REPLACE 操作配置
 */
export interface ReplaceMessageOperation extends MessageOperationConfig {
  operation: 'REPLACE';
  /** 要替换的消息索引（相对于当前批次） */
  index: number;
  /** 新的消息内容 */
  message: Message;
}

/**
 * TRUNCATE 操作配置
 */
export interface TruncateMessageOperation extends MessageOperationConfig {
  operation: 'TRUNCATE';
  /** 保留前N条消息 */
  keepFirst?: number;
  /** 保留后N条消息 */
  keepLast?: number;
  /** 删除前N条消息 */
  removeFirst?: number;
  /** 删除后N条消息 */
  removeLast?: number;
  /** 保留消息的索引范围 [start, end) */
  range?: { start: number; end: number };
  /** 按角色过滤后再截断 */
  role?: Message['role'];
}

/**
 * CLEAR 操作配置
 */
export interface ClearMessageOperation extends MessageOperationConfig {
  operation: 'CLEAR';
  /** 是否保留系统消息 */
  keepSystemMessage?: boolean;
}

/**
 * FILTER 操作配置
 */
export interface FilterMessageOperation extends MessageOperationConfig {
  operation: 'FILTER';
  /** 按角色过滤 */
  roles?: Message['role'][];
  /** 按内容关键词过滤（包含指定关键词的消息） */
  contentContains?: string[];
  /** 按内容关键词排除（不包含指定关键词的消息） */
  contentExcludes?: string[];
}

/**
 * ROLLBACK 操作配置
 */
export interface RollbackMessageOperation extends MessageOperationConfig {
  operation: 'ROLLBACK';
  /** 目标批次索引 */
  targetBatchIndex: number;
}

/**
 * 消息操作结果
 */
export interface MessageOperationResult {
  /** 操作后的消息数组状态 */
  state: MessageArrayState;
  /** 操作影响的批次索引 */
  affectedBatchIndex: number;
  /** 操作统计信息 */
  stats: MessageArrayStats;
}
```

### 2.5 批次快照类型（batch-snapshot.ts）

```typescript
import type { Message } from './message';

/**
 * 批次快照
 * 存储批次创建时的完整消息数组状态
 */
export interface BatchSnapshot {
  /** 批次索引 */
  batchIndex: number;
  /** 批次创建时间戳 */
  timestamp: number;
  /** 批次消息数组的深拷贝（如果为空数组表示无额外拷贝开销） */
  messages: Message[];
  /** 批次消息数量 */
  messageCount: number;
  /** 批次描述 */
  description?: string;
}

/**
 * 批次快照数组
 */
export type BatchSnapshotArray = BatchSnapshot[];
```

## 3. 操作行为规范

### 3.1 批次创建规则

| 操作类型 | 是否创建新批次 | 快照内容 | 说明 |
|---------|--------------|---------|------|
| APPEND | 否 | 无 | 尾插操作，不创建新批次 |
| INSERT | 是 | 完整消息数组深拷贝 | 中间插入，需要创建新批次 |
| REPLACE | 是 | 完整消息数组深拷贝 | 替换操作，需要创建新批次 |
| TRUNCATE | 是 | 完整消息数组深拷贝 | 截断操作，需要创建新批次 |
| CLEAR | 是 | 空数组 | 清空操作，无额外拷贝开销 |
| FILTER | 是 | 完整消息数组深拷贝 | 过滤操作，需要创建新批次 |
| ROLLBACK | 否 | 无 | 回退操作，不创建新批次 |

### 3.2 操作开销分析

#### 3.2.1 APPEND 操作（低开销）
```typescript
// 操作流程
function executeAppend(state: MessageArrayState, operation: AppendMessageOperation): MessageOperationResult {
  // 1. 直接追加消息到当前批次
  const newMessages = [...state.messages, ...operation.messages];
  
  // 2. 不创建新批次，不创建快照
  const newState: MessageArrayState = {
    messages: newMessages,
    batchSnapshots: state.batchSnapshots,
    currentBatchIndex: state.currentBatchIndex,
    totalMessageCount: newMessages.length
  };
  
  return {
    state: newState,
    affectedBatchIndex: state.currentBatchIndex,
    stats: calculateStats(newState)
  };
}
```

#### 3.2.2 INSERT 操作（高开销）
```typescript
// 操作流程
function executeInsert(state: MessageArrayState, operation: InsertMessageOperation): MessageOperationResult {
  // 1. 创建当前批次的快照（深拷贝）
  const snapshot: BatchSnapshot = {
    batchIndex: state.currentBatchIndex,
    timestamp: Date.now(),
    messages: JSON.parse(JSON.stringify(state.messages)), // 深拷贝
    messageCount: state.messages.length,
    description: `Before INSERT at position ${operation.position}`
  };
  
  // 2. 执行插入操作
  const newMessages = [...state.messages];
  newMessages.splice(operation.position, 0, ...operation.messages);
  
  // 3. 创建新批次
  const newBatchIndex = state.currentBatchIndex + 1;
  const newState: MessageArrayState = {
    messages: newMessages,
    batchSnapshots: [...state.batchSnapshots, snapshot],
    currentBatchIndex: newBatchIndex,
    totalMessageCount: newMessages.length
  };
  
  return {
    state: newState,
    affectedBatchIndex: newBatchIndex,
    stats: calculateStats(newState)
  };
}
```

#### 3.2.3 CLEAR 操作（低开销）
```typescript
// 操作流程
function executeClear(state: MessageArrayState, operation: ClearMessageOperation): MessageOperationResult {
  // 1. 创建空快照（无额外拷贝开销）
  const snapshot: BatchSnapshot = {
    batchIndex: state.currentBatchIndex,
    timestamp: Date.now(),
    messages: [], // 空数组，无拷贝开销
    messageCount: 0,
    description: 'Before CLEAR'
  };
  
  // 2. 执行清空操作
  const keepSystemMessage = operation.keepSystemMessage ?? true;
  const newMessages = keepSystemMessage 
    ? state.messages.filter(msg => msg.role === 'system')
    : [];
  
  // 3. 创建新批次
  const newBatchIndex = state.currentBatchIndex + 1;
  const newState: MessageArrayState = {
    messages: newMessages,
    batchSnapshots: [...state.batchSnapshots, snapshot],
    currentBatchIndex: newBatchIndex,
    totalMessageCount: newMessages.length
  };
  
  return {
    state: newState,
    affectedBatchIndex: newBatchIndex,
    stats: calculateStats(newState)
  };
}
```

## 4. 消息数组管理器

### 4.1 核心接口

```typescript
import type { 
  MessageArrayState, 
  MessageOperationConfig, 
  MessageOperationResult,
  MessageArrayStats
} from './message-array';

/**
 * 消息数组管理器接口
 */
export interface MessageArrayManager {
  /**
   * 执行消息操作
   * @param operation 操作配置
   * @returns 操作结果
   */
  execute(operation: MessageOperationConfig): MessageOperationResult;
  
  /**
   * 获取当前消息数组状态
   * @returns 消息数组状态
   */
  getState(): MessageArrayState;
  
  /**
   * 获取当前批次的消息
   * @returns 当前批次的消息数组
   */
  getCurrentMessages(): Message[];
  
  /**
   * 获取统计信息
   * @returns 统计信息
   */
  getStats(): MessageArrayStats;
  
  /**
   * 回退到指定批次
   * @param batchIndex 批次索引
   * @returns 操作结果
   */
  rollback(batchIndex: number): MessageOperationResult;
  
  /**
   * 获取批次快照
   * @param batchIndex 批次索引
   * @returns 批次快照
   */
  getBatchSnapshot(batchIndex: number): BatchSnapshot | null;
}
```

### 4.2 实现示例

```typescript
export class MessageArrayManagerImpl implements MessageArrayManager {
  private state: MessageArrayState;
  
  constructor(initialMessages: Message[] = []) {
    this.state = {
      messages: initialMessages,
      batchSnapshots: [],
      currentBatchIndex: 0,
      totalMessageCount: initialMessages.length
    };
  }
  
  execute(operation: MessageOperationConfig): MessageOperationResult {
    switch (operation.operation) {
      case 'APPEND':
        return this.executeAppend(operation as AppendMessageOperation);
      case 'INSERT':
        return this.executeInsert(operation as InsertMessageOperation);
      case 'REPLACE':
        return this.executeReplace(operation as ReplaceMessageOperation);
      case 'TRUNCATE':
        return this.executeTruncate(operation as TruncateMessageOperation);
      case 'CLEAR':
        return this.executeClear(operation as ClearMessageOperation);
      case 'FILTER':
        return this.executeFilter(operation as FilterMessageOperation);
      case 'ROLLBACK':
        return this.executeRollback(operation as RollbackMessageOperation);
      default:
        throw new Error(`Unsupported operation type: ${operation.operation}`);
    }
  }
  
  // ... 其他方法实现
}
```

## 5. 迁移计划

### 5.1 第一阶段：创建独立类型模块
- 创建 `packages/types/src/message/` 目录
- 实现消息基础类型（message.ts）
- 实现消息数组类型（message-array.ts）
- 实现消息操作类型（message-operations.ts）
- 实现批次快照类型（batch-snapshot.ts）
- 创建统一导出文件（index.ts）

### 5.2 第二阶段：重构现有代码
- 从 `packages/types/src/llm.ts` 中移除消息操作相关定义
- 更新 `ContextProcessorNodeConfig` 使用新的操作类型
- 删除 `createNewBatch` 配置选项
- 更新 `ContinueFromTriggerNodeConfig` 使用新的操作类型

### 5.3 第三阶段：实现消息数组管理器
- 实现 `MessageArrayManager` 接口
- 实现各种操作的处理逻辑
- 实现批次快照机制
- 实现回退功能

### 5.4 第四阶段：集成测试
- 编写单元测试验证各种操作
- 性能基准测试对比优化前后
- 集成测试验证端到端工作流

## 6. 预期收益

1. **类型定义统一**：所有消息操作类型集中在单一模块，确保一致性
2. **操作开销优化**：通过批次快照机制，避免维护过度延长的数组
3. **操作语义清晰**：明确区分不同操作类型的批次行为
4. **内存效率提升**：APPEND 和 CLEAR 操作无额外拷贝开销
5. **可维护性增强**：消息管理独立于 LLM，职责清晰
6. **向后兼容**：通过适配器模式保持现有 API 兼容性