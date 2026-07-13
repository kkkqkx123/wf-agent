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
 * - Consistent sentinel: undefined = unlimited for both dimensions
 *
 * Problem #4 Fix: Support per-branch budget allocation to prevent starvation
 * - Allocate budget shares to each branch (totalRetries / N)
 * - Per-branch canRetry() checks both global and branch budgets
 * - Borrow-from-pool: branches can use global remaining after exhausting their allocation
 *
 * Problem #5 Fix: Support two time budget modes
 * - 'delay-only': Track retry delays only (default, backward compatible)
 * - 'total-time': Track delays + execution time
 */

import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "RetryBudget" });

export type TimeBudgetMode = 'delay-only' | 'total-time';

/**
 * Retry budget configuration
 *
 * maxRetries and timeBudgetMs each follow the same rule:
 * - undefined = unlimited (no constraint on this dimension)
 * - 0 = no capacity (immediately exhausted on this dimension)
 * - positive number = the limit
 */
export interface RetryBudgetConfig {
  /**
   * Maximum retry count. undefined = unlimited count (time-only mode).
   * 0 = no retries allowed (budget immediately exhausted on count dimension).
   */
  maxRetries?: number;
  /**
   * Optional time budget in milliseconds.
   * undefined = unlimited time.
   * 0 = no time allowed (immediately exhausted on time dimension).
   */
  timeBudgetMs?: number;
  /** Time budget mode: 'delay-only' (default) or 'total-time' (Problem #5) */
  timeBudgetMode?: TimeBudgetMode;
  /** Name for identification and logging */
  name?: string;
  /** Whether to enable detailed logging */
  verbose?: boolean;
}

/**
 * Retry budget state snapshot
 */
export interface RetryBudgetState {
  /** Total retries available (-1 means unlimited) */
  totalRetries: number;
  /** Retries consumed so far */
  retriesConsumed: number;
  /** Retries remaining */
  retriesRemaining: number;
  /** Time budget in milliseconds (-1 means unlimited) */
  timeBudgetMs: number;
  /** Time budget mode ('delay-only' or 'total-time') */
  timeBudgetMode: TimeBudgetMode;
  /** Time budget consumed in milliseconds (delay + optional execution time) */
  timeBudgetConsumed: number;
  /** Execution time consumed in milliseconds (only tracked in total-time mode) */
  executionTimeConsumedMs: number;
  /** Elapsed time since budget creation */
  elapsedTimeMs: number;
  /** Whether budget is exhausted */
  isExhausted: boolean;
  /** Total delay consumed (always delay-only portion of timeBudgetConsumed) */
  totalDelayConsumedMs: number;
  /** Remaining time budget (0 if no limit, MAX_SAFE_INTEGER if unlimited) */
  remainingMs: number;
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
 * Time budget check result — used by both canRetry() and canConsumeDelay()
 */
export interface BudgetCheckResult {
  allowed: boolean;
  remaining: number;
  reason?: string;
}

/**
 * Create a BudgetCheckResult for an allowed retry.
 */
function allowedResult(remaining: number): BudgetCheckResult {
  return { allowed: true, remaining };
}

/**
 * Create a BudgetCheckResult for a rejected retry.
 */
function deniedResult(remaining: number, reason: string): BudgetCheckResult {
  return { allowed: false, remaining, reason };
}

/**
 * Retry Budget Manager
 *
 * Manages global retry budget for workflow/agent execution to prevent
 * unbounded retry loops across FORK branches and AGENT_LOOP iterations.
 *
 * Two dimensions: retry count (maxRetries) and time (timeBudgetMs).
 * Both use the same semantic: undefined = unlimited, 0 = no capacity.
 *
 * Problem #4: Supports per-branch budget allocation with global pool borrowing
 * Problem #5: Supports configurable time budget modes
 */
export class RetryBudget {
  /** Maximum retry count. undefined means unlimited. */
  private maxRetries: number | undefined;
  private retriesConsumed: number = 0;
  /** Time budget in milliseconds. undefined means unlimited. */
  private timeBudgetMs: number | undefined;
  private timeBudgetMode: TimeBudgetMode;
  private timeBudgetConsumedMs: number = 0;
  private executionTimeConsumedMs: number = 0;
  private readonly startTime: number;
  private readonly name: string;
  private verbose: boolean;

  /** Per-branch budgets (Problem #4) */
  private branchBudgets: Map<string, { allocated: number; consumed: number }> = new Map();
  /** Total retries allocated to all branches (used for pool calculation) */
  private totalBranchAllocated: number = 0;

