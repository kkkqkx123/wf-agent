# 日志系统配置指南

## 概述

本文档介绍如何配置 Modular Agent Framework 的日志系统，包括日志级别、输出目标和各种选项。

## 配置层次

日志配置分为三个层次：

1. **CLI-APP层**：控制整体日志行为
2. **SDK层**：SDK内部的日志配置
3. **模块层**：单个模块的日志配置

## CLI-APP配置

### 命令行选项

```bash
# 启用详细日志
modular-agent --verbose

# 启用调试日志
modular-agent --debug

# 指定日志文件
modular-agent --log-file /path/to/app.log
```

### 环境变量

```bash
# 禁用SDK日志
export DISABLE_SDK_LOGS=true

# 设置SDK日志级别
export SDK_LOG_LEVEL=debug  # debug | info | warn | error | off

# 禁用日志终端输出
export DISABLE_LOG_TERMINAL=true
```

### 配置文件

在 `modular-agent.config.json` 中：

```json
{
  "output": {
    "dir": "./logs",
    "logFilePattern": "cli-app-{date}.log",
    "enableSDKLogs": true,
    "sdkLogLevel": "info"
  }
}
```

## SDK配置

### 程序化配置

```typescript
import { configureSDKLogger } from "@wf-agent/sdk";
import { createFileStream, createConsoleStream } from "@wf-agent/common-utils";

// 配置SDK logger输出到文件
configureSDKLogger({
  level: "info",
  stream: createFileStream({
    filePath: "/var/log/myapp/sdk.log",
    append: true,
  }),
});

// 配置SDK logger输出到控制台（开发调试）
configureSDKLogger({
  level: "debug",
  stream: createConsoleStream({
    pretty: true,
    timestamp: true,
  }),
});
```

### 动态调整日志级别

```typescript
import { logger, setAllLoggersLevel } from "@wf-agent/sdk";

// 调整单个logger级别
logger.setLevel("debug");

// 调整所有logger级别
setAllLoggersLevel("warn");
```

## 日志级别

### 级别定义

| 级别 | 值 | 用途 |
|-----|---|------|
| debug | 0 | 详细调试信息 |
| info | 1 | 一般信息 |
| warn | 2 | 警告信息 |
| error | 3 | 错误信息 |
| off | 4 | 关闭日志 |

### 级别继承

```
Global Level: info
  └── SDK Level: debug (覆盖全局)
       └── Module Level: warn (覆盖SDK)
```

## 输出流配置

### FileStream

```typescript
import { createFileStream } from "@wf-agent/common-utils";

const fileStream = createFileStream({
  filePath: "/path/to/log/file.log",
  append: true,        // 追加模式
  json: true,          // JSON格式输出
  timestamp: true,     // 包含时间戳
});
```

### ConsoleStream

```typescript
import { createConsoleStream } from "@wf-agent/common-utils";

const consoleStream = createConsoleStream({
  pretty: true,        // 美化输出
  json: false,         // 纯文本格式
  timestamp: true,     // 包含时间戳
});
```

### Multistream

同时输出到多个目标：

```typescript
import { createMultistream, createFileStream, createConsoleStream } from "@wf-agent/common-utils";

const multiStream = createMultistream([
  { stream: createFileStream({ filePath: "app.log" }), level: "info" },
  { stream: createConsoleStream({ pretty: true }), level: "warn" },
]);
```

## 典型配置场景

### 场景1：生产环境

```typescript
// 只记录warn及以上级别到文件
configureSDKLogger({
  level: "warn",
  stream: createFileStream({
    filePath: "/var/log/app/production.log",
    append: true,
    json: true,
  }),
});
```

### 场景2：开发环境

```typescript
// 记录所有级别到控制台
configureSDKLogger({
  level: "debug",
  stream: createConsoleStream({
    pretty: true,
    timestamp: true,
  }),
});
```

### 场景3：测试环境

```typescript
// 禁用日志或只记录error
configureSDKLogger({
  level: "error",
  stream: createFileStream({
    filePath: "./test.log",
    append: false,  // 每次测试重新开始
  }),
});
```

### 场景4：调试特定模块

```typescript
import { createModuleLogger } from "@wf-agent/sdk";

// 创建模块级logger并设置debug级别
const moduleLogger = createModuleLogger("my-module");
moduleLogger.setLevel("debug");
```

## 日志文件管理

### 日志轮转

虽然当前实现不包含自动轮转，但可以通过配置实现：

```typescript
import { createFileStream } from "@wf-agent/common-utils";
import * as path from "path";

// 按日期创建日志文件
const date = new Date().toISOString().split("T")[0];
const logFile = path.join("logs", `app-${date}.log`);

const stream = createFileStream({
  filePath: logFile,
  append: true,
});
```

### 日志清理

```typescript
// 定期清理旧日志文件
import * as fs from "fs";
import * as path from "path";

function cleanOldLogs(logDir: string, maxAgeDays: number): void {
  const files = fs.readdirSync(logDir);
  const now = Date.now();
  
  files.forEach(file => {
    const filePath = path.join(logDir, file);
    const stats = fs.statSync(filePath);
    const ageDays = (now - stats.mtime.getTime()) / (1000 * 60 * 60 * 24);
    
    if (ageDays > maxAgeDays) {
      fs.unlinkSync(filePath);
    }
  });
}
```

## 故障排查

### 日志不输出

检查清单：

1. 日志级别是否设置正确？
2. stream是否正确创建？
3. 文件路径是否有写权限？
4. 磁盘空间是否充足？

### 日志重复输出

可能原因：

1. 多次调用 `configureSDKLogger`
2. 使用了Multistream但配置不当
3. 同时注册了多个logger

### 性能问题

优化建议：

1. 生产环境使用 `json: true` 而非 `pretty: true`
2. 适当调整日志级别（避免debug级别）
3. 使用异步写入（FileStream默认）

## 参考

- [架构设计](./architecture.md)
- [迁移指南](./migration-guide.md)
