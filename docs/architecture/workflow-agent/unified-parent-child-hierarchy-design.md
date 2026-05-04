# 统一父子关系架构设计

## 1. 背景与现状分析

### 1.1 当前架构问题

目前系统中存在**三种独立的父子关系管理机制**，缺乏统一的设计：

#### **Workflow → Workflow（工作流调用工作流）**
- **实现位置**: `WorkflowExecutionEntity.triggeredSubworkflowContext`
- **跟踪方式**: `childExecutionIds: ID[]`（数组）
- **父引用**: `parentExecutionId`（在 context 中）
- **注册表**: `WorkflowRegistry` 维护定义级关系，`WorkflowExecutionRegistry` 管理运行时实例
- **特点**: 支持异步执行、任务队列管理、生命周期解耦

#### **Workflow → Agent（工作流调用智能体）**
- **实现位置**: `WorkflowExecutionEntity.childAgentLoopIds`
- **跟踪方式**: `Set<string>`（集合）
- **父引用**: `AgentLoopEntity.parentExecutionId` + `nodeId`
- **注册表**: `AgentLoopRegistry` 提供按父ID查询/清理方法
- **特点**: 同步执行为主，生命周期紧密绑定

#### **缺失的能力：Agent → Agent（智能体调用智能体）**
- **现状**: ❌ 不支持
- **需求**: 复杂任务分解场景下，主智能体需要委派子任务给专用智能体
- **参考**: Lim-Code 的 SubAgents 工具模式（但未实现层级追踪）

### 1.2 核心痛点

1. **类型不一致**: 
   - Workflow 使用数组 `ID[]`
   - Agent 使用集合 `Set<string>`
   - 缺乏统一的类型抽象

2. **管理分散**:
   - 父子关系字段散落在不同实体中
   - 没有统一的接口契约
   - 查询和清理逻辑重复

3. **扩展性差**:
   - 新增执行类型（如 Tool Execution）需要修改多处代码
   - 无法优雅支持混合层级（Workflow → Agent → Agent）

4. **缺少强类型约束**:
   - 父引用可以是任意字符串，编译时无法验证
   - 没有区分"Workflow父级"和"Agent父级"

---

## 2. 设计目标

### 2.1 核心原则

1. **联合类型统一管理**: 使用 TypeScript 联合类型（Union Types）表达多种父子关系
2. **强类型安全**: 编译时验证父级类型，避免运行时错误
3. **对称设计**: 父→子和子→父关系保持一致的API风格
4. **可扩展性**: 新增执行类型无需修改现有代码
5. **向后兼容**: 保持现有功能不变，仅增强类型系统

### 2.2 支持的场景

| 场景 | 父类型 | 子类型 | 说明 |
|------|--------|--------|------|
| Workflow → Workflow | WorkflowExecution | WorkflowExecution | 触发式子工作流 |
| Workflow → Agent | WorkflowExecution | AgentLoopExecution | AGENT_LOOP节点执行 |
| Agent → Agent | AgentLoopExecution | AgentLoopExecution | 智能体委派（新增） |
| Agent → Workflow | AgentLoopExecution | WorkflowExecution | 智能体调用工作流（新增） |

---

## 3. 架构设计方案

### 3.1 类型层设计（@wf-agent/types）

#### **3.1.1 执行类型枚举**

```typescript
// packages/types/src/execution/hierarchy.ts

/**
 * 执行实例类型
 */
export type ExecutionType = 'WORKFLOW' | 'AGENT_LOOP';

/**
 * 执行实例标识符（带类型标签）
 */
export interface ExecutionIdentity {
  /** 执行类型 */
  type: ExecutionType;
  /** 执行ID */
  id: ID;
}
```

#### **3.1.2 统一的父子关系上下文**

