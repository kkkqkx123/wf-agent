/**
 * Interruption State
 * Provides unified management of interrupt status and operations
 *
 * Responsibilities:
 * - Manages interrupt status (PAUSE/STOP)
 * - Supplies the AbortSignal for deep interrupts
 * - Coordinates interrupt requests and recovery operations
 *
 * Design Principles:
 * - Single Responsibility Principle: Responsible only for interrupt status management
 * - Encapsulation: Hides internal implementation details
 * - Execution Safety: Ensures atomicity of state changes
 * - Unified Use of AbortSignal as the primary interrupt mechanism
 * - Portability: Can be shared by both the Graph module and the Agent module
 */

import { InterruptedException } from "../../types/interruption-types.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { InterruptionPropagationProxy } from "./interruption-propagation-proxy.js";

const logger = createContextualLogger({ component: "InterruptionState" });

/**
 * Interrupt types
 */
export type InterruptionType = "PAUSE" | "STOP" | null;

/**
 * Interrupt exception information
 */
export interface InterruptionInfo {
  /** Interrupt Types */
  type: InterruptionType;
  /** Interrupt message */
  message: string;
  /** Context ID (such as execution ID, session ID, etc.) */
  contextId?: string;
  /** Node ID (optional) */
  nodeId?: string;
}

/**
 * Interruption State Configuration
 */
export interface InterruptionStateConfig {
  /** Context ID (such as execution ID, session ID, etc.) */
  contextId: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Custom Interrupt Exception Creation Function */
  createInterruptionError?: (info: InterruptionInfo) => InterruptedException;
}

/**
 * Interruption State
 *
 * A general-purpose interrupt management component that supports shared use by both the Graph module and the Agent module.
 */
export class InterruptionState {
  private abortController: AbortController = new AbortController();
  private interruptionType: InterruptionType = null;
  private contextId: string;
  private nodeId: string;
  private createInterruptionError?: (info: InterruptionInfo) => InterruptedException;
  
  /** Event listeners for resume notifications */
  private resumeListeners: Array<() => void> = [];
  
  /** Interruption propagation proxy (manages parent-child sync) */
  private propagationProxy?: InterruptionPropagationProxy;
  
  /** Event listeners for interruption notifications (PAUSE/STOP/RESUME) */
  private interruptionListeners: Array<(type: "PAUSE" | "STOP" | "RESUME") => void> = [];

  /**
   * Constructor
   * @param config Configuration options
   */
  constructor(config: InterruptionStateConfig);

  constructor(configOrContextId: InterruptionStateConfig | string, nodeId?: string) {
    if (typeof configOrContextId === "string") {
      // Backward compatibility: The old form of the constructor
      this.contextId = configOrContextId;
      this.nodeId = nodeId ?? "";
    } else {
      // New configuration object format
      this.contextId = configOrContextId.contextId;
      this.nodeId = configOrContextId.nodeId ?? "";
      this.createInterruptionError = configOrContextId.createInterruptionError;
    }
  }

  /**
   * Request to pause
   */
  requestPause(): void {
    if (this.interruptionType === "PAUSE") {
      return; // It is already in a paused state.
    }

    logger.info("Execution pause requested", { contextId: this.contextId, nodeId: this.nodeId });
    this.interruptionType = "PAUSE";
    const error = this.createError("Execution paused", "PAUSE");
    this.abortController.abort(error);
    
    // Notify all listeners (including propagation proxy)
    this.notifyInterruptionListeners("PAUSE");
  }

  /**
   * Request to stop
   */
  requestStop(): void {
    if (this.interruptionType === "STOP") {
      return; // It is already in a stopped state.
    }

    logger.info("Execution stop requested", { contextId: this.contextId, nodeId: this.nodeId });
    this.interruptionType = "STOP";
    const error = this.createError("Execution stopped", "STOP");
    this.abortController.abort(error);
    
    // Notify all listeners (including propagation proxy)
    this.notifyInterruptionListeners("STOP");
  }

  /**
   * Resume execution
   * 
   * This method resets the interruption state and creates a new AbortController.
   * All registered resume listeners will be notified to refresh their signal references.
   * 
   * IMPORTANT: External code should NOT cache AbortSignal references across pause/resume cycles.
   * Always call getAbortSignal() or getFreshAbortSignal() after resume() to obtain the current signal.
   * 
   * NOTE: Resume now automatically propagates to all children via event propagation,
   * consistent with PAUSE/STOP behavior. This ensures state consistency across the entire
   * execution hierarchy.
   */
  resume(): void {
    logger.info("Execution resumed", { contextId: this.contextId, nodeId: this.nodeId });
    this.interruptionType = null;
    
    // Create a new AbortController for fresh state
    this.abortController = new AbortController();
    
    // Notify all listeners to refresh their signal references
    // This prevents stale signal references from causing issues
    const listeners = [...this.resumeListeners];
    this.resumeListeners = []; // Clear listeners to prevent memory leaks
    listeners.forEach(listener => {
      try {
        listener();
      } catch (error) {
        logger.warn("Error in resume listener", { 
          contextId: this.contextId, 
          error: error instanceof Error ? error.message : String(error) 
        });
      }
    });
    
    // Auto-propagate resume to children (consistent with pause/stop)
    this.notifyInterruptionListeners("RESUME");
    
    // Help GC by removing references to old controller
    // Note: The old controller's signal may still be referenced externally,
    // but we've done our part to notify listeners
  }
  
