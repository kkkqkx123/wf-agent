# 统一父子关系架构 - 可视化图解

## 1. 当前架构 vs 新架构对比

### 1.1 当前架构（分散管理）

```
┌─────────────────────────────────────────────────────────┐
│                    WorkflowExecution                     │
│                                                          │
│  triggeredSubworkflowContext: {                         │
│    parentExecutionId: ID                                │
│    childExecutionIds: ID[]          ← 数组              │
│  }                                                       │
│                                                          │
│  childAgentLoopIds: Set<string>     ← 集合              │
└─────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────┐
│                   AgentLoopEntity                        │
│                                                          │
│  parentExecutionId?: ID             ← 独立字段          │
│  nodeId?: ID                        ← 独立字段          │
│                                                          │
│  ❌ 无法追踪 Agent → Agent 关系                          │
└─────────────────────────────────────────────────────────┘

问题：
- 类型不一致（数组 vs 集合 vs 独立字段）
- 管理分散，缺乏统一接口
- 无法支持 Agent → Agent
```

### 1.2 新架构（统一管理）

```
┌─────────────────────────────────────────────────────────┐
│              ExecutionHierarchyMetadata                  │
│                                                          │
│  parent?: ParentExecutionContext     ← 联合类型         │
│    ├─ { parentType: 'WORKFLOW', parentId, nodeId? }    │
│    └─ { parentType: 'AGENT_LOOP', parentId, purpose? } │
│                                                          │
│  children: ChildExecutionReference[] ← 统一数组         │
│    ├─ { childType: 'WORKFLOW', childId, createdAt }    │
│    └─ { childType: 'AGENT_LOOP', childId, createdAt }  │
│                                                          │
│  depth: number                                           │
│  rootExecutionId: ID                                     │
│  rootExecutionType: ExecutionType                        │
└─────────────────────────────────────────────────────────┘
                          ↓
        附加到所有执行实例（Workflow & Agent）

优势：
- ✅ 类型一致（统一联合类型）
- ✅ 统一管理（单一元数据结构）
- ✅ 支持所有场景（包括 Agent → Agent）
- ✅ 完整层级追踪（depth, root）
```

---

## 2. 支持的场景矩阵

```
                    子执行类型
                ┌──────────┬──────────┐
                │ Workflow │  Agent   │
      ┌─────────┼──────────┼──────────┤
      │         │          │          │
父    │Workflow │    ✅    │    ✅    │
执    │         │ 已有     │ 已有     │
行    ├─────────┼──────────┼──────────┤
类    │         │          │          │
型    │ Agent   │  🔴新增  │  🔴新增  │
      │         │          │          │
      └─────────┴──────────┴──────────┘

✅ = 当前已支持（需迁移到新API）
🔴 = 新增能力（设计重点）
```

---

## 3. 层级树示例

### 3.1 复杂层级场景

```
Root Workflow (depth=0)
├─ Sub-Workflow 1 (depth=1)
│  ├─ Agent Loop A (depth=2)
│  │  └─ Sub-Agent B (depth=3)        ← 新增能力
│  └─ Agent Loop C (depth=2)
│     └─ Sub-Workflow 2 (depth=3)     ← 新增能力
└─ Sub-Workflow 3 (depth=1)
   └─ Agent Loop D (depth=2)

层级查询示例：
- Root.getAllDescendants() → [Sub1, AgentA, SubAgentB, AgentC, Sub2, Sub3, AgentD]
- AgentA.getChildren() → [SubAgentB]
- SubAgentB.getParentContext() → { parentType: 'AGENT_LOOP', parentId: 'AgentA' }
```

### 3.2 数据流示例