```typescript
// packages/types/src/execution/hierarchy.ts

/**
 * 父执行上下文（联合类型）
 * 
 * 使用 discriminated union 确保类型安全
 */
export type ParentExecutionContext =
  | {
      /** 父类型为工作流 */
      parentType: 'WORKFLOW';
      /** 父工作流执行ID */
      parentId: ID;
      /** 在父工作流中的节点ID（可选） */
      nodeId?: ID;
    }
  | {
      /** 父类型为智能体 */
      parentType: 'AGENT_LOOP';
      /** 父智能体执行ID */
      parentId: ID;
      /** 委派目的/原因（可选） */
      delegationPurpose?: string;
    };

/**
 * 子执行引用（联合类型）
 * 
 * 用于父执行跟踪所有子执行
 */
export type ChildExecutionReference =
  | {
      /** 子类型为工作流 */
      childType: 'WORKFLOW';
      /** 子工作流执行ID */
      childId: ID;
      /** 创建时间戳 */
      createdAt: Timestamp;
    }
  | {
      /** 子类型为智能体 */
      childType: 'AGENT_LOOP';
      /** 子智能体执行ID */
      childId: ID;
      /** 创建时间戳 */
      createdAt: Timestamp;
    };
```

#### **3.1.3 执行层级元数据**

```typescript
// packages/types/src/execution/hierarchy.ts

/**
 * 执行层级元数据
 * 
 * 附加到每个执行实例，描述其在层级树中的位置
 */
export interface ExecutionHierarchyMetadata {
  /** 父执行上下文（如果存在） */
  parent?: ParentExecutionContext;
  
  /** 子执行引用列表 */
  children: ChildExecutionReference[];
  
  /** 层级深度（根节点为0） */
  depth: number;
  
  /** 根执行ID（层级树的根） */
  rootExecutionId: ID;
  
  /** 根执行类型 */
  rootExecutionType: ExecutionType;
}
```

#### **3.1.4 更新现有类型定义**

**WorkflowExecution 增强**:
```typescript
// packages/types/src/workflow-execution/definition.ts

export interface WorkflowExecution {
  // ... existing fields ...
  
  /**
   * 执行层级元数据（新增）
   * 
   * 替代原有的 triggeredSubworkflowContext
   * 统一管理所有父子关系
   */
  hierarchy?: ExecutionHierarchyMetadata;
  
  /**
   * @deprecated 使用 hierarchy.children 替代
   * 保留以保持向后兼容
   */
  triggeredSubworkflowContext?: TriggeredSubworkflowContext;
}
```

**AgentLoopExecution 增强**:
```typescript
// packages/types/src/agent-execution/definition.ts

export interface AgentLoopExecution {
  // ... existing fields ...
  
  /**
   * 执行层级元数据（新增）
   * 
   * 替代原有的 parentWorkflowExecutionId 和 nodeId
   * 支持更丰富的父子关系（包括 Agent → Agent）
   */
  hierarchy?: ExecutionHierarchyMetadata;
  
  /**
   * @deprecated 使用 hierarchy.parent 替代
   * 保留以保持向后兼容
   */
  parentWorkflowExecutionId?: ID;
  
  /**
   * @deprecated 使用 hierarchy.parent.nodeId 替代
   * 保留以保持向后兼容
   */
  nodeId?: ID;
}
```

### 3.2 SDK实体层设计

#### **3.2.1 WorkflowExecutionEntity 增强**

