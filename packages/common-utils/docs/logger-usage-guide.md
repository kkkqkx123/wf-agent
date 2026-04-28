# 日志系统使用指南

## 概述

基于pino设计思想的轻量级日志系统，支持包级别和模块级别的日志管理，提供高性能的日志输出。

## 核心特性

- ✅ **Child Logger模式**：支持包级别和模块级别日志实例
- ✅ **性能优化**：预计算字符串拼接，避免运行时开销
- ✅ **异步支持**：可选的异步输出和批量处理
- ✅ **JSON格式**：可选的结构化日志输出
- ✅ **零外部依赖**：完全基于内置功能实现
- ✅ **TypeScript支持**：完整的类型定义

## 基本使用

### 1. 创建包级别日志器（推荐）

```typescript
import { createPackageLogger } from '@modular-agent/common-utils/logger';

// 创建SDK包的日志器
const sdkLogger = createPackageLogger('sdk', { 
  level: 'info',
  json: true  // 使用JSON格式输出
});

// 创建模块级别日志器
const coreLogger = sdkLogger.child('core');
const apiLogger = sdkLogger.child('api');

// 使用日志器
coreLogger.info('Workflow started', { workflowId: '123' });
apiLogger.error('API call failed', { error: 'timeout', url: '/api/workflows' });
```

### 2. 创建普通日志器

```typescript
import { createLogger } from '@modular-agent/common-utils/logger';

const logger = createLogger({ 
  level: 'debug',
  name: 'MyModule'
});

logger.debug('Debug message');
logger.info('Info message');
logger.warn('Warning message');
logger.error('Error message');
```

### 3. 异步日志输出

```typescript
import { createLogger } from '@modular-agent/common-utils/logger';

const asyncLogger = createLogger({
  level: 'info',
  async: true,        // 启用异步输出
  batchSize: 20       // 批量大小
});

// 日志会被批量异步输出，不会阻塞主线程
for (let i = 0; i < 100; i++) {
  asyncLogger.info(`Processing item ${i}`);
}
```

### 4. JSON格式输出

```typescript
import { createLogger } from '@modular-agent/common-utils/logger';

const jsonLogger = createLogger({
  level: 'info',
  json: true  // 启用JSON格式
});

jsonLogger.info('User logged in', { userId: '123', ip: '192.168.1.1' });

// 输出：
// {"level":"info","time":"2024-01-01T00:00:00.000Z","msg":"User logged in","userId":"123","ip":"192.168.1.1"}
```

## 在各个包中使用

### SDK包

```typescript
// packages/sdk/src/index.ts
import { createPackageLogger } from '@modular-agent/common-utils/logger';

// 创建SDK包的主日志器
export const logger = createPackageLogger('sdk', { level: 'info' });

// 在core模块中使用
// packages/sdk/core/index.ts
import { logger as sdkLogger } from '../index';
export const logger = sdkLogger.child('core');

logger.info('Core module initialized');
```

### Tool-Executors包

```typescript
// packages/tool-executors/src/index.ts
import { createPackageLogger } from '@modular-agent/common-utils/logger';

export const logger = createPackageLogger('tool-executors', { level: 'debug' });

// 在MCP执行器中使用
// packages/tool-executors/src/mcp/McpExecutor.ts
import { logger as pkgLogger } from '../index';
const logger = pkgLogger.child('mcp');

logger.info('MCP server connected', { serverName: 'filesystem' });
```

### Common-Utils包

```typescript
// packages/common-utils/src/index.ts
import { createPackageLogger } from '@modular-agent/common-utils/logger';

export const logger = createPackageLogger('common-utils', { level: 'warn' });
```

## 日志级别

日志级别从低到高：`debug` < `info` < `warn` < `error` < `off`

```typescript
const logger = createLogger({ level: 'warn' });

logger.debug('This will not be logged');  // 不会输出
logger.info('This will not be logged');   // 不会输出
logger.warn('This will be logged');      // 会输出
logger.error('This will be logged');     // 会输出
```

## 动态调整日志级别

```typescript
import { setGlobalLogLevel, getGlobalLogLevel } from '@modular-agent/common-utils/logger';

// 设置全局日志级别
setGlobalLogLevel('debug');

// 获取当前全局日志级别
const currentLevel = getGlobalLogLevel();
console.log(`Current level: ${currentLevel}`);

// 为特定日志器设置级别
const logger = createLogger({ level: 'info' });
logger.setLevel('debug');
```

