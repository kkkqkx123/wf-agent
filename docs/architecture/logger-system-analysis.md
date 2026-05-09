# Logger 系统设计深度分析

## 一、缓存清理问题分析与解决方案

### 1.1 问题诊断

#### 🔴 核心问题：全局状态污染

当前 Logger 系统存在三个全局 Map 对象：

```typescript
// packages/common-utils/src/logger/lazy-logger.ts
const lazyLoggerCache: Map<string, Logger> = new Map();              // Line 17
const lazyLoggerPendingConfig: Map<string, {...}> = new Map();       // Line 23

// packages/common-utils/src/logger/logger-registry.ts  
export const loggerRegistry: Map<string, Logger> = new Map();        // Line 19
```

**问题表现**：
1. ❌ 测试之间相互污染：前一个测试创建的 logger 会影响后一个测试
2. ❌ 无法在运行时重置 logger 系统
3. ❌ 缺少生命周期管理接口

**对比项目中的其他单例服务**：

```typescript
// ✅ WorkflowExecutionPool - 有明确的清理机制
export class WorkflowExecutionPool {
  private static instance: WorkflowExecutionPool | null = null;
  
  public static reset(): void {
    this.instance = null;
  }
}

// ✅ CheckpointState - 实现了 LifecycleCapable 接口
export class CheckpointState implements LifecycleCapable<void> {
  cleanup(): void {
    this.checkpointSizes.clear();
  }
}

// ❌ Logger 系统 - 缺失这些能力
```

### 1.2 解决方案实施

#### ✅ 方案：添加显式的清理 API

已创建 `packages/common-utils/src/logger/logger-cleanup.ts`，提供以下功能：

```typescript
/**
 * 1. 清理 Lazy Logger 缓存
 */
export function clearLazyLoggerCache(): void {
  lazyLoggerCache.clear();
  lazyLoggerPendingConfig.clear();
}

/**
 * 2. 清理全局 Logger 注册表
 */
export function clearLoggerRegistry(): void {
  loggerRegistry.clear();
}

/**
 * 3. 重置全局 Logger
 */
export function resetGlobalLogger(): void {
  setGlobalLogger(new BaseLogger({ level: "info" }));
}

/**
 * 4. 完全重置 Logger 系统（主要用于测试）
 */
export function resetLoggerSystem(): void {
  clearLazyLoggerCache();
  clearLoggerRegistry();
  resetGlobalLogger();
}

/**
 * 5. 获取系统统计信息（用于调试）
 */
export function getLoggerSystemStats(): {
  lazyLoggerCount: number;
  pendingConfigCount: number;
  registeredLoggerCount: number;
}
```

#### 使用示例

```typescript
// 在测试中使用
import { clearLazyLoggerCache } from '@wf-agent/common-utils';

describe('Logger Tests', () => {
  beforeEach(() => {
    // 每个测试前清理状态，确保隔离
    clearLazyLoggerCache();
  });
  
  it('test 1', () => {
    // 干净的初始状态
  });
  
  it('test 2', () => {
    // 不受 test 1 影响
  });
});
```

### 1.3 设计权衡

| 方面 | 说明 |
|------|------|
| **为什么导出内部变量** | `lazyLoggerCache` 和 `lazyLoggerPendingConfig` 被标记为 `@internal`，仅供清理工具使用，不应在业务代码中直接访问 |
| **为什么不自动清理** | 自动清理会导致不可预测的行为，显式 API 让开发者明确控制何时清理 |
| **为什么不实现 Disposable** | Node.js 的 FinalizationRegistry 不够可靠，显式清理更可控 |
| **Exit Handlers 为何不清理** | `process.on()` 注册的事件监听器无法安全移除，且应该在整个进程生命周期中存在 |

---

## 二、Logger 是否应该全局共享？

### 2.1 当前架构分析

#### 📊 现有的三层 Logger 架构

