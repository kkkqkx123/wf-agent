/**
 * Runtime Storage Manager
 * Unified management of all storage instances, shared across applications.
 *
 * Extracted from apps/cli-app/src/storage/storage-manager.ts to eliminate
 * duplication between cli-app and server.
 */

import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
  TaskStorageAdapter,
  AgentLoopStorageAdapter,
  TriggerStorageAdapter,
  ToolStorageAdapter,
  ScriptStorageAdapter,
  NodeTemplateStorageAdapter,
  HookTemplateStorageAdapter,
  AgentProfileStorageAdapter,
  MetricsStorageAdapter,
} from "@wf-agent/storage";
import type { SDKOptions } from "@wf-agent/sdk/api";
import {
  // SQLite adapters
  SqliteCheckpointStorage,
  SqliteWorkflowStorage,
  SqliteWorkflowExecutionStorage,
  SqliteTaskStorage,
  SqliteAgentLoopStorage,
  SqliteTriggerStorage,
  SqliteToolStorage,
  SqliteScriptStorage,
  SqliteNodeTemplateStorage,
  SqliteHookTemplateStorage,
  SqliteAgentProfileStorage,
  SqliteMetricsStorage,
  type BaseSqliteStorageConfig,
  configurePragmas,
  // PostgreSQL adapters
  PostgresCheckpointStorage,
  PostgresWorkflowStorage,
  PostgresWorkflowExecutionStorage,
  PostgresTaskStorage,
  PostgresAgentLoopStorage,
  PostgresTriggerStorage,
  PostgresToolStorage,
  PostgresScriptStorage,
  PostgresNodeTemplateStorage,
  PostgresHookTemplateStorage,
  PostgresAgentProfileStorage,
  PostgresMetricsStorage,
  type BasePostgresStorageConfig,
  getPostgresGlobalConnectionPool,
  // Memory adapters
  MemoryCheckpointStorage,
  MemoryWorkflowStorage,
  MemoryWorkflowExecutionStorage,
  MemoryTaskStorage,
  MemoryAgentLoopStorage,
  MemoryTriggerStorage,
  MemoryToolStorage,
  MemoryScriptStorage,
  MemoryNodeTemplateStorage,
  MemoryHookTemplateStorage,
  MemoryAgentProfileStorage,
  MemoryMetricsStorage,
} from "@wf-agent/storage";
import Database from "better-sqlite3";
import { createPackageLogger, registerLogger, createLazyLogger } from "@wf-agent/common-utils";
import type { RuntimeStorageConfig } from "../config/types.js";

