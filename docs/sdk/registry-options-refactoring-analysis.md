# Registry Options 重构分析

## 概述

本文档分析了 `TriggerTemplateRegistry` 和 `WorkflowRegistry` 中 options 参数的设计问题，以及 SDK/API 层和 CLI-App 的集成方案。

## 一、原有设计问题分析

### 1.1 类型定义不统一

两个注册表都使用内联类型定义，而非共享公共类型：

```typescript
// TriggerTemplateRegistry
register(template: TriggerTemplate, options?: { force?: boolean }): void

// WorkflowRegistry
register(workflow: WorkflowDefinition, options?: { force?: boolean }): void
unregister(workflowId: string, options?: { force?: boolean; checkReferences?: boolean }): void
```

**问题**：
- 类型定义重复，维护成本高
- 扩展选项时需要修改多处
- 缺乏一致性保证

### 1.2 API 不对称

| 操作 | TriggerTemplateRegistry | WorkflowRegistry |
|------|------------------------|------------------|
| register | 有 `force` 选项 | 有 `force` 选项 |
| unregister | **无选项** | 有 `force` + `checkReferences` |

**问题**：`unregister` 方法在两个注册表中的行为不一致。

### 1.3 force 选项的语义问题

`force` 选项的语义是"覆盖已存在的项"，这是一种**破坏性操作**，可能带来以下风险：

1. **数据丢失**：覆盖会丢失原有配置
2. **引用断裂**：其他组件可能引用了被覆盖的项
3. **状态不一致**：运行时状态可能与新配置不兼容

## 二、重构方案

### 2.1 新增公共类型定义

在 `packages/types/src/registry-options.ts` 中定义：

```typescript
/**
 * 注册操作选项
 */
export interface RegisterOptions {
  /**
   * 跳过已存在的项（幂等操作）
   * - true: 如果已存在则跳过，不报错
   * - false/undefined: 如果已存在则抛出错误
   */
  skipIfExists?: boolean;
}

/**
 * 批量注册操作选项
 */
export interface BatchRegisterOptions extends RegisterOptions {
  /**
   * 跳过错误继续执行
   */
  skipErrors?: boolean;
}

/**
 * 删除操作选项
 */
export interface UnregisterOptions {
  /**
   * 强制删除，忽略引用检查
   */
  force?: boolean;

  /**
   * 是否检查引用
   */
  checkReferences?: boolean;
}

/**
 * 批量删除操作选项
 */
export interface BatchUnregisterOptions extends UnregisterOptions {
  skipErrors?: boolean;
}

/**
 * 更新操作选项
 */
export interface UpdateOptions {
  /**
   * 允许更新不存在的项（自动创建）
   */
  createIfNotExists?: boolean;
}
```

### 2.2 职责分离设计

采用职责分离模式，每个方法职责单一：

| 方法 | 语义 | 使用场景 |
|------|------|---------|
| `register()` | 仅新增 | 初始化、首次注册 |
| `update()` | 仅修改 | 已知存在，需要更新 |
| `upsert()` | 新增或修改 | 不确定是否存在 |
| `unregister()` | 删除 | 移除注册项 |

### 2.3 修改后的 API

#### TriggerTemplateRegistry

```typescript
class TriggerTemplateRegistry {
  // 注册（仅新增）
  register(template: TriggerTemplate, options?: RegisterOptions): void;
  
  // 更新（仅修改）
  update(name: string, updates: Partial<TriggerTemplate>, options?: UpdateOptions): void;
  
  // 注册或更新
  upsert(template: TriggerTemplate): void;
  
  // 删除
  unregister(name: string, options?: UnregisterOptions): void;
  
  // 批量操作
  registerBatch(templates: TriggerTemplate[], options?: BatchRegisterOptions): void;
  unregisterBatch(names: string[], options?: BatchUnregisterOptions): void;
}
```

#### WorkflowRegistry

```typescript
class WorkflowRegistry {
  // 注册（仅新增）
  register(workflow: WorkflowDefinition, options?: RegisterOptions): void;
  
  // 更新（仅修改）
  update(workflowId: string, updates: Partial<WorkflowDefinition>, options?: UpdateOptions): void;
  
  // 注册或更新
  upsert(workflow: WorkflowDefinition): void;
  
  // 删除
  unregister(workflowId: string, options?: UnregisterOptions): void;
  
  // 批量操作
  registerBatch(workflows: WorkflowDefinition[], options?: BatchRegisterOptions): void;
  unregisterBatch(workflowIds: string[], options?: BatchUnregisterOptions): void;
}
```

