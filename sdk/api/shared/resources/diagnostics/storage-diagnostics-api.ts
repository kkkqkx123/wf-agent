/**
 * StorageDiagnosticsAPI - Storage Diagnostics API
 * Provides aggregated storage health monitoring and diagnostics
 *
 * Responsibilities:
 * - Check which storage adapters are configured
 * - Report storage health status
 * - Query item counts across all storage types
 * - Provide diagnostics information for debugging
 */

import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "StorageDiagnosticsAPI" });

/**
 * Storage adapter health status
 */
export interface StorageAdapterHealth {
  /** Adapter name */
  name: string;
  /** Whether the adapter is configured */
  configured: boolean;
  /** Status message */
  status: "healthy" | "not_configured" | "error";
  /** Error message if applicable */
  error?: string;
}

/**
 * Storage item counts
 */
export interface StorageItemCounts {
  /** Number of registered workflows */
  workflows: number;
  /** Number of active workflow executions */
  executions: number;
  /** Number of preprocessed workflow graphs */
  graphs: number;
  /** Number of tasks (from task registry stats) */
  tasks: {
    total: number;
    queued: number;
    running: number;
    completed: number;
    failed: number;
    cancelled: number;
    timeout: number;
  };
}

/**
 * Storage diagnostics report
 */
export interface StorageDiagnosticsReport {
  /** Overall health status */
  overallStatus: "healthy" | "degraded" | "unhealthy";
  /** Per-adapter health status */
  adapterHealth: StorageAdapterHealth[];
  /** Storage item counts */
  itemCounts: StorageItemCounts;
  /** Timestamp of the report */
  timestamp: number;
}

/**
 * StorageDiagnosticsAPI - Storage Diagnostics and Health Monitoring API
 */
export class StorageDiagnosticsAPI {
  private deps: APIDependencyManager;

  constructor(deps: APIDependencyManager) {
    this.deps = deps;
    logger.info("StorageDiagnosticsAPI initialized");
  }

  /**
   * Get the health status of all storage adapters
   * @returns Array of adapter health statuses
   */
  async getAdapterHealth(): Promise<StorageAdapterHealth[]> {
    const adapters: StorageAdapterHealth[] = [];

    // Check WorkflowStorageAdapter
    try {
      const workflowAdapter = this.deps.getWorkflowStorageAdapter();
      adapters.push({
        name: "WorkflowStorageAdapter",
        configured: workflowAdapter !== null,
        status: workflowAdapter ? "healthy" : "not_configured",
      });
    } catch (error) {
      adapters.push({
        name: "WorkflowStorageAdapter",
        configured: false,
        status: "error",
        error: String(error),
      });
    }

    // Check WorkflowExecutionStorageAdapter
    try {
      const executionAdapter = this.deps.getWorkflowExecutionStorageAdapter();
      adapters.push({
        name: "WorkflowExecutionStorageAdapter",
        configured: executionAdapter !== null,
        status: executionAdapter ? "healthy" : "not_configured",
      });
    } catch (error) {
      adapters.push({
        name: "WorkflowExecutionStorageAdapter",
        configured: false,
        status: "error",
        error: String(error),
      });
    }

    // Check CheckpointStorageAdapter
    try {
      const checkpointAdapter = this.deps.getCheckpointStorageAdapter();
      adapters.push({
        name: "CheckpointStorageAdapter",
        configured: checkpointAdapter !== null,
        status: checkpointAdapter ? "healthy" : "not_configured",
      });
    } catch (error) {
      adapters.push({
        name: "CheckpointStorageAdapter",
        configured: false,
        status: "error",
        error: String(error),
      });
    }

    // Check TaskStorageAdapter
    try {
      const taskAdapter = this.deps.getTaskStorageAdapter();
      adapters.push({
        name: "TaskStorageAdapter",
        configured: taskAdapter !== null,
        status: taskAdapter ? "healthy" : "not_configured",
      });
    } catch (error) {
      adapters.push({
        name: "TaskStorageAdapter",
        configured: false,
        status: "error",
        error: String(error),
      });
    }

    return adapters;
  }

  /**
   * Get storage item counts from in-memory registries
   * @returns Storage item counts
   */
  async getItemCounts(): Promise<StorageItemCounts> {
    const workflowRegistry = this.deps.getWorkflowRegistry();
    const executionRegistry = this.deps.getWorkflowExecutionRegistry();
    const graphRegistry = this.deps.getWorkflowGraphRegistry();
    const taskRegistry = this.deps.getTaskRegistry();

    const workflows = workflowRegistry.size();
    const executions = executionRegistry.getAll().length;
    const graphs = graphRegistry.size();

    // Get task stats using getStats() (not getStats() from TaskStorageAdapter)
    // TaskRegistry.getStats() returns in-memory stats
    const taskStats = taskRegistry.getStats();

    return {
      workflows,
      executions,
      graphs,
      tasks: {
        total: taskStats.total,
        queued: taskStats.queued,
        running: taskStats.running,
        completed: taskStats.completed,
        failed: taskStats.failed,
        cancelled: taskStats.cancelled,
        timeout: taskStats.timeout,
      },
    };
  }

  /**
   * Get a full storage diagnostics report
   * @returns Storage diagnostics report
   */
  async getDiagnosticsReport(): Promise<StorageDiagnosticsReport> {
    const adapterHealth = await this.getAdapterHealth();
    const itemCounts = await this.getItemCounts();

    // Determine overall status
    const hasError = adapterHealth.some(a => a.status === "error");
    const allConfigured = adapterHealth.every(
      a => a.status === "healthy" || a.status === "not_configured",
    );
    const hasHealthy = adapterHealth.some(a => a.status === "healthy");

    let overallStatus: "healthy" | "degraded" | "unhealthy";
    if (hasError) {
      overallStatus = "unhealthy";
    } else if (!hasHealthy) {
      overallStatus = "degraded";
    } else if (!allConfigured) {
      overallStatus = "degraded";
    } else {
      overallStatus = "healthy";
    }

    return {
      overallStatus,
      adapterHealth,
      itemCounts,
      timestamp: Date.now(),
    };
  }

  /**
   * Quick health check - checks if any storage adapters are configured
   * @returns Whether at least one storage adapter is configured and healthy
   */
  async isStorageAvailable(): Promise<boolean> {
    const adapters = await this.getAdapterHealth();
    return adapters.some(a => a.status === "healthy");
  }
}
