/**
 * CallbackState - Callback State Manager
 *
 * Responsibilities:
 * - Manages callback functions for asynchronous thread execution
 * - Supports Promise-based callbacks
 * - Provides functions for registering, triggering, and cleaning up callbacks
 *
 * Design Principles:
 * - Stateful with multiple instances, held by the State Holder
 * - Thread-safe callback management
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
  /** Thread ID */
  threadId: string;
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
   * @param threadId Thread ID
   * @param resolve Promise resolve function
   * @param reject Promise reject function
   * @returns Whether the registration was successful
   */
  registerCallback(
    threadId: string,
    resolve: (value: T) => void,
    reject: (error: Error) => void,
  ): boolean {
    if (this.callbacks.has(threadId)) {
      return false;
    }

    const callbackInfo: GenericCallbackInfo<T> = {
      threadId,
      resolve,
      reject,
      registeredAt: now(),
    };

    this.callbacks.set(threadId, callbackInfo);
    return true;
  }

  /**
   * Trigger a successful callback
   * @param threadId: Thread ID
   * @param result: Execution result
   * @returns: Whether the trigger was successful
   */
  triggerCallback(threadId: string, result: T): boolean {
    const callbackInfo = this.callbacks.get(threadId);
    if (!callbackInfo) {
      return false;
    }

    try {
      // Call the resolve function.
      callbackInfo.resolve(result);

      // Remove from the callback mapping.
      this.callbacks.delete(threadId);
      return true;
    } catch (error) {
      const errorObj = getErrorOrNew(error);
      const sdkError = new SDKError(
        `Error triggering callback for thread ${threadId}`,
        "warning",
        { threadId },
        errorObj,
      );
      logError(sdkError, { threadId });
      this.callbacks.delete(threadId);
      return false;
    }
  }

  /**
   * Trigger a failure callback
   * @param threadId: Thread ID
   * @param error: Error message
   * @returns: Whether the trigger was successful
   */
  triggerErrorCallback(threadId: string, error: Error): boolean {
    const callbackInfo = this.callbacks.get(threadId);
    if (!callbackInfo) {
      return false;
    }

    try {
      // Call the reject function
      callbackInfo.reject(error);

      // Remove from the callback map.
      this.callbacks.delete(threadId);
      return true;
    } catch (err) {
      logger.error(`Error triggering error callback for thread ${threadId}`, { err, threadId });
      this.callbacks.delete(threadId);
      return false;
    }
  }

  /**
   * Check if the callback exists
   * @param threadId Thread ID
   * @returns Whether it exists
   */
  hasCallback(threadId: string): boolean {
    return this.callbacks.has(threadId);
  }

  /**
   * Retrieve callback information
   * @param threadId: Thread ID
   * @returns: Callback information
   */
  getCallback(threadId: string): GenericCallbackInfo<T> | undefined {
    return this.callbacks.get(threadId);
  }

  /**
   * Clean up all callbacks
   */
  cleanup(): void {
    // Loop through all callbacks and call the reject function.
    this.callbacks.forEach((callbackInfo, threadId) => {
      try {
        const error = new Error(`Callback cleanup for thread ${threadId}`);
        callbackInfo.reject(error);
      } catch (error) {
        logger.error(`Error cleaning up callback for thread ${threadId}`, { error, threadId });
      }
    });

    // Clear the callback mapping
    this.callbacks.clear();
  }

  /**
   * Clean up the callbacks for the specified thread
   * @param threadId Thread ID
   * @returns Whether the cleanup was successful
   */
  cleanupCallback(threadId: string): boolean {
    const callbackInfo = this.callbacks.get(threadId);
    if (!callbackInfo) {
      return false;
    }

    try {
      const error = new Error(`Callback cleanup for thread ${threadId}`);
      callbackInfo.reject(error);
    } catch (error) {
      logger.error(`Error cleaning up callback for thread ${threadId}`, { error, threadId });
    }

    this.callbacks.delete(threadId);
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
   * Get all thread IDs
   * @returns Array of thread IDs
   */
  getThreadIds(): string[] {
    return Array.from(this.callbacks.keys());
  }
}