## 三、API 方法暴露策略

### 3.1 方法分类原则

根据方法的用途和调用场景，将方法分为以下几类：

| 分类 | 定义 | 是否暴露 API |
|------|------|-------------|
| **核心 CRUD** | 用户直接调用的增删改查操作 | ✅ 暴露 |
| **查询辅助** | 用户查询数据的辅助方法 | ✅ 暴露 |
| **导入导出** | 数据迁移相关操作 | ✅ 暴露 |
| **内部状态管理** | 由其他模块间接调用的状态管理 | ❌ 不暴露 |
| **引用关系管理** | 由系统内部维护的关系 | ❌ 不暴露 |
| **验证方法** | 内部验证逻辑 | ❌ 不暴露 |

### 3.2 TriggerTemplateRegistry 方法分析

| 方法 | 分类 | 暴露 API | 理由 |
|------|------|----------|------|
| `register()` | 核心 CRUD | ✅ | 用户注册模板 |
| `registerBatch()` | 核心 CRUD | ✅ | 批量注册 |
| `update()` | 核心 CRUD | ✅ | 用户更新模板 |
| `upsert()` | 核心 CRUD | ✅ | 注册或更新 |
| `unregister()` | 核心 CRUD | ✅ | 用户删除模板 |
| `unregisterBatch()` | 核心 CRUD | ✅ | 批量删除 |
| `get()` | 查询辅助 | ✅ | 获取单个模板 |
| `has()` | 查询辅助 | ✅ | 检查存在 |
| `list()` | 查询辅助 | ✅ | 列出所有 |
| `listSummaries()` | 查询辅助 | ✅ | 列出摘要 |
| `search()` | 查询辅助 | ✅ | 搜索模板 |
| `export()` | 导入导出 | ✅ | 导出为 JSON |
| `import()` | 导入导出 | ✅ | 从 JSON 导入 |
| `clear()` | 核心 CRUD | ⚠️ 可选 | 危险操作，谨慎暴露 |
| `size()` | 查询辅助 | ✅ | 获取数量 |
| `convertToWorkflowTrigger()` | 内部使用 | ❌ | 由 TriggerManager 内部调用 |
| `validateTemplate()` | 验证方法 | ❌ | 内部验证 |

### 3.3 WorkflowRegistry 方法分析

| 方法 | 分类 | 暴露 API | 理由 |
|------|------|----------|------|
| `register()` | 核心 CRUD | ✅ | 用户注册工作流 |
| `registerBatch()` | 核心 CRUD | ✅ | 批量注册 |
| `update()` | 核心 CRUD | ✅ | 用户更新工作流 |
| `upsert()` | 核心 CRUD | ✅ | 注册或更新 |
| `unregister()` | 核心 CRUD | ✅ | 用户删除工作流 |
| `unregisterBatch()` | 核心 CRUD | ✅ | 批量删除 |
| `get()` | 查询辅助 | ✅ | 获取单个工作流 |
| `getByName()` | 查询辅助 | ✅ | 按名称获取 |
| `getByTags()` | 查询辅助 | ✅ | 按标签获取 |
| `getByCategory()` | 查询辅助 | ✅ | 按分类获取 |
| `getByAuthor()` | 查询辅助 | ✅ | 按作者获取 |
| `has()` | 查询辅助 | ✅ | 检查存在 |
| `list()` | 查询辅助 | ✅ | 列出所有摘要 |
| `search()` | 查询辅助 | ✅ | 搜索工作流 |
| `export()` | 导入导出 | ✅ | 导出为 JSON |
| `import()` | 导入导出 | ✅ | 从 JSON 导入 |
| `clear()` | 核心 CRUD | ⚠️ 可选 | 危险操作，谨慎暴露 |
| `size()` | 查询辅助 | ✅ | 获取数量 |
| `validate()` | 验证方法 | ❌ | 内部验证 |
| `validateBatch()` | 验证方法 | ❌ | 内部验证 |
| **内部状态管理方法** | | | |
| `addActiveWorkflow()` | 内部状态 | ❌ | 由 ThreadExecutor 调用 |
| `removeActiveWorkflow()` | 内部状态 | ❌ | 由 ThreadExecutor 调用 |
| `isWorkflowActive()` | 内部状态 | ❌ | 内部状态检查 |
| `getActiveWorkflows()` | 内部状态 | ❌ | 内部状态查询 |
| **引用关系管理方法** | | | |
| `addReferenceRelation()` | 引用关系 | ❌ | 由系统内部维护 |
| `removeReferenceRelation()` | 引用关系 | ❌ | 由系统内部维护 |
| `hasReferences()` | 引用关系 | ❌ | 内部检查 |
| `getReferenceRelations()` | 引用关系 | ❌ | 内部查询 |
| `clearReferenceRelations()` | 引用关系 | ❌ | 内部清理 |
| `cleanupWorkflowReferences()` | 引用关系 | ❌ | 内部清理 |
| `getReferencingWorkflows()` | 引用关系 | ❌ | 内部查询 |
| `checkWorkflowReferences()` | 引用关系 | ❌ | 内部检查 |
| `canSafelyDelete()` | 引用关系 | ⚠️ 可选 | 可暴露给高级用户 |
| **子图关系管理方法** | | | |
| `registerSubgraphRelationship()` | 子图关系 | ❌ | 由预处理流程调用 |
| `getWorkflowHierarchy()` | 子图关系 | ✅ | 用户查询层次结构 |
| `getParentWorkflow()` | 子图关系 | ✅ | 用户查询父工作流 |
| `getChildWorkflows()` | 子图关系 | ✅ | 用户查询子工作流 |

