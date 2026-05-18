# 中断级联传播架构设计

## 📋 设计目标

实现一个 **Workflow/Agent 通用**的中断级联传播机制,满足以下需求:

| 操作 | 父 → 子传播 | 子 → 父传播 | 说明 |
|------|------------|------------|------|
| **PAUSE** | ✅ 必须传播 | ❌ 不应传播 | 父暂停时,所有子执行实例应立即暂停 |
| **STOP** | ✅ 必须传播 | ❌ 不应传播 | 父停止时,所有子执行实例应立即停止 |
| **RESUME** | ✅ 必须传播 | ❌ 不应传播 | 父恢复时,所有子执行实例应同步恢复 |

### 核心要求

1. **即时生效**: 父执行实例的中断操作必须在**毫秒级**内传播到所有子实例
2. **类型安全**: 与现有的 `ExecutionHierarchyMetadata` 无缝整合
3. **零轮询**: 不使用定时检查,采用事件驱动机制
4. **双向隔离**: 子实例的中断不影响父实例(单向传播)
5. **通用性**: 同时支持 Workflow 和 Agent Loop,未来可扩展到其他执行类型

---

## 🏗️ 架构设计

### 1. 核心概念

#### 1.1 中断传播树 (Interruption Propagation Tree)

基于现有的 `ExecutionHierarchyMetadata`,构建一棵**中断传播树**:

```
Root Workflow (depth=0)
├── Subgraph Workflow (depth=1)          ← 继承父的中断状态
│   ├── Nested Subgraph (depth=2)        ← 继承祖父的中断状态
│   └── Agent Loop (depth=2)             ← 继承祖父的中断状态
├── Fork Branch 1 (depth=1)              ← 继承父的中断状态
├── Fork Branch 2 (depth=1)              ← 继承父的中断状态
└── Triggered Workflow (depth=1)         ← 独立中断状态 (异步,不继承)
```

**关键规则**:
- **同步子执行** (SUBGRAPH, FORK_BRANCH): 继承父的中断状态
- **异步子执行** (TRIGGERED_SUBWORKFLOW): 独立中断状态,不继承

#### 1.2 中断传播代理 (InterruptionPropagationProxy)

每个执行实例的 `InterruptionState` 内部维护一个**传播代理**,负责:
- 监听父实例的中断事件
- 向子实例广播中断事件
- 管理订阅关系(防止内存泄漏)

---

### 2. 数据模型扩展

#### 2.1 扩展 `InterruptionStateConfig`

```typescript
// sdk/core/types/interruption-state.ts

export interface InterruptionStateConfig {
  /** Context ID (execution ID) */
  contextId: string;
  
  /** Node ID (optional, for workflow nodes) */
  nodeId?: string;
  
  /** Custom interrupt exception creator */
  createInterruptionError?: (info: InterruptionInfo) => InterruptedException;
  
  // ===== 新增字段 =====
  
  /** 
   * Parent interruption state reference (for cascade propagation)
   * If provided, this instance will automatically propagate parent's interruptions
   */
  parentInterruptionState?: InterruptionState;
  
  /**
   * Execution type (for logging and debugging)
   */
  executionType?: 'WORKFLOW' | 'AGENT_LOOP';
}
```

#### 2.2 扩展 `ChildExecutionReference`

在 `packages/types/src/execution/hierarchy.ts` 中添加中断传播标记:

```typescript
export type ChildExecutionReference =
  | {
      childType: 'WORKFLOW';
      childId: ID;
      createdAt: Timestamp;
      forkPathId?: ID;
      
      // ===== 新增字段 =====
      /** 
       * Whether this child inherits parent's interruption state
       * - true: SUBGRAPH, FORK_BRANCH (synchronous, inherits)
       * - false: TRIGGERED_SUBWORKFLOW (asynchronous, independent)
       */
      inheritsInterruption?: boolean;
    }
  | {
      childType: 'AGENT_LOOP';
      childId: ID;
      createdAt: Timestamp;
      
      // ===== 新增字段 =====
      inheritsInterruption?: boolean;
    };
```

---

### 3. 核心机制设计

#### 3.1 中断传播代理类

