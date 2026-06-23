# 日志系统架构设计

## 概述

本文档描述 Modular Agent Framework 的日志系统架构，包括其设计目标、核心组件、数据流以及使用方式。

## 设计目标

1. **输出分离**：用户可见的输出（stdout/stderr）与调试日志（log file）完全分离
2. **延迟初始化**：SDK日志在CLI-APP配置后才初始化，避免污染控制台
3. **统一接口**：所有模块使用一致的日志接口
4. **灵活配置**：支持运行时动态调整日志级别和输出目标

## 架构概览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CLI-APP Layer                                  │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐ │
│  │   CLIOutput     │  │   Logger Init   │  │   File Stream Creation      │ │
│  │   (stdout)      │  │   (configure)   │  │   (createFileStream)        │ │
│  └────────┬────────┘  └────────┬────────┘  └─────────────┬───────────────┘ │
│           │                    │                         │                 │
│           └────────────────────┴─────────────────────────┘                 │
│                                │                                           │
│                                ▼                                           │
│                     ┌─────────────────────┐                                │
│                     │   configureSDKLogger  │                              │
│                     │   (level + stream)    │                              │
│                     └──────────┬──────────┘                                │
└────────────────────────────────┼──────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                               SDK Layer                                     │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │                         Logger Proxy                                  │ │
│  │  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐   │ │
│  │  │   Lazy Init     │───►│  BaseLogger     │───►│  FileStream     │   │ │
│  │  │   (getLogger)   │    │  (setStream)    │    │  (log file)     │   │ │
│  │  └─────────────────┘    └─────────────────┘    └─────────────────┘   │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

## 核心组件

### 1. Common Utils Logger (packages/common-utils/src/logger/)

基础日志实现，提供：

- **Logger接口**：统一的日志操作接口
- **BaseLogger**：核心日志实现，支持stream输出
- **LogStream**：抽象流接口，支持多种输出目标
- **FileStream**：文件输出流实现
- **ConsoleStream**：控制台输出流实现

关键类型定义：
```typescript
interface Logger {
  debug(message: string, context?: Record<string, any>): void;
  info(message: string, context?: Record<string, any>): void;
  warn(message: string, context?: Record<string, any>): void;
  error(message: string, context?: Record<string, any>): void;
  setLevel(level: LogLevel): void;
  setStream?(stream: LogStream): void;  // 运行时stream替换
}
```

### 2. SDK Logger (sdk/utils/logger.ts)

SDK层的日志封装，特点：

- **延迟初始化**：使用Proxy模式，首次访问时才创建实例
- **配置外部化**：通过 `configureSDKLogger` 由CLI-APP配置
- **Stream替换**：支持运行时替换输出流

```typescript
// 延迟初始化实现
let loggerInstance: Logger | null = null;

export const logger: Logger = new Proxy({} as Logger, {
  get(_, prop: string | symbol) {
    const instance = getLoggerInstance();
    const value = (instance as any)[prop];
    return typeof value === "function" ? value.bind(instance) : value;
  },
});

// 配置函数（由CLI-APP调用）
export function configureSDKLogger(config: {
  level: LogLevel;
  stream?: LogStream;
}): void {
  // 更新pendingConfig或已存在的logger
}
```

### 3. CLI Logger (apps/cli-app/src/utils/logger.ts)

CLI-APP的日志初始化：

```typescript
export function initSDKLogger(options: LoggerOptions = {}): void {
  const output = getOutput();
  
  // 创建文件流（仅输出到日志文件）
  const fileStream = createFileStream({
    filePath: output.logFile,
    append: true,
  });

  // 配置SDK logger使用文件流
  configureSDKLogger({
    level,
    stream: fileStream,
  });
}
```

### 4. CLI Output (apps/cli-app/src/utils/output.ts)

统一输出管理：

- **stdout**：用户可见输出
- **stderr**：错误信息输出
- **log file**：调试日志输出

## 数据流

### 初始化流程

```
CLI启动
  │
  ▼
initializeOutput() ──► 创建log文件路径
  │
  ▼
initLogger() ──► 创建CLI logger（文件流）
  │
  ▼
initSDKLogger() ──► 调用configureSDKLogger()
  │                    │
  │                    ▼
  │              设置pendingConfig
  │              （如果logger已存在则更新）
  │
  ▼
SDK首次使用logger
  │
  ▼
Proxy.get() ──► getLoggerInstance()
  │
  ▼
检查pendingConfig ──► 创建BaseLogger（使用FileStream）
```

### 日志输出流程

```
SDK代码调用 logger.info("message", context)
  │
  ▼
Proxy拦截 ──► 转发到BaseLogger实例
  │
  ▼
BaseLogger.info()
  │
  ▼
检查level ──► 创建LogEntry
  │
  ▼
stream.write(entry) ──► FileStream.write()
  │
  ▼
写入日志文件（不经过控制台）
```

## 关键设计决策

### 1. 为什么使用Proxy模式？

**问题**：SDK模块在import时就需要logger，但CLI-APP需要在运行时配置logger（确定日志文件路径等）。

**解决方案**：
- 使用Proxy拦截所有logger访问
- 首次访问时才创建实际logger实例
- 允许CLI-APP在SDK使用logger前配置它

### 2. 为什么需要setStream？

**问题**：某些场景下logger可能已经被创建，但需要改变输出目标。

**解决方案**：
- 添加 `setStream` 方法到Logger接口
- 允许运行时替换输出流
- 自动flush旧stream的数据

### 3. 如何确保SDK日志不污染CLI输出？

**措施**：
1. SDK代码中禁止使用 `console.*`，统一使用logger
2. CLI-APP配置SDK logger使用FileStream而非ConsoleStream
3. FileStream直接写入文件，不经过stdout/stderr

## 使用指南

### SDK开发

```typescript
// 正确：使用logger
import { logger } from "../../utils/logger.js";
logger.info("Operation completed", { operationId });

// 错误：直接使用console
console.log("Operation completed");  // 不要这样做！
```

### CLI-APP配置

```typescript
import { configureSDKLogger } from "@wf-agent/sdk";
import { createFileStream } from "@wf-agent/common-utils";

// 创建文件流
const fileStream = createFileStream({
  filePath: "/path/to/log/file.log",
  append: true,
});

// 配置SDK logger
configureSDKLogger({
  level: "info",
  stream: fileStream,
});
```

### 创建模块级Logger

```typescript
import { createModuleLogger } from "@wf-agent/sdk";

const logger = createModuleLogger("my-module");
logger.info("Module initialized");
```

## 文件结构

```
packages/common-utils/src/logger/
├── logger.ts           # BaseLogger实现
├── types.ts            # 类型定义
├── streams/
│   ├── file-stream.ts  # 文件输出流
│   ├── console-stream.ts # 控制台输出流
│   └── index.ts
└── index.ts

sdk/utils/
├── logger.ts           # SDK logger（延迟初始化）
├── contextual-logger.ts # 上下文日志
└── index.ts

apps/cli-app/src/utils/
├── logger.ts           # CLI logger初始化
└── output.ts           # 统一输出管理
```

## 总结

当前日志系统通过以下机制实现了"SDK日志只输出到文件，不污染CLI输出"的目标：

1. **架构层面**：分离用户输出流（stdout）和日志流（file）
2. **实现层面**：Proxy延迟初始化 + FileStream输出
3. **规范层面**：禁止直接使用console，统一使用logger接口
4. **配置层面**：CLI-APP完全控制SDK logger的配置
