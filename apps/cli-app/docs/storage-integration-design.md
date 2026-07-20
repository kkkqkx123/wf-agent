# CLI App 存储集成设计文档

## 1. 概述

本文档描述了 CLI 应用如何通过 `@wf-agent/runtime` 的 `StorageManager` 集成 `@wf-agent/storage` 包的 SQLite 存储实现，以实现工作流、工作流执行实例、检查点等数据的持久化存储。

## 2. 当前问题分析

### 2.1 存在的问题

1. **数据丢失**：当 CLI 应用使用 `memory` 存储类型时，退出后所有注册的工作流、工作流执行实例等数据都会丢失。

2. **测试复杂度高**：测试模式需要设置多个环境变量（`TEST_MODE`、`LOG_DIR`、`DISABLE_LOG_TERMINAL`、`DISABLE_SDK_LOGS`、`SDK_LOG_LEVEL`），使用繁琐。

3. **缺乏配置管理**：存储路径、输出路径等配置硬编码或依赖环境变量，缺乏统一的配置管理机制。

### 2.2 根本原因

- SDK 默认使用内存存储（`WorkflowRegistry`、`WorkflowExecutionRegistry` 等），没有集成持久化存储。
- CLI 应用没有通过 `@wf-agent/runtime` 的 `StorageManager` 注入 SQLite 存储适配器。
- 配置系统只支持应用级配置（如 API URL、超时等），不支持存储相关配置。

## 3. 存储架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI App                              │
├─────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────┐  │
│  │              Configuration Layer                      │  │
│  │  - CLIConfig (extends AppConfig)                      │  │
│  │  - StorageConfig (from @wf-agent/types)               │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              @wf-agent/runtime Bootstrap              │  │
│  │  - createAppSDK()                                     │  │
│  │  - StorageManager (main integration point)            │  │
│  │  - getAllAdapters() → SDK adapter injection           │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              @wf-agent/storage Package                │  │
│  │  - SqliteWorkflowStorage                              │  │
│  │  - SqliteWorkflowExecutionStorage                     │  │
│  │  - SqliteCheckpointStorage                            │  │
│  │  - SqliteTaskStorage                                  │  │
│  │  - SqliteAgentLoopStorage                             │  │
│  │  - SqliteTriggerStorage                               │  │
│  │  - SqliteToolStorage                                  │  │
│  │  - SqliteScriptStorage                                │  │
│  │  - SqliteNodeTemplateStorage                          │  │
│  │  - SqliteHookTemplateStorage                          │  │
│  │  - SqliteAgentProfileStorage                          │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              SQLite Database                           │  │
│  │  storage/cli-app.db (single file)                     │  │
│  │  - WAL mode for concurrent read performance           │  │
│  │  - INCREMENTAL auto-vacuum for space management       │  │
│  │  - Journal size limit (64MB) to prevent unbounded WAL │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 存储类型映射

| SDK 存储接口 | SQLite 存储实现 | 存储的数据 |
|--------------|----------------|-----------|
| `WorkflowStorageAdapter` | `SqliteWorkflowStorage` | 工作流定义、版本历史 |
| `WorkflowExecutionStorageAdapter` | `SqliteWorkflowExecutionStorage` | 工作流执行实例状态、消息历史 |
| `CheckpointStorageAdapter` | `SqliteCheckpointStorage` | 检查点状态快照 |
| `TaskStorageAdapter` | `SqliteTaskStorage` | 任务执行状态 |
| `AgentLoopStorageAdapter` | `SqliteAgentLoopStorage` | Agent 循环检查点 |
| `TriggerStorageAdapter` | `SqliteTriggerStorage` | 触发器定义和状态 |
| `ToolStorageAdapter` | `SqliteToolStorage` | 工具注册信息 |
| `ScriptStorageAdapter` | `SqliteScriptStorage` | 脚本定义 |
| `NodeTemplateStorageAdapter` | `SqliteNodeTemplateStorage` | 节点模板 |
| `HookTemplateStorageAdapter` | `SqliteHookTemplateStorage` | 钩子模板 |
| `AgentProfileStorageAdapter` | `SqliteAgentProfileStorage` | Agent 配置信息 |

