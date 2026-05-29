/**
 * Timeout Strategies
 * 
 * Advanced timeout strategies for different use cases.
 */

import type { TimeoutRegistration, TimeoutHandle } from "../../types/timeout.js";
import type { TimeoutManager } from "../../state-managers/timeout-manager.js";

/**
 * Idle Timeout Options
 */
export interface IdleTimeoutOptions extends Omit<TimeoutRegistration, "duration"> {
  /** Idle duration in milliseconds before timeout triggers */
  idleDuration: number;

  /** Function to detect if activity is still happening */
  activityDetector: () => boolean;

  /** Polling interval to check activity (ms) */
  checkInterval?: number;
}

/**
 * Register an idle timeout that triggers after a period of inactivity
 * 
 * The timeout resets whenever activity is detected.
 * 
 * @param manager TimeoutManager instance
 * @param options Idle timeout configuration
 * @returns TimeoutHandle
 * 
 * @example
 * ```typescript
 * const handle = registerIdleTimeout(manager, {
 *   id: 'idle-check',
 *   idleDuration: 60000,
 *   activityDetector: () => isProcessing(),
 *   onTimeout: () => cleanup()
 * });
 * ```
 */
export function registerIdleTimeout(
  manager: TimeoutManager,
  options: IdleTimeoutOptions
): TimeoutHandle {
  const {
    idleDuration,
    activityDetector,
    checkInterval = 1000,
    ...registrationOptions
  } = options;

  let checkTimerId: NodeJS.Timeout | undefined = undefined;

  // Create the actual timeout registration
  const handle = manager.register({
    ...registrationOptions,
    duration: idleDuration,
    onTimeout: async () => {
      // Clear the activity checker
      if (checkTimerId) {
        clearInterval(checkTimerId);
      }

      // Execute the original timeout callback if provided
      if (options.onTimeout) {
        await options.onTimeout();
      }
    },
  });

  // Set up activity checker
  checkTimerId = setInterval(() => {
    if (!handle.isActive()) {
      // Timeout already expired or cancelled
      clearInterval(checkTimerId!);
      return;
    }

    if (activityDetector()) {
      // Activity detected, refresh timeout
      manager.refresh(handle);
    }
  }, checkInterval);

  return handle;
}

/**
 * Hierarchical Timeout Options
 */
export interface HierarchicalTimeoutOptions extends TimeoutRegistration {
  /** Child timeout handles that should be tracked */
  children: TimeoutHandle[];

  /** Callback when any child times out */
  onChildTimeout?: (childId: string) => void | Promise<void>;
}

/**
 * Register a hierarchical timeout with parent-child relationships
 * 
 * The parent timeout can monitor and react to child timeouts.
 * 
 * @param manager TimeoutManager instance
 * @param options Hierarchical timeout configuration
 * @returns TimeoutHandle for the parent timeout
 * 
 * @example
 * ```typescript
 * const child1 = manager.register({ id: 'child1', duration: 5000, ... });
 * const child2 = manager.register({ id: 'child2', duration: 5000, ... });
 * 
 * const parent = registerHierarchicalTimeout(manager, {
 *   id: 'parent',
 *   duration: 15000,
 *   children: [child1, child2],
 *   onChildTimeout: (childId) => console.log(`${childId} timed out`)
 * });
 * ```
 */
export function registerHierarchicalTimeout(
  manager: TimeoutManager,
  options: HierarchicalTimeoutOptions
): TimeoutHandle {
  const { children, onChildTimeout } = options;

  // Track child timeouts
  const childCleanupFunctions: Array<() => void> = [];

  children.forEach((child) => {
    // Monitor child status by polling
    // NOTE: This polling is necessary because TimeoutHandle doesn't provide event notifications
    // for expiration/cancellation. If TimeoutHandle adds event support in the future, this
    // should be refactored to use events instead of polling.
    const checkInterval = setInterval(() => {
      if (!child.isActive()) {
        // Child has expired or been cancelled
        clearInterval(checkInterval);

        // Notify parent
        if (onChildTimeout) {
          const result = onChildTimeout(child.id);
          if (result instanceof Promise) {
            result.catch((error: Error) => {
              console.error(`Error in onChildTimeout callback:`, error);
            });
          }
        }
      }
    }, 100);

    childCleanupFunctions.push(() => clearInterval(checkInterval));
  });

  // Register parent timeout
  const parentHandle = manager.register({
    ...options,
    onTimeout: async () => {
      // Clean up child monitors
      childCleanupFunctions.forEach((cleanup) => cleanup());

      // Cancel all active children
      children.forEach((child) => {
        if (child.isActive()) {
          child.cancel();
        }
      });

      // Execute parent timeout callback if provided
      if (options.onTimeout) {
        await options.onTimeout();
      }
    },
  });

  // Override cancel to also clean up child monitors
  const originalCancel = parentHandle.cancel.bind(parentHandle);
  parentHandle.cancel = () => {
    childCleanupFunctions.forEach((cleanup) => cleanup());
    originalCancel();
  };

  return parentHandle;
}

