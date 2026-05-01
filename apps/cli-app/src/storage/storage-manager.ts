/**
 * Storage Manager
 * Unified management of all storage instances
 */

import type {
  CheckpointStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
  TaskStorageAdapter,
  AgentLoopCheckpointStorageAdapter,
} from "@wf-agent/storage";
import {
  JsonCheckpointStorage,
  JsonWorkflowStorage,
  JsonWorkflowExecutionStorage,
  JsonTaskStorage,
  JsonNoteStorage,
  JsonAgentLoopCheckpointStorage,
  type BaseJsonStorageConfig,
  SqliteCheckpointStorage,
  SqliteWorkflowStorage,
  SqliteWorkflowExecutionStorage,
  SqliteTaskStorage,
  SqliteAgentLoopCheckpointStorage,
  type BaseSqliteStorageConfig,
} from "@wf-agent/storage";
import type { CLIConfig } from "../config/index.js";
import { createPackageLogger, registerLogger } from "@wf-agent/common-utils";

const logger = createPackageLogger("cli-app").child("storage-manager");
registerLogger("cli-app.storage-manager", logger);

/**
 * Storage Manager
 * Unified management of all storage instances
 */
export class StorageManager {
  private workflowStorage: WorkflowStorageAdapter | null = null;
  private workflowExecutionStorage: WorkflowExecutionStorageAdapter | null = null;
  private checkpointStorage: CheckpointStorageAdapter | null = null;
  private taskStorage: TaskStorageAdapter | null = null;
  private agentLoopCheckpointStorage: AgentLoopCheckpointStorageAdapter | null = null;
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
    if (!storageConfig) {
      logger.info("No storage configuration found, skipping storage initialization");
      this.initialized = true;
      return;
    }

    if (storageConfig.type === "json") {
      await this.initializeJsonStorage(storageConfig.json);
    } else if (storageConfig.type === "sqlite") {
      await this.initializeSQLiteStorage(storageConfig.sqlite);
    } else {
      throw new Error(`Unknown storage type: ${(storageConfig as any).type}`);
    }

    this.initialized = true;
    logger.info("StorageManager initialized successfully");
  }

  /**
   * Initialize JSON storage
   */
  private async initializeJsonStorage(
    config?: CLIConfig["storage"] extends { json?: infer T } | undefined ? T : never,
  ): Promise<void> {
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

    this.workflowStorage = new JsonWorkflowStorage(baseConfig);
    await this.workflowStorage.initialize();
    logger.info("WorkflowStorage initialized", { baseDir });

    this.workflowExecutionStorage = new JsonWorkflowExecutionStorage(baseConfig);
    await this.workflowExecutionStorage.initialize();
    logger.info("WorkflowExecutionStorage initialized", { baseDir });

    this.checkpointStorage = new JsonCheckpointStorage(baseConfig);
    await this.checkpointStorage.initialize();
    logger.info("CheckpointStorage initialized", { baseDir });

    this.taskStorage = new JsonTaskStorage(baseConfig);
    await this.taskStorage.initialize();
    logger.info("TaskStorage initialized", { baseDir });

    this.agentLoopCheckpointStorage = new JsonAgentLoopCheckpointStorage(baseConfig);
    await this.agentLoopCheckpointStorage.initialize();
    logger.info("AgentLoopCheckpointStorage initialized", { baseDir });
  }

  /**
   * Initialize SQLite storage
   */
  private async initializeSQLiteStorage(config?: {
    dbPath?: string;
    enableWAL?: boolean;
    enableLogging?: boolean;
    readonly?: boolean;
    fileMustExist?: boolean;
    timeout?: number;
  }): Promise<void> {
    const dbPath = config?.dbPath ?? "./storage/cli-app.db";
    const enableWAL = config?.enableWAL ?? true;
    const enableLogging = config?.enableLogging ?? false;
    const readonly = config?.readonly ?? false;
    const fileMustExist = config?.fileMustExist ?? false;
    const timeout = config?.timeout ?? 5000;

    const baseConfig: BaseSqliteStorageConfig = {
      dbPath,
      enableLogging,
      readonly,
      fileMustExist,
      timeout,
    };

    this.workflowStorage = new SqliteWorkflowStorage(baseConfig);
    await this.workflowStorage.initialize();
    logger.info("WorkflowStorage initialized", { dbPath, enableWAL });

    this.workflowExecutionStorage = new SqliteWorkflowExecutionStorage(baseConfig);
    await this.workflowExecutionStorage.initialize();
    logger.info("WorkflowExecutionStorage initialized", { dbPath, enableWAL });

    this.checkpointStorage = new SqliteCheckpointStorage(baseConfig);
    await this.checkpointStorage.initialize();
    logger.info("CheckpointStorage initialized", { dbPath, enableWAL });

    this.taskStorage = new SqliteTaskStorage(baseConfig);
    await this.taskStorage.initialize();
    logger.info("TaskStorage initialized", { dbPath, enableWAL });

    this.agentLoopCheckpointStorage = new SqliteAgentLoopCheckpointStorage(baseConfig);
    await this.agentLoopCheckpointStorage.initialize();
    logger.info("AgentLoopCheckpointStorage initialized", { dbPath, enableWAL });
  }

  /**
   * Get workflow storage
   */
  getWorkflowStorage(): WorkflowStorageAdapter | null {
    return this.workflowStorage;
  }

  /**
   * Get workflow execution storage
   */
  getWorkflowExecutionStorage(): WorkflowExecutionStorageAdapter | null {
    return this.workflowExecutionStorage;
  }

  /**
   * Get checkpoint storage
   */
  getCheckpointStorage(): CheckpointStorageAdapter | null {
    return this.checkpointStorage;
  }

  /**
   * Get task storage
   */
  getTaskStorage(): TaskStorageAdapter | null {
    return this.taskStorage;
  }

  /**
   * Get agent loop checkpoint storage
   */
  getAgentLoopCheckpointStorage(): AgentLoopCheckpointStorageAdapter | null {
    return this.agentLoopCheckpointStorage;
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
    if (this.agentLoopCheckpointStorage) {
      promises.push(this.agentLoopCheckpointStorage.close());
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
    if (this.agentLoopCheckpointStorage) {
      promises.push(this.agentLoopCheckpointStorage.clear());
    }

    await Promise.all(promises);
    logger.info("StorageManager cleared");
  }
}

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
