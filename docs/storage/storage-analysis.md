# 存储模块需求分析与大小估算

## 一、存储模块概览

### 1.1 存储模块架构

当前项目包含以下四种主要存储类型：

| 存储类型 | 主要用途 | 实现方式 |
|---------|---------|---------|
| Checkpoint Storage | 工作流执行状态快照 | JSON/SQLite/Memory |
| Thread Storage | 线程执行记录 | JSON/SQLite/Memory |
| Task Storage | 异步任务管理 | JSON/SQLite/Memory |
| Workflow Storage | 工作流定义持久化 | JSON/SQLite/Memory |

### 1.2 存储实现层次

```
packages/storage/
├── json/              # JSON文件存储实现
│   ├── json-checkpoint-storage.ts
│   ├── json-thread-storage.ts
│   ├── json-task-storage.ts
│   └── json-workflow-storage.ts
├── sqlite/            # SQLite数据库存储实现
│   ├── sqlite-checkpoint-storage.ts
│   ├── sqlite-thread-storage.ts
│   ├── sqlite-task-storage.ts
│   └── sqlite-workflow-storage.ts
└── memory/            # 内存存储实现（用于测试）
    ├── memory-checkpoint-storage.ts
    ├── memory-thread-storage.ts
    ├── memory-task-storage.ts
    └── memory-workflow-storage.ts
```

---

## 二、Checkpoint存储分析

### 2.1 数据结构

Checkpoint是最大的存储对象，包含完整的工作流执行状态：

```typescript
interface Checkpoint {
  id: ID;                          // 检查点ID (~36 bytes)
  threadId: ID;                    // 线程ID (~36 bytes)
  workflowId: ID;                  // 工作流ID (~36 bytes)
  timestamp: Timestamp;            // 时间戳 (~8 bytes)
  type?: CheckpointType;           // 类型 (~5 bytes)
  baseCheckpointId?: ID;           // 基线ID (~36 bytes)
  previousCheckpointId?: ID;       // 前一检查点ID (~36 bytes)
  delta?: CheckpointDelta;         // 增量数据 (可变)
  threadState?: ThreadStateSnapshot; // 完整状态快照 (可变，主要存储开销)
  metadata?: CheckpointMetadata;   // 元数据 (~100-500 bytes)
}
```

### 2.2 ThreadStateSnapshot详细分析

ThreadStateSnapshot是Checkpoint的核心数据，包含：

```typescript
interface ThreadStateSnapshot {
  status: ThreadStatus;            // 线程状态 (~10 bytes)
  currentNodeId: ID;               // 当前节点ID (~36 bytes)
  variables: any[];                // 变量数组 (可变，通常 1-10KB)
  variableScopes: VariableScopes;  // 变量作用域 (~500 bytes - 2KB)
  input: Record<string, any>;      // 输入数据 (可变，通常 1-5KB)
  output: Record<string, any>;     // 输出数据 (可变，通常 1-5KB)
  nodeResults: Record<string, NodeExecutionResult>; // 节点结果 (可变，通常 5-50KB)
  errors: any[];                   // 错误数组 (通常 < 1KB)
  conversationState: {             // 对话状态 (主要存储开销)
    messages: any[];               // 消息历史 (60-80% 的存储空间)
    markMap: MessageMarkMap;       // 消息标记 (~1KB)
    tokenUsage: TokenUsageStats;   // Token统计 (~100 bytes)
    currentRequestUsage: TokenUsageStats; // 当前请求统计 (~100 bytes)
  };
  toolApprovalState?: {...};       // 工具审批状态 (~500 bytes)
  triggerStates?: Map<ID, TriggerRuntimeState>; // 触发器状态 (可变)
  forkJoinContext?: {...};         // Fork/Join上下文 (~100 bytes)
  triggeredSubworkflowContext?: {...}; // 子工作流上下文 (~200 bytes)
}
```

### 2.3 消息数据结构分析

消息是最大的存储开销，每条消息的结构：

```typescript
interface Message {
  role: MessageRole;               // 角色 (~10 bytes)
  content: MessageContent;         // 内容 (可变，通常 100 bytes - 10KB)
  id?: string;                     // 消息ID (~36 bytes)
  timestamp?: number;              // 时间戳 (~8 bytes)
  metadata?: Record<string, any>;  // 元数据 (可变)
}

// MessageContent可以是：
type MessageContent = 
  | string                         // 简单文本 (100 bytes - 5KB)
  | Array<{                        // 复杂内容块
      type: "text" | "image_url" | "tool_use" | "tool_result" | "thinking";
      text?: string;               // 文本内容 (可变)
      citations?: TextCitation[];  // 引用列表 (可变)
      image_url?: { url: string }; // 图片URL (可变)
      tool_use?: {...};            // 工具调用 (可变)
      tool_result?: {...};         // 工具结果 (可变)
      thinking?: string;           // 思考内容 (可变)
    }>;
```

