# Registry 模块重构方案

## 概述

本文档基于对 SDK 包中 shared、workflow、agent、plugin 四个域的 Registry 管理现状分析，提出系统性重构方案。核心目标是统一架构模式、消除重复代码、明确职责边界。

**当前问题汇总**：
1. 三种不同的 Registry 架构模式（Interface / Abstract Class / Composition）互不兼容
2. `BaseExecutionRegistry` 抽象基类设计缺陷（`get` 签名不一致、`register` 被覆盖、`clear` 不被使用）
3. `AgentLoopRegistry` 混合 Entity 管理与 Task 管理，违反 SRP
4. `WorkflowRegistry` 承载 CRUD + 索引 + 验证 + 关系管理，职责过重
5. Shared 层 5 个 Registry 存在大量重复模式（ToolRegistry、ScriptRegistry、NodeTemplateRegistry、HookTemplateRegistry、TriggerTemplateRegistry）
6. Plugin Registry 自成体系，与主 Registry 体系隔离
7. API 层 Registry 包装增加额外间接层，形成四层调用链

---

## 一、设计原则

### 1.1 统一 Registry 接口层次

```
Registry<T>                           # 只读（get/has/list/keys/size）
  └── MutableRegistry<T>              # 读写（+ set/delete/clear）
       └── PersistentRegistry<T>      # 持久化（+ save/load/delete）
```

所有 Registry 类都必须实现 `MutableRegistry<T>` 接口，只有需要持久化的才实现 `PersistentRegistry<T>`。不再使用抽象基类继承模式。

### 1.2 职责分离原则

| 职责 | 归属 |
|---|---|
| 注册/注销/查询 | Registry 类 |
| 持久化 | 独立的 Storage Adapter（通过依赖注入） |
| 业务逻辑（验证、执行、转换） | 独立的 Service 类 |
| 索引管理 | 独立的 Index Service |
| 任务管理 | 独立的 Task Manager（复用现有 TaskRegistry） |

### 1.3 消除间接层

> API → Registry → Service → Storage Adapter

API 层只做参数校验和结果格式化，不重复封装 Registry 方法。Registry 只做注册/发现，不包含业务逻辑。Service 层承载业务逻辑。Storage Adapter 承载持久化。

---

## 二、重构方案

### 2.1 统一基础 Registry 实现

**目标**：用统一的 `createRegistry()` 增强版代替所有不同的基类和组合模式。

**当前**：三种模式并存
```
Interface 模式: ToolRegistry implements Registry<T>
Abstract 模式: AgentLoopRegistry extends BaseExecutionRegistry
Composition 模式: WorkflowGraphRegistry { items = createRegistry() }
```

**重构后**：所有 Registry 使用统一的 `RegistryImpl<T>` 基类

```typescript
// 统一的 Registry 实现基类
class RegistryImpl<T> implements MutableRegistry<T> {
  protected items = new Map<string, T>();

  get(key: string): T | undefined { ... }
  has(key: string): boolean { ... }
  list(): T[] { ... }
  keys(): string[] { ... }
  get size(): number { ... }
  set(key: string, value: T): void { ... }
  delete(key: string): boolean { ... }
  clear(): void { ... }

  // 统一的事件钩子（可选，通过 options 启用）
  protected onBeforeSet?: (key: string, value: T) => void;
  protected onAfterDelete?: (key: string) => void;
}

// 带持久化的扩展
class PersistentRegistryImpl<T> extends RegistryImpl<T> {
  constructor(
    private storageAdapter: StorageAdapter<T> | null,
    private serializer: Serializer<T>,
  ) { super(); }

  async save(key: string, value: T): Promise<void> { ... }
  async load(key: string): Promise<T | null> { ... }
  async delete(key: string): Promise<void> { ... }
  async initialize(): Promise<void> { ... }
}
```

