# 所有权体系 - 快速参考指南

## 概览速查表

### 当前问题（3个关键问题）

| # | 问题 | 风险 | 位置 |
|---|------|------|------|
| 1 | **多服务冲突**：ThreadRegistry、TaskRegistry、TriggeredSubworkflowManager 都持有相同实体的引用 | Use-After-Free | TriggeredSubworkflowManager.handleSubgraphCompleted() |
| 2 | **Graph 引用**：Thread 直接引用共享的 PreprocessedGraph，无隔离 | 数据不一致 | ThreadBuilder.buildFromPreprocessedGraph() L112 |
| 3 | **清理无序**：多个地方清理同一对象，无明确顺序 | 内存泄漏 | 分散在各服务中 |

---

## 核心概念

### 所有权模式

```
强所有权 (Strong Ownership)
  └─ 负责创建和销毁对象
  └─ 拥有删除权
  └─ 例：GraphRegistry 拥有 PreprocessedGraph
  └─ 例：ThreadBuilder 创建 ThreadEntity

弱引用 (Weak Reference)
  └─ 仅读取对象
  └─ 无删除权
  └─ 删除前必须通知所有者
  └─ 例：TaskRegistry 对 ThreadEntity 的引用
```

### 生命周期阶段

```
ThreadEntity 生命周期：

  创建 (CREATE)
    ↓
  注册 (REGISTER) → ThreadRegistry
    ↓
  执行 (EXECUTE) → TaskRegistry
    ↓
  完成 (COMPLETE) or 失败 (FAILED)
    ↓
  清理 (CLEANUP) → 所有服务清理
    ↓
  删除 (DELETE) → ThreadRegistry 删除
```

---

## 实体关系总览

### 快速查找表

| 实体 | 类型 | 所有者 | 关键方法 | 文件 |
|-----|------|-------|--------|------|
| **PreprocessedGraph** | 数据结构 | GraphRegistry | register, get, delete | packages/types/src/graph/preprocessed-graph.ts |
| **Thread** | 数据对象 | ThreadEntity | - | packages/types/src/thread/definition.ts |
| **ThreadEntity** | 包装器 | ThreadRegistry | getThreadId, cleanup | sdk/core/entities/thread-entity.ts |
| **ThreadRegistry** | 注册表 | DI容器 | register, get, delete | sdk/core/services/thread-registry.ts |
| **TaskRegistry** | 注册表 | DI容器 | register, delete, getAll | sdk/core/services/task-registry.ts |
| **GraphRegistry** | 注册表 | DI容器 | register, get, delete | sdk/core/services/graph-registry.ts |
| **TriggeredSubworkflowManager** | 管理器 | DI容器 | executeTriggeredSubgraph, cleanup | sdk/core/services/triggered-subworkflow-manager.ts |

---

## 改进方案速查

### 方案 1: ThreadLifecycleCoordinator（必做 ⭐⭐⭐）

**是什么**：中央协调者，统一管理线程生命周期  
**为什么**：解决多服务冲突问题  
**怎么做**：

```
1. 创建 ThreadLifecycleCoordinator 类
2. 提供 createThread(), executeThread(), cleanupThread() 方法
3. 各服务注册清理检查点
4. 协调器管理清理流程
```

**代码位置**：`sdk/core/execution/coordinators/thread-lifecycle-coordinator.ts`  
**时间**：11-13 天  
**优先级**：⭐⭐⭐ 必做

**关键 API**：
```typescript
await coordinator.createThread(workflowId, options);
await coordinator.executeThread(threadId);
await coordinator.cleanupThread(threadId);

coordinator.registerCleanupCheckpoint('name', handler, timeout);
coordinator.onLifecycleEvent('CREATED', handler);
```

---

### 方案 2: 所有权追踪（推荐 ⭐⭐）

**是什么**：明确标注谁可以删除对象  
**为什么**：防止 Use-After-Free，保证删除安全  
**怎么做**：

```
1. ThreadRegistry 中添加 owners 追踪
2. register(entity, owner) 记录所有者
3. delete(id, owner) 检查是否有其他所有者
4. 只有最后一个所有者可以删除
```

**代码位置**：`sdk/core/services/thread-registry.ts`  
**时间**：6-7 天  
**优先级**：⭐⭐ 推荐

**检查清单**：
```typescript
// 修改 ThreadRegistry
owners: Map<string, Set<string>> = new Map();

register(entity, owner) {
  if (!owners.has(id)) {
    owners.set(id, new Set());
  }
  owners.get(id).add(owner);
}

delete(id, owner) {
  owners.get(id).delete(owner);
  if (owners.get(id).size === 0) {
    // 安全删除
    threadEntities.delete(id);
  }
}
```

---

### 方案 3: Graph 隔离（推荐 ⭐⭐）

**是什么**：每个线程有独立的 Graph 副本  
**为什么**：线程之间隔离，防止干扰  
**怎么做**：

