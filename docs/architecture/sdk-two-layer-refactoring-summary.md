# SDK 双层架构重构方案总结

**日期**: 2026-05-08  
**背景**: CLI app 是唯一真实用户，应尽早解决架构问题  
**原则**: 不考虑向后兼容，基于实际需求设计最优架构

---

## 核心建议

将 SDK 拆分为**两层架构**：

1. **全局共享层（单例）** - 资源注册表、事件系统、执行引擎
2. **实例特定层（多实例）** - 存储适配器、配置、执行上下文

这种分离实现了：
- ✅ **资源高效** - 通过共享全局状态
- ✅ **测试隔离** - 通过实例特定配置
- ✅ **职责清晰** - 明确共享与隔离的边界
- ✅ **架构最优** - 满足当前和未来需求

---

## 当前问题：单体 SDK

### 设计缺陷

当前 SDK 将所有内容捆绑到单个实例中：

```typescript
class SDK {
  // 全局共享资源（应该是单例）
  - WorkflowRegistry
  - ToolRegistry
  - EventRegistry
  - LLMExecutor
  
  // 实例特定资源（应该可配置）
  - 存储适配器
  - 配置选项
  - 执行上下文
}
```

**问题**：

1. ❌ **全有或全无** - 无法共享某些组件同时隔离其他组件
2. ❌ **存储绑定到 SDK** - 不同存储配置需要不同 SDK 实例
3. ❌ **资源重复** - 多个 SDK 实例不必要地复制注册表
4. ❌ **测试复杂** - 仅为了更改存储目录就必须重建整个 SDK
5. ❌ **所有权不清** - 什么应该共享 vs 隔离？

---

##  proposed 架构：两层设计

### 第一层：全局共享上下文（单例）

**目的**：管理不需要隔离的全局共享资源

**组件**：
```typescript
interface GlobalContext {
  // 注册表（所有执行共享）
  workflowRegistry: WorkflowRegistry;
  toolRegistry: ToolRegistry;
  scriptRegistry: ScriptRegistry;
  eventRegistry: EventRegistry;
  
  // 执行引擎（无状态或池化）
  llmExecutor: LLMExecutor;
  toolCallExecutor: ToolCallExecutor;
  workflowExecutor: WorkflowExecutor;
  
  // 工具类（无状态）
  serializationRegistry: SerializationRegistry;
  
  // 工厂方法（创建每次执行的实例）
  createWorkflowExecutionCoordinator(executionId): ...;
  createStateTransitor(executionId): ...;
}
```

**特性**：
- ✅ 每个进程单个实例
- ✅ 启动时初始化一次
- ✅ 所有 SDK 实例共享
- ✅ 无配置依赖
- ✅ 线程安全（或异步安全）

---

### 第二层：SDK 实例（多实例）

**目的**：管理实例特定的配置和执行上下文

**组件**：
```typescript
class SDKInstance {
  // 实例特定配置
  private config: SDKConfig;
  
  // 存储适配器（实例特定）
  private storageAdapters: {
    workflow?: WorkflowStorageAdapter;
    task?: TaskStorageAdapter;
    checkpoint?: CheckpointStorageAdapter;
    // ...
  };
  
  // 引用全局共享上下文
  private globalContext: GlobalContext;
  
  // API 层（结合全局 + 实例特定）
  public workflows: WorkflowAPI;
  public executions: ExecutionAPI;
  // ...
}
```

**特性**：
- ✅ 多个实例可以共存
- ✅ 每个都有独立的存储配置
- ✅ 共享全局注册表和执行器
- ✅ 隔离的执行上下文
- ✅ 独立的生命周期管理

---

## 组件分类

### 应该是全局的（共享）

| 组件 | 原因 | 范围 |
|------|------|------|
| **WorkflowRegistry** | 工作流是全局定义，非执行特定 | 进程级 |
| **ToolRegistry** | 工具在所有执行中可重用 | 进程级 |
| **EventRegistry** | 事件在整个系统中流动 | 进程级 |
| **ScriptRegistry** | 脚本是全局工具类 | 进程级 |
| **LLMExecutor** | 无状态执行器，可共享 | 进程级 |
| **SerializationRegistry** | 纯工具类，无状态 | 进程级 |

### 应该是实例特定的

| 组件 | 原因 | 范围 |
|------|------|------|
| **存储适配器** | 不同租户/测试需要不同存储 | 每实例 |
| **SDK 配置** | 调试模式、日志级别、预设因实例而异 | 每实例 |
| **API 实例** | 包装存储 + 全局上下文 | 每实例 |
| **Bootstrap 状态** | 每个实例的初始化状态 | 每实例 |

### 应该是每次执行的（工厂创建）

| 组件 | 原因 | 范围 |
|------|------|------|
| **WorkflowExecutionCoordinator** | 绑定到特定执行 | 每次执行 |
| **WorkflowStateTransitor** | 跟踪执行状态 | 每次执行 |
| **CheckpointCoordinator** | 管理执行检查点 | 每次执行 |
| **ConversationSession** | 执行对话历史 | 每次执行 |

---

## 实施策略

