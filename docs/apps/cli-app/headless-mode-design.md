# CLI App 无头模式设计方案

## 概述

本文档提出增强 CLI App 无头模式的详细设计方案，解决当前架构的不足，支持更广泛的自动化场景和跨应用复用。

## 设计目标

### 主要目标

1. **稳定的无头执行**：解决测试超时和输出捕获问题
2. **清晰的模式分离**：解耦 TEST_MODE 与 HEADLESS 模式
3. **灵活的配置选项**：支持精细控制无头模式行为
4. **跨应用复用**：为其他 app 提供程序化调用能力

### 使用场景

| 场景 | 需求 | 当前支持 | 目标 |
|------|------|----------|------|
| 自动化测试 | 稳定退出、输出捕获 | ⚠️ 部分 | ✅ 完整 |
| CI/CD 集成 | JSON 输出、非交互 | ⚠️ 部分 | ✅ 完整 |
| 其他 App 调用 | 程序化 API | ❌ 不支持 | ✅ 支持 |
| 批处理任务 | 后台执行、日志重定向 | ✅ 支持 | ✅ 保持 |

## 架构设计

### 三层执行模式架构

```
┌─────────────────────────────────────────────────────────────────┐
│                     Execution Mode Manager                      │
│                     (执行模式管理器)                              │
├─────────────────┬─────────────────┬─────────────────────────────┤
│    交互模式      │    无头模式      │        程序化模式            │
│  Interactive    │    Headless     │     Programmatic            │
├─────────────────┼─────────────────┼─────────────────────────────┤
│ • 彩色输出       │ • JSON 输出      │ • 直接函数调用               │
│ • 用户确认       │ • 纯文本输出     │ • 返回结构化数据              │
│ • 进度显示       │ • 无交互        │ • 无进程开销                 │
│ • 终端交互       │ • 自动退出       │ • 可嵌入其他应用              │
│ • 表格/列表      │ • 结构化日志     │ • 类型安全                   │
└─────────────────┴─────────────────┴─────────────────────────────┘
```

### 模式对比

| 特性 | 交互模式 | 无头模式 | 程序化模式 |
|------|----------|----------|------------|
| 目标用户 | 终端用户 | 自动化脚本 | 其他应用 |
| 输出目标 | stdout | stdout/file | 返回值 |
| 输出格式 | 人类可读 | JSON/文本 | 数据结构 |
| 错误处理 | 显示错误信息 | 退出码+错误信息 | 异常抛出 |
| 日志记录 | 文件 | 文件 | 可选 |
| 交互能力 | 完整 | 无 | 无 |

## 详细设计

### 1. 执行模式配置接口

```typescript
// src/types/execution-mode.ts

/**
 * 执行模式类型
 */
export type ExecutionMode = 'interactive' | 'headless' | 'programmatic';

/**
 * 输出格式类型
 */
export type OutputFormat = 'text' | 'json' | 'silent';

/**
 * 执行模式配置
 */
export interface ExecutionModeConfig {
  /** 执行模式 */
  mode: ExecutionMode;
  
  /** 输出格式 */
  outputFormat?: OutputFormat;
  
  /** 是否自动退出（无头模式） */
  autoExit?: boolean;
  
  /** 超时时间（毫秒） */
  timeout?: number;
  
  /** 日志级别 */
  logLevel?: 'debug' | 'verbose' | 'info' | 'warn' | 'error';
  
  /** 日志文件路径 */
  logFile?: string;
  
  /** 是否禁用彩色输出 */
  noColor?: boolean;
  
  /** 退出码映射 */
  exitCodes?: ExitCodeConfig;
}

/**
 * 退出码配置
 */
export interface ExitCodeConfig {
  success: number;
  error: number;
  validationError: number;
  timeout: number;
  cancelled: number;
}

/**
 * 默认配置
 */
export const defaultExecutionConfig: ExecutionModeConfig = {
  mode: 'interactive',
  outputFormat: 'text',
  autoExit: false,
  timeout: 30000,
  logLevel: 'info',
  noColor: false,
  exitCodes: {
    success: 0,
    error: 1,
    validationError: 2,
    timeout: 124,
    cancelled: 130,
  },
};
```

### 2. 执行模式管理器

