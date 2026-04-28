# CLI App 配置功能设计文档

## 1. 概述

本文档描述了 CLI 应用的配置功能设计，包括配置文件格式、配置加载机制、配置验证和配置管理等方面的详细设计。

## 2. 当前问题分析

### 2.1 存在的问题

1. **测试配置复杂**：测试模式需要设置多个环境变量（`TEST_MODE`、`LOG_DIR`、`DISABLE_LOG_TERMINAL`、`DISABLE_SDK_LOGS`、`SDK_LOG_LEVEL`），使用繁琐且容易出错。

2. **配置分散**：配置信息分散在环境变量、代码硬编码和用户设置中，缺乏统一的管理。

3. **缺乏灵活性**：用户无法通过配置文件自定义存储路径、输出路径等关键配置。

4. **测试隔离困难**：不同测试套件之间难以实现配置隔离，容易相互干扰。

### 2.2 根本原因

- 配置系统只支持应用级配置（如 API URL、超时等），不支持测试相关配置。
- 缺乏配置文件的优先级机制，无法区分开发、测试、生产环境。
- 配置加载逻辑与业务逻辑耦合，缺乏灵活性。

## 3. 配置架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI App                              │
├─────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Configuration Sources                    │  │
│  │  1. Configuration File (.modular-agentrc.toml)        │  │
│  │  2. Environment Variables                             │  │
│  │  3. Command Line Options                              │  │
│  │  4. Default Values                                    │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Configuration Loader                      │  │
│  │  - Load configuration from multiple sources            │  │
│  │  - Merge configurations with priority                 │  │
│  │  - Validate configuration schema                       │  │
│  │  - Cache configuration                                 │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Configuration Manager                     │  │
│  │  - Provide configuration access API                   │  │
│  │  - Support configuration hot reload                    │  │
│  │  - Support configuration validation                    │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Configuration Consumers                  │  │
│  │  - Output System (CLIOutput)                           │  │
│  │  - Logger System (Logger)                              │  │
│  │  - Storage Manager (StorageManager)                    │  │
│  │  - SDK (getSDK)                                        │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 配置优先级

配置优先级从高到低：

1. **命令行选项**：最高优先级，用于临时覆盖配置
2. **环境变量**：用于 CI/CD 和测试环境
3. **配置文件**：用于持久化配置
4. **默认值**：最低优先级，作为后备

### 3.3 配置文件搜索路径

配置加载器按以下顺序搜索配置文件：

1. 当前工作目录：`./.modular-agentrc.toml`
2. 用户主目录：`~/.modular-agentrc.toml`
3. 项目根目录：`<project-root>/.modular-agentrc.toml`
4. 测试配置：`./__tests__/config/test-config.toml`（仅在测试模式下）

## 4. 配置 Schema 设计

### 4.1 完整配置 Schema

