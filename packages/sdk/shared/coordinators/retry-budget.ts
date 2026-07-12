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
 *
 * Problem #4 Fix: Support per-branch budget allocation to prevent starvation
 * - Allocate budget shares to each branch (totalRetries / N)
 * - Per-branch canRetry() checks both global and branch budgets
 *
 * Problem #5 Fix: Support two time budget modes
 * - 'delay-only': Track retry delays only (current behavior, backward compatible)
 * - 'total-time': Track delays + execution time
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
  /** Time budget mode: 'delay-only' (default) or 'total-time' (Problem #5) */
  timeBudgetMode?: 'delay-only' | 'total-time';
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
  /** Time budget mode ('delay-only' or 'total-time') */
  timeBudgetMode: 'delay-only' | 'total-time';
  /** Time budget consumed in milliseconds */
  timeBudgetConsumed: number;
  /** Whether budget is exhausted */
  isExhausted: boolean;
}

/**
 * Per-branch budget state (Problem #4)
 */
export interface BranchBudgetState {
  /** Branch ID */
  branchId: string;
  /** Allocated retries for this branch */
  allocatedRetries: number;
  /** Retries consumed by this branch */
  retriesConsumed: number;
  /** Retries remaining for this branch */
  retriesRemaining: number;
}

/**
 * Retry Budget Manager
 *
 * Manages global retry budget for workflow execution to prevent
 * unbounded retry loops across FORK branches and AGENT_LOOP iterations.
 *
 * Problem #4: Supports per-branch budget allocation to prevent starvation
 * Problem #5: Supports configurable time budget modes
 */
export class RetryBudget {
  private totalRetries: number;
  private retriesConsumed: number = 0;
  private timeBudgetMs: number;
  private timeBudgetMode: 'delay-only' | 'total-time';
  private timeBudgetConsumedMs: number = 0;
  private executionTimeConsumedMs: number = 0;
  private verbose: boolean;

  /** Per-branch budgets (Problem #4) */
  private branchBudgets: Map<string, { allocated: number; consumed: number }> = new Map();

  constructor(config: RetryBudgetConfig) {
    if (config.totalRetries < 0) {
      throw new Error("totalRetries must be >= 0");
    }
    if (config.timeBudgetMs && config.timeBudgetMs < 0) {
      throw new Error("timeBudgetMs must be >= 0");
    }

    this.totalRetries = config.totalRetries;
    this.timeBudgetMs = config.timeBudgetMs ?? 0; // 0 means unlimited
    this.timeBudgetMode = config.timeBudgetMode ?? 'delay-only';
    this.verbose = config.verbose ?? false;

    logger.info("RetryBudget created", {
      totalRetries: this.totalRetries,
      timeBudgetMs: this.timeBudgetMs === 0 ? "unlimited" : this.timeBudgetMs,
      timeBudgetMode: this.timeBudgetMode,
    });
  }

  /**
   * Allocate per-branch budget (Problem #4)
   * Must be called BEFORE any branch starts retrying
   *
   * @param branchIds Array of branch IDs in the FORK node
   * @returns Allocated retries per branch
   */
  allocateBranchBudgets(branchIds: string[]): number {
    if (branchIds.length === 0) {
      return 0;
    }

    // Distribute total retries equally among branches
    const allocatedPerBranch = Math.floor(this.totalRetries / branchIds.length);

    for (const branchId of branchIds) {
      this.branchBudgets.set(branchId, {
        allocated: allocatedPerBranch,
        consumed: 0,
      });
    }

    if (this.verbose) {
      logger.info("Branch budgets allocated (Problem #4)", {
        branchCount: branchIds.length,
        allocatedPerBranch,
        totalAllocated: allocatedPerBranch * branchIds.length,
      });
    }

    return allocatedPerBranch;
  }

