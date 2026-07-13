/**
 * Retry Budget - Global budget management for retries across agent/workflow execution
 *
 * Unified retry budget that supports both count-based and time-based budgeting:
 * - Agent: time-only mode (unlimited retry count, only time budget matters)
 * - Workflow: count + optional time budget, with per-branch allocation
 *
 * Prevents unbounded retry spending by tracking a global retry budget
 * that is shared across all FORK branches, AGENT_LOOP iterations, and other retryable operations.
 *
 * Design Principles:
 * - Single global budget per workflow/agent execution
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

const logger = createContextualLogger({ component: "RetryBudget" });

export type TimeBudgetMode = 'delay-only' | 'total-time';

/**
 * Retry budget configuration
 *
 * When maxRetries is undefined, the budget is time-only (no retry count limit).
 * When timeBudgetMs is 0 or undefined, there is no time limit (count-only).
 * When both are set, both dimensions constrain retries.
 */
export interface RetryBudgetConfig {
  /**
   * Maximum retry count. undefined = unlimited count (time-only mode).
   * 0 = no retries allowed (budget immediately exhausted on count dimension).
   */
  maxRetries?: number;
  /** Optional time budget in milliseconds (0 or undefined = unlimited time) */
  timeBudgetMs?: number;
  /** Time budget mode: 'delay-only' (default) or 'total-time' (Problem #5) */
  timeBudgetMode?: TimeBudgetMode;
  /** Name for identification and logging */
  name?: string;
  /** Whether to enable detailed logging */
  verbose?: boolean;
}

/**
 * Retry budget state
 */
export interface RetryBudgetState {
  /** Total retries available (0 means unlimited) */
  totalRetries: number;
  /** Retries consumed so far */
  retriesConsumed: number;
  /** Retries remaining */
  retriesRemaining: number;
  /** Time budget in milliseconds (0 = unlimited) */
  timeBudgetMs: number;
  /** Time budget mode ('delay-only' or 'total-time') */
  timeBudgetMode: TimeBudgetMode;
  /** Time budget consumed in milliseconds (delay only) */
  timeBudgetConsumed: number;
  /** Execution time consumed in milliseconds (only tracked in total-time mode) */
  executionTimeConsumedMs: number;
  /** Elapsed time since budget creation */
  elapsedTimeMs: number;
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
 * Time budget check result (for agent-style usage)
 */
export interface BudgetCheckResult {
  allowed: boolean;
  remaining: number;
  reason?: string;
}

/**
 * Retry Budget Manager
 *
 * Manages global retry budget for workflow/agent execution to prevent
 * unbounded retry loops across FORK branches and AGENT_LOOP iterations.
 *
 * Problem #4: Supports per-branch budget allocation to prevent starvation
 * Problem #5: Supports configurable time budget modes
 */
export class RetryBudget {
  /** Maximum retry count. < 0 means unlimited. */
  private maxRetries: number;
  private retriesConsumed: number = 0;
  private timeBudgetMs: number;
  private timeBudgetMode: TimeBudgetMode;
  private timeBudgetConsumedMs: number = 0;
  private executionTimeConsumedMs: number = 0;
  private readonly startTime: number;
  private readonly name: string;
  private verbose: boolean;

  /** Per-branch budgets (Problem #4) */
  private branchBudgets: Map<string, { allocated: number; consumed: number }> = new Map();

  constructor(config: RetryBudgetConfig) {
    if (config.maxRetries !== undefined && config.maxRetries < 0) {
      throw new Error("maxRetries must be >= 0 or undefined");
    }
    if (config.timeBudgetMs && config.timeBudgetMs < 0) {
      throw new Error("timeBudgetMs must be >= 0");
    }

    // Internally store maxRetries. -1 means unlimited (no count check).
    this.maxRetries = config.maxRetries ?? -1;
    this.timeBudgetMs = config.timeBudgetMs ?? 0; // 0 means unlimited
    this.timeBudgetMode = config.timeBudgetMode ?? 'delay-only';
    this.verbose = config.verbose ?? false;
    this.startTime = Date.now();
    this.name = config.name ?? 'RetryBudget';

    logger.info("RetryBudget created", {
      name: this.name,
      maxRetries: this.maxRetries < 0 ? 'unlimited' : this.maxRetries,
      timeBudgetMs: this.timeBudgetMs === 0 ? 'unlimited' : this.timeBudgetMs,
      timeBudgetMode: this.timeBudgetMode,
    });
  }