```typescript
// apps/cli-app/src/config/config-loader.ts

import { cosmiconfig } from "cosmiconfig";
import { z } from "zod";
import { getOutput } from "../utils/output.js";

const output = getOutput();

/**
 * Compression Configuration
 */
const CompressionConfigSchema = z.object({
  /** Whether to enable compression */
  enabled: z.boolean().default(false),
  /** Compression algorithm */
  algorithm: z.enum(["gzip", "brotli", "zlib"]).default("gzip"),
  /** Compression threshold (bytes) */
  threshold: z.number().default(1024),
});

/**
 * JSON Storage Configuration
 */
const JsonStorageConfigSchema = z.object({
  /** Base storage directory */
  baseDir: z.string().default("./storage"),
  /** Whether to enable file locking */
  enableFileLock: z.boolean().default(false),
  /** Compression configuration */
  compression: CompressionConfigSchema.optional(),
});

/**
 * SQLite Storage Configuration (Reserved)
 */
const SqliteStorageConfigSchema = z.object({
  /** Database file path */
  dbPath: z.string().default("./storage/cli-app.db"),
  /** Whether to enable WAL mode */
  enableWAL: z.boolean().default(true),
});

/**
 * Storage Configuration
 */
const StorageConfigSchema = z.object({
  /** Storage type */
  type: z.enum(["json", "sqlite", "memory"]).default("json"),
  /** JSON storage configuration */
  json: JsonStorageConfigSchema.optional(),
  /** SQLite storage configuration */
  sqlite: SqliteStorageConfigSchema.optional(),
});

/**
 * Output Configuration
 */
const OutputConfigSchema = z.object({
  /** Output directory */
  dir: z.string().default("./outputs"),
  /** Log file pattern */
  logFilePattern: z.string().default("cli-app-{date}.log"),
  /** Whether to enable log terminal output */
  enableLogTerminal: z.boolean().default(true),
  /** Whether to enable SDK logs */
  enableSDKLogs: z.boolean().default(true),
  /** SDK log level */
  sdkLogLevel: z.enum(["silent", "error", "warn", "info", "debug"]).default("silent"),
});

/**
 * Context Compression Preset Configuration
 */
const ContextCompressionPresetConfigSchema = z.object({
  /** Whether to enable context compression */
  enabled: z.boolean().default(true),
  /** Compression prompt */
  prompt: z.string().optional(),
  /** Compression timeout */
  timeout: z.number().optional(),
  /** Maximum number of triggers */
  maxTriggers: z.number().optional(),
});

/**
 * Predefined Tools Preset Configuration
 */
const PredefinedToolsPresetConfigSchema = z.object({
  /** Whether to enable predefined tools */
  enabled: z.boolean().default(true),
  /** Allow list */
  allowList: z.array(z.string()).optional(),
  /** Block list */
  blockList: z.array(z.string()).optional(),
  /** Tool-specific configuration */
  config: z
    .object({
      /** Read file tool configuration */
      readFile: z
        .object({
          workspaceDir: z.string().optional(),
          maxFileSize: z.number().optional(),
        })
        .optional(),
      /** Write file tool configuration */
      writeFile: z
        .object({
          workspaceDir: z.string().optional(),
        })
        .optional(),
      /** Edit file tool configuration */
      editFile: z
        .object({
          workspaceDir: z.string().optional(),
        })
        .optional(),
      /** Bash tool configuration */
      bash: z
        .object({
          defaultTimeout: z.number().optional(),
          maxTimeout: z.number().optional(),
        })
        .optional(),
      /** Session note tool configuration */
      sessionNote: z
        .object({
          workspaceDir: z.string().optional(),
          memoryFile: z.string().optional(),
        })
        .optional(),
      /** Background shell tool configuration */
      backgroundShell: z
        .object({
          workspaceDir: z.string().optional(),
        })
        .optional(),
    })
    .optional(),
});

/**
 * Predefined Prompts Preset Configuration
 */
const PredefinedPromptsPresetConfigSchema = z.object({
  /** Whether to enable predefined prompts */
  enabled: z.boolean().default(true),
});

/**
 * Presets Configuration
 */
const PresetsConfigSchema = z.object({
  /** Context compression preset */
  contextCompression: ContextCompressionPresetConfigSchema.optional(),
  /** Predefined tools preset */
  predefinedTools: PredefinedToolsPresetConfigSchema.optional(),
  /** Predefined prompts preset */
  predefinedPrompts: PredefinedPromptsPresetConfigSchema.optional(),
});

/**
 * Complete Configuration Schema
 */
const ConfigSchema = z.object({
  /** API URL (reserved) */
  apiUrl: z.string().url().optional(),
  /** API Key (reserved) */
  apiKey: z.string().optional(),
  /** Default timeout */
  defaultTimeout: z.number().positive().default(30000),
  /** Whether to enable verbose mode */
  verbose: z.boolean().default(false),
  /** Whether to enable debug mode */
  debug: z.boolean().default(false),
  /** Log level */
  logLevel: z.enum(["error", "warn", "info", "debug"]).default("warn"),
  /** Output format */
  outputFormat: z.enum(["json", "table", "plain"]).default("table"),
  /** Maximum concurrent threads */
  maxConcurrentThreads: z.number().positive().default(5),
  /** Storage configuration */
  storage: StorageConfigSchema.optional(),
  /** Output configuration */
  output: OutputConfigSchema.optional(),
  /** Presets configuration */
  presets: PresetsConfigSchema.optional(),
});

/**
 * Configuration Type
 */
export type CLIConfig = z.infer<typeof ConfigSchema>;

/**
 * Default Configuration
 */
const DEFAULT_CONFIG: Partial<CLIConfig> = {
  defaultTimeout: 30000,
  verbose: false,
  debug: false,
  logLevel: "warn",
  outputFormat: "table",
  maxConcurrentThreads: 5,
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
  presets: {
    contextCompression: {
      enabled: true,
    },
    predefinedTools: {
      enabled: true,
    },
    predefinedPrompts: {
      enabled: true,
    },
  },
};
```

### 4.2 配置文件示例

#### 4.2.1 开发环境配置