**影响范围**：
- `ToolRegistry` → 继承 `PersistentRegistryImpl<Tool>`
- `ScriptRegistry` → 继承 `PersistentRegistryImpl<Script>`
- `NodeTemplateRegistry` → 继承 `PersistentRegistryImpl<NodeTemplate>`
- `HookTemplateRegistry` → 继承 `PersistentRegistryImpl<HookTemplate>`
- `TriggerTemplateRegistry` → 继承 `PersistentRegistryImpl<TriggerTemplate>`
- `AgentProfileRegistry` → 继承 `PersistentRegistryImpl<AgentProfileMeta>`
- `WorkflowGraphRegistry` → 继承 `RegistryImpl<WorkflowGraph>`
- `FragmentRegistry` → 继承 `RegistryImpl<SystemPromptFragment>`（保留依赖追踪）
- `ExecutionHierarchyRegistry` → 继承 `RegistryImpl<AnyExecutionEntity>`

---

### 2.2 BaseExecutionRegistry 重构

**目标**：消除 `BaseExecutionRegistry` 抽象基类，将其拆分为职责清晰的组件。

**问题**：
- `get()` 签名在子类中不一致（同步 vs Promise）
- `register()` 被两个子类覆盖，基类逻辑未被复用
- `clear()` 不被子类使用

**方案**：拆分为 `ExecutionStore`（数据存储）和 `ExecutionCoordinatorStore`（协调器存储）两个独立的轻量级类。

```typescript
// 执行实体存储（纯数据层）
class ExecutionStore<T extends { id: string; cleanup(): void }> {
  private entities = new Map<string, T>();

  register(entity: T): void { ... }
  get(id: string): T | undefined { ... }
  has(id: string): boolean { ... }
  getAll(): T[] { ... }
  getAllIds(): string[] { ... }
  size(): number { ... }
  delete(id: string): void { ... }
  clear(): void { ... }  // 调用每个 entity.cleanup()
}

// 协调器存储（独立，可选）
class CoordinatorStore<T extends { cleanup(): void }> {
  private coordinators = new Map<string, T>();

  register(id: string, coordinator: T): void { ... }
  get(id: string): T | null { ... }
  delete(id: string): void { ... }
  clear(): void { ... }  // 调用每个 coordinator.cleanup()
}
```

然后 `AgentLoopRegistry` 和 `WorkflowExecutionRegistry` 各自持有这两个 Store 实例：

```typescript
class AgentLoopRegistry implements MutableRegistry<AgentLoopEntity> {
  private store = new ExecutionStore<AgentLoopEntity>();
  private coordinatorStore = new CoordinatorStore<AgentStateCoordinator>();
  private storageAdapter?: AgentLoopStorageAdapter;
  private taskManager?: TaskManager;  // 复用 TaskRegistry，而非内置

  register(entity: AgentLoopEntity): void { ... }
  get(id: ID): AgentLoopEntity | undefined { ... }
  // ... 其他 MutableRegistry 方法
}
```

**影响范围**：
- `AgentLoopRegistry` — 不再继承 `BaseExecutionRegistry`，改为实现 `MutableRegistry<AgentLoopEntity>`
- `WorkflowExecutionRegistry` — 不再继承 `BaseExecutionRegistry`，改为实现 `MutableRegistry<WorkflowExecutionEntity>`

---

### 2.3 AgentLoopRegistry 职责拆分

**目标**：将 Task 管理从 AgentLoopRegistry 中剥离。

**当前**：`AgentLoopRegistry` 包含 12 个 Task 相关方法（`registerAsTask`、`updateTaskStatusToRunning`、`cancelTask`、`deleteTask`、`cleanupExpiredTasks` 等）

**方案**：将 Task 管理迁移到 `TaskRegistry`（已在 shared 层存在）

```typescript
// 取消 AgentLoopRegistry 中的以下方法
// - registerAsTask()        → 调用方直接使用 TaskRegistry.register()
// - updateTaskStatusToRunning() → TaskRegistry.updateStatus()
// - updateTaskStatusToCompleted() → TaskRegistry.updateStatus()
// - updateTaskStatusToFailed() → TaskRegistry.updateStatus()
// - updateTaskStatusToCancelled() → TaskRegistry.updateStatus()
// - getTaskInfo()           → TaskRegistry.get()
// - getTasksByStatus()      → TaskRegistry.getByStatus()
// - getTaskStats()          → 由 TaskRegistry 提供
// - cancelTask()            → TaskRegistry.cancelTask()
// - deleteTask()            → TaskRegistry.delete()
// - cleanupExpiredTasks()   → TaskRegistry.cleanup()
// - findTaskIdByAgentLoopId() → TaskRegistry.getByExecutionId()
```

