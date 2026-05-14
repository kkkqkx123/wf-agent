# WorkflowGraph StaticNodeType 集成改进

## 概述

本次改进解决了 `workflow-graph.ts` 文件中完全绕过 `StaticNodeType` 类型系统的问题，增强了类型安全性和代码一致性。

## 问题分析

### 原有问题

1. **类型不安全**：`nodeConfigs` 和 `triggerConfigs` 使用 `Map<ID, unknown>` 类型
2. **未利用类型系统**：导入了 `StaticNodeType` 但未实际使用
3. **与项目规范不一致**：其他模块广泛使用 `StaticNodeType` 进行类型检查
4. **缺少类型守卫**：没有提供基于 `StaticNodeType` 的便捷查询方法

### 影响范围

- 类型安全性降低，容易引入运行时错误
- 代码可维护性下降，需要额外的类型断言
- 与其他模块的模式不一致，增加理解成本

## 改进内容

### 1. 类型定义更新

#### packages/types/src/graph/preprocessed-graph.ts

```typescript
// 之前
nodeConfigs: Map<ID, unknown>;
triggerConfigs: Map<ID, unknown>;

// 之后
nodeConfigs: Map<ID, StaticNode>;
triggerConfigs: Map<ID, WorkflowTrigger>;
```

**优势：**
- 编译时类型检查
- IDE 智能提示支持
- 减少运行时错误

### 2. 实现类增强

#### sdk/workflow/entities/workflow-graph.ts

##### 2.1 字段类型修正

```typescript
public nodeConfigs: Map<ID, StaticNode>;
public triggerConfigs: Map<ID, WorkflowTrigger>;
```

##### 2.2 新增工具方法

###### getNodeConfig(nodeId: ID): StaticNode | undefined

获取指定节点的静态配置。

```typescript
const config = graph.getNodeConfig("node-1");
if (config) {
  console.log(config.type); // TypeScript 知道这是 StaticNode
}
```

###### getNodeConfigByType<T extends StaticNodeType>(nodeId: ID, nodeType: T)

类型安全的节点配置获取，带有类型守卫。

```typescript
const startNode = graph.getNodeConfigByType("start-1", "START");
if (startNode) {
  // TypeScript 知道这是 StartNode 类型
  console.log(startNode.config); // 自动推断为 WorkflowStartConfig
}
```

###### isNodeOfType(nodeId: ID, nodeType: StaticNodeType): boolean

检查节点是否存在且为指定类型。

```typescript
if (graph.isNodeOfType("node-1", "FORK")) {
  // 安全地执行 FORK 相关操作
}
```

###### getNodeIdsByType(nodeType: StaticNodeType): ID[]

获取指定类型的所有节点 ID。

```typescript
const scriptNodes = graph.getNodeIdsByType("SCRIPT");
// 返回: ["script-1", "script-2", ...]
```

###### getTriggerConfig(triggerId: ID): WorkflowTrigger | undefined

获取触发器配置。

```typescript
const trigger = graph.getTriggerConfig("trigger-1");
if (trigger) {
  console.log(trigger.condition); // 类型安全访问
}
```

###### setNodeConfig / setTriggerConfig

类型安全的配置设置方法。

```typescript
graph.setNodeConfig("node-1", staticNode);
graph.setTriggerConfig("trigger-1", trigger);
```

## 使用示例

### 示例 1：类型安全的节点查询

```typescript
import { WorkflowGraph } from "@wf-agent/sdk";
import type { ForkNode } from "@wf-agent/types";

const graph = new WorkflowGraph();

// 存储节点配置
const forkNode: ForkNode = {
  id: "fork-1",
  type: "FORK",
  name: "Parallel Execution",
  config: {
    forkPaths: [
      { pathId: "path-1", childNodeId: "node-a" },
      { pathId: "path-2", childNodeId: "node-b" },
    ],
  },
};

graph.setNodeConfig(forkNode.id, forkNode);

// 类型安全的检索
const retrievedFork = graph.getNodeConfigByType("fork-1", "FORK");
if (retrievedFork) {
  // TypeScript 知道这是 ForkNode 类型
  const paths = retrievedFork.config.forkPaths; // ✅ 类型安全
  console.log(`Found ${paths.length} fork paths`);
}
```

