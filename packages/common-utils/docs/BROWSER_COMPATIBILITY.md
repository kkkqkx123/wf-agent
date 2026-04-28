# Logger 浏览器兼容性设计方案

## 一、背景分析

### 1.1 Pino Browser.js 设计思路

通过分析 `ref/pino-10.3.1/browser.js`，Pino在浏览器环境的设计特点：

1. **Console API适配**：使用浏览器console对象，提供fallback机制
2. **Transmit功能**：支持将日志发送到远程服务器
3. **序列化器**：支持自定义序列化，特别是错误对象
4. **格式化器**：支持自定义日志格式
5. **Caller信息**：通过stack trace获取调用位置
6. **GlobalThis兼容**：使用polyfill支持不同环境的globalThis

### 1.2 当前Logger系统的浏览器兼容性问题

#### Node.js依赖
- **FileStream**：使用`fs`模块，浏览器不可用
- **AsyncStream**：使用`setImmediate`，浏览器不支持
- **时间戳格式**：使用`toISOString()`，基本兼容

#### 浏览器环境限制
- 无法直接写入文件系统
- 没有进程级别的stdout/stderr
- 异步API差异（setImmediate vs setTimeout）
- Console API差异（颜色支持、分组等）

### 1.3 设计原则

根据用户要求：
- **应用层集成**：浏览器兼容性改造在应用层完成
- **核心层保持**：SDK层的logger保持Node.js环境设计
- **最小侵入**：不修改核心logger代码
- **灵活适配**：提供多种适配方案

## 二、应用层兼容方案

### 2.1 方案一：浏览器专用Stream实现

在应用层创建浏览器专用的stream实现，包装核心logger。

#### 2.1.1 BrowserConsoleStream

```typescript
// apps/web-app/src/logger/browser-streams.ts
import type { LogStream, LogEntry } from '@common-utils/logger';

/**
 * 浏览器Console Stream
 * 适配浏览器console API
 */
export class BrowserConsoleStream implements LogStream {
  private json: boolean;
  private timestamp: boolean;
  private pretty: boolean;

  constructor(options: { json?: boolean; timestamp?: boolean; pretty?: boolean } = {}) {
    this.json = options.json ?? false;
    this.timestamp = options.timestamp ?? true;
    this.pretty = options.pretty ?? true;
  }

  write(entry: LogEntry): void {
    const { level, message, timestamp, context, ...rest } = entry;
    
    if (this.json) {
      // JSON格式输出
      console.log(JSON.stringify(entry));
    } else {
      // 使用浏览器console方法
      const timestampStr = timestamp ? `[${timestamp}] ` : '';
      const levelStr = `[${level.toUpperCase()}] `;
      const extraData = { ...context, ...rest };
      
      const formattedMessage = `${timestampStr}${levelStr}${message}`;
      
      // 根据级别选择console方法
      switch (level) {
        case 'debug':
          console.debug(formattedMessage, extraData);
          break;
        case 'warn':
          console.warn(formattedMessage, extraData);
          break;
        case 'error':
          console.error(formattedMessage, extraData);
          break;
        default:
          console.log(formattedMessage, extraData);
      }
    }
  }

  flush(callback?: () => void): void {
    if (callback) {
      setTimeout(callback, 0);
    }
  }

  end(): void {
    // console无需结束
  }
}
```

#### 2.1.2 BrowserTransmitStream

```typescript
/**
 * 浏览器Transmit Stream
 * 将日志发送到远程服务器
 */
export class BrowserTransmitStream implements LogStream {
  private endpoint: string;
  private batchSize: number;
  private buffer: LogEntry[] = [];
  private flushTimer?: number;

  constructor(options: {
    endpoint: string;
    batchSize?: number;
    flushInterval?: number;
  }) {
    this.endpoint = options.endpoint;
    this.batchSize = options.batchSize ?? 10;
    
    // 定期刷新
    if (options.flushInterval) {
      this.flushTimer = window.setInterval(() => {
        this.flush();
      }, options.flushInterval);
    }
  }

  write(entry: LogEntry): void {
    this.buffer.push(entry);
    
    if (this.buffer.length >= this.batchSize) {
      this.flush();
    }
  }

  flush(callback?: () => void): void {
    if (this.buffer.length === 0) {
      if (callback) callback();
      return;
    }

    const entries = this.buffer.splice(0);
    
    // 使用fetch API发送日志
    fetch(this.endpoint, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify(entries),
      keepalive: true // 使用keepalive确保页面关闭时也能发送
    }).catch(err => {
      console.error('Failed to transmit logs:', err);
    }).finally(() => {
      if (callback) callback();
    });
  }

  end(): void {
    this.flush();
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
    }
  }
}
```

