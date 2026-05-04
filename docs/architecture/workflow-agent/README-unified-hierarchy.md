# 统一父子关系架构设计 - 执行摘要

## 📋 文档概述

本文档提出了使用 **TypeScript 联合类型（Union Types）** 统一管理系统中所有执行实例父子关系的架构设计方案。

**完整设计文档**: [unified-parent-child-hierarchy-design.md](./unified-parent-child-hierarchy-design.md)

---

## 🎯 核心问题

当前系统存在三种独立的父子关系管理机制：

1. **Workflow → Workflow**: 通过 `triggeredSubworkflowContext` 管理
2. **Workflow → Agent**: 通过 `childAgentLoopIds` 管理  
3. **Agent → Agent**: ❌ **不支持**（需要新增）

**痛点**:
- 类型不一致（数组 vs 集合）
- 管理分散，缺乏统一接口
- 扩展性差，新增类型需修改多处代码
- 缺少强类型约束

---

## 💡 解决方案

### 核心设计：联合类型统一管理

```typescript
// 父执行上下文（区分不同类型的父级）
export type ParentExecutionContext =
  | { parentType: 'WORKFLOW'; parentId: ID; nodeId?: ID }
  | { parentType: 'AGENT_LOOP'; parentId: ID; delegationPurpose?: string };

// 子执行引用（区分不同类型的子级）
export type ChildExecutionReference =
  | { childType: 'WORKFLOW'; childId: ID; createdAt: Timestamp }
  | { childType: 'AGENT_LOOP'; childId: ID; createdAt: Timestamp };

// 层级元数据（附加到每个执行实例）
export interface ExecutionHierarchyMetadata {
  parent?: ParentExecutionContext;
  children: ChildExecutionReference[];
  depth: number;
  rootExecutionId: ID;
  rootExecutionType: ExecutionType;
}
```

### 支持的场景

| 场景 | 父类型 | 子类型 | 状态 |
|------|--------|--------|------|
| Workflow → Workflow | WorkflowExecution | WorkflowExecution | ✅ 已有，需迁移 |
| Workflow → Agent | WorkflowExecution | AgentLoopExecution | ✅ 已有，需迁移 |
| **Agent → Agent** | **AgentLoopExecution** | **AgentLoopExecution** | 🔴 **新增** |
| **Agent → Workflow** | **AgentLoopExecution** | **WorkflowExecution** | 🔴 **新增** |

---

## 🏗️ 架构层次

### 1. 类型层（@wf-agent/types）
- 定义联合类型 `ParentExecutionContext` 和 `ChildExecutionReference`
- 定义元数据接口 `ExecutionHierarchyMetadata`
- 增强现有 `WorkflowExecution` 和 `AgentLoopExecution` 类型

### 2. SDK实体层
- `WorkflowExecutionEntity` 和 `AgentLoopEntity` 添加 `ExecutionHierarchyManager`
- 提供统一的API：`setParentContext()`, `registerChild()`, `getChildren()`
- 保留旧API标记为 `@deprecated` 以保持向后兼容

### 3. 注册表层
- 创建 `ExecutionHierarchyRegistry` 全局管理所有层级关系
- 提供跨类型查询：`getAllDescendants()`, `cleanupHierarchy()`
- 增强现有注册表使用新API

---

## 🚀 实施计划

### 阶段1：类型层准备（2-3天，低风险）
- ✅ 在 `packages/types` 中添加新类型定义
- ✅ 保持旧字段不变，标记为 `@deprecated`
- ✅ 更新文档

### 阶段2：SDK实体层增强（5-7天，中风险）
- 在实体类中添加 `ExecutionHierarchyManager`
- 实现新旧API双写逻辑
- 添加单元测试

### 阶段3：注册表整合（7-10天，中风险）
- 创建 `ExecutionHierarchyRegistry`
- 重构现有注册表
- 更新工厂方法和协调器

### 阶段4：应用层适配（3-5天，低风险）
- 更新 apps 层代码
- 集成测试验证

