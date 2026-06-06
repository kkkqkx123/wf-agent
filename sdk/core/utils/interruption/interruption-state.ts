/**
 * Interruption State
 * Provides unified management of interrupt status and operations
 *
 * Responsibilities:
 * - Manages interrupt status (PAUSE/STOP) as primary coordination state
 * - Supplies the AbortSignal for deep interrupt I/O cancellation
 * - Emits interruption events via EventRegistry for cascade propagation
 *
 * Design Principles:
 * - Single Responsibility Principle: Responsible only for interrupt status management
 * - Encapsulation: Hides internal implementation details
 * - Execution Safety: Ensures atomicity of state changes
 * - Event-driven coordination: interruption commands emit events for cascade and observability
 * - AbortSignal for I/O only: the signal is scoped per execution phase (fresh after resume)
 * - Portability: Can be shared by both the Graph module and the Agent module
 */

import type { EventRegistry } from "../../registry/event-registry.js";
import type { InterruptionType } from "../../types/interruption-types.js";
import type { ExecutionDomainContext } from "@wf-agent/types";
import {
  InterruptionHistoryManager,
  type InterruptionHistoryEntry,
} from "./interruption-history-manager.js";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "InterruptionState" });

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
  /** Domain-specific execution context metadata */
  context?: ExecutionDomainContext;
}

/**
 * Interruption State Configuration
 */
export interface InterruptionStateConfig {
  /** Context ID (such as execution ID, session ID, etc.) */
  contextId: string;
  /** Domain-specific execution context metadata */
  context?: ExecutionDomainContext;
  /** EventRegistry for event emission and cascade propagation (optional, can be set later) */
  eventRegistry?: EventRegistry;
  /** Parent execution ID for cascade event subscription (optional, can be set later) */
  parentExecutionId?: string;
}

/**
 * Interruption State
 *
 * A general-purpose interrupt management component that supports shared use by both
 * the Graph module and the Agent module.
 *
 * ## Architecture
 *
 * ```
 * requestPause()
 *   → this.interruptionType = "PAUSE"        // primary state
 *   → abortController.abort(error)           // only for I/O cancellation
 *   → eventRegistry.emit({ type: "EXECUTION_PAUSED", executionId })
 * → Checkpoints: this.getInterruptionType() !== null  // direct state check
 * → Children: subscribe to parent's emitter via EventRegistry
 * ```
 *
 * The context field carries domain-specific metadata as a discriminated union
 * (ExecutionDomainContext). Use the `domain` discriminant to narrow the type:
 *   domain: "WORKFLOW_NODE" → { workflowId, nodeId, nodeExecutionId }
 *   domain: "AGENT_LOOP"   → { agentExecutionId, iteration }
 *   domain: "UNKNOWN"      → fallback with index signature
 *
 * This keeps InterruptionState portable while allowing each domain to attach
 * its own strongly-typed semantics.
 */
export class InterruptionState {
  private abortController: AbortController = new AbortController();
  private interruptionType: InterruptionType = null;
  private contextId: string;

  /** Domain-specific execution context metadata */
  private context: ExecutionDomainContext;

  /** EventRegistry for emitting interruption events (cascade + observability) */
  private eventRegistry?: EventRegistry;

  /** Subscription to parent execution's emitter for cascade propagation */
  private parentUnsubscribe?: () => void;
  /** Internal listeners for interruption state changes (used by TimeoutManager, etc.) */
  private interruptionListeners: Array<(type: "PAUSE" | "STOP" | "RESUME") => void> = [];
  /** Internal listeners for resume events specifically */
  private resumeListeners: Array<() => void> = [];

  /** Interruption history manager */
  private historyManager: InterruptionHistoryManager;

  /** Dispose flag to prevent use after disposal */
  private _disposed: boolean = false;

  // ========== Interruption Event Type Constants ==========
  // These abstract event names keep InterruptionState domain-agnostic.
  // The EventRegistry maps them as-is; workflow-specific consumers emit
  // their own rich events (WORKFLOW_EXECUTION_PAUSED etc.) separately.
  private static readonly EVENT_PAUSED = "EXECUTION_PAUSED";
  private static readonly EVENT_CANCELLED = "EXECUTION_CANCELLED";
  private static readonly EVENT_RESUMED = "EXECUTION_RESUMED";

