# 存储模块集成问题分析

## 问题概述

在编写工作流集成测试时发现，使用 Memory 存储适配器时，`sdk.workflows.create()` 操作失败，错误信息为：

```
Storage not initialized. Call initialize() first.
```

该问题影响了所有需要持久化存储的操作，导致集成测试无法正常运行。

---

## 问题链分析

### 1. 根本原因：存储适配器未被初始化

**问题链路：**

```
SDK.bootstrap()
  → 创建 WorkflowRegistry(storageAdapter)
  → 未调用 WorkflowRegistry.initializeFromStorage()
  → 未调用 storageAdapter.initialize()
  → storageAdapter.ensureInitialized() 抛出错误
```

**关键代码位置：**

| 文件 | 代码位置 | 问题 |
|------|----------|------|
| `sdk/api/shared/core/sdk-instance.ts` | `bootstrap()` 方法 | 未调用 `WorkflowRegistry.initializeFromStorage()` |
| `sdk/workflow/stores/workflow-registry.ts` | `initializeFromStorage()` 方法 | 存在但从未被调用 |
| `packages/storage/src/memory/base-memory-storage.ts` | `ensureInitialized()` 方法 | 要求 `initialized = true` 才能操作 |
| `packages/storage/src/types/adapter/storage-adapter-base.ts` | `ensureInitialized()` 方法 | 抛出 `StorageError` |

### 2. 问题详情

#### 2.1 SDK Bootstrap 流程

当前 `SDKInstance.bootstrap()` 方法初始化了以下组件：

```typescript
// sdk/api/shared/core/sdk-instance.ts
private async bootstrap(): Promise<void> {
  // 1. 初始化 TOML 解析器
  await initializeTomlParser();

  // 2. 初始化 TaskRegistry
  const taskRegistry = container.get(TaskRegistry);
  await taskRegistry.initialize();

  // 3. 配置 MCP
  // 4. 初始化 FileCheckpointManager
  // 5. 配置 SkillRegistry
  // 6. 配置 LLM Profiles
  // 7. 配置 Validation
  // 8. 配置 Event System

  // ❌ 缺失：初始化 WorkflowRegistry
  // ❌ 缺失：初始化存储适配器
}
```

**缺失的初始化步骤：**

```typescript
// 应该添加的代码
const workflowRegistry = container.get(WorkflowRegistry);
await workflowRegistry.initializeFromStorage();
```

#### 2.2 WorkflowRegistry 初始化方法

`WorkflowRegistry` 已经提供了 `initializeFromStorage()` 方法：

```typescript
// sdk/workflow/stores/workflow-registry.ts
async initializeFromStorage(): Promise<void> {
  if (!this.storageAdapter) {
    return;
  }
  await initializeWorkflowsFromStorage(this.storageAdapter, this.workflows);
}
```

但该方法从未被 SDK bootstrap 调用。

#### 2.3 存储适配器初始化要求

所有 Memory 存储适配器继承自 `BaseMemoryStorage`，要求在使用前调用 `initialize()`：

```typescript
// packages/storage/src/memory/base-memory-storage.ts
async initialize(): Promise<void> {
  this.initialized = true;
}

protected ensureInitialized(): void {
  if (!this.initialized) {
    throw new StorageError("Storage not initialized. Call initialize() first.");
  }
}
```

每个操作方法（`save`, `load`, `delete`, `list` 等）都会先调用 `ensureInitialized()`。

---

## 影响范围

### 受影响的存储适配器

| 适配器 | 用途 | 影响 |
|--------|------|------|
| `MemoryWorkflowStorage` | 工作流定义存储 | ❌ 无法创建/更新工作流 |
| `MemoryCheckpointStorage` | 检查点存储 | ❌ 无法保存/恢复执行状态 |
| `MemoryTaskStorage` | 任务存储 | ⚠️ TaskRegistry 有独立初始化 |
| `MemoryWorkflowExecutionStorage` | 执行记录存储 | ❌ 无法记录执行历史 |

### 受影响的测试

- `sdk/__tests__/integration/workflow/workflow-execution.int.test.ts` - 所有 `sdk.workflows.create()` 测试被跳过
- `sdk/__tests__/integration/workflow/workflow-scenarios.int.test.ts` - ROUTE、错误处理、输入数据测试被跳过
- `sdk/__tests__/e2e/workflow/workflow-execution.e2e.test.ts` - E2E 测试同样受影响

---

## 解决方案

### 方案 A：在 SDK Bootstrap 中初始化存储适配器（推荐）

**修改文件：** `sdk/api/shared/core/sdk-instance.ts`

**修改内容：**

