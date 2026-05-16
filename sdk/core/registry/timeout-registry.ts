/**
 * Timeout Registry
 * 
 * Global registry for managing TimeoutManager instances across executions.
 * Provides centralized timeout registration, batch operations, and metrics aggregation.
 */

import { TimeoutManager } from "../state-managers/timeout-manager.js";
import type {
  TimeoutHandle,
  TimeoutRegistration,
} from "../types/timeout.js";
import type {
  TimeoutRegistryConfig,
  ResolvedTimeoutRegistryConfig,
} from "../types/timeout-config.js";
import { DEFAULT_TIMEOUT_REGISTRY_CONFIG } from "../types/timeout-config.js";

/**
 * Timeout Registry
 * 
 * Centralized registry for managing timeouts across all executions.
 * Maintains a TimeoutManager instance per execution ID.
 * 
 * Features:
 * - Per-execution TimeoutManager management
 * - Batch operations by tag or execution ID
 * - Automatic cleanup on execution end
 * - Cross-execution metrics aggregation
 */
export class TimeoutRegistry {
  /** Map of TimeoutManager instances by execution ID */
  private managers: Map<string, TimeoutManager> = new Map();

  /** Map to track tags across all executions for efficient tag-based operations */
  private tagIndex: Map<string, Set<string>> = new Map(); // tag -> Set<executionId>

  /** Resolved configuration */
  private config: ResolvedTimeoutRegistryConfig;

  /** Global statistics */
  private globalStats = {
    totalRegistered: 0,
    timedOutCount: 0,
    cancelledCount: 0,
  };

  /**
   * Create a new TimeoutRegistry
   * @param config Optional configuration (will be merged with defaults)
   */
  constructor(config?: TimeoutRegistryConfig) {
    this.config = {
      ...DEFAULT_TIMEOUT_REGISTRY_CONFIG,
      ...config,
      defaultManagerConfig: {
        ...DEFAULT_TIMEOUT_REGISTRY_CONFIG.defaultManagerConfig,
        ...config?.defaultManagerConfig,
      },
    };

    // Validate configuration
    if (this.config.maxTimeoutsPerExecution <= 0) {
      throw new Error("maxTimeoutsPerExecution must be positive");
    }
  }

  /**
   * Get or create a TimeoutManager for an execution
   * @param executionId Execution ID
   * @returns TimeoutManager instance
   */
  getManager(executionId: string): TimeoutManager {
    let manager = this.managers.get(executionId);

    if (!manager) {
      manager = new TimeoutManager(this.config.defaultManagerConfig);
      this.managers.set(executionId, manager);
    }

    return manager;
  }

  /**
   * Register a timeout for a specific execution
   * Convenience method that gets the manager and registers in one call
   * @param executionId Execution ID
   * @param options Timeout registration options
   * @returns TimeoutHandle
   */
  register(executionId: string, options: TimeoutRegistration): TimeoutHandle {
    const manager = this.getManager(executionId);
    const handle = manager.register(options);

    // Update tag index if tag is provided
    if (options.tag) {
      if (!this.tagIndex.has(options.tag)) {
        this.tagIndex.set(options.tag, new Set());
      }
      this.tagIndex.get(options.tag)!.add(executionId);
    }

    // Update global statistics
    this.globalStats.totalRegistered++;

    return handle;
  }

  /**
   * Cancel all timeouts for an execution
   * @param executionId Execution ID
   */
  cancelByExecutionId(executionId: string): void {
    const manager = this.managers.get(executionId);
    if (manager) {
      try {
        manager.clear();
      } catch (error) {
        console.error(`Failed to cancel timeouts for execution ${executionId}:`, error);
      }
    }
  }

  /**
   * Cancel all timeouts with a specific tag across all executions
   * @param tag Tag to cancel
   */
  cancelByTag(tag: string): void {
    const executionIds = this.tagIndex.get(tag);
    if (!executionIds) {
      return; // No timeouts with this tag
    }

    // Cancel timeouts in all executions that have this tag
    executionIds.forEach((executionId) => {
      const manager = this.managers.get(executionId);
      if (manager) {
        // Note: TimeoutManager doesn't have a cancelByTag method yet
        // For now, we clear all timeouts in those executions
        // A more refined implementation would require adding cancelByTag to TimeoutManager
        manager.clear();
      }
    });

    // Clean up the tag index
    this.tagIndex.delete(tag);
  }