```typescript
// sdk/workflow/entities/workflow-execution-entity.ts

export class WorkflowExecutionEntity {
  // ... existing fields ...
  
  /**
   * 执行层级管理器（新增）
   * 
   * 封装所有父子关系操作
   */
  private hierarchyManager: ExecutionHierarchyManager;
  
  constructor(workflowExecution: WorkflowExecution, ...) {
    // ... existing initialization ...
    
    // 初始化层级管理器
    this.hierarchyManager = new ExecutionHierarchyManager(
      workflowExecution.id,
      'WORKFLOW',
      workflowExecution.hierarchy
    );
  }
  
  // ========== 统一的层级管理API（新增）==========
  
  /**
   * 设置父执行上下文
   */
  setParentContext(parentContext: ParentExecutionContext): void {
    this.hierarchyManager.setParent(parentContext);
  }
  
  /**
   * 获取父执行上下文
   */
  getParentContext(): ParentExecutionContext | undefined {
    return this.hierarchyManager.getParent();
  }
  
  /**
   * 注册子执行
   */
  registerChild(childRef: ChildExecutionReference): void {
    this.hierarchyManager.addChild(childRef);
  }
  
  /**
   * 注销子执行
   */
  unregisterChild(childId: ID, childType: ExecutionType): void {
    this.hierarchyManager.removeChild(childId, childType);
  }
  
  /**
   * 获取所有子执行
   */
  getChildren(): ChildExecutionReference[] {
    return this.hierarchyManager.getChildren();
  }
  
  /**
   * 获取指定类型的子执行
   */
  getChildrenByType(type: ExecutionType): ChildExecutionReference[] {
    return this.hierarchyManager.getChildren().filter(ref => ref.childType === type);
  }
  
  /**
   * 获取层级深度
   */
  getDepth(): number {
    return this.hierarchyManager.getDepth();
  }
  
  /**
   * 获取根执行ID
   */
  getRootExecutionId(): ID {
    return this.hierarchyManager.getRootExecutionId();
  }
  
  // ========== 向后兼容的API（保留）==========
  
  /**
   * @deprecated 使用 registerChild 替代
   */
  registerChildExecution(childExecutionId: ID): void {
    this.registerChild({
      childType: 'WORKFLOW',
      childId: childExecutionId,
      createdAt: Date.now(),
    });
  }
  
  /**
   * @deprecated 使用 getChildrenByType('WORKFLOW') 替代
   */
  getChildExecutionIds(): ID[] {
    return this.getChildrenByType('WORKFLOW').map(ref => ref.childId);
  }
  
  /**
   * @deprecated 使用 registerChild 替代
   */
  registerChildAgentLoop(agentLoopId: string): void {
    this.registerChild({
      childType: 'AGENT_LOOP',
      childId: agentLoopId,
      createdAt: Date.now(),
    });
  }
  
  /**
   * @deprecated 使用 getChildrenByType('AGENT_LOOP') 替代
   */
  getChildAgentLoopIds(): Set<string> {
    return new Set(
      this.getChildrenByType('AGENT_LOOP').map(ref => ref.childId)
    );
  }
}
```

#### **3.2.2 AgentLoopEntity 增强**

```typescript
// sdk/agent/entities/agent-loop-entity.ts

export class AgentLoopEntity {
  // ... existing fields ...
  
  /**
   * 执行层级管理器（新增）
   */
  private hierarchyManager: ExecutionHierarchyManager;
  
  constructor(id: string, config: AgentLoopRuntimeConfig, ...) {
    // ... existing initialization ...
    
    // 初始化层级管理器
    this.hierarchyManager = new ExecutionHierarchyManager(
      id,
      'AGENT_LOOP',
      undefined // AgentLoopExecution 可能还没有 hierarchy
    );
  }
  
  // ========== 统一的层级管理API（新增）==========
  
  /**
   * 设置父执行上下文
   */
  setParentContext(parentContext: ParentExecutionContext): void {
    this.hierarchyManager.setParent(parentContext);
    
    // 同时更新旧字段以保持兼容
    if (parentContext.parentType === 'WORKFLOW') {
      this.parentExecutionId = parentContext.parentId;
      this.nodeId = parentContext.nodeId;
    }
  }
  
  /**
   * 获取父执行上下文
   */
  getParentContext(): ParentExecutionContext | undefined {
    return this.hierarchyManager.getParent();
  }
  
  /**
   * 注册子执行（支持 Agent → Agent 和 Agent → Workflow）
   */
  registerChild(childRef: ChildExecutionReference): void {
    this.hierarchyManager.addChild(childRef);
  }
  
  /**
   * 注销子执行
   */
  unregisterChild(childId: ID, childType: ExecutionType): void {
    this.hierarchyManager.removeChild(childId, childType);
  }
  
  /**
   * 获取所有子执行
   */
  getChildren(): ChildExecutionReference[] {
    return this.hierarchyManager.getChildren();
  }
  
  /**
   * 获取子智能体执行IDs（新增：支持 Agent → Agent）
   */
  getChildAgentLoopIds(): Set<ID> {
    return new Set(
      this.getChildren()
        .filter(ref => ref.childType === 'AGENT_LOOP')
        .map(ref => ref.childId)
    );
  }
  
  /**
   * 获取子工作流执行IDs（新增：支持 Agent → Workflow）
   */
  getChildWorkflowExecutionIds(): Set<ID> {
    return new Set(
      this.getChildren()
        .filter(ref => ref.childType === 'WORKFLOW')
        .map(ref => ref.childId)
    );
  }
}
```

