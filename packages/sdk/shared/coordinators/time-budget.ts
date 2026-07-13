/**
 * Time Budget Manager for Retry Operations
 *
 * Provides configurable time budget tracking for retry operations.
 * Supports two modes:
 * - 'delay-only': Only tracks retry delays
 * - 'total-time': Tracks both delays and execution time
 *
 * Problem #5 Fix: Unified time budget management across retry operations
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TimeBudget" });

export type TimeBudgetMode = "delay-only" | "total-time";

/**
 * Time budget tracking result
 */
export interface TimeBudgetCheckResult {
  allowed: boolean;
  remaining: number;
  reason?: string;
}

/**
 * Time Budget - Tracks resource allocation for retries
 *
 * In 'delay-only' mode: only delays count towards budget
 * In 'total-time' mode: both delays and execution time count
 *
 * Typical usage:
 * ```typescript
 * const budget = new TimeBudget({
 *   totalBudgetMs: 300000,  // 5 minutes
 *   mode: 'total-time',
 *   startTime: Date.now(),
 * });
 *
 * // Check before retry delay
 * const delayCheck = budget.canConsumeDelay(5000);
 * if (delayCheck.allowed) {
 *   budget.consumeDelay(5000);
 * }
 *
 * // Track execution time
 * const execStart = Date.now();
 * await executeNode();
 * budget.recordExecutionTime(Date.now() - execStart);
 * ```
 */
export class TimeBudget {
  private readonly totalBudgetMs: number;
  private readonly mode: TimeBudgetMode;
  private readonly startTime: number;
  private readonly name: string;

  private totalDelayConsumed: number = 0;
  private totalExecutionTimeConsumed: number = 0;
  private retryCount: number = 0;

  constructor(config: {
    totalBudgetMs: number;
    mode?: TimeBudgetMode;
    startTime?: number;
    name?: string;
  }) {
    this.totalBudgetMs = config.totalBudgetMs;
    this.mode = config.mode ?? "delay-only";
    this.startTime = config.startTime ?? Date.now();
    this.name = config.name ?? "TimeBudget";

    logger.debug("TimeBudget initialized", {
      name: this.name,
      totalBudgetMs: this.totalBudgetMs,
      mode: this.mode,
    });
  }

  /**
   * Get elapsed time since budget creation
   */
  getElapsedTime(): number {
    return Date.now() - this.startTime;
  }

  /**
   * Get total time consumed
   * In 'delay-only' mode: only delays count
   * In 'total-time' mode: delays + execution time count
   */
  getTotalTimeConsumed(): number {
    if (this.mode === "delay-only") {
      return this.totalDelayConsumed;
    }
    return this.totalDelayConsumed + this.totalExecutionTimeConsumed;
  }

  /**
   * Get remaining budget
   */
  getRemaining(): number {
    return Math.max(0, this.totalBudgetMs - this.getTotalTimeConsumed());
  }

  /**
   * Check if delay can be consumed before retry
   * @param delayMs Delay amount in milliseconds
   * @returns Check result with remaining budget
   */
  canConsumeDelay(delayMs: number): TimeBudgetCheckResult {
    const consumed = this.getTotalTimeConsumed();
    const remaining = this.totalBudgetMs - consumed;

    if (delayMs > remaining) {
      return {
        allowed: false,
        remaining,
        reason: `Retry delay ${delayMs}ms would exceed budget (remaining: ${remaining}ms)`,
      };
    }

    return {
      allowed: true,
      remaining: remaining - delayMs,
    };
  }

  /**
   * Consume delay from budget
   * @param delayMs Delay amount in milliseconds
   * @returns Success status
   */
  consumeDelay(delayMs: number): boolean {
    const check = this.canConsumeDelay(delayMs);
    if (!check.allowed) {
      logger.warn("Delay consumption rejected", {
        name: this.name,
        requestedMs: delayMs,
        remaining: check.remaining,
        reason: check.reason,
      });
      return false;
    }

    this.totalDelayConsumed += delayMs;
    this.retryCount++;

    logger.debug("Delay consumed from budget", {
      name: this.name,
      delayMs,
      totalDelayConsumed: this.totalDelayConsumed,
      remaining: this.getRemaining(),
      retryCount: this.retryCount,
    });

    return true;
  }

  /**
   * Record execution time (only applies in 'total-time' mode)
   * @param executionTimeMs Execution time in milliseconds
   */
  recordExecutionTime(executionTimeMs: number): void {
    if (this.mode === "delay-only") {
      // In delay-only mode, execution time is not tracked against budget
      return;
    }

    this.totalExecutionTimeConsumed += executionTimeMs;

    logger.debug("Execution time recorded", {
      name: this.name,
      executionTimeMs,
      totalExecutionTime: this.totalExecutionTimeConsumed,
      remaining: this.getRemaining(),
      mode: this.mode,
    });
  }

  /**
   * Check if budget is exhausted
   */
  isExhausted(): boolean {
    return this.getTotalTimeConsumed() >= this.totalBudgetMs;
  }

  /**
   * Get budget statistics for observability
   */
  getStats(): {
    mode: TimeBudgetMode;
    totalBudgetMs: number;
    elapsedTimeMs: number;
    totalDelayConsumedMs: number;
    totalExecutionTimeConsumedMs: number;
    totalTimeConsumedMs: number;
    remainingMs: number;
    retryCount: number;
    exhausted: boolean;
  } {
    return {
      mode: this.mode,
      totalBudgetMs: this.totalBudgetMs,
      elapsedTimeMs: this.getElapsedTime(),
      totalDelayConsumedMs: this.totalDelayConsumed,
      totalExecutionTimeConsumedMs: this.totalExecutionTimeConsumed,
      totalTimeConsumedMs: this.getTotalTimeConsumed(),
      remainingMs: this.getRemaining(),
      retryCount: this.retryCount,
      exhausted: this.isExhausted(),
    };
  }

  /**
   * Reset budget (for testing purposes)
   */
  reset(): void {
    this.totalDelayConsumed = 0;
    this.totalExecutionTimeConsumed = 0;
    this.retryCount = 0;

    logger.debug("TimeBudget reset", { name: this.name });
  }
}

/**
 * Create a time budget for a retry operation
 * Helper factory function
 */
export function createTimeBudget(
  totalBudgetMs: number,
  mode: TimeBudgetMode = "delay-only",
  name?: string,
): TimeBudget {
  return new TimeBudget({
    totalBudgetMs,
    mode,
    startTime: Date.now(),
    name: name ?? `retry-budget-${Date.now()}`,
  });
}
