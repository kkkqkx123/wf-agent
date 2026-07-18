/**
 * ErrorChainManager - Shared Error Chain and Pattern Analysis
 *
 * Encapsulates the common error chain building, traversal, and pattern analysis
 * logic that was previously duplicated between WorkflowExecutionState and AgentLoopState.
 *
 * Design Principles:
 * - Pure data transformation: no side effects, no external dependencies
 * - Composable: works with any ExecutionErrorRecord array
 * - Domain-neutral: the caller provides domain-specific aggregation callbacks
 *
 * Usage:
 *   const errorRecord = ErrorChainManager.recordError(errorRecords, newError);
 *   const chain = ErrorChainManager.getErrorChain(errorRecords, errorId);
 *   const root = ErrorChainManager.getRootCauseError(errorRecords);
 *   const pattern = ErrorChainManager.analyzeErrorPattern(errorRecords, {
 *     getItemKey: (err) => err.nodeId,  // or (err) => err.context.toolName
 *     itemKey: 'nodeProblems',          // or 'toolProblems'
 *   });
 */

import type { ExecutionErrorRecord } from "@wf-agent/types";

/**
 * Error pattern analysis result (base type)
 * Domain-specific consumers extend this with their own item fields
 */
export interface ErrorPattern {
  type: "none" | "single" | "chain";
  count: number;
  errors: ExecutionErrorRecord[];
  typeDistribution: Record<string, number>;
  severityBreakdown: Record<string, number>;
}

/**
 * Configuration for error pattern analysis
 */
export interface ErrorPatternConfig {
  /** Function to extract the item key from an error record */
  getItemKey: (error: ExecutionErrorRecord) => string | undefined;
  /** Key name for the items array in the result (e.g. "nodeProblems", "toolProblems") */
  itemKey: string;
  /** Max number of top items to include */
  maxItems?: number;
  /** Function to create an item from [key, count] */
  buildItem?: (key: string, count: number) => { [k: string]: string | number };
}

/**
 * Shared error chain management utilities
 */
export class ErrorChainManager {
  /**
   * Record an error with automatic error chain building.
   *
   * Automatically establishes relationships between errors:
   * - Sets parentErrorId to the last error in the records
   * - Builds errorChain array
   * - Identifies root cause
   *
   * @param errorRecords - The current list of error records (mutated in place)
   * @param error - The new error to record
   * @returns The recorded error with chain fields populated
   */
  static recordError(
    errorRecords: ExecutionErrorRecord[],
    error: ExecutionErrorRecord,
  ): ExecutionErrorRecord {
    // 1. Standardize error ID if not provided
    const errorId = error.id || `error:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`;
    error.id = errorId;

    // 2. Build error chain relationships
    //    Uses explicit parentErrorId if provided (causal link); otherwise falls
    //    back to sequential ordering (link to last recorded error).
    if (errorRecords.length > 0) {
      // 2a. Resolve parent: prefer explicit parentErrorId, fall back to last record
      const parentError = error.parentErrorId
        ? errorRecords.find(e => e.id === error.parentErrorId)
        : undefined;
      const parent = parentError ?? errorRecords[errorRecords.length - 1]!;

      // Only set parentErrorId if not already explicitly provided
      if (!error.parentErrorId) {
        error.parentErrorId = parent.id;
      }

      // 2b. Build error chain from parent's chain or start new chain
      if (parent.errorChain) {
        error.errorChain = [...parent.errorChain, errorId];
      } else {
        // First time establishing chain
        error.errorChain = [parent.id, errorId];
      }

      // 2c. Quick reference to root cause
      error.rootCauseId = parent.rootCauseId || parent.id;
    } else {
      // This is the first error, it is the root cause
      error.errorChain = [errorId];
      error.rootCauseId = errorId;
    }

    // 3. Add to records
    errorRecords.push(error);
    return error;
  }

  /**
   * Get the complete error chain for a specific error.
   *
   * Returns all errors in the chain starting from the root cause
   * up to and including the specified error.
   *
   * @param errorRecords - All error records
   * @param fromErrorId - Error ID to get chain for (default: last error)
   * @returns Array of errors in chain order
   */
  static getErrorChain(
    errorRecords: ExecutionErrorRecord[],
    fromErrorId?: string,
  ): ExecutionErrorRecord[] {
    if (errorRecords.length === 0) {
      return [];
    }

    const targetErrorId = fromErrorId || errorRecords[errorRecords.length - 1]!.id;
    const targetError = errorRecords.find(e => e.id === targetErrorId);

    if (!targetError) {
      return [];
    }

    if (!targetError.errorChain) {
      return [targetError];
    }

    return targetError.errorChain
      .map(id => errorRecords.find(e => e.id === id))
      .filter((e): e is ExecutionErrorRecord => Boolean(e));
  }