#### **3.2.3 层级管理器实现**

```typescript
// sdk/core/execution/execution-hierarchy-manager.ts

import type {
  ExecutionType,
  ParentExecutionContext,
  ChildExecutionReference,
  ExecutionHierarchyMetadata,
  ID,
} from '@wf-agent/types';

/**
 * 执行层级管理器
 * 
 * 统一管理父子关系的增删查改
 */
export class ExecutionHierarchyManager {
  private executionId: ID;
  private executionType: ExecutionType;
  private parent?: ParentExecutionContext;
  private children: Map<string, ChildExecutionReference> = new Map();
  private depth: number = 0;
  private rootExecutionId: ID;
  private rootExecutionType: ExecutionType;
  
  constructor(
    executionId: ID,
    executionType: ExecutionType,
    existingHierarchy?: ExecutionHierarchyMetadata
  ) {
    this.executionId = executionId;
    this.executionType = executionType;
    
    // 从现有元数据恢复状态
    if (existingHierarchy) {
      this.parent = existingHierarchy.parent;
      this.depth = existingHierarchy.depth;
      this.rootExecutionId = existingHierarchy.rootExecutionId;
      this.rootExecutionType = existingHierarchy.rootExecutionType;
      
      // 恢复子执行引用
      for (const child of existingHierarchy.children) {
        const key = `${child.childType}:${child.childId}`;
        this.children.set(key, child);
      }
    } else {
      // 新实例：自己是根节点
      this.rootExecutionId = executionId;
      this.rootExecutionType = executionType;
    }
  }
  
  /**
   * 设置父执行上下文
   */
  setParent(parentContext: ParentExecutionContext): void {
    this.parent = parentContext;
    
    // 重新计算深度和根节点
    this.recalculateHierarchy();
  }
  
  /**
   * 获取父执行上下文
   */
  getParent(): ParentExecutionContext | undefined {
    return this.parent;
  }
  
  /**
   * 添加子执行
   */
  addChild(childRef: ChildExecutionReference): void {
    const key = `${childRef.childType}:${childRef.childId}`;
    this.children.set(key, childRef);
  }
  
  /**
   * 移除子执行
   */
  removeChild(childId: ID, childType: ExecutionType): boolean {
    const key = `${childType}:${childId}`;
    return this.children.delete(key);
  }
  
  /**
   * 获取所有子执行
   */
  getChildren(): ChildExecutionReference[] {
    return Array.from(this.children.values());
  }
  
  /**
   * 获取层级深度
   */
  getDepth(): number {
    return this.depth;
  }
  
  /**
   * 获取根执行ID
   */
  getRootExecutionId(): ID {
    return this.rootExecutionId;
  }
  
  /**
   * 获取根执行类型
   */
  getRootExecutionType(): ExecutionType {
    return this.rootExecutionType;
  }
  
  /**
   * 转换为可序列化的元数据
   */
  toMetadata(): ExecutionHierarchyMetadata {
    return {
      parent: this.parent,
      children: this.getChildren(),
      depth: this.depth,
      rootExecutionId: this.rootExecutionId,
      rootExecutionType: this.rootExecutionType,
    };
  }
  
  /**
   * 重新计算层级信息
   * 
   * 当设置父节点时，需要递归计算深度和根节点
   */
  private recalculateHierarchy(): void {
    if (!this.parent) {
      // 无父节点：自己是根
      this.depth = 0;
      this.rootExecutionId = this.executionId;
      this.rootExecutionType = this.executionType;
    } else {
      // 有父节点：深度 = 父深度 + 1，根节点 = 父的根节点
      // 注意：这里需要外部传入父的层级信息，或者通过注册表查询
      // 简化实现：假设父的 depth 已知
      this.depth = this.calculateDepthFromParent();
      this.rootExecutionId = this.calculateRootFromParent();
      this.rootExecutionType = this.calculateRootTypeFromParent();
    }
  }
  
  private calculateDepthFromParent(): number {
    // TODO: 需要从父执行的层级管理器获取深度
    // 这需要注入 ExecutionHierarchyRegistry 或类似的全局注册表
    return 1; // 临时实现
  }
  
  private calculateRootFromParent(): ID {
    // TODO: 需要从父执行的层级管理器获取根ID
    return this.parent?.parentId ?? this.executionId;
  }
  
  private calculateRootTypeFromParent(): ExecutionType {
    // TODO: 需要从父执行的层级管理器获取根类型
    return this.parent?.parentType ?? this.executionType;
  }
}
```