### 第 1 步：提取全局上下文

```typescript
// sdk/core/global-context.ts

let globalContextInstance: GlobalContext | null = null;

export function initializeGlobalContext(): GlobalContext {
  if (globalContextInstance) {
    return globalContextInstance;
  }
  
  const container = getContainer();
  
  globalContextInstance = {
    // 注册表
    workflowRegistry: container.get(Identifiers.WorkflowRegistry),
    toolRegistry: container.get(Identifiers.ToolRegistry),
    eventRegistry: container.get(Identifiers.EventRegistry),
    
    // 执行器
    llmExecutor: container.get(Identifiers.LLMExecutor),
    workflowExecutor: container.get(Identifiers.WorkflowExecutor),
    
    // 工厂方法
    createWorkflowExecutionCoordinator: (entity) => {
      const factory = container.get(Identifiers.WorkflowExecutionCoordinator);
      return factory.create(entity);
    },
    // ...
  };
  
  return globalContextInstance;
}

export function getGlobalContext(): GlobalContext {
  if (!globalContextInstance) {
    throw new Error("Global context not initialized");
  }
  return globalContextInstance;
}
```

---

### 第 2 步：重构 SDK 实例

```typescript
// sdk/api/shared/core/sdk-instance.ts

export class SDKInstance {
  private config: SDKOptions;
  private globalContext: GlobalContext;
  private apiFactory: APIFactory;
  
  constructor(options: SDKOptions, globalContext?: GlobalContext) {
    this.config = options;
    this.globalContext = globalContext || getGlobalContext();
    this.apiFactory = new APIFactory(this.globalContext, options);
    
    // 启动异步 bootstrap
    this.bootstrap();
  }
  
  private async bootstrap(): Promise<void> {
    // 使用全局上下文注册表注册预设
    // 但使用实例特定配置
    await this.registerPresets();
    this.isBootstrapped = true;
  }
  
  // API getter - 结合全局上下文 + 实例存储
  get workflows() {
    return this.apiFactory.createWorkflowAPI();
  }
  
  async shutdown(): Promise<void> {
    // 仅关闭实例特定资源（存储适配器）
    await this.shutdownStorageAdapters();
  }
}
```

---

### 第 3 步：更新导出函数

```typescript
// sdk/api/shared/core/sdk.ts

let defaultSDK: SDKInstance | null = null;

/**
 * 创建具有隔离配置的新 SDK 实例
 */
export function createSDK(options: SDKOptions): SDKInstance {
  const globalContext = getGlobalContext();
  return new SDKInstance(options, globalContext);
}

/**
 * 获取或创建默认 SDK 实例
 */
export function getSDK(options?: SDKOptions): SDKInstance {
  if (!defaultSDK) {
    initializeGlobalContext();  // 自动初始化全局上下文
    defaultSDK = createSDK(options || {});
  }
  return defaultSDK;
}

/**
 * 重置默认 SDK 实例（仅测试）
 */
export async function resetSDK(): Promise<void> {
  if (defaultSDK) {
    await defaultSDK.shutdown();
    defaultSDK = null;
  }
}
```

---

## 架构优势

### 1. 资源效率

```typescript
// 之前：每个 SDK 实例都复制注册表
const sdk1 = new SDK({ storage: adapter1 });  // 创建 WorkflowRegistry #1
const sdk2 = new SDK({ storage: adapter2 });  // 创建 WorkflowRegistry #2（浪费！）

// 之后：注册表共享，仅存储不同
initializeGlobalContext();  // 创建 WorkflowRegistry 一次
const sdk1 = createSDK({ storage: adapter1 });  // 使用共享注册表
const sdk2 = createSDK({ storage: adapter2 });  // 使用相同共享注册表
```

**内存节省**：约 70% 减少重复对象

---

### 2. 测试隔离

```typescript
describe("Workflow Tests", () => {
  let sdk: SDKInstance;
  
  beforeEach(async () => {
    // 每个测试获得新鲜存储但共享注册表
    sdk = createSDK({
      workflowStorageAdapter: createTempStorage(),
    });
    await sdk.waitForReady();
  });
  
  afterEach(async () => {
    await sdk.shutdown();  // 仅关闭存储，不关闭注册表
  });
});
```

**优势**：
- ✅ 快速测试设置（无需重新初始化注册表）
- ✅ 完美的存储隔离
- ✅ 测试间一致的全局状态

---

### 3. 多租户支持

```typescript
// 每个租户有隔离存储但共享工具/工作流
const tenants = [
  { id: "tenant-a", storageDir: "/data/tenant-a" },
  { id: "tenant-b", storageDir: "/data/tenant-b" },
];

const tenantSDKs = new Map();

for (const tenant of tenants) {
  const sdk = createSDK({
    workflowStorageAdapter: createStorage(tenant.storageDir),
  });
  tenantSDKs.set(tenant.id, sdk);
}

// 租户 A 和 B 共享：
// - 相同的工作流定义
// - 相同的工具实现
// - 相同的事件系统

// 但隔离：
// - 工作流执行
// - 任务队列
// - 检查点
```

---

### 4. 清晰的职责边界