```typescript
// sdk/core/types/interruption-propagation-proxy.ts

/**
 * Interruption Propagation Proxy
 * 
 * Manages parent-child interruption state synchronization using event-driven mechanism.
 * Ensures immediate propagation without polling.
 */
export class InterruptionPropagationProxy {
  private parentState?: InterruptionState;
  private childStates: Set<InterruptionState> = new Set();
  private unsubscribeFromParent?: () => void;
  
  /**
   * Attach to parent interruption state
   * Listens for pause/stop/resume events and propagates to children
   */
  attachToParent(parentState: InterruptionState): void {
    this.parentState = parentState;
    
    // Subscribe to parent's interruption events
    this.unsubscribeFromParent = parentState.onInterrupted((type) => {
      this.propagateToInterruption(type);
    });
    
    // Immediately sync with parent's current state
    if (parentState.isAborted()) {
      const reason = parentState.getAbortReason();
      if (reason instanceof InterruptedException) {
        this.propagateToInterruption(reason.type === "PAUSE" ? "PAUSE" : "STOP");
      }
    }
  }
  
  /**
   * Register a child interruption state
   * The child will receive all future interruption events from this proxy
   */
  registerChild(childState: InterruptionState): void {
    this.childStates.add(childState);
    
    // Immediately sync with current interruption state
    if (this.parentState?.isAborted()) {
      const reason = this.parentState.getAbortReason();
      if (reason instanceof InterruptedException) {
        const type = reason.type === "PAUSE" ? "PAUSE" : "STOP";
        this.syncChildState(childState, type);
      }
    }
  }
  
  /**
   * Unregister a child interruption state (cleanup)
   */
  unregisterChild(childState: InterruptionState): void {
    this.childStates.delete(childState);
  }
  
  /**
   * Propagate interruption to all registered children
   */
  private propagateToInterruption(type: "PAUSE" | "STOP" | "RESUME"): void {
    logger.debug("Propagating interruption to children", {
      type,
      childCount: this.childStates.size,
    });
    
    for (const childState of this.childStates) {
      this.syncChildState(childState, type);
    }
  }
  
  /**
   * Sync a single child's interruption state
   */
  private syncChildState(childState: InterruptionState, type: "PAUSE" | "STOP" | "RESUME"): void {
    try {
      if (type === "PAUSE") {
        childState.requestPause();
      } else if (type === "STOP") {
        childState.requestStop();
      } else if (type === "RESUME") {
        childState.resume();
      }
    } catch (error) {
      logger.warn("Failed to propagate interruption to child", {
        childContextId: childState.getContextId(),
        error,
      });
    }
  }
  
  /**
   * Cleanup all subscriptions (prevent memory leaks)
   */
  dispose(): void {
    if (this.unsubscribeFromParent) {
      this.unsubscribeFromParent();
      this.unsubscribeFromParent = undefined;
    }
    this.childStates.clear();
    this.parentState = undefined;
  }
}
```

#### 3.2 扩展 `InterruptionState`

