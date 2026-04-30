# Workflow 定义与管理

本文档详细描述 WorkflowDefinition 的结构、注册流程和管理机制。

## 概述

WorkflowDefinition 是工作流的静态定义，描述了工作流的结构、配置和元数据。它是整个执行流程的起点。

## 核心类型

### WorkflowDefinition

**位置**：`packages/types/src/workflow/definition.ts`

```typescript
interface WorkflowDefinition {
  // 基本标识
  id: ID;                                    // 唯一标识符
  name: string;                              // 名称
  type: WorkflowType;                        // 类型（AGENT_LOOP, GRAPH）
  description?: string;                      // 描述
  version: Version;                          // 版本号
  
  // 结构定义
  nodes: Node[];                             // 节点数组
  edges: Edge[];                             // 边数组
  
  // 变量和触发器
  variables?: WorkflowVariable[];            // 变量定义
  triggers?: (WorkflowTrigger | TriggerReference)[];  // 触发器
  
  // 配置
  triggeredSubworkflowConfig?: TriggeredSubworkflowConfig;
  config?: WorkflowConfig;                   // 执行配置
  metadata?: WorkflowMetadata;               // 元数据
  
  // 时间戳
  createdAt: Timestamp;                      // 创建时间
  updatedAt: Timestamp;                      // 更新时间
  
  // 工具配置
  availableTools?: {
    initial: Set<string>;                    // 初始可用工具集
  };
}
```

### WorkflowType

```typescript
enum WorkflowType {
  AGENT_LOOP = "AGENT_LOOP",    // Agent 循环模式
  GRAPH = "GRAPH"               // 图工作流模式
}
```

### WorkflowConfig

**位置**：`packages/types/src/workflow/config.ts`

```typescript
interface WorkflowConfig {
  timeout?: number;                    // 执行超时（毫秒）
  maxSteps?: number;                   // 最大执行步数
  enableCheckpoints?: boolean;         // 是否启用检查点
  checkpointConfig?: CheckpointConfig; // 检查点配置
  retryPolicy?: {                      // 重试策略
    maxRetries?: number;
    retryDelay?: number;
    backoffMultiplier?: number;
  };
  toolApproval?: ToolApprovalConfig;   // 工具审批配置
}
```

### WorkflowVariable

**位置**：`packages/types/src/workflow/variables.ts`

```typescript
interface WorkflowVariable {
  name: string;                        // 变量名
  type: VariableValueType;             // 变量类型
  defaultValue?: unknown;              // 默认值
  description?: string;                // 描述
  required?: boolean;                  // 是否必需
  readonly?: boolean;                  // 是否只读
  scope?: VariableScope;               // 作用域
}
```

### WorkflowMetadata

**位置**：`packages/types/src/workflow/metadata.ts`

```typescript
interface WorkflowMetadata {
  author?: string;                     // 作者
  tags?: string[];                     // 标签
  category?: string;                   // 分类
  customFields?: Record<string, unknown>;  // 自定义字段
}
```

---

## WorkflowRegistry

### 职责

WorkflowRegistry 负责 WorkflowDefinition 的注册、查询和管理。

**位置**：`sdk/graph/stores/workflow-registry.ts`

### 核心数据结构

```typescript
class WorkflowRegistry {
  // Workflow 定义存储
  private workflows: Map<string, WorkflowDefinition> = new Map();
  
  // Workflow 关系管理
  private workflowRelationships: Map<string, WorkflowRelationship> = new Map();
  
  // 活跃 Workflow 集合
  private activeWorkflows: Set<string> = new Set();
  
  // 引用关系
  private referenceRelations: Map<string, WorkflowReferenceRelation[]> = new Map();
}
```

### 主要方法

#### 1. 注册 Workflow

```typescript
// 同步注册（不执行预处理）
register(workflow: WorkflowDefinition, options?: RegisterOptions): void {
  // 1. 验证 workflow 定义
  const validationResult = this.validate(workflow);
  if (!validationResult.valid) {
    throw new ConfigurationValidationError(...);
  }
  
  // 2. 检查 ID 是否已存在
  if (this.workflows.has(workflow.id)) {
    if (options?.skipIfExists) return;  // 幂等操作
    throw new ConfigurationValidationError(...);
  }
  
  // 3. 保存 workflow 定义
  this.workflows.set(workflow.id, workflow);
}

// 异步注册（执行完整预处理）
async registerAsync(workflow: WorkflowDefinition, options?: RegisterOptions): Promise<void> {
  // 1-3. 同上
  
  // 4. 异步预处理 workflow
  try {
    await this.preprocessWorkflow(workflow);
  } catch (error) {
    this.workflows.delete(workflow.id);  // 失败时回滚
    throw new ConfigurationValidationError(...);
  }
}
```