const logger = createLazyLogger("runtime:storage-manager", () =>
  createPackageLogger("runtime").child("storage-manager")
);
registerLogger("runtime.storage-manager", logger);

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
  private metricsStorage: MetricsStorageAdapter | null = null;
  private initialized: boolean = false;
  /** Shared SQLite connection injected into all storage instances */
  private sharedDb: Database.Database | null = null;

  constructor(private config: RuntimeStorageConfig) {}

  async initialize(): Promise<void> {
    if (this.initialized) {
      logger.warn("StorageManager already initialized");
      return;
    }

    const storageConfig = this.config.storage;
    if (!storageConfig) {
      logger.info("No storage configuration found, skipping storage initialization");
      this.initialized = true;
      return;
    }

    if (storageConfig.type === "sqlite") {
      await this.initializeSQLiteStorage(storageConfig.sqlite);
    } else if (storageConfig.type === "memory") {
      await this.initializeMemoryStorage();
    } else if (storageConfig.type === "postgres") {
      const pgConfig = storageConfig.postgres;
      if (!pgConfig) {
        throw new Error("PostgreSQL storage config is missing. Set storage.postgres in config.");
      }
      // Convert user-facing PostgresStorageConfig to internal BasePostgresStorageConfig
      await this.initializePostgresStorage({
        connectionString: `postgresql://${encodeURIComponent(pgConfig.username)}:${encodeURIComponent(pgConfig.password)}@${pgConfig.host}:${pgConfig.port ?? 5432}/${pgConfig.database}`,
        poolConfig: {
          max: pgConfig.poolSize,
          min: pgConfig.minConnections,
          idleTimeoutMillis: pgConfig.idleTimeout,
          connectionTimeoutMillis: pgConfig.connectionTimeout,
          maxUses: pgConfig.maxUses,
        },
      });
    } else {
      throw new Error(
        `Unsupported storage type: "${(storageConfig as unknown as Record<string, unknown>)["type"]}". ` +
        `Currently only "sqlite", "postgres", and "memory" are supported.`
      );
    }

    this.initialized = true;
    logger.info("StorageManager initialized successfully");
  }

  private async initializeSQLiteStorage(config?: BaseSqliteStorageConfig): Promise<void> {
    const appName = this.config.appName ?? "app";
    const dbPath = config?.dbPath ?? `./storage/${appName}.db`;

    const baseConfig: BaseSqliteStorageConfig = {
      ...config,
      dbPath,
    };

    // Create a single shared SQLite connection for all storage instances
    this.sharedDb = new Database(dbPath);
    configurePragmas(this.sharedDb, {
      autoVacuum: config?.autoVacuum,
      journalSizeLimit: config?.journalSizeLimit,
      synchronous: 'NORMAL',
      walAutocheckpoint: 1000,
    });

    // Apply additional pragmas that configurePragmas does not cover
    // (must match SqliteKeyValueStorageBase.createSchema defaults)
    this.sharedDb.pragma("cache_size = -64000"); // 64MB
    this.sharedDb.pragma("foreign_keys = ON");

    // Apply page size if configured (must be set before any tables are created)
    if (config?.pageSize) {
      this.sharedDb.pragma(`page_size = ${config.pageSize}`);
    }

    logger.info("Created shared SQLite connection", { dbPath });

    // Create all storage instances and inject the shared connection
    this.workflowStorage = new SqliteWorkflowStorage(baseConfig);
    (this.workflowStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.workflowStorage.initialize();

    this.workflowExecutionStorage = new SqliteWorkflowExecutionStorage(baseConfig);
    (this.workflowExecutionStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.workflowExecutionStorage.initialize();

    this.checkpointStorage = new SqliteCheckpointStorage(baseConfig);
    (this.checkpointStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.checkpointStorage.initialize();

    this.taskStorage = new SqliteTaskStorage(baseConfig);
    (this.taskStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.taskStorage.initialize();

    this.agentLoopStorage = new SqliteAgentLoopStorage(baseConfig);
    (this.agentLoopStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.agentLoopStorage.initialize();

    this.triggerStorage = new SqliteTriggerStorage(baseConfig);
    (this.triggerStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.triggerStorage.initialize();

    this.toolStorage = new SqliteToolStorage(baseConfig);
    (this.toolStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.toolStorage.initialize();

    this.scriptStorage = new SqliteScriptStorage(baseConfig);
    (this.scriptStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.scriptStorage.initialize();

    this.nodeTemplateStorage = new SqliteNodeTemplateStorage(baseConfig);
    (this.nodeTemplateStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.nodeTemplateStorage.initialize();

    this.hookTemplateStorage = new SqliteHookTemplateStorage(baseConfig);
    (this.hookTemplateStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.hookTemplateStorage.initialize();

    this.agentProfileStorage = new SqliteAgentProfileStorage(baseConfig);
    (this.agentProfileStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.agentProfileStorage.initialize();

    this.metricsStorage = new SqliteMetricsStorage(baseConfig);
    (this.metricsStorage as unknown as { setExternalDb: (db: Database.Database) => void }).setExternalDb(this.sharedDb);
    await this.metricsStorage.initialize();

    logger.info("SQLite storage initialized with shared connection", { dbPath });
  }

  private async initializeMemoryStorage(): Promise<void> {
    logger.info("Storage type is 'memory', initializing in-memory storage adapters");

    this.workflowStorage = new MemoryWorkflowStorage();
    await this.workflowStorage.initialize();

    this.workflowExecutionStorage = new MemoryWorkflowExecutionStorage();
    await this.workflowExecutionStorage.initialize();

    this.checkpointStorage = new MemoryCheckpointStorage();
    await this.checkpointStorage.initialize();

    this.taskStorage = new MemoryTaskStorage();
    await this.taskStorage.initialize();

    this.agentLoopStorage = new MemoryAgentLoopStorage();
    await this.agentLoopStorage.initialize();

    this.triggerStorage = new MemoryTriggerStorage();
    await this.triggerStorage.initialize();

    this.toolStorage = new MemoryToolStorage();
    await this.toolStorage.initialize();

    this.scriptStorage = new MemoryScriptStorage();
    await this.scriptStorage.initialize();

    this.nodeTemplateStorage = new MemoryNodeTemplateStorage();
    await this.nodeTemplateStorage.initialize();

    this.hookTemplateStorage = new MemoryHookTemplateStorage();
    await this.hookTemplateStorage.initialize();

    this.agentProfileStorage = new MemoryAgentProfileStorage();
    await this.agentProfileStorage.initialize();

    this.metricsStorage = new MemoryMetricsStorage();
    await this.metricsStorage.initialize();

    logger.info("Memory storage initialized with all adapters");
  }

  private async initializePostgresStorage(config?: BasePostgresStorageConfig): Promise<void> {
    const connectionString = config?.connectionString ?? "";
    if (!connectionString) {
      throw new Error("PostgreSQL connection string is required. Set storage.postgres.connectionString in config.");
    }

    const baseConfig: BasePostgresStorageConfig = {
      ...config,
      connectionString,
    };

    // Use the global connection pool by default
    if (!baseConfig.connectionPool) {
      baseConfig.connectionPool = getPostgresGlobalConnectionPool();
    }

    logger.info("Initializing PostgreSQL storage", { connectionString });

    this.workflowStorage = new PostgresWorkflowStorage(baseConfig);
    await this.workflowStorage.initialize();

    this.workflowExecutionStorage = new PostgresWorkflowExecutionStorage(baseConfig);
    await this.workflowExecutionStorage.initialize();

    this.checkpointStorage = new PostgresCheckpointStorage(baseConfig);
    await this.checkpointStorage.initialize();

    this.taskStorage = new PostgresTaskStorage(baseConfig);
    await this.taskStorage.initialize();

    this.agentLoopStorage = new PostgresAgentLoopStorage(baseConfig);
    await this.agentLoopStorage.initialize();

    this.triggerStorage = new PostgresTriggerStorage(baseConfig);
    await this.triggerStorage.initialize();

    this.toolStorage = new PostgresToolStorage(baseConfig);
    await this.toolStorage.initialize();

    this.scriptStorage = new PostgresScriptStorage(baseConfig);
    await this.scriptStorage.initialize();

    this.nodeTemplateStorage = new PostgresNodeTemplateStorage(baseConfig);
    await this.nodeTemplateStorage.initialize();

    this.hookTemplateStorage = new PostgresHookTemplateStorage(baseConfig);
    await this.hookTemplateStorage.initialize();

    this.agentProfileStorage = new PostgresAgentProfileStorage(baseConfig);
    await this.agentProfileStorage.initialize();

    this.metricsStorage = new PostgresMetricsStorage(baseConfig);
    await this.metricsStorage.initialize();

    logger.info("PostgreSQL storage initialized with all adapters", { connectionString });
  }

  getWorkflowStorage(): WorkflowStorageAdapter | null {
    return this.workflowStorage;
  }

  getWorkflowExecutionStorage(): WorkflowExecutionStorageAdapter | null {
    return this.workflowExecutionStorage;
  }

  getCheckpointStorage(): CheckpointStorageAdapter | null {
    return this.checkpointStorage;
  }

  getTaskStorage(): TaskStorageAdapter | null {
    return this.taskStorage;
  }

  getAgentLoopStorage(): AgentLoopStorageAdapter | null {
    return this.agentLoopStorage;
  }

  getTriggerStorage(): TriggerStorageAdapter | null {
    return this.triggerStorage;
  }

  getToolStorage(): ToolStorageAdapter | null {
    return this.toolStorage;
  }

  getScriptStorage(): ScriptStorageAdapter | null {
    return this.scriptStorage;
  }

  getNodeTemplateStorage(): NodeTemplateStorageAdapter | null {
    return this.nodeTemplateStorage;
  }

  getHookTemplateStorage(): HookTemplateStorageAdapter | null {
    return this.hookTemplateStorage;
  }

  getAgentProfileStorage(): AgentProfileStorageAdapter | null {
    return this.agentProfileStorage;
  }

  /**
   * Get all storage adapters as an SDKOptions-compatible object.
   * Convenience method to simplify createSDK() call.
   */
  getAllAdapters(): Pick<
    SDKOptions,
    | "checkpointStorageAdapter"
    | "workflowStorageAdapter"
    | "workflowExecutionStorageAdapter"
    | "taskStorageAdapter"
    | "agentLoopCheckpointStorageAdapter"
    | "triggerStorageAdapter"
    | "toolStorageAdapter"
    | "scriptStorageAdapter"
    | "nodeTemplateStorageAdapter"
    | "hookTemplateStorageAdapter"
    | "agentProfileStorageAdapter"
    | "metricsStorageAdapter"
  > {
    return {
      checkpointStorageAdapter: this.checkpointStorage ?? undefined,
      workflowStorageAdapter: this.workflowStorage ?? undefined,
      workflowExecutionStorageAdapter: this.workflowExecutionStorage ?? undefined,
      taskStorageAdapter: this.taskStorage ?? undefined,
      agentLoopCheckpointStorageAdapter: this.agentLoopStorage ?? undefined,
      triggerStorageAdapter: this.triggerStorage ?? undefined,
      toolStorageAdapter: this.toolStorage ?? undefined,
      scriptStorageAdapter: this.scriptStorage ?? undefined,
      nodeTemplateStorageAdapter: this.nodeTemplateStorage ?? undefined,
      hookTemplateStorageAdapter: this.hookTemplateStorage ?? undefined,
      agentProfileStorageAdapter: this.agentProfileStorage ?? undefined,
      metricsStorageAdapter: this.metricsStorage ?? undefined,
    };
  }

  async close(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    // Close all individual storage instances (they will skip closing the shared connection)
    const results = await Promise.allSettled([
      this.workflowStorage?.close(),
      this.workflowExecutionStorage?.close(),
      this.checkpointStorage?.close(),
      this.taskStorage?.close(),
      this.agentLoopStorage?.close(),
      this.triggerStorage?.close(),
      this.toolStorage?.close(),
      this.scriptStorage?.close(),
      this.nodeTemplateStorage?.close(),
      this.hookTemplateStorage?.close(),
      this.agentProfileStorage?.close(),
    ]);

    for (const result of results) {
      if (result.status === "rejected") {
        logger.error("Storage close error", { error: result.reason });
      }
    }

    // Close the shared connection last
    if (this.sharedDb) {
      try {
        this.sharedDb.close();
        logger.info("Shared SQLite connection closed");
      } catch (error) {
        logger.error("Error closing shared SQLite connection", { error: (error as Error).message });
      } finally {
        this.sharedDb = null;
      }
    }

    this.initialized = false;
    logger.info("StorageManager closed");
  }

  async clear(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    const results = await Promise.allSettled([
      this.workflowStorage?.clear(),
      this.workflowExecutionStorage?.clear(),
      this.checkpointStorage?.clear(),
      this.taskStorage?.clear(),
      this.agentLoopStorage?.clear(),
      this.triggerStorage?.clear(),
      this.toolStorage?.clear(),
      this.scriptStorage?.clear(),
      this.nodeTemplateStorage?.clear(),
      this.hookTemplateStorage?.clear(),
      this.agentProfileStorage?.clear(),
    ]);

    for (const result of results) {
      if (result.status === "rejected") {
        logger.error("Storage clear error", { error: result.reason });
      }
    }

    logger.info("StorageManager cleared");
  }
}