```typescript
// sdk/core/types/interruption-state.ts

export class InterruptionState {
  // ... existing fields ...
  
  /** Interruption propagation proxy (manages parent-child sync) */
  private propagationProxy?: InterruptionPropagationProxy;
  
  /** Event listeners for interruption notifications (new) */
  private interruptionListeners: Array<(type: "PAUSE" | "STOP" | "RESUME") => void> = [];
  
  constructor(config: InterruptionStateConfig) {
    // ... existing initialization ...
    
    // Initialize propagation proxy if parent is provided
    if (config.parentInterruptionState) {
      this.propagationProxy = new InterruptionPropagationProxy();
      this.propagationProxy.attachToParent(config.parentInterruptionState);
      
      logger.debug("InterruptionState attached to parent", {
        contextId: this.contextId,
        parentContextId: config.parentInterruptionState.getContextId(),
      });
    }
  }
  
  /**
   * Register a child interruption state for cascade propagation
   * 
   * @param childState Child's interruption state
   * @example
   * ```typescript
   * parentInterruptionState.registerChild(childInterruptionState);
   * ```
   */
  registerChild(childState: InterruptionState): void {
    if (!this.propagationProxy) {
      this.propagationProxy = new InterruptionPropagationProxy();
    }
    this.propagationProxy.registerChild(childState);
    
    logger.debug("Child interruption state registered", {
      parentContextId: this.contextId,
      childContextId: childState.getContextId(),
    });
  }
  
  /**
   * Unregister a child interruption state (cleanup)
   */
  unregisterChild(childState: InterruptionState): void {
    this.propagationProxy?.unregisterChild(childState);
  }
  
  /**
   * Subscribe to interruption events
   * 
   * @param callback Called when pause/stop is requested
   * @returns Unsubscribe function
   * @example
   * ```typescript
   * const unsubscribe = interruptionState.onInterrupted((type) => {
   *   console.log(`Interrupted: ${type}`);
   * });
   * ```
   */
  onInterrupted(callback: (type: "PAUSE" | "STOP" | "RESUME") => void): () => void {
    this.interruptionListeners.push(callback);
    
    return () => {
      const index = this.interruptionListeners.indexOf(callback);
      if (index !== -1) {
        this.interruptionListeners.splice(index, 1);
      }
    };
  }
  
  /**
   * Request to pause (enhanced with event notification)
   */
  requestPause(): void {
    if (this.interruptionType === "PAUSE") {
      return;
    }

    logger.info("Execution pause requested", { 
      contextId: this.contextId, 
      nodeId: this.nodeId 
    });
    
    this.interruptionType = "PAUSE";
    const error = this.createError("Execution paused", "PAUSE");
    this.abortController.abort(error);
    
    // Notify all listeners (including propagation proxy)
    this.notifyInterruptionListeners("PAUSE");
  }
  
  /**
   * Request to stop (enhanced with event notification)
   */
  requestStop(): void {
    if (this.interruptionType === "STOP") {
      return;
    }

    logger.info("Execution stop requested", { 
      contextId: this.contextId, 
      nodeId: this.nodeId 
    });
    
    this.interruptionType = "STOP";
    const error = this.createError("Execution stopped", "STOP");
    this.abortController.abort(error);
    
    // Notify all listeners (including propagation proxy)
    this.notifyInterruptionListeners("STOP");
  }
  
  /**
   * Resume execution (enhanced with automatic cascade propagation)
   * 
   * IMPORTANT: Resume now automatically propagates to all children,
   * consistent with PAUSE/STOP behavior. This ensures state consistency
   * across the entire execution hierarchy.
   */
  resume(): void {
    logger.info("Execution resumed", { 
      contextId: this.contextId, 
      nodeId: this.nodeId 
    });
    
    this.interruptionType = null;
    this.abortController = new AbortController();
    
    // Notify resume listeners (existing)
    const resumeListeners = [...this.resumeListeners];
    this.resumeListeners = [];
    resumeListeners.forEach(listener => {
      try {
        listener();
      } catch (error) {
        logger.warn("Error in resume listener", { error });
      }
    });
    
    // Auto-propagate resume to children (consistent with pause/stop)
    this.notifyInterruptionListeners("RESUME");
  }
  
  /**
   * Notify all interruption listeners
   */
  private notifyInterruptionListeners(type: "PAUSE" | "STOP" | "RESUME"): void {
    const listeners = [...this.interruptionListeners];
    listeners.forEach(listener => {
      try {
        listener(type);
      } catch (error) {
        logger.warn("Error in interruption listener", { error });
      }
    });
  }
  
  /**
   * Cleanup resources (prevent memory leaks)
   */
  dispose(): void {
    this.propagationProxy?.dispose();
    this.interruptionListeners = [];
    this.resumeListeners = [];
  }
}
```

---

### 4. 集成点设计

#### 4.1 Workflow 子执行创建流程