## 4. 配置设计

### 4.1 配置 Schema

存储配置使用 `@wf-agent/types` 包中定义的共享 Schema：

```typescript
// packages/types/src/config/schemas.ts

export const StorageConfigSchema = z.object({
  type: StorageTypeSchema,  // "sqlite" | "json" | "postgres" | "memory"
  sqlite: SqliteStorageConfigSchema.optional(),
  postgres: z.object({ ... }).optional(),
});
```

### 4.2 默认配置

```typescript
// apps/cli-app/src/config/cli/defaults.ts

export const DEFAULT_CONFIG: CLIConfig = {
  // ... base defaults ...
  storage: {
    type: "sqlite",
    sqlite: {
      dbPath: "./storage/cli-app.db",
      enableWAL: true,
      enableLogging: false,
      readonly: false,
      fileMustExist: false,
      timeout: 5000,
      autoVacuum: 'INCREMENTAL',
      journalSizeLimit: 67108864,
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
# .modular-agent.toml

[storage]
type = "sqlite"

[storage.sqlite]
dbPath = "./storage/cli-app.db"
enableWAL = true
enableLogging = false
timeout = 5000

[output]
dir = "./outputs"
logFilePattern = "cli-app-{date}.log"
enableLogTerminal = true
enableSDKLogs = true
sdkLogLevel = "silent"
```

## 5. 实现设计

### 5.1 StorageManager (位于 @wf-agent/runtime)

`StorageManager` 位于 `@wf-agent/runtime` 包中，是一个共享的存储管理器，被 cli-app 和 server 共同使用。

```typescript
// packages/runtime/src/storage/storage-manager.ts

export class StorageManager {
  private workflowStorage: WorkflowStorageAdapter | null = null;
  private workflowExecutionStorage: WorkflowExecutionStorageAdapter | null = null;
  private checkpointStorage: CheckpointStorageAdapter | null = null;
  private taskStorage: TaskStorageAdapter | null = null;
  private agentLoopStorage: AgentLoopStorageAdapter | null = null;
  private triggerStorage: TriggerStorageAdapter | null = null;
  private toolStorage: ToolStorageAdapter | null = null;
  private scriptStorage: ScriptStorageAdapter | null = null;
  private nodeTemplateStorage: NodeTemplateStorageAdapter | null = null;
  private hookTemplateStorage: HookTemplateStorageAdapter | null = null;
  private agentProfileStorage: AgentProfileStorageAdapter | null = null;
  private initialized: boolean = false;

  constructor(private config: RuntimeStorageConfig) {}

  async initialize(): Promise<void> {
    // 支持 sqlite 和 memory 两种类型
    // sqlite → 初始化 11 个 SQLite 存储实例
    // memory → 跳过初始化（SDK 使用默认内存存储）
  }

  /**
   * 返回所有存储适配器，用于注入到 createSDK()
   */
  getAllAdapters(): Pick<SDKOptions, ...> {
    return {
      checkpointStorageAdapter: this.checkpointStorage ?? undefined,
      workflowStorageAdapter: this.workflowStorage ?? undefined,
      // ...
    };
  }
}
```

### 5.2 集成到 CLI 主入口

CLI 应用通过 `@wf-agent/runtime/bootstrap` 的 `createAppSDK()` 统一初始化：

```typescript
// apps/cli-app/src/index.ts

import { createAppSDK } from "@wf-agent/runtime/bootstrap";

const { sdk, storageManager } = await createAppSDK({
  appName: "cli-app",
  storage: {
    storage: config.storage,  // 来自配置文件
    appName: "cli-app",
  },
  // ... 其他配置 ...
  hooks: {
    onBootstrapStart: () => { /* ... */ },
    onBootstrapComplete: () => { /* ... */ },
    onBootstrapError: (error) => { /* ... */ },
  },
});

// SDK 的 onDestroy hook 自动关闭 StorageManager
// shutdown() 中调用 sdk.destroy() 触发 storageManager.close()
```

### 5.3 关闭流程

```
shutdown()
  └─ sdk.destroy()
       └─ onDestroy hook (set by createAppSDK)
            └─ storageManager.close()
                 └─ Promise.allSettled(11 × storage.close())
  └─ storageManager.close() (安全网，防止 SDK destroy 未覆盖)
  └─ container.cleanup()
  └─ output.close()
```

