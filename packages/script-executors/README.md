# @modular-agent/script-executors

脚本执行器包，提供统一的脚本执行接口和多种脚本类型的执行器实现。

## 功能特性

- **统一的执行器接口**: `IScriptExecutor` 接口定义了所有执行器必须实现的契约
- **抽象基类**:
  - `BaseScriptExecutor`: 提供通用的执行逻辑（重试、超时）
  - `CommandLineExecutor`: 提供命令行执行的统一实现，封装 spawn 调用
- **验证职责分离**: 脚本验证由 SDK 在配置加载时完成，执行器专注于执行
- **内置执行器**: 提供开箱即用的脚本执行器实现
  - `ShellExecutor`: Shell 脚本执行器（使用 `sh -c`）
  - `PythonExecutor`: Python 脚本执行器（使用 `python3 -c`）
  - `JavaScriptExecutor`: JavaScript 脚本执行器（使用 `node -e`）
  - `PowerShellExecutor`: PowerShell 脚本执行器（使用 `pwsh -Command`）
  - `CmdExecutor`: Windows CMD 批处理执行器（使用 `cmd.exe /c`）
- **通用组件**:
  - `RetryStrategy`: 重试策略
  - `TimeoutController`: 超时控制器

## 命名说明

本包使用 "Script" 而非 "Code" 来命名相关概念，因为：

- **更准确**: 实际执行的是脚本，而不是通用的"代码"
- **一致性**: 与 SDK 中的 `ScriptService`、`ScriptNode` 等命名保持一致
- **清晰性**: 避免使用模糊的 "Code" 词汇

相关命名映射：

- `ScriptService` - 脚本服务（原 `CodeService`）
- `ScriptExecutionError` - 脚本执行错误（原 `CodeExecutionError`）
- `ScriptHandler` - 脚本处理器（原 `CodeHandler`）
- `SCRIPT_NODE` - 脚本节点（原 `CODE_NODE`）
- `ScriptNodeConfig` - 脚本节点配置（原 `CodeNodeConfig`）

## 安装

```bash
pnpm add @modular-agent/script-executors
```

## 使用示例

### 基本使用

```typescript
import { ShellExecutor, PythonExecutor, JavaScriptExecutor } from "@modular-agent/script-executors";
import type { Script } from "@modular-agent/types";

// 创建 Shell 执行器
const shellExecutor = new ShellExecutor();

// 定义脚本
const script: Script = {
  id: "test-script",
  name: "test-script",
  type: "SHELL",
  description: "Test script",
  content: 'echo "Hello, World!"',
  options: {
    timeout: 5000,
    retries: 3,
    retryDelay: 1000,
  },
};

// 执行脚本
const result = await shellExecutor.execute(script);

if (result.success) {
  console.log("Output:", result.stdout);
} else {
  console.error("Error:", result.error);
}
```

### 使用 Python 执行器

```typescript
import { PythonExecutor } from "@modular-agent/script-executors";

const pythonExecutor = new PythonExecutor();

const pythonScript: Script = {
  id: "python-script",
  name: "python-script",
  type: "PYTHON",
  description: "Python script",
  content: 'print("Hello from Python!")',
  options: {
    timeout: 5000,
  },
};

const result = await pythonExecutor.execute(pythonScript);
```

### 使用 JavaScript 执行器

```typescript
import { JavaScriptExecutor } from "@modular-agent/script-executors";

const jsExecutor = new JavaScriptExecutor();

const jsScript: Script = {
  id: "js-script",
  name: "js-script",
  type: "JAVASCRIPT",
  description: "JavaScript script",
  content: 'console.log("Hello from JavaScript!");',
  options: {
    timeout: 5000,
  },
};

const result = await jsExecutor.execute(jsScript);
```

**注意**: JavaScript 执行器使用 `node -e` 执行脚本，而非 vm 模块。这种方式更简单且与系统环境一致。

### 自定义执行器配置

```typescript
import { ShellExecutor } from "@modular-agent/script-executors";

const executor = new ShellExecutor({
  type: "SHELL",
  maxRetries: 5,
  retryDelay: 2000,
  exponentialBackoff: true,
  timeout: 10000,
  resourceLimits: {
    memory: 256,
    cpu: 2,
  },
});
```