```typescript
// sdk/workflow/execution/factories/workflow-execution-builder.ts

async createChildExecution(
  parent: WorkflowExecutionEntity,
  options: ChildExecutionOptions
): Promise<WorkflowExecutionBuildResult> {
  const { type, config } = options;
  
  // Step 1-4: Existing logic (validation, graph retrieval, entity creation, variable init)
  // ...
  
  // Step 5: Establish hierarchy relationship (ENHANCED)
  this.establishHierarchyWithInterruption(parent, childEntity, type, config);
  
  // Step 6-7: Existing logic (conversation session, state coordinator)
  // ...
  
  return { /* ... */ };
}

/**
 * Establish hierarchy relationship WITH interruption cascade propagation
 */
private establishHierarchyWithInterruption(
  parent: WorkflowExecutionEntity,
  child: WorkflowExecutionEntity,
  type: ChildExecutionType,
  config: ChildExecutionConfig
): void {
  // Step 1: Update hierarchy metadata (existing logic)
  this.establishHierarchy(parent, child, type, config);
  
  // Step 2: Setup interruption cascade propagation (NEW)
  const shouldInheritInterruption = this.shouldInheritInterruption(type, config);
  
  if (shouldInheritInterruption) {
    const parentInterruptionState = parent.getInterruptionState();
    const childInterruptionState = child.getInterruptionState();
    
    if (parentInterruptionState && childInterruptionState) {
      // Register child with parent's interruption state
      parentInterruptionState.registerChild(childInterruptionState);
      
      logger.info("Interruption cascade established", {
        parentExecutionId: parent.id,
        childExecutionId: child.id,
        childType: type,
      });
    }
  }
}

/**
 * Determine whether child should inherit parent's interruption state
 */
private shouldInheritInterruption(
  type: ChildExecutionType,
  config: ChildExecutionConfig
): boolean {
  switch (type) {
    case 'SUBGRAPH':
      return true; // Synchronous, inherits
      
    case 'FORK_BRANCH':
      return true; // Synchronous, inherits
      
    case 'TRIGGERED':
      // Explicitly check execution mode for clarity
      return config.executionMode === 'SYNCHRONOUS';
      
    default:
      return false;
  }
}
```

#### 4.2 Agent Loop 子执行创建流程

```typescript
// sdk/agent/entities/agent-loop-entity.ts

/**
 * Spawn a sub-agent (delegation)
 * 
 * The sub-agent will inherit the parent's interruption state.
 */
spawnSubAgent(options: SubAgentSpawnOptions): AgentLoopEntity {
  // Step 1: Create sub-agent entity
  const subAgent = new AgentLoopEntity(/* ... */);
  
  // Step 2: Set parent context (existing)
  subAgent.setParentContext({
    parentType: 'AGENT_LOOP',
    parentId: this.id,
    delegationPurpose: options.purpose,
  });
  
  // Step 3: Register as child (existing)
  this.registerChild({
    childType: 'AGENT_LOOP',
    childId: subAgent.id,
    createdAt: Date.now(),
  });
  
  // Step 4: Setup interruption cascade (NEW)
  const parentInterruptionState = this.getInterruptionState();
  const childInterruptionState = subAgent.getInterruptionState();
  
  if (parentInterruptionState && childInterruptionState) {
    parentInterruptionState.registerChild(childInterruptionState);
    
    logger.info("Agent interruption cascade established", {
      parentAgentId: this.id,
      childAgentId: subAgent.id,
    });
  }
  
  return subAgent;
}
```

#### 4.3 清理机制

```typescript
// sdk/workflow/entities/workflow-execution-entity.ts

/**
 * Cleanup execution resources (called when execution completes)
 */
cleanup(): void {
  // Step 1: Unregister from parent's interruption cascade
  const parentContext = this.getParentContext();
  if (parentContext) {
    const parentEntity = this.workflowExecutionRegistry?.get(parentContext.parentId);
    if (parentEntity) {
      const parentInterruptionState = parentEntity.getInterruptionState();
      const childInterruptionState = this.getInterruptionState();
      
      if (parentInterruptionState && childInterruptionState) {
        parentInterruptionState.unregisterChild(childInterruptionState);
        
        logger.debug("Unregistered from parent interruption cascade", {
          parentExecutionId: parentEntity.id,
          childExecutionId: this.id,
        });
      }
    }
  }
  
  // Step 2: Dispose interruption state (cleanup all listeners)
  this.interruptionState?.dispose();
  
  // Step 3: Other cleanup logic...
}
```

---

### 5. 恢复流程设计

#### 5.1 父工作流恢复时的级联恢复

```typescript
// sdk/workflow/execution/coordinators/workflow-lifecycle-coordinator.ts

async resume(executionId: string): Promise<WorkflowExecutionResult> {
  // Step 1: Restore from checkpoint (existing)
  const workflowExecutionEntity = await this.restoreFromCheckpoint(executionId);
  
  // Step 2: Resume parent interruption state (existing)
  // NOTE: Resume now automatically cascades to children via event propagation
  workflowExecutionEntity.resetInterrupt();
  
  // Step 3: Execute workflow (existing)
  // Children are already resumed via automatic propagation in resetInterrupt()
  const result = await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
  
  return result;
}
```

---

### 6. 时序图

#### 6.1 PAUSE 传播时序

