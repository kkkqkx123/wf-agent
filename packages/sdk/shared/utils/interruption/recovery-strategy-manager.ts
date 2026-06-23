/**
 * Interruption Recovery Strategy Manager
 *
 * Provides a framework for custom recovery logic when interruptions occur.
 * Supports before/after callbacks for PAUSE and RESUME operations.
 *
 * Features:
 * - Registration of execution-type-specific strategies
 * - Before/after interruption callbacks
 * - Automatic checkpoint save/restore support
 * - Extensible strategy pattern
 */

import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "RecoveryStrategyManager" });

/**
 * Recovery context passed to strategy callbacks
 */
export interface RecoveryContext {
  /** Execution ID */
  executionId: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Iteration number (for Agent loops, optional) */
  iteration?: number;
  /** Current state object (can be modified by strategies) */
  state: Record<string, unknown>;
}

/**
 * Recovery strategy interface
 *
 * Strategies can implement callbacks for different phases of interruption/recovery.
 */
export interface RecoveryStrategy {
  /**
   * Called before an interruption occurs (PAUSE or STOP)
   * Useful for saving state, cleanup, etc.
   */
  beforeInterrupt?(type: "PAUSE" | "STOP", context: RecoveryContext): Promise<void>;

  /**
   * Called before resuming execution
   * Useful for loading state, validation, etc.
   */
  beforeResume?(context: RecoveryContext): Promise<void>;

  /**
   * Called after successfully resuming execution
   * Useful for cleanup, notifications, etc.
   */
  afterResume?(context: RecoveryContext): Promise<void>;
}

/**
 * Recovery Strategy Manager
 *
 * Manages registration and execution of recovery strategies for different execution types.
 */
export class RecoveryStrategyManager {
  private strategies: Map<string, RecoveryStrategy> = new Map();

  constructor() {
    logger.info("Recovery strategy manager initialized");
  }

  /**
   * Register a recovery strategy for a specific execution type
   *
   * @param executionType Type identifier (e.g., "workflow", "agent-loop", "subgraph")
   * @param strategy The recovery strategy to register
   */
  register(executionType: string, strategy: RecoveryStrategy): void {
    this.strategies.set(executionType, strategy);
    logger.info("Recovery strategy registered", { executionType });
  }

  /**
   * Unregister a recovery strategy
   *
   * @param executionType Type identifier
   */
  unregister(executionType: string): void {
    this.strategies.delete(executionType);
    logger.info("Recovery strategy unregistered", { executionType });
  }

  /**
   * Execute beforeInterrupt callbacks for all registered strategies
   *
   * @param executionType Execution type
   * @param type Interruption type (PAUSE or STOP)
   * @param context Recovery context
   */
  async beforeInterrupt(
    executionType: string,
    type: "PAUSE" | "STOP",
    context: RecoveryContext,
  ): Promise<void> {
    const strategy = this.strategies.get(executionType);
    if (strategy?.beforeInterrupt) {
      try {
        await strategy.beforeInterrupt(type, context);
        logger.debug("beforeInterrupt callback executed", {
          executionType,
          type,
        });
      } catch (error) {
        logger.error("Error in beforeInterrupt callback", {
          executionType,
          type,
          error: error instanceof Error ? error.message : String(error),
        });
        // Don't throw - strategy errors should not block interruption
      }
    }
  }

  /**
   * Execute beforeResume callbacks for all registered strategies
   *
   * @param executionType Execution type
   * @param context Recovery context
   */
  async beforeResume(executionType: string, context: RecoveryContext): Promise<void> {
    const strategy = this.strategies.get(executionType);
    if (strategy?.beforeResume) {
      try {
        await strategy.beforeResume(context);
        logger.debug("beforeResume callback executed", {
          executionType,
        });
      } catch (error) {
        logger.error("Error in beforeResume callback", {
          executionType,
          error: error instanceof Error ? error.message : String(error),
        });
        // Don't throw - strategy errors should not block resume
      }
    }
  }

  /**
   * Execute afterResume callbacks for all registered strategies
   *
   * @param executionType Execution type
   * @param context Recovery context
   */
  async afterResume(executionType: string, context: RecoveryContext): Promise<void> {
    const strategy = this.strategies.get(executionType);
    if (strategy?.afterResume) {
      try {
        await strategy.afterResume(context);
        logger.debug("afterResume callback executed", {
          executionType,
        });
      } catch (error) {
        logger.error("Error in afterResume callback", {
          executionType,
          error: error instanceof Error ? error.message : String(error),
        });
        // Don't throw - strategy errors should not block normal execution
      }
    }
  }

  /**
   * Check if a strategy is registered for an execution type
   *
   * @param executionType Execution type
   * @returns true if strategy exists
   */
  hasStrategy(executionType: string): boolean {
    return this.strategies.has(executionType);
  }

  /**
   * Get all registered execution types
   *
   * @returns Array of execution type identifiers
   */
  getRegisteredTypes(): string[] {
    return Array.from(this.strategies.keys());
  }

  /**
   * Clear all registered strategies
   */
  clear(): void {
    this.strategies.clear();
    logger.info("All recovery strategies cleared");
  }
}

/**
 * Built-in auto-save recovery strategy
 *
 * Automatically saves checkpoints on pause and restores them on resume.
 * This is a convenience implementation that can be used as a template.
 */
export function createAutoSaveStrategy(options: {
  saveCheckpoint: (executionId: string, state: Record<string, unknown>) => Promise<void>;
  loadCheckpoint: (executionId: string) => Promise<Record<string, unknown>>;
}): RecoveryStrategy {
  return {
    async beforeInterrupt(type, context) {
      if (type === "PAUSE") {
        try {
          await options.saveCheckpoint(context.executionId, context.state);
          logger.info("Auto-saved checkpoint", {
            executionId: context.executionId,
          });
        } catch (error) {
          logger.error("Failed to auto-save checkpoint", {
            executionId: context.executionId,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    },

    async beforeResume(context) {
      try {
        const checkpoint = await options.loadCheckpoint(context.executionId);
        if (checkpoint) {
          // Merge checkpoint state into current state
          Object.assign(context.state, checkpoint["state"]);
          logger.info("Restored from checkpoint", {
            executionId: context.executionId,
          });
        }
      } catch (error) {
        logger.error("Failed to restore checkpoint", {
          executionId: context.executionId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    },
  };
}
