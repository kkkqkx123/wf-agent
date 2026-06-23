# WorkflowBuilder 与模板构建器集成分析

## 概述

本文档分析 `NodeTemplateBuilder`、`TriggerTemplateBuilder` 与 `WorkflowBuilder` 的集成方式，以提供预定义的节点和触发器功能。

## 架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                    模板层 (Template Layer)                    │
├─────────────────────────────────────────────────────────────┤
│  NodeTemplateBuilder    │    TriggerTemplateBuilder          │
│  - 创建节点模板          │    - 创建触发器模板                 │
│  - 注册到注册表          │    - 注册到注册表                   │
└──────────┬──────────────┴──────────────┬────────────────────┘
           │                             │
           │ 注册                        │ 注册
           ▼                             ▼
┌─────────────────────────────────────────────────────────────┐
│                  注册表层 (Registry Layer)                    │
├─────────────────────────────────────────────────────────────┤
│  nodeTemplateRegistry    │    triggerTemplateRegistry        │
│  - 存储节点模板          │    - 存储触发器模板                │
│  - 查询和验证            │    - 查询和验证                    │
│  - 转换为Node            │    - 转换为WorkflowTrigger         │
└──────────┬──────────────┴──────────────┬────────────────────┘
           │                             │
           │ 引用                        │ 引用
           ▼                             ▼
┌─────────────────────────────────────────────────────────────┐
│                  构建层 (Builder Layer)                       │
├─────────────────────────────────────────────────────────────┤
│                    WorkflowBuilder                           │
│  - 从模板添加节点          │  - 从模板添加触发器               │
│  - 直接添加节点            │  - 直接添加触发器                 │
│  - 构建工作流定义          │  - 展开模板引用                   │
└─────────────────────────────────────────────────────────────┘
```

## 当前实现状态

### 1. NodeTemplateBuilder

**功能：**
- 提供链式API创建节点模板
- 支持设置描述、配置、元数据、分类、标签
- 提供 `build()` 和 `buildAndRegister()` 方法
- 自动注册到 `nodeTemplateRegistry`

**关键方法：**
```typescript
// 创建并注册节点模板
NodeTemplateBuilder.create(name, type)
  .description(desc)
  .config(config)
  .category(category)
  .tags(tag1, tag2)
  .buildAndRegister();
```

**注册表功能：**
- `register(template)` - 注册模板
- `get(name)` - 获取模板
- `list()` - 列出所有模板
- `search(keyword)` - 搜索模板

### 2. TriggerTemplateBuilder

**功能：**
- 提供链式API创建触发器模板
- 支持设置条件、动作、启用状态、最大触发次数
- 提供 `build()` 和 `buildAndRegister()` 方法
- 自动注册到 `triggerTemplateRegistry`

**关键方法：**
```typescript
// 创建并注册触发器模板
TriggerTemplateBuilder.create(name)
  .description(desc)
  .condition(condition)
  .action(action)
  .enabled(true)
  .maxTriggers(10)
  .buildAndRegister();
```

**注册表功能：**
- `register(template)` - 注册模板
- `get(name)` - 获取模板
- `convertToWorkflowTrigger()` - 转换为WorkflowTrigger
- `search(keyword)` - 搜索模板

### 3. WorkflowBuilder

**当前功能：**
- 提供链式API构建工作流定义
- 支持添加各种类型的节点（START, END, LLM, CODE, VARIABLE, ROUTE等）
- 支持添加边和变量
- 提供工作流验证

**缺失功能：**
- ❌ 从节点模板添加节点
- ❌ 添加触发器（直接或从模板）
- ❌ 处理模板引用的展开

## 集成方案设计

### 方案1：在 WorkflowBuilder 中添加模板引用方法

#### 1.1 添加从节点模板添加节点的方法

```typescript
/**
 * 从节点模板添加节点
 * @param nodeId 节点ID（工作流中唯一）
 * @param templateName 节点模板名称
 * @param configOverride 配置覆盖（可选）
 * @param nodeName 节点名称（可选）
 * @returns this
 */