**影响范围**：
- `AgentLoopRegistry` 缩减约 200 行，回归纯 Registry 职责
- 所有调用方改为依赖 `TaskRegistry`（通过 DI 注入）

---

### 2.4 WorkflowRegistry 职责拆分

**目标**：将索引管理、验证、引用检查从 WorkflowRegistry 中剥离。

**当前**：`WorkflowRegistry` 耦合了 8 类职责。

**方案**：拆分出独立的 Service

```typescript
// 1. 索引管理 → 独立 WorkflowIndexService
class WorkflowIndexService {
  private byName: Map<string, string> = new Map();
  private byTags: Map<string, Set<string>> = new Map();
  private byCategory: Map<string, Set<string>> = new Map();
  private byAuthor: Map<string, Set<string>> = new Map();

  index(workflow: WorkflowTemplate): void { ... }
  removeFromIndex(workflow: WorkflowTemplate): void { ... }
  findByName(name: string): string | undefined { ... }
  findByTags(tags: string[]): string[] { ... }
  findByCategory(category: string): string[] { ... }
  findByAuthor(author: string): string[] { ... }
  clear(): void { ... }
}

// 2. 验证 → 已有 WorkflowValidator，直接注入使用
// 3. 引用检查 → 已有 checkWorkflowReferences，保持独立
// 4. 关系管理 → 已委托给 WorkflowRelationshipRegistry，保持
```

`WorkflowRegistry` 精简后：

```typescript
class WorkflowRegistry implements PersistentRegistry<WorkflowTemplate> {
  private items = new RegistryImpl<WorkflowTemplate>();
  private indexService: WorkflowIndexService;
  private relationshipRegistry: WorkflowRelationshipRegistry;
  private storageAdapter: WorkflowStorageAdapter | null;

  // 只保留核心 CRUD + 委托
  async register(workflow: WorkflowTemplate): Promise<void> { ... }
  get(id: string): WorkflowTemplate | undefined { ... }
  async unregister(id: string): Promise<void> { ... }
  // ... 其他 MutableRegistry 方法

  // 委托给 indexService
  getByName(name: string): WorkflowTemplate | undefined { ... }
  getByTags(tags: string[]): WorkflowTemplate[] { ... }

  // 委托给 relationshipRegistry
  registerSubgraphRelationship(...): void { ... }
  getWorkflowHierarchy(id: string): WorkflowHierarchy { ... }
}
```

**影响范围**：
- `WorkflowRegistry` 从 ~847 行缩减至约 300 行
- 新增 `WorkflowIndexService`（约 150 行）
- 对外接口不变，调用方无需修改

---

### 2.5 Shared Registry 重复代码消除

**目标**：消除 ToolRegistry、ScriptRegistry、NodeTemplateRegistry、HookTemplateRegistry、TriggerTemplateRegistry 之间的重复代码。

**当前**：每个 Registry 都独立实现了相同的模式（双版本注册、双版本更新、搜索、批量操作、导出/导入、存储初始化）

**方案**：提取通用 mixin 或工具函数

```typescript
// 方案 A：使用 RegistryBuilder 工具函数（推荐，避免继承层级）
function withSearchable<T>(base: MutableRegistry<T>): MutableRegistry<T> & SearchableRegistry<T> {
  return {
    ...base,
    search(query: string): T[] { ... },
    listByCategory(category: string): T[] { ... },
    listByTags(tags: string[]): T[] { ... },
  };
}

function withBatchOps<T>(base: MutableRegistry<T>): MutableRegistry<T> & BatchOperations<T> {
  return {
    ...base,
    async registerBatch(items: T[]): Promise<void> { ... },
    async unregisterBatch(keys: string[]): Promise<void> { ... },
  };
}

function withExportImport<T>(base: MutableRegistry<T>): MutableRegistry<T> & ExportableRegistry<T> {
  return {
    ...base,
    export(key: string): string { ... },
    import(json: string): string { ... },
  };
}

// 方案 B：提取通用 PersistentRegistryMixin（如果使用类继承）
class BasePersistentRegistry<T> extends PersistentRegistryImpl<T> {
  async initializeFromStorage(): Promise<void> { ... }
  search(query: string): T[] { ... }
  listByCategory(category: string): T[] { ... }
  listByTags(tags: string[]): T[] { ... }
  async registerBatch(items: T[]): Promise<void> { ... }
}
```

