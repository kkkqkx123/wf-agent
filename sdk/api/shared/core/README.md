# Command模式核心模块

## 概述

本模块提供了Command模式的基础设施，包括Command接口、执行器和中间件系统。这是SDK API改造阶段1的核心交付物。

## 核心组件

### 1. Command接口

所有命令都需要实现 [`Command<T>`](command.ts:52) 接口：

```typescript
interface Command<T> {
  execute(): Promise<ExecutionResult<T>>;
  undo?(): Promise<ExecutionResult<void>>;
  validate(): CommandValidationResult;
  getMetadata(): CommandMetadata;
}
```

### 2. BaseCommand抽象类

[`BaseCommand<T>`](command.ts:82) 提供了通用的命令实现，包含执行时间跟踪等功能。

### 3. CommandExecutor

[`CommandExecutor`](command-executor.ts:26) 负责执行命令并管理中间件链：

```typescript
const executor = new CommandExecutor();
executor.addMiddleware(new LoggingMiddleware());
const result = await executor.execute(command);
```

### 4. 中间件系统

提供了5个内置中间件：

- [`LoggingMiddleware`](command-middleware.ts:38) - 日志记录
- [`ValidationMiddleware`](command-middleware.ts:80) - 参数验证
- [`CacheMiddleware`](command-middleware.ts:101) - 结果缓存
- [`MetricsMiddleware`](command-middleware.ts:149) - 指标收集
- [`RetryMiddleware`](command-middleware.ts:223) - 自动重试

## 快速开始

### 基本使用

```typescript
import { CommandExecutor, BaseCommand, CommandMetadata, validationSuccess } from "sdk/api";
import { success } from "sdk/api/types";

// 创建自定义命令
class MyCommand extends BaseCommand<string> {
  async execute(): Promise<ExecutionResult<string>> {
    return success("Hello, World!", this.getExecutionTime());
  }

  validate(): CommandValidationResult {
    return validationSuccess();
  }

  getMetadata(): CommandMetadata {
    return {
      name: "MyCommand",
      description: "My custom command",
      category: "execution",
      requiresAuth: false,
      version: "1.0.0",
    };
  }
}

// 执行命令
const executor = new CommandExecutor();
const result = await executor.execute(new MyCommand());
```

### 使用中间件

```typescript
import { CommandExecutor, LoggingMiddleware, ValidationMiddleware, CacheMiddleware } from "sdk/api";

const executor = new CommandExecutor();

// 添加中间件
executor.addMiddleware(new LoggingMiddleware());
executor.addMiddleware(new ValidationMiddleware());
executor.addMiddleware(new CacheMiddleware(5000)); // 5秒TTL

// 执行命令
const result = await executor.execute(command);
```

### 批量执行

```typescript
// 串行执行
const results = await executor.executeBatch(commands, false);

// 并行执行
const results = await executor.executeBatch(commands, true);
```

## 示例代码

详细的示例代码请参考：

- [`examples/example-commands.ts`](examples/example-commands.ts) - 5个Command实现示例
- [`examples/usage-examples.ts`](examples/usage-examples.ts) - 10个使用场景示例

## 类型定义

### ExecutionResult<T>

统一的执行结果类型：

```typescript
interface ExecutionResult<T> {
  result: Result<T, SDKError>;
  executionTime: number;
}
```

基于 packages/types 的 Result 类型，添加 executionTime 支持。

### ExecutionOptions

统一的执行选项：

```typescript
interface ExecutionOptions {
  timeout?: number;
  retries?: number;
  retryDelay?: number;
  cache?: boolean;
  logging?: boolean;
  validation?: boolean;
}
```

## 设计原则

1. **统一性**：所有命令都遵循相同的接口和执行模式
2. **可扩展性**：通过中间件系统轻松添加横切关注点
3. **可测试性**：Command可以独立测试
4. **类型安全**：完整的TypeScript类型支持
5. **灵活性**：支持同步和异步命令，支持撤销操作

## 下一步

阶段1已完成Command模式的基础设施建设。下一阶段（阶段2）将使用Command模式重构核心API：

- ThreadExecutorAPI
- LLMAPI
- ToolAPI
- ScriptAPI

## 相关文档

- [阶段1完成总结](../../../docs/sdk/api/phase-1-completion-summary.md)
- [API改造分阶段执行方案](../../../docs/sdk/api/sdk-api-implementation-phases.md)
- [API重新设计方案](../../../docs/sdk/api/sdk-api-redesign-specification.md)
