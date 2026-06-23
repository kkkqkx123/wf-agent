# CLI App 存储集成设计文档

## 1. 概述

本文档描述了 CLI 应用如何集成 `@wf-agent/storage` 包的 JSON 存储实现，以实现工作流、工作流执行实例、检查点等数据的持久化存储。

## 2. 当前问题分析

### 2.1 存在的问题

1. **数据丢失**：当前 CLI 应用在退出后，所有注册的工作流、工作流执行实例等数据都会丢失，因为没有持久化存储。

2. **测试复杂度高**：测试模式需要设置多个环境变量（`TEST_MODE`、`LOG_DIR`、`DISABLE_LOG_TERMINAL`、`DISABLE_SDK_LOGS`、`SDK_LOG_LEVEL`），使用繁琐。

3. **缺乏配置管理**：存储路径、输出路径等配置硬编码或依赖环境变量，缺乏统一的配置管理机制。

### 2.2 根本原因

- SDK 默认使用内存存储（`WorkflowRegistry`、`WorkflowExecutionRegistry` 等），没有集成持久化存储。
- CLI 应用没有提供存储回调接口的实现。
- 配置系统只支持应用级配置（如 API URL、超时等），不支持存储相关配置。

## 3. 存储架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI App                              │
├─────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Configuration Layer                      │  │
│  │  - CLIConfig (application config)                     │  │
│  │  - StorageConfig (storage-specific config)            │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Storage Integration Layer                │  │
│  │  - StorageManager (main integration point)            │  │
│  │  - JsonWorkflowStorage                               │  │
│  │  - JsonWorkflowExecutionStorage                       │  │
│  │  - JsonCheckpointStorage                              │  │
│  │  - JsonTaskStorage                                    │  │
│  │  - JsonNoteStorage                                    │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Storage Callback Adapter                 │  │
│  │  - CheckpointStorageCallback (SDK interface)          │  │
│  │  - WorkflowStorageCallback (SDK interface)            │  │
│  │  - WorkflowExecutionStorageCallback (SDK interface)   │  │
│  │  - TaskStorageCallback (SDK interface)                │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              @wf-agent/storage Package             │  │
│  │  - BaseJsonStorage (metadata-data separation)         │  │
│  │  - Compression support (optional)                     │  │
│  │  - File locking (optional)                            │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              File System                              │  │
│  │  storage/                                             │  │
│  │    ├── metadata/                                      │  │
│  │    │   ├── workflow/                                  │  │
│  │    │   ├── workflow-execution/                        │  │
│  │    │   ├── checkpoint/                                │  │
│  │    │   ├── task/                                      │  │
│  │    │   └── note/                                      │  │
│  │    └── data/                                          │  │
│  │        ├── workflow/                                  │  │
│  │        ├── workflow-execution/                        │  │
│  │        ├── checkpoint/                                │  │
│  │        ├── task/                                      │  │
│  │        └── note/                                      │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 存储类型映射

| SDK 存储接口 | JSON 存储实现 | 存储的数据 |
|--------------|--------------|-----------|
| `WorkflowStorageCallback` | `JsonWorkflowStorage` | 工作流定义、版本历史 |
| `WorkflowExecutionStorageCallback` | `JsonWorkflowExecutionStorage` | 工作流执行实例状态、消息历史 |
| `CheckpointStorageCallback` | `JsonCheckpointStorage` | 检查点状态快照 |
| `TaskStorageCallback` | `JsonTaskStorage` | 任务执行状态 |
| `NoteStorageCallback` | `JsonNoteStorage` | 会话笔记 |

## 4. 配置设计

### 4.1 扩展配置 Schema

在现有的 `CLIConfig` 基础上，添加存储相关配置：