addNodeFromTemplate(
  nodeId: string,
  templateName: string,
  configOverride?: Partial<NodeConfig>,
  nodeName?: string
): this {
  const template = nodeTemplateRegistry.get(templateName);
  if (!template) {
    throw new Error(`节点模板 '${templateName}' 不存在`);
  }

  // 合并配置
  const mergedConfig = configOverride 
    ? { ...template.config, ...configOverride }
    : template.config;

  return this.addNode(
    nodeId,
    template.type,
    mergedConfig,
    nodeName || template.name
  );
}
```

#### 1.2 添加触发器相关方法

```typescript
private triggers: (WorkflowTrigger | TriggerReference)[] = [];

/**
 * 添加触发器
 * @param trigger 触发器定义
 * @returns this
 */
addTrigger(trigger: WorkflowTrigger): this {
  this.triggers.push(trigger);
  return this;
}

/**
 * 从触发器模板添加触发器
 * @param triggerId 触发器ID（工作流中唯一）
 * @param templateName 触发器模板名称
 * @param configOverride 配置覆盖（可选）
 * @param triggerName 触发器名称（可选）
 * @returns this
 */
addTriggerFromTemplate(
  triggerId: string,
  templateName: string,
  configOverride?: {
    condition?: any;
    action?: any;
    enabled?: boolean;
    maxTriggers?: number;
  },
  triggerName?: string
): this {
  const reference: TriggerReference = {
    templateName,
    triggerId,
    triggerName,
    configOverride
  };
  this.triggers.push(reference);
  return this;
}
```

#### 1.3 修改 build() 方法

```typescript
build(): WorkflowDefinition {
  // 更新节点的边引用
  this.updateNodeEdgeReferences();

  // 验证工作流
  this.validate();

  // 构建完整的工作流定义
  const workflow: WorkflowDefinition = {
    ...this.workflow,
    nodes: Array.from(this.nodes.values()),
    edges: this.edges,
    variables: this.variables.length > 0 ? this.variables : undefined,
    triggers: this.triggers.length > 0 ? this.triggers : undefined,
    updatedAt: now()
  } as WorkflowDefinition;

  return workflow;
}
```

### 方案2：创建独立的模板解析器

创建一个 `TemplateResolver` 类，专门负责模板引用的解析和展开：

```typescript
class TemplateResolver {
  /**
   * 解析节点引用
   */
  resolveNodeReference(reference: NodeReferenceConfig): Node {
    const template = nodeTemplateRegistry.get(reference.templateName);
    if (!template) {
      throw new Error(`节点模板 '${reference.templateName}' 不存在`);
    }

    const mergedConfig = reference.configOverride
      ? { ...template.config, ...reference.configOverride }
      : template.config;

    return {
      id: reference.nodeId,
      type: template.type,
      name: reference.nodeName || template.name,
      config: mergedConfig,
      outgoingEdgeIds: [],
      incomingEdgeIds: []
    };
  }

  /**
   * 解析触发器引用
   */
  resolveTriggerReference(reference: TriggerReference): WorkflowTrigger {
    return triggerTemplateRegistry.convertToWorkflowTrigger(
      reference.templateName,
      reference.triggerId,
      reference.triggerName,
      reference.configOverride
    );
  }
}
```

## 推荐实现方案

**推荐使用方案1**，原因如下：

1. **简洁性**：直接在 WorkflowBuilder 中添加方法，无需额外的类
2. **一致性**：与现有的 `addNode()` 等方法保持一致
3. **易用性**：用户可以直接使用链式API，无需了解模板解析细节
4. **灵活性**：支持直接添加和从模板添加两种方式

## 使用示例

### 示例1：预定义节点模板

```typescript
// 1. 注册节点模板
NodeTemplateBuilder.create('greeting-llm', NodeType.LLM)
  .description('问候语生成LLM节点')
  .config({
    profileId: 'gpt-4',
    prompt: '请生成一个友好的问候语'
  })
  .category('common')
  .tags('llm', 'greeting')
  .buildAndRegister();

