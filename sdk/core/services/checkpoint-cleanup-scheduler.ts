/**
 * Checkpoint Cleanup Scheduler
 *
 * Decouples checkpoint cleanup from creation to improve performance.
 * Runs background cleanup tasks on a configurable schedule.
 */

import { sdkLogger as logger } from "../../utils/logger.js";
import type { CheckpointStorageAdapter } from "@wf-agent/storage";
import type { EventRegistry } from "../registry/event-registry.js";

export interface CleanupSchedulerConfig {
  /** Interval between cleanup runs (ms) - default: 60000 (1 minute) */
  intervalMs?: number;
  /** Maximum age of checkpoints to keep (ms) - default: 3600000 (1 hour) */
  maxAgeMs?: number;
  /** Maximum number of checkpoints per execution to keep - default: 10 */
  maxCheckpointsPerExecution?: number;
  /** Enable automatic cleanup - default: true */
  enabled?: boolean;
}

export class CheckpointCleanupScheduler {
  private config: Required<CleanupSchedulerConfig>;
  private storageAdapter: CheckpointStorageAdapter;
  private eventManager?: EventRegistry;
  private timerId: NodeJS.Timeout | null = null;
  private isRunning = false;
  private stopped = false;

  constructor(
    storageAdapter: CheckpointStorageAdapter,
    config: CleanupSchedulerConfig = {},
    eventManager?: EventRegistry,
  ) {
    this.storageAdapter = storageAdapter;
    this.eventManager = eventManager;
    this.config = {
      intervalMs: config.intervalMs ?? 60000,
      maxAgeMs: config.maxAgeMs ?? 3600000,
      maxCheckpointsPerExecution: config.maxCheckpointsPerExecution ?? 10,
      enabled: config.enabled ?? true,
    };
  }

  /**
   * Start the cleanup scheduler
   */
  start(): void {
    if (!this.config.enabled) {
      logger.info("Checkpoint cleanup scheduler disabled");
      return;
    }

    if (this.timerId) {
      logger.warn("Checkpoint cleanup scheduler already running");
      return;
    }

    this.stopped = false;
    this.scheduleNextRun();
    logger.info("Checkpoint cleanup scheduler started", {
      intervalMs: this.config.intervalMs,
      maxAgeMs: this.config.maxAgeMs,
    });
  }

  /**
   * Stop the cleanup scheduler
   */
  stop(): void {
    if (this.timerId) {
      clearInterval(this.timerId);
      this.timerId = null;
      logger.info("Checkpoint cleanup scheduler stopped");
    }
    this.stopped = true;
  }

  /**
   * Run a single cleanup cycle immediately
   */
  async runCleanup(): Promise<void> {
    if (this.isRunning) {
      logger.warn("Cleanup already in progress, skipping");
      return;
    }

    this.isRunning = true;
    const startTime = Date.now();

    try {
      logger.debug("Starting checkpoint cleanup cycle");

      // Get all checkpoint IDs
      const allIds = await this.storageAdapter.list();
      logger.debug(`Found ${allIds.length} total checkpoints`);

      let cleanedCount = 0;
      const now = Date.now();

      // Group checkpoints by execution ID for per-execution limits
      const checkpointsByExecution = new Map<string, string[]>();

      for (const id of allIds) {
        try {
          const metadata = await this.storageAdapter.getMetadata(id);
          if (!metadata) {
            // No metadata, safe to delete
            await this.storageAdapter.delete(id);
            cleanedCount++;
            continue;
          }

          const executionId = metadata.executionId;
          if (!checkpointsByExecution.has(executionId)) {
            checkpointsByExecution.set(executionId, []);
          }
          checkpointsByExecution.get(executionId)!.push(id);
        } catch (error) {
          logger.warn("Failed to get metadata for checkpoint", { id, error });
        }
      }

      // Clean up old and excess checkpoints per execution
      for (const [executionId, ids] of checkpointsByExecution.entries()) {
        const result = await this.cleanupExecutionCheckpoints(executionId, ids, now);
        cleanedCount += result.cleanedCount;
      }

      const duration = Date.now() - startTime;
      logger.info("Checkpoint cleanup completed", {
        cleanedCount,
        duration,
        remainingCount: allIds.length - cleanedCount,
      });
    } catch (error) {
      logger.error("Checkpoint cleanup failed", {
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      this.isRunning = false;
    }
  }

  /**
   * Clean up checkpoints for a specific execution
   */
  private async cleanupExecutionCheckpoints(
    executionId: string,
    checkpointIds: string[],
    now: number,
  ): Promise<{ cleanedCount: number }> {
    let cleanedCount = 0;

    // Sort by timestamp (newest first)
    const sorted = await Promise.all(
      checkpointIds.map(async (id) => {
        const metadata = await this.storageAdapter.getMetadata(id);
        return { id, timestamp: metadata?.timestamp || 0 };
      }),
    );
    sorted.sort((a, b) => b.timestamp - a.timestamp);

    // Delete old checkpoints based on maxAge
    for (const { id, timestamp } of sorted) {
      const age = now - timestamp;
      if (age > this.config.maxAgeMs) {
        try {
          await this.storageAdapter.delete(id);
          cleanedCount++;
          logger.debug("Deleted old checkpoint", { id, age });
        } catch (error) {
          logger.warn("Failed to delete old checkpoint", { id, error });
        }
      }
    }

    // Keep only the most recent N checkpoints
    const remaining = sorted.filter(({ timestamp }) => now - timestamp <= this.config.maxAgeMs);
    if (remaining.length > this.config.maxCheckpointsPerExecution) {
      const toDelete = remaining.slice(this.config.maxCheckpointsPerExecution);
      for (const { id } of toDelete) {
        try {
          await this.storageAdapter.delete(id);
          cleanedCount++;
          logger.debug("Deleted excess checkpoint", { id });
        } catch (error) {
          logger.warn("Failed to delete excess checkpoint", { id, error });
        }
      }
    }

    return { cleanedCount };
  }

  /**
   * Schedule the next cleanup run
   */
  private scheduleNextRun(): void {
    if (this.stopped) return;

    this.timerId = setTimeout(async () => {
      await this.runCleanup();
      this.scheduleNextRun();
    }, this.config.intervalMs);
  }

  /**
   * Check if scheduler is running
   */
  isSchedulerRunning(): boolean {
    return this.timerId !== null && !this.stopped;
  }

  /**
   * Get current configuration
   */
  getConfig(): Required<CleanupSchedulerConfig> {
    return { ...this.config };
  }

  /**
   * Update configuration
   */
  updateConfig(newConfig: Partial<CleanupSchedulerConfig>): void {
    this.config = { ...this.config, ...newConfig };
    logger.debug("Cleanup scheduler config updated", this.config);
  }
}
