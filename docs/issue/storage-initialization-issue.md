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

1. **修改 SDK Bootstrap**
   - 在 `sdk-instance.ts` 中添加 `initializeStorageAdapters()` 方法
   - 在 `bootstrap()` 中调用存储适配器初始化
   - 在 `bootstrap()` 中调用 `WorkflowRegistry.initializeFromStorage()`

2. **更新测试**
   - 移除测试中的 `describe.skip`
   - 验证所有测试通过

3. **添加集成测试**
   - 添加存储适配器初始化的专门测试
   - 验证不同存储适配器的初始化顺序

4. **更新文档**
   - 更新 SDK 使用文档，说明存储适配器初始化要求

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