// 2. 在工作流中使用模板
const workflow = WorkflowBuilder.create('greeting-workflow')
  .name('问候工作流')
  .addStartNode()
  .addNodeFromTemplate('greeting', 'greeting-llm', {
    prompt: '请生成一个中文问候语'
  })
  .addEndNode()
  .addEdge('start', 'greeting')
  .addEdge('greeting', 'end')
  .build();
```

### 示例2：预定义触发器模板

```typescript
// 1. 注册触发器模板
TriggerTemplateBuilder.create('error-alert')
  .description('错误告警触发器')
  .condition({
    eventType: EventType.NODE_ERROR,
    filters: { nodeId: 'critical-node' }
  })
  .action({
    type: TriggerActionType.SEND_NOTIFICATION,
    config: {
      channel: 'email',
      recipients: ['admin@example.com']
    }
  })
  .enabled(true)
  .buildAndRegister();

// 2. 在工作流中使用模板
const workflow = WorkflowBuilder.create('monitored-workflow')
  .name('监控工作流')
  .addStartNode()
  .addEndNode()
  .addEdge('start', 'end')
  .addTriggerFromTemplate('error-alert-1', 'error-alert', {
    enabled: true
  })
  .build();
```

### 示例3：混合使用

```typescript
// 注册多个模板
NodeTemplateBuilder.create('data-fetch', NodeType.CODE)
  .config({
    scriptName: 'fetch-data',
    scriptType: 'javascript',
    risk: 'low'
  })
  .buildAndRegister();

NodeTemplateBuilder.create('data-process', NodeType.LLM)
  .config({
    profileId: 'gpt-4',
    prompt: '处理以下数据'
  })
  .buildAndRegister();

// 构建工作流
const workflow = WorkflowBuilder.create('data-pipeline')
  .name('数据处理流水线')
  .addStartNode()
  .addNodeFromTemplate('fetch', 'data-fetch')
  .addNodeFromTemplate('process', 'data-process', {
    prompt: '分析并总结以下数据'
  })
  .addEndNode()
  .addEdge('start', 'fetch')
  .addEdge('fetch', 'process')
  .addEdge('process', 'end')
  .build();
```

## 类型定义支持

### NodeReferenceConfig（已存在）

```typescript
export interface NodeReferenceConfig {
  templateName: string;
  nodeId: ID;
  nodeName?: string;
  configOverride?: Partial<NodeConfig>;
}
```

### TriggerReference（已存在）

```typescript
export interface TriggerReference {
  templateName: string;
  triggerId: ID;
  triggerName?: string;
  configOverride?: TriggerConfigOverride;
}
```

## 优势

1. **代码复用**：预定义的节点和触发器可以在多个工作流中复用
2. **一致性**：确保相同功能的节点配置一致
3. **可维护性**：修改模板即可影响所有引用该模板的工作流
4. **类型安全**：TypeScript 提供完整的类型检查
5. **灵活性**：支持配置覆盖，允许在引用时进行定制

## 注意事项

1. **模板验证**：注册时验证模板配置，确保有效性
2. **版本管理**：考虑模板版本控制（未来扩展）
3. **依赖管理**：确保模板引用的依赖（如Profile）存在
4. **错误处理**：提供清晰的错误信息，帮助调试
5. **性能考虑**：模板查询是O(1)操作，性能良好

## 未来扩展

1. **模板继承**：支持模板继承和覆盖
2. **模板组合**：支持多个模板组合成新模板
3. **模板市场**：建立模板市场，共享预定义模板
4. **模板版本**：支持模板版本管理和迁移
5. **模板验证增强**：添加更复杂的验证规则

## 总结

通过在 `WorkflowBuilder` 中添加模板引用方法，可以实现：

1. ✅ 从节点模板添加节点
2. ✅ 从触发器模板添加触发器
3. ✅ 支持配置覆盖
4. ✅ 保持链式API的一致性
5. ✅ 提供完整的类型安全

这种集成方式简洁、高效，符合现有的设计模式，能够很好地支持预定义节点和触发器的需求。