```typescript
// 1. 创建根工作流
const rootWorkflow = await builder.create('root-wf');
// hierarchy: { depth: 0, rootExecutionId: 'root-wf', children: [] }

// 2. 创建子工作流
const subWorkflow = await builder.create('sub-wf');
subWorkflow.setParentContext({
  parentType: 'WORKFLOW',
  parentId: 'root-wf',
});
rootWorkflow.registerChild({
  childType: 'WORKFLOW',
  childId: 'sub-wf',
  createdAt: Date.now(),
});
// subWorkflow hierarchy: { depth: 1, rootExecutionId: 'root-wf', ... }

// 3. 在子工作流中创建智能体
const agent = await AgentLoopFactory.create(config, {
  parentExecutionId: 'sub-wf',
  nodeId: 'agent-node-1',
});
agent.setParentContext({
  parentType: 'WORKFLOW',
  parentId: 'sub-wf',
  nodeId: 'agent-node-1',
});
subWorkflow.registerChild({
  childType: 'AGENT_LOOP',
  childId: agent.id,
  createdAt: Date.now(),
});
// agent hierarchy: { depth: 2, rootExecutionId: 'root-wf', ... }

// 4. 智能体委派给子智能体（新增）
const subAgent = await AgentLoopFactory.create(subConfig);
subAgent.setParentContext({
  parentType: 'AGENT_LOOP',
  parentId: agent.id,
  delegationPurpose: '代码审查',
});
agent.registerChild({
  childType: 'AGENT_LOOP',
  childId: subAgent.id,
  createdAt: Date.now(),
});
// subAgent hierarchy: { depth: 3, rootExecutionId: 'root-wf', ... }
```

---

## 4. 类型流转图

```
┌──────────────────────────────────────────────────────────┐
│                   类型定义层 (@wf-agent/types)            │
│                                                           │
│  export type ExecutionType = 'WORKFLOW' | 'AGENT_LOOP';  │
│                                                           │
│  export type ParentExecutionContext =                    │
│    | { parentType: 'WORKFLOW'; parentId; nodeId? }       │
│    | { parentType: 'AGENT_LOOP'; parentId; purpose? };   │
│                                                           │
│  export type ChildExecutionReference =                   │
│    | { childType: 'WORKFLOW'; childId; createdAt }       │
│    | { childType: 'AGENT_LOOP'; childId; createdAt };    │
│                                                           │
│  export interface ExecutionHierarchyMetadata {           │
│    parent?: ParentExecutionContext;                      │
│    children: ChildExecutionReference[];                  │
│    depth: number;                                        │
│    rootExecutionId: ID;                                  │
│    rootExecutionType: ExecutionType;                     │
│  }                                                        │
└──────────────────────────────────────────────────────────┘
                           ↓ 使用
┌──────────────────────────────────────────────────────────┐
│                 SDK 实体层 (sdk/)                         │
│                                                           │
│  class WorkflowExecutionEntity {                         │
│    private hierarchyManager: ExecutionHierarchyManager;  │
│                                                           │
│    setParentContext(ctx: ParentExecutionContext): void   │
│    registerChild(ref: ChildExecutionReference): void     │
│    getChildren(): ChildExecutionReference[]              │
│    getChildrenByType(type): ChildExecutionReference[]    │
│  }                                                        │
│                                                           │
│  class AgentLoopEntity {                                 │
│    private hierarchyManager: ExecutionHierarchyManager;  │
│                                                           │
│    setParentContext(ctx: ParentExecutionContext): void   │
│    registerChild(ref: ChildExecutionReference): void     │
│    getChildAgentLoopIds(): Set<ID>                       │
│    getChildWorkflowExecutionIds(): Set<ID>               │
│  }                                                        │
└──────────────────────────────────────────────────────────┘
                           ↓ 管理
┌──────────────────────────────────────────────────────────┐
│               注册表层 (sdk/core/)                        │
│                                                           │
│  class ExecutionHierarchyRegistry {                      │
│    private executions: Map<ID, AnyExecutionEntity>;      │
│                                                           │
│    register(execution): void                             │
│    getAllDescendants(id): AnyExecutionEntity[]           │
│    cleanupHierarchy(id): number                          │
│    getExecutionsByRoot(id): { workflows, agents }        │
│  }                                                        │
└──────────────────────────────────────────────────────────┘
```

