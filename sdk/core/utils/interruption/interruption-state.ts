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
import { InterruptionHistoryManager, type InterruptionHistoryEntry } from "./interruption-history-manager.js";

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
 * Maximum number of event listeners to prevent performance degradation
 */
const MAX_EVENT_LISTENERS = 100;

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
  
  /** Concurrency control - prevents race conditions */
  private isProcessing: boolean = false;
  private pendingRequests: Array<() => void> = [];
  
  /** Pending children registration - solves async creation race condition */
  private pendingChildren: Map<string, { childState?: InterruptionState; registeredAt: number }> = new Map();
  
  /** Interruption history manager */
  private historyManager: InterruptionHistoryManager;

  /**
   * Constructor
   * @param config Configuration options
   */
  constructor(config: InterruptionStateConfig & { historyMaxSize?: number }) {
    this.contextId = config.contextId;
    this.nodeId = config.nodeId ?? "";
    this.createInterruptionError = config.createInterruptionError;
    this.historyManager = new InterruptionHistoryManager(config.historyMaxSize ?? 1000);
  }

  /**
   * Request to pause
   */
  requestPause(): void {
    this.enqueueRequest(() => {
      if (this.interruptionType === "PAUSE") {
        return; // It is already in a paused state.
      }

      logger.info("Execution pause requested", { contextId: this.contextId, nodeId: this.nodeId });
      this.interruptionType = "PAUSE";
      const error = this.createError("Execution paused", "PAUSE");
      this.abortController.abort(error);
      
      // Record to history
      this.historyManager.record({
        type: "PAUSE",
        contextId: this.contextId,
        nodeId: this.nodeId || undefined,
        triggeredBy: "user",
      });
      
      // Notify all listeners (including propagation proxy)
      this.notifyInterruptionListeners("PAUSE");
    });
  }

  /**
   * Request to stop
   */
  requestStop(): void {
    this.enqueueRequest(() => {
      if (this.interruptionType === "STOP") {
        return; // It is already in a stopped state.
      }

      // If already PAUSE, upgrade to STOP
      if (this.interruptionType === "PAUSE") {
        logger.info("Upgrading PAUSE to STOP", {
          contextId: this.contextId,
          nodeId: this.nodeId,
        });
      }

      logger.info("Execution stop requested", { contextId: this.contextId, nodeId: this.nodeId });
      this.interruptionType = "STOP";
      const error = this.createError("Execution stopped", "STOP");
      this.abortController.abort(error);
      
      // Record to history
      this.historyManager.record({
        type: "STOP",
        contextId: this.contextId,
        nodeId: this.nodeId || undefined,
        triggeredBy: "user",
      });
      
      // Notify all listeners (including propagation proxy)
      this.notifyInterruptionListeners("STOP");
    });
  }

  /**
   * Resume execution
   * 
   * This method resets the interruption state and creates a new AbortController.
   * All registered resume listeners will be notified to refresh their signal references.
   * 
   * IMPORTANT: External code should NOT cache AbortSignal references across pause/resume cycles.
   * Always call getAbortSignal() after resume() to obtain the current signal.
   * 
   * NOTE: Resume now automatically propagates to all children via event propagation,
   * consistent with PAUSE/STOP behavior. This ensures state consistency across the entire
   * execution hierarchy.
   * 
   * LISTENER BEHAVIOR:
   * - All resume listeners are called once and then cleared to prevent memory leaks
   * - If you need persistent listening, re-register after each resume using the unsubscribe pattern:
   *   ```typescript
   *   let unsubscribe = interruptionState.onResumed(() => {
   *     // Handle resume
   *     // Re-register for next resume
   *     unsubscribe = interruptionState.onResumed(/* ... *\/);
   *   });
   *   ```
   */
  resume(): void {
    this.enqueueRequest(() => {
      logger.info("Execution resumed", { contextId: this.contextId, nodeId: this.nodeId });
      
      // Calculate pause duration before resetting state
      const pauseDuration = this.historyManager.getPauseDuration(this.contextId);
      
      this.interruptionType = null;
      
      // Create a new AbortController for fresh state
      this.abortController = new AbortController();
      
      // Record to history
      this.historyManager.record({
        type: "RESUME",
        contextId: this.contextId,
        nodeId: this.nodeId || undefined,
        triggeredBy: "user",
        duration: pauseDuration ?? undefined,
      });
      
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
    });
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
    if (this.resumeListeners.length >= MAX_EVENT_LISTENERS) {
      logger.warn("Too many resume listeners registered", {
        count: this.resumeListeners.length,
        limit: MAX_EVENT_LISTENERS,
        contextId: this.contextId,
      });
    }
    
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
   * Pre-register a child before async creation to prevent race conditions
   * 
   * This method creates a placeholder registration that will be confirmed later.
   * If an interruption occurs during child creation, it will be applied when confirmed.
   * 
   * @param childId Unique identifier for the child (e.g., execution ID)
   * @returns Function to confirm or cancel the registration
   * @example
   * ```typescript
   * // Before async creation
   * const confirm = parentState.preRegisterChild(childExecutionId);
   * 
   * // Async creation
   * const childAgent = await createAgentLoop(...);
   * 
   * // After creation - confirm registration
   * confirm(childAgent.getInterruptionState());
   * ```
   */
  preRegisterChild(childId: string): (childState?: InterruptionState) => void {
    // Record pending registration
    this.pendingChildren.set(childId, {
      childState: undefined,
      registeredAt: Date.now(),
    });
    
    logger.debug("Child pre-registered (pending confirmation)", {
      parentContextId: this.contextId,
      childId,
    });
    
    // Return confirmation function
    return (childState?: InterruptionState) => {
      this.confirmChildRegistration(childId, childState);
    };
  }
  
  /**
   * Confirm a pre-registered child
   * 
   * @param childId Child identifier
   * @param childState Actual child interruption state (optional)
   */
  private confirmChildRegistration(childId: string, childState?: InterruptionState): void {
    const pending = this.pendingChildren.get(childId);
    if (!pending) {
      logger.warn("Confirmation for non-existent pending child", { childId });
      return;
    }
    
    // Update pending entry with actual state
    pending.childState = childState;
    
    // If we have an actual child state, register it normally
    if (childState) {
      this.registerChild(childState);
      logger.info("Child registration confirmed", {
        parentContextId: this.contextId,
        childId,
        childContextId: childState.getContextId(),
      });
    } else {
      // No child state provided - remove from pending
      logger.debug("Child registration cancelled (no state provided)", { childId });
    }
    
    // Clean up pending entry after a delay to allow for interruption checks
    setTimeout(() => {
      this.pendingChildren.delete(childId);
    }, 1000);
  }
  
  /**
   * Check if there are any pending child registrations
   */
  hasPendingChildren(): boolean {
    return this.pendingChildren.size > 0;
  }
  
  /**
   * Get count of pending children
   */
  getPendingChildrenCount(): number {
    return this.pendingChildren.size;
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
    if (this.interruptionListeners.length >= MAX_EVENT_LISTENERS) {
      logger.warn("Too many interruption listeners registered", {
        count: this.interruptionListeners.length,
        limit: MAX_EVENT_LISTENERS,
        contextId: this.contextId,
      });
    }
    
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
    // Abort the controller to release any pending operations
    if (!this.abortController.signal.aborted) {
      this.abortController.abort(new Error('InterruptionState disposed'));
    }
    
    this.propagationProxy?.dispose();
    this.interruptionListeners = [];
    this.resumeListeners = [];
    this.pendingRequests = []; // Clear pending requests
    this.pendingChildren.clear(); // Clear pending children
    this.isProcessing = false;
    
    // Help GC by nullifying the controller reference
    (this as any).abortController = null;
    
    logger.debug("InterruptionState disposed", {
      contextId: this.contextId,
    });
  }
  
  /**
   * Get interruption history
   * 
   * @param filter Optional filter options
   * @returns Filtered history entries
   */
  getHistory(filter?: import("./interruption-history-manager.js").HistoryFilter): InterruptionHistoryEntry[] {
    return this.historyManager.getHistory(filter);
  }
  
  /**
   * Get interruption statistics
   * 
   * @returns Statistics object
   */
  getStatistics() {
    return this.historyManager.getStatistics();
  }
  
  /**
   * Enqueue request for sequential processing (prevents race conditions)
   */
  private enqueueRequest(request: () => void): void {
    if (this.isProcessing) {
      // Queue the request for later processing
      this.pendingRequests.push(request);
      return;
    }
    
    this.isProcessing = true;
    try {
      request();
    } finally {
      this.isProcessing = false;
      // Process any pending requests
      this.processPendingRequests();
    }
  }
  
  /**
   * Process pending requests sequentially
   */
  private processPendingRequests(): void {
    while (this.pendingRequests.length > 0) {
      const request = this.pendingRequests.shift();
      if (request) {
        this.isProcessing = true;
        try {
          request();
        } finally {
          this.isProcessing = false;
        }
      }
    }
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
    // Use InterruptionInfo as the context to avoid duplication
    return new InterruptedException(message, type, {
      contextId: info.contextId,
      nodeId: info.nodeId,
    });
  }
}
