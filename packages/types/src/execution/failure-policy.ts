/**
 * Failure Policy Framework - Unified Error Handling Strategy
 *
 * Provides a consistent interface for handling failures across all execution types:
 * - Nodes (SUBGRAPH, FORK, AGENT_LOOP, etc.)
 * - Child executions (branches, sub-workflows)
 * - Tool executions
 *
 * Replaces ad-hoc error handling with declarative policies.
 */

import type { ErrorSeverity } from "../errors/base.js";
import type { RuntimeNode } from "../node/runtime-node-types.js";

// ============================================================================
// Enums and Constants
// ============================================================================

/**
 * Failure handling actions
 */
export enum FailureAction {
  /** Continue execution, ignore the failure */
  CONTINUE = "continue",

  /** Retry the execution */
  RETRY = "retry",

  /** Use fallback value */
  FALLBACK = "fallback",

  /** Stop execution, propagate error */
  FAIL = "fail",
}

// ============================================================================
// Retry Policy
// ============================================================================

/**
 * Retry configuration and strategy
 */
export interface RetryPolicy {
  /** Whether retry is enabled */
  enabled: boolean;

  /** Maximum number of retry attempts */
  maxRetries: number;

  /** Base delay between retries in milliseconds */
  baseDelay: number;

  /** Exponential backoff multiplier */
  backoffMultiplier: number;

  /** Maximum delay in milliseconds */
  maxDelay: number;

  /** Whether to add random jitter to delays */
  jitter: boolean;

  /**
   * Determine if the error should be retried
   * @param error The error that occurred
   * @param attemptCount Current attempt number (0-indexed)
   * @returns true if should retry, false otherwise
   */
  shouldRetry(error: Error, attemptCount: number): boolean;

  /**
   * Calculate delay before next retry attempt
   * @param attemptCount Current attempt number (0-indexed)
   * @returns Delay in milliseconds
   */
  getNextDelay(attemptCount: number): number;
}

// ============================================================================
// Fallback Policy
// ============================================================================

/**
 * Fallback strategy when execution fails
 */
export interface FallbackPolicy {
  /** Fallback value to use when execution fails */
  fallbackValue?: unknown;

  /** Whether to log fallback events */
  logFallback: boolean;

  /** Whether to continue execution after using fallback */
  continueAfterFallback: boolean;
}

// ============================================================================
// Failure Context
// ============================================================================

/**
 * Execution context provided to policy decision methods
 */
export interface FailureContext {
  /** ID of the current execution */
  executionId?: string;

  /** ID of the parent workflow */
  workflowId?: string;

  /** ID of the current node */
  nodeId?: string;

  /** ID of the child execution (for sub-executions) */
  childExecutionId?: string;

  /** Current attempt number (starts at 0) */
  attemptCount?: number;

  /** Time elapsed since execution started (milliseconds) */
  elapsedTime?: number;

  /** Remaining budget for retries (milliseconds or count) */
  budgetRemaining?: number;

  /** Custom metadata */
  metadata?: Record<string, unknown>;
}

// ============================================================================
// Main Failure Policy Interface
// ============================================================================

/**
 * Failure Policy - Unified interface for handling failures
 *
 * Implementations should provide consistent decision-making for:
 * - Node execution failures
 * - Child execution failures (SUBGRAPH, FORK, etc.)
 * - Tool execution failures
 */
export interface FailurePolicy {
  /**
   * Decide how to handle a node execution failure
   *
   * @param error The error that occurred
   * @param node The node that failed (optional)
   * @param context Execution context
   * @returns The action to take
   */
  onNodeFailure(
    error: Error,
    node?: RuntimeNode,
    context?: FailureContext,
  ): Promise<FailureAction> | FailureAction;

  /**
   * Decide how to handle a child execution failure
   *
   * Child executions are: SUBGRAPH, FORK_BRANCH, AGENT_LOOP, EMBED_GRAPH
   *
   * @param error The error that occurred
   * @param childType Type of child execution
   * @param context Execution context
   * @returns The action to take
   */
  onChildExecutionFailure(
    error: Error,
    childType: "SUBGRAPH" | "FORK_BRANCH" | "AGENT_LOOP" | "EMBED_GRAPH",
    context?: FailureContext,
  ): Promise<FailureAction> | FailureAction;

  /**
   * Decide if a tool execution should be retried
   *
   * @param toolName Name of the tool that failed
   * @param error The error that occurred
   * @param context Execution context
   * @returns true if should retry, false otherwise
   */
  onToolFailure(
    toolName: string,
    error: Error,
    context?: FailureContext,
  ): Promise<boolean> | boolean;

  /**
   * Get retry policy for given error severity
   *
   * @param severity Error severity level
   * @returns Retry policy configuration
   */
  getRetryPolicy(severity: ErrorSeverity): RetryPolicy;

  /**
   * Get fallback policy
   *
   * @returns Fallback policy configuration
   */
  getFallbackPolicy(): FallbackPolicy;

  /**
   * Determine if error should be logged
   *
   * @param severity Error severity
   * @returns true if should log
   */
  shouldLogError(severity: ErrorSeverity): boolean;

  /**
   * Extract severity from error
   *
   * @param error The error object
   * @returns Extracted or inferred severity
   */
  getErrorSeverity(error: Error): ErrorSeverity;
}

// ============================================================================
// Configuration Type
// ============================================================================

/**
 * Configuration for FailurePolicy implementation
 */
export interface FailurePolicyConfig {
  /** Global retry policy overrides */
  retryPolicy?: Partial<RetryPolicy>;

  /** Fallback policy configuration */
  fallbackPolicy?: Partial<FallbackPolicy>;

  /** Per-error-type retry policy overrides */
  retryPolicyByErrorType?: Record<string, Partial<RetryPolicy>>;

  /** Error types that should never be retried */
  nonRetryableErrors?: string[];

  /** Log level for error messages */
  logLevel?: "debug" | "info" | "warn" | "error";

  /** Whether to enable metrics collection */
  metricsEnabled?: boolean;
}