### 3.3 注册表层设计

#### **3.3.1 统一的层级注册表**

```typescript
// sdk/core/execution/execution-hierarchy-registry.ts

import type { ID, ExecutionType } from '@wf-agent/types';
import type { WorkflowExecutionEntity } from '../../workflow/entities/workflow-execution-entity.js';
import type { AgentLoopEntity } from '../../agent/entities/agent-loop-entity.js';

/**
 * 执行实例联合类型
 */
export type AnyExecutionEntity = WorkflowExecutionEntity | AgentLoopEntity;

/**
 * 执行层级注册表
 * 
 * 全局管理所有执行实例的层级关系
 * 提供跨类型的查询和清理能力
 */
export class ExecutionHierarchyRegistry {
  private executions: Map<ID, AnyExecutionEntity> = new Map();
  
  /**
   * 注册执行实例
   */
  register(execution: AnyExecutionEntity): void {
    this.executions.set(execution.id, execution);
  }
  
  /**
   * 注销执行实例
   */
  unregister(executionId: ID): void {
    this.executions.delete(executionId);
  }
  
  /**
   * 获取执行实例
   */
  get(executionId: ID): AnyExecutionEntity | undefined {
    return this.executions.get(executionId);
  }
  
  /**
   * 获取某执行的所有子孙执行（递归）
   */
  getAllDescendants(executionId: ID, includeSelf: boolean = false): AnyExecutionEntity[] {
    const result: AnyExecutionEntity[] = [];
    
    if (includeSelf) {
      const self = this.get(executionId);
      if (self) result.push(self);
    }
    
    const entity = this.get(executionId);
    if (!entity) return result;
    
    // 获取直接子执行
    const children = this.getDirectChildren(executionId);
    
    // 递归获取每个子的后代
    for (const child of children) {
      result.push(child);
      result.push(...this.getAllDescendants(child.id, false));
    }
    
    return result;
  }
  
  /**
   * 获取某执行的直接子执行
   */
  getDirectChildren(executionId: ID): AnyExecutionEntity[] {
    const parent = this.get(executionId);
    if (!parent) return [];
    
    const children: AnyExecutionEntity[] = [];
    
    // 根据实体类型获取子执行
    if ('getChildren' in parent) {
      const childRefs = parent.getChildren();
      for (const ref of childRefs) {
        const child = this.get(ref.childId);
        if (child) children.push(child);
      }
    }
    
    return children;
  }
  
  /**
   * 清理执行及其所有子孙执行
   */
  cleanupHierarchy(executionId: ID): number {
    const descendants = this.getAllDescendants(executionId, true);
    let count = 0;
    
    for (const descendant of descendants) {
      // 停止执行（如果正在运行）
      if ('stop' in descendant && typeof descendant.stop === 'function') {
        descendant.stop();
      }
      
      // 清理资源
      if ('cleanup' in descendant && typeof descendant.cleanup === 'function') {
        descendant.cleanup();
      }
      
      // 从注册表移除
      this.unregister(descendant.id);
      count++;
    }
    
    return count;
  }
  
  /**
   * 获取某根执行下的所有执行（按类型分组）
   */
  getExecutionsByRoot(rootExecutionId: ID): {
    workflows: WorkflowExecutionEntity[];
    agents: AgentLoopEntity[];
  } {
    const allDescendants = this.getAllDescendants(rootExecutionId, true);
    
    return {
      workflows: allDescendants.filter(
        (e): e is WorkflowExecutionEntity => 'getWorkflowId' in e
      ),
      agents: allDescendants.filter(
        (e): e is AgentLoopEntity => 'conversationManager' in e
      ),
    };
  }
}
```

#### **3.3.2 增强现有注册表**