```typescript
// src/execution/execution-mode-manager.ts

import { 
  ExecutionMode, 
  ExecutionModeConfig, 
  defaultExecutionConfig 
} from '../types/execution-mode.js';

/**
 * 执行模式管理器
 * 负责检测和管理当前执行模式
 */
export class ExecutionModeManager {
  private static instance: ExecutionModeManager;
  private config: ExecutionModeConfig;

  private constructor() {
    this.config = this.detectModeFromEnvironment();
  }

  static getInstance(): ExecutionModeManager {
    if (!ExecutionModeManager.instance) {
      ExecutionModeManager.instance = new ExecutionModeManager();
    }
    return ExecutionModeManager.instance;
  }

  /**
   * 从环境变量检测执行模式
   */
  private detectModeFromEnvironment(): ExecutionModeConfig {
    const mode = this.detectMode();
    
    return {
      ...defaultExecutionConfig,
      mode,
      outputFormat: this.detectOutputFormat(mode),
      autoExit: mode !== 'interactive',
      noColor: mode !== 'interactive' || process.env['NO_COLOR'] === 'true',
      logLevel: this.detectLogLevel(),
      logFile: process.env['CLI_LOG_FILE'],
    };
  }

  /**
   * 检测执行模式
   */
  private detectMode(): ExecutionMode {
    // 程序化模式优先级最高
    if (process.env['CLI_MODE'] === 'programmatic') {
      return 'programmatic';
    }
    
    // 无头模式
    if (process.env['CLI_MODE'] === 'headless' || 
        process.env['HEADLESS'] === 'true') {
      return 'headless';
    }
    
    // 测试模式（向后兼容）
    if (process.env['TEST_MODE'] === 'true') {
      return 'headless';
    }
    
    // 默认交互模式
    return 'interactive';
  }

  /**
   * 检测输出格式
   */
  private detectOutputFormat(mode: ExecutionMode): OutputFormat {
    const format = process.env['CLI_OUTPUT_FORMAT'];
    if (format === 'json' || format === 'text' || format === 'silent') {
      return format;
    }
    return mode === 'headless' ? 'json' : 'text';
  }

  /**
   * 检测日志级别
   */
  private detectLogLevel(): string {
    const level = process.env['CLI_LOG_LEVEL'];
    if (level) return level;
    
    if (process.env['DEBUG'] === 'true') return 'debug';
    if (process.env['VERBOSE'] === 'true') return 'verbose';
    
    return 'info';
  }

  /**
   * 获取当前配置
   */
  getConfig(): ExecutionModeConfig {
    return { ...this.config };
  }

  /**
   * 更新配置
   */
  updateConfig(updates: Partial<ExecutionModeConfig>): void {
    this.config = { ...this.config, ...updates };
  }

  /**
   * 当前模式检查
   */
  isInteractive(): boolean {
    return this.config.mode === 'interactive';
  }

  isHeadless(): boolean {
    return this.config.mode === 'headless';
  }

  isProgrammatic(): boolean {
    return this.config.mode === 'programmatic';
  }

  /**
   * 是否使用 JSON 输出
   */
  isJsonOutput(): boolean {
    return this.config.outputFormat === 'json';
  }
}

/**
 * 便捷函数
 */
export function getExecutionModeManager(): ExecutionModeManager {
  return ExecutionModeManager.getInstance();
}

export function isHeadlessMode(): boolean {
  return ExecutionModeManager.getInstance().isHeadless();
}

export function isProgrammaticMode(): boolean {
  return ExecutionModeManager.getInstance().isProgrammatic();
}
```

### 3. 增强的输出管理器

