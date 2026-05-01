/**
 * PromiseResolutionManager - Promise Resolution State Manager
 *
 * Responsibilities:
 * - Manages Promise resolve/reject callbacks for asynchronous workflow execution
 * - Supports generic type parameters for different result types
 * - Provides functions for registering, triggering, and cleaning up callbacks
 * - Emits events for callback lifecycle stages (when EventRegistry is provided)
 *
 * Design Principles:
 * - Stateful with multiple instances, held by the State Holder
 * - Execution-safe callback management
 * - Integrated with event system for observability
 */

import { now, getErrorOrNew } from "@wf-agent/common-utils";
import { SDKError } from "@wf-agent/types";
import type { EventRegistry } from "../../core/registry/event-registry.js";
import { safeEmit } from "../../core/utils/event/event-emitter.js";
import { logError } from "../../core/utils/error-utils.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "PromiseResolutionManager" });

/**
 * Callback Information Interface (Generic Version)
 */
export interface GenericCallbackInfo<T> {
  /** Execution ID */
  executionId: string;
  /** Promise resolve function */
  resolve: (value: T) => void;
  /** Promise reject function */
  reject: (error: Error) => void;
  /** Registration time */
  registeredAt: number;
}

/**
 * PromiseResolutionManager - Promise Resolution State Manager (Generic Version)
 * @typeparam T Result Type of the Execution
 */
export class PromiseResolutionManager<T = unknown> {
  /**
   * Callback Mapping
   */
  private callbacks: Map<string, GenericCallbackInfo<T>> = new Map();

  /**
   * Optional EventRegistry for emitting lifecycle events
   */
  private eventManager?: EventRegistry;

  constructor(eventManager?: EventRegistry) {
    this.eventManager = eventManager;
  }

  /**
   * Register callback
   * @param executionId Execution ID
   * @param resolve Promise resolve function
   * @param reject Promise reject function
   * @returns Whether the registration was successful
   */
  async registerCallback(
    executionId: string,
    resolve: (value: T) => void,
    reject: (error: Error) => void,
  ): Promise<boolean> {
    if (this.callbacks.has(executionId)) {
      return false;
    }

    const callbackInfo: GenericCallbackInfo<T> = {
      executionId,
      resolve,
      reject,
      registeredAt: now(),
    };

    this.callbacks.set(executionId, callbackInfo);

    // Emit event for observability
    if (this.eventManager) {
      await safeEmit(this.eventManager, {
        type: "PROMISE_CALLBACK_REGISTERED",
        executionId,
        timestamp: now(),
      }).catch((error) => {
        logger.warn("Failed to emit PROMISE_CALLBACK_REGISTERED event", { error, executionId });
      });
    }

    return true;
  }

  /**
   * Trigger a successful callback
   * @param executionId: Execution ID
   * @param result: Execution result
   * @returns: Whether the trigger was successful
   */
  async triggerCallback(executionId: string, result: T): Promise<boolean> {
    const callbackInfo = this.callbacks.get(executionId);
    if (!callbackInfo) {
      return false;
    }

    try {
      // Call the resolve function.
      callbackInfo.resolve(result);

      // Remove from the callback mapping.
      this.callbacks.delete(executionId);

      // Emit event for observability
      if (this.eventManager) {
        await safeEmit(this.eventManager, {
          type: "PROMISE_CALLBACK_RESOLVED",
          executionId,
          timestamp: now(),
        }).catch((error) => {
          logger.warn("Failed to emit PROMISE_CALLBACK_RESOLVED event", { error, executionId });
        });
      }

      return true;
    } catch (error) {
      const errorObj = getErrorOrNew(error);
      const sdkError = new SDKError(
        `Error triggering callback for workflow execution ${executionId}`,
        "warning",
        { executionId },
        errorObj,
      );
      logError(sdkError, { executionId });
      this.callbacks.delete(executionId);

      // Emit failure event
      if (this.eventManager) {
        await safeEmit(this.eventManager, {
          type: "PROMISE_CALLBACK_FAILED",
          executionId,
          error: sdkError.message,
          timestamp: now(),
        }).catch((err) => {
          logger.warn("Failed to emit PROMISE_CALLBACK_FAILED event", { err, executionId });
        });
      }

      return false;
    }
  }