**影响范围**：
- 5 个 Shared Registry 每个减少约 100-200 行重复代码
- 具体的验证逻辑保留在每个 Registry 的 `register()` 或 `validate()` 方法中

---

### 2.6 WorkflowExecutionRegistry 与 AgentLoopRegistry 包装冗余消除

**目标**：消除 `registerStateCoordinator` / `getStateCoordinator` 的冗余包装。

**当前**：
```typescript
// AgentLoopRegistry
registerStateCoordinator(id, coordinator) {
  this.registerCoordinator(id, coordinator);  // 仅委托
}
getStateCoordinator(id) {
  return this.getCoordinator(id);  // 仅委托
}
// WorkflowExecutionRegistry 同理
```

**方案**：直接暴露 `CoordinatorStore` 或统一命名为 `getCoordinator` / `registerCoordinator`，不再包装。

```typescript
// AgentLoopRegistry 直接使用 CoordinatorStore 实例
class AgentLoopRegistry {
  readonly coordinators = new CoordinatorStore<AgentStateCoordinator>();
  // 外部调用：registry.coordinators.register(id, coordinator)
  // 外部调用：registry.coordinators.get(id)
}
```

**影响范围**：所有调用方需更新调用方式（从 `registry.getStateCoordinator(id)` 改为 `registry.coordinators.get(id)`）

---

### 2.7 Plugin Registry 与主 Registry 体系融合

**目标**：`BaseContributionRegistry` 实现 `MutableRegistry<T>` 接口，与主体系打通。

**当前**：
```typescript
abstract class BaseContributionRegistry<TEntry extends ContributionEntry> {
  protected entries = new Map<string, TEntry>();
  getOwner(key: string): string | undefined { ... }
  unregisterByPluginId(pluginId: string): void { ... }
  clear(): void { ... }
}
```

**方案**：
```typescript
abstract class BaseContributionRegistry<TEntry extends ContributionEntry>
  implements MutableRegistry<TEntry> {

  protected items = new Map<string, TEntry>();

  // 实现 MutableRegistry 接口
  get(key: string): TEntry | undefined { return this.items.get(key); }
  has(key: string): boolean { return this.items.has(key); }
  list(): TEntry[] { return Array.from(this.items.values()); }
  keys(): string[] { return Array.from(this.items.keys()); }
  get size(): number { return this.items.size; }
  set(key: string, value: TEntry): void { this.items.set(key, value); }
  delete(key: string): boolean { return this.items.delete(key); }
  clear(): void { this.items.clear(); }

  // 扩展方法
  getOwner(key: string): string | undefined { ... }
  unregisterByPluginId(pluginId: string): void { ... }
}
```

**影响范围**：
- 7 个贡献 Registry 自动获得 `MutableRegistry` 接口
- `ContributionManager` 可以通过统一的 `Registry` 接口操作所有子 Registry

---

### 2.8 API 层间接层消除

**目标**：消除 API 层对 Registry 的简单包装。

**当前**：`ToolRegistryAPI` 封装了 `ToolRegistry`，增加了没有实质业务逻辑的代理方法。

**方案**：
- 对于纯代理方法（`getTool`、`listTools`、`hasTool` 等），直接移除 API 层，调用方直接使用 `ToolRegistry`
- 对于有业务逻辑的方法（如 `validateTool`、`resolveDependencies`），保留在 API 层，但内部直接访问 `ToolRegistry`
- 统一 API 层的定位：只做**参数校验 + 结果格式化 + 跨 Registry 编排**，不重复封装

**影响范围**：`tool-registry-api.ts`、`agent-loop-registry-api.ts`、`agent-template-registry-api.ts` 等

---

## 三、实施阶段

### 第一阶段：统一基础层（预计 3 天）

