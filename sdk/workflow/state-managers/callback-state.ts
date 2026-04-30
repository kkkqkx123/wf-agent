/**
 * CallbackState - Callback State Manager
 *
 * Responsibilities:
 * - Manages callback functions for asynchronous workflow execution
 * - Supports Promise-based callbacks
 * - Provides functions for registering, triggering, and cleaning up callbacks
 *
 * Design Principles:
 * - Stateful with multiple instances, held by the State Holder
 * - Execution-safe callback management
 * - Supports generics to adapt to different result types
 */

import { now, getErrorOrNew } from "@wf-agent/common-utils";
import { SDKError } from "@wf-agent/types";
import { logError } from "../../core/utils/error-utils.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "CallbackState" });

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
 * CallbackState - Callback State Manager (Generic Version)
 * @typeparam T Result Type of the Execution
 */
export class CallbackState<T = unknown> {
  /**
   * Callback Mapping
   */
  private callbacks: Map<string, GenericCallbackInfo<T>> = new Map();

  /**
   * Register callback
   * @param executionId Execution ID
   * @param resolve Promise resolve function
   * @param reject Promise reject function
   * @returns Whether the registration was successful
   */
  registerCallback(
    executionId: string,
    resolve: (value: T) => void,
    reject: (error: Error) => void,
  ): boolean {
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
    return true;
  }

  /**
   * Trigger a successful callback
   * @param executionId: Execution ID
   * @param result: Execution result
   * @returns: Whether the trigger was successful
   */
  triggerCallback(executionId: string, result: T): boolean {
    const callbackInfo = this.callbacks.get(executionId);
    if (!callbackInfo) {
      return false;
    }

    try {
      // Call the resolve function.
      callbackInfo.resolve(result);

      // Remove from the callback mapping.
      this.callbacks.delete(executionId);
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
      return false;
    }
  }

  /**
   * Trigger a failure callback
   * @param executionId: Execution ID
   * @param error: Error message
   * @returns: Whether the trigger was successful
   */
  triggerErrorCallback(executionId: string, error: Error): boolean {
    const callbackInfo = this.callbacks.get(executionId);
    if (!callbackInfo) {
      return false;
    }

    try {
      // Call the reject function
      callbackInfo.reject(error);

      // Remove from the callback map.
      this.callbacks.delete(executionId);
      return true;
    } catch (err) {
      logger.error(`Error triggering error callback for workflow execution ${executionId}`, { err, executionId });
      this.callbacks.delete(executionId);
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
  cleanup(): void {
    // Loop through all callbacks and call the reject function.
    this.callbacks.forEach((callbackInfo, executionId) => {
      try {
        const error = new Error(`Callback cleanup for workflow execution ${executionId}`);
        callbackInfo.reject(error);
      } catch (error) {
        logger.error(`Error cleaning up callback for workflow execution ${executionId}`, { error, executionId });
      }
    });

    // Clear the callback mapping
    this.callbacks.clear();
  }

  /**
   * Clean up the callbacks for the specified thread
   * @param executionId Execution ID
   * @returns Whether the cleanup was successful
   */
  cleanupCallback(executionId: string): boolean {
    const callbackInfo = this.callbacks.get(executionId);
    if (!callbackInfo) {
      return false;
    }

    try {
      const error = new Error(`Callback cleanup for workflow execution ${executionId}`);
      callbackInfo.reject(error);
    } catch (error) {
      logger.error(`Error cleaning up callback for workflow execution ${executionId}`, { error, executionId });
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
  getThreadIds(): string[] {
    return Array.from(this.callbacks.keys());
  }
}