  constructor(config: RetryBudgetConfig) {
    if (config.maxRetries !== undefined && config.maxRetries < 0) {
      throw new Error("maxRetries must be >= 0 or undefined");
    }
    if (config.timeBudgetMs !== undefined && config.timeBudgetMs < 0) {
      throw new Error("timeBudgetMs must be >= 0");
    }

    // undefined = unlimited for both dimensions
    this.maxRetries = config.maxRetries === undefined ? undefined : config.maxRetries;
    this.timeBudgetMs = config.timeBudgetMs === undefined ? undefined : config.timeBudgetMs;
    this.timeBudgetMode = config.timeBudgetMode ?? 'delay-only';
    this.verbose = config.verbose ?? false;
    this.startTime = Date.now();
    this.name = config.name ?? 'RetryBudget';

    logger.info("RetryBudget created", {
      name: this.name,
      maxRetries: this.maxRetries === undefined ? 'unlimited' : String(this.maxRetries),
      timeBudgetMs: this.timeBudgetMs === undefined ? 'unlimited' : String(this.timeBudgetMs),
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
   * Must be called BEFORE any branch starts retrying.
   *
   * @param branchIds Array of branch IDs in the FORK node
   * @returns Allocated retries per branch
   */
  allocateBranchBudgets(branchIds: string[]): number {
    if (branchIds.length === 0) {
      return 0;
    }

    // In unlimited count mode, skip per-branch budget allocation
    if (this.maxRetries === undefined) {
      return Number.MAX_SAFE_INTEGER;
    }

    // Distribute total retries equally among branches (floor)
    const allocatedPerBranch = Math.floor(this.maxRetries / branchIds.length);

    // Track new branches for pool calculation
    const existingCount = this.branchBudgets.size;
    for (const branchId of branchIds) {
      // Skip branches that already have a budget allocated to prevent re-allocation reset
      if (this.branchBudgets.has(branchId)) {
        continue;
      }
      this.branchBudgets.set(branchId, {
        allocated: allocatedPerBranch,
        consumed: 0,
      });
    }

    // Update total allocated
    const newBranchCount = this.branchBudgets.size - existingCount;
    this.totalBranchAllocated += newBranchCount * allocatedPerBranch;

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
   * Check if retry is allowed within budget (both global and per-branch).
   *
   * Returns a BudgetCheckResult with:
   * - allowed: whether the retry is allowed
   * - remaining: remaining time budget
   * - reason: if denied, explains which constraint was hit
   *
   * Checks in order:
   * 1. Global retry count
   * 2. Per-branch count (with pool borrowing)
   * 3. Time budget (mode-dependent)
   *
   * @param delayMs Proposed delay for this retry in milliseconds
   * @param branchId Optional branch ID (for per-branch budget check - Problem #4)
   * @param executionTimeMs Optional execution time to add to budget (for total-time mode - Problem #5)
   */
  canRetry(
    delayMs: number = 0,
    branchId?: string,
    executionTimeMs: number = 0,
  ): BudgetCheckResult {
    // 1. Check global retry count budget (skip if unlimited: maxRetries is undefined)
    if (this.maxRetries !== undefined && this.retriesConsumed >= this.maxRetries) {
      if (this.verbose) {
        logger.warn("Retry budget exhausted (global count)", {
          retriesConsumed: this.retriesConsumed,
          maxRetries: this.maxRetries,
        });
      }
      return deniedResult(
        0,
        `Retry count budget exhausted (${this.retriesConsumed}/${this.maxRetries})`,
      );
    }

    // 2. Check per-branch budget with pool borrowing (Problem #4)
    if (branchId) {
      const branchBudget = this.branchBudgets.get(branchId);
      if (branchBudget && branchBudget.consumed >= branchBudget.allocated) {
        // Branch allocation exhausted — check if pool retries are available.
        // Pool = maxRetries - totalBranchAllocated (unallocated remainder from floor division).
        // A branch can only borrow from the unallocated pool, NOT from other branches' allocations.
        const poolSize = this.maxRetries !== undefined
          ? Math.max(0, this.maxRetries - this.totalBranchAllocated)
          : 0;

        if (poolSize <= 0) {
          // No pool to borrow from — truly denied
          if (this.verbose) {
            logger.warn("Branch retry budget exhausted (no pool available, Problem #4)", {
              branchId,
              consumed: branchBudget.consumed,
              allocated: branchBudget.allocated,
            });
          }
          return deniedResult(
            0,
            `Branch ${branchId} retry budget exhausted (${branchBudget.consumed}/${branchBudget.allocated}, no pool available)`,
          );
        }

        // Pool exists — check if it has been fully consumed
        // Pool consumed = total retries consumed by branches beyond their allocation
        let poolConsumed = 0;
        for (const bb of this.branchBudgets.values()) {
          poolConsumed += Math.max(0, bb.consumed - bb.allocated);
        }
        const poolRemaining = poolSize - poolConsumed;

        if (poolRemaining <= 0) {
          if (this.verbose) {
            logger.warn("Branch retry budget exhausted (pool consumed, Problem #4)", {
              branchId,
              consumed: branchBudget.consumed,
              allocated: branchBudget.allocated,
              poolSize,
              poolConsumed,
            });
          }
          return deniedResult(
            0,
            `Branch ${branchId} retry budget exhausted (pool fully consumed)`,
          );
        }
        // Pool has remaining — allow borrowing
      }
    }

    // 3. Check time budget (Problem #5)
    if (this.timeBudgetMs !== undefined) {
      if (this.timeBudgetMs === 0) {
        // No time budget allowed at all
        return deniedResult(0, "Time budget is 0 — no time allowed for retries");
      }

      let projectedBudgetMs = this.timeBudgetConsumedMs;

      if (this.timeBudgetMode === 'delay-only') {
        projectedBudgetMs += delayMs;
      } else if (this.timeBudgetMode === 'total-time') {
        projectedBudgetMs += delayMs + executionTimeMs;
      }

      if (projectedBudgetMs > this.timeBudgetMs) {
        const remaining = Math.max(0, this.timeBudgetMs - this.timeBudgetConsumedMs);
        if (this.verbose) {
          logger.warn("Retry budget exhausted (time)", {
            mode: this.timeBudgetMode,
            timeBudgetConsumedMs: this.timeBudgetConsumedMs,
            projectedBudgetMs,
            timeBudgetMs: this.timeBudgetMs,
          });
        }
        return deniedResult(remaining, `Retry delay would exceed time budget (remaining: ${remaining}ms)`);
      }

      const remaining = Math.max(0, this.timeBudgetMs - projectedBudgetMs);
      return allowedResult(remaining);
    }

    // No time limit — unlimited remaining
    return allowedResult(Number.MAX_SAFE_INTEGER);
  }

  /**
   * Consume retry from budget (both global and per-branch)
   *
   * There are two usage patterns:
   * 1. FORK/workflow path: pass executionTimeMs directly with consumeRetry
   *    → consumeRetry(delayMs, branchId, executionTimeMs)
   *    In total-time mode, this adds (delayMs + executionTimeMs) to timeBudgetConsumedMs.
   *
   * 2. Agent path: use recordExecutionTime() separately, then consumeRetry without executionTimeMs
   *    → recordExecutionTime(execTime)
   *    → consumeRetry(delayMs, branchId)  // executionTimeMs defaults to 0
   *    The agent path separates execution time tracking (tracks every attempt's duration)
   *    from retry budget consumption (only deducts delay on retry).
   *
   * @param delayMs Delay incurred in milliseconds
   * @param branchId Optional branch ID (for per-branch budget tracking - Problem #4)
   * @param executionTimeMs Optional execution time (for total-time mode - Problem #5).
   *                        Use 0 (default) when using agent path with recordExecutionTime.
   * @returns true if consumed successfully, false if budget exhausted
   */
  consumeRetry(
    delayMs: number = 0,
    branchId?: string,
    executionTimeMs: number = 0,
  ): boolean {
    const check = this.canRetry(delayMs, branchId, executionTimeMs);
    if (!check.allowed) {
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
   *
   * Call pattern: recordExecutionTime(execTime) + consumeRetry(delayMs)
   * The total-time consumed = previously recorded executionTime + delayMs from consumeRetry.
   * In delay-only mode this call is a no-op.
   */
  recordExecutionTime(executionTimeMs: number): void {
    if (executionTimeMs < 0) {
      logger.warn("recordExecutionTime called with negative value", { executionTimeMs });
      return;
    }
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
   * Get current budget state — the single source of truth for snapshot data.
   *
   * Replaces both the old getState() and getStats() by including all
   * derived fields on RetryBudgetState.
   */
  getState(): RetryBudgetState {
    const countExhausted = this.maxRetries !== undefined && this.retriesConsumed >= this.maxRetries;
    const timeExhausted = this.timeBudgetMs !== undefined && this.timeBudgetConsumedMs >= this.timeBudgetMs;

    // Derived values
    const totalDelayConsumedMs = this.timeBudgetConsumedMs - this.executionTimeConsumedMs;
    const remainingMs = this.timeBudgetMs !== undefined
      ? Math.max(0, this.timeBudgetMs - this.timeBudgetConsumedMs)
      : Number.MAX_SAFE_INTEGER;

    return {
      totalRetries: this.maxRetries === undefined ? -1 : this.maxRetries,
      retriesConsumed: this.retriesConsumed,
      retriesRemaining: this.getRetriesRemaining(),
      timeBudgetMs: this.timeBudgetMs === undefined ? -1 : this.timeBudgetMs,
      timeBudgetMode: this.timeBudgetMode,
      timeBudgetConsumed: this.timeBudgetConsumedMs,
      executionTimeConsumedMs: this.executionTimeConsumedMs,
      elapsedTimeMs: this.getElapsedTime(),
      isExhausted: countExhausted || timeExhausted,
      // New derived fields (formerly in getStats())
      totalDelayConsumedMs,
      remainingMs,
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
    if (this.maxRetries === undefined) return Number.MAX_SAFE_INTEGER;
    return Math.max(0, this.maxRetries - this.retriesConsumed);
  }

  /**
   * Get name for identification
   */
  getName(): string {
    return this.name;
  }

  /**
   * Reset budget consumption.
   *
   * @param resetStartTime When true, also resets the elapsed-time clock (default: false)
   */
  reset(resetStartTime: boolean = false): void {
    this.retriesConsumed = 0;
    this.timeBudgetConsumedMs = 0;
    this.executionTimeConsumedMs = 0;
    this.branchBudgets.clear();
    this.totalBranchAllocated = 0;
    if (resetStartTime) {
      (this as unknown as { startTime: number }).startTime = Date.now();
    }
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
