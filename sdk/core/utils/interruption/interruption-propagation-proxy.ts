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
  private currentDepth: number = 0;
  
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
    // Detect circular reference
    if (this.parentState === childState) {
      throw new Error("Cannot register parent as child");
    }
    
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
  private propagateToInterruption(type: "PAUSE" | "STOP" | "RESUME"): PropagationResult {
    this.currentDepth++;
    
    const failedChildren: string[] = [];
    const totalChildren = this.childStates.size;
    
    // Monitor deep nesting
    if (this.currentDepth > InterruptionPropagationProxy.MAX_RECOMMENDED_DEPTH) {
      logger.warn("Deep interruption propagation detected", {
        depth: this.currentDepth,
        type,
        parentContextId: this.parentState?.getContextId(),
        maxRecommendedDepth: InterruptionPropagationProxy.MAX_RECOMMENDED_DEPTH,
      });
    }
    
    logger.debug("Propagating interruption to children", {
      type,
      childCount: totalChildren,
      depth: this.currentDepth,
    });
    
    for (const childState of this.childStates) {
      try {
        this.syncChildState(childState, type);
      } catch (error) {
        failedChildren.push(childState.getContextId());
        logger.error("Critical: Failed to propagate to child", {
          childContextId: childState.getContextId(),
          type,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
    
    this.currentDepth--;
    
    const result: PropagationResult = {
      success: failedChildren.length === 0,
      failedChildren,
      totalChildren,
    };
    
    if (!result.success) {
      logger.error("Interruption propagation partially failed", {
        type,
        failedCount: failedChildren.length,
        totalCount: totalChildren,
        failedChildren,
      });
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
    this.currentDepth = 0;
    
    logger.debug("Propagation proxy disposed", {
      cleanedUpChildren: childCount,
    });
  }
}
