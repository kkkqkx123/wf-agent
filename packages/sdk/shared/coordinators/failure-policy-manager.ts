/**
 * Failure Policy Manager - Default Implementation
 *
 * Provides configurable, production-ready failure handling strategies.
 * Supports retry with exponential backoff, fallback values, and error severity mapping.
 */

import type {
  FailurePolicy,
  RetryPolicy,
  FailurePolicyConfig,
  FailureContext,
  FallbackPolicy,
} from "@wf-agent/types";
import type { ErrorSeverity } from "@wf-agent/types";
import { FailureAction } from "@wf-agent/types";
import type { SDKError, RuntimeNode } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "FailurePolicyManager" });

// ============================================================================
// Default Configurations
// ============================================================================

const DEFAULT_RETRY_POLICY: Required<RetryPolicy> = {
  enabled: true,
  maxRetries: 3,
  baseDelay: 1000,
  backoffMultiplier: 2,
  maxDelay: 30000,
  jitter: true,
  shouldRetry: (error: SDKError, attemptCount: number) => {
    // Do NOT retry timeout errors - already waited maximum time
    if (error.message?.toLowerCase().includes("timeout")) {
      return false;
    }
    // By default: only retry error severity, not warning/info
    return error.severity === "error" && attemptCount < 3;
  },
  getNextDelay: (attemptCount: number) => {
    const delay =
      DEFAULT_RETRY_POLICY.baseDelay *
      Math.pow(DEFAULT_RETRY_POLICY.backoffMultiplier, attemptCount);
    const delayWithMax = Math.min(delay, DEFAULT_RETRY_POLICY.maxDelay);

    if (DEFAULT_RETRY_POLICY.jitter) {
      // Add ±10% random jitter
      const jitterFactor = 0.9 + Math.random() * 0.2;
      return Math.floor(delayWithMax * jitterFactor);
    }
    return delayWithMax;
  },
};

const DEFAULT_FALLBACK_POLICY: Required<FallbackPolicy> = {
  fallbackValue: undefined,
  logFallback: true,
  continueAfterFallback: true,
};

// ============================================================================
// Default Failure Policy Implementation
// ============================================================================

/**
 * DefaultFailurePolicy - Production-ready failure policy implementation
 *
 * Features:
 * - Configurable retry strategies per error severity
 * - Exponential backoff with jitter
 * - Fallback value support
 * - Error severity inference
 * - Comprehensive logging
 */
export class DefaultFailurePolicy implements FailurePolicy {
  private config: FailurePolicyConfig;
  private retryPolicies: Map<string, RetryPolicy> = new Map();

  constructor(config?: FailurePolicyConfig) {
    this.config = config || {};
    this.initializeRetryPolicies();
  }

  /**
   * Initialize retry policies for each error severity
   */
  private initializeRetryPolicies(): void {
    // Base policies for each severity
    const basePolicies: Record<ErrorSeverity, Required<RetryPolicy>> = {
      info: {
        ...DEFAULT_RETRY_POLICY,
        enabled: false,
      },
      warning: {
        ...DEFAULT_RETRY_POLICY,
        maxRetries: 2,
      },
      error: {
        ...DEFAULT_RETRY_POLICY,
      },
      critical: {
        ...DEFAULT_RETRY_POLICY,
        maxRetries: 1,
      },
    };

    // Apply user overrides
    for (const [severity, basePolicy] of Object.entries(basePolicies)) {
      const overrides = this.config.retryPolicyByErrorType?.[severity];
      const finalPolicy: RetryPolicy = {
        ...basePolicy,
        ...overrides,
      };

      // Ensure shouldRetry and getNextDelay are defined
      if (!finalPolicy.shouldRetry) {
        finalPolicy.shouldRetry = basePolicy.shouldRetry;
      }
      if (!finalPolicy.getNextDelay) {
        finalPolicy.getNextDelay = basePolicy.getNextDelay;
      }

      this.retryPolicies.set(severity, finalPolicy);
    }
  }

  /**
   * Decide how to handle node execution failure
   */
  async onNodeFailure(
    error: SDKError,
    node?: RuntimeNode,
    context?: FailureContext,
  ): Promise<FailureAction> {
    const severity = this.getErrorSeverity(error);

    logger.debug("Node failure policy decision", {
      nodeId: node?.id,
      severity,
      attemptCount: context?.attemptCount,
      executionId: context?.executionId,
    });

    switch (severity) {
      case "critical":
        return FailureAction.FAIL;

      case "error": {
        // Check if we should retry
        const retryPolicy = this.getRetryPolicy(severity);
        const attemptCount = context?.attemptCount || 0;

        if (
          retryPolicy.enabled &&
          attemptCount < retryPolicy.maxRetries &&
          retryPolicy.shouldRetry(error, attemptCount)
        ) {
          logger.debug("Node error will be retried", {
            nodeId: node?.id,
            attemptCount,
            maxRetries: retryPolicy.maxRetries,
          });
          return FailureAction.RETRY;
        }
        return FailureAction.FAIL;
      }

      case "warning":
      case "info":
        return FailureAction.CONTINUE;

      default:
        return FailureAction.FAIL;
    }
  }