```typescript
// apps/cli-app/src/config/config-loader.ts

const ConfigSchema = z.object({
  // ... 现有配置 ...

  // 新增：存储配置
  storage: z
    .object({
      // 存储类型：'json' | 'sqlite' | 'memory'
      type: z.enum(["json", "sqlite", "memory"]).default("json"),

      // JSON 存储配置
      json: z
        .object({
          // 基础存储目录
          baseDir: z.string().default("./storage"),

          // 是否启用文件锁
          enableFileLock: z.boolean().default(false),

          // 压缩配置
          compression: z
            .object({
              enabled: z.boolean().default(false),
              algorithm: z.enum(["gzip", "brotli", "zlib"]).default("gzip"),
              threshold: z.number().default(1024), // bytes
            })
            .optional(),
        })
        .optional(),

      // SQLite 存储配置（预留）
      sqlite: z
        .object({
          dbPath: z.string().default("./storage/cli-app.db"),
          enableWAL: z.boolean().default(true),
        })
        .optional(),
    })
    .optional(),

  // 输出配置
  output: z
    .object({
      // 输出目录
      dir: z.string().default("./outputs"),

      // 日志文件名模式
      logFilePattern: z.string().default("cli-app-{date}.log"),

      // 是否启用日志终端输出
      enableLogTerminal: z.boolean().default(true),

      // 是否启用 SDK 日志
      enableSDKLogs: z.boolean().default(true),

      // SDK 日志级别
      sdkLogLevel: z.enum(["silent", "error", "warn", "info", "debug"]).default("silent"),
    })
    .optional(),
});

export type CLIConfig = z.infer<typeof ConfigSchema>;
```

### 4.2 默认配置

```typescript
const DEFAULT_CONFIG: Partial<CLIConfig> = {
  // ... 现有默认配置 ...

  storage: {
    type: "json",
    json: {
      baseDir: "./storage",
      enableFileLock: false,
      compression: {
        enabled: false,
      },
    },
  },

  output: {
    dir: "./outputs",
    logFilePattern: "cli-app-{date}.log",
    enableLogTerminal: true,
    enableSDKLogs: true,
    sdkLogLevel: "silent",
  },
};
```

### 4.3 配置文件示例

```toml
# .modular-agentrc.toml

[storage]
type = "json"

[storage.json]
baseDir = "./storage"
enableFileLock = false

[storage.json.compression]
enabled = true
algorithm = "gzip"
threshold = 1024

[output]
dir = "./outputs"
logFilePattern = "cli-app-{date}.log"
enableLogTerminal = true
enableSDKLogs = true
sdkLogLevel = "silent"

[presets.contextCompression]
enabled = true
timeout = 30000
maxTriggers = 10

[presets.predefinedTools]
enabled = true

[presets.predefinedPrompts]
enabled = true
```

## 5. 实现设计

### 5.1 StorageManager

创建统一的存储管理器，负责初始化和管理所有存储实例。