**AgentLoopRegistry 增强**:
```typescript
// sdk/agent/stores/agent-loop-registry.ts

export class AgentLoopRegistry {
  // ... existing methods ...
  
  /**
   * @deprecated 使用 ExecutionHierarchyRegistry 替代
   * 保留以保持向后兼容
   */
  getByParentExecutionId(executionId: ID): AgentLoopEntity[] {
    return this.getAll().filter(
      entity => entity.getParentContext()?.parentId === executionId
    );
  }
  
  /**
   * @deprecated 使用 ExecutionHierarchyRegistry 替代
   */
  cleanupByParentExecutionId(executionId: ID): number {
    const entities = this.getByParentExecutionId(executionId);
    for (const entity of entities) {
      if (entity.isRunning()) {
        entity.stop();
      }
      if (typeof entity.cleanup === 'function') {
        entity.cleanup();
      }
      this.unregister(entity.id);
    }
    return entities.length;
  }
  
  /**
   * 新增：获取以某Agent为父的所有子Agent
   */
  getChildrenByParentAgentLoopId(parentAgentLoopId: ID): AgentLoopEntity[] {
    return this.getAll().filter(entity => {
      const parent = entity.getParentContext();
      return parent?.parentType === 'AGENT_LOOP' && parent.parentId === parentAgentLoopId;
    });
  }
}
```

---

## 4. 迁移策略

### 4.1 分阶段实施

#### **阶段1：类型层准备（低风险）**
- ✅ 在 `@wf-agent/types` 中添加新的联合类型定义
- ✅ 保持旧字段不变，标记为 `@deprecated`
- ✅ 更新文档说明新类型的用法

**工作量**: 2-3天  
**风险**: 低（仅添加类型，不影响运行时）

#### **阶段2：SDK实体层增强（中风险）**
- 在 `WorkflowExecutionEntity` 和 `AgentLoopEntity` 中添加 `ExecutionHierarchyManager`
- 实现新旧API的双写逻辑（同时更新旧字段和新字段）
- 添加单元测试验证兼容性

**工作量**: 5-7天  
**风险**: 中（需要确保双写逻辑正确）

#### **阶段3：注册表整合（中风险）**
- 创建 `ExecutionHierarchyRegistry`
- 重构 `AgentLoopRegistry` 和 `WorkflowExecutionRegistry` 使用统一注册表
- 更新工厂方法和协调器使用新API

**工作量**: 7-10天  
**风险**: 中（涉及多处代码修改）

#### **阶段4：应用层适配（低风险）**
- 更新 apps 层代码使用新API
- 逐步替换旧的父子关系管理代码
- 集成测试验证完整流程

**工作量**: 3-5天  
**风险**: 低（应用层改动较小）

#### **阶段5：清理废弃代码（低风险）**
- 移除标记为 `@deprecated` 的旧字段和方法
- 简化实体类代码
- 最终版本发布

**工作量**: 2-3天  
**风险**: 低（确认无使用后删除）

### 4.2 向后兼容保证

1. **字段双写**: 新API写入时同时更新旧字段
2. **读取优先**: 优先从新字段读取，降级到旧字段
3. **渐进式迁移**: 允许新旧代码共存
4. **完整测试**: 确保现有功能不受影响

---

## 5. 使用示例

### 5.1 Workflow → Agent（现有场景，新API）

```typescript
// 创建工作流执行实例
const workflowEntity = await workflowBuilder.create(workflowId);

// 创建智能体执行实例（作为工作流的子执行）
const agentEntity = await AgentLoopFactory.create(agentConfig, {
  parentExecutionId: workflowEntity.id,
  nodeId: 'agent-node-1',
});

// 使用新API设置父子关系
agentEntity.setParentContext({
  parentType: 'WORKFLOW',
  parentId: workflowEntity.id,
  nodeId: 'agent-node-1',
});

// 注册子执行
workflowEntity.registerChild({
  childType: 'AGENT_LOOP',
  childId: agentEntity.id,
  createdAt: Date.now(),
});

// 查询子执行
const agentChildren = workflowEntity.getChildrenByType('AGENT_LOOP');
console.log(agentChildren); // [{ childType: 'AGENT_LOOP', childId: '...', ... }]
```

### 5.2 Agent → Agent（新增场景）