```
┌─────────────────────────────────────────┐
│         Global Logger (单例)             │
│  - getGlobalLogger() / setGlobalLogger()│
│  - 默认 fallback logger                 │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│      Logger Registry (全局 Map)          │
│  - loggerRegistry: Map<string, Logger>  │
│  - registerLogger() / unregisterLogger()│
│  - setAllLoggersLevel()                 │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│     Package-Level Loggers (命名空间)     │
│  - cli-app                              │
│  - storage                              │
│  - tool-executors                       │
│  - sdk / sdk.graph / sdk.agent          │
└──────────────┬──────────────────────────┘
               │
┌──────────────▼──────────────────────────┐
│     Module-Level Loggers (Child)         │
│  - logger.child("moduleName")           │
│  - 继承 parent 的 stream 和 level       │
└─────────────────────────────────────────┘
```

### 2.2 全局共享的优势 ✅

#### 1. **统一的日志输出管理**

```typescript
// 所有 package 的 logger 都注册到全局 registry
registerLogger("cli-app", cliLogger);
registerLogger("storage", storageLogger);
registerLogger("sdk", sdkLogger);

// 可以统一控制所有 logger 的级别
setAllLoggersLevel("debug");  // 一键开启所有调试日志
```

**优势**：
- ✅ 集中配置：在一个地方配置所有 logger
- ✅ 一致性：确保整个应用使用相同的日志格式和输出目标
- ✅ 简化运维：生产环境中可以通过环境变量统一调整日志级别

#### 2. **进程退出时的可靠 Flush**

```typescript
// logger-registry.ts Line 144-180
export function setupExitHandlers(): void {
  process.on("exit", () => {
    flushAllLoggersSync();  // Flush 所有注册的 logger
  });
  
  process.on("SIGINT", () => {
    flushAllLoggers(() => process.exit(0));
  });
  // ...
}
```

**优势**：
- ✅ 防止日志丢失：确保所有 logger 在进程退出前 flush
- ✅ 自动化：无需每个模块单独处理退出逻辑
- ✅ 可靠性：即使是未捕获的异常也能保证日志写入

#### 3. **跨模块的日志关联**

```typescript
// 所有 logger 共享相同的 stream（文件/控制台）
const fileStream = createRotatingFileStream({...});

configureLazyLogger("storage", { stream: fileStream });
configureLazyLogger("tool-executors", { stream: fileStream });
configureSDKLogger({ stream: fileStream });
```

**优势**：
- ✅ 时间线完整：所有模块的日志按时间顺序写入同一文件
- ✅ 便于调试：追踪跨模块调用链时不需要合并多个日志文件
- ✅ 资源节约：只有一个文件句柄，减少 I/O 开销

#### 4. **符合 Monorepo 架构**

```
wf-agent/
├── apps/cli-app          ← 使用 cli-app logger
├── packages/storage      ← 使用 storage logger
├── packages/tool-executors ← 使用 tool-executors logger
└── sdk/                  ← 使用 sdk/graph/agent loggers
```

**优势**：
- ✅ 包独立性：每个包有自己的 logger，不依赖其他包的实现
- ✅ 解耦：包之间通过全局 registry 协调，而非直接引用
- ✅ 可替换性：可以轻松替换某个包的 logger 实现

### 2.3 全局共享的风险 ⚠️

#### 1. **测试隔离性问题**

**问题**：
```typescript
// Test A
const logger = createLazyLogger("test-logger", factory);
logger.info("test A");

// Test B - 可能受到 Test A 的影响
const logger = createLazyLogger("test-logger", factory);  // 返回的是同一个实例！
```

**已解决**：通过 `clearLazyLoggerCache()` 在 `beforeEach` 中清理

#### 2. **内存泄漏风险**

**问题**：
```typescript
// 如果不断创建新的 logger 但不清理
for (let i = 0; i < 10000; i++) {
  createLazyLogger(`logger-${i}`, factory);
}
// lazyLoggerCache 会无限增长
```

**缓解措施**：
- Logger 应该是长期存在的单例，不应该频繁创建
- Package-level logger 数量有限（通常 < 20 个）
- 提供了 `getLoggerSystemStats()` 用于监控

#### 3. **初始化顺序依赖**

**问题**：
```typescript
// ❌ 错误顺序
import { storageLogger } from "@wf-agent/storage";  // 触发初始化
storageLogger.info("early log");  // 使用默认配置

initSDKLogger({ level: "off" });  // 配置太晚，无效！
```

**已解决**：
- 通过 `configureLazyLogger` 预配置
- 添加警告机制检测配置时序问题
- 文档化正确的初始化顺序

### 2.4 与其他架构模式对比