  /**
   * Get aggregated statistics across all executions
   * @returns Global timeout statistics
   */
  getStats(): {
    activeExecutions: number;
    totalTimeouts: number;
    totalRegistered: number;
    timedOutCount: number;
    cancelledCount: number;
  } {
    let totalTimeouts = 0;
    let timedOutCount = 0;
    let cancelledCount = 0;

    this.managers.forEach((manager) => {
      const stats = manager.getStats();
      totalTimeouts += stats.activeTimeouts;
      timedOutCount += stats.timedOutCount;
      cancelledCount += stats.cancelledCount;
    });

    return {
      activeExecutions: this.managers.size,
      totalTimeouts,
      totalRegistered: this.globalStats.totalRegistered,
      timedOutCount: this.globalStats.timedOutCount + timedOutCount,
      cancelledCount: this.globalStats.cancelledCount + cancelledCount,
    };
  }

  /**
   * Clean up a TimeoutManager when execution ends
   * Cancels all active timeouts and removes the manager
   * @param executionId Execution ID to clean up
   */
  cleanup(executionId: string): void {
    const manager = this.managers.get(executionId);
    if (manager) {
      try {
        // Get stats before clearing to capture current state
        const statsBefore = manager.getStats();
        
        // Clear all timeouts (this will update manager's internal cancelledCount)
        manager.clear();
        
        // Update global statistics
        // The active timeouts that were cleared should be counted as cancelled
        this.globalStats.cancelledCount += statsBefore.activeTimeouts;
        this.globalStats.timedOutCount += statsBefore.timedOutCount;

        // Remove from tag index
        this.tagIndex.forEach((executionIds, tag) => {
          executionIds.delete(executionId);
          if (executionIds.size === 0) {
            this.tagIndex.delete(tag);
          }
        });

        // Remove manager from registry
        this.managers.delete(executionId);
      } catch (error) {
        console.error(`Failed to cleanup execution ${executionId}:`, error);
        // Still remove the manager even if cleanup fails
        this.managers.delete(executionId);
      }
    }
  }

  /**
   * Clean up all managers (e.g., on application shutdown)
   */
  cleanupAll(): void {
    const errors: Array<{ executionId: string; error: unknown }> = [];

    this.managers.forEach((manager, executionId) => {
      try {
        manager.clear();
      } catch (error) {
        errors.push({ executionId, error });
        console.error(`Failed to cleanup execution ${executionId}:`, error);
      }
    });

    this.managers.clear();
    this.tagIndex.clear();

    if (errors.length > 0) {
      console.warn(`Cleanup completed with ${errors.length} errors`);
    }
  }

  /**
   * Check if a manager exists for an execution
   * @param executionId Execution ID
   * @returns true if manager exists
   */
  hasManager(executionId: string): boolean {
    return this.managers.has(executionId);
  }

  /**
   * Get number of active execution managers
   * @returns Number of managers
   */
  getManagerCount(): number {
    return this.managers.size;
  }

  /**
   * Remove a manager without cleaning up timeouts
   * Use with caution - may cause resource leaks
   * @param executionId Execution ID
   */
  removeManager(executionId: string): void {
    this.managers.delete(executionId);
  }
}

/**
 * Singleton instance of TimeoutRegistry
 * Exported for convenience, but applications should create their own instances
 */
let defaultRegistry: TimeoutRegistry | null = null;

/**
 * Get the default TimeoutRegistry instance
 * Creates one if it doesn't exist
 * @returns Default TimeoutRegistry
 */
export function getDefaultTimeoutRegistry(): TimeoutRegistry {
  if (!defaultRegistry) {
    defaultRegistry = new TimeoutRegistry();
  }
  return defaultRegistry;
}

/**
 * Reset the default registry (useful for testing)
 */
export function resetDefaultTimeoutRegistry(): void {
  if (defaultRegistry) {
    defaultRegistry.cleanupAll();
    defaultRegistry = null;
  }
}