```typescript
// apps/cli-app/src/storage/storage-manager.ts

import type {
  CheckpointStorageCallback,
  WorkflowStorageCallback,
  WorkflowExecutionStorageCallback,
  TaskStorageCallback,
  NoteStorageCallback,
} from "@wf-agent/storage";
import {
  JsonCheckpointStorage,
  JsonWorkflowStorage,
  JsonWorkflowExecutionStorage,
  JsonTaskStorage,
  JsonNoteStorage,
  type BaseJsonStorageConfig,
} from "@wf-agent/storage";
import type { CLIConfig } from "../config/config-loader.js";
import { logger } from "../utils/logger.js";

/**
 * Storage Manager
 * Unified management of all storage instances
 */
export class StorageManager {
  private workflowStorage: WorkflowStorageCallback | null = null;
  private workflowExecutionStorage: WorkflowExecutionStorageCallback | null = null;
  private checkpointStorage: CheckpointStorageCallback | null = null;
  private taskStorage: TaskStorageCallback | null = null;
  private noteStorage: NoteStorageCallback | null = null;
  private initialized: boolean = false;

  constructor(private config: CLIConfig) {}

  /**
   * Initialize all storage instances
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      logger.warn("StorageManager already initialized");
      return;
    }

    const storageConfig = this.config.storage;
    if (!storageConfig || storageConfig.type === "memory") {
      logger.info("Using in-memory storage (no persistence)");
      this.initialized = true;
      return;
    }

    if (storageConfig.type === "json") {
      await this.initializeJsonStorage(storageConfig.json);
    } else if (storageConfig.type === "sqlite") {
      throw new Error("SQLite storage not yet implemented");
    } else {
      throw new Error(`Unknown storage type: ${storageConfig.type}`);
    }

    this.initialized = true;
    logger.info("StorageManager initialized successfully");
  }

  /**
   * Initialize JSON storage
   */
  private async initializeJsonStorage(config?: BaseJsonStorageConfig): Promise<void> {
    const baseDir = config?.baseDir ?? "./storage";
    const enableFileLock = config?.enableFileLock ?? false;
    const compression = config?.compression;

    const baseConfig: BaseJsonStorageConfig = {
      baseDir,
      enableFileLock,
      compression: compression
        ? {
            enabled: compression.enabled,
            algorithm: compression.algorithm,
            threshold: compression.threshold,
          }
        : undefined,
    };

    // Initialize workflow storage
    this.workflowStorage = new JsonWorkflowStorage(baseConfig);
    await this.workflowStorage.initialize();
    logger.info("WorkflowStorage initialized", { baseDir });

    // Initialize workflow execution storage
    this.workflowExecutionStorage = new JsonWorkflowExecutionStorage(baseConfig);
    await this.workflowExecutionStorage.initialize();
    logger.info("WorkflowExecutionStorage initialized", { baseDir });

    // Initialize checkpoint storage
    this.checkpointStorage = new JsonCheckpointStorage(baseConfig);
    await this.checkpointStorage.initialize();
    logger.info("CheckpointStorage initialized", { baseDir });

    // Initialize task storage
    this.taskStorage = new JsonTaskStorage(baseConfig);
    await this.taskStorage.initialize();
    logger.info("TaskStorage initialized", { baseDir });

    // Initialize note storage
    this.noteStorage = new JsonNoteStorage(baseConfig);
    await this.noteStorage.initialize();
    logger.info("NoteStorage initialized", { baseDir });
  }

  /**
   * Get workflow storage
   */
  getWorkflowStorage(): WorkflowStorageCallback | null {
    return this.workflowStorage;
  }

  /**
   * Get workflow execution storage
   */
  getWorkflowExecutionStorage(): WorkflowExecutionStorageCallback | null {
    return this.workflowExecutionStorage;
  }

  /**
   * Get checkpoint storage
   */
  getCheckpointStorage(): CheckpointStorageCallback | null {
    return this.checkpointStorage;
  }

  /**
   * Get task storage
   */
  getTaskStorage(): TaskStorageCallback | null {
    return this.taskStorage;
  }

  /**
   * Get note storage
   */
  getNoteStorage(): NoteStorageCallback | null {
    return this.noteStorage;
  }

  /**
   * Close all storage instances
   */
  async close(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    const promises: Promise<void>[] = [];

    if (this.workflowStorage) {
      promises.push(this.workflowStorage.close());
    }
    if (this.workflowExecutionStorage) {
      promises.push(this.workflowExecutionStorage.close());
    }
    if (this.checkpointStorage) {
      promises.push(this.checkpointStorage.close());
    }
    if (this.taskStorage) {
      promises.push(this.taskStorage.close());
    }
    if (this.noteStorage) {
      promises.push(this.noteStorage.close());
    }

    await Promise.all(promises);
    this.initialized = false;
    logger.info("StorageManager closed");
  }

  /**
   * Clear all storage data
   */
  async clear(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    const promises: Promise<void>[] = [];

    if (this.workflowStorage) {
      promises.push(this.workflowStorage.clear());
    }
    if (this.workflowExecutionStorage) {
      promises.push(this.workflowExecutionStorage.clear());
    }
    if (this.checkpointStorage) {
      promises.push(this.checkpointStorage.clear());
    }
    if (this.taskStorage) {
      promises.push(this.taskStorage.clear());
    }
    if (this.noteStorage) {
      promises.push(this.noteStorage.clear());
    }

    await Promise.all(promises);
    logger.info("StorageManager cleared");
  }
}

/**
 * Global storage manager instance
 */
let globalStorageManager: StorageManager | null = null;

/**
 * Get the global storage manager instance
 */
export function getStorageManager(): StorageManager | null {
  return globalStorageManager;
}

/**
 * Initialize the global storage manager
 */
export async function initializeStorageManager(config: CLIConfig): Promise<StorageManager> {
  if (globalStorageManager) {
    return globalStorageManager;
  }

  globalStorageManager = new StorageManager(config);
  await globalStorageManager.initialize();
  return globalStorageManager;
}

/**
 * Close the global storage manager
 */
export async function closeStorageManager(): Promise<void> {
  if (globalStorageManager) {
    await globalStorageManager.close();
    globalStorageManager = null;
  }
}
```

### 5.2 集成到 CLI 主入口