| 模式 | 优势 | 劣势 | 适用场景 |
|------|------|------|----------|
| **全局共享（当前）** | 统一管理、易于配置、资源节约 | 测试隔离困难、耦合度高 | Monorepo、单一应用 |
| **完全隔离** | 测试简单、无副作用 | 配置重复、资源浪费 | 微服务、多租户 |
| **DI 容器管理** | 灵活、可测试 | 复杂度高、需要框架支持 | 大型应用、需要 mock |
| **混合模式** | 平衡灵活性和便利性 | 实现复杂 | 中等规模应用 |

**项目选择全局共享的理由**：
1. ✅ Monorepo 架构，所有包属于同一应用
2. ✅ 需要统一的日志输出和管理
3. ✅ 通过命名空间（package name）实现逻辑隔离
4. ✅ 提供了清理 API 解决测试隔离问题

### 2.5 最佳实践建议

#### ✅ 推荐做法

```typescript
// 1. 在应用启动时初始化所有 loggers
import { initAllLoggers } from './utils/logger.js';
initAllLoggers({ debug: false, verbose: true });

// 2. 在包级别导出 logger（单例）
// packages/storage/src/logger.ts
export const logger = createLazyLogger("storage", createLoggerInstance);

// 3. 模块内使用 child logger
// packages/storage/src/json/base-json-storage.ts
import { logger } from '../logger.js';
const moduleLogger = logger.child("json-storage");

// 4. 测试时清理状态
// __tests__/example.test.ts
import { clearLazyLoggerCache } from '@wf-agent/common-utils';

beforeEach(() => {
  clearLazyLoggerCache();
});
```

#### ❌ 避免的做法

```typescript
// 1. 不要在运行时动态创建大量 logger
for (const item of items) {
  const logger = createLogger(`logger-${item.id}`);  // ❌
}

// 2. 不要绕过全局 registry
const logger = createLogger("my-logger");  // ❌ 应该用 createLazyLogger + registerLogger

// 3. 不要在初始化之前使用 SDK loggers
import { sdkLogger } from '@wf-agent/sdk';
sdkLogger.info("early log");  // ❌ 应该在 initSDKLogger() 之后使用
```

---

## 三、总结与建议

### 3.1 已完成改进

| 改进项 | 状态 | 文件 |
|--------|------|------|
| 配置时序警告 | ✅ | `lazy-logger.ts` |
| 优化初始化顺序 | ✅ | `logger.ts` |
| 诊断工具函数 | ✅ | `lazy-logger.ts` |
| 文档化契约 | ✅ | `logger.ts` |
| 清理 API | ✅ | `logger-cleanup.ts` |
| 集成测试 | ✅ | `logger-initialization.test.ts` |

### 3.2 架构决策验证

**Logger 全局共享是合理的选择**，因为：

1. ✅ **符合项目架构**：Monorepo 中所有包属于同一应用
2. ✅ **提供核心价值**：统一配置、集中管理、资源节约
3. ✅ **风险可控**：通过清理 API 和命名空间解决主要问题
4. ✅ **业界实践**：类似 pino、winston 等主流库也采用类似设计

### 3.3 未来可能的改进

1. **可选的 DI 集成**：
   ```typescript
   // 为需要严格隔离的场景提供 DI 支持
   container.bind(ILogger).toFactory(() => createPackageLogger("..."));
   ```

2. **Logger 作用域管理**：
   ```typescript
   // 类似 async context，支持请求级别的 logger
   withLoggerContext({ requestId: "123" }, () => {
     // 此上下文中的所有日志自动包含 requestId
   });
   ```

3. **性能监控**：
   ```typescript
   // 监控 logger 系统的性能指标
   getLoggerSystemStats();
   // { lazyLoggerCount: 5, pendingConfigCount: 2, registeredLoggerCount: 8 }
   ```

### 3.4 关键要点

> **Logger 应该全局共享，但需要通过以下机制保证安全性**：
> 1. 命名空间隔离（package name）
> 2. 懒加载避免过早初始化
> 3. 显式清理 API 支持测试隔离
> 4. 文档化的初始化顺序契约
> 5. 警告机制检测配置问题

这套设计在保证便利性的同时，通过良好的工程实践规避了全局状态的主要风险。