```typescript
// src/utils/output.ts 增强

import { ExecutionModeManager } from '../execution/execution-mode-manager.js';

/**
 * 根据执行模式调整输出行为
 */
export class CLIOutput {
  // ... 现有代码 ...

  /**
   * 获取执行模式管理器
   */
  private get modeManager(): ExecutionModeManager {
    return ExecutionModeManager.getInstance();
  }

  /**
   * 输出内容（根据模式自动调整）
   */
  output(content: string): void {
    if (this.modeManager.isJsonOutput()) {
      // JSON 模式下不直接输出文本
      return;
    }
    this._stdout.write(content + "\n");
  }

  /**
   * 输出结构化数据（JSON 模式）
   */
  structuredOutput(data: unknown): void {
    if (this.modeManager.isJsonOutput()) {
      this._stdout.write(JSON.stringify(data) + "\n");
    }
  }

  /**
   * 输出结果（自动根据模式选择格式）
   */
  result(data: unknown, options?: { 
    message?: string; 
    success?: boolean 
  }): void {
    const { message, success = true } = options || {};

    if (this.modeManager.isJsonOutput()) {
      this.structuredOutput({
        success,
        data,
        message,
        timestamp: new Date().toISOString(),
      });
    } else {
      if (success) {
        this.success(message || 'Operation completed');
      } else {
        this.fail(message || 'Operation failed');
      }
      if (data) {
        this.json(data);
      }
    }
  }

  /**
   * 输出错误（自动根据模式选择格式）
   */
  errorResult(error: Error | string, code?: string): void {
    const errorMessage = error instanceof Error ? error.message : error;

    if (this.modeManager.isJsonOutput()) {
      this.structuredOutput({
        success: false,
        error: {
          message: errorMessage,
          code,
          timestamp: new Date().toISOString(),
        },
      });
    } else {
      this.fail(errorMessage);
    }
  }

  /**
   * 确保所有输出完成（用于无头模式安全退出）
   */
  async ensureDrained(): Promise<void> {
    const drains: Promise<void>[] = [];

    // 等待 stdout
    if (this._stdout.writable) {
      drains.push(new Promise((resolve) => {
        if (this._stdout.writableNeedDrain) {
          this._stdout.once('drain', resolve);
        } else {
          resolve();
        }
      }));
    }

    // 等待 stderr
    if (this._stderr.writable) {
      drains.push(new Promise((resolve) => {
        if (this._stderr.writableNeedDrain) {
          this._stderr.once('drain', resolve);
        } else {
          resolve();
        }
      }));
    }

    // 等待日志流
    if (this._logStream && this._logStream.writable) {
      drains.push(new Promise((resolve) => {
        this._logStream!.once('finish', resolve);
        this._logStream!.end();
      }));
    }

    await Promise.all(drains);
  }
}
```

### 4. 安全退出机制

```typescript
// src/utils/exit-manager.ts

import { getOutput } from './output.js';
import { getExecutionModeManager } from '../execution/execution-mode-manager.js';

/**
 * 退出管理器
 * 确保无头模式下安全退出
 */
export class ExitManager {
  private static isShuttingDown = false;

  /**
   * 安全退出
   */
  static async exit(code: number = 0): Promise<never> {
    if (ExitManager.isShuttingDown) {
      return process.exit(code);
    }

    ExitManager.isShuttingDown = true;
    const output = getOutput();

    try {
      // 等待所有输出完成
      await output.ensureDrained();
      
      // 等待事件循环清空
      await ExitManager.waitForEventLoopDrain();
      
      process.exit(code);
    } catch (error) {
      // 紧急退出
      process.exit(code || 1);
    }
  }

  /**
   * 等待事件循环清空
   */
  private static async waitForEventLoopDrain(): Promise<void> {
    return new Promise((resolve) => {
      // 使用 setImmediate 让出执行权
      setImmediate(() => {
        // 检查是否还有未完成的异步操作
        const checkInterval = setInterval(() => {
          // 简化检查：等待一个事件循环周期
          setImmediate(() => {
            clearInterval(checkInterval);
            resolve();
          });
        }, 10);
        
        // 超时保护
        setTimeout(() => {
          clearInterval(checkInterval);
          resolve();
        }, 1000);
      });
    });
  }

  /**
   * 设置退出处理器
   */
  static setupExitHandlers(): void {
    const modeManager = getExecutionModeManager();

    // 信号处理
    process.on('SIGINT', async () => {
      await ExitManager.exit(130);
    });

    process.on('SIGTERM', async () => {
      await ExitManager.exit(143);
    });

    // 未捕获异常
    process.on('uncaughtException', async (error) => {
      const output = getOutput();
      output.errorLog(`Uncaught exception: ${error.message}`);
      
      if (modeManager.isHeadless()) {
        output.errorResult(error, 'UNCAUGHT_EXCEPTION');
      }
      
      await ExitManager.exit(1);
    });

    // 未处理的 Promise 拒绝
    process.on('unhandledRejection', async (reason) => {
      const output = getOutput();
      output.errorLog(`Unhandled rejection: ${String(reason)}`);
      
      if (modeManager.isHeadless()) {
        output.errorResult(
          reason instanceof Error ? reason : new Error(String(reason)),
          'UNHANDLED_REJECTION'
        );
      }
      
      await ExitManager.exit(1);
    });
  }
}
```

### 5. 入口文件改造

