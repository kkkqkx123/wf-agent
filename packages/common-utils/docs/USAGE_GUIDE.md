# Logger 使用指南

基于流的日志系统，参考pino设计思想，支持多种输出方式和灵活的配置。

## 概述

新的logger系统采用基于流的架构，具有以下特点：
- **流式输出**：通过`LogStream`接口统一管理日志输出
- **多种输出方式**：支持控制台、文件、异步、多目标等输出
- **灵活配置**：支持JSON/普通格式、彩色输出、级别过滤等
- **高性能**：异步stream不阻塞主线程，支持批量处理
- **易于扩展**：实现`LogStream`接口即可添加新的输出方式

## 快速开始

### 基础使用

```typescript
import { createLogger } from '@common-utils/logger';

// 创建默认logger（输出到console，普通格式）
const logger = createLogger({ level: 'info' });

logger.info('Hello, World!');
logger.debug('Debug message', { userId: 123 });
logger.warn('Warning message');
logger.error('Error message', { error: new Error('Something went wrong') });
```

### 使用不同的输出格式

```typescript
// JSON格式输出
const jsonLogger = createLogger({
  level: 'info',
  json: true
});

// 彩色输出（开发环境推荐）
const prettyLogger = createLogger({
  level: 'info',
  pretty: true
});

// 不包含时间戳
const noTimestampLogger = createLogger({
  level: 'info',
  timestamp: false
});
```

## Stream 使用

### 1. Console Stream（控制台输出）

```typescript
import { createConsoleStream, createLogger } from '@common-utils/logger';

// 创建console stream
const consoleStream = createConsoleStream({
  json: false,        // 使用普通格式
  timestamp: true,    // 包含时间戳
  pretty: true        // 彩色输出
});

// 使用stream创建logger
const logger = createLogger({
  level: 'info',
  stream: consoleStream
});

logger.info('This is a pretty colored message');
```

### 2. File Stream（文件输出）

```typescript
import { createFileStream, createLogger } from '@common-utils/logger';

// 创建文件stream
const fileStream = createFileStream({
  filePath: './logs/app.log',  // 文件路径
  json: true,                   // JSON格式
  timestamp: true,              // 包含时间戳
  append: true                  // 追加模式
});

const logger = createLogger({
  level: 'info',
  stream: fileStream
});

logger.info('This will be written to file');
```

### 3. Async Stream（异步输出）

异步stream使用队列批量处理日志，不阻塞主线程，适合高并发场景。

```typescript
import { createConsoleStream, createAsyncStream, createLogger } from '@common-utils/logger';

// 创建目标stream
const consoleStream = createConsoleStream();

// 包装为异步stream
const asyncStream = createAsyncStream(consoleStream, {
  batchSize: 20  // 批量大小
});

const logger = createLogger({
  level: 'info',
  stream: asyncStream
});

// 日志会被批量异步处理
for (let i = 0; i < 100; i++) {
  logger.info(`Message ${i}`);
}
```

### 4. Multistream（多目标输出）

支持同时输出到多个stream，每个stream可以设置不同的日志级别。

```typescript
import { 
  createConsoleStream, 
  createFileStream, 
  createMultistream, 
  createLogger 
} from '@common-utils/logger';

// 创建多个stream
const consoleStream = createConsoleStream({ 
  pretty: true,
  json: false 
});

const fileStream = createFileStream({ 
  filePath: './logs/app.log',
  json: true 
});

const errorFileStream = createFileStream({ 
  filePath: './logs/error.log',
  json: true 
});

// 创建multistream
const multiStream = createMultistream([
  { stream: consoleStream, level: 'info' },      // info及以上输出到console
  { stream: fileStream, level: 'debug' },        // debug及以上输出到文件
  { stream: errorFileStream, level: 'error' }    // 只输出error到错误文件
]);

const logger = createLogger({
  level: 'debug',
  stream: multiStream
});

logger.debug('Debug message');    // 只输出到app.log
logger.info('Info message');      // 输出到console和app.log
logger.warn('Warn message');      // 输出到console和app.log
logger.error('Error message');    // 输出到所有目标
```

#### Multistream 高级用法

```typescript
// 动态添加stream
multiStream.add({
  stream: createFileStream({ filePath: './logs/new.log' }),
  level: 'warn'
});

// 克隆multistream（可以指定不同的级别）
const warnOnlyStream = multiStream.clone('warn');

// 去重模式（只输出到最高级别的stream）
const dedupeStream = createMultistream([
  { stream: consoleStream, level: 'info' },
  { stream: fileStream, level: 'warn' }
], { dedupe: true });
```

## Transport 使用

### 1. Destination（目标工厂）

`destination()`函数可以创建各种目标stream。

```typescript
import { destination, createLogger } from '@common-utils/logger';

// 输出到文件
const fileLogger = createLogger({
  level: 'info',
  stream: destination('./logs/app.log')
});

// 输出到stdout
const stdoutLogger = createLogger({
  level: 'info',
  stream: destination(process.stdout)
});

// 输出到stderr
const stderrLogger = createLogger({
  level: 'info',
  stream: destination(process.stderr)
});

// 使用文件描述符
const fdLogger = createLogger({
  level: 'info',
  stream: destination(1)  // 1 = stdout, 2 = stderr
});
```

