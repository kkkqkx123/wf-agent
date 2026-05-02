/**
 * Background Cleanup Scheduler
 * Provides asynchronous background cleanup for storage systems
 */

import { createModuleLogger } from "../logger.js";

const logger = createModuleLogger("cleanup-scheduler");

/**
 * Cleanup scheduler configuration
 */
export interface CleanupSchedulerConfig {
  /** Cleanup interval in milliseconds (default: 5 minutes) */
  intervalMs: number;
  /** Enable/disable scheduler */
  enabled: boolean;
}

/**
 * Default cleanup scheduler configuration
 */
export const DEFAULT_CLEANUP_SCHEDULER_CONFIG: CleanupSchedulerConfig = {
  intervalMs: 5 * 60 * 1000, // 5 minutes
  enabled: true,
};

/**
 * Cleanup result interface
 */
export interface CleanupResult {
  deletedCount: number;
  freedSpaceBytes: number;
}

/**
 * State manager interface for cleanup operations
 */
export interface CleanupStateManager {
  executeCleanup(): Promise<CleanupResult>;
}

/**
 * Background Cleanup Scheduler
 * 
 * Manages periodic cleanup operations without blocking main operations.
 * Prevents concurrent cleanup executions and provides start/stop control.
 */
export class CleanupScheduler {
  private timer: NodeJS.Timeout | null = null;
  private isRunning: boolean = false;
  private config: CleanupSchedulerConfig;

  constructor(
    private stateManager: CleanupStateManager,
    config?: Partial<CleanupSchedulerConfig>
  ) {
    this.config = {
      ...DEFAULT_CLEANUP_SCHEDULER_CONFIG,
      ...config,
    };
  }

  /**
   * Start the cleanup scheduler
   */
  start(): void {
    if (!this.config.enabled || this.timer) {
      logger.debug("Cleanup scheduler not started", {
        enabled: this.config.enabled,
        hasTimer: !!this.timer,
      });
      return;
    }

    logger.info("Starting cleanup scheduler", {
      intervalMs: this.config.intervalMs,
    });

    this.timer = setInterval(async () => {
      await this.executeCleanup();
    }, this.config.intervalMs);

    // Ensure timer doesn't prevent process exit
    if (this.timer.unref) {
      this.timer.unref();
    }
  }

  /**
   * Stop the cleanup scheduler
   */
  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
      logger.info("Cleanup scheduler stopped");
    }
  }

  /**
   * Execute a single cleanup operation
   * Prevents concurrent executions
   */
  private async executeCleanup(): Promise<void> {
    if (this.isRunning) {
      logger.debug("Skipping cleanup - already running");
      return;
    }

    try {
      this.isRunning = true;
      logger.debug("Executing background cleanup");

      const startTime = Date.now();
      const result = await this.stateManager.executeCleanup();
      const elapsed = Date.now() - startTime;

      if (result.deletedCount > 0) {
        logger.info("Background cleanup completed", {
          deletedCount: result.deletedCount,
          freedSpaceBytes: result.freedSpaceBytes,
          durationMs: elapsed,
        });
      } else {
        logger.debug("Background cleanup completed - nothing to delete", {
          durationMs: elapsed,
        });
      }
    } catch (error) {
      logger.error("Background cleanup failed", {
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      this.isRunning = false;
    }
  }

  /**
   * Check if scheduler is currently running
   */
  isActive(): boolean {
    return this.timer !== null;
  }

  /**
   * Get current configuration
   */
  getConfig(): CleanupSchedulerConfig {
    return { ...this.config };
  }

  /**
   * Update configuration
   */
  updateConfig(config: Partial<CleanupSchedulerConfig>): void {
    const wasActive = this.isActive();
    
    if (wasActive) {
      this.stop();
    }

    this.config = {
      ...this.config,
      ...config,
    };

    if (wasActive && this.config.enabled) {
      this.start();
    }

    logger.debug("Cleanup scheduler configuration updated", {
      config: this.config,
    });
  }
}
