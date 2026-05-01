/**
 * Storage Initialization Service
 *
 * Centralized service for initializing and managing storage adapters.
 * Ensures proper initialization order and prevents uninitialized access.
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "StorageInitializationService" });
import type {
  CheckpointStorageAdapter,
  TaskStorageAdapter,
  WorkflowStorageAdapter,
  WorkflowExecutionStorageAdapter,
} from "@wf-agent/storage";

export interface StorageAdapters {
  checkpoint?: CheckpointStorageAdapter;
  task?: TaskStorageAdapter;
  workflow?: WorkflowStorageAdapter;
  workflowExecution?: WorkflowExecutionStorageAdapter;
}

export interface HealthCheckResult {
  adapter: string;
  healthy: boolean;
  message?: string;
  details?: Record<string, unknown>;
}

export interface HealthCheckReport {
  timestamp: Date;
  results: HealthCheckResult[];
  overallHealthy: boolean;
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
      // Validate adapters - at least one should be provided
      const hasAnyAdapter = 
        !!adapters.checkpoint || 
        !!adapters.task || 
        !!adapters.workflow || 
        !!adapters.workflowExecution;

      if (!hasAnyAdapter) {
        logger.warn("No storage adapters provided during initialization");
      }

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
   * Shutdown all storage adapters gracefully
   * Closes connections, flushes buffers, and releases resources
   */
  async shutdown(): Promise<void> {
    if (!this.initialized || !this.adapters) {
      logger.warn("StorageInitializationService not initialized, skipping shutdown");
      return;
    }

    const shutdownErrors: Array<{ type: string; error: Error }> = [];

    // Shutdown each adapter that has a close/shutdown method
    const adapterTypes = [
      { name: 'checkpoint', adapter: this.adapters.checkpoint },
      { name: 'task', adapter: this.adapters.task },
      { name: 'workflow', adapter: this.adapters.workflow },
      { name: 'workflowExecution', adapter: this.adapters.workflowExecution },
    ] as const;

    for (const { name, adapter } of adapterTypes) {
      if (adapter) {
        try {
          // Check if adapter has a close or shutdown method
          if ('close' in adapter && typeof adapter.close === 'function') {
            await (adapter.close as () => Promise<void>)();
            logger.info(`${name} storage adapter closed successfully`);
          } else if ('shutdown' in adapter && typeof adapter.shutdown === 'function') {
            await (adapter.shutdown as () => Promise<void>)();
            logger.info(`${name} storage adapter shut down successfully`);
          } else {
            logger.debug(`${name} storage adapter does not have close/shutdown method, skipping`);
          }
        } catch (error) {
          const err = error instanceof Error ? error : new Error(String(error));
          shutdownErrors.push({ type: name, error: err });
          logger.error(`Failed to shutdown ${name} storage adapter`, { error: err });
        }
      }
    }

    // Reset state after shutdown
    this.adapters = null;
    this.initialized = false;

    if (shutdownErrors.length > 0) {
      logger.warn(`Shutdown completed with ${shutdownErrors.length} error(s)`, {
        errors: shutdownErrors.map(e => ({ type: e.type, message: e.error.message })),
      });
      throw new Error(
        `Shutdown completed with errors: ${shutdownErrors.map(e => `${e.type}: ${e.error.message}`).join(', ')}`,
      );
    }

    logger.info("All storage adapters shut down successfully");
  }

  /**
   * Perform health checks on all initialized adapters
   */
  async healthCheck(): Promise<HealthCheckReport> {
    if (!this.initialized || !this.adapters) {
      return {
        timestamp: new Date(),
        results: [],
        overallHealthy: false,
      };
    }

    const results: HealthCheckResult[] = [];

    // Check each adapter that has a healthCheck method
    const adapterTypes = [
      { name: 'checkpoint', adapter: this.adapters.checkpoint },
      { name: 'task', adapter: this.adapters.task },
      { name: 'workflow', adapter: this.adapters.workflow },
      { name: 'workflowExecution', adapter: this.adapters.workflowExecution },
    ] as const;

    for (const { name, adapter } of adapterTypes) {
      if (adapter) {
        try {
          // Check if adapter has a healthCheck method
          if ('healthCheck' in adapter && typeof adapter.healthCheck === 'function') {
            const result = await (adapter.healthCheck as () => Promise<{ healthy: boolean; message?: string; details?: Record<string, unknown> }> )();
            results.push({
              adapter: name,
              healthy: result.healthy,
              message: result.message,
              details: result.details,
            });
          } else {
            // If no healthCheck method, assume healthy if adapter exists
            results.push({
              adapter: name,
              healthy: true,
              message: 'No healthCheck method available, assuming healthy',
            });
          }
        } catch (error) {
          const err = error instanceof Error ? error : new Error(String(error));
          results.push({
            adapter: name,
            healthy: false,
            message: `Health check failed: ${err.message}`,
            details: { stack: err.stack },
          });
          logger.error(`Health check failed for ${name} storage adapter`, { error: err });
        }
      }
    }

    const overallHealthy = results.every(r => r.healthy);

    return {
      timestamp: new Date(),
      results,
      overallHealthy,
    };
  }
}