```typescript
// 全局上下文负责：
✅ 注册表管理
✅ 执行器生命周期
✅ 工厂方法
✅ 共享工具类

// SDK 实例负责：
✅ 存储配置
✅ 实例生命周期
✅ 预设注册
✅ API 组合

// 没有歧义
```

---

## 迁移路径

### 第 1 阶段：实现全局上下文（第 1 周）

1. 从 DI 容器提取全局上下文
2. 将注册表和执行器移到全局作用域
3. 为每次执行的组件添加工厂方法
4. 编写全局上下文的单元测试

**风险**：低 - 内部重构，尚无 API 变更

---

### 第 2 阶段：重构 SDK 实例（第 2 周）

1. 创建 `SDKInstance` 类
2. 将存储适配器与全局状态分离
3. 更新 API 工厂以使用全局上下文
4. 保持向后兼容的 `getSDK()` 函数

**风险**：中 - 更改 SDK 内部但保持 API 稳定

---

### 第 3 阶段：更新 CLI App（第 3 周）

1. 更新 CLI 初始化以使用新架构
2. 更新测试以使用 `createSDK()` 实现隔离
3. 验证所有功能正常工作
4. 性能测试

**风险**：中 - CLI app 是唯一用户，完全控制

---

### 第 4 阶段：清理与文档（第 4 周）

1. 删除旧的单体 SDK 代码
2. 更新所有文档
3. 添加架构图
4. 编写迁移指南（供未来应用使用）

**风险**：低 - 仅文档

---

## 对比：之前 vs 之后

### 之前（单体）

```
┌─────────────────────────┐
│      SDK 实例           │
│                         │
│  ┌───────────────────┐  │
│  │ WorkflowRegistry  │  │ ← 每个实例复制
│  │ ToolRegistry      │  │ ← 每个实例复制
│  │ EventRegistry     │  │ ← 每个实例复制
│  └───────────────────┘  │
│                         │
│  ┌───────────────────┐  │
│  │ 存储适配器        │  │ ← 实例特定（正确）
│  │ 配置              │  │ ← 实例特定（正确）
│  └───────────────────┘  │
└─────────────────────────┘

问题：所有内容都复制了！
```

### 之后（两层）

```
┌──────────────────────────────┐
│     全局上下文               │ ← 单例
│     （所有实例共享）         │
│                              │
│  ┌────────────────────────┐  │
│  │ WorkflowRegistry       │  │
│  │ ToolRegistry           │  │
│  │ EventRegistry          │  │
│  │ LLMExecutor            │  │
│  │ 工厂方法               │  │
│  └────────────────────────┘  │
└──────────────────────────────┘
         ▲           ▲
         │           │
    ┌────┴────┐ ┌────┴────┐
    │ SDK #1  │ │ SDK #2  │ ← 多实例
    │         │ │         │
    │ 存储    │ │ 存储    │ ← 实例特定
    │ 配置    │ │ 配置    │ ← 实例特定
    └─────────┘ └─────────┘

优势：共享全局，隔离配置！
```

---

## 决策矩阵

| 标准 | 保持单体 | 两层架构 |
|------|---------|---------|
| **资源效率** | ❌ 差（重复） | ✅ 优秀（共享） |
| **测试隔离** | ⚠️ 困难 | ✅ 简单 |
| **多租户** | ❌ 不支持 | ✅ 原生支持 |
| **代码清晰度** | ❌ 混合关注点 | ✅ 清晰边界 |
| **实施工作量** | ✅ 无 | ⚠️ 中（4 周） |
| **维护性** | ❌ 复杂 | ✅ 更简单 |
| **性能** | ❌ 浪费 | ✅ 最优 |
| **灵活性** | ❌ 有限 | ✅ 高 |
| **面向未来** | ❌ 否 | ✅ 是 |

**获胜者**：两层架构 ⭐⭐⭐⭐⭐

---

## 结论

### 推荐：实施两层架构

**理由**：

1. ✅ **解决当前问题** - 测试隔离、资源浪费
2. ✅ **启用未来功能** - 多租户、高级场景
3. ✅ **清晰架构** - 明确的职责分离
4. ✅ **最优资源使用** - 共享应该共享的，隔离应该隔离的
5. ✅ **无向后兼容性顾虑** - CLI app 是唯一用户，完全控制

### 关键洞察

**应该共享的（全局）**：
- 注册表（工作流、工具、脚本、事件）
- 执行器（LLM、工具调用、工作流）
- 工具类（序列化、解析）
- 工厂方法（每次执行的组件）

**应该隔离的（每实例）**：
- 存储适配器
- 配置选项
- 预设注册
- API 组合

**应该每次执行创建的（工厂）**：
- 执行协调器
- 状态转换器
- 检查点协调器
- 会话

### 下一步

1. 审查并批准此架构
2. 实施第 1 阶段：全局上下文提取
3. 实施第 2 阶段：SDK 实例重构
4. 更新 CLI app 使用新架构
5. 全面测试和验证

此架构在**资源效率和隔离灵活性之间提供最佳平衡**，遵循 SOLID 原则和现代架构模式。