#### 2. 查询 Workflow

```typescript
// 根据 ID 获取
get(workflowId: string): WorkflowDefinition | undefined {
  return this.workflows.get(workflowId);
}

// 根据名称获取
getByName(name: string): WorkflowDefinition | undefined {
  for (const workflow of this.workflows.values()) {
    if (workflow.name === name) {
      return workflow;
    }
  }
  return undefined;
}

// 根据标签查询
getByTags(tags: string[]): WorkflowDefinition[] {
  const result: WorkflowDefinition[] = [];
  for (const workflow of this.workflows.values()) {
    const workflowTags = workflow.metadata?.tags || [];
    if (tags.every(tag => workflowTags.includes(tag))) {
      result.push(workflow);
    }
  }
  return result;
}

// 根据分类查询
getByCategory(category: string): WorkflowDefinition[] {
  const result: WorkflowDefinition[] = [];
  for (const workflow of this.workflows.values()) {
    if (workflow.metadata?.category === category) {
      result.push(workflow);
    }
  }
  return result;
}
```

#### 3. 更新 Workflow

```typescript
update(workflowId: string, updates: Partial<WorkflowDefinition>, options?: UpdateOptions): void {
  const workflow = this.workflows.get(workflowId);
  if (!workflow) {
    if (options?.createIfNotExists && updates.id === workflowId) {
      this.register({ ...updates, id: workflowId } as WorkflowDefinition);
      return;
    }
    throw new WorkflowNotFoundError(...);
  }
  
  // 创建更新后的 workflow
  const updatedWorkflow: WorkflowDefinition = {
    ...workflow,
    ...updates,
    id: workflow.id,        // ID 不可修改
    updatedAt: Date.now(),
  };
  
  // 验证更新后的 workflow
  const validationResult = this.validate(updatedWorkflow);
  if (!validationResult.valid) {
    throw new ConfigurationValidationError(...);
  }
  
  // 更新存储
  this.workflows.set(workflowId, updatedWorkflow);
}
```

#### 4. 删除 Workflow

```typescript
unregister(workflowId: string, options?: UnregisterOptions): void {
  // 1. 检查是否存在
  if (!this.workflows.has(workflowId)) {
    throw new WorkflowNotFoundError(...);
  }
  
  // 2. 检查引用关系
  if (options?.checkReferences !== false) {
    const checkResult = this.canSafelyDelete(workflowId, options);
    if (!checkResult.canDelete) {
      throw new ConfigurationValidationError(checkResult.details);
    }
  }
  
  // 3. 删除 workflow
  this.workflows.delete(workflowId);
  
  // 4. 清理引用关系
  this.cleanupWorkflowReferences(workflowId);
}
```

#### 5. 验证 Workflow

```typescript
validate(workflow: WorkflowDefinition): { valid: boolean; errors: string[] } {
  const errors: string[] = [];
  
  // 基本验证
  if (!workflow.id) {
    errors.push("Workflow ID is required");
  }
  
  if (!workflow.name) {
    errors.push("Workflow name is required");
  }
  
  if (!workflow.nodes || workflow.nodes.length === 0) {
    errors.push("Workflow must have at least one node");
  }
  
  if (!workflow.edges) {
    errors.push("Workflow edges are required");
  }
  
  return {
    valid: errors.length === 0,
    errors,
  };
}
```

---

## 注册流程

### 同步注册流程

```
WorkflowDefinition
       ↓
validate(workflow)
       ↓
检查 ID 是否存在
       ↓
  [不存在] → workflows.set(workflow.id, workflow)
       ↓
  [已存在] → skipIfExists ? 跳过 : 抛错
```

### 异步注册流程

```
WorkflowDefinition
       ↓
validate(workflow)
       ↓
检查 ID 是否存在
       ↓
  [不存在] → workflows.set(workflow.id, workflow)
       ↓
preprocessWorkflow(workflow)
       ↓
  [成功] → GraphRegistry.register(preprocessedGraph)
       ↓
  [失败] → workflows.delete(workflow.id) → 抛错
```

---

## 引用关系管理

### WorkflowReferenceRelation

```typescript
interface WorkflowReferenceRelation {
  sourceWorkflowId: string;      // 源 Workflow ID
  targetWorkflowId: string;      // 目标 Workflow ID
  referenceType: WorkflowReferenceType;  // 引用类型
  nodeId?: string;               // 节点 ID（如果是节点引用）
}
```