### 验证脚本

```typescript
const validationResult = executor.validate(script);

if (!validationResult.valid) {
  console.error("Validation errors:", validationResult.errors);
}
```

**注意**: `validate()` 方法已废弃，脚本验证由 SDK 在配置加载时完成。此方法仅返回成功以保持接口兼容性。

### 获取支持的脚本类型

```typescript
const supportedTypes = executor.getSupportedTypes();
console.log("Supported types:", supportedTypes);
```

## 架构设计

### 继承层次

```
IScriptExecutor (接口)
    ↓
BaseScriptExecutor (抽象基类)
    ├── 重试、超时控制
    └── 结果标准化
    ↓
CommandLineExecutor<T> (抽象基类)
    ├── 命令行执行逻辑
    ├── 环境变量管理
    ├── 工作目录管理
    ├── 输出收集
    └── 泛型类型支持
    ↓
具体执行器
    ├── ShellExecutor
    ├── CmdExecutor
    ├── PowerShellExecutor
    ├── PythonExecutor
    └── JavaScriptExecutor
```

### 核心接口

```typescript
interface IScriptExecutor {
  execute(
    script: Script,
    options?: ScriptExecutionOptions,
    context?: ExecutionContext,
  ): Promise<ScriptExecutionResult>;

  validate(script: Script): ValidationResult;

  getSupportedTypes(): ScriptType[];

  cleanup?(): Promise<void>;

  getExecutorType(): string;
}
```

### 抽象基类

#### BaseScriptExecutor

提供通用的执行逻辑：

- **重试机制**: 使用 `RetryStrategy` 管理重试逻辑
- **超时控制**: 使用 `TimeoutController` 控制执行超时
- **结果标准化**: 统一的执行结果格式

**注意**: 脚本验证由 SDK 在配置加载时完成，执行器不再重复验证。`validate()` 方法已废弃，仅返回成功以保持接口兼容性。

#### CommandLineExecutor<T>

提供命令行执行的统一实现：

- **命令行执行**: 封装 `spawn` 调用
- **环境变量管理**: 合并进程、脚本、上下文的环境变量
- **工作目录管理**: 支持自定义工作目录
- **输出收集**: 统一收集 stdout 和 stderr
- **泛型类型支持**: 通过泛型参数提供类型安全的执行器类型
- **自动类型推断**: 自动实现 `getSupportedTypes()` 和 `getExecutorType()`

### 执行流程

1. 准备执行环境（环境变量、工作目录）
2. 执行命令（带重试和超时）
3. 收集输出
4. 标准化结果
5. 清理资源

**注意**: 脚本验证在配置加载时由 SDK 完成，不在执行流程中。

### 设计优势

1. **代码复用**: 所有命令行执行器共享 90%+ 的代码
2. **类型安全**: 泛型设计确保类型正确性
3. **易于扩展**: 新增执行器只需实现 `getCommandLineConfig()` 方法
4. **统一行为**: 所有执行器具有一致的错误处理、重试、超时行为
5. **职责分离**: 验证由 SDK 负责，执行器专注于执行，避免重复验证

## 与 ScriptService 集成

```typescript
import { ScriptService } from "@modular-agent/sdk";
import { ShellExecutor, PythonExecutor, JavaScriptExecutor } from "@modular-agent/script-executors";

const scriptService = new ScriptService();

// 注册执行器
scriptService.registerExecutor("SHELL", new ShellExecutor());
scriptService.registerExecutor("PYTHON", new PythonExecutor());
scriptService.registerExecutor("JAVASCRIPT", new JavaScriptExecutor());

// 注册脚本
scriptService.registerScript({
  id: "my-script",
  name: "my-script",
  type: "SHELL",
  description: "My script",
  content: 'echo "Hello!"',
  options: {
    timeout: 5000,
  },
});

// 执行脚本
const result = await scriptService.execute("my-script");
```

## 测试

```bash
# 运行测试
pnpm test

# 运行测试并生成覆盖率报告
pnpm test:coverage

# 监听模式
pnpm test:watch
```

## 开发

```bash
# 类型检查
pnpm typecheck

# 构建
pnpm build
```

## 许可证

MIT