```
User                    Parent Workflow          Subgraph Workflow        Agent Loop
 |                           |                         |                     |
 |--- pause() ------------->|                         |                     |
 |                           |                         |                     |
 |                           |-- requestPause() ------>|                     |
 |                           |  (via propagation)      |                     |
 |                           |                         |                     |
 |                           |                         |-- requestPause() -->|
 |                           |                         |  (via propagation)  |
 |                           |                         |                     |
 |                           |<-- abort signal --------|                     |
 |                           |   (immediate)           |                     |
 |                           |                         |                     |
 |                           |<-- abort signal ------------------------------|
 |                           |   (immediate)                                 |
 |                           |                         |                     |
 |<-- PAUSED event ----------|                         |                     |
```

**关键点**:
- 传播是**同步的**,在 `requestPause()` 调用期间完成
- 所有子实例在**同一调用栈**内被中止
- 无延迟,无轮询

#### 6.2 RESUME 传播时序

```
User                    Parent Workflow          Subgraph Workflow        Agent Loop
 |                           |                         |                     |
 |--- resume() ------------>|                         |                     |
 |                           |                         |                     |
 |                           |-- resume() ------------>|                     |
 |                           |  (auto propagation)     |                     |
 |                           |                         |                     |
 |                           |                         |-- resume() -------->|
 |                           |                         |  (auto propagation) |
 |                           |                         |                     |
 |                           |<-- new AbortSignal -----|                     |
 |                           |                         |                     |
 |                           |<-- new AbortSignal -------------------------->|
 |                           |                         |                     |
 |<-- RESUMED event ---------|                         |                     |
```

**关键点**:
- Resume **自动传播**,与 PAUSE/STOP 保持一致
- 通过事件驱动机制实现,无需协调器显式调用
- 确保整个执行树的状态一致性

---

### 7. 边界情况处理

#### 7.1 深层嵌套 (Deep Nesting)

```
Workflow (depth=0)
└── Subgraph (depth=1)
    └── Subgraph (depth=2)
        └── Subgraph (depth=3)
            └── Agent (depth=4)
```

**处理方式**:
- 传播是**递归的**,通过 `InterruptionPropagationProxy` 自动处理
- 每层只关心自己的直接子节点
- 无深度限制,但建议监控性能

#### 7.2 循环引用检测

虽然理论上不应该出现,但需要防御性编程:

```typescript
registerChild(childState: InterruptionState): void {
  // Detect circular reference
  if (this === childState) {
    throw new Error("Cannot register self as child");
  }
  
  // Optional: Check if child is already an ancestor
  if (this.isAncestorOf(childState)) {
    throw new Error("Circular interruption dependency detected");
  }
  
  // ... normal registration logic
}
```

#### 7.3 并发中断请求

场景: 父和子同时收到 PAUSE 请求

**处理方式**:
- `InterruptionState` 已有防护: `if (this.interruptionType === "PAUSE") return;`
- 多次调用是**安全的**,只会触发一次实际的中断
- 传播代理会检查子实例的当前状态,避免重复传播

#### 7.4 子实例已完成

场景: 父收到 PAUSE 时,子实例已经完成执行

**处理方式**:
- 子实例的 `InterruptionState` 仍然有效,可以接收中断信号
- 但子实例的执行协调器应该检查状态,忽略已完成实例的中断
- 传播代理捕获异常并记录日志,不阻断其他子实例的传播

---

### 8. 性能考虑

#### 8.1 时间复杂度

- **PAUSE/STOP/RESUME 传播**: O(n),其中 n 是直接子节点数量
- **深层嵌套**: O(d × n),其中 d 是深度,n 是平均子节点数
- **典型场景**: < 1ms (假设 d < 5, n < 10)

#### 8.2 空间复杂度

- **每个 InterruptionState**: O(c),其中 c 是子节点数量
- **传播代理**: 使用 `Set` 存储子状态引用,无额外开销
- **总开销**: 可忽略不计 (< 1KB per execution)

#### 8.3 内存泄漏防护

```typescript
// 1. 子实例完成时主动注销
childEntity.cleanup(); // calls unregisterChild()

// 2. 父实例销毁时清理所有引用
parentInterruptionState.dispose(); // clears all child references

// 3. 使用弱引用 (可选优化)
private childStates: WeakSet<InterruptionState> = new WeakSet();
```

#### 8.4 深层嵌套监控

为防止极端情况下的性能问题,添加深度监控:

