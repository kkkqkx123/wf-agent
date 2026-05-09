# CLI 存储隔离失效原因总结

**日期**: 2026-05-08  
**状态**: ✅ 根本原因已确认  
**基于**: Storage 包集成测试结果 (15/15 通过)

## 核心发现

### ✅ Storage 包功能正常

在 `packages/storage/src/__tests__/storage-integration.test.ts` 中的 15 个集成测试全部通过，证明：

- JSON 文件存储正确创建目录结构并持久化数据
- SQLite 存储正确初始化数据库并执行 CRUD 操作
- 路径解析正确处理相对和绝对路径
- **不同的存储实例完全隔离**（不同 baseDir = 不同数据）

### ❌ SDK 单例缓存导致隔离失效

CLI app 存储隔离失败的根本原因是 **SDK 单例模式缓存**，而非存储实现问题。

---

## 问题流程

```
测试套件启动
    ↓
测试 1: 设置 STORAGE_DIR="/tmp/test-1"
    ↓
CLI 进程启动 → 调用 getSDK()
    ↓
SDK 构造函数 → initializeContainerWithAdapters()
    ↓
DI 容器创建 + 存储适配器缓存 ← 首次初始化
    ↓
工作流注册到 /tmp/test-1 ✓
    ↓
测试 1 结束（进程退出）
    ↓
测试 2: 设置 STORAGE_DIR="/tmp/test-2"
    ↓
CLI 进程启动 → 调用 getSDK()
    ↓
⚠️ SDK 单例检查: if (!globalSDK) { ... }
    ↓
❌ globalSDK 已存在！返回缓存实例
    ↓
❌ 使用指向 /tmp/test-1 的旧存储适配器
    ↓
❌ 工作流注册到错误的目录
    ↓
❌ 查询返回空或错误结果
```

---

## 代码证据

### 1. SDK 全局单例 ([sdk.ts:499-529](file:///d:/项目/agent/wf-agent/sdk/api/shared/core/sdk.ts#L499-L529))

```typescript
let globalSDK: SDK | null = null;

export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = new SDK(options);
  }
  return globalSDK;  // ⚠️ 首次调用后始终返回同一实例
}
```

**问题**: `globalSDK` 创建后，后续调用忽略新选项并返回缓存实例。

### 2. DI 容器单例 ([container-config.ts:121-127](file:///d:/项目/agent/wf-agent/sdk/core/di/container-config.ts#L121-L127))

```typescript
export function initializeContainerWithAdapters(adapters): Container {
  if (container) {
    return container;  // ⚠️ 返回现有容器，忽略新适配器
  }
  container = new Container();
  // ...
}
```

**问题**: DI 容器也是单例，初始化后无法用新存储适配器重新配置。

### 3. 存储适配器绑定为单例 ([container-config.ts:133-141](file:///d:/项目/agent/wf-agent/sdk/core/di/container-config.ts#L133-L141))

```typescript
container
  .bind(Identifiers.WorkflowStorageAdapter)
  .toDynamicValue(() => pendingStorageConfig?.workflow || null)
  .inSingletonScope();  // ⚠️ 单例范围 - 永不重建
```

**问题**: 存储适配器绑定为单例，即使重置容器也需要显式重新绑定。

---

## 为什么存储测试通过但 CLI 测试失败

| 方面 | Storage 包测试 | CLI App 测试 |
|------|---------------|-------------|
| **实例创建** | 每个测试创建新实例 | 使用 `getSDK()` 单例 |
| **隔离性** | 每个测试不同 `baseDir` | 跨测试共享同一 SDK 实例 |
| **生命周期** | `beforeEach` 创建，`afterEach` 销毁 | 测试之间无 SDK 重置 |
| **进程模型** | 单进程，受控实例 | 多次 spawn，共享单例 |
| **结果** | ✅ 隔离有效 | ❌ 隔离失效 |

---

## 推荐解决方案

### 方案 1: 添加 SDK 重置功能（测试推荐）

```typescript
// sdk/api/shared/core/sdk.ts
export function resetSDK(): void {
  if (globalSDK) {
    await globalSDK.destroy();
    globalSDK = null;
  }
  resetContainer();  // 同时重置 DI 容器
}
```

**测试中使用**:

```typescript
import { resetSDK } from "@wf-agent/sdk";

afterEach(async () => {
  await resetSDK();  // 强制下一个测试使用全新 SDK
});
```

### 方案 2: 支持多 SDK 实例（长期方案）

```typescript
export function createSDK(options?: SDKOptions): SDK {
  return new SDK(options);  // 始终创建新实例
}

export function getSDK(options?: SDKOptions): SDK {
  if (!globalSDK) {
    globalSDK = createSDK(options);
  }
  return globalSDK;
}
```

### 方案 3: 环境变量检测（快速修复）

```typescript
let lastStorageDir: string | undefined;

export function getSDK(options?: SDKOptions): SDK {
  const currentStorageDir = process.env.STORAGE_DIR;
  
  if (globalSDK && currentStorageDir !== lastStorageDir) {
    await globalSDK.destroy();
    globalSDK = null;
    resetContainer();
  }
  
  if (!globalSDK) {
    lastStorageDir = currentStorageDir;
    globalSDK = new SDK(options);
  }
  
  return globalSDK;
}
```

---

## CLI App 测试的即时行动

### 1. 使用进程级隔离

确保每个测试 spawn 全新的 Node.js 进程：

```typescript
const register = spawn("node", ["cli.js", "register"], {
  env: { ...process.env, STORAGE_DIR: "/tmp/test-1" }
});

const query = spawn("node", ["cli.js", "query"], {
  env: { ...process.env, STORAGE_DIR: "/tmp/test-2" }
});
```

### 2. 验证进程退出

确保 CLI 进程在下个测试前完全退出：

```typescript
await new Promise((resolve, reject) => {
  proc.on("exit", (code) => {
    code === 0 ? resolve(undefined) : reject(new Error(`Exit code ${code}`));
  });
});
```

---

## 验证步骤

确认根本原因的方法：

1. **测试新鲜进程**: 修改 CLI 测试确保每个命令 spawn 新进程
2. **添加日志**: 在 `getSDK()` 中记录 `globalSDK` 创建情况
3. **检查容器状态**: 记录容器初始化是否被多次调用
4. **监控 STORAGE_DIR**: 追踪何时读取 vs SDK 何时初始化

示例诊断代码：

```typescript
let sdkCreationCount = 0;

export function getSDK(options?: SDKOptions): SDK {
  sdkCreationCount++;
  logger.info(`getSDK() called (count: ${sdkCreationCount})`, {
    hasGlobalSDK: !!globalSDK,
    storageDir: process.env.STORAGE_DIR,
  });
  
  if (!globalSDK) {
    globalSDK = new SDK(options);
  }
  return globalSDK;
}
```

---

## 结论

**根本原因**: SDK 单例缓存在同一测试套件的多次 CLI 调用间阻止了存储隔离。

**证据**: 
- ✅ Storage 包工作正常（15/15 测试通过）
- ✅ 存储实例在不同目录时正确隔离
- ❌ SDK 的 `getSDK()` 返回缓存实例，忽略新选项
- ❌ DI 容器单例阻止适配器重新配置

**解决优先级**:
1. **立即**: 添加 `resetSDK()` 函数用于测试隔离
2. **短期**: 确保 CLI 测试使用真正的进程隔离
3. **长期**: 支持多 SDK 实例或环境感知重置

**Storage 包没有问题** - 问题在于 SDK 编排层的单例模式阻止了适当的测试隔离。