### 引用类型

```typescript
enum WorkflowReferenceType {
  SUBGRAPH = "SUBGRAPH",                    // 子图引用
  TRIGGERED_SUBWORKFLOW = "TRIGGERED_SUBWORKFLOW",  // 触发子工作流
  TRIGGER = "TRIGGER"                       // 触发器引用
}
```

### 引用检查

```typescript
checkWorkflowReferences(workflowId: string): WorkflowReferenceInfo {
  // 检查静态引用（SUBGRAPH 节点）
  // 检查运行时引用（正在执行的 Thread）
  // 返回引用信息
}

canSafelyDelete(workflowId: string, options?: UnregisterOptions): { canDelete: boolean; details: string } {
  const referenceInfo = this.checkWorkflowReferences(workflowId);
  
  if (!referenceInfo.hasReferences) {
    return { canDelete: true, details: "No references found" };
  }
  
  if (options?.force) {
    return { canDelete: true, details: "Force delete enabled" };
  }
  
  return { canDelete: false, details: "Has references" };
}
```

---

## Workflow 层次关系

### 父子关系

```typescript
// 注册子图关系
registerSubgraphRelationship(
  parentWorkflowId: string,
  subgraphNodeId: string,
  childWorkflowId: string
): void {
  // 更新父 workflow 的子 workflow 集合
  // 更新子 workflow 的父 workflow 引用
  // 计算层次深度
}

// 获取父 workflow
getParentWorkflow(workflowId: string): string | null {
  const relationship = this.workflowRelationships.get(workflowId);
  return relationship?.parentWorkflowId || null;
}

// 获取子 workflow 列表
getChildWorkflows(workflowId: string): string[] {
  const relationship = this.workflowRelationships.get(workflowId);
  return relationship ? Array.from(relationship.childWorkflowIds) : [];
}

// 获取层次结构
getWorkflowHierarchy(workflowId: string): WorkflowHierarchy {
  // 构建祖先链
  // 构建后代链
  // 返回层次信息
}
```

---

## 导入导出

### 导出 Workflow

```typescript
export(workflowId: string): string {
  const workflow = this.workflows.get(workflowId);
  if (!workflow) {
    throw new WorkflowNotFoundError(...);
  }
  
  return JSON.stringify(workflow, null, 2);
}
```

### 导入 Workflow

```typescript
import(json: string, options?: RegisterOptions): string {
  try {
    const workflow = JSON.parse(json) as WorkflowDefinition;
    this.register(workflow, options);
    return workflow.id;
  } catch (error) {
    throw new ConfigurationValidationError(...);
  }
}
```

---

## 搜索功能

### 列出所有 Workflow

```typescript
list(): WorkflowSummary[] {
  const summaries: WorkflowSummary[] = [];
  for (const workflow of this.workflows.values()) {
    summaries.push({
      id: workflow.id,
      name: workflow.name,
      description: workflow.description,
      version: workflow.version,
      nodeCount: workflow.nodes.length,
      edgeCount: workflow.edges.length,
      createdAt: workflow.createdAt,
      updatedAt: workflow.updatedAt,
      tags: workflow.metadata?.tags,
      category: workflow.metadata?.category,
    });
  }
  return summaries;
}
```

### 搜索 Workflow

```typescript
search(keyword: string): WorkflowSummary[] {
  const lowerKeyword = keyword.toLowerCase();
  return this.list().filter(
    summary =>
      summary.name.toLowerCase().includes(lowerKeyword) ||
      summary.description?.toLowerCase().includes(lowerKeyword) ||
      summary.id.toLowerCase().includes(lowerKeyword)
  );
}
```

---

## 设计原则

### 1. 单一职责

- WorkflowRegistry 只负责 WorkflowTemplate 的存储和管理
- 不负责预处理（由 processWorkflow 处理）
- 不负责执行（由 WorkflowExecutor 处理）

### 2. 验证分离

- 基本验证在 WorkflowRegistry.validate()
- 深度验证在 WorkflowValidator
- 图验证在 GraphValidator

### 3. 引用完整性

- 删除前检查引用关系
- 支持强制删除（force 选项）
- 维护父子关系

### 4. 幂等性

- skipIfExists 选项支持幂等注册
- 重复注册不会抛错

---

## 相关文档

- [整体数据流](./README.md)
- [Graph 预处理](./graph-preprocessing.md)
- [Thread 执行实例](./thread-execution.md)