### 3.4 API 层与 Registry 层的关系

```
┌─────────────────────────────────────────────────────────────┐
│                      用户/CLI-App                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    API 层 (WorkflowRegistryAPI)              │
│  - 暴露用户可调用的方法                                        │
│  - 提供验证、错误处理、结果包装                                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Registry 层 (WorkflowRegistry)               │
│  - 核心数据管理                                               │
│  - 包含内部方法（不通过 API 暴露）                              │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              其他内部模块 (ThreadExecutor, etc.)              │
│  - 通过 DI 获取 Registry 实例                                 │
│  - 直接调用内部方法                                            │
└─────────────────────────────────────────────────────────────┘
```

### 3.5 内部方法调用方式

内部方法应该：
1. 保持 `public` 可见性（因为需要被其他内部模块调用）
2. 不在 API 层暴露
3. 通过 DI 容器获取 Registry 实例来调用

```typescript
// 内部模块通过 DI 获取 Registry 实例
const workflowRegistry = container.get(Identifiers.WorkflowRegistry);

// 内部调用
workflowRegistry.addActiveWorkflow(threadId);
workflowRegistry.addReferenceRelation(relation);
workflowRegistry.registerSubgraphRelationship(parentId, nodeId, childId);
```

## 四、SDK/API 层修改方案

### 4.1 GenericResourceAPI 修改

`GenericResourceAPI` 需要支持 options 参数：

```typescript
// sdk/api/shared/resources/generic-resource-api.ts

export abstract class GenericResourceAPI<T, ID extends string | number, Filter = any> {
  // 修改抽象方法签名
  protected abstract createResource(resource: T, options?: RegisterOptions): Promise<void>;
  protected abstract updateResource(id: ID, updates: Partial<T>, options?: UpdateOptions): Promise<void>;
  protected abstract deleteResource(id: ID, options?: UnregisterOptions): Promise<void>;

  // 修改公共方法签名
  async create(resource: T, options?: RegisterOptions): Promise<ExecutionResult<void>> {
    // ...
    await this.createResource(resource, options);
    // ...
  }

  async update(id: ID, updates: Partial<T>, options?: UpdateOptions): Promise<ExecutionResult<void>> {
    // ...
    await this.updateResource(id, updates, options);
    // ...
  }

  async delete(id: ID, options?: UnregisterOptions): Promise<ExecutionResult<void>> {
    // ...
    await this.deleteResource(id, options);
    // ...
  }

  // 新增 upsert 方法
  async upsert(resource: T): Promise<ExecutionResult<void>> {
    // 子类实现
  }
}
```

### 4.2 WorkflowRegistryAPI 公共 API