```
1. ThreadBuilder.createGraphCopyForThread() 创建浅拷贝
2. 共享节点/边数据（不拷贝）
3. 拷贝元数据（ID映射、配置）
4. 每个线程有独立的 graph 副本
```

**代码位置**：`sdk/core/execution/thread-builder.ts`  
**时间**：7-8 天  
**优先级**：⭐⭐ 推荐

**性能指标**：
- 拷贝时间：< 10ms
- 内存增长：< 10%
- 共享效率：70-80% 的数据被共享

---

### 方案 4: Graph 引用计数（可选 ⭐）

**是什么**：自动管理 Graph 的生命周期  
**为什么**：完全自动化，无人工管理  
**怎么做**：

```
1. 创建 ManagedGraphRegistry
2. addRef(workflowId, threadId) 增加引用
3. removeRef(workflowId, threadId) 减少引用
4. 引用数为 0 时自动删除
```

**代码位置**：`sdk/core/services/managed-graph-registry.ts`  
**时间**：9-11 天  
**优先级**：⭐ 可选（长期）

**监控 API**：
```typescript
const stats = graphRegistry.getStats('workflow-1');
// { refCount: 3, threadIds: [...], age: 45000 }

const allStats = graphRegistry.getAllStats();
graphRegistry.cleanup(maxAge); // 清理过期 Graph
```

---

## 常见问题与答案

### Q1: ThreadRegistry 和 TaskRegistry 都持有 ThreadEntity，怎么办？

**A**：使用所有权追踪（方案2）
```typescript
// 注册时标注所有者
threadRegistry.register(entity, 'TaskRegistry');

// 删除时检查
if (threadRegistry.delete(id, 'TaskRegistry')) {
  // 成功删除（没有其他所有者）
} else {
  // 还有其他所有者，不删除
}
```

---

### Q2: 如何保证清理顺序正确？

**A**：使用 ThreadLifecycleCoordinator（方案1）
```
定义的清理顺序：
1. ExecutionContext
2. TriggeredSubworkflowManager
3. TaskRegistry
4. ThreadRegistry
5. GraphRegistry

协调器保证按此顺序执行
```

---

### Q3: 线程共享 Graph 会有什么问题？

**A**：
- 如果 Graph 被修改，所有线程都看到修改
- 如果 GraphRegistry 删除 Graph，线程无法访问
- 多线程间缺乏隔离

**解决**：使用 Graph 隔离（方案3）

---

### Q4: 引用计数会影响性能吗？

**A**：
- 增加/删除引用：O(1) 操作
- 查询引用数：O(1) 操作
- **性能影响**：< 5%（通常可忽略）

---

### Q5: 实施顺序应该是什么？

**A**（推荐）：
1. **第一步**：方案1（ThreadLifecycleCoordinator）
   - 解决最严重的 Use-After-Free 问题
   - 8-10 周时间
   - 立即开始

2. **第二步**：方案2 + 方案3（同时进行）
   - 进一步增强所有权管理
   - 实现 Graph 隔离
   - 4-6 周时间

3. **第三步**：方案4（长期优化）
   - 完全自动化管理
   - 可延后实施
   - 2-3 周时间

---

## 改进前后对比

### 问题1：多服务冲突

**改进前**：
```typescript
// 不安全，多个地方删除
TriggeredSubworkflowManager.handleSubgraphCompleted() {
  taskRegistry.delete(taskId);      // ❌ 删除
  activeTasks.delete(threadId);     // ❌ 删除
  // threadRegistry 未删除，导致不一致
}
```

**改进后**：
```typescript
// 安全，单一协调者
ThreadLifecycleCoordinator.cleanupThread(threadId) {
  emitEvent('CLEANING');
  executeCleanupCheckpoints();      // 所有服务清理
  taskRegistry.delete(...);
  threadRegistry.delete(...);       // 协调器决定何时删除
}
```

---

### 问题2：Graph 引用隔离

**改进前**：
```typescript
const preprocessedGraph = graphRegistry.get('workflow-1');

Thread1.graph = preprocessedGraph;  // 直接引用
Thread2.graph = preprocessedGraph;  // 同一对象

// 问题：修改或删除会影响所有线程
preprocessedGraph.triggers = newTriggers;  // ❌ 两个线程都看到修改
```

**改进后**：
```typescript
const preprocessedGraph = graphRegistry.get('workflow-1');

const graph1 = threadBuilder.createGraphCopyForThread(preprocessedGraph);
const graph2 = threadBuilder.createGraphCopyForThread(preprocessedGraph);

Thread1.graph = graph1;  // ✅ 独立副本
Thread2.graph = graph2;  // ✅ 独立副本

graph1.triggers = newTriggers;  // ✅ 只影响 Thread1
// Thread2 不受影响
```

---

### 问题3：清理顺序无序

**改进前**：
```
handleSubgraphCompleted() ──┬─→ unregisterParentChild()
                            ├─→ taskRegistry.delete()
                            ├─→ activeTasks.delete()
                            └─→ triggerCallback()
// 顺序不确定，容易出错
```