### 示例 2：批量处理特定类型的节点

```typescript
// 获取所有 SCRIPT 节点
const scriptNodeIds = graph.getNodeIdsByType("SCRIPT");

for (const nodeId of scriptNodeIds) {
  const config = graph.getNodeConfigByType(nodeId, "SCRIPT");
  if (config) {
    // 类型安全地访问 ScriptNode 特有属性
    console.log(`${config.name}: ${config.config.language}`);
  }
}
```

### 示例 3：条件判断与类型守卫

```typescript
const nodeId = "some-node";

if (graph.isNodeOfType(nodeId, "LLM")) {
  const llmNode = graph.getNodeConfigByType(nodeId, "LLM");
  if (llmNode) {
    // TypeScript 知道这是 LLMNode 类型
    const model = llmNode.config.model;
    const temperature = llmNode.config.temperature;
    // ... 执行 LLM 相关逻辑
  }
}
```

## 测试覆盖

创建了完整的单元测试套件：`workflow-graph-static-node-type.test.ts`

### 测试用例

1. ✅ nodeConfigs 类型安全性测试
   - 存储和检索 StaticNode 配置
   - 处理不存在的节点

2. ✅ getNodeConfigByType 类型守卫功能
   - 类型匹配时返回正确类型
   - 类型不匹配时返回 undefined

3. ✅ isNodeOfType 类型检查
   - 节点存在且类型匹配
   - 类型不匹配
   - 节点不存在

4. ✅ getNodeIdsByType 过滤功能
   - 返回指定类型的所有节点 ID
   - 处理空结果

5. ✅ triggerConfigs 类型安全性
   - 存储和检索 WorkflowTrigger 配置
   - 处理不存在的触发器

6. ✅ 与现有图结构的集成
   - 与父类的 WorkflowNode 存储共存

所有测试通过 ✅

## 兼容性说明

### 向后兼容性

- ✅ 接口签名保持兼容（从 `unknown` 到具体类型是更严格的约束）
- ✅ 现有代码无需修改即可工作
- ✅ 新方法是增量添加，不影响现有功能

### 迁移指南

如果现有代码使用了这些字段：

```typescript
// 之前的代码（需要类型断言）
const config = graph.nodeConfigs.get("node-1") as StaticNode;

// 现在的代码（类型安全）
const config = graph.getNodeConfig("node-1");
// 或者
const config = graph.nodeConfigs.get("node-1"); // 自动推断为 StaticNode | undefined
```

## 性能影响

- **内存占用**：无变化（仅改变类型定义）
- **运行时性能**：新方法都是 O(1) 或 O(n) 复杂度，与直接 Map 操作相当
- **编译时间**：略微增加（类型检查更严格），但可忽略不计

## 未来改进建议

1. **添加更多类型守卫**
   ```typescript
   public isStartNode(nodeId: ID): boolean;
   public isEndNode(nodeId: ID): boolean;
   public isForkNode(nodeId: ID): boolean;
   // ...
   ```

2. **支持批量操作**
   ```typescript
   public setNodeConfigs(configs: Map<ID, StaticNode>): void;
   public getNodeConfigsByTypes(types: StaticNodeType[]): Map<ID, StaticNode>;
   ```

3. **添加验证方法**
   ```typescript
   public validateNodeConfig(nodeId: ID): Result<void, ValidationError[]>;
   ```

## 相关文件

- `sdk/workflow/entities/workflow-graph.ts` - 主要实现
- `packages/types/src/graph/preprocessed-graph.ts` - 类型定义
- `sdk/__tests__/workflow/entities/workflow-graph-static-node-type.test.ts` - 单元测试
- `sdk/docs/data-flow/graph-preprocessing.md` - 相关文档

## 总结

本次改进成功地将 `StaticNodeType` 类型系统集成到 `WorkflowGraph` 类中，显著提升了：

1. **类型安全性** - 编译时捕获更多错误
2. **开发体验** - IDE 智能提示和自动补全
3. **代码一致性** - 与项目其他模块保持一致
4. **可维护性** - 减少类型断言，提高代码可读性

所有改进都经过充分测试，确保不会引入破坏性变更。
