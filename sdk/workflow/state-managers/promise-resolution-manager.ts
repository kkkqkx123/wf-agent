/**
 * AsyncCompletionManager - Asynchronous Task Completion Manager
 *
 * Responsibilities:
 * - Manages completion callbacks for asynchronously executed tasks
 * - Provides notification handlers when async tasks complete or fail
 * - Supports cleanup during shutdown scenarios
 * - Emits events for callback lifecycle stages (when EventRegistry is provided)
 *
 * Design Principles:
 * - Simple callback registry without Promise semantics
 * - Execution-safe callback management
 * - Integrated with event system for observability
 *
 * Note:
 * - This is NOT for Promise resolution (that's handled by TaskQueue for sync execution)
 * - This is purely for async task completion notifications
 */

import { now, getErrorOrNew } from "@wf-agent/common-utils";
import { SDKError } from "@wf-agent/types";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import { emit } from "../../core/utils/event/event-emitter.js";
import {
  buildPromiseCallbackRegisteredEvent,
  buildPromiseCallbackResolvedEvent,
  buildPromiseCallbackRejectedEvent,
  buildPromiseCallbackFailedEvent,
  buildPromiseCallbackCleanedUpEvent,
} from "../../core/utils/event/builders/promise-callback-events.js";
import { logError } from "../../core/utils/error-utils.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type { StateManager } from "../../core/types/state-manager.js";

const logger = createContextualLogger({ component: "PromiseResolutionManager" });

/**
 * Completion Handler Interface
 */
export interface CompletionHandler<T> {
  /** Execution ID */
  executionId: string;
  /** Success callback */
  onComplete: (value: T) => void;
  /** Error callback */
  onError: (error: Error) => void;
  /** Registration time */
  registeredAt: number;
}

/**
 * AsyncCompletionManager - Asynchronous Task Completion Manager
 * @typeparam T Result Type of the Task
 */
export class AsyncCompletionManager<T = unknown> implements StateManager<Map<string, CompletionHandler<T>>> {
  /**
   * Completion Handler Mapping
   */
  private handlers: Map<string, CompletionHandler<T>> = new Map();

  /**
   * Optional EventRegistry for emitting lifecycle events
   */
  private eventManager?: EventRegistry;

  constructor(eventManager?: EventRegistry) {
    this.eventManager = eventManager;
  }

  /**
   * Register completion handler
   * @param executionId Execution ID
   * @param onComplete Success callback
   * @param onError Error callback
   * @returns Whether the registration was successful
   */
  async registerHandler(
    executionId: string,
    onComplete: (value: T) => void,
    onError: (error: Error) => void,
  ): Promise<boolean> {
    if (this.handlers.has(executionId)) {
      return false;
    }

    const handler: CompletionHandler<T> = {
      executionId,
      onComplete,
      onError,
      registeredAt: now(),
    };

    this.handlers.set(executionId, handler);

    // Emit event for observability (non-critical)
    if (this.eventManager) {
      try {
        const event = buildPromiseCallbackRegisteredEvent({ executionId });
        await emit(this.eventManager, event);
      } catch (error) {
        logger.warn("Failed to emit ASYNC_COMPLETION_REGISTERED event", { 
          error: getErrorOrNew(error).message, 
          executionId 
        });
      }
    }

    return true;
  }

  /**
   * Trigger completion handler
   * @param executionId: Execution ID
   * @param result: Task result
   * @returns: Whether the trigger was successful
   */
  async triggerCompletion(executionId: string, result: T): Promise<boolean> {
    const handler = this.handlers.get(executionId);
    if (!handler) {
      return false;
    }

    try {
      // Call the completion callback.
      handler.onComplete(result);

      // Remove from the handler mapping.
      this.handlers.delete(executionId);

      // Emit event for observability (non-critical)
      if (this.eventManager) {
        try {
          const event = buildPromiseCallbackResolvedEvent({ executionId });
          await emit(this.eventManager, event);
        } catch (error) {
          logger.warn("Failed to emit ASYNC_COMPLETION_TRIGGERED event", { 
            error: getErrorOrNew(error).message, 
            executionId 
          });
        }
      }

      return true;
    } catch (error) {
      const errorObj = getErrorOrNew(error);
      const sdkError = new SDKError(
        `Error triggering completion handler for execution ${executionId}`,
        "warning",
        { executionId },
        errorObj,
      );
      logError(sdkError, { executionId });
      this.handlers.delete(executionId);

      // Emit failure event (non-critical)
      if (this.eventManager) {
        try {
          const event = buildPromiseCallbackFailedEvent({ executionId, error: sdkError });
          await emit(this.eventManager, event);
        } catch (error) {
          logger.warn("Failed to emit ASYNC_COMPLETION_FAILED event", { 
            error: getErrorOrNew(error).message, 
            executionId 
          });
        }
      }

      return false;
    }
  }

