# 日志系统架构设计

## 概述

本文档描述了 Modular Agent Framework 中日志系统的架构设计，重点说明如何在 TypeScript 多模块项目中正确使用日志器，避免循环依赖问题。

## 架构原则

### 1. 分层依赖原则

在 TypeScript/Node.js 多模块项目中，模块依赖必须遵循单向依赖原则：

```
Layer 0: 外部包 (@modular-agent/common-utils)
    ↑
Layer 1: 基础设施层 (logger.ts, types.ts)
    ↑
Layer 2: 业务模块层 (executors/, services/)
    ↑
Layer 3: 包入口层 (index.ts)
```

**规则**：
- Layer N 只能依赖 Layer N-1 及以下
- 禁止反向依赖
- 禁止同层循环依赖

### 2. 包入口职责原则

`index.ts` 的职责应该单一化：

- **推荐**：仅作为导出聚合点，不包含业务逻辑和初始化代码
- **备选**：可以包含简单的初始化代码，但必须确保不导入业务模块

## 正确的架构设计

### 目录结构

```
packages/script-executors/
├── src/
│   ├── index.ts              # Layer 3: 包入口，仅导出
│   ├── logger.ts             # Layer 1: 日志器单例
│   ├── core/                 # Layer 2: 业务模块
│   │   └── base/
│   │       └── BaseScriptExecutor.ts
│   └── shell/
│       └── ShellExecutor.ts
```

### 依赖关系图

```
                    common-utils
                         ↑
                         │
    ┌────────────────────┼────────────────────┐
    │                    │                    │
logger.ts          ShellExecutor.ts    BaseScriptExecutor.ts
    │                    │                    │
    ├─ createPackageLogger  ├─ 导入 logger      ├─ 导入 logger
    │                    │  from logger.ts    │  from logger.ts
    └─ 导出 logger        │                    │
                         │                    │
                    index.ts (仅导出)
```

### 代码示例

#### logger.ts（基础设施层）

```typescript
import { createPackageLogger, registerLogger } from '@modular-agent/common-utils';
import type { Logger } from '@modular-agent/common-utils';

/**
 * 包级别日志器
 * 单例模式，所有子模块共享此实例
 */
export const logger = createPackageLogger('script-executors', {
  level: (process.env['SCRIPT_EXECUTORS_LOG_LEVEL'] as any) || 'info',
  json: process.env['NODE_ENV'] === 'production'
});

// 注册到全局注册表，支持统一控制
registerLogger('script-executors', logger);

/**
 * 创建模块级日志器
 */
export function createModuleLogger(moduleName: string): Logger {
  return logger.child(moduleName);
}
```

#### ShellExecutor.ts（业务模块层）

```typescript
// 正确：从 logger.ts 导入，不导入 index.ts
import { logger as pkgLogger } from '../logger.js';

const logger = pkgLogger.child('shell-executor');

export class ShellExecutor {
  execute() {
    logger.info('Executing shell script');
  }
}
```

#### index.ts（包入口层）

```typescript
// 正确：仅导出，不导入业务模块
export { logger, createModuleLogger } from './logger.js';
export { ShellExecutor } from './shell/ShellExecutor.js';
export { BaseScriptExecutor } from './core/base/BaseScriptExecutor.js';
```

## 错误模式分析

### 错误模式 1：子模块导入 index.ts

```typescript
// ❌ 错误：ShellExecutor.ts 导入 index.ts
import { logger } from '../index.js';

// 这会导致循环依赖：
// index.ts → 导出 ShellExecutor
// ShellExecutor.ts → 导入 logger from index.ts
// 结果：logger 在 ShellExecutor 加载时还未初始化
```

**循环依赖链**：
```
index.ts (导出 ShellExecutor)
    ↓
ShellExecutor.ts (导入 logger from index.ts)
    ↓
index.ts (logger 还未初始化完成)
    ↓
undefined.child() → TypeError
```

### 错误模式 2：重复创建 Logger 实例

```typescript
// ❌ 错误：每个文件都创建新的 Logger 实例
// index.ts
export const logger = createPackageLogger('script-executors', { level: 'info' });

// ShellExecutor.ts
const logger = createPackageLogger('script-executors').child('shell-executor');
// 问题：创建了新的 Logger 实例，配置不一致

// BaseScriptExecutor.ts
const logger = createPackageLogger('script-executors').child('base-executor');
// 问题：又创建了新的 Logger 实例
```

**问题**：
1. 每个 `createPackageLogger` 调用都会创建新的 Logger 实例
2. 每个实例都有独立的 ConsoleStream
3. 环境变量配置只在 `index.ts` 中生效，其他文件使用默认配置
4. 无法统一控制日志级别

### 错误模式 3：Child Logger 命名冲突

```typescript
// ❌ 错误：不同文件使用相同的 child logger 名称
// CmdExecutor.ts
const logger = pkgLogger.child('cmd-executor');

// CommandLineExecutor.ts
const logger = pkgLogger.child('cmd-executor');  // 命名冲突！
```

**问题**：日志来源无法准确区分，调试困难。

## 最佳实践总结

### 1. 使用独立的 logger.ts 文件

将日志器创建逻辑放在独立的 `logger.ts` 文件中，作为基础设施层。

### 2. 子模块从 logger.ts 导入

业务模块应该从 `logger.ts` 导入日志器，而不是从 `index.ts` 导入。

### 3. 包入口仅做导出

`index.ts` 应该只负责导出聚合，不包含业务逻辑和初始化代码。

### 4. 使用 createModuleLogger 函数

提供 `createModuleLogger` 函数，确保所有子模块使用一致的配置：

```typescript
// logger.ts
export function createModuleLogger(moduleName: string): Logger {
  return logger.child(moduleName);
}

// ShellExecutor.ts
import { createModuleLogger } from '../logger.js';
const logger = createModuleLogger('shell-executor');
```

### 5. 注册到全局注册表

使用 `registerLogger` 将包级日志器注册到全局注册表，支持统一控制：

```typescript
import { registerLogger, setAllLoggersLevel } from '@modular-agent/common-utils';

// 注册
registerLogger('script-executors', logger);

// 统一控制
setAllLoggersLevel('debug');
```

## 全局日志控制 API

```typescript
// 注册 Logger
registerLogger(name: string, logger: Logger): void

// 注销 Logger
unregisterLogger(name: string): void

// 获取已注册的 Logger
getRegisteredLogger(name: string): Logger | undefined

// 获取所有已注册的 Logger 名称
getRegisteredLoggerNames(): string[]

// 设置指定 Logger 的日志级别（支持通配符）
setLoggerLevel(name: string, level: LogLevel): void
// 示例：setLoggerLevel('script-executors.*', 'debug')

// 设置所有已注册 Logger 的日志级别
setAllLoggersLevel(level: LogLevel): void
```

## 使用示例

### 统一设置日志级别

```typescript
import { setAllLoggersLevel, setLoggerLevel } from '@modular-agent/common-utils';

// 设置所有包的日志级别
setAllLoggersLevel('debug');

// 设置特定包的日志级别
setLoggerLevel('script-executors', 'debug');

// 使用通配符设置
setLoggerLevel('script-executors.*', 'debug');
```

### 环境变量控制

```bash
# 设置 script-executors 包的日志级别
SCRIPT_EXECUTORS_LOG_LEVEL=debug

# 设置 storage 包的日志级别
STORAGE_LOG_LEVEL=debug

# 生产环境使用 JSON 格式
NODE_ENV=production
```