```typescript
private async bootstrap(): Promise<void> {
  // ... 现有代码 ...

  // 初始化存储适配器
  await this.initializeStorageAdapters();

  // 初始化 WorkflowRegistry
  const workflowRegistry = this.globalContext.container.get(
    ServiceIdentifiers.WorkflowRegistry
  ) as { initializeFromStorage: () => Promise<void> };
  await workflowRegistry.initializeFromStorage();

  // ... 其他初始化代码 ...
}

private async initializeStorageAdapters(): Promise<void> {
  const adapters = [
    this.config?.workflowStorageAdapter,
    this.config?.checkpointStorageAdapter,
    this.config?.taskStorageAdapter,
    this.config?.workflowExecutionStorageAdapter,
    this.config?.agentLoopCheckpointStorageAdapter,
    this.config?.triggerStorageAdapter,
    this.config?.toolStorageAdapter,
    this.config?.scriptStorageAdapter,
  ];

  for (const adapter of adapters) {
    if (adapter && 'initialize' in adapter) {
      try {
        await (adapter as { initialize: () => Promise<void> }).initialize();
      } catch (error) {
        logger.error("Failed to initialize storage adapter", {
          adapter: adapter.constructor.name,
          error: getErrorMessage(error),
        });
      }
    }
  }
}
```

**优点：**
- 符合 SDK 设计原则，统一管理初始化流程
- 一次修改解决所有存储适配器问题
- 不影响现有 API

**缺点：**
- 需要修改核心 SDK 代码

### 方案 B：在测试中手动初始化存储适配器

**修改文件：** 测试文件

**修改内容：**

```typescript
beforeEach(async () => {
  const workflowStorage = new MemoryWorkflowStorage();
  const checkpointStorage = new MemoryCheckpointStorage();
  // ... 其他存储适配器 ...

  // 手动初始化
  await workflowStorage.initialize();
  await checkpointStorage.initialize();

  sdk = createSDK({
    workflowStorageAdapter: workflowStorage,
    checkpointStorageAdapter: checkpointStorage,
    // ...
  });
  await sdk.waitForReady();
});
```

**优点：**
- 不修改 SDK 代码
- 测试可以立即运行

**缺点：**
- 每个测试文件都需要重复初始化代码
- 不是根本解决方案
- 生产环境可能仍有问题

### 方案 C：在 WorkflowRegistry 构造函数中初始化存储适配器

**修改文件：** `sdk/workflow/stores/workflow-registry.ts`

**修改内容：**

```typescript
constructor(
  private readonly storageAdapter: WorkflowStorageAdapter | null = null,
  // ...
) {
  // 初始化存储适配器
  if (storageAdapter && 'initialize' in storageAdapter) {
    storageAdapter.initialize().catch(error => {
      logger.error("Failed to initialize storage adapter", { error });
    });
  }
}
```

**优点：**
- 修改范围小

**缺点：**
- 异步初始化可能不完整
- 不符合单一职责原则
- 其他 Registry 可能也需要类似修改

---

## 推荐方案

**推荐采用方案 A**，理由如下：

1. **统一管理**：SDK bootstrap 是所有初始化逻辑的统一入口
2. **完整性**：确保所有存储适配器在使用前都已初始化
3. **可维护性**：未来添加新的存储适配器时，只需在 `initializeStorageAdapters()` 中添加
4. **符合设计原则**：DI 容器创建服务，bootstrap 负责初始化

---

## 实施步骤

1. **修改 SDK Bootstrap** ✅
   - 在 `sdk-instance.ts` 中添加 `tryInitAdapter` 辅助函数
   - 在 `bootstrap()` 中为所有 11 个存储适配器调用 `initialize()`
   - 在 `bootstrap()` 中调用 `WorkflowRegistry.initializeFromStorage()` 等 4 个 Registry 的初始化

2. **验证测试** ✅
   - 集成测试：22/22 通过（`workflow-execution.int.test.ts` + `workflow-scenarios.int.test.ts`）
   - E2E 测试：4/4 通过（`workflow-execution.e2e.test.ts`）
   - 总计：26/26 测试通过

3. **更新文档** ✅
   - 此文档已更新，包含实施细节和分析结果

---

## 其他存储实现分析

### 现有存储实现概览

SDK 中所有存储实现均继承自 `StorageAdapterBase<TMetadata, TListOptions>`，该基类定义了统一的初始化契约：

```typescript
// packages/storage/src/types/adapter/storage-adapter-base.ts
export abstract class StorageAdapterBase<...> {
  protected initialized: boolean = false;
  abstract initialize(): Promise<void>;
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new StorageError("Storage not initialized. Call initialize() first.", "initialize");
    }
  }
}
```

所有 CRUD 操作在实现类中都会调用 `this.ensureInitialized()`，因此**任何存储实现都必须在使用前调用 `initialize()`**。

### 各实现对比