  /**
   * Trigger error handler
   * @param executionId: Execution ID
   * @param error: Error message
   * @returns: Whether the trigger was successful
   */
  async triggerError(executionId: string, error: Error): Promise<boolean> {
    const handler = this.handlers.get(executionId);
    if (!handler) {
      return false;
    }

    try {
      // Call the error callback
      handler.onError(error);

      // Remove from the handler map.
      this.handlers.delete(executionId);

      // Emit event for observability (non-critical)
      if (this.eventManager) {
        try {
          const event = buildPromiseCallbackRejectedEvent({ executionId, error });
          await emit(this.eventManager, event);
        } catch (error) {
          logger.warn("Failed to emit ASYNC_ERROR_TRIGGERED event", { 
            error: getErrorOrNew(error).message, 
            executionId 
          });
        }
      }

      return true;
    } catch (err) {
      logger.error(`Error triggering error handler for execution ${executionId}`, {
        err,
        executionId,
      });
      this.handlers.delete(executionId);

      // Emit failure event (non-critical)
      if (this.eventManager) {
        try {
          const event = buildPromiseCallbackFailedEvent({ 
            executionId, 
            error: err instanceof Error ? err : new Error(String(err))
          });
          await emit(this.eventManager, event);
        } catch (error) {
          logger.warn("Failed to emit ASYNC_COMPLETION_FAILED event", { 
            error: getErrorOrNew(error).message, 
            executionId 
          });
        }
      }

      return false;
    }
  }

  /**
   * Check if the handler exists
   * @param executionId Execution ID
   * @returns Whether it exists
   */
  hasHandler(executionId: string): boolean {
    return this.handlers.has(executionId);
  }

  /**
   * Retrieve handler information
   * @param executionId: Execution ID
   * @returns: Handler information
   */
  getHandler(executionId: string): CompletionHandler<T> | undefined {
    return this.handlers.get(executionId);
  }

  /**
   * Clean up all handlers
   */
  async cleanup(): Promise<void> {
    const executionIds = Array.from(this.handlers.keys());

    // Loop through all handlers and call the error callback.
    for (const executionId of executionIds) {
      const handler = this.handlers.get(executionId);
      if (!handler) continue;

      try {
        const error = new Error(`Handler cleanup for execution ${executionId}`);
        handler.onError(error);

        // Emit cleanup event (non-critical)
        if (this.eventManager) {
          try {
            const event = buildPromiseCallbackCleanedUpEvent({ 
              executionId,
              reason: "global_cleanup"
            });
            await emit(this.eventManager, event);
          } catch (error) {
            logger.warn("Failed to emit ASYNC_COMPLETION_CLEANED_UP event", { 
              error: getErrorOrNew(error).message, 
              executionId 
            });
          }
        }
      } catch (error) {
        logger.error(`Error cleaning up handler for execution ${executionId}`, {
          error,
          executionId,
        });
      }
    }

    // Clear the handler mapping
    this.handlers.clear();
  }

  /**
   * Clean up the handler for the specified execution
   * @param executionId Execution ID
   * @returns Whether the cleanup was successful
   */
  async cleanupHandler(executionId: string): Promise<boolean> {
    const handler = this.handlers.get(executionId);
    if (!handler) {
      return false;
    }

    try {
      const error = new Error(`Handler cleanup for execution ${executionId}`);
      handler.onError(error);

      // Emit cleanup event (non-critical)
      if (this.eventManager) {
        try {
          const event = buildPromiseCallbackCleanedUpEvent({ 
            executionId,
            reason: "individual_cleanup"
          });
          await emit(this.eventManager, event);
        } catch (error) {
          logger.warn("Failed to emit ASYNC_COMPLETION_CLEANED_UP event", { 
            error: getErrorOrNew(error).message, 
            executionId 
          });
        }
      }
    } catch (error) {
      logger.error(`Error cleaning up handler for execution ${executionId}`, {
        error,
        executionId,
      });
    }

    this.handlers.delete(executionId);
    return true;
  }

  /**
   * Get the number of handlers
   * @returns The number of handlers
   */
  size(): number {
    return this.handlers.size;
  }

  /**
   * Check if there are no handlers registered
   * @returns true if no handlers exist
   */
  isEmpty(): boolean {
    return this.handlers.size === 0;
  }

  /**
   * Create a snapshot of all completion handlers
   * @returns Snapshot containing all handler data
   */
  createSnapshot(): Map<string, CompletionHandler<T>> {
    const snapshot = new Map<string, CompletionHandler<T>>();
    for (const [executionId, handler] of this.handlers.entries()) {
      snapshot.set(executionId, { ...handler });
    }
    return snapshot;
  }

  /**
   * Restore completion handlers from snapshot
   * @param snapshot The handler snapshot
   */
  restoreFromSnapshot(snapshot: Map<string, CompletionHandler<T>>): void {
    this.handlers.clear();
    for (const [executionId, handler] of snapshot.entries()) {
      this.handlers.set(executionId, { ...handler });
    }
  }

  /**
   * Reset to initial state
   * Clears all completion handlers
   */
  reset(): void {
    this.cleanup();
  }

  /**
   * Get all execution IDs
   * @returns Array of execution IDs
   */
  getExecutionIds(): string[] {
    return Array.from(this.handlers.keys());
  }
}

// Backward compatibility alias (deprecated)
/** @deprecated Use AsyncCompletionManager instead */
export const PromiseResolutionManager = AsyncCompletionManager;