```toml
# .modular-agentrc.toml (Development)

[storage]
type = "json"

[storage.json]
baseDir = "./storage"
enableFileLock = false

[storage.json.compression]
enabled = false

[output]
dir = "./outputs"
logFilePattern = "cli-app-{date}.log"
enableLogTerminal = true
enableSDKLogs = true
sdkLogLevel = "debug"

[presets.contextCompression]
enabled = true
timeout = 30000
maxTriggers = 10

[presets.predefinedTools]
enabled = true

[presets.predefinedPrompts]
enabled = true
```

#### 4.2.2 测试环境配置

```toml
# __tests__/config/test-config.toml (Test)

[storage]
type = "json"

[storage.json]
baseDir = "./__tests__/storage"
enableFileLock = false

[storage.json.compression]
enabled = false

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

#### 4.2.3 生产环境配置

```toml
# .modular-agentrc.toml (Production)

[storage]
type = "json"

[storage.json]
baseDir = "./storage"
enableFileLock = true

[storage.json.compression]
enabled = true
algorithm = "gzip"
threshold = 1024

[output]
dir = "./outputs"
logFilePattern = "cli-app-{date}.log"
enableLogTerminal = true
enableSDKLogs = true
sdkLogLevel = "warn"

[presets.contextCompression]
enabled = true
timeout = 30000
maxTriggers = 10

[presets.predefinedTools]
enabled = true

[presets.predefinedPrompts]
enabled = true
```

## 5. 配置加载机制

### 5.1 ConfigLoader 实现

```typescript
// apps/cli-app/src/config/config-loader.ts

import { cosmiconfig } from "cosmiconfig";
import { z } from "zod";
import { getOutput } from "../utils/output.js";

const output = getOutput();

/**
 * Configuration Loader Class
 */
export class ConfigLoader {
  private explorer: ReturnType<typeof cosmiconfig>;
  private cachedConfig: CLIConfig | null = null;

  constructor() {
    this.explorer = cosmiconfig("modular-agent", {
      searchPlaces: [
        "package.json",
        ".modular-agentrc",
        ".modular-agentrc.json",
        ".modular-agentrc.toml",
        ".modular-agentrc.ts",
        ".modular-agentrc.js",
        "modular-agent.config.js",
        "modular-agent.config.ts",
        "modular-agent.config.toml",
      ],
    });
  }

  /**
   * Load configuration.
   * @param configPath Optional configuration file path
   * @returns Configuration object
   */
  async load(configPath?: string): Promise<CLIConfig> {
    if (this.cachedConfig) {
      return this.cachedConfig;
    }

    try {
      let result;

      if (configPath) {
        // Load from specified path
        result = await this.explorer.load(configPath);
      } else {
        // Search for configuration file
        result = await this.explorer.search();
      }

      if (result?.config) {
        // Verify the configuration.
        const validatedConfig = ConfigSchema.parse(result.config);
        this.cachedConfig = { ...DEFAULT_CONFIG, ...validatedConfig };
        return this.cachedConfig;
      }
    } catch (error) {
      output.warnLog("Configuration loading failed; using default configuration:", {
        error: String(error),
      });
    }

    // Return the default configuration.
    this.cachedConfig = ConfigSchema.parse(DEFAULT_CONFIG);
    return this.cachedConfig;
  }

  /**
   * Clear cached configuration.
   */
  clearCache(): void {
    this.cachedConfig = null;
  }

  /**
   * Get a specific configuration item.
   */
  async get<K extends keyof CLIConfig>(key: K): Promise<CLIConfig[K]> {
    const config = await this.load();
    return config[key];
  }

  /**
   * Set a configuration item (in memory only).
   */
  set<K extends keyof CLIConfig>(key: K, value: CLIConfig[K]): void {
    if (!this.cachedConfig) {
      this.cachedConfig = { ...DEFAULT_CONFIG };
    }
    this.cachedConfig[key] = value;
  }