### 2.4 Checkpoint大小估算

#### 典型场景估算

| 场景 | 消息数量 | 平均消息大小 | 其他数据 | 总大小估算 |
|-----|---------|------------|---------|-----------|
| 简单对话（10轮） | 20条 | 500 bytes | 10KB | ~20KB |
| 中等复杂度（50轮） | 100条 | 800 bytes | 30KB | ~110KB |
| 复杂工作流（100轮） | 200条 | 1KB | 50KB | ~250KB |
| 长时间运行（500轮） | 1000条 | 1.2KB | 100KB | ~1.3MB |

#### 详细分解（以中等复杂度场景为例）

```
Checkpoint总大小: ~110KB
├── 基础字段: ~200 bytes
├── threadState:
│   ├── status, currentNodeId: ~50 bytes
│   ├── variables: ~5KB
│   ├── variableScopes: ~1KB
│   ├── input/output: ~5KB
│   ├── nodeResults: ~20KB
│   ├── errors: ~500 bytes
│   ├── conversationState:
│   │   ├── messages: ~80KB (100条 × 800 bytes)
│   │   ├── markMap: ~1KB
│   │   └── tokenUsage: ~200 bytes
│   └── 其他上下文: ~2KB
└── metadata: ~500 bytes
```

---

## 三、Thread存储分析

### 3.1 数据结构

```typescript
interface ThreadStorageMetadata {
  threadId: ID;                    // 线程ID (~36 bytes)
  workflowId: ID;                  // 工作流ID (~36 bytes)
  workflowVersion: Version;        // 版本号 (~10 bytes)
  status: ThreadStatus;            // 状态 (~10 bytes)
  threadType?: ThreadType;         // 类型 (~15 bytes)
  currentNodeId?: ID;              // 当前节点ID (~36 bytes)
  parentThreadId?: ID;             // 父线程ID (~36 bytes)
  startTime: Timestamp;            // 开始时间 (~8 bytes)
  endTime?: Timestamp;             // 结束时间 (~8 bytes)
  tags?: string[];                 // 标签数组 (~100-500 bytes)
  customFields?: Record<string, unknown>; // 自定义字段 (可变)
}
```

### 3.2 Thread大小估算

Thread存储主要是元数据，不包含实际执行状态：

| 场景 | 大小估算 |
|-----|---------|
| 基础Thread | ~200-300 bytes |
| 带标签和自定义字段 | ~500 bytes - 2KB |
| Fork子线程 | ~300 bytes |

**注意**：Thread存储本身很小，但每个Thread会关联多个Checkpoint，实际存储开销主要在Checkpoint。

---

## 四、Task存储分析

### 4.1 数据结构

```typescript
interface TaskStorageMetadata {
  taskId: ID;                      // 任务ID (~36 bytes)
  threadId: ID;                    // 线程ID (~36 bytes)
  workflowId: ID;                  // 工作流ID (~36 bytes)
  status: TaskStatus;              // 状态 (~10 bytes)
  submitTime: Timestamp;           // 提交时间 (~8 bytes)
  startTime?: Timestamp;           // 开始时间 (~8 bytes)
  completeTime?: Timestamp;        // 完成时间 (~8 bytes)
  timeout?: number;                // 超时时间 (~8 bytes)
  error?: string;                  // 错误信息 (可变，通常 < 1KB)
  errorStack?: string;             // 错误堆栈 (可变，通常 < 5KB)
  tags?: string[];                 // 标签数组 (~100-500 bytes)
  customFields?: Record<string, unknown>; // 自定义字段 (可变)
}
```

### 4.2 Task大小估算

| 场景 | 大小估算 |
|-----|---------|
| 正常完成的任务 | ~200-300 bytes |
| 失败的任务（带错误信息） | ~1-5KB |
| 带自定义字段的任务 | ~500 bytes - 2KB |

---

## 五、Workflow存储分析

### 5.1 数据结构