  /**
   * Constructor
   * @param config Configuration options
   */
  constructor(config: InterruptionStateConfig & { historyMaxSize?: number }) {
    this.contextId = config.contextId;
    this.context = config.context ?? { domain: "UNKNOWN" };
    this.eventRegistry = config.eventRegistry;
    this.historyManager = new InterruptionHistoryManager(config.historyMaxSize ?? 1000);

    // If parent execution ID is provided, subscribe to parent's interruption events
    if (config.parentExecutionId && config.eventRegistry) {
      this.connectToParent(config.parentExecutionId);
    }
  }

  // ========== Command Entry Points ==========

  /**
   * Request to pause execution
   *
   * Sets interruptionType, aborts current AbortController (for I/O cancellation),
   * and emits interruption event via EventRegistry for cascade propagation.
   */
  requestPause(): void {
    if (this._disposed) {
      logger.warn("Cannot pause: InterruptionState already disposed", {
        contextId: this.contextId,
      });
      return;
    }
    if (this.interruptionType === "PAUSE") {
      return; // Already in a paused state
    }

    logger.info("Execution pause requested", { contextId: this.contextId, context: this.context });
    this.interruptionType = "PAUSE";

    // Create abort reason with interruptionType so checkExecutionInterruption can extract PAUSE type
    const pauseReason = new Error("Execution paused") as Error & {
      interruptionType: string;
      executionId: string;
    };
    pauseReason.interruptionType = "PAUSE";
    pauseReason.executionId = this.contextId;
    this.abortController.abort(pauseReason);

    // Record to history
    this.recordToHistory("PAUSE", "user");

    // Emit event for cascade propagation and observability
    this.emitInterruptionEvent("PAUSE");

    // Notify internal listeners
    this.notifyInterruptionListeners("PAUSE");
  }

  /**
   * Request to stop execution
   *
   * Sets interruptionType to STOP, aborts AbortController, and emits event.
   * If already PAUSE, upgrades to STOP.
   */
  requestStop(): void {
    if (this._disposed) {
      logger.warn("Cannot stop: InterruptionState already disposed", { contextId: this.contextId });
      return;
    }
    if (this.interruptionType === "STOP") {
      return; // Already in a stopped state
    }

    if (this.interruptionType === "PAUSE") {
      logger.info("Upgrading PAUSE to STOP", {
        contextId: this.contextId,
        context: this.context,
      });
    }

    logger.info("Execution stop requested", { contextId: this.contextId, context: this.context });
    this.interruptionType = "STOP";

    // Create abort reason with interruptionType so checkExecutionInterruption can extract STOP type
    const stopReason = new Error("Execution stopped") as Error & {
      interruptionType: string;
      executionId: string;
    };
    stopReason.interruptionType = "STOP";
    stopReason.executionId = this.contextId;
    this.abortController.abort(stopReason);

    // Record to history
    this.recordToHistory("STOP", "user");

    // Emit event for cascade propagation and observability
    this.emitInterruptionEvent("STOP");

    // Notify internal listeners
    this.notifyInterruptionListeners("STOP");
  }

  /**
   * Resume execution
   *
   * Resets interruptionType to null and creates a fresh AbortController
   * for the next execution phase. Emits resume event for cascade propagation.
   *
   * The new AbortController is scoped to the current execution phase only.
   * External consumers should call getAbortSignal() after resume to get the fresh signal.
   */
  resume(): void {
    if (this._disposed) {
      logger.warn("Cannot resume: InterruptionState already disposed", {
        contextId: this.contextId,
      });
      return;
    }

    if (this.interruptionType !== "PAUSE") {
      logger.warn("Cannot resume: not in a paused state", {
        contextId: this.contextId,
        currentState: this.interruptionType,
      });
      return;
    }

    logger.info("Execution resumed", { contextId: this.contextId, context: this.context });

    const pauseDuration = this.historyManager.getPauseDuration(this.contextId);

    this.interruptionType = null;

    // Create a new AbortController for fresh I/O cancellation
    this.abortController = new AbortController();

    // Record to history
    this.recordToHistory("RESUME", "user", pauseDuration ?? undefined);

    // Emit event for cascade propagation and observability
    this.emitInterruptionEvent("RESUME");

    // Notify internal listeners
    this.notifyInterruptionListeners("RESUME");
  }

  // ========== EventRegistry Integration ==========

  /**
   * Set EventRegistry for event emission (can be called after construction)
   */
  setEventRegistry(registry: EventRegistry): void {
    this.eventRegistry = registry;
  }

