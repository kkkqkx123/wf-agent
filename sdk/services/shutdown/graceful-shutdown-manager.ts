/**
 * Graceful Shutdown Manager
 * Ensures all active workflows are checkpointed before process termination
 */

import { WorkflowExecutionRegistry } from "../../workflow/stores/workflow-execution-registry.js";
import { CheckpointCoordinator, type CheckpointDependencies } from "../../workflow/checkpoint/checkpoint-coordinator.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import { 
  createPlatformSignalHandler,
  getPlatformSignals,
  getMaxShutdownTimeout,
} from "@wf-agent/common-utils";

const logger = createContextualLogger({ component: "GracefulShutdownManager" });

/**
 * Shutdown signal types
 */
export type ShutdownSignal = "SIGTERM" | "SIGINT" | "SIGHUP" | "SIGBREAK";

/**
 * Graceful shutdown result for a single execution
 */
export interface ShutdownCheckpointResult {
  success: boolean;
  executionId: string;
  error?: string;
}

/**
 * Graceful Shutdown Manager Configuration
 */
export interface GracefulShutdownConfig {
  /**
   * Maximum time to wait for all checkpoints during shutdown (milliseconds)
   * Default: 15000 (15 seconds)
   */
  timeoutMs?: number;

  /**
   * Whether to enable graceful shutdown
   * Default: true
   */
  enabled?: boolean;
}

/**
 * Graceful Shutdown Manager
 * Handles process termination signals and creates checkpoints for all active executions
 */
export class GracefulShutdownManager {
  private isShuttingDown: boolean = false;
  private shutdownPromise: Promise<void> | null = null;
  private workflowExecutionRegistry: WorkflowExecutionRegistry;
  private checkpointDependencies: CheckpointDependencies;
  private config: Required<GracefulShutdownConfig>;
  private signalHandler: ReturnType<typeof createPlatformSignalHandler>;

  constructor(
    workflowExecutionRegistry: WorkflowExecutionRegistry,
    checkpointDependencies: CheckpointDependencies,
    config: GracefulShutdownConfig = {},
  ) {
    this.workflowExecutionRegistry = workflowExecutionRegistry;
    this.checkpointDependencies = checkpointDependencies;
    this.config = {
      timeoutMs: config.timeoutMs ?? 15000,
      enabled: config.enabled ?? true,
    };
    this.signalHandler = createPlatformSignalHandler();
  }

  /**
   * Register signal handlers for graceful shutdown
   * Call this during application startup
   */
  registerSignalHandlers(): void {
    if (!this.config.enabled) {
      logger.info("Graceful shutdown is disabled");
      return;
    }

    const platformInfo = getPlatformSignals();
    logger.info("Registering graceful shutdown signal handlers", {
      platform: platformInfo.platform,
      supportedSignals: platformInfo.available,
    });

    // Adjust timeout based on platform limitations
    const adjustedTimeout = getMaxShutdownTimeout(this.config.timeoutMs);
    if (adjustedTimeout !== this.config.timeoutMs) {
      logger.info(`Adjusted shutdown timeout from ${this.config.timeoutMs}ms to ${adjustedTimeout}ms for platform compatibility`, {
        platform: platformInfo.platform,
      });
      this.config.timeoutMs = adjustedTimeout;
    }

    // Use platform-aware signal handler
    this.signalHandler.register(async (signal) => {
      await this.handleShutdown(signal as ShutdownSignal);
    });

    logger.info("Signal handlers registered successfully");
  }

  /**
   * Handle shutdown signal
   * Creates checkpoints for all active executions before exiting
   * @param signal The shutdown signal received
   */
  async handleShutdown(signal: ShutdownSignal): Promise<void> {
    if (this.isShuttingDown) {
      logger.warn("Shutdown already in progress, ignoring signal", { signal });
      return;
    }

    this.isShuttingDown = true;
    logger.info(`Received ${signal}, initiating graceful shutdown...`);

    try {
      // Create shutdown promise with timeout
      this.shutdownPromise = this.createShutdownCheckpoints(signal);

      // Wait for shutdown to complete or timeout
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => {
          reject(new Error(`Graceful shutdown timed out after ${this.config.timeoutMs}ms`));
        }, this.config.timeoutMs);
      });

      await Promise.race([this.shutdownPromise, timeoutPromise]);

      logger.info("Graceful shutdown completed successfully");
      process.exit(0);
    } catch (error) {
      logger.error("Graceful shutdown failed", {
        error: error instanceof Error ? error.message : String(error),
      });
      process.exit(1);
    }
  }

  /**
   * Create checkpoints for all active executions during shutdown
   * @param signal The shutdown signal that triggered this
   */
  private async createShutdownCheckpoints(signal: ShutdownSignal): Promise<void> {
    const activeExecutions = this.workflowExecutionRegistry.getActive();

    if (activeExecutions.length === 0) {
      logger.info("No active executions, immediate shutdown");
      return;
    }

    logger.info(`Creating checkpoints for ${activeExecutions.length} active executions`, {
      signal,
    });

    const results = await Promise.allSettled<ShutdownCheckpointResult>(
      activeExecutions.map(async (entity): Promise<ShutdownCheckpointResult> => {
        try {
          await CheckpointCoordinator.createCheckpoint(
            entity.id,
            this.checkpointDependencies,
            {
              description: `Graceful shutdown (${signal})`,
              customFields: {
                shutdownSignal: signal,
                timestamp: Date.now(),
                reason: "process_termination",
              },
            },
            undefined, // conversationManager will be retrieved from stateCoordinatorMap
          );

          logger.debug(`Checkpoint created for execution ${entity.id}`);
          return { success: true, executionId: entity.id };
        } catch (error) {
          logger.error(`Failed to create checkpoint for ${entity.id}`, {
            error: error instanceof Error ? error.message : String(error),
          });
          return {
            success: false,
            executionId: entity.id,
            error: error instanceof Error ? error.message : String(error),
          };
        }
      }),
    );

    // Log summary
    const successCount = results.filter(
      (r) => r.status === "fulfilled" && r.value.success,
    ).length;
    const failureCount = results.length - successCount;

    logger.info("Shutdown checkpoint summary", {
      total: results.length,
      success: successCount,
      failures: failureCount,
    });

    if (failureCount > 0) {
      const failures = results
        .filter((r) => r.status === "rejected" || !r.value.success)
        .map((r) => ({
          executionId: r.status === "fulfilled" ? r.value.executionId : "unknown",
          error: r.status === "rejected" ? String(r.reason) : r.value.error,
        }));

      logger.warn("Some checkpoints failed during shutdown", {
        failures,
      });
    }
  }

  /**
   * Manually trigger graceful shutdown (for testing or programmatic use)
   * @param signal Optional shutdown signal (default: "SIGTERM")
   */
  async triggerShutdown(signal: ShutdownSignal = "SIGTERM"): Promise<void> {
    await this.handleShutdown(signal);
  }

  /**
   * Check if shutdown is in progress
   */
  isShutdownInProgress(): boolean {
    return this.isShuttingDown;
  }
}