  /**
   * Register a callback to be invoked when resume() is called
   * 
   * This allows external code to refresh their AbortSignal references
   * without polling or caching stale signals.
   * 
   * @param callback Function to call on resume
   * @returns Unsubscribe function
   * 
   * @example
   * ```typescript
   * const unsubscribe = interruptionState.onResumed(() => {
   *   // Refresh signal reference
   *   const freshSignal = interruptionState.getAbortSignal();
   *   // Re-subscribe to events, update references, etc.
   * });
   * 
   * // Later, when no longer needed
   * unsubscribe();
   * ```
   */
  onResumed(callback: () => void): () => void {
    this.resumeListeners.push(callback);
    
    // Return unsubscribe function
    return () => {
      const index = this.resumeListeners.indexOf(callback);
      if (index !== -1) {
        this.resumeListeners.splice(index, 1);
      }
    };
  }

  /**
   * Get the interrupt type
   */
  getInterruptionType(): InterruptionType {
    return this.interruptionType;
  }

  /**
   * Get the AbortSignal
   * 
   * IMPORTANT: Do NOT cache this signal across pause/resume cycles.
   * After calling resume(), you MUST call this method again to get the fresh signal.
   * 
   * For better safety, consider using onResumed() to automatically refresh your signal reference:
   * ```typescript
   * let signal = interruptionState.getAbortSignal();
   * const unsubscribe = interruptionState.onResumed(() => {
   *   signal = interruptionState.getAbortSignal(); // Auto-refresh
   * });
   * ```
   * 
   * @returns The current AbortSignal
   */
  getAbortSignal(): AbortSignal {
    return this.abortController.signal;
  }

  /**
   * Get a fresh AbortSignal (guaranteed to be current)
   * 
   * This is an alias for getAbortSignal() but makes it explicit that
   * you're getting the most recent signal.
   * 
   * RECOMMENDED: Use this method after resume() or in conjunction with onResumed().
   * 
   * @returns The current AbortSignal
   */
  getFreshAbortSignal(): AbortSignal {
    return this.getAbortSignal();
  }

  /**
   * Check if it has been aborted.
   */
  isAborted(): boolean {
    return this.abortController.signal.aborted;
  }

  /**
   * Check whether it should be paused.
   */
  shouldPause(): boolean {
    return this.interruptionType === "PAUSE";
  }

  /**
   * Check whether it should be stopped.
   */
  shouldStop(): boolean {
    return this.interruptionType === "STOP";
  }

  /**
   * Get the reason for the termination.
   */
  getAbortReason(): Error | undefined {
    return this.abortController.signal.reason as Error | undefined;
  }

  /**
   * Update the current node ID
   */
  updateNodeId(nodeId: string): void {
    this.nodeId = nodeId;
  }

  /**
   * Get the context ID
   */
  getContextId(): string {
    return this.contextId;
  }

  /**
   * Get the node ID
   */
  getNodeId(): string {
    return this.nodeId;
  }
  
  /**
   * Register a child interruption state for cascade propagation
   * 
   * @param childState Child's interruption state
   * @example
   * ```typescript
   * parentInterruptionState.registerChild(childInterruptionState);
   * ```
   */
  registerChild(childState: InterruptionState): void {
    if (!this.propagationProxy) {
      this.propagationProxy = new InterruptionPropagationProxy();
      // Attach to self (this is the parent)
      this.propagationProxy.attachToParent(this);
    }
    this.propagationProxy.registerChild(childState);
    
    logger.debug("Child interruption state registered", {
      parentContextId: this.contextId,
      childContextId: childState.getContextId(),
    });
  }
  
  /**
   * Unregister a child interruption state (cleanup)
   */
  unregisterChild(childState: InterruptionState): void {
    this.propagationProxy?.unregisterChild(childState);
  }
  
  /**
   * Subscribe to interruption events
   * 
   * @param callback Called when pause/stop/resume is requested
   * @returns Unsubscribe function
   * @example
   * ```typescript
   * const unsubscribe = interruptionState.onInterrupted((type) => {
   *   console.log(`Interrupted: ${type}`);
   * });
   * ```
   */
  onInterrupted(callback: (type: "PAUSE" | "STOP" | "RESUME") => void): () => void {
    this.interruptionListeners.push(callback);
    
    return () => {
      const index = this.interruptionListeners.indexOf(callback);
      if (index !== -1) {
        this.interruptionListeners.splice(index, 1);
      }
    };
  }
  
  /**
   * Notify all interruption listeners
   */
  private notifyInterruptionListeners(type: "PAUSE" | "STOP" | "RESUME"): void {
    const listeners = [...this.interruptionListeners];
    listeners.forEach(listener => {
      try {
        listener(type);
      } catch (error) {
        logger.warn("Error in interruption listener", { 
          contextId: this.contextId,
          error: error instanceof Error ? error.message : String(error) 
        });
      }
    });
  }

  /**
   * Cleanup resources (prevent memory leaks)
   */
  dispose(): void {
    this.propagationProxy?.dispose();
    this.interruptionListeners = [];
    this.resumeListeners = [];
    
    logger.debug("InterruptionState disposed", {
      contextId: this.contextId,
    });
  }
  
  /**
   * Create an interrupt error
   */
  private createError(message: string, type: "PAUSE" | "STOP"): InterruptedException {
    const info: InterruptionInfo = {
      type,
      message,
      contextId: this.contextId,
      nodeId: this.nodeId,
    };

    if (this.createInterruptionError) {
      return this.createInterruptionError(info);
    }

    // Default: throw a generic interruption exception.
    // Callers should provide createInterruptionError callback for module-specific exceptions.
    return new InterruptedException(message, type, {
      contextId: this.contextId,
      nodeId: this.nodeId,
    });
  }
}
