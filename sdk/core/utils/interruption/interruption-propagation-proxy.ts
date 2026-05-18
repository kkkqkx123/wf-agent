/**
 * Interruption Propagation Proxy
 * 
 * Manages parent-child interruption state synchronization using event-driven mechanism.
 * Ensures immediate propagation without polling.
 * 
 * Responsibilities:
 * - Listen to parent's interruption events (PAUSE/STOP/RESUME)
 * - Broadcast interruption events to all registered children
 * - Manage subscription relationships (prevent memory leaks)
 * - Monitor propagation depth for performance
 */

import { InterruptionState } from "./interruption-state.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "InterruptionPropagationProxy" });

/**
 * Result of interruption propagation
 */
export interface PropagationResult {
  success: boolean;
  failedChildren: string[];
  totalChildren: number;
}

/**
 * Propagation failure type classification
 */
export enum PropagationFailureType {
  TEMPORARY = 'TEMPORARY',    // Temporary failure, retryable
  PERMANENT = 'PERMANENT',    // Permanent failure, not retryable
  PARTIAL = 'PARTIAL',        // Partial failure
}

/**
 * Propagation error with classification
 */
export interface PropagationError extends Error {
  readonly name: string;
  readonly failureType: PropagationFailureType;
  readonly childContextId: string;
  readonly retryable: boolean;
}

/**
 * Aggregate error for multiple propagation failures
 */
export class AggregatePropagationError extends Error {
  constructor(
    public failures: Array<{ contextId: string; error: PropagationError }>,
    public interruptionType: string
  ) {
    super(`Propagation failed for ${failures.length} children`);
    this.name = 'AggregatePropagationError';
  }
}

/**
 * Error thrown when propagation fails critically
 */
export class InterruptionPropagationError extends Error {
  constructor(
    message: string,
    public failedChildren: string[],
    public totalChildren: number
  ) {
    super(message);
    this.name = 'InterruptionPropagationError';
  }
}

/**
 * Interruption Propagation Proxy
 * 
 * Manages parent-child interruption state synchronization using event-driven mechanism.
 * Ensures immediate propagation without polling.
 */
export class InterruptionPropagationProxy {
  private parentState?: InterruptionState;
  private childStates: Set<InterruptionState> = new Set();
  private unsubscribeFromParent?: () => void;
  
  // Performance monitoring
  private static MAX_RECOMMENDED_DEPTH = 10;
  private static MAX_ACCEPTABLE_LATENCY_MS = 100;
  
  /**
   * Attach to parent interruption state
   * Listens for pause/stop/resume events and propagates to children
   */
  attachToParent(parentState: InterruptionState): void {
    this.parentState = parentState;
    
    // Subscribe to parent's interruption events
    this.unsubscribeFromParent = parentState.onInterrupted((type) => {
      this.propagateToInterruption(type);
    });
    
    // Immediately sync with parent's current state
    if (parentState.isAborted()) {
      const reason = parentState.getAbortReason();
      if (reason) {
        const type = reason.message.includes("paused") ? "PAUSE" : "STOP";
        this.propagateToInterruption(type);
      }
    }
    
    logger.debug("Attached to parent interruption state", {
      parentContextId: parentState.getContextId(),
    });
  }
  
  /**
   * Register a child interruption state
   * The child will receive all future interruption events from this proxy
   */
  registerChild(childState: InterruptionState): void {
    // Detect circular reference (including indirect references)
    const visited = new Set<string>();
    if (this.parentState) {
      visited.add(this.parentState.getContextId());
    }
    this.detectCircularReference(childState, visited);
    
    this.childStates.add(childState);
    
    // Immediately sync with current interruption state
    if (this.parentState?.isAborted()) {
      const reason = this.parentState.getAbortReason();
      if (reason) {
        const type = reason.message.includes("paused") ? "PAUSE" : "STOP";
        this.syncChildState(childState, type);
      }
    }
    
    logger.debug("Child interruption state registered", {
      parentContextId: this.parentState?.getContextId(),
      childContextId: childState.getContextId(),
      totalChildren: this.childStates.size,
    });
  }
  