```typescript
// sdk/api/graph/resources/workflows/workflow-registry-api.ts

export class WorkflowRegistryAPI extends GenericResourceAPI<WorkflowDefinition, string, WorkflowFilter> {
  
  // ========== 核心 CRUD ==========
  
  protected async createResource(workflow: WorkflowDefinition, options?: RegisterOptions): Promise<void> {
    this.dependencies.getWorkflowRegistry().register(workflow, options);
  }

  protected async updateResource(
    id: string, 
    updates: Partial<WorkflowDefinition>, 
    options?: UpdateOptions
  ): Promise<void> {
    this.dependencies.getWorkflowRegistry().update(id, updates, options);
  }

  protected async deleteResource(id: string, options?: UnregisterOptions): Promise<void> {
    this.dependencies.getWorkflowRegistry().unregister(id, options);
  }

  async upsert(workflow: WorkflowDefinition): Promise<ExecutionResult<void>> {
    // ...
    this.dependencies.getWorkflowRegistry().upsert(workflow);
    // ...
  }

  // ========== 查询方法 ==========
  
  async getByName(name: string): Promise<WorkflowDefinition | null>;
  async getByTags(tags: string[]): Promise<WorkflowDefinition[]>;
  async getByCategory(category: string): Promise<WorkflowDefinition[]>;
  async getByAuthor(author: string): Promise<WorkflowDefinition[]>;
  async search(keyword: string): Promise<WorkflowSummary[]>;
  
  // ========== 导入导出 ==========
  
  async export(id: string): Promise<string>;
  async import(json: string, options?: RegisterOptions): Promise<string>;
  
  // ========== 层次结构查询 ==========
  
  async getHierarchy(id: string): Promise<WorkflowHierarchy>;
  async getParent(id: string): Promise<string | null>;
  async getChildren(id: string): Promise<string[]>;
  
  // ========== 高级操作（可选） ==========
  
  async canSafelyDelete(id: string, options?: UnregisterOptions): Promise<{ canDelete: boolean; details: string }>;
}
```

## 五、CLI-App 集成方案

### 5.1 WorkflowAdapter 修改

```typescript
// apps/cli-app/src/adapters/workflow-adapter.ts

export class WorkflowAdapter extends BaseAdapter {
  
  /**
   * 从文件注册工作流
   */
  async registerFromFile(
    filePath: string,
    parameters?: Record<string, any>,
    options?: RegisterOptions  // 新增 options
  ): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const workflow = await this.configManager.loadWorkflow(fullPath, parameters);
      
      const api = this.sdk.workflows;
      await api.create(workflow, options);  // 传递 options
      
      this.logger.success(`工作流已注册: ${workflow.id}`);
      return workflow;
    }, '注册工作流');
  }

  /**
   * 从目录批量注册工作流
   */
  async registerFromDirectory(
    options: ConfigLoadOptions = {},
    registerOptions?: RegisterOptions  // 新增 registerOptions
  ): Promise<{
    success: any[];
    failures: Array<{ filePath: string; error: string }>;
  }> {
    return this.executeWithErrorHandling(async () => {
      const result = await this.configManager.loadWorkflows(options);
      
      const success: any[] = [];
      const failures = result.failures;

      const api = this.sdk.workflows;
      for (const workflow of result.configs) {
        try {
          await api.create(workflow, registerOptions);  // 传递 options
          success.push(workflow);
          this.logger.success(`工作流已注册: ${workflow.id}`);
        } catch (error) {
          failures.push({
            filePath: workflow.id,
            error: error instanceof Error ? error.message : String(error)
          });
          this.logger.error(`注册工作流失败: ${workflow.id}`);
        }
      }

      return { success, failures };
    }, '批量注册工作流');
  }

  /**
   * 注册或更新工作流（新增）
   */
  async upsertFromFile(
    filePath: string,
    parameters?: Record<string, any>
  ): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const fullPath = resolve(process.cwd(), filePath);
      const workflow = await this.configManager.loadWorkflow(fullPath, parameters);
      
      const api = this.sdk.workflows;
      await api.upsert(workflow);  // 使用 upsert
      
      this.logger.success(`工作流已注册或更新: ${workflow.id}`);
      return workflow;
    }, '注册或更新工作流');
  }

  /**
   * 删除工作流
   */
  async deleteWorkflow(
    id: string,
    options?: UnregisterOptions  // 新增 options
  ): Promise<void> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.workflows;
      await api.delete(id, options);  // 传递 options

      this.logger.success(`工作流已删除: ${id}`);
    }, '删除工作流');
  }
}
```