  /**
   * Get the root cause error.
   *
   * Returns the first error in the chain that triggered all subsequent errors.
   *
   * @param errorRecords - All error records
   * @returns Root cause error, or null if no errors
   */
  static getRootCauseError(errorRecords: ExecutionErrorRecord[]): ExecutionErrorRecord | null {
    if (errorRecords.length === 0) {
      return null;
    }

    const lastError = errorRecords[errorRecords.length - 1];
    if (!lastError) {
      return null;
    }

    const rootCauseId = lastError.rootCauseId || lastError.id;
    return errorRecords.find(e => e.id === rootCauseId) || lastError;
  }

  /**
   * Analyze error pattern across all recorded errors.
   *
   * Provides distribution by type, affected items (nodes/tools), and severity breakdown.
   *
   * @param errorRecords - All error records
   * @param config - Domain-specific configuration for item extraction
   * @returns Error pattern analysis
   */
  static analyzeErrorPattern(
    errorRecords: ExecutionErrorRecord[],
    config: ErrorPatternConfig,
  ): ErrorPattern {
    if (errorRecords.length === 0) {
      return {
        type: "none" as const,
        count: 0,
        errors: [],
        typeDistribution: {},
        [config.itemKey]: [],
        severityBreakdown: {},
      };
    }

    const errors = errorRecords;
    const typeCount: Record<string, number> = {};
    const itemCount: Record<string, number> = {};
    const severityCount: Record<string, number> = {};

    errors.forEach(err => {
      typeCount[err.errorType] = (typeCount[err.errorType] ?? 0) + 1;
      const itemKey = config.getItemKey(err);
      if (itemKey) {
        itemCount[itemKey] = (itemCount[itemKey] ?? 0) + 1;
      }
      severityCount[err.severity] = (severityCount[err.severity] ?? 0) + 1;
    });

    const maxItems = config.maxItems ?? 5;
    const buildItem = config.buildItem ?? ((key: string, count: number) => ({ name: key, count }));

    return {
      type: errors.length > 1 ? "chain" as const : "single" as const,
      count: errors.length,
      errors,
      typeDistribution: typeCount,
      [config.itemKey]: Object.entries(itemCount)
        .sort(([, a], [, b]) => b - a)
        .slice(0, maxItems)
        .map(([key, count]) => buildItem(key, count)),
      severityBreakdown: severityCount,
    };
  }

  /**
   * Get recommended recovery action based on error chain analysis.
   *
   * Workflow-specific heuristic: checks last error's recoverable flag,
   * recovery action hint, error chain length, and repeated tool errors.
   *
   * @param errorRecords - All error records
   * @returns Recommended action
   */
  static getRecommendedRecoveryActionWorkflow(
    errorRecords: ExecutionErrorRecord[],
  ): "retry" | "fallback" | "manual_intervention" | "abort" {
    if (errorRecords.length === 0) return "retry";

    const lastError = errorRecords[errorRecords.length - 1];
    if (!lastError) return "retry";

    // If the last error is recoverable, suggest retry
    if (lastError.isRecoverable) return "retry";

    // If recovery action was specified in the error, use it
    if (lastError.recoveryAction) {
      return lastError.recoveryAction as "retry" | "fallback" | "manual_intervention" | "abort";
    }

    // Check error chain patterns
    if (errorRecords.length >= 3) {
      // Multiple errors in chain suggests deeper issues
      return "manual_intervention";
    }

    // Check for repeated tool errors
    const toolErrors = errorRecords.filter(
      e => e.errorType === "tool_error" || e.context?.toolName,
    );
    if (toolErrors.length >= 2) {
      return "fallback";
    }

    return "abort";
  }

  /**
   * Get recommended recovery action based on error chain analysis.
   *
   * Agent-specific heuristic: counts recoveryAction hints across all errors
   * and returns the most common one.
   *
   * @param errorRecords - All error records
   * @returns Recommended action
   */
  static getRecommendedRecoveryActionAgent(
    errorRecords: ExecutionErrorRecord[],
  ): "retry" | "fallback" | "manual_intervention" | "abort" {
    if (errorRecords.length === 0) {
      return "abort";
    }

    // Check if all recoverable errors suggest the same action
    const retryCount = errorRecords.filter(e => e.recoveryAction === "retry").length;
    const fallbackCount = errorRecords.filter(e => e.recoveryAction === "fallback").length;
    const skipCount = errorRecords.filter(e => e.recoveryAction === "skip").length;

    // Prefer the most common recovery action
    if (retryCount >= fallbackCount && retryCount >= skipCount) {
      return "retry";
    }
    if (fallbackCount >= skipCount) {
      return "fallback";
    }
    if (skipCount > 0) {
      return "retry"; // Use retry as fallback for skip
    }

    return "manual_intervention";
  }
}