/**
 * Two-Stage Timeout Options
 */
export interface TwoStageTimeoutOptions extends TimeoutRegistration {
  /** Warning threshold in milliseconds before final timeout */
  warningThreshold: number;

  /** Callback invoked at warning stage */
  onWarning: () => void | Promise<void>;
}

/**
 * Register a two-stage timeout with warning and final stages
 * 
 * This is a convenience wrapper around the built-in warning support.
 * 
 * @param manager TimeoutManager instance
 * @param options Two-stage timeout configuration
 * @returns TimeoutHandle
 * 
 * @example
 * ```typescript
 * const handle = registerTwoStageTimeout(manager, {
 *   id: 'operation',
 *   duration: 300000, // 5 minutes
 *   warningThreshold: 60000, // Warn at 4 minutes
 *   onWarning: () => sendAlert('Operation taking long'),
 *   onTimeout: () => cancelOperation()
 * });
 * ```
 */
export function registerTwoStageTimeout(
  manager: TimeoutManager,
  options: TwoStageTimeoutOptions
): TimeoutHandle {
  return manager.register({
    id: options.id,
    duration: options.duration,
    warningThreshold: options.warningThreshold,
    onWarning: options.onWarning,
    onTimeout: options.onTimeout,
    interruptionState: options.interruptionState,
    tag: options.tag,
    executionId: options.executionId,
    metadata: options.metadata,
  });
}

/**
 * Retry with Timeout Options
 */
export interface RetryWithTimeoutOptions<T> {
  /** Function to retry */
  fn: () => Promise<T>;

  /** Maximum number of retries */
  maxRetries: number;

  /** Base timeout for each attempt (ms) */
  baseTimeout: number;

  /** Maximum timeout across all retries (ms) */
  maxTimeout?: number;

  /** Delay between retries (ms) */
  retryDelay?: number;

  /** Callback on each retry */
  onRetry?: (attempt: number, error: Error) => void | Promise<void>;
}

/**
 * Execute a function with retry and adaptive timeout
 * 
 * Each retry gets an exponentially increasing timeout.
 * 
 * @param options Retry configuration
 * @returns Result of the function
 * 
 * @example
 * ```typescript
 * const result = await retryWithTimeout({
 *   fn: () => fetchData(),
 *   maxRetries: 3,
 *   baseTimeout: 5000,
 *   maxTimeout: 30000,
 *   retryDelay: 1000
 * });
 * ```
 */
export async function retryWithTimeout<T>(
  options: RetryWithTimeoutOptions<T>
): Promise<T> {
  const {
    fn,
    maxRetries,
    baseTimeout,
    maxTimeout = baseTimeout * Math.pow(2, maxRetries),
    retryDelay = 0,
    onRetry,
  } = options;

  let lastError: Error | undefined;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      // Calculate adaptive timeout for this attempt
      const timeout = calculateAdaptiveTimeout(baseTimeout, attempt, maxTimeout);

      // Execute with timeout
      return await executeWithTimeout(fn, timeout);
    } catch (error) {
      lastError = error as Error;

      // If this was the last attempt, throw the error
      if (attempt === maxRetries) {
        break;
      }

      // Call retry callback
      if (onRetry) {
        await onRetry(attempt + 1, lastError);
      }

      // Wait before retry
      if (retryDelay > 0) {
        await new Promise((resolve) => setTimeout(resolve, retryDelay));
      }
    }
  }

  throw lastError || new Error("Unknown error during retry");
}

/**
 * Helper: Execute with timeout
 */
async function executeWithTimeout<T>(fn: () => Promise<T>, timeout: number): Promise<T> {
  let timeoutHandle: NodeJS.Timeout;

  const timeoutPromise = new Promise<never>((_, reject) => {
    timeoutHandle = setTimeout(() => {
      reject(new Error(`Operation timed out after ${timeout}ms`));
    }, timeout);
  });

  try {
    return await Promise.race([fn(), timeoutPromise]);
  } finally {
    clearTimeout(timeoutHandle!);
  }
}

/**
 * Helper: Calculate adaptive timeout
 */
function calculateAdaptiveTimeout(baseTimeout: number, retryCount: number, maxTimeout: number): number {
  const calculatedTimeout = baseTimeout * Math.pow(2, retryCount);
  return Math.min(calculatedTimeout, maxTimeout);
}