### 2. Transport（传输工厂）

`transport()`函数支持更复杂的配置，兼容pino的transport格式。

```typescript
import { transport, createLogger } from '@common-utils/logger';

// 单目标transport
const logger1 = createLogger({
  level: 'info',
  stream: transport({
    target: 'console'
  })
});

// 文件transport
const logger2 = createLogger({
  level: 'info',
  stream: transport({
    target: './logs/app.log'
  })
});

// 多目标transport
const logger3 = createLogger({
  level: 'debug',
  stream: transport({
    targets: [
      { target: 'console', level: 'info' },
      { target: './logs/app.log', level: 'warn' },
      { target: './logs/error.log', level: 'error' }
    ],
    dedupe: true  // 启用去重
  })
});
```

## Logger 功能

### Child Logger（子记录器）

Child logger继承父logger的配置和stream，并可以添加额外的上下文。

```typescript
import { createLogger } from '@common-utils/logger';

const parentLogger = createLogger({
  level: 'info',
  name: 'my-app'
});

// 创建child logger
const authLogger = parentLogger.child('auth', { userId: 123 });
const dbLogger = parentLogger.child('database');

parentLogger.info('Parent message');
authLogger.info('Child message');
dbLogger.info('Database query');

// 输出会包含module信息
// { level: 'info', message: 'Child message', context: { module: 'auth', userId: 123 } }
```

### 包级别Logger

```typescript
import { createPackageLogger } from '@common-utils/logger';

const logger = createPackageLogger('my-package', {
  level: 'info',
  json: true
});

logger.info('Package message');
// 输出会包含pkg信息
// { level: 'info', message: 'Package message', context: { pkg: 'my-package' } }
```

### 全局Logger

```typescript
import { 
  setGlobalLogger, 
  getGlobalLogger, 
  setGlobalLogLevel,
  getGlobalLogLevel 
} from '@common-utils/logger';

// 设置全局logger
setGlobalLogger(createLogger({ level: 'debug' }));

// 获取全局logger
const logger = getGlobalLogger();
logger.info('Global logger message');

// 设置全局日志级别
setGlobalLogLevel('warn');

// 获取全局日志级别
const level = getGlobalLogLevel();
console.log('Current level:', level);
```

### 刷新缓冲区

```typescript
import { createLogger, createFileStream } from '@common-utils/logger';

const fileStream = createFileStream({ filePath: './logs/app.log' });
const logger = createLogger({ level: 'info', stream: fileStream });

// 写入日志
logger.info('Message 1');
logger.info('Message 2');

// 刷新缓冲区（确保日志写入文件）
logger.flush(() => {
  console.log('Logs flushed to file');
});
```

## 高级用法

### 自定义Stream

实现`LogStream`接口即可创建自定义的输出方式。

```typescript
import type { LogStream, LogEntry } from '@common-utils/logger';
import { createLogger } from '@common-utils/logger';

class CustomStream implements LogStream {
  write(entry: LogEntry): void {
    // 自定义输出逻辑
    console.log('[CUSTOM]', JSON.stringify(entry));
  }

  flush(callback?: () => void): void {
    if (callback) callback();
  }

  end(): void {
    console.log('[CUSTOM] Stream ended');
  }

  on(event: string, handler: Function): void {
    // 可选：实现事件监听
  }

  off(event: string, handler: Function): void {
    // 可选：实现事件移除
  }
}

const customStream = new CustomStream();
const logger = createLogger({
  level: 'info',
  stream: customStream
});

logger.info('Custom stream message');
```

### 组合多个Stream

```typescript
import { 
  createConsoleStream, 
  createFileStream, 
  createAsyncStream,
  createMultistream,
  createLogger 
} from '@common-utils/logger';

// 创建异步文件stream
const asyncFileStream = createAsyncStream(
  createFileStream({ filePath: './logs/app.log' }),
  { batchSize: 50 }
);

// 创建彩色console stream
const prettyConsoleStream = createConsoleStream({ 
  pretty: true,
  json: false 
});

// 组合多个stream
const multiStream = createMultistream([
  { stream: prettyConsoleStream, level: 'info' },
  { stream: asyncFileStream, level: 'debug' }
]);

const logger = createLogger({
  level: 'debug',
  stream: multiStream
});
```

### 日志级别过滤

```typescript
import { createMultistream, createConsoleStream, createFileStream, createLogger } from '@common-utils/logger';

const multiStream = createMultistream([
  { stream: createConsoleStream(), level: 'info' },  // 只输出info及以上
  { stream: createFileStream({ filePath: './logs/debug.log' }), level: 'debug' },  // 输出所有级别
  { stream: createFileStream({ filePath: './logs/error.log' }), level: 'error' }  // 只输出error
]);

const logger = createLogger({
  level: 'debug',
  stream: multiStream
});

logger.debug('Debug message');  // 只输出到debug.log
logger.info('Info message');    // 输出到console和debug.log
logger.error('Error message');  // 输出到所有文件
```