```typescript
class InterruptionPropagationProxy {
  private static MAX_RECOMMENDED_DEPTH = 10;
  private currentDepth: number = 0;
  
  private propagateToInterruption(type: "PAUSE" | "STOP" | "RESUME"): void {
    this.currentDepth++;
    
    if (this.currentDepth > InterruptionPropagationProxy.MAX_RECOMMENDED_DEPTH) {
      logger.warn("Deep interruption propagation detected", {
        depth: this.currentDepth,
        type,
      });
    }
    
    // ... propagation logic ...
    
    this.currentDepth--;
  }
}
```

---

### 9. 测试策略

#### 9.1 单元测试

```typescript
describe('InterruptionPropagationProxy', () => {
  it('should propagate PAUSE from parent to child immediately', () => {
    const parent = new InterruptionState({ contextId: 'parent' });
    const child = new InterruptionState({ 
      contextId: 'child',
      parentInterruptionState: parent,
    });
    
    parent.requestPause();
    
    expect(child.isAborted()).toBe(true);
    expect(child.shouldPause()).toBe(true);
  });
  
  it('should NOT propagate interruption from child to parent', () => {
    const parent = new InterruptionState({ contextId: 'parent' });
    const child = new InterruptionState({ 
      contextId: 'child',
      parentInterruptionState: parent,
    });
    
    child.requestPause();
    
    expect(parent.isAborted()).toBe(false);
  });
  
  it('should handle deep nesting correctly', () => {
    const grandparent = new InterruptionState({ contextId: 'grandparent' });
    const parent = new InterruptionState({ 
      contextId: 'parent',
      parentInterruptionState: grandparent,
    });
    const child = new InterruptionState({ 
      contextId: 'child',
      parentInterruptionState: parent,
    });
    
    grandparent.requestStop();
    
    expect(parent.isAborted()).toBe(true);
    expect(child.isAborted()).toBe(true);
  });
});
```

#### 9.2 集成测试

```typescript
describe('Workflow Interruption Cascade', () => {
  it('should pause subgraph when parent workflow is paused', async () => {
    const parentWorkflow = await createWorkflow('parent');
    const subgraphWorkflow = await createSubgraph(parentWorkflow);
    
    parentWorkflow.pause();
    
    expect(subgraphWorkflow.getStatus()).toBe('PAUSED');
  });
  
  it('should resume subgraph when parent workflow is resumed', async () => {
    const parentWorkflow = await createWorkflow('parent');
    const subgraphWorkflow = await createSubgraph(parentWorkflow);
    
    parentWorkflow.pause();
    parentWorkflow.resume();
    
    expect(subgraphWorkflow.getStatus()).toBe('RUNNING');
  });
});
```

---

### 10. 迁移指南

#### 10.1 现有代码兼容性

- ✅ **向后兼容**: 不传入 `parentInterruptionState` 时,行为与之前完全一致
- ✅ **渐进式采用**: 可以逐步为 SUBGRAPH/FORK 启用级联传播
- ⚠️ **需要注意**: TRIGGERED_SUBWORKFLOW 默认不继承,如需继承需显式配置

#### 10.2 迁移步骤

1. **Phase 1**: 实现 `InterruptionPropagationProxy` 和扩展 `InterruptionState`
2. **Phase 2**: 修改 `createChildExecution()` 添加级联逻辑
3. **Phase 3**: 为现有测试添加级联中断验证
4. **Phase 4**: 监控生产环境,确认无性能问题

---

## 📊 总结

### 优势

1. ✅ **即时生效**: 事件驱动,无延迟,无轮询
2. ✅ **类型安全**: 与现有 hierarchy 系统无缝整合
3. ✅ **通用性强**: 同时支持 Workflow 和 Agent
4. ✅ **易于维护**: 单一职责,清晰的边界
5. ✅ **可扩展**: 轻松添加新的执行类型

### 风险

1. ⚠️ **深层嵌套性能**: 需要监控极端情况下的性能
2. ⚠️ **内存泄漏**: 需要确保正确清理订阅关系
3. ⚠️ **调试复杂性**: 级联传播可能使问题定位变复杂

### 下一步

1. 评审设计方案
2. 实现核心组件 (`InterruptionPropagationProxy`)
3. 编写单元测试
4. 集成到 Workflow/Agent 创建流程
5. 端到端测试验证
