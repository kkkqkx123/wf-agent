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
} from "@wf-agent/storage";
import type { SDKOptions } from "@wf-agent/sdk/api";
import {
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
  type BaseSqliteStorageConfig,
} from "@wf-agent/storage";
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
  private initialized: boolean = false;

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
    } else {
      throw new Error(
        `Unsupported storage type: ${(storageConfig as unknown as Record<string, unknown>)["type"]}`
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

    this.workflowStorage = new SqliteWorkflowStorage(baseConfig);
    await this.workflowStorage.initialize();

    this.workflowExecutionStorage = new SqliteWorkflowExecutionStorage(baseConfig);
    await this.workflowExecutionStorage.initialize();

    this.checkpointStorage = new SqliteCheckpointStorage(baseConfig);
    await this.checkpointStorage.initialize();

    this.taskStorage = new SqliteTaskStorage(baseConfig);
    await this.taskStorage.initialize();

    this.agentLoopStorage = new SqliteAgentLoopStorage(baseConfig);
    await this.agentLoopStorage.initialize();

    this.triggerStorage = new SqliteTriggerStorage(baseConfig);
    await this.triggerStorage.initialize();

    this.toolStorage = new SqliteToolStorage(baseConfig);
    await this.toolStorage.initialize();

    this.scriptStorage = new SqliteScriptStorage(baseConfig);
    await this.scriptStorage.initialize();

    this.nodeTemplateStorage = new SqliteNodeTemplateStorage(baseConfig);
    await this.nodeTemplateStorage.initialize();

    this.hookTemplateStorage = new SqliteHookTemplateStorage(baseConfig);
    await this.hookTemplateStorage.initialize();

    this.agentProfileStorage = new SqliteAgentProfileStorage(baseConfig);
    await this.agentProfileStorage.initialize();

    logger.info("SQLite storage initialized", { dbPath });
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
    };
  }

  async close(): Promise<void> {
    if (!this.initialized) {
      return;
    }

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