### 5.2 命令行参数支持

建议在 CLI 命令中支持以下参数：

```bash
# 注册工作流（跳过已存在）
modular-agent workflow register ./workflow.toml --skip-if-exists

# 强制删除工作流
modular-agent workflow delete my-workflow --force

# 删除工作流（跳过引用检查）
modular-agent workflow delete my-workflow --no-check-references

# 注册或更新工作流
modular-agent workflow upsert ./workflow.toml
```

### 5.3 配置文件支持

可以在 CLI 配置文件中添加默认行为：

```toml
# .modular-agentrc.toml

[registry]
# 注册时默认跳过已存在的项
defaultSkipIfExists = true

# 删除时默认检查引用
defaultCheckReferences = true
```

## 六、修改文件清单

| 优先级 | 文件路径 | 修改内容 |
|--------|----------|----------|
| P0 | `packages/types/src/registry-options.ts` | 新增公共类型定义 |
| P0 | `packages/types/src/index.ts` | 导出新类型 |
| P0 | `sdk/core/services/trigger-template-registry.ts` | 适配新 API |
| P0 | `sdk/graph/services/workflow-registry.ts` | 适配新 API |
| P0 | `sdk/api/shared/resources/generic-resource-api.ts` | 支持 options |
| P0 | `sdk/api/graph/resources/workflows/workflow-registry-api.ts` | 适配新 API |
| P0 | `sdk/api/graph/resources/templates/trigger-template-registry-api.ts` | 适配新 API |
| P1 | `apps/cli-app/src/adapters/workflow-adapter.ts` | 支持 options |
| P1 | `apps/cli-app/src/adapters/template-adapter.ts` | 支持 options |
| P2 | 其他 RegistryAPI | 按需适配 |

## 七、设计原则总结

1. **职责分离**：`register()` 仅负责新增，`update()` 仅负责修改，`upsert()` 提供显式的"存在则更新"语义
2. **幂等操作**：`skipIfExists` 选项支持幂等注册，多次调用不会报错
3. **类型统一**：所有注册表使用相同的 options 类型定义
4. **批量操作支持**：批量方法支持传递 options
5. **API 层一致性**：GenericResourceAPI 作为基类，提供统一的 options 支持
6. **CLI 灵活性**：通过适配器层传递 options，保持命令行灵活性
7. **内部方法隔离**：内部状态管理和引用关系管理方法不通过 API 暴露，通过 DI 直接调用

## 八、方法暴露统计

| 方法类型 | 数量 | API 暴露 |
|----------|------|----------|
| 核心 CRUD | 8 | ✅ 全部暴露 |
| 查询辅助 | 10 | ✅ 全部暴露 |
| 导入导出 | 2 | ✅ 全部暴露 |
| 内部状态管理 | 4 | ❌ 不暴露 |
| 引用关系管理 | 9 | ❌ 不暴露（canSafelyDelete 可选） |
| 子图关系管理 | 4 | 3 暴露，1 不暴露 |
| 验证方法 | 2 | ❌ 不暴露 |

## 九、已完成修改

以下修改已完成并通过类型检查：

1. ✅ `packages/types/src/registry-options.ts` - 新增公共类型定义
2. ✅ `packages/types/src/index.ts` - 导出新类型
3. ✅ `sdk/core/services/trigger-template-registry.ts` - 适配新 API
4. ✅ `sdk/graph/services/workflow-registry.ts` - 适配新 API
5. ✅ `sdk/core/services/predefined-triggers.ts` - 更新使用新 API
6. ✅ 修复 `CheckpointTriggerType` 类型错误

## 十、待完成修改

以下修改待实施：

1. ⏳ `sdk/api/shared/resources/generic-resource-api.ts` - 支持 options
2. ⏳ `sdk/api/graph/resources/workflows/workflow-registry-api.ts` - 适配新 API
3. ⏳ `sdk/api/graph/resources/templates/trigger-template-registry-api.ts` - 适配新 API
4. ⏳ `apps/cli-app/src/adapters/workflow-adapter.ts` - 支持 options
5. ⏳ `apps/cli-app/src/adapters/template-adapter.ts` - 支持 options