---

## 5. API 对比

### 5.1 旧 API（分散）

```typescript
// Workflow → Workflow
workflow.registerChildExecution(childId);
workflow.getChildExecutionIds(); // ID[]

// Workflow → Agent
workflow.registerChildAgentLoop(agentId);
workflow.getChildAgentLoopIds(); // Set<string>

// Agent 获取父级
agent.parentExecutionId; // ID | undefined
agent.nodeId; // ID | undefined

// 查询
registry.getByParentExecutionId(executionId);
registry.cleanupByParentExecutionId(executionId);
```

**问题**: 
- ❌ 方法名不一致
- ❌ 返回类型不一致（数组 vs 集合）
- ❌ 无法区分父级类型
- ❌ 不支持 Agent → Agent

### 5.2 新 API（统一）

```typescript
// 所有场景统一 API
entity.setParentContext({
  parentType: 'WORKFLOW' | 'AGENT_LOOP',
  parentId: ID,
  nodeId?: ID,
  delegationPurpose?: string,
});

entity.registerChild({
  childType: 'WORKFLOW' | 'AGENT_LOOP',
  childId: ID,
  createdAt: Timestamp,
});

entity.getChildren(); // ChildExecutionReference[]
entity.getChildrenByType('WORKFLOW'); // ChildExecutionReference[]
entity.getChildrenByType('AGENT_LOOP'); // ChildExecutionReference[]

// 层级查询
registry.getAllDescendants(executionId);
registry.cleanupHierarchy(executionId);
registry.getExecutionsByRoot(rootId);
```

**优势**:
- ✅ 统一的接口风格
- ✅ 统一的返回类型
- ✅ 强类型区分父/子类型
- ✅ 支持所有场景

---

## 6. 迁移路径

```
阶段1: 类型层准备
┌─────────────────────────────────────┐
│ • 添加新类型定义                     │
│ • 标记旧字段 @deprecated             │
│ • 新旧字段共存                       │
└─────────────────────────────────────┘
            ↓
阶段2: SDK 实体层增强
┌─────────────────────────────────────┐
│ • 添加 ExecutionHierarchyManager    │
│ • 实现双写逻辑（新旧字段同步更新）    │
│ • 新 API 优先读取，降级到旧字段       │
└─────────────────────────────────────┘
            ↓
阶段3: 注册表整合
┌─────────────────────────────────────┐
│ • 创建 ExecutionHierarchyRegistry   │
│ • 重构现有注册表使用新 API           │
│ • 保留旧方法作为兼容层               │
└─────────────────────────────────────┘
            ↓
阶段4: 应用层适配
┌─────────────────────────────────────┐
│ • 更新 apps 层代码                   │
│ • 逐步替换旧 API 调用                │
│ • 集成测试验证                       │
└─────────────────────────────────────┘
            ↓
阶段5: 清理废弃代码
┌─────────────────────────────────────┐
│ • 移除 @deprecated 字段和方法        │
│ • 简化实体类代码                     │
│ • 最终版本发布                       │
└─────────────────────────────────────┘
```

---

## 7. 错误处理流程

```typescript
// 循环引用检测
try {
  agentA.setParentContext({
    parentType: 'AGENT_LOOP',
    parentId: agentB.id,
  });
  
  agentB.setParentContext({
    parentType: 'AGENT_LOOP',
    parentId: agentA.id, // ❌ 检测到循环
  });
} catch (error) {
  // Error: Circular reference detected: agentB -> agentA -> agentB
}

// 深度限制
try {
  // depth=0 → depth=1 → depth=2 → ... → depth=11
  deepAgent.setParentContext({
    parentType: 'AGENT_LOOP',
    parentId: parent.id,
  });
} catch (error) {
  // Error: Maximum depth exceeded: 11 > 10
}

// 类型验证（编译时）
agent.setParentContext({
  parentType: 'INVALID_TYPE', // ❌ TypeScript 编译错误
  parentId: 'agent-123',
});
```