#### 2.1.3 BrowserLocalStorageStream

```typescript
/**
 * 浏览器LocalStorage Stream
 * 将日志存储到localStorage
 */
export class BrowserLocalStorageStream implements LogStream {
  private key: string;
  private maxSize: number;

  constructor(options: {
    key?: string;
    maxSize?: number; // 最大存储条数
  }) {
    this.key = options.key ?? 'app-logs';
    this.maxSize = options.maxSize ?? 1000;
  }

  write(entry: LogEntry): void {
    try {
      const logs = this.getLogs();
      logs.push(entry);
      
      // 限制大小
      if (logs.length > this.maxSize) {
        logs.splice(0, logs.length - this.maxSize);
      }
      
      localStorage.setItem(this.key, JSON.stringify(logs));
    } catch (err) {
      console.error('Failed to write to localStorage:', err);
    }
  }

  flush(callback?: () => void): void {
    if (callback) callback();
  }

  end(): void {
    // localStorage无需结束
  }

  private getLogs(): LogEntry[] {
    try {
      const data = localStorage.getItem(this.key);
      return data ? JSON.parse(data) : [];
    } catch {
      return [];
    }
  }
}
```

### 2.2 方案二：应用层Logger工厂

在应用层创建logger工厂，根据环境自动选择合适的stream。

```typescript
// apps/web-app/src/logger/factory.ts
import { createLogger, createMultistream } from '@common-utils/logger';
import { 
  BrowserConsoleStream, 
  BrowserTransmitStream,
  BrowserLocalStorageStream 
} from './browser-streams';

/**
 * 创建浏览器环境的logger
 */
export function createBrowserLogger(options: {
  level?: string;
  enableConsole?: boolean;
  enableTransmit?: boolean;
  enableLocalStorage?: boolean;
  transmitEndpoint?: string;
}) {
  const streams: any[] = [];

  // Console输出
  if (options.enableConsole !== false) {
    streams.push({
      stream: new BrowserConsoleStream({
        json: process.env.NODE_ENV === 'production',
        pretty: process.env.NODE_ENV !== 'production'
      }),
      level: 'info'
    });
  }

  // 远程传输
  if (options.enableTransmit && options.transmitEndpoint) {
    streams.push({
      stream: new BrowserTransmitStream({
        endpoint: options.transmitEndpoint,
        batchSize: 20,
        flushInterval: 5000
      }),
      level: 'warn'
    });
  }

  // LocalStorage存储
  if (options.enableLocalStorage) {
    streams.push({
      stream: new BrowserLocalStorageStream({
        key: 'app-logs',
        maxSize: 500
      }),
      level: 'error'
    });
  }

  const multiStream = createMultistream(streams);

  return createLogger({
    level: options.level || 'info',
    stream: multiStream
  });
}
```

### 2.3 方案三：环境检测和条件导出

在应用层根据环境动态导入和创建logger。

```typescript
// apps/web-app/src/logger/index.ts
import { createLogger } from '@common-utils/logger';

// 环境检测
const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';

async function createAppLogger() {
  if (isBrowser) {
    // 浏览器环境
    const { createBrowserLogger } = await import('./factory');
    return createBrowserLogger({
      level: process.env.LOG_LEVEL || 'info',
      enableConsole: true,
      enableTransmit: process.env.NODE_ENV === 'production',
      enableLocalStorage: true,
      transmitEndpoint: process.env.LOG_TRANSMIT_ENDPOINT
    });
  } else {
    // Node.js环境
    const { createFileStream, createConsoleStream, createMultistream } = await import('@common-utils/logger');
    
    const multiStream = createMultistream([
      {
        stream: createConsoleStream({ pretty: true }),
        level: 'info'
      },
      {
        stream: createFileStream({ filePath: './logs/app.log' }),
        level: 'debug'
      }
    ]);
    
    return createLogger({
      level: process.env.LOG_LEVEL || 'info',
      stream: multiStream
    });
  }
}

export const logger = await createAppLogger();
```

## 三、方案对比

### 3.1 方案一：浏览器专用Stream