  /**
   * Check if retry is allowed within budget (both global and per-branch)
   * @param delayMs Proposed delay for this retry in milliseconds
   * @param branchId Optional branch ID (for per-branch budget check - Problem #4)
   * @param executionTimeMs Optional execution time to add to budget (for total-time mode - Problem #5)
   * @returns true if within budget, false otherwise
   */
  canRetry(
    delayMs: number = 0,
    branchId?: string,
    executionTimeMs: number = 0,
  ): boolean {
    // Check global retry count budget
    if (this.retriesConsumed >= this.totalRetries) {
      if (this.verbose) {
        logger.warn("Retry budget exhausted (global count)", {
          retriesConsumed: this.retriesConsumed,
          totalRetries: this.totalRetries,
        });
      }
      return false;
    }

    // Check per-branch budget (Problem #4)
    if (branchId) {
      const branchBudget = this.branchBudgets.get(branchId);
      if (branchBudget && branchBudget.consumed >= branchBudget.allocated) {
        if (this.verbose) {
          logger.warn("Branch retry budget exhausted (Problem #4)", {
            branchId,
            consumed: branchBudget.consumed,
            allocated: branchBudget.allocated,
          });
        }
        return false;
      }
    }

    // Check time budget (Problem #5)
    if (this.timeBudgetMs > 0) {
      let projectedBudgetMs = this.timeBudgetConsumedMs;

      if (this.timeBudgetMode === 'delay-only') {
        // Only count delay time
        projectedBudgetMs += delayMs;
      } else if (this.timeBudgetMode === 'total-time') {
        // Count delay + execution time
        projectedBudgetMs += delayMs + executionTimeMs;
      }

      if (projectedBudgetMs > this.timeBudgetMs) {
        if (this.verbose) {
          logger.warn("Retry budget exhausted (time)", {
            mode: this.timeBudgetMode,
            timeBudgetConsumedMs: this.timeBudgetConsumedMs,
            projectedBudgetMs,
            timeBudgetMs: this.timeBudgetMs,
          });
        }
        return false;
      }
    }

    return true;
  }

  /**
   * Consume retry from budget (both global and per-branch)
   * @param delayMs Delay incurred in milliseconds
   * @param branchId Optional branch ID (for per-branch budget tracking - Problem #4)
   * @param executionTimeMs Optional execution time (for total-time mode - Problem #5)
   * @returns true if consumed successfully, false if budget exhausted
   */
  consumeRetry(
    delayMs: number = 0,
    branchId?: string,
    executionTimeMs: number = 0,
  ): boolean {
    if (!this.canRetry(delayMs, branchId, executionTimeMs)) {
      return false;
    }

    // Consume from global budget
    this.retriesConsumed++;

    // Consume from per-branch budget (Problem #4)
    if (branchId) {
      const branchBudget = this.branchBudgets.get(branchId);
      if (branchBudget) {
        branchBudget.consumed++;
      }
    }

    // Update time budget consumption (Problem #5)
    this.timeBudgetConsumedMs += delayMs;
    if (this.timeBudgetMode === 'total-time') {
      this.executionTimeConsumedMs += executionTimeMs;
      this.timeBudgetConsumedMs += executionTimeMs;
    }

    if (this.verbose) {
      logger.debug("Retry consumed from budget", {
        retriesConsumed: this.retriesConsumed,
        totalRetries: this.totalRetries,
        retriesRemaining: this.totalRetries - this.retriesConsumed,
        timeBudgetMode: this.timeBudgetMode,
        timeBudgetConsumedMs: this.timeBudgetConsumedMs,
        branchId,
      });
    }

    return true;
  }

  /**
   * Get current budget state
   */
  getState(): RetryBudgetState {
    return {
      totalRetries: this.totalRetries,
      retriesConsumed: this.retriesConsumed,
      retriesRemaining: Math.max(0, this.totalRetries - this.retriesConsumed),
      timeBudgetMs: this.timeBudgetMs,
      timeBudgetMode: this.timeBudgetMode,
      timeBudgetConsumed: this.timeBudgetConsumedMs,
      isExhausted:
        this.retriesConsumed >= this.totalRetries ||
        (this.timeBudgetMs > 0 && this.timeBudgetConsumedMs >= this.timeBudgetMs),
    };
  }

  /**
   * Get per-branch budget state (Problem #4)
   */
  getBranchBudgetState(branchId: string): BranchBudgetState | null {
    const branchBudget = this.branchBudgets.get(branchId);
    if (!branchBudget) {
      return null;
    }

    return {
      branchId,
      allocatedRetries: branchBudget.allocated,
      retriesConsumed: branchBudget.consumed,
      retriesRemaining: Math.max(0, branchBudget.allocated - branchBudget.consumed),
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
    this.executionTimeConsumedMs = 0;
    this.branchBudgets.clear();
  }
}