  /**
   * Detect circular reference in the interruption state hierarchy
   * Uses DFS to traverse the entire tree and check for cycles
   */
  private detectCircularReference(
    childState: InterruptionState,
    visited: Set<string>
  ): void {
    const childId = childState.getContextId();
    
    // Check if we've already visited this node
    if (visited.has(childId)) {
      throw new Error(
        `Circular reference detected: ${childId}. ` +
        `Visited path: ${Array.from(visited).join(' -> ')}`
      );
    }
    
    visited.add(childId);
    
    // Recursively check the child's children (if it has a propagation proxy)
    const childProxy = (childState as any).propagationProxy as InterruptionPropagationProxy | undefined;
    if (childProxy) {
      for (const grandChild of childProxy.childStates) {
        this.detectCircularReference(grandChild, new Set(visited));
      }
    }
  }
  
  /**
   * Unregister a child interruption state (cleanup)
   */
  unregisterChild(childState: InterruptionState): void {
    const removed = this.childStates.delete(childState);
    
    if (removed) {
      logger.debug("Child interruption state unregistered", {
        parentContextId: this.parentState?.getContextId(),
        childContextId: childState.getContextId(),
        remainingChildren: this.childStates.size,
      });
    }
  }
  
  /**
   * Propagate interruption to all registered children
   * @returns Propagation result with success status and failed children list
   */
  private propagateToInterruption(
    type: "PAUSE" | "STOP" | "RESUME",
    currentDepth: number = 0
  ): PropagationResult {
    const startTime = performance.now();
    const depth = currentDepth + 1;
    
    const failedChildren: Array<{ contextId: string; error: PropagationError }> = [];
    const totalChildren = this.childStates.size;
    
    // Monitor deep nesting
    if (depth > InterruptionPropagationProxy.MAX_RECOMMENDED_DEPTH) {
      logger.warn("Deep interruption propagation detected", {
        depth,
        type,
        parentContextId: this.parentState?.getContextId(),
        maxRecommendedDepth: InterruptionPropagationProxy.MAX_RECOMMENDED_DEPTH,
      });
      
      // Report metric
      this.reportMetric('propagation_depth', depth);
    }
    
    logger.debug("Propagating interruption to children", {
      type,
      childCount: totalChildren,
      depth,
    });
    
    for (const childState of this.childStates) {
      try {
        this.syncChildState(childState, type);
        // Recursively propagate to grandchildren with incremented depth
        const childProxy = (childState as any).propagationProxy as InterruptionPropagationProxy | undefined;
        if (childProxy && childProxy.childStates.size > 0) {
          childProxy.propagateToInterruption(type, depth);
        }
      } catch (error) {
        const propError = this.classifyError(error, childState);
        failedChildren.push({
          contextId: childState.getContextId(),
          error: propError,
        });
        
        // Attempt retry for temporary failures
        if (propError.retryable) {
          this.retryPropagation(childState, type, propError).catch((retryError) => {
            logger.error("Propagation retry exhausted", {
              childContextId: childState.getContextId(),
              attempts: 3,
              originalError: propError.message,
              retryError: retryError instanceof Error ? retryError.message : String(retryError),
            });
          });
        } else {
          logger.error("Non-retryable propagation failure", {
            childContextId: childState.getContextId(),
            type,
            error: propError.message,
            failureType: propError.failureType,
          });
        }
      }
    }
    
    const elapsed = performance.now() - startTime;
    
    // Monitor latency
    if (elapsed > InterruptionPropagationProxy.MAX_ACCEPTABLE_LATENCY_MS) {
      logger.warn("Slow interruption propagation", {
        latencyMs: elapsed,
        depth,
        type,
        childCount: totalChildren,
      });
      
      // Report metric
      this.reportMetric('propagation_latency', elapsed);
    }
    
    const failedIds = failedChildren.map(f => f.contextId);
    const result: PropagationResult = {
      success: failedIds.length === 0,
      failedChildren: failedIds,
      totalChildren,
    };
    
    if (!result.success) {
      logger.error("Interruption propagation partially failed", {
        type,
        failedCount: failedIds.length,
        totalCount: totalChildren,
        failedChildren: failedIds,
      });
      
      // Throw aggregate error for caller to handle
      throw new AggregatePropagationError(failedChildren, type);
    }
    
    return result;
  }
  