### 阶段5：清理废弃代码（2-3天，低风险）
- 移除 `@deprecated` 字段和方法
- 最终版本发布

**总工作量**: 19-28天

---

## ✨ 核心优势

### 1. 类型安全
```typescript
// ✅ 编译时验证
agent.setParentContext({
  parentType: 'AGENT_LOOP',
  parentId: 'agent-123',
});

// ❌ 编译错误
agent.setParentContext({
  parentType: 'INVALID_TYPE', // TypeScript 报错
  parentId: 'agent-123',
});
```

### 2. 统一API
```typescript
// Workflow 和 Agent 使用相同的API
workflowEntity.registerChild(childRef);
agentEntity.registerChild(childRef);
```

### 3. 扩展性强
```typescript
// 未来添加工具执行类型只需扩展联合类型
export type ExecutionType = 'WORKFLOW' | 'AGENT_LOOP' | 'TOOL_EXECUTION';
```

### 4. 完整层级追踪
```typescript
const metadata = entity.hierarchyManager.toMetadata();
console.log(metadata.depth); // 2
console.log(metadata.rootExecutionId); // 'root-workflow-id'
```

---

## 🔧 使用示例

### Agent → Agent（新增能力）

```typescript
// 主智能体
const mainAgent = await AgentLoopFactory.create(mainAgentConfig);

// 子智能体
const subAgent = await AgentLoopFactory.create(subAgentConfig);

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
```

### 层级清理

```typescript
const registry = new ExecutionHierarchyRegistry();
registry.register(workflow);
registry.register(agent1);
registry.register(agent2);

// 清理 workflow 及其所有子孙执行
const cleanedCount = registry.cleanupHierarchy(workflow.id);
```

---

## ⚠️ 潜在问题与解决

### 1. 循环引用检测
```typescript
setParent(parentContext: ParentExecutionContext): void {
  if (this.wouldCreateCycle(parentContext.parentId)) {
    throw new Error('Circular reference detected');
  }
  // ...
}
```

### 2. 深度限制
```typescript
const MAX_DEPTH = 10; // 配置化

if (parentDepth + 1 > MAX_DEPTH) {
  throw new Error(`Maximum depth exceeded`);
}
```

### 3. 序列化复杂性
- 只保存ID引用，不保存完整对象
- 反序列化时从注册表重建引用

---

## 📊 对比分析

| 维度 | 当前架构 | 新架构 |
|------|---------|--------|
| 类型一致性 | ❌ 数组/集合混用 | ✅ 统一联合类型 |
| 类型安全 | ❌ 弱类型字符串 | ✅ 编译时验证 |
| API一致性 | ❌ 分散的方法 | ✅ 统一接口 |
| 扩展性 | ❌ 需修改多处 | ✅ 扩展联合类型 |
| Agent→Agent | ❌ 不支持 | ✅ 原生支持 |
| 层级追踪 | ❌ 仅一层 | ✅ 完整树结构 |
| 向后兼容 | N/A | ✅ 渐进式迁移 |

---

## 🎯 建议与下一步

### 优先级排序
1. 🔴 **高优先级**: 类型层设计（阶段1）- 风险最低，收益最高
2. 🟡 **中优先级**: SDK实体层增强（阶段2）
3. 🟢 **低优先级**: 注册表整合（阶段3-5）

### 立即行动
1. ✅ 评审本设计文档
2. ✅ 在 `packages/types` 中实现类型定义
3. ✅ 编写类型层的单元测试
4. ✅ 在小范围内试点新API（如新增的 Agent → Agent 场景）
5. ✅ 收集反馈，迭代优化

---

## 📚 相关文档

- [完整设计文档](./unified-parent-child-hierarchy-design.md)
- [Agent父子关系类型定义](../../sdk/docs/agent-parent-child-relationship-types.md)
- [Workflow执行架构](../../sdk/docs/data-flow/workflow-execution.md)

---

**文档版本**: v1.0  
**创建日期**: 2026-05-04  
**作者**: AI Assistant  
**状态**: 待评审
