/**
 * Timeout Configuration Processor
 * 
 * Provides functions for processing and merging timeout configuration.
 * This module handles the business logic for timeout config without file I/O.
 * 
 * Following the project architecture pattern:
 * - All configuration processing happens in api/shared/config layer
 * - Pure functions, no side effects
 * - No file I/O operations
 */

import type { TimeoutConfig } from "@wf-agent/types";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "TimeoutConfigProcessor" });

/**
 * Default timeout configuration
 * Matches current hardcoded values for backward compatibility
 * All values are in milliseconds
 */
const DEFAULT_TIMEOUT_CONFIG: Required<TimeoutConfig> = {
  workflowExecutionCompletion: 30000,   // 30 seconds
  workflowExecutionPause: 5000,         // 5 seconds
  workflowExecutionCancel: 10000,       // 10 seconds
  workflowExecutionResume: 5000,        // 5 seconds
  childExecutionWait: 30000,            // 30 seconds
  cascadeCancel: 30000,                 // 30 seconds
  nodeCompletion: 30000,                // 30 seconds
  nodeFailed: 30000,                    // 30 seconds
  syncBranchWait: 60000,                // 60 seconds
  joinCompletion: 60000,                // 60 seconds
  lifecycleEvent: 5000,                 // 5 seconds
  pollingWait: 30000,                   // 30 seconds
  pollingInterval: 100,                 // 100 milliseconds
  default: 30000,                       // 30 seconds
  maxAllowed: 300000,                   // 5 minutes
};

/**
 * Special constant for "wait forever"
 * When timeout equals this value, wait indefinitely
 */
export const WAIT_FOREVER = -1;

/**
 * Merge user config with defaults
 * Performs shallow merge of timeout configuration
 * 
 * @param userConfig - User-provided partial configuration
 * @returns Merged configuration with defaults applied
 */
export function mergeTimeoutWithDefaults(userConfig: Partial<TimeoutConfig>): Required<TimeoutConfig> {
  return {
    ...DEFAULT_TIMEOUT_CONFIG,
    ...userConfig,
  };
}

/**
 * Get default config for specific environment
 * Provides environment-optimized defaults
 * 
 * @param env - Environment name ("development" or "production")
 * @returns Environment-specific default configuration
 */
export function getTimeoutEnvironmentDefaults(env: "development" | "production"): Required<TimeoutConfig> {
  if (env === "development") {
    // Development: shorter timeouts for faster feedback
    return {
      ...DEFAULT_TIMEOUT_CONFIG,
      workflowExecutionCompletion: 15000,  // 15 seconds (faster for dev)
      childExecutionWait: 15000,           // 15 seconds
      nodeCompletion: 15000,               // 15 seconds
      pollingWait: 15000,                  // 15 seconds
    };
  }
  
  // Production defaults - more conservative
  return {
    ...DEFAULT_TIMEOUT_CONFIG,
    // Use standard production timeouts
  };
}

/**
 * Validate timeout value
 * 
 * @param timeout Timeout value in milliseconds
 * @param context Description of the operation for error messages
 * @throws Error if timeout is invalid
 */
export function validateTimeout(timeout: number, context: string): void {
  if (timeout < 0 && timeout !== WAIT_FOREVER) {
    throw new Error(
      `Invalid timeout for ${context}: ${timeout}ms (must be non-negative or WAIT_FOREVER)`
    );
  }
  
  if (timeout > DEFAULT_TIMEOUT_CONFIG.maxAllowed) {
    logger.warn(
      `Very long timeout for ${context}: ${timeout}ms (>${DEFAULT_TIMEOUT_CONFIG.maxAllowed / 1000}s). ` +
      `Consider if this is intentional.`
    );
  }
}

/**
 * Check if a timeout value means "wait forever"
 * 
 * @param timeout Timeout value to check
 * @returns true if timeout means wait forever
 */
export function isWaitForever(timeout: number): boolean {
  return timeout === WAIT_FOREVER;
}

/**
 * Convert timeout to actual value (handles WAIT_FOREVER)
 * 
 * @param timeout Timeout value (may be WAIT_FOREVER)
 * @returns Actual timeout in milliseconds, or undefined for WAIT_FOREVER
 */
export function toActualTimeout(timeout: number): number | undefined {
  return isWaitForever(timeout) ? undefined : timeout;
}