  /**
   * Get elapsed time since budget creation
   */
  getElapsedTime(): number {
    return Date.now() - this.startTime;
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

    // In unlimited count mode, skip per-branch budget allocation
    if (this.maxRetries < 0) {
      return Number.MAX_SAFE_INTEGER;
    }

    // Distribute total retries equally among branches
    const allocatedPerBranch = Math.floor(this.maxRetries / branchIds.length);

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
    // Check global retry count budget (skip if unlimited: maxRetries < 0)
    if (this.maxRetries >= 0 && this.retriesConsumed >= this.maxRetries) {
      if (this.verbose) {
        logger.warn("Retry budget exhausted (global count)", {
          retriesConsumed: this.retriesConsumed,
          maxRetries: this.maxRetries,
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
   * Check if a delay can be consumed (agent-style check with rich result).
   * Returns a result with reason string if not allowed.
   */
  canConsumeDelay(delayMs: number): BudgetCheckResult {
    if (this.timeBudgetMs > 0) {
      const projected = this.timeBudgetConsumedMs + delayMs;
      if (projected > this.timeBudgetMs) {
        return {
          allowed: false,
          remaining: Math.max(0, this.timeBudgetMs - this.timeBudgetConsumedMs),
          reason: `Retry delay ${delayMs}ms would exceed time budget (remaining: ${Math.max(0, this.timeBudgetMs - this.timeBudgetConsumedMs)}ms)`,
        };
      }
    }
    return {
      allowed: true,
      remaining: this.timeBudgetMs > 0
        ? Math.max(0, this.timeBudgetMs - this.timeBudgetConsumedMs - delayMs)
        : Number.MAX_SAFE_INTEGER,
    };
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
        maxRetries: this.maxRetries,
        retriesRemaining: this.getRetriesRemaining(),
        timeBudgetMode: this.timeBudgetMode,
        timeBudgetConsumedMs: this.timeBudgetConsumedMs,
        branchId,
      });
    }

    return true;
  }

  /**
   * Record execution time against the time budget (total-time mode only).
   * Unlike consumeRetry(), this does NOT consume a retry count slot.
   * This is used for agent-style usage where execution time is tracked
   * after each attempt (successful or failed), separate from retry count.
   */
  recordExecutionTime(executionTimeMs: number): void {
    if (this.timeBudgetMode !== 'total-time') {
      return;
    }
    this.executionTimeConsumedMs += executionTimeMs;
    this.timeBudgetConsumedMs += executionTimeMs;

    logger.debug("Execution time recorded", {
      name: this.name,
      executionTimeMs,
      totalExecutionTimeMs: this.executionTimeConsumedMs,
      timeBudgetConsumedMs: this.timeBudgetConsumedMs,
      mode: this.timeBudgetMode,
    });
  }

  /**
   * Check if budget is exhausted
   */
  isExhausted(): boolean {
    const state = this.getState();
    return state.isExhausted;
  }

  /**
   * Get current budget state (backward compatible)
   */
  getState(): RetryBudgetState {
    const countExhausted = this.maxRetries >= 0 && this.retriesConsumed >= this.maxRetries;
    const timeExhausted = this.timeBudgetMs > 0 && this.timeBudgetConsumedMs >= this.timeBudgetMs;
    return {
      totalRetries: this.maxRetries < 0 ? 0 : this.maxRetries,
      retriesConsumed: this.retriesConsumed,
      retriesRemaining: this.getRetriesRemaining(),
      timeBudgetMs: this.timeBudgetMs,
      timeBudgetMode: this.timeBudgetMode,
      timeBudgetConsumed: this.timeBudgetConsumedMs,
      executionTimeConsumedMs: this.executionTimeConsumedMs,
      elapsedTimeMs: this.getElapsedTime(),
      isExhausted: countExhausted || timeExhausted,
    };
  }

  /**
   * Rich stats for observability (agent-friendly)
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
    totalRetries: number;
    retriesConsumed: number;
  } {
    const totalTimeConsumed = this.timeBudgetMode === 'delay-only'
      ? this.timeBudgetConsumedMs
      : this.timeBudgetConsumedMs;
    return {
      mode: this.timeBudgetMode,
      totalBudgetMs: this.timeBudgetMs,
      elapsedTimeMs: this.getElapsedTime(),
      totalDelayConsumedMs: this.timeBudgetConsumedMs - this.executionTimeConsumedMs,
      totalExecutionTimeConsumedMs: this.executionTimeConsumedMs,
      totalTimeConsumedMs: totalTimeConsumed,
      remainingMs: this.timeBudgetMs > 0
        ? Math.max(0, this.timeBudgetMs - this.timeBudgetConsumedMs)
        : Number.MAX_SAFE_INTEGER,
      retryCount: this.retriesConsumed,
      exhausted: this.isExhausted(),
      totalRetries: this.maxRetries < 0 ? 0 : this.maxRetries,
      retriesConsumed: this.retriesConsumed,
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
    if (this.maxRetries < 0) return Number.MAX_SAFE_INTEGER;
    return Math.max(0, this.maxRetries - this.retriesConsumed);
  }

  /**
   * Get name for identification
   */
  getName(): string {
    return this.name;
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

/**
 * Create a retry budget with convenient defaults.
 * When only timeBudgetMs is provided, creates a time-only budget (no count limit).
 * When only maxRetries is provided, creates a count-only budget (no time limit).
 * When both are provided, both dimensions are enforced.
 */
export function createRetryBudget(
  config: RetryBudgetConfig,
): RetryBudget {
  return new RetryBudget(config);
}