  /**
   * Merge configuration with environment variables
   * Environment variables have higher priority than configuration files
   */
  async loadWithEnvOverride(): Promise<CLIConfig> {
    const config = await this.load();

    // Override with environment variables
    if (process.env["CLI_VERBOSE"] === "true") {
      config.verbose = true;
    }
    if (process.env["CLI_DEBUG"] === "true") {
      config.debug = true;
    }
    if (process.env["CLI_LOG_LEVEL"]) {
      config.logLevel = process.env["CLI_LOG_LEVEL"] as any;
    }
    if (process.env["CLI_OUTPUT_FORMAT"]) {
      config.outputFormat = process.env["CLI_OUTPUT_FORMAT"] as any;
    }

    // Override output configuration
    if (process.env["LOG_DIR"]) {
      config.output = config.output || {};
      config.output.dir = process.env["LOG_DIR"];
    }
    if (process.env["DISABLE_LOG_TERMINAL"] === "true") {
      config.output = config.output || {};
      config.output.enableLogTerminal = false;
    }
    if (process.env["DISABLE_SDK_LOGS"] === "true") {
      config.output = config.output || {};
      config.output.enableSDKLogs = false;
    }
    if (process.env["SDK_LOG_LEVEL"]) {
      config.output = config.output || {};
      config.output.sdkLogLevel = process.env["SDK_LOG_LEVEL"] as any;
    }

    // Override storage configuration
    if (process.env["STORAGE_DIR"]) {
      config.storage = config.storage || {};
      config.storage.json = config.storage.json || {};
      config.storage.json.baseDir = process.env["STORAGE_DIR"];
    }

    return config;
  }
}

/**
 * Global configuration loader instance.
 */
let globalConfigLoader: ConfigLoader | null = null;

/**
 * Get the global configuration loader instance.
 */
export function getConfigLoader(): ConfigLoader {
  if (!globalConfigLoader) {
    globalConfigLoader = new ConfigLoader();
  }
  return globalConfigLoader;
}

/**
 * Convenience function for loading configuration.
 */
export async function loadConfig(configPath?: string): Promise<CLIConfig> {
  return getConfigLoader().load(configPath);
}

/**
 * Convenience function for loading configuration with environment variable override.
 */
export async function loadConfigWithEnvOverride(configPath?: string): Promise<CLIConfig> {
  return getConfigLoader().loadWithEnvOverride();
}
```

### 5.2 配置验证

```typescript
// apps/cli-app/src/config/config-validator.ts

import type { CLIConfig } from "./config-loader.js";
import { getOutput } from "../utils/output.js";

const output = getOutput();

/**
 * Configuration Validator
 */
export class ConfigValidator {
  /**
   * Validate configuration
   */
  static validate(config: CLIConfig): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    // Validate storage configuration
    if (config.storage) {
      if (config.storage.type === "json") {
        if (!config.storage.json) {
          errors.push("JSON storage configuration is required when storage type is 'json'");
        }
      } else if (config.storage.type === "sqlite") {
        if (!config.storage.sqlite) {
          errors.push("SQLite storage configuration is required when storage type is 'sqlite'");
        }
      }
    }

    // Validate output configuration
    if (config.output) {
      if (config.output.sdkLogLevel === "silent" && config.output.enableSDKLogs) {
        output.warnLog("SDK logs are enabled but log level is 'silent', no logs will be output");
      }
    }

