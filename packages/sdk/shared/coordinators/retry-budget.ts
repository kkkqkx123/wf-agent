/**
 * Retry Budget - Global budget management for retries across workflow execution
 *
 * Prevents unbounded retry spending by tracking a global retry budget
 * that is shared across all FORK branches, AGENT_LOOP iterations, and other retryable operations.
 *
 * Design Principles:
 * - Single global budget per workflow execution
 * - Decrement on each retry attempt
 * - Prevent retries when budget exhausted
 * - Observable budget consumption via metrics
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "retry-budget" });

/**
 * Retry budget configuration
 */
export interface RetryBudgetConfig {
  /** Total retry budget in count (e.g., 10 = allow 10 retries across entire workflow) */
  totalRetries: number;
  /** Optional time budget in milliseconds (0 = unlimited time) */
  timeBudgetMs?: number;
  /** Whether to enable detailed logging */
  verbose?: boolean;
}

/**
 * Retry budget state
 */
export interface RetryBudgetState {
  /** Total retries available */
  totalRetries: number;
  /** Retries consumed so far */
  retriesConsumed: number;
  /** Retries remaining */
  retriesRemaining: number;
  /** Time budget in milliseconds (0 = unlimited) */
  timeBudgetMs: number;
  /** Time budget consumed in milliseconds */
  timeBudgetConsumed: number;
  /** Whether budget is exhausted */
  isExhausted: boolean;
}

/**
 * Retry Budget Manager
 *
 * Manages global retry budget for workflow execution to prevent
 * unbounded retry loops across FORK branches and AGENT_LOOP iterations.
 */
export class RetryBudget {
  private totalRetries: number;
  private retriesConsumed: number = 0;
  private timeBudgetMs: number;
  private timeBudgetConsumedMs: number = 0;
  private verbose: boolean;

  constructor(config: RetryBudgetConfig) {
    if (config.totalRetries < 0) {
      throw new Error("totalRetries must be >= 0");
    }
    if (config.timeBudgetMs && config.timeBudgetMs < 0) {
      throw new Error("timeBudgetMs must be >= 0");
    }

    this.totalRetries = config.totalRetries;
    this.timeBudgetMs = config.timeBudgetMs ?? 0; // 0 means unlimited
    this.verbose = config.verbose ?? false;

    logger.info("RetryBudget created", {
      totalRetries: this.totalRetries,
      timeBudgetMs: this.timeBudgetMs === 0 ? "unlimited" : this.timeBudgetMs,
    });
  }

  /**
   * Check if retry is allowed within budget
   * @param delayMs Proposed delay for this retry in milliseconds
   * @returns true if within budget, false otherwise
   *
   * NOTE: Time budget tracks RETRY DELAYS ONLY, not execution time.
   * This allows retries to be bounded independently of how long execution takes.
   */
  canRetry(delayMs: number = 0): boolean {
    // Check retry count budget
    if (this.retriesConsumed >= this.totalRetries) {
      if (this.verbose) {
        logger.warn("Retry budget exhausted (count)", {
          retriesConsumed: this.retriesConsumed,
          totalRetries: this.totalRetries,
        });
      }
      return false;
    }

    // Check time budget (accumulated delays only, not wall-clock time)
    if (this.timeBudgetMs > 0) {
      const projectedDelayMs = this.timeBudgetConsumedMs + delayMs;

      if (projectedDelayMs > this.timeBudgetMs) {
        if (this.verbose) {
          logger.warn("Retry budget exhausted (time)", {
            timeBudgetConsumedMs: this.timeBudgetConsumedMs,
            projectedDelayMs,
            timeBudgetMs: this.timeBudgetMs,
          });
        }
        return false;
      }
    }

    return true;
  }

  /**
   * Consume retry from budget
   * @param delayMs Delay incurred in milliseconds
   * @returns true if consumed successfully, false if budget exhausted
   */
  consumeRetry(delayMs: number = 0): boolean {
    if (!this.canRetry(delayMs)) {
      return false;
    }

    this.retriesConsumed++;
    this.timeBudgetConsumedMs += delayMs;

    if (this.verbose) {
      logger.debug("Retry consumed from budget", {
        retriesConsumed: this.retriesConsumed,
        totalRetries: this.totalRetries,
        retriesRemaining: this.totalRetries - this.retriesConsumed,
        timeBudgetConsumedMs: this.timeBudgetConsumedMs,
      });
    }

    return true;
  }

  /**
   * Get current budget state
   *
   * NOTE: Time budget consumed tracks RETRY DELAYS ONLY, not wall-clock execution time.
   * This allows retries to be bounded independently of how long execution takes.
   */
  getState(): RetryBudgetState {
    return {
      totalRetries: this.totalRetries,
      retriesConsumed: this.retriesConsumed,
      retriesRemaining: Math.max(0, this.totalRetries - this.retriesConsumed),
      timeBudgetMs: this.timeBudgetMs,
      timeBudgetConsumed: this.timeBudgetConsumedMs,
      isExhausted:
        this.retriesConsumed >= this.totalRetries ||
        (this.timeBudgetMs > 0 && this.timeBudgetConsumedMs >= this.timeBudgetMs),
    };
  }

  /**
   * Get retries remaining
   */
  getRetriesRemaining(): number {
    return Math.max(0, this.totalRetries - this.retriesConsumed);
  }

  /**
   * Check if exhausted
   */
  isExhausted(): boolean {
    const state = this.getState();
    return state.isExhausted;
  }

  /**
   * Reset budget (rarely used, mainly for testing)
   */
  reset(): void {
    this.retriesConsumed = 0;
    this.timeBudgetConsumedMs = 0;
  }
}