```typescript
// 主智能体
const mainAgent = await AgentLoopFactory.create(mainAgentConfig);

// 子智能体（由主智能体委派）
const subAgent = await AgentLoopFactory.create(subAgentConfig, {
  // 不再使用 parentExecutionId，改用统一的 parentContext
});

// 设置父子关系
subAgent.setParentContext({
  parentType: 'AGENT_LOOP',
  parentId: mainAgent.id,
  delegationPurpose: '代码审查任务委派',
});

// 注册子执行
mainAgent.registerChild({
  childType: 'AGENT_LOOP',
  childId: subAgent.id,
  createdAt: Date.now(),
});

// 查询子智能体
const childAgents = mainAgent.getChildAgentLoopIds();
console.log(childAgents); // Set { 'sub-agent-id' }

// 层级查询
const registry = new ExecutionHierarchyRegistry();
registry.register(mainAgent);
registry.register(subAgent);

const allDescendants = registry.getAllDescendants(mainAgent.id, true);
console.log(allDescendants.length); // 2 (mainAgent + subAgent)
```

### 5.3 Agent → Workflow（新增场景）

```typescript
// 智能体调用工作流
const agent = await AgentLoopFactory.create(agentConfig);
const workflow = await workflowBuilder.create(workflowId);

// 设置工作流的父级为智能体
workflow.setParentContext({
  parentType: 'AGENT_LOOP',
  parentId: agent.id,
});

// 注册子执行
agent.registerChild({
  childType: 'WORKFLOW',
  childId: workflow.id,
  createdAt: Date.now(),
});

// 混合查询
const children = agent.getChildren();
console.log(children);
// [
//   { childType: 'AGENT_LOOP', childId: 'sub-agent-1', ... },
//   { childType: 'WORKFLOW', childId: 'workflow-1', ... }
// ]
```

### 5.4 层级清理

```typescript
// 清理整个层级树
const registry = new ExecutionHierarchyRegistry();
registry.register(workflow);
registry.register(agent1);
registry.register(agent2);
registry.register(subWorkflow);

// 清理 workflow 及其所有子孙执行
const cleanedCount = registry.cleanupHierarchy(workflow.id);
console.log(`Cleaned ${cleanedCount} executions`);
// Cleaned 4 executions
```

---

## 6. 优势分析

### 6.1 类型安全

✅ **编译时验证**:
```typescript
// ✅ 正确：类型匹配
agent.setParentContext({
  parentType: 'AGENT_LOOP',
  parentId: 'agent-123',
});

// ❌ 编译错误：缺少必需字段
agent.setParentContext({
  parentType: 'AGENT_LOOP',
  // parentId 缺失
});

// ❌ 编译错误：无效的类型值
agent.setParentContext({
  parentType: 'INVALID_TYPE', // TypeScript 报错
  parentId: 'agent-123',
});
```

### 6.2 统一API

✅ **一致的接口**:
```typescript
// Workflow 和 Agent 使用相同的API
workflowEntity.registerChild(childRef);
agentEntity.registerChild(childRef);

// 统一的查询方法
workflowEntity.getChildren();
agentEntity.getChildren();
```

### 6.3 扩展性

✅ **新增执行类型只需扩展联合类型**:
```typescript
// 未来添加工具执行类型
export type ExecutionType = 'WORKFLOW' | 'AGENT_LOOP' | 'TOOL_EXECUTION';

export type ChildExecutionReference =
  | { childType: 'WORKFLOW'; ... }
  | { childType: 'AGENT_LOOP'; ... }
  | { childType: 'TOOL_EXECUTION'; ... }; // 新增
```

### 6.4 层级追踪

✅ **完整的层级树支持**:
```typescript
// 查询任意节点的完整层级信息
const metadata = entity.hierarchyManager.toMetadata();
console.log(metadata.depth); // 2
console.log(metadata.rootExecutionId); // 'root-workflow-id'
console.log(metadata.children.length); // 3
```

---

## 7. 潜在问题与解决方案

### 7.1 循环引用检测

**问题**: Agent A → Agent B → Agent A 形成循环

