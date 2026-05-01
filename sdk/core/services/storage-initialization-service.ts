/**
 * Storage Initialization Service
 *
 * Centralized service for initializing and managing storage adapters.
 * Ensures proper initialization order and prevents uninitialized access.
 */

import { logger } from "../../utils/logger.js";
import type {
  CheckpointStorageAdapter,
  TaskStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
} from "@wf-agent/storage";

export interface StorageAdapters {
  checkpoint: CheckpointStorageAdapter;
  task: TaskStorageAdapter;
  workflow: WorkflowStorageAdapter;
  workflowExecution: WorkflowExecutionStorageAdapter;
}

export class StorageInitializationService {
  private static instance: StorageInitializationService | null = null;
  private adapters: StorageAdapters | null = null;
  private initialized = false;

  private constructor() {}

  static getInstance(): StorageInitializationService {
    if (!StorageInitializationService.instance) {
      StorageInitializationService.instance = new StorageInitializationService();
    }
    return StorageInitializationService.instance;
  }

  /**
   * Initialize all storage adapters with proper ordering
   */
  async initialize(adapters: StorageAdapters): Promise<void> {
    if (this.initialized) {
      logger.warn("StorageInitializationService already initialized");
      return;
    }

    try {
      // Validate adapters
      this.validateAdapters(adapters);

      // Store adapters
      this.adapters = adapters;
      this.initialized = true;

      logger.info("Storage adapters initialized successfully", {
        checkpoint: !!adapters.checkpoint,
        task: !!adapters.task,
        workflow: !!adapters.workflow,
        workflowExecution: !!adapters.workflowExecution,
      });
    } catch (error) {
      logger.error("Failed to initialize storage adapters", { error });
      throw error;
    }
  }

  /**
   * Get initialized adapters (throws if not initialized)
   */
  getAdapters(): StorageAdapters {
    if (!this.initialized || !this.adapters) {
      throw new Error(
        "Storage adapters not initialized. Call initialize() first.",
      );
    }
    return this.adapters;
  }

  /**
   * Get specific adapter by type
   */
  getAdapter<T extends keyof StorageAdapters>(type: T): StorageAdapters[T] {
    const adapters = this.getAdapters();
    return adapters[type];
  }

  /**
   * Check if service is initialized
   */
  isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Reset initialization state (for testing)
   */
  reset(): void {
    this.adapters = null;
    this.initialized = false;
    logger.debug("StorageInitializationService reset");
  }

  /**
   * Validate that all required adapters are provided
   */
  private validateAdapters(adapters: StorageAdapters): void {
    const required: Array<keyof StorageAdapters> = [
      "checkpoint",
      "task",
      "workflow",
      "workflowExecution",
    ];

    const missing = required.filter((type) => !adapters[type]);

    if (missing.length > 0) {
      throw new Error(`Missing required storage adapters: ${missing.join(", ")}`);
    }
  }
}