  /**
   * Trigger a failure callback
   * @param executionId: Execution ID
   * @param error: Error message
   * @returns: Whether the trigger was successful
   */
  async triggerErrorCallback(executionId: string, error: Error): Promise<boolean> {
    const callbackInfo = this.callbacks.get(executionId);
    if (!callbackInfo) {
      return false;
    }

    try {
      // Call the reject function
      callbackInfo.reject(error);

      // Remove from the callback map.
      this.callbacks.delete(executionId);

      // Emit event for observability
      if (this.eventManager) {
        await safeEmit(this.eventManager, {
          type: "PROMISE_CALLBACK_REJECTED",
          executionId,
          errorMessage: error.message,
          timestamp: now(),
        }).catch((err) => {
          logger.warn("Failed to emit PROMISE_CALLBACK_REJECTED event", { err, executionId });
        });
      }

      return true;
    } catch (err) {
      logger.error(`Error triggering error callback for workflow execution ${executionId}`, {
        err,
        executionId,
      });
      this.callbacks.delete(executionId);

      // Emit failure event
      if (this.eventManager) {
        await safeEmit(this.eventManager, {
          type: "PROMISE_CALLBACK_FAILED",
          executionId,
          error: err instanceof Error ? err.message : String(err),
          timestamp: now(),
        }).catch((error) => {
          logger.warn("Failed to emit PROMISE_CALLBACK_FAILED event", { error, executionId });
        });
      }

      return false;
    }
  }

  /**
   * Check if the callback exists
   * @param executionId Execution ID
   * @returns Whether it exists
   */
  hasCallback(executionId: string): boolean {
    return this.callbacks.has(executionId);
  }

  /**
   * Retrieve callback information
   * @param executionId: Execution ID
   * @returns: Callback information
   */
  getCallback(executionId: string): GenericCallbackInfo<T> | undefined {
    return this.callbacks.get(executionId);
  }

  /**
   * Clean up all callbacks
   */
  async cleanup(): Promise<void> {
    const executionIds = Array.from(this.callbacks.keys());

    // Loop through all callbacks and call the reject function.
    for (const executionId of executionIds) {
      const callbackInfo = this.callbacks.get(executionId);
      if (!callbackInfo) continue;

      try {
        const error = new Error(`Callback cleanup for workflow execution ${executionId}`);
        callbackInfo.reject(error);

        // Emit cleanup event
        if (this.eventManager) {
          await safeEmit(this.eventManager, {
            type: "PROMISE_CALLBACK_CLEANED_UP",
            executionId,
            reason: "global_cleanup",
            timestamp: now(),
          }).catch((err) => {
            logger.warn("Failed to emit PROMISE_CALLBACK_CLEANED_UP event", { err, executionId });
          });
        }
      } catch (error) {
        logger.error(`Error cleaning up callback for workflow execution ${executionId}`, {
          error,
          executionId,
        });
      }
    }

    // Clear the callback mapping
    this.callbacks.clear();
  }

  /**
   * Clean up the callbacks for the specified workflow execution
   * @param executionId Execution ID
   * @returns Whether the cleanup was successful
   */
  async cleanupCallback(executionId: string): Promise<boolean> {
    const callbackInfo = this.callbacks.get(executionId);
    if (!callbackInfo) {
      return false;
    }

    try {
      const error = new Error(`Callback cleanup for workflow execution ${executionId}`);
      callbackInfo.reject(error);

      // Emit cleanup event
      if (this.eventManager) {
        await safeEmit(this.eventManager, {
          type: "PROMISE_CALLBACK_CLEANED_UP",
          executionId,
          reason: "individual_cleanup",
          timestamp: now(),
        }).catch((err) => {
          logger.warn("Failed to emit PROMISE_CALLBACK_CLEANED_UP event", { err, executionId });
        });
      }
    } catch (error) {
      logger.error(`Error cleaning up callback for workflow execution ${executionId}`, {
        error,
        executionId,
      });
    }

    this.callbacks.delete(executionId);
    return true;
  }

  /**
   * Get the number of callbacks
   * @returns The number of callbacks
   */
  size(): number {
    return this.callbacks.size;
  }

  /**
   * Get all execution IDs
   * @returns Array of execution IDs
   */
  getExecutionIds(): string[] {
    return Array.from(this.callbacks.keys());
  }
}