---

## 8. 性能考虑

### 8.1 查询复杂度

| 操作 | 复杂度 | 说明 |
|------|--------|------|
| `getChildren()` | O(1) | 直接从 metadata.children 读取 |
| `getParentContext()` | O(1) | 直接从 metadata.parent 读取 |
| `getAllDescendants()` | O(n) | n = 子孙节点数量，需要递归遍历 |
| `cleanupHierarchy()` | O(n) | n = 子孙节点数量，需要递归清理 |
| `wouldCreateCycle()` | O(d) | d = 层级深度，向上遍历祖先链 |

### 8.2 优化策略

```typescript
// 缓存层级信息
class ExecutionHierarchyManager {
  private depthCache?: number;
  private rootCache?: { id: ID; type: ExecutionType };
  
  getDepth(): number {
    if (this.depthCache === undefined) {
      this.depthCache = this.calculateDepth();
    }
    return this.depthCache;
  }
  
  // 当父节点变化时清除缓存
  setParent(parentContext: ParentExecutionContext): void {
    this.parent = parentContext;
    this.depthCache = undefined; // 清除缓存
    this.rootCache = undefined;  // 清除缓存
  }
}
```

---

## 9. 实际应用场景

### 场景1: 代码审查工作流

```
Main Workflow (代码审查流程)
├─ Review Agent (主审查智能体)
│  ├─ Security Agent (安全检查)        ← Agent → Agent
│  ├─ Performance Agent (性能检查)     ← Agent → Agent
│  └─ Style Agent (代码风格检查)       ← Agent → Agent
└─ Report Workflow (生成报告)          ← Agent → Workflow

优势：
- 每个专用智能体独立配置（不同模型、工具集）
- 主智能体可以并行委派多个子任务
- 完整的执行追踪和审计日志
```

### 场景2: 数据分析管道

```
Data Pipeline Workflow
├─ Extract Agent (数据提取)
│  └─ Web Scraping Agent (网页抓取)   ← Agent → Agent
├─ Transform Workflow (数据转换)
│  └─ Validation Agent (数据验证)     ← Workflow → Agent
└─ Load Agent (数据加载)
   └─ Database Workflow (数据库操作)  ← Agent → Workflow

优势：
- 混合编排工作流和智能体
- 灵活的任务分解和委派
- 统一的错误处理和重试机制
```

### 场景3: 客户服务系统

```
Customer Service Agent
├─ Intent Classification Workflow (意图分类)
├─ Response Generation Agent (响应生成)
│  ├─ Knowledge Base Agent (知识库查询)  ← Agent → Agent
│  └─ Sentiment Analysis Agent (情感分析) ← Agent → Agent
└─ Escalation Workflow (升级处理)

优势：
- 动态任务路由
- 多层级的智能决策
- 完整的对话历史追踪
```

---

## 10. 总结

```
┌─────────────────────────────────────────────────────────┐
│                   核心价值总结                           │
│                                                          │
│  ✅ 类型安全: 编译时验证，减少运行时错误                  │
│  ✅ 统一管理: 单一接口管理所有父子关系                    │
│  ✅ 扩展性强: 新增类型只需扩展联合类型                    │
│  ✅ 完整追踪: 支持任意深度的层级树                        │
│  ✅ 向后兼容: 渐进式迁移，不影响现有功能                  │
│  ✅ 新增能力: 原生支持 Agent → Agent 和 Agent → Workflow │
└─────────────────────────────────────────────────────────┘
```

---

**图表版本**: v1.0  
**创建日期**: 2026-05-04  
**配套文档**: 
- [完整设计文档](./unified-parent-child-hierarchy-design.md)
- [执行摘要](./README-unified-hierarchy.md)