**优点**：
- 完全在应用层实现，不修改核心代码
- 灵活性高，可以针对浏览器特性优化
- 易于测试和维护

**缺点**：
- 需要手动创建和配置stream
- 代码重复（与Node.js stream类似）

**适用场景**：
- 需要精细控制浏览器日志行为
- 需要浏览器特定功能（如transmit、localStorage）

### 3.2 方案二：应用层Logger工厂

**优点**：
- 提供统一的创建接口
- 自动处理环境差异
- 配置集中管理

**缺点**：
- 需要维护工厂函数
- 灵活性相对较低

**适用场景**：
- 需要快速集成
- 配置相对固定

### 3.3 方案三：环境检测和条件导出

**优点**：
- 完全自动化，无需手动配置
- 代码简洁
- 易于扩展

**缺点**：
- 动态导入可能影响性能
- 调试相对复杂

**适用场景**：
- 需要同时支持浏览器和Node.js
- 希望自动化处理

## 四、推荐方案

### 4.1 混合方案（推荐）

结合方案一和方案二的优点：

1. **核心层**：保持现有设计，专注于Node.js环境
2. **应用层**：
   - 提供浏览器专用stream实现
   - 提供统一的logger工厂
   - 支持环境检测和自动配置

### 4.2 实施步骤

#### 阶段1：创建浏览器Stream
- 实现`BrowserConsoleStream`
- 实现`BrowserTransmitStream`
- 实现`BrowserLocalStorageStream`

#### 阶段2：创建Logger工厂
- 实现`createBrowserLogger`
- 实现`createNodeLogger`
- 实现环境检测

#### 阶段3：集成到应用
- 在web-app中集成浏览器logger
- 配置环境变量
- 测试和优化

## 五、技术细节

### 5.1 异步处理

浏览器环境使用`setTimeout`替代`setImmediate`：

```typescript
// Node.js
setImmediate(callback);

// 浏览器
setTimeout(callback, 0);
```

### 5.2 错误处理

浏览器环境的错误处理：

```typescript
// 使用window.onerror捕获全局错误
window.onerror = (message, source, lineno, colno, error) => {
  logger.error('Global error', { message, source, lineno, colno, error });
};

// 使用window.addEventListener('unhandledrejection')
window.addEventListener('unhandledrejection', (event) => {
  logger.error('Unhandled promise rejection', { reason: event.reason });
});
```

### 5.3 性能优化

- 使用`requestIdleCallback`在空闲时处理日志
- 使用`requestAnimationFrame`优化UI相关日志
- 使用`keepalive`确保页面关闭时也能发送日志

### 5.4 调试支持

- 支持浏览器DevTools的console分组
- 支持console.table展示结构化数据
- 支持console.trace显示调用栈

## 六、配置示例

### 6.1 开发环境

```typescript
const logger = createBrowserLogger({
  level: 'debug',
  enableConsole: true,
  enableTransmit: false,
  enableLocalStorage: false
});
```

### 6.2 生产环境

```typescript
const logger = createBrowserLogger({
  level: 'info',
  enableConsole: false,  // 生产环境关闭console
  enableTransmit: true,
  enableLocalStorage: true,
  transmitEndpoint: 'https://logs.example.com/api/logs'
});
```

### 6.3 测试环境

```typescript
const logger = createBrowserLogger({
  level: 'debug',
  enableConsole: true,
  enableTransmit: false,
  enableLocalStorage: true  // 保存日志用于测试分析
});
```

## 七、未来扩展

### 7.1 可能的增强功能

1. **日志可视化**：集成日志查看器
2. **日志分析**：提供日志分析工具
3. **性能监控**：集成性能指标
4. **用户反馈**：允许用户提交日志
5. **离线支持**：使用IndexedDB存储日志

### 7.2 与其他工具集成

- Sentry：错误追踪
- LogRocket：用户会话回放
- Google Analytics：用户行为分析
- 自定义日志平台

## 八、总结

通过在应用层实现浏览器兼容性，可以：

1. **保持核心简洁**：SDK层专注于Node.js环境
2. **灵活适配**：应用层可以根据需求定制
3. **易于维护**：浏览器相关代码集中在应用层
4. **渐进增强**：可以根据需要逐步添加功能

推荐采用混合方案，在应用层提供浏览器专用的stream实现和统一的logger工厂，实现环境自动检测和配置。