  /**
   * Subscribe to parent execution's interruption events for cascade propagation.
   * When parent emits PAUSE/STOP/RESUME events, this state will react accordingly.
   *
   * @param parentExecutionId The execution ID of the parent
   */
  connectToParent(parentExecutionId: string): void {
    if (!this.eventRegistry) {
      logger.warn("Cannot connect to parent: EventRegistry not set", {
        childContextId: this.contextId,
        parentExecutionId,
      });
      return;
    }

    // Cleanup previous subscription if any
    this.parentUnsubscribe?.();

    try {
      const parentEmitter = this.eventRegistry.getEmitter(parentExecutionId);

      // Subscribe to parent's interruption events
      this.parentUnsubscribe = parentEmitter.on(InterruptionState.EVENT_PAUSED, () => {
        this.requestPause();
      });

      // Also subscribe to STOP (CANCELLED) events
      this.parentUnsubscribe = combineUnsubscribe(
        this.parentUnsubscribe,
        parentEmitter.on(InterruptionState.EVENT_CANCELLED, () => {
          this.requestStop();
        }),
      );

      // Also subscribe to RESUME events
      this.parentUnsubscribe = combineUnsubscribe(
        this.parentUnsubscribe,
        parentEmitter.on(InterruptionState.EVENT_RESUMED, () => {
          this.resume();
        }),
      );

      logger.debug("Child interruption state connected to parent", {
        childContextId: this.contextId,
        parentExecutionId,
      });
    } catch (error) {
      logger.warn("Failed to connect to parent emitter for cascade", {
        childContextId: this.contextId,
        parentExecutionId,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Disconnect from parent (cleanup on child disposal)
   */
  disconnectFromParent(): void {
    this.parentUnsubscribe?.();
    this.parentUnsubscribe = undefined;
  }

  // ========== Internal Listeners ==========

  /**
   * Register a callback for interruption state changes (PAUSE/STOP/RESUME).
   * Used by internal consumers like TimeoutManager.
   *
   * @returns Unsubscribe function
   */
  onInterrupted(callback: (type: "PAUSE" | "STOP" | "RESUME") => void): () => void {
    this.interruptionListeners.push(callback);
    return () => {
      const idx = this.interruptionListeners.indexOf(callback);
      if (idx >= 0) {
        this.interruptionListeners.splice(idx, 1);
      }
    };
  }

  /**
   * Register a callback specifically for resume events.
   * Used by TimeoutManager to refresh timeouts on resume.
   *
   * @returns Unsubscribe function
   */
  onResumed(callback: () => void): () => void {
    this.resumeListeners.push(callback);
    return () => {
      const idx = this.resumeListeners.indexOf(callback);
      if (idx >= 0) {
        this.resumeListeners.splice(idx, 1);
      }
    };
  }

  /** Notify all internal interruption listeners */
  private notifyInterruptionListeners(type: "PAUSE" | "STOP" | "RESUME"): void {
    for (const listener of this.interruptionListeners) {
      try {
        listener(type);
      } catch (error) {
        logger.warn("Interruption listener error", {
          contextId: this.contextId,
          type,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    if (type === "RESUME") {
      for (const listener of this.resumeListeners) {
        try {
          listener();
        } catch (error) {
          logger.warn("Resume listener error", {
            contextId: this.contextId,
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }
  }

  // ========== State Query ==========

  /**
   * Get the interrupt type (primary coordination check mechanism)
   *
   * This is the main way to check for interruptions. Returns:
   * - "PAUSE": execution should pause
   * - "STOP": execution should stop
   * - null: execution should continue
   */
  getInterruptionType(): InterruptionType {
    return this.interruptionType;
  }

  /**
   * Get the AbortSignal for I/O cancellation
   *
   * Used ONLY for passing to native I/O APIs (fetch, streams, etc.).
   * Do NOT use this for coordination checks - use getInterruptionType() instead.
   * The signal is scoped per execution phase; after resume(), a new signal is created.
   */
  getAbortSignal(): AbortSignal {
    return this.abortController.signal;
  }

  /**
   * Check if execution is aborted (interrupted).
   * Equivalent to `getInterruptionType() !== null`.
   */
  isAborted(): boolean {
    return this.interruptionType !== null;
  }

  /**
   * Check if execution should pause
   */
  shouldPause(): boolean {
    return this.interruptionType === "PAUSE";
  }

  /**
   * Check if execution should stop
   */
  shouldStop(): boolean {
    return this.interruptionType === "STOP";
  }

  /**
   * Get the abort reason (for I/O error propagation)
   */
  getAbortReason(): Error | undefined {
    return this.abortController.signal.reason as Error | undefined;
  }

  /**
   * Update the domain-specific context (replace entirely)
   *
   * Unlike the previous merge behavior, this replaces the entire context object
   * to maintain the integrity of the discriminated union type (domain + fields).
   *
   * @param ctx New execution domain context. Use `{ domain: "UNKNOWN" }` to clear.
   */
  updateContext(ctx: ExecutionDomainContext): void {
    this.context = ctx;
  }

  /**
   * Get the context ID (execution ID)
   */
  getContextId(): string {
    return this.contextId;
  }

  /**
   * Get the domain-specific execution context metadata
   */
  getContext(): ExecutionDomainContext {
    return this.context;
  }

  // ========== History ==========

  /**
   * Get interruption history
   */
  getHistory(
    filter?: import("./interruption-history-manager.js").HistoryFilter,
  ): InterruptionHistoryEntry[] {
    return this.historyManager.getHistory(filter);
  }

  /**
   * Get interruption statistics
   */
  getStatistics() {
    return this.historyManager.getStatistics();
  }

  // ========== Lifecycle ==========

  /**
   * Cleanup resources (prevent memory leaks)
   *
   * Unlike abort, dispose does NOT trigger the AbortSignal, preventing
   * spurious interruption detection in downstream listeners.
   */
  dispose(): void {
    if (this._disposed) {
      return;
    }
    this._disposed = true;

    // Clear internal listeners to prevent stale references
    this.interruptionListeners = [];
    this.resumeListeners = [];

    // Disconnect from parent first
    this.disconnectFromParent();

    // Keep aboctController alive to maintain type safety for getAbortSignal() / getAbortReason().
    // After dispose these methods remain callable (returning a harmless unused signal)
    // rather than throwing on null — callers should check isDisposed() first.
    this.eventRegistry = undefined;

    logger.debug("InterruptionState disposed", {
      contextId: this.contextId,
    });
  }

  /**
   * Check if this InterruptionState has been disposed
   */
  isDisposed(): boolean {
    return this._disposed;
  }

  // ========== Private ==========

  /**
   * Record an interruption event to history, extracting domain-specific fields
   * from the typed ExecutionDomainContext using discriminated union narrowing.
   */
  private recordToHistory(
    type: "PAUSE" | "STOP" | "RESUME",
    triggeredBy: "user" | "system" | "timeout" | "error",
    duration?: number,
  ): void {
    const entry: Omit<InterruptionHistoryEntry, "id" | "timestamp"> = {
      type,
      contextId: this.contextId,
      triggeredBy,
      duration,
    };

    // Extract domain-specific fields from the typed context
    switch (this.context.domain) {
      case "WORKFLOW_NODE":
        entry.nodeId = this.context.nodeId;
        break;
      case "AGENT_LOOP":
        entry.iteration = this.context.iteration;
        break;
      case "UNKNOWN":
        break;
    }

    this.historyManager.record(entry);
  }

  /**
   * Emit interruption event via EventRegistry for cascade propagation.
   *
   * Children subscribe to the parent execution's emitter and react to these events.
   * This replaces the old InterruptionPropagationProxy mechanism.
   */
  private emitInterruptionEvent(command: "PAUSE" | "STOP" | "RESUME"): void {
    if (!this.eventRegistry) {
      return; // No EventRegistry, no event emission
    }

    let eventType: string;
    let reason: string | undefined;
    switch (command) {
      case "PAUSE":
        eventType = InterruptionState.EVENT_PAUSED;
        reason = "Execution paused";
        break;
      case "STOP":
        eventType = InterruptionState.EVENT_CANCELLED;
        reason = "Execution stopped";
        break;
      case "RESUME":
        eventType = InterruptionState.EVENT_RESUMED;
        reason = undefined;
        break;
    }

    const emitter = this.eventRegistry.getEmitter(this.contextId);

    try {
      emitter.emit({
        type: eventType,
        executionId: this.contextId,
        id: `evt_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        timestamp: Date.now(),
        context: { ...this.context },
        reason,
      } as any);
    } catch (error) {
      logger.warn("Failed to emit interruption event", {
        contextId: this.contextId,
        command,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}

/**
 * Combine multiple unsubscribe functions into one
 */
function combineUnsubscribe(...unsubFns: Array<() => void | undefined>): () => void {
  const fns = unsubFns.filter((fn): fn is () => void => typeof fn === "function");
  return () => {
    fns.forEach(fn => fn());
  };
}