`StorageManager.close()` 使用 `Promise.allSettled` 确保所有存储实例都被关闭，即使个别关闭失败也不会阻塞整体流程。

## 6. 测试设计

### 6.1 测试配置文件

测试环境使用 `sqlite` 存储类型，输出目录和数据库路径指向测试目录：

```toml
# apps/cli-app/__tests__/config/test-config.toml

[storage]
type = "sqlite"

[storage.sqlite]
dbPath = "./__tests__/storage/cli-app.db"
enableWAL = true
enableLogging = false
timeout = 5000

[output]
dir = "./__tests__/outputs"
logFilePattern = "cli-app-{date}.log"
enableLogTerminal = false
enableSDKLogs = false
sdkLogLevel = "silent"
```

### 6.2 测试运行器

`CLIRunner` 通过子进程方式运行 CLI，每个测试用例独立运行，数据通过 SQLite 文件持久化：

```typescript
export class CLIRunner {
  constructor(cliPath?: string, outputDir?: string, configPath?: string) {
    this.defaultEnv = {
      ...process.env,
      NODE_ENV: "test",
      TEST_MODE: "true",
      CLI_CONFIG_PATH: configPath || resolve(__dirname, "../config/test-config.toml"),
    };
  }
}
```

## 7. 关键设计决策

### 7.1 为什么选择 SQLite 而不是 JSON

| 维度 | SQLite | JSON 文件 |
|------|--------|-----------|
| 查询能力 | 支持复杂查询、过滤、排序 | 需要全量加载后过滤 |
| 并发控制 | 内置 WAL 模式，读写并发 | 需要额外文件锁 |
| 数据一致性 | 事务支持，原子提交 | 多文件写入非原子 |
| 性能 | 索引优化，单文件查询快 | 大量小文件，IO 开销大 |
| 维护 | 单文件，易备份/迁移 | 多目录多文件，管理复杂 |

### 7.2 为什么 StorageManager 放在 @wf-agent/runtime

- **消除代码重复**：cli-app 和 server 使用相同的存储初始化逻辑
- **统一管理**：共享的 11 个存储适配器初始化
- **一致的生命周期**：统一的 `initialize()` / `close()` / `clear()` 接口

## 8. 风险和挑战

### 8.1 技术风险

1. **存储性能**：SQLite 单文件在大量写入时可能存在锁竞争，WAL 模式缓解了读并发但写仍然是串行的。

2. **数据文件膨胀**：WAL 文件可能无限增长，通过 `journalSizeLimit` 和 `autoVacuum` 控制。

3. **并发控制**：多进程访问同一数据库文件可能导致 `SQLITE_BUSY`，通过 `timeout` 配置缓解。

### 8.2 兼容性风险

1. **向后兼容**：从 `memory` 切换到 `sqlite` 后，之前的数据不可见（但未丢失，仍在内存中）。

2. **Schema 变更**：SQLite 表结构变更需要迁移策略，当前通过 `better-sqlite3` 的 `CREATE TABLE IF NOT EXISTS` 处理。

### 8.3 测试风险

1. **测试隔离**：每个测试子进程使用独立的 SQLite 数据库文件，确保数据隔离。

2. **测试清理**：测试后需要清理测试数据库文件和输出目录。

## 9. 总结

本设计文档描述了 CLI 应用如何通过 `@wf-agent/runtime` 的 `StorageManager` 集成 `@wf-agent/storage` 包的 SQLite 存储实现，以实现工作流、执行实例、检查点等数据的持久化存储。通过 `createAppSDK()` 统一初始化流程：

1. **解决数据丢失问题**：通过 SQLite 持久化存储，确保数据在应用退出后仍然存在。

2. **简化配置管理**：通过 `@wf-agent/types` 的共享 Schema，提供统一的配置定义。

3. **提高可扩展性**：通过 `@wf-agent/runtime` 的 `StorageManager`，支持未来添加更多存储后端。

4. **改善用户体验**：默认启用 SQLite 持久化，用户无需额外配置即可获得持久化能力。