    // Validate presets configuration
    if (config.presets) {
      if (config.presets.predefinedTools) {
        const { allowList, blockList } = config.presets.predefinedTools;
        if (allowList && blockList) {
          const intersection = allowList.filter(item => blockList.includes(item));
          if (intersection.length > 0) {
            errors.push(
              `Predefined tools allowList and blockList have intersection: ${intersection.join(", ")}`,
            );
          }
        }
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Validate configuration and throw error if invalid
   */
  static validateOrThrow(config: CLIConfig): void {
    const { valid, errors } = this.validate(config);
    if (!valid) {
      throw new Error(`Configuration validation failed:\n${errors.join("\n")}`);
    }
  }
}
```

## 6. 配置管理 API

### 6.1 配置访问 API

```typescript
// apps/cli-app/src/config/config-manager.ts

import type { CLIConfig } from "./config-loader.js";

/**
 * Configuration Manager
 * Provides convenient configuration access API
 */
export class ConfigManager {
  private config: CLIConfig;

  constructor(config: CLIConfig) {
    this.config = config;
  }

  /**
   * Get storage configuration
   */
  getStorageConfig() {
    return this.config.storage;
  }

  /**
   * Get output configuration
   */
  getOutputConfig() {
    return this.config.output;
  }

  /**
   * Get presets configuration
   */
  getPresetsConfig() {
    return this.config.presets;
  }

  /**
   * Get verbose mode
   */
  isVerbose(): boolean {
    return this.config.verbose;
  }

  /**
   * Get debug mode
   */
  isDebug(): boolean {
    return this.config.debug;
  }

  /**
   * Get log level
   */
  getLogLevel(): string {
    return this.config.logLevel;
  }

  /**
   * Get output format
   */
  getOutputFormat(): string {
    return this.config.outputFormat;
  }

  /**
   * Get default timeout
   */
  getDefaultTimeout(): number {
    return this.config.defaultTimeout;
  }

  /**
   * Get max concurrent threads
   */
  getMaxConcurrentThreads(): number {
    return this.config.maxConcurrentThreads;
  }

  /**
   * Update configuration
   */
  update(partialConfig: Partial<CLIConfig>): void {
    this.config = { ...this.config, ...partialConfig };
  }

  /**
   * Get complete configuration
   */
  getConfig(): CLIConfig {
    return this.config;
  }
}

/**
 * Global configuration manager instance
 */
let globalConfigManager: ConfigManager | null = null;

/**
 * Get the global configuration manager instance
 */
export function getConfigManager(): ConfigManager | null {
  return globalConfigManager;
}

/**
 * Initialize the global configuration manager
 */
export function initializeConfigManager(config: CLIConfig): ConfigManager {
  globalConfigManager = new ConfigManager(config);
  return globalConfigManager;
}
```

## 7. 测试配置支持

### 7.1 测试配置加载器

```typescript
// apps/cli-app/__tests__/utils/test-config-loader.ts

import { loadConfigWithEnvOverride } from "../../src/config/config-loader.js";
import { resolve } from "path";

/**
 * Load test configuration
 * @param configPath Optional configuration file path
 * @returns Configuration object
 */
export function loadTestConfig(configPath?: string) {
  const defaultTestConfigPath = resolve(__dirname, "../config/test-config.toml");
  return loadConfigWithEnvOverride(configPath ?? defaultTestConfigPath);
}

/**
 * Get the default test configuration path
 */
export function getDefaultTestConfigPath() {
  return resolve(__dirname, "../config/test-config.toml");
}

/**
 * Create test configuration with custom overrides
 * @param overrides Configuration overrides
 * @returns Configuration object
 */
export function createTestConfig(overrides: Record<string, any> = {}) {
  const config = loadTestConfig();

  // Apply overrides
  for (const [key, value] of Object.entries(overrides)) {
    const keys = key.split(".");
    let current: any = config;

    for (let i = 0; i < keys.length - 1; i++) {
      if (!current[keys[i]]) {
        current[keys[i]] = {};
      }
      current = current[keys[i]];
    }

    current[keys[keys.length - 1]] = value;
  }

  return config;
}
```

### 7.2 测试配置文件

```toml
# apps/cli-app/__tests__/config/test-config.toml

# Storage configuration for testing
[storage]
type = "json"

[storage.json]
baseDir = "./__tests__/storage"
enableFileLock = false

[storage.json.compression]
enabled = false

# Output configuration for testing
[output]
dir = "./__tests__/outputs"
logFilePattern = "cli-app-{date}.log"
enableLogTerminal = false
enableSDKLogs = false
sdkLogLevel = "silent"

# Presets configuration for testing
[presets.contextCompression]
enabled = true

[presets.predefinedTools]
enabled = true

[presets.predefinedPrompts]
enabled = true
```

### 7.3 更新测试运行器

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

  // ... rest of the implementation ...
}
```

### 7.4 更新测试用例

```typescript
// apps/cli-app/__tests__/integration/workflows/01-registration.test.ts

import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from "vitest";
import { CLIRunner, TestHelper, createTestHelper, TestLogger } from "../../utils";
import { createWorkflowTestHelper, WorkflowTestHelper } from "../../helpers/workflow-test-helpers";
import { resolve } from "path";
import { getDefaultTestConfigPath } from "../../utils/test-config-loader";

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

  afterAll(() => {
    const summary = logger.getSummary();
    console.log("\nWorkflow Registration Test Summary:", summary);
  });

  beforeEach(() => {
    helper = createTestHelper("workflow-registration", testOutputDir);
    workflowHelper = createWorkflowTestHelper(helper);
  });

  afterEach(async () => {
    await helper.cleanup();
  });

  // ... rest of the test code ...
});
```

## 8. 配置热重载

### 8.1 配置监听器

```typescript
// apps/cli-app/src/config/config-watcher.ts

import { watch } from "chokidar";
import { getOutput } from "../utils/output.js";

const output = getOutput();

/**
 * Configuration Watcher
 * Watch for configuration file changes and reload configuration
 */
export class ConfigWatcher {
  private watcher: ReturnType<typeof watch> | null = null;
  private configPath: string;
  private callback: () => void;

  constructor(configPath: string, callback: () => void) {
    this.configPath = configPath;
    this.callback = callback;
  }

  /**
   * Start watching configuration file
   */
  start() {
    if (this.watcher) {
      output.warnLog("ConfigWatcher already started");
      return;
    }

    this.watcher = watch(this.configPath).on("change", () => {
      output.infoLog("Configuration file changed, reloading...");
      this.callback();
    });

    output.infoLog("ConfigWatcher started", { configPath: this.configPath });
  }

  /**
   * Stop watching configuration file
   */
  stop() {
    if (this.watcher) {
      this.watcher.close();
      this.watcher = null;
      output.infoLog("ConfigWatcher stopped");
    }
  }
}
```

### 8.2 集成到 CLI 主入口

```typescript
// apps/cli-app/src/index.ts

import { ConfigWatcher } from "./config/config-watcher.js";

// ... 在 preAction hook 中 ...

program.hook("preAction", async thisCommand => {
  // Load configuration
  const configPath = process.env["CLI_CONFIG_PATH"];
  const config = await loadConfigWithEnvOverride(configPath);

  // Validate configuration
  ConfigValidator.validateOrThrow(config);

  // Initialize configuration manager
  initializeConfigManager(config);

  // Start configuration watcher (optional)
  if (config.debug) {
    const watcher = new ConfigWatcher(configPath || ".modular-agentrc.toml", async () => {
      const newConfig = await loadConfigWithEnvOverride(configPath);
      ConfigValidator.validateOrThrow(newConfig);
      initializeConfigManager(newConfig);
      output.infoLog("Configuration reloaded successfully");
    });
    watcher.start();
  }

  // ... rest of the initialization ...
});
```

## 9. 迁移计划

### 9.1 阶段一：配置系统扩展（Week 1）

1. **配置 Schema 扩展**
   - 扩展 `CLIConfig` Schema，添加存储和输出配置
   - 添加压缩配置
   - 添加预设配置

2. **配置加载器更新**
   - 实现 `loadWithEnvOverride` 方法
   - 添加配置验证逻辑
   - 添加配置管理器

3. **配置文件创建**
   - 创建开发环境配置文件
   - 创建测试环境配置文件
   - 创建生产环境配置文件

### 9.2 阶段二：测试配置支持（Week 2）

1. **测试配置加载器**
   - 实现 `test-config-loader.ts`
   - 创建测试配置文件
   - 实现配置覆盖功能

2. **测试运行器更新**
   - 更新 `CLIRunner` 以支持配置文件
   - 移除环境变量依赖
   - 更新测试用例

3. **测试执行**
   - 运行所有集成测试
   - 修复发现的问题
   - 验证配置隔离

### 9.3 阶段三：优化和完善（Week 3）

1. **配置热重载**
   - 实现配置监听器
   - 集成到 CLI 主入口
   - 测试热重载功能

2. **文档完善**
   - 更新用户文档
   - 添加配置示例
   - 编写配置指南

3. **错误处理**
   - 完善错误处理逻辑
   - 添加详细的错误日志
   - 提供友好的错误提示

## 10. 风险和挑战

### 10.1 技术风险

1. **配置验证**：复杂的配置验证逻辑可能导致性能问题。

2. **配置热重载**：配置热重载可能导致状态不一致。

3. **向后兼容**：需要确保现有用户的工作流不受影响。

### 10.2 兼容性风险

1. **环境变量**：需要确保环境变量仍然有效。

2. **配置文件格式**：需要支持多种配置文件格式。

### 10.3 测试风险

1. **测试隔离**：需要确保测试之间的配置隔离。

2. **测试清理**：需要确保测试后的配置清理。

## 11. 总结

本设计文档详细描述了 CLI 应用的配置功能设计，包括配置文件格式、配置加载机制、配置验证和配置管理等方面的详细设计。通过扩展配置系统、实现配置加载器、添加配置验证和配置管理器，我们可以：

1. **简化测试配置**：通过配置文件，避免设置多个环境变量，提高测试的可维护性。

2. **提高灵活性**：用户可以通过配置文件自定义存储路径、输出路径等关键配置。

3. **改善用户体验**：通过友好的配置文件和详细的文档，降低用户的使用门槛。

4. **支持多环境**：通过配置文件和环境变量，支持开发、测试、生产等多种环境。

5. **提高可维护性**：通过统一的配置管理，降低代码复杂度，提高可维护性。