| 任务 | 描述 |
|---|---|
| 1.1 | 实现 `RegistryImpl<T>` 基类 |
| 1.2 | 实现 `PersistentRegistryImpl<T>` 扩展类 |
| 1.3 | 实现 `ExecutionStore<T>` 和 `CoordinatorStore<T>` |
| 1.4 | 迁移 `FragmentRegistry`、`WorkflowGraphRegistry`、`AgentProfileRegistry` 到 `RegistryImpl` |
| 1.5 | 更新所有测试，确保通过 |

### 第二阶段：Shared Registry 重构（预计 3 天）

| 任务 | 描述 |
|---|---|
| 2.1 | 迁移 `ToolRegistry` 到 `PersistentRegistryImpl` |
| 2.2 | 迁移 `ScriptRegistry` 到 `PersistentRegistryImpl` |
| 2.3 | 迁移 `NodeTemplateRegistry` 到 `PersistentRegistryImpl` |
| 2.4 | 迁移 `HookTemplateRegistry` 到 `PersistentRegistryImpl` |
| 2.5 | 迁移 `TriggerTemplateRegistry` 到 `PersistentRegistryImpl` |
| 2.6 | 提取通用 `SearchableRegistry` / `BatchOperations` / `ExportableRegistry` mixin |
| 2.7 | 更新所有测试，确保通过 |

### 第三阶段：Agent & Workflow Registry 重构（预计 4 天）

| 任务 | 描述 |
|---|---|
| 3.1 | 替换 `BaseExecutionRegistry` 为 `ExecutionStore` + `CoordinatorStore` |
| 3.2 | 重构 `AgentLoopRegistry`：移除 Task 管理，改为依赖 `TaskRegistry` |
| 3.3 | 重构 `WorkflowExecutionRegistry`：替换 `BaseExecutionRegistry` |
| 3.4 | 从 `WorkflowRegistry` 提取 `WorkflowIndexService` |
| 3.5 | 精简 `WorkflowRegistry` 核心方法 |
| 3.6 | 消除 `registerStateCoordinator` / `getStateCoordinator` 冗余包装 |
| 3.7 | 更新所有测试，确保通过 |

### 第四阶段：Plugin & API 层（预计 2 天）

| 任务 | 描述 |
|---|---|
| 4.1 | `BaseContributionRegistry` 实现 `MutableRegistry` 接口 |
| 4.2 | 消除 API 层的纯代理方法 |
| 4.3 | 更新所有测试，确保通过 |

### 第五阶段：集成验证（预计 1 天）

| 任务 | 描述 |
|---|---|
| 5.1 | 运行全量单元测试 |
| 5.2 | 运行集成测试 |
| 5.3 | 运行 E2E 测试 |
| 5.4 | 代码审查 |

---

## 四、预期的度量指标

| 指标 | 当前 | 目标 |
|---|---|---|
| shared/registry 总行数 | ~200KB | ~120KB（-40%） |
| AgentLoopRegistry 行数 | 535 行 | ~250 行 |
| WorkflowRegistry 行数 | 847 行 | ~350 行 |
| 重复的 registry 模式数 | 5 份 | 1 份统一实现 |
| Registry 架构模式数 | 3 种 | 1 种（`RegistryImpl<T>`） |
| Plugin Registry 与主体系隔离 | 完全隔离 | 实现 `MutableRegistry` 接口 |
| 测试通过率 | 100% | 保持 100% |

---

## 五、风险与注意事项

1. **向后兼容性**：当前项目处于开发阶段，不要求向后兼容（见 AGENTS.md）。但接口变更需通知所有调用方同步更新。
2. **测试覆盖**：每个阶段完成后必须运行对应的测试，确保重构不引入回归。
3. **增量重构**：建议按阶段逐步实施，每个阶段完成后提交一次，避免大范围冲突。
4. **`AgentLoopRegistry` 的 Task 管理**：当前 `AgentLoopRegistry` 内部维护的 Task 状态与 `TaskRegistry` 可能存在状态不同步，迁移时需要处理数据一致性。
5. **`WorkflowRegistry` 的 `activeWorkflows`**：需要与 `WorkflowExecutionRegistry` 的 `isWorkflowActive` 方法统一数据源，避免 `WorkflowRegistry` 中 `activeWorkflows` Set 与 `WorkflowExecutionRegistry` 中实体状态不一致的问题。