## 最佳实践

### 1. 使用包级别Logger

```typescript
// 在包的入口文件中
import { createPackageLogger } from '@common-utils/logger';

export const logger = createPackageLogger('my-package', {
  level: process.env.LOG_LEVEL || 'info',
  json: process.env.NODE_ENV === 'production',
  pretty: process.env.NODE_ENV !== 'production'
});

// 在包的其他文件中使用
import { logger } from './index';

logger.info('Package initialized');
```

### 2. 使用Child Logger组织模块

```typescript
const appLogger = createLogger({ 
  name: 'my-app',
  level: 'info'
});

const authLogger = appLogger.child('auth');
const dbLogger = appLogger.child('database');
const apiLogger = appLogger.child('api');

authLogger.info('User logged in');
dbLogger.info('Query executed');
apiLogger.info('API request received');
```

### 3. 生产环境配置

```typescript
const logger = createLogger({
  level: process.env.LOG_LEVEL || 'info',
  json: process.env.NODE_ENV === 'production',
  pretty: process.env.NODE_ENV !== 'production',
  timestamp: true
});
```

### 4. 使用Multistream实现日志分级

```typescript
import { 
  createConsoleStream, 
  createFileStream, 
  createMultistream,
  createLogger 
} from '@common-utils/logger';

const multiStream = createMultistream([
  // 开发环境：彩色console输出
  { 
    stream: createConsoleStream({ 
      pretty: true,
      json: false 
    }), 
    level: 'info' 
  },
  // 生产环境：所有日志写入文件
  { 
    stream: createFileStream({ 
      filePath: './logs/app.log',
      json: true 
    }), 
    level: 'debug' 
  },
  // 错误日志单独存储
  { 
    stream: createFileStream({ 
      filePath: './logs/error.log',
      json: true 
    }), 
    level: 'error' 
  }
]);

const logger = createLogger({
  level: 'debug',
  stream: multiStream
});
```

### 5. 使用Async Stream提高性能

```typescript
import { createConsoleStream, createAsyncStream, createLogger } from '@common-utils/logger';

// 在高并发场景下使用异步stream
const asyncStream = createAsyncStream(
  createConsoleStream(),
  { batchSize: 50 }
);

const logger = createLogger({
  level: 'info',
  stream: asyncStream
});

// 批量日志不会阻塞主线程
for (let i = 0; i < 1000; i++) {
  logger.info(`Processing item ${i}`);
}
```

### 6. 错误处理

```typescript
import { createLogger, createFileStream } from '@common-utils/logger';

const fileStream = createFileStream({ filePath: './logs/app.log' });

// 监听stream错误
fileStream.on('error', (err) => {
  console.error('Stream error:', err);
});

const logger = createLogger({
  level: 'info',
  stream: fileStream
});

// 记录错误
try {
  // 一些可能出错的操作
} catch (error) {
  logger.error('Operation failed', { error });
}
```

## API 参考

### 主要类型

- `Logger`: 日志器接口
- `LogStream`: 日志输出流接口
- `LogLevel`: 日志级别类型 ('debug' | 'info' | 'warn' | 'error' | 'off')
- `LoggerOptions`: 日志器配置选项
- `StreamOptions`: Stream配置选项
- `LogEntry`: 日志条目格式

### 主要函数

#### Logger创建
- `createLogger(options)`: 创建日志器实例
- `createPackageLogger(pkg, options)`: 创建包级别日志器
- `createConsoleLogger(level)`: 创建默认console日志器
- `createNoopLogger()`: 创建空操作日志器

#### Stream创建
- `createConsoleStream(options)`: 创建控制台stream
- `createFileStream(options)`: 创建文件stream
- `createAsyncStream(targetStream, options)`: 创建异步stream
- `createMultistream(streams, options)`: 创建多目标stream

#### Transport
- `destination(dest)`: 创建目标stream
- `transport(options)`: 创建transport stream

#### 全局管理
- `setGlobalLogger(logger)`: 设置全局logger
- `getGlobalLogger()`: 获取全局logger
- `setGlobalLogLevel(level)`: 设置全局日志级别
- `getGlobalLogLevel()`: 获取全局日志级别

#### 工具函数
- `shouldLog(currentLevel, messageLevel)`: 检查是否应该输出日志
- `formatTimestamp()`: 格式化时间戳
- `mergeContext(base, additional)`: 合并上下文
- `createLogEntry(level, message, context, timestamp)`: 创建日志条目

## 迁移指南

如果你之前使用的是旧版本的logger，迁移到新版本非常简单：

### 旧版本
```typescript
import { createLogger } from '@common-utils/logger';

const logger = createLogger({
  level: 'info',
  async: true,
  json: true
});
```

### 新版本
```typescript
import { createLogger, createConsoleStream, createAsyncStream } from '@common-utils/logger';

const logger = createLogger({
  level: 'info',
  stream: createAsyncStream(
    createConsoleStream({ json: true })
  )
});
```

新的API更加灵活，可以轻松组合不同的stream实现。