```typescript
// apps/cli-app/src/index.ts

import { initializeStorageManager, closeStorageManager } from "./storage/storage-manager.js";
import { getStorageManager } from "./storage/storage-manager.js";
import { setStorageCallback } from "@wf-agent/sdk";

// ... 在 preAction hook 中 ...

program
  .hook("preAction", async thisCommand => {
    // 1. Initialize the output system
    const output = initializeOutput({
      config: config.output,
    });

    // 2. Initialize the formatter
    initializeFormatter(output.colorEnabled);

    // 3. Initialize the log
    initLogger({
      config: config.output,
    });

    initSDKLogger({
      config: config.output,
    });

    // 4. Initialize storage manager
    await initializeStorageManager(config);

    const storageManager = getStorageManager();

    // 5. Register storage callbacks with SDK
    if (storageManager) {
      const checkpointStorage = storageManager.getCheckpointStorage();
      if (checkpointStorage) {
        setStorageCallback(checkpointStorage);
      }
    }

    // 6. Load the global configuration and initialize the SDK
    const sdk = getSDK({
      debug: options.debug,
      logLevel: options.debug ? "debug" : options.verbose ? "info" : "warn",
      presets: config.presets,
      checkpointStorageCallback: storageManager?.getCheckpointStorage() ?? undefined,
    });

    // ... rest of the initialization ...
  });

// ... 在 shutdown 函数中 ...

const shutdown = async () => {
  const output = getOutput();
  output.infoLog("Cleaning up resources...");

  try {
    // Close storage manager
    await closeStorageManager();

    // Close the output stream
    await output.close();
    process.exit(0);
  } catch (error) {
    output.errorLog(`Error cleaning up resources: ${error instanceof Error ? error.message : String(error)}`);
    await output.close();
    process.exit(1);
  }
};
```

### 5.3 更新输出系统以支持配置

```typescript
// apps/cli-app/src/utils/output.ts

export interface OutputConfig {
  /** Log file path */
  logFile?: string;
  /** Whether to enable colors */
  color?: boolean;
  /** Is it in detail mode? */
  verbose?: boolean;
  /** Is it in debug mode? */
  debug?: boolean;
  /** Output directory (from config) */
  outputDir?: string;
  /** Log file pattern (from config) */
  logFilePattern?: string;
  /** Whether to enable log terminal output */
  enableLogTerminal?: boolean;
  /** Whether to enable SDK logs */
  enableSDKLogs?: boolean;
  /** SDK log level */
  sdkLogLevel?: string;
}

export class CLIOutput {
  constructor(config: OutputConfig = {}) {
    this._stdout = process.stdout;
    this._stderr = process.stderr;
    this._colorEnabled = config.color ?? this._supportsColor();
    this._verbose = config.verbose ?? false;
    this._debug = config.debug ?? false;

    // Initialize the log file
    this._logFile = config.logFile || this._getDefaultLogPath(config);
    this._logStream = fs.createWriteStream(this._logFile, { flags: "a" });
  }

  private _getDefaultLogPath(config: OutputConfig = {}): string {
    const outputDir = config.outputDir ?? (process.env["TEST_MODE"] && process.env["LOG_DIR"]
      ? process.env["LOG_DIR"]
      : path.join(process.cwd(), "logs"));

    if (!fs.existsSync(outputDir)) {
      fs.mkdirSync(outputDir, { recursive: true });
    }

    const pattern = config.logFilePattern ?? "cli-app-{date}.log";
    const date = new Date().toISOString().split("T")[0];
    const logFileName = pattern.replace("{date}", date);

    return path.join(outputDir, logFileName);
  }
}

export function initializeOutput(config: OutputConfig = {}): CLIOutput {
  globalOutput = new CLIOutput(config);
  return globalOutput;
}
```

### 5.4 更新日志系统以支持配置

