/**
 * Interruption Timeout Manager
 * 
 * Provides timeout protection for interruption-related operations to prevent
 * indefinite hangs and resource leaks.
 * 
 * Features:
 * - Configurable timeouts for different operation types
 * - Automatic cleanup on operation completion
 * - Integration with AbortSignal for cancellation
 * - Event-based timeout notifications
 */

import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "InterruptionTimeoutManager" });

/**
 * Timeout configuration for different operation types
 */
export interface TimeoutConfig {
  /** Default timeout in milliseconds (30 seconds) */
  defaultTimeout: number;
  /** Hook execution timeout in milliseconds (10 seconds) */
  hookTimeout: number;
  /** Tool execution timeout in milliseconds (60 seconds) */
  toolExecutionTimeout: number;
  /** LLM call timeout in milliseconds (120 seconds) */
  llmCallTimeout: number;
}

/**
 * Default timeout configuration
 */
export const DEFAULT_TIMEOUT_CONFIG: TimeoutConfig = {
  defaultTimeout: 30000,      // 30 seconds
  hookTimeout: 10000,         // 10 seconds
  toolExecutionTimeout: 60000, // 60 seconds
  llmCallTimeout: 120000,     // 120 seconds
};

/**
 * Timeout entry for tracking active timeouts
 */
interface TimeoutEntry {
  operationId: string;
  timeoutId: NodeJS.Timeout;
  startTime: number;
  abortSignal?: AbortSignal;
  onTimeout?: () => void;
}

/**
 * Interruption Timeout Manager
 * 
 * Manages timeouts for interruption-related operations to prevent indefinite hangs.
 */
export class InterruptionTimeoutManager {
  private timeouts: Map<string, TimeoutEntry> = new Map();
  private config: TimeoutConfig;

  constructor(config?: Partial<TimeoutConfig>) {
    this.config = { ...DEFAULT_TIMEOUT_CONFIG, ...config };
  }

  /**
   * Set a timeout for an operation
   * 
   * @param operationId Unique identifier for the operation
   * @param timeoutMs Timeout duration in milliseconds (uses default if not specified)
   * @param options Optional configuration
   * @returns Function to clear the timeout manually
   */
  setTimeout(
    operationId: string,
    timeoutMs?: number,
    options?: {
      abortSignal?: AbortSignal;
      onTimeout?: () => void;
      operationType?: keyof TimeoutConfig;
    }
  ): () => void {
    // Clear any existing timeout for this operation
    this.clearTimeout(operationId);

    const actualTimeout = timeoutMs ?? (options?.operationType ? this.config[options.operationType] : this.config.defaultTimeout);
    
    const timeoutId = setTimeout(() => {
      this.handleTimeout(operationId);
    }, actualTimeout);

    const entry: TimeoutEntry = {
      operationId,
      timeoutId,
      startTime: Date.now(),
      abortSignal: options?.abortSignal,
      onTimeout: options?.onTimeout,
    };

    this.timeouts.set(operationId, entry);

    // Listen to abort signal if provided
    if (options?.abortSignal) {
      options.abortSignal.addEventListener(
        "abort",
        () => {
          this.clearTimeout(operationId);
        },
        { once: true }
      );
    }

    logger.debug("Timeout set for operation", {
      operationId,
      timeoutMs: actualTimeout,
      operationType: options?.operationType,
    });

    // Return cleanup function
    return () => this.clearTimeout(operationId);
  }

  /**
   * Clear a timeout for an operation
   * 
   * @param operationId Operation identifier
   */
  clearTimeout(operationId: string): void {
    const entry = this.timeouts.get(operationId);
    if (entry) {
      clearTimeout(entry.timeoutId);
      this.timeouts.delete(operationId);
      
      const duration = Date.now() - entry.startTime;
      logger.debug("Timeout cleared for operation", {
        operationId,
        duration,
      });
    }
  }

  /**
   * Handle timeout expiration
   * 
   * @param operationId Operation identifier
   */
  private handleTimeout(operationId: string): void {
    const entry = this.timeouts.get(operationId);
    if (!entry) {
      return;
    }

    const duration = Date.now() - entry.startTime;
    
    logger.warn("Operation timed out", {
      operationId,
      duration,
      timeoutMs: this.config.defaultTimeout,
    });

    // Call custom timeout handler if provided
    if (entry.onTimeout) {
      try {
        entry.onTimeout();
      } catch (error) {
        logger.error("Error in timeout handler", {
          operationId,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    // Clean up
    this.timeouts.delete(operationId);
  }

  /**
   * Execute an operation with timeout protection
   * 
   * @param operation The operation to execute
   * @param options Timeout configuration
   * @returns Promise resolving to operation result
   */
  async executeWithTimeout<T>(
    operation: () => Promise<T>,
    options: {
      operationId: string;
      timeoutMs?: number;
      abortSignal?: AbortSignal;
      operationType?: keyof TimeoutConfig;
      onTimeout?: () => void;
    }
  ): Promise<T> {
    return new Promise((resolve, reject) => {
      let completed = false;

      // Set timeout
      const cleanup = this.setTimeout(options.operationId, options.timeoutMs, {
        abortSignal: options.abortSignal,
        operationType: options.operationType,
        onTimeout: () => {
          if (!completed) {
            completed = true;
            reject(new Error(`Operation '${options.operationId}' timed out`));
            
            if (options.onTimeout) {
              try {
                options.onTimeout();
              } catch (error) {
                logger.error("Error in custom timeout handler", {
                  operationId: options.operationId,
                  error: error instanceof Error ? error.message : String(error),
                });
              }
            }
          }
        },
      });

      // Execute operation
      operation()
        .then(result => {
          if (!completed) {
            completed = true;
            cleanup();
            resolve(result);
          }
        })
        .catch(error => {
          if (!completed) {
            completed = true;
            cleanup();
            reject(error);
          }
        });

      // Listen to abort signal
      if (options.abortSignal) {
        options.abortSignal.addEventListener(
          "abort",
          () => {
            if (!completed) {
              completed = true;
              cleanup();
              reject(new Error(`Operation '${options.operationId}' aborted`));
            }
          },
          { once: true }
        );
      }
    });
  }

  /**
   * Get the number of active timeouts
   */
  getActiveTimeoutCount(): number {
    return this.timeouts.size;
  }

  /**
   * Clear all active timeouts
   */
  cleanup(): void {
    const count = this.timeouts.size;
    this.timeouts.forEach(entry => {
      clearTimeout(entry.timeoutId);
    });
    this.timeouts.clear();
    
    logger.info("All timeouts cleared", { count });
  }

  /**
   * Get timeout configuration
   */
  getConfig(): TimeoutConfig {
    return { ...this.config };
  }

  /**
   * Update timeout configuration
   */
  updateConfig(config: Partial<TimeoutConfig>): void {
    this.config = { ...this.config, ...config };
    logger.info("Timeout configuration updated", { config });
  }
}