| 实现 | 基类 | `initialize()` 行为 | 是否需要修复 |
|------|------|---------------------|-------------|
| **Memory** (`BaseMemoryStorage`) | `StorageAdapterBase` | 设置 `this.initialized = true` | ✅ 已修复 |
| **SQLite** (`BaseSqliteStorage`) | `StorageAdapterBase` | 创建 DB 连接、建表、启动维护定时器 | ✅ 已修复（通过相同机制） |
| **JSON** (`BaseJsonStorage`) | `StorageAdapterBase` | 创建目录结构、加载元数据索引 | ✅ 已修复（通过相同机制） |
| **Postgres** (`BasePostgresStorage`) | `StorageAdapterBase` | 创建连接池、运行 schema 迁移 | ✅ 已修复（通过相同机制） |

### 验证：SQLite 的 `ensureInitialized()` 调用

```typescript
// packages/storage/src/sqlite/base-sqlite-storage.ts
async getDb(): Promise<Database.Database> {
  this.ensureInitialized();  // ← 同样会检查 initialized
  if (!this.db) {
    throw new StorageError("Storage not initialized. Call initialize() first.", "initialize");
  }
  return this.db!;
}
```

### 验证：JSON 的 `ensureInitialized()` 调用

```typescript
// packages/storage/src/json/base-json-storage.ts
async save(id: string, ...): Promise<void> {
  this.ensureInitialized();  // ← 同样会检查 initialized
  // ...
}
```

### 结论

**所有四种存储实现（Memory、SQLite、JSON、Postgres）都需要相同的 `initialize()` 调用修复。** 修复代码中使用的 `tryInitAdapter` 辅助函数是通用的——它通过鸭子类型（检测 `initialize` 方法的存在）而非具体类型来判断，因此对所有实现都适用。

修复位于 `sdk-instance.ts` 的 `bootstrap()` 方法中：

```typescript
// 通用辅助函数：初始化任何存储适配器
const tryInitAdapter = async (adapter: unknown, name: string): Promise<void> => {
  if (adapter && typeof (adapter as { initialize: () => Promise<void> }).initialize === 'function') {
    try {
      await (adapter as { initialize: () => Promise<void> }).initialize();
      logger.debug(`${name} storage adapter initialized`);
    } catch (error) {
      logger.error(`Failed to initialize ${name} storage adapter: ${getErrorMessage(error)}`);
    }
  }
};

// 初始化所有 11 个适配器
await tryInitAdapter(this.config?.workflowStorageAdapter, "workflow");
await tryInitAdapter(this.config?.triggerStorageAdapter, "trigger");
await tryInitAdapter(this.config?.toolStorageAdapter, "tool");
await tryInitAdapter(this.config?.scriptStorageAdapter, "script");
await tryInitAdapter(this.config?.checkpointStorageAdapter, "checkpoint");
await tryInitAdapter(this.config?.taskStorageAdapter, "task");
await tryInitAdapter(this.config?.workflowExecutionStorageAdapter, "workflowExecution");
await tryInitAdapter(this.config?.agentLoopCheckpointStorageAdapter, "agentLoop");
await tryInitAdapter(this.config?.nodeTemplateStorageAdapter, "nodeTemplate");
await tryInitAdapter(this.config?.hookTemplateStorageAdapter, "hookTemplate");
await tryInitAdapter(this.config?.agentProfileStorageAdapter, "agentProfile");
```

### 警告：close() 方法尚未被调用

需要指出的是，`shutdown()` 和 `destroy()` 方法目前**没有**显式调用各存储适配器的 `close()` 方法。当前仅通过 `ContainerManager.destroyContainer()` 来清理容器资源。如果某些存储实现（如 SQLite、Postgres）需要显式关闭连接，这可能成为一个问题。建议在后续迭代中为存储适配器添加统一的 `close()` 调用机制。

---

## 相关问题

### 问题 1：Event 必须有 executionId

**错误信息：**
```
Event must have executionId
```

**原因：** 某些事件在发射时缺少 `executionId` 字段。

**影响：** 工作流执行完成后的某些事件发射失败。

**状态：** 需要单独追踪和修复。

### 问题 2：TaskRegistry 初始化与其他 Registry 不一致

**现状：** `TaskRegistry` 有独立的 `initialize()` 方法并在 bootstrap 中被调用，但其他 Registry（如 `WorkflowRegistry`）没有类似处理。

**建议：** 统一所有 Registry 的初始化模式，要么都在 bootstrap 中初始化，要么都不需要显式初始化。

---

## 附录：存储适配器接口

```typescript
interface StorageAdapter {
  // 初始化
  initialize(): Promise<void>;

  // 基本操作
  save(id: string, data: Uint8Array, metadata: TMetadata): Promise<void>;
  load(id: string): Promise<Uint8Array | null>;
  delete(id: string): Promise<void>;
  exists(id: string): Promise<boolean>;
  list(options?: TListOptions): Promise<string[]>;

  // 批量操作
  saveBatch(items: Array<{ id: string; data: Uint8Array; metadata: TMetadata }>): Promise<void>;
  deleteBatch(ids: string[]): Promise<void>;

  // 生命周期
  shutdown(): Promise<void>;
}
```

所有 Memory 存储适配器都要求在使用前调用 `initialize()`。