```typescript
// src/index.ts 改造

import { Command } from "commander";
import { initializeOutput, getOutput } from "./utils/output.js";
import { initializeFormatter } from "./utils/formatter.js";
import { initLogger, initSDKLogger } from "./utils/logger.js";
import { loadConfig } from "./config/config-loader.js";
import { getSDK } from "@wf-agent/sdk";
import { ExitManager } from "./utils/exit-manager.js";
import { 
  getExecutionModeManager,
  ExecutionModeManager 
} from "./execution/execution-mode-manager.js";

const program = new Command();

program
  .name("modular-agent")
  .description("Modular Agent Framework CLI")
  .version("1.0.0")
  .option("-v, --verbose", "Enable verbose output")
  .option("-d, --debug", "Enable debug mode")
  .option("-l, --log-file <path>", "Specify log file path")
  .option("--no-color", "Disable colored output")
  .option("--json", "Output in JSON format (headless mode)")
  .hook("preAction", async thisCommand => {
    const options = thisCommand.opts();
    
    // 1. 检测执行模式
    const modeManager = getExecutionModeManager();
    
    // 命令行选项覆盖环境变量
    if (options.json) {
      modeManager.updateConfig({ 
        mode: 'headless', 
        outputFormat: 'json',
        autoExit: true 
      });
    }

    // 2. 初始化输出系统
    const output = initializeOutput({
      logFile: options.logFile,
      verbose: options.verbose,
      debug: options.debug,
      color: !options.noColor && modeManager.isInteractive(),
    });

    // 3. 初始化格式化器
    initializeFormatter(output.colorEnabled);

    // 4. 初始化日志
    initLogger({
      verbose: options.verbose,
      debug: options.debug,
      logFile: options.logFile,
    });

    initSDKLogger({
      verbose: options.verbose,
      debug: options.debug,
      logFile: options.logFile,
    });

    // 5. 加载配置并初始化 SDK
    const config = await loadConfig();
    getSDK({
      debug: options.debug,
      logLevel: options.debug ? "debug" : options.verbose ? "info" : "warn",
      presets: config.presets,
    });

    // 6. 设置退出处理器
    ExitManager.setupExitHandlers();
  });

// ... 命令注册 ...

// 后置钩子：处理无头模式退出
program.hook("postAction", async () => {
  const modeManager = getExecutionModeManager();
  
  if (modeManager.getConfig().autoExit) {
    await ExitManager.exit(0);
  }
});

program.parse(process.argv);

// 无命令时显示帮助
if (!process.argv.slice(2).length) {
  program.outputHelp();
  
  const modeManager = getExecutionModeManager();
  if (modeManager.getConfig().autoExit) {
    setTimeout(() => ExitManager.exit(0), 100);
  }
}
```

### 6. 程序化 API 层