**改进后**：
```
cleanupThread() {
  emitEvent('CLEANING')
    ├─→ checkpoint 'execution-context'
    ├─→ checkpoint 'triggered-subworkflow'
    ├─→ checkpoint 'task-registry'
    └─→ checkpoint 'thread-registry'
  
  emitEvent('DELETED')
}
// 顺序明确，受控
```

---

## 代码审查检查清单

使用此清单审查涉及线程和 Graph 管理的代码：

### 创建相关代码
- [ ] 是否通过 ThreadLifecycleCoordinator 创建？
- [ ] 是否正确标注了所有权？
- [ ] ThreadEntity 是否已注册到 ThreadRegistry？

### 删除相关代码
- [ ] 是否检查了所有其他所有者？
- [ ] 是否通过 cleanupThread() 而非直接 delete()？
- [ ] 是否触发了生命周期事件？

### Graph 访问相关代码
- [ ] 是否直接修改 thread.graph？
- [ ] 是否假设 graph 在整个生命周期内不变？
- [ ] 是否处理了 graph 为 null 的情况？

### 异常处理
- [ ] 是否在异常时也清理资源？
- [ ] 是否有超时保护？
- [ ] 是否记录了清理过程？

---

## 监控和告警建议

### 关键指标

```
1. 活跃线程数
   alert if > 1000 for > 5 min

2. TaskRegistry 大小
   alert if > 500 tasks

3. GraphRegistry 大小
   alert if > 50 graphs

4. 清理时间
   alert if avg > 1000 ms

5. 内存占用
   alert if heap > 1 GB
```

### 日志监控

```
关键日志模式：

✓ 期望看到：
  "Lifecycle event: CREATED (threadId: ...)"
  "Lifecycle event: DELETED (threadId: ...)"
  
❌ 警告标志：
  "Cleanup checkpoint timed out"
  "Thread not found in registry"
  "Ref count leak detected"
```

---

## 文件组织结构

### 改进相关的主要文件

```
sdk/
├── core/
│   ├── entities/
│   │   └── thread-entity.ts           (不变)
│   ├── services/
│   │   ├── thread-registry.ts         (修改 - 方案2)
│   │   ├── task-registry.ts           (修改 - 方案2)
│   │   ├── graph-registry.ts          (修改 - 方案4)
│   │   ├── managed-graph-registry.ts  (新增 - 方案4)
│   │   └── triggered-subworkflow-manager.ts (修改 - 方案1)
│   ├── execution/
│   │   ├── thread-builder.ts          (修改 - 方案3)
│   │   ├── thread-executor.ts         (不变)
│   │   └── coordinators/
│   │       ├── thread-lifecycle-coordinator.ts (新增 - 方案1)
│   │       └── ...
│   └── di/
│       └── container-config.ts        (修改 - 方案1)
│
└── __tests__/
    ├── thread-lifecycle-integration.test.ts (新增 - 方案1)
    ├── ownership-tracking.test.ts          (新增 - 方案2)
    ├── graph-isolation.test.ts             (新增 - 方案3)
    └── managed-graph-registry.test.ts      (新增 - 方案4)
```

---

## 快速开始指南

### 如果你要修改 ThreadRegistry：

1. 查看 `entity-ownership-system-analysis.md` 第2部分
2. 参考 `ownership-refactoring-details.md` 第一部分的问题演示
3. 实现 `ownership-refactoring-details.md` 中的所有权追踪
4. 运行测试验证

### 如果你要修改 TriggeredSubworkflowManager：

1. 了解 `entity-ownership-system-analysis.md` 中问题1
2. 创建 ThreadLifecycleCoordinator（如果还未创建）
3. 将清理逻辑迁移到检查点回调
4. 参考 `ownership-refactoring-details.md` 中的完整示例

### 如果你要修改 ThreadBuilder：

1. 理解 `entity-ownership-system-analysis.md` 第二部分的 Graph 问题
2. 实现 `createGraphCopyForThread()` 方法
3. 集成到 `buildFromPreprocessedGraph()`
4. 运行 graph-isolation 测试

---

## 相关文档导航

| 文档 | 用途 | 适合人群 |
|------|------|--------|
| entity-ownership-system-analysis.md | 完整分析和方案设计 | 架构师、技术主管 |
| ownership-refactoring-details.md | 代码实现细节和示例 | 开发工程师 |
| ownership-system-implementation-roadmap.md | 项目计划和时间表 | 项目经理、工程主管 |
| **ownership-system-quick-reference.md** | 快速查找和常见问题 | 所有人 |

---

## 获取帮助

如有疑问，参考以下顺序：

1. **快速问题** → 查看本文档的常见问题部分
2. **实现细节** → 查看 ownership-refactoring-details.md
3. **设计原理** → 查看 entity-ownership-system-analysis.md
4. **项目管理** → 查看 ownership-system-implementation-roadmap.md