  /**
   * Sync a single child's interruption state
   */
  private syncChildState(childState: InterruptionState, type: "PAUSE" | "STOP" | "RESUME"): void {
    try {
      if (type === "PAUSE") {
        childState.requestPause();
      } else if (type === "STOP") {
        childState.requestStop();
      } else if (type === "RESUME") {
        childState.resume();
      }
    } catch (error) {
      logger.warn("Failed to propagate interruption to child", {
        childContextId: childState.getContextId(),
        type,
        error: error instanceof Error ? error.message : String(error),
      });
      throw error; // Re-throw to be caught by propagateToInterruption
    }
  }
  
  /**
   * Classify error for appropriate handling
   */
  private classifyError(error: any, childState: InterruptionState): PropagationError {
    // Check for timeout errors (temporary)
    if (error instanceof Error && error.message.includes('timeout')) {
      return {
        name: 'PropagationTimeoutError',
        message: error.message,
        failureType: PropagationFailureType.TEMPORARY,
        childContextId: childState.getContextId(),
        retryable: true,
      };
    }
    
    // Check for null/undefined state errors (permanent)
    if (error instanceof TypeError && error.message.includes('null') || error.message.includes('undefined')) {
      return {
        name: 'StateCorruptionError',
        message: error.message,
        failureType: PropagationFailureType.PERMANENT,
        childContextId: childState.getContextId(),
        retryable: false,
      };
    }
    
    // Default to temporary failure for unknown errors
    return {
      name: 'UnknownPropagationError',
      message: error instanceof Error ? error.message : String(error),
      failureType: PropagationFailureType.TEMPORARY,
      childContextId: childState.getContextId(),
      retryable: true,
    };
  }
  
  /**
   * Retry propagation for temporary failures
   */
  private async retryPropagation(
    childState: InterruptionState,
    type: "PAUSE" | "STOP" | "RESUME",
    error: PropagationError,
    maxRetries: number = 3
  ): Promise<void> {
    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        // Exponential backoff (skip delay on first attempt)
        if (attempt > 1) {
          await this.delay(100 * Math.pow(2, attempt - 2));
        }
        
        this.syncChildState(childState, type);
        
        logger.info("Propagation retry succeeded", {
          childContextId: childState.getContextId(),
          attempt,
          type,
        });
        return;
      } catch (retryError) {
        if (attempt === maxRetries) {
          logger.error("Propagation retry exhausted", {
            childContextId: childState.getContextId(),
            attempts: maxRetries,
            originalError: error.message,
            retryError: retryError instanceof Error ? retryError.message : String(retryError),
          });
          throw retryError;
        }
        
        logger.debug("Propagation retry attempt failed", {
          childContextId: childState.getContextId(),
          attempt,
          maxRetries,
        });
      }
    }
  }
  
  /**
   * Delay utility for retry backoff
   */
  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
  
  /**
   * Report performance metric
   * @param name Metric name
   * @param value Metric value
   */
  private reportMetric(name: string, value: number): void {
    // Integration point for metrics system
    // Example: metrics.histogram(`interruption.${name}`, value);
    logger.debug("Interruption propagation metric", {
      metric: name,
      value,
      parentContextId: this.parentState?.getContextId(),
    });
  }
  
  /**
   * Cleanup all subscriptions (prevent memory leaks)
   */
  dispose(): void {
    if (this.unsubscribeFromParent) {
      this.unsubscribeFromParent();
      this.unsubscribeFromParent = undefined;
    }
    
    const childCount = this.childStates.size;
    this.childStates.clear();
    this.parentState = undefined;
    
    logger.debug("Propagation proxy disposed", {
      cleanedUpChildren: childCount,
    });
  }
}