**解决方案**:
```typescript
class ExecutionHierarchyManager {
  setParent(parentContext: ParentExecutionContext): void {
    // 检测循环引用
    if (this.wouldCreateCycle(parentContext.parentId)) {
      throw new Error(
        `Circular reference detected: ${parentContext.parentId} -> ... -> ${this.executionId}`
      );
    }
    this.parent = parentContext;
  }
  
  private wouldCreateCycle(targetId: ID): boolean {
    // 递归检查 targetId 是否是当前执行的祖先
    let current = this.parent;
    while (current) {
      if (current.parentId === targetId) {
        return true;
      }
      // 需要从注册表获取父的父节点
      current = this.getParentOf(current.parentId);
    }
    return false;
  }
}
```

### 7.2 深度限制

**问题**: 过深的嵌套导致性能问题

**解决方案**:
```typescript
const MAX_DEPTH = 10; // 配置化

class ExecutionHierarchyManager {
  setParent(parentContext: ParentExecutionContext): void {
    const parentDepth = this.getParentDepth(parentContext.parentId);
    if (parentDepth + 1 > MAX_DEPTH) {
      throw new Error(
        `Maximum depth exceeded: ${parentDepth + 1} > ${MAX_DEPTH}`
      );
    }
    // ...
  }
}
```

### 7.3 序列化复杂性

**问题**: `ExecutionHierarchyMetadata` 包含引用，序列化时需要特殊处理

**解决方案**:
```typescript
// 序列化时只保存ID引用，不保存完整对象
interface SerializableHierarchyMetadata {
  parent?: {
    parentType: ExecutionType;
    parentId: ID;
    nodeId?: ID;
  };
  children: Array<{
    childType: ExecutionType;
    childId: ID;
    createdAt: Timestamp;
  }>;
  depth: number;
  rootExecutionId: ID;
  rootExecutionType: ExecutionType;
}

// 反序列化时从注册表重建引用
static fromSerializable(
  data: SerializableHierarchyMetadata,
  registry: ExecutionHierarchyRegistry
): ExecutionHierarchyMetadata {
  // 从注册表获取实际的执行实例
  // ...
}
```

---

## 8. 总结与建议

### 8.1 核心价值

1. **统一模型**: 用联合类型统一管理所有父子关系
2. **类型安全**: 编译时验证，减少运行时错误
3. **扩展性强**: 新增执行类型无需修改现有代码
4. **向后兼容**: 渐进式迁移，不影响现有功能

### 8.2 实施建议

**优先级排序**:
1. 🔴 **高优先级**: 类型层设计（阶段1）
2. 🟡 **中优先级**: SDK实体层增强（阶段2）
3. 🟢 **低优先级**: 注册表整合（阶段3-5）

**理由**:
- 类型层改动风险最低，收益最高
- 可以先让团队熟悉新类型，再逐步迁移实现
- 旧API可以长期保留，不需要急于清理

### 8.3 下一步行动

1. ✅ 评审本设计文档
2. ✅ 在 `packages/types` 中实现类型定义
3. ✅ 编写类型层的单元测试
4. ✅ 在小范围内试点新API（如新增的 Agent → Agent 场景）
5. ✅ 收集反馈，迭代优化
6. ✅ 全面推广到现有代码

---

## 附录：类型定义完整示例

```typescript
// packages/types/src/execution/hierarchy.ts

import type { ID, Timestamp } from '../common.js';

/**
 * 执行实例类型
 */
export type ExecutionType = 'WORKFLOW' | 'AGENT_LOOP';

/**
 * 父执行上下文（联合类型）
 */
export type ParentExecutionContext =
  | {
      parentType: 'WORKFLOW';
      parentId: ID;
      nodeId?: ID;
    }
  | {
      parentType: 'AGENT_LOOP';
      parentId: ID;
      delegationPurpose?: string;
    };

/**
 * 子执行引用（联合类型）
 */
export type ChildExecutionReference =
  | {
      childType: 'WORKFLOW';
      childId: ID;
      createdAt: Timestamp;
    }
  | {
      childType: 'AGENT_LOOP';
      childId: ID;
      createdAt: Timestamp;
    };

/**
 * 执行层级元数据
 */
export interface ExecutionHierarchyMetadata {
  parent?: ParentExecutionContext;
  children: ChildExecutionReference[];
  depth: number;
  rootExecutionId: ID;
  rootExecutionType: ExecutionType;
}
```