```typescript
// apps/cli-app/src/utils/logger.ts

export interface LoggerOptions {
  verbose?: boolean;
  debug?: boolean;
  logFile?: string;
  outputDir?: string;
  logFilePattern?: string;
  enableLogTerminal?: boolean;
  enableSDKLogs?: boolean;
  sdkLogLevel?: string;
}

export function initLogger(options: LoggerOptions = {}): void {
  // Determine log level based on environment variables or configuration
  const disableLogTerminal =
    process.env["DISABLE_LOG_TERMINAL"] === "true" || options.enableLogTerminal === false;

  const level = options.debug ? "debug" : options.verbose ? "info" : "warn";

  const output = getOutput();
  const logStream = output.logStream as Writable;

  const logger = createPackageLogger("cli-app", {
    level,
    stream: logStream,
    timestamp: true,
  });

  registerLogger("cli-app", logger);
}

export function initSDKLogger(options: LoggerOptions = {}): void {
  const disableSDKLogs = process.env["DISABLE_SDK_LOGS"] === "true" || options.enableSDKLogs === false;

  if (disableSDKLogs) {
    // SDK logs are disabled, do not register SDK logger
    return;
  }

  const sdkLogLevelEnv = process.env["SDK_LOG_LEVEL"];
  const sdkLogLevelConfig = options.sdkLogLevel ?? "silent";

  // Use environment variables first, then use the configuration, and finally use the default value
  const sdkLogLevel = sdkLogLevelEnv ?? sdkLogLevelConfig;

  // If the log level is 'silent', do not register the SDK logger
  if (sdkLogLevel === "silent") {
    return;
  }

  const level = sdkLogLevel === "debug" ? "debug" : sdkLogLevel === "info" ? "info" : sdkLogLevel === "warn" ? "warn" : "error";

  const output = getOutput();
  const logStream = output.logStream as Writable;

  const logger = createPackageLogger("sdk", {
    level,
    stream: logStream,
    timestamp: true,
  });

  registerLogger("sdk", logger);
}
```

## 6. 测试设计

### 6.1 测试配置文件

为测试环境创建专用配置文件：

```toml
# apps/cli-app/__tests__/config/test-config.toml

[storage]
type = "json"

[storage.json]
baseDir = "./__tests__/storage"
enableFileLock = false

[output]
dir = "./__tests__/outputs"
logFilePattern = "cli-app-{date}.log"
enableLogTerminal = false
enableSDKLogs = false
sdkLogLevel = "silent"

[presets.contextCompression]
enabled = true

[presets.predefinedTools]
enabled = true

[presets.predefinedPrompts]
enabled = true
```

### 6.2 更新测试辅助工具

```typescript
// apps/cli-app/__tests__/utils/test-config-loader.ts

import { ConfigLoader } from "../../src/config/config-loader.js";

/**
 * Load test configuration
 */
export function loadTestConfig(configPath?: string) {
  const loader = new ConfigLoader();
  const config = loader.load(configPath);
  return config;
}

/**
 * Get the default test configuration path
 */
export function getDefaultTestConfigPath() {
  return "./__tests__/config/test-config.toml";
}
```

### 6.3 更新测试运行器

```typescript
// apps/cli-app/__tests__/utils/cli-runner.ts

export class CLIRunner {
  private cliPath: string;
  private outputDir: string;
  private defaultEnv: Record<string, string>;
  private outputFileCounter: number;

  constructor(cliPath?: string, outputDir?: string, configPath?: string) {
    this.cliPath = cliPath || this.findCLIPath();
    this.outputDir = outputDir || resolve(__dirname, "../outputs");
    this.defaultEnv = {
      ...process.env,
      NODE_ENV: "test",
      TEST_MODE: "true",
      // Use configuration file to replace environment variables
      CLI_CONFIG_PATH: configPath || resolve(__dirname, "../config/test-config.toml"),
    };
    this.outputFileCounter = 0;
  }

  async run(args: string[], options: CLIRunOptions = {}): Promise<CLIRunResult> {
    const {
      timeout = 30000,
      input,
      cwd = process.cwd(),
      env = {},
      saveOutput = true,
      outputSubdir = "general",
    } = options;

    const startTime = Date.now();
    const result = await this.executeCommand(args, {
      timeout,
      input,
      cwd,
      env,
    });

    result.duration = Date.now() - startTime;

    if (saveOutput) {
      const outputFilePath = await this.saveOutput(result, args, outputSubdir);
      result.outputFilePath = outputFilePath;
    }

    return result;
  }

  private async executeCommand(
    args: string[],
    options: { timeout: number; input?: string; cwd: string; env: Record<string, string> },
  ): Promise<CLIRunResult> {
    return new Promise(resolve => {
      const child = spawn("node", [this.cliPath, ...args], {
        env: { ...this.defaultEnv, ...options.env },
        cwd: options.cwd,
        stdio: ["pipe", "pipe", "pipe"],
      });

      let stdout = "";
      let stderr = "";

      child.stdout?.on("data", data => {
        stdout += data.toString();
      });

      child.stderr?.on("data", data => {
        stderr += data.toString();
      });

      if (options.input && child.stdin) {
        child.stdin.write(options.input);
        child.stdin.end();
      }

      const timer = setTimeout(() => {
        child.kill();
        resolve({
          exitCode: -1,
          stdout,
          stderr: `Timeout after ${options.timeout}ms`,
          duration: 0,
        });
      }, options.timeout);

      child.on("close", code => {
        clearTimeout(timer);
        resolve({
          exitCode: code,
          stdout,
          stderr,
          duration: 0,
        });
      });

      child.on("error", error => {
        clearTimeout(timer);
        resolve({
          exitCode: -1,
          stdout,
          stderr: error.message,
          duration: 0,
        });
      });
    });
  }
}
```