  /**
   * Decide how to handle child execution failure
   */
  async onChildExecutionFailure(
    error: SDKError,
    childType: "SUBGRAPH" | "FORK_BRANCH" | "AGENT_LOOP" | "EMBED_GRAPH",
    context?: FailureContext,
  ): Promise<FailureAction> {
    logger.debug("Child execution failure policy decision", {
      childType,
      childExecutionId: context?.childExecutionId,
      severity: error.severity,
    });

    // Different child types have different failure semantics
    switch (childType) {
      case "SUBGRAPH":
        // SUBGRAPH failure typically propagates to parent
        return FailureAction.FAIL;

      case "FORK_BRANCH":
        // FORK_BRANCH failure is handled by JOIN strategy
        // Return CONTINUE as default; JOIN will decide
        return FailureAction.CONTINUE;

      case "AGENT_LOOP":
        // AGENT_LOOP failure decision based on error severity
        return this.onNodeFailure(error, undefined, context);

      case "EMBED_GRAPH":
        // EMBED_GRAPH failure propagates like SUBGRAPH
        return FailureAction.FAIL;

      default:
        return FailureAction.FAIL;
    }
  }

  /**
   * Decide if tool execution should be retried
   */
  async onToolFailure(
    toolName: string,
    error: SDKError,
    context?: FailureContext,
  ): Promise<boolean> {
    const retryPolicy = this.getRetryPolicy(error.severity);
    const attemptCount = context?.attemptCount || 0;

    logger.debug("Tool failure policy decision", {
      toolName,
      severity: error.severity,
      attemptCount,
      maxRetries: retryPolicy.maxRetries,
    });

    return (
      retryPolicy.enabled &&
      attemptCount < retryPolicy.maxRetries &&
      retryPolicy.shouldRetry(error, attemptCount)
    );
  }

  /**
   * Get retry policy for given error severity
   */
  getRetryPolicy(severity: ErrorSeverity): RetryPolicy {
    const policy = this.retryPolicies.get(severity);
    if (!policy) {
      logger.warn("No retry policy found for severity, using default", {
        severity,
      });
      return { ...DEFAULT_RETRY_POLICY };
    }
    return policy;
  }

  /**
   * Get fallback policy
   */
  getFallbackPolicy(): FallbackPolicy {
    return {
      ...DEFAULT_FALLBACK_POLICY,
      ...this.config.fallbackPolicy,
    };
  }

  /**
   * Determine if error should be logged
   */
  shouldLogError(severity: ErrorSeverity): boolean {
    const logLevel = this.config.logLevel || "info";
    const levels = { debug: 0, info: 1, warn: 2, error: 3 };
    const severityToLogLevel: Record<ErrorSeverity, number> = {
      info: 0,
      warning: 2,
      error: 3,
      critical: 3,
    };

    return severityToLogLevel[severity] >= levels[logLevel];
  }

  /**
   * Extract or infer error severity
   */
  getErrorSeverity(error: Error | SDKError): ErrorSeverity {
    // If it's already an SDKError with severity
    if ("severity" in error && typeof error.severity === "string") {
      const severity = error.severity as string;
      if (["info", "warning", "error", "critical"].includes(severity)) {
        return severity as ErrorSeverity;
      }
    }

    // Infer from error type/message
    if (error.name === "AbortError") {
      return "warning";
    }
    if (error.message?.toLowerCase().includes("timeout")) {
      return "warning";
    }
    if (error.message?.toLowerCase().includes("critical")) {
      return "critical";
    }

    // Default to ERROR
    return "error";
  }
}

// ============================================================================
// Factory Functions
// ============================================================================

/**
 * Create default failure policy with optional config
 */
export function createDefaultFailurePolicy(
  config?: FailurePolicyConfig,
): FailurePolicy {
  return new DefaultFailurePolicy(config);
}

/**
 * Create strict failure policy - any error causes immediate failure, no retry
 */
export function createStrictFailurePolicy(): FailurePolicy {
  return new DefaultFailurePolicy({
    retryPolicy: { enabled: false },
  });
}

/**
 * Create permissive failure policy - try to continue as much as possible
 */
export function createPermissiveFailurePolicy(): FailurePolicy {
  return new DefaultFailurePolicy({
    retryPolicy: {
      enabled: true,
      maxRetries: 5,
      baseDelay: 500,
      backoffMultiplier: 1.5,
      maxDelay: 60000,
    },
  });
}