```typescript
interface WorkflowDefinition {
  id: ID;                          // 工作流ID (~36 bytes)
  name: string;                    // 名称 (~50 bytes)
  type: WorkflowType;              // 类型 (~15 bytes)
  description?: string;            // 描述 (~100-500 bytes)
  nodes: Node[];                   // 节点数组 (可变，主要开销)
  edges: Edge[];                   // 边数组 (可变)
  variables?: WorkflowVariable[];  // 变量定义 (~1-5KB)
  triggers?: (WorkflowTrigger | TriggerReference)[]; // 触发器 (~1-5KB)
  triggeredSubworkflowConfig?: TriggeredSubworkflowConfig; // 子工作流配置
  config?: WorkflowConfig;         // 工作流配置 (~1-5KB)
  metadata?: WorkflowMetadata;     // 元数据 (~500 bytes - 2KB)
  version: Version;                // 版本号 (~10 bytes)
  createdAt: Timestamp;            // 创建时间 (~8 bytes)
  updatedAt: Timestamp;            // 更新时间 (~8 bytes)
  availableTools?: { initial: Set<string> }; // 可用工具 (~1-5KB)
}
```

### 5.2 Node数据结构

每个Node的结构：

```typescript
interface BaseNodeProps {
  id: ID;                          // 节点ID (~36 bytes)
  name: string;                    // 名称 (~50 bytes)
  description?: string;            // 描述 (~100 bytes)
  metadata?: Metadata;             // 元数据 (~200 bytes)
  outgoingEdgeIds: ID[];           // 出边ID数组 (~100-500 bytes)
  incomingEdgeIds: ID[];           // 入边ID数组 (~100-500 bytes)
  properties?: any[];              // 动态属性 (可变)
  hooks?: any[];                   // 钩子配置 (可变)
  checkpointBeforeExecute?: boolean; // 检查点配置
  checkpointAfterExecute?: boolean;  // 检查点配置
  config: NodeConfigMap[T];        // 节点特定配置 (可变)
}
```

### 5.3 Workflow大小估算

| 场景 | 节点数量 | 边数量 | 总大小估算 |
|-----|---------|--------|-----------|
| 简单工作流 | 5-10个 | 5-10条 | ~5-10KB |
| 中等复杂度 | 20-30个 | 20-40条 | ~20-40KB |
| 复杂工作流 | 50-100个 | 50-150条 | ~50-150KB |
| 超大工作流 | 200+个 | 200+条 | ~200KB+ |

#### 详细分解（以中等复杂度工作流为例）

```
Workflow总大小: ~30KB
├── 基础字段: ~200 bytes
├── nodes (25个):
│   ├── 基础属性: 25 × 200 bytes = ~5KB
│   └── 配置数据: 25 × 500 bytes = ~12.5KB
├── edges (30条): 30 × 100 bytes = ~3KB
├── variables: ~2KB
├── triggers: ~3KB
├── config: ~2KB
└── metadata: ~1KB
```

---

## 六、存储需求总结

### 6.1 单对象大小范围

| 存储类型 | 最小大小 | 典型大小 | 最大大小 |
|---------|---------|---------|---------|
| Checkpoint | 10KB | 50-200KB | 1MB+ |
| Thread | 200 bytes | 500 bytes | 2KB |
| Task | 200 bytes | 500 bytes | 5KB |
| Workflow | 5KB | 20-50KB | 200KB+ |

### 6.2 存储增长模式

#### Checkpoint存储增长

```
场景：长时间运行的工作流
- 每个Checkpoint: 50KB - 200KB
- 每小时创建Checkpoint数: 10-50个
- 每小时存储增长: 500KB - 10MB
- 每天存储增长: 12MB - 240MB
```

#### Thread存储增长

```
场景：频繁创建线程
- 每个Thread: ~500 bytes
- 每小时创建Thread数: 100-1000个
- 每小时存储增长: 50KB - 500KB
- 每天存储增长: 1.2MB - 12MB
```

#### Task存储增长

```
场景：高并发任务执行
- 每个Task: ~500 bytes
- 每小时创建Task数: 1000-10000个
- 每小时存储增长: 500KB - 5MB
- 每天存储增长: 12MB - 120MB
```

### 6.3 总体存储需求估算

#### 小型部署（单用户，轻量使用）

```
每天新增存储:
- Checkpoint: 10MB
- Thread: 1MB
- Task: 10MB
- Workflow: 0.1MB (偶尔创建新工作流)
总计: ~21MB/天

每月存储: ~630MB
每年存储: ~7.5GB
```

#### 中型部署（10用户，中等使用）

```
每天新增存储:
- Checkpoint: 100MB
- Thread: 10MB
- Task: 100MB
- Workflow: 1MB
总计: ~211MB/天

每月存储: ~6.3GB
每年存储: ~75GB
```

#### 大型部署（100用户，重度使用）

```
每天新增存储:
- Checkpoint: 1GB
- Thread: 100MB
- Task: 1GB
- Workflow: 10MB
总计: ~2.1GB/天

每月存储: ~63GB
每年存储: ~750GB
```