### 6.4 更新测试用例

```typescript
// apps/cli-app/__tests__/integration/workflows/01-registration.test.ts

import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";
import { loadTestConfig, getDefaultTestConfigPath } from "../../utils/test-config-loader";

describe("Workflow Registration Tests", () => {
  let helper: TestHelper;
  let workflowHelper: WorkflowTestHelper;
  let logger: TestLogger;
  let runner: CLIRunner;
  const testOutputDir = resolve(__dirname, "../../outputs/workflow-registration");

  beforeAll(() => {
    logger = new TestLogger(testOutputDir);
    // Load test configuration file
    const testConfigPath = getDefaultTestConfigPath();
    runner = new CLIRunner(undefined, testOutputDir, testConfigPath);
  });

  // ... rest of the test code ...
});
```

## 7. 迁移计划

### 7.1 阶段一：基础集成（Week 1）

1. **配置系统扩展**
   - 扩展 `CLIConfig` Schema，添加存储和输出配置
   - 更新默认配置
   - 创建测试配置文件

2. **存储管理器实现**
   - 实现 `StorageManager` 类
   - 实现 JSON 存储初始化逻辑
   - 实现存储回调接口

3. **CLI 主入口集成**
   - 在 `preAction` hook 中初始化存储管理器
   - 注册存储回调到 SDK
   - 在 `shutdown` 函数中关闭存储管理器

4. **输出系统更新**
   - 更新 `CLIOutput` 以支持配置
   - 更新日志系统以支持配置

### 7.2 阶段二：测试迁移（Week 2）

1. **测试辅助工具更新**
   - 实现 `test-config-loader.ts`
   - 更新 `CLIRunner` 以支持配置文件
   - 更新测试用例以使用配置文件

2. **测试执行**
   - 运行所有集成测试
   - 修复发现的问题
   - 验证数据持久化

### 7.3 阶段三：优化和完善（Week 3）

1. **性能优化**
   - 评估压缩配置
   - 优化文件锁策略
   - 添加缓存机制

2. **错误处理**
   - 完善错误处理逻辑
   - 添加详细的错误日志
   - 提供友好的错误提示

3. **文档完善**
   - 更新用户文档
   - 添加配置示例
   - 编写故障排查指南

## 8. 风险和挑战

### 8.1 技术风险

1. **存储性能**：JSON 文件存储在大量数据时可能性能不佳，需要评估压缩和索引策略。

2. **并发控制**：文件锁机制可能影响性能，需要评估是否启用。

3. **数据一致性**：需要确保元数据和数据的原子性更新。

### 8.2 兼容性风险

1. **向后兼容**：需要确保现有用户的工作流不受影响。

2. **配置迁移**：需要提供配置迁移指南。

### 8.3 测试风险

1. **测试隔离**：需要确保测试之间的数据隔离。

2. **测试清理**：需要确保测试后的数据清理。

## 9. 总结

本设计文档详细描述了 CLI 应用如何集成 `@wf-agent/storage` 包的 JSON 存储实现，以实现工作流、线程、检查点等数据的持久化存储。通过扩展配置系统、实现存储管理器、更新 CLI 主入口和测试辅助工具，我们可以：

1. **解决数据丢失问题**：通过持久化存储，确保数据在应用退出后仍然存在。

2. **简化测试配置**：通过配置文件，避免设置多个环境变量，提高测试的可维护性。

3. **提高可扩展性**：通过统一的存储管理器，便于未来支持其他存储后端（如 SQLite）。

4. **改善用户体验**：通过友好的配置文件和详细的文档，降低用户的使用门槛。