## 上下文信息

```typescript
const logger = createLogger({ level: 'info' });

// 添加上下文信息
logger.info('Processing request', {
  requestId: '123',
  userId: '456',
  duration: 1234
});

// 输出：
// [2024-01-01T00:00:00.000Z] [INFO] Processing request {"requestId":"123","userId":"456","duration":1234}
```

## Child Logger继承

```typescript
const parentLogger = createPackageLogger('sdk', { level: 'debug' });

// 创建child logger，继承父级配置
const childLogger = parentLogger.child('core');

// child logger可以有自己的配置
childLogger.setLevel('warn');  // 只影响child logger

// parent logger的配置不受影响
console.log(parentLogger.getLevel());  // 'debug'
console.log(childLogger.getLevel());   // 'warn'
```

## 性能优化建议

1. **使用包级别日志器**：推荐使用`createPackageLogger`创建包级别日志器
2. **合理设置日志级别**：生产环境使用`info`或`warn`，开发环境使用`debug`
3. **异步输出**：在高频日志场景下启用异步输出
4. **JSON格式**：需要日志分析时使用JSON格式
5. **避免过度日志**：在热路径中减少日志输出

## 最佳实践

### 1. 在模块顶部创建日志器

```typescript
// ✅ 推荐
import { createPackageLogger } from '@modular-agent/common-utils/logger';

const logger = createPackageLogger('my-package').child('my-module');

export function myFunction() {
  logger.info('Function called');
}

// ❌ 不推荐
export function myFunction() {
  const logger = createLogger();  // 每次调用都创建新实例
  logger.info('Function called');
}
```

### 2. 使用有意义的日志消息

```typescript
// ✅ 推荐
logger.info('User login successful', { userId: '123', ip: '192.168.1.1' });

// ❌ 不推荐
logger.info('ok');
```

### 3. 合理使用日志级别

```typescript
// debug: 详细的调试信息
logger.debug('Variable value', { value: someVariable });

// info: 一般信息
logger.info('Process started', { processId: '123' });

// warn: 警告信息
logger.warn('Cache miss', { key: 'user:123' });

// error: 错误信息
logger.error('Database connection failed', { error: err.message });
```

### 4. 结构化上下文

```typescript
// ✅ 推荐：使用结构化对象
logger.info('API request', {
  method: 'POST',
  url: '/api/users',
  statusCode: 200,
  duration: 123
});

// ❌ 不推荐：字符串拼接
logger.info(`API request: POST /api/users, status: 200, duration: 123`);
```

## 迁移指南

### 从console.log迁移

```typescript
// 之前
console.log('User logged in', userId);
console.error('Error occurred', error);

// 之后
import { createPackageLogger } from '@modular-agent/common-utils/logger';

const logger = createPackageLogger('my-package');
logger.info('User logged in', { userId });
logger.error('Error occurred', { error: error.message });
```

### 从旧日志系统迁移

```typescript
// 之前
import { createLogger } from '@modular-agent/common-utils';
const logger = createLogger({ name: 'MyModule' });

// 之后
import { createPackageLogger } from '@modular-agent/common-utils/logger';
const logger = createPackageLogger('my-package').child('MyModule');
```

## 故障排查

### 日志没有输出

1. 检查日志级别是否正确设置
2. 确认日志级别是否高于当前配置的级别
3. 检查是否有自定义输出函数

```typescript
const logger = createLogger({ level: 'warn' });
logger.info('This will not be logged');  // 级别太低

// 解决方案
logger.setLevel('info');  // 或创建时设置正确的级别
```

### 性能问题

1. 在高频场景下启用异步输出
2. 减少日志输出频率
3. 使用批量处理

```typescript
const logger = createLogger({
  level: 'info',
  async: true,
  batchSize: 50
});
```

## 总结

新的日志系统提供了：
- 📦 包级别和模块级别的日志管理
- ⚡ 高性能的日志输出
- 🔧 灵活的配置选项
- 🎯 清晰的使用模式

遵循最佳实践，可以有效地使用日志系统进行调试和监控。