```typescript
// src/api/programmatic-api.ts

import { ExecutionModeManager } from '../execution/execution-mode-manager.js';
import { WorkflowAdapter } from '../adapters/workflow-adapter.js';
import { ThreadAdapter } from '../adapters/thread-adapter.js';
// ... 其他适配器

/**
 * 程序化 API 选项
 */
export interface ProgrammaticAPIOptions {
  /** 日志级别 */
  logLevel?: 'debug' | 'verbose' | 'info' | 'warn' | 'error';
  /** 是否启用日志 */
  enableLogging?: boolean;
  /** 配置文件路径 */
  configPath?: string;
}

/**
 * CLI App 程序化 API
 * 允许其他应用直接调用 CLI 功能
 */
export class ProgrammaticAPI {
  private options: ProgrammaticAPIOptions;

  constructor(options: ProgrammaticAPIOptions = {}) {
    this.options = {
      logLevel: 'error',
      enableLogging: false,
      ...options,
    };
    
    // 设置为程序化模式
    process.env['CLI_MODE'] = 'programmatic';
    ExecutionModeManager.getInstance().updateConfig({
      mode: 'programmatic',
      outputFormat: 'silent',
    });
  }

  /**
   * 初始化 API
   */
  async initialize(): Promise<void> {
    // 初始化必要组件
    const { initializeOutput } = await import('../utils/output.js');
    const { initLogger, initSDKLogger } = await import('../utils/logger.js');
    const { loadConfig } = await import('../config/config-loader.js');
    const { getSDK } = await import('@wf-agent/sdk');

    initializeOutput({
      verbose: this.options.logLevel === 'verbose',
      debug: this.options.logLevel === 'debug',
    });

    if (this.options.enableLogging) {
      initLogger({
        verbose: this.options.logLevel === 'verbose',
        debug: this.options.logLevel === 'debug',
      });
      initSDKLogger({
        verbose: this.options.logLevel === 'verbose',
        debug: this.options.logLevel === 'debug',
      });
    }

    const config = await loadConfig(this.options.configPath);
    getSDK({
      debug: this.options.logLevel === 'debug',
      logLevel: this.options.logLevel,
      presets: config.presets,
    });
  }

  /**
   * 工作流 API
   */
  get workflows() {
    return {
      register: async (filePath: string, parameters?: Record<string, unknown>) => {
        const adapter = new WorkflowAdapter();
        return adapter.registerFromFile(filePath, parameters);
      },
      
      list: async (filter?: any) => {
        const adapter = new WorkflowAdapter();
        return adapter.listWorkflows(filter);
      },
      
      get: async (id: string) => {
        const adapter = new WorkflowAdapter();
        return adapter.getWorkflow(id);
      },
      
      delete: async (id: string) => {
        const adapter = new WorkflowAdapter();
        return adapter.deleteWorkflow(id);
      },
    };
  }

  /**
   * 线程 API
   */
  get threads() {
    return {
      create: async (workflowId: string, input?: Record<string, unknown>) => {
        const adapter = new ThreadAdapter();
        return adapter.createThread(workflowId, input);
      },
      
      execute: async (threadId: string, input?: Record<string, unknown>) => {
        const adapter = new ThreadAdapter();
        return adapter.executeThread(threadId, input);
      },
      
      get: async (threadId: string) => {
        const adapter = new ThreadAdapter();
        return adapter.getThread(threadId);
      },
    };
  }

  // ... 其他 API
}

/**
 * 创建程序化 API 实例
 */
export function createAPI(options?: ProgrammaticAPIOptions): ProgrammaticAPI {
  return new ProgrammaticAPI(options);
}
```

## 使用示例

### 1. 无头模式命令行使用

```bash
# JSON 输出模式
modular-agent workflow list --json

# 环境变量方式
CLI_MODE=headless CLI_OUTPUT_FORMAT=json modular-agent workflow list

# 完整配置
CLI_MODE=headless \
  CLI_OUTPUT_FORMAT=json \
  CLI_LOG_LEVEL=error \
  modular-agent workflow register ./workflow.toml
```

### 2. 程序化 API 使用

```typescript
// 在其他应用中使用
import { createAPI } from '@wf-agent/cli-app/api';

async function main() {
  const api = createAPI({
    logLevel: 'error',
    enableLogging: true,
  });
  
  await api.initialize();
  
  // 注册工作流
  const workflow = await api.workflows.register('./my-workflow.toml');
  console.log('Registered:', workflow.id);
  
  // 创建并执行线程
  const thread = await api.threads.create(workflow.id, { input: 'test' });
  const result = await api.threads.execute(thread.id);
  
  return result;
}
```

### 3. 测试中使用

```typescript
// 测试代码
import { createAPI } from '@wf-agent/cli-app/api';

describe('Workflow Tests', () => {
  let api: ReturnType<typeof createAPI>;
  
  beforeAll(async () => {
    api = createAPI({ logLevel: 'silent' });
    await api.initialize();
  });
  
  test('should register workflow', async () => {
    const workflow = await api.workflows.register('./test-workflow.toml');
    expect(workflow).toBeDefined();
    expect(workflow.id).toBeTruthy();
  });
});
```

## 环境变量参考

| 环境变量 | 说明 | 可选值 | 默认值 |
|----------|------|--------|--------|
| `CLI_MODE` | 执行模式 | `interactive`, `headless`, `programmatic` | `interactive` |
| `HEADLESS` | 启用无头模式（向后兼容） | `true`, `false` | `false` |
| `TEST_MODE` | 测试模式（启用无头特性） | `true`, `false` | `false` |
| `CLI_OUTPUT_FORMAT` | 输出格式 | `text`, `json`, `silent` | 根据模式 |
| `CLI_LOG_LEVEL` | 日志级别 | `debug`, `verbose`, `info`, `warn`, `error` | `info` |
| `CLI_LOG_FILE` | 日志文件路径 | 文件路径 | - |
| `NO_COLOR` | 禁用彩色输出 | `true`, `false` | `false` |

## 相关文档

- [execution-modes-analysis.md](./execution-modes-analysis.md) - 执行模式架构分析
- [headless-mode-implementation.md](./headless-mode-implementation.md) - 无头模式实现指南