---

## 七、存储优化建议

### 7.1 当前优化机制

项目已实现以下优化机制（详见 `docs/plan/checkpoint-storage-optimization-design.md`）：

1. **增量存储策略**
   - 基线检查点 + 增量检查点
   - 只存储差异，减少重复数据
   - 预期节省: 70-90%

2. **共享引用机制**
   - 跨检查点共享不变数据
   - 引用计数和垃圾回收
   - 预期节省: 80-95%

### 7.2 存储清理策略

#### Checkpoint清理策略

```typescript
// 时间策略：保留最近30天的检查点
const timePolicy: TimeBasedCleanupPolicy = {
  type: "time",
  retentionDays: 30,
  minRetention: 10  // 至少保留10个检查点
};

// 数量策略：最多保留100个检查点
const countPolicy: CountBasedCleanupPolicy = {
  type: "count",
  maxCount: 100,
  minRetention: 10
};

// 大小策略：最多占用1GB空间
const sizePolicy: SizeBasedCleanupPolicy = {
  type: "size",
  maxSizeBytes: 1024 * 1024 * 1024,  // 1GB
  minRetention: 10
};
```

#### Thread清理策略

- 已完成的Thread：保留7-30天
- 失败的Thread：保留30-90天（用于问题排查）
- 运行中的Thread：不清理

#### Task清理策略

- 已完成的Task：保留7天
- 失败的Task：保留30天
- 超时的Task：保留14天

### 7.3 存储监控指标

建议监控以下指标：

```typescript
interface StorageMetrics {
  // Checkpoint指标
  checkpointCount: number;          // 检查点总数
  checkpointTotalSize: number;      // 检查点总大小（bytes）
  checkpointAvgSize: number;        // 检查点平均大小
  checkpointMaxSize: number;        // 最大检查点大小
  
  // Thread指标
  threadCount: number;              // 线程总数
  threadActiveCount: number;        // 活跃线程数
  
  // Task指标
  taskCount: number;                // 任务总数
  taskPendingCount: number;         // 待处理任务数
  taskRunningCount: number;         // 运行中任务数
  
  // Workflow指标
  workflowCount: number;            // 工作流总数
  workflowEnabledCount: number;     // 启用的工作流数
  
  // 存储空间指标
  totalStorageUsed: number;         // 总存储空间使用量
  storageGrowthRate: number;        // 存储增长率（bytes/day）
}
```

---

## 八、存储实现选择建议

### 8.1 JSON文件存储

**适用场景**：
- 开发和测试环境
- 单用户或小团队使用
- 工作流数量较少（< 100个）
- 不需要复杂查询

**优点**：
- 实现简单，易于调试
- 无需额外依赖
- 便于版本控制

**缺点**：
- 大量文件时性能下降
- 不支持复杂查询
- 并发写入需要文件锁

### 8.2 SQLite存储

**适用场景**：
- 生产环境
- 多用户使用
- 工作流数量较多（> 100个）
- 需要复杂查询和统计

**优点**：
- 单文件，易于备份
- 支持复杂查询
- 良好的并发性能
- 内置索引优化

**缺点**：
- 需要SQLite依赖
- 调试相对复杂
- 需要数据库迁移管理

### 8.3 内存存储

**适用场景**：
- 单元测试
- 临时工作流执行
- 不需要持久化的场景

**优点**：
- 最快的访问速度
- 无持久化开销

**缺点**：
- 重启后数据丢失
- 内存占用大

---

## 九、结论

### 9.1 关键发现

1. **Checkpoint是主要存储开销**：占总存储的80-90%
2. **消息历史是Checkpoint的主要组成部分**：占Checkpoint的60-80%
3. **存储增长与使用强度线性相关**：重度使用场景每天可能增长数GB

### 9.2 优化优先级

1. **高优先级**：Checkpoint增量存储（已实现）
2. **中优先级**：Checkpoint共享引用（已实现）
3. **低优先级**：消息压缩机制

### 9.3 容量规划建议

| 部署规模 | 用户数 | 推荐存储空间 | 清理策略 |
|---------|--------|-------------|---------|
| 小型 | 1-10 | 50GB | 保留30天 |
| 中型 | 10-50 | 200GB | 保留14天 |
| 大型 | 50-200 | 1TB | 保留7天 |
| 超大型 | 200+ | 5TB+ | 保留3天 + 归档 |

### 9.4 监控告警建议

建议设置以下告警阈值：

- 存储空间使用率 > 80%：警告
- 存储空间使用率 > 90%：严重告警
- Checkpoint平均大小 > 500KB：检查是否有异常
- 存储增长率异常：可能存在清理策略失效
