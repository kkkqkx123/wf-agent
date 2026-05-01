/**
 * Unified Error Handling Framework
 * 
 * Provides centralized error handling for all callback/event systems with configurable policies.
 * Supports retry, fallback, and fail-fast strategies across different error sources.
 */

import type { SDKError, ErrorContext } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "UnifiedErrorHandler" });

/**
 * Error source types
 */
export type ErrorSource =
  | "promise-callback"
  | "event-listener"
  | "storage-operation"
  | "serialization"
  | "workflow-execution"
  | "tool-execution"
  | "llm-call"
  | "unknown";

/**
 * Error severity levels (extends base severity with additional levels)
 */
export type ErrorHandlerSeverity = "info" | "warning" | "error" | "critical";

/**
 * Error handling action types
 */
export type ErrorHandlingAction =
  | "log" // Just log and continue
  | "retry" // Retry the operation
  | "fail-fast" // Stop execution immediately
  | "fallback" // Use fallback value
  | "ignore"; // Silently ignore

/**
 * Error context for handling decisions
 */
export interface ErrorHandlingContext {
  error: Error | SDKError;
  source: ErrorSource;
  severity: ErrorHandlerSeverity;
  metadata?: Record<string, unknown>;
  retryCount?: number;
  maxRetries?: number;
}

/**
 * Error handling result
 */
export interface ErrorHandlingResult {
  action: ErrorHandlingAction;
  retryCount?: number;
  fallbackValue?: unknown;
  delayMs?: number; // Delay before retry
}

/**
 * Error handler interface
 */
export interface ErrorHandler {
  /**
   * Handle an error and return handling result
   */
  handleError(context: ErrorHandlingContext): Promise<ErrorHandlingResult> | ErrorHandlingResult;
}

/**
 * Default error handler with configurable policies
 */
export class DefaultErrorHandler implements ErrorHandler {
  private readonly defaultMaxRetries: number;
  private readonly defaultRetryDelay: number;

  constructor(config?: { defaultMaxRetries?: number; defaultRetryDelay?: number }) {
    this.defaultMaxRetries = config?.defaultMaxRetries ?? 3;
    this.defaultRetryDelay = config?.defaultRetryDelay ?? 1000;
  }

  async handleError(context: ErrorHandlingContext): Promise<ErrorHandlingResult> {
    const { error, source, severity, retryCount = 0, maxRetries = this.defaultMaxRetries } = context;

    // Log the error
    this.logError(error, source, severity, context.metadata);

    // Determine action based on severity and source
    switch (severity) {
      case "critical":
        return { action: "fail-fast" };

      case "error":
        // For errors, check if retry is appropriate
        if (this.isRetryable(error, source) && retryCount < maxRetries) {
          return {
            action: "retry",
            retryCount: retryCount + 1,
            delayMs: this.calculateRetryDelay(retryCount),
          };
        }
        return { action: "fail-fast" };

      case "warning":
        // For warnings, continue with logging
        return { action: "log" };

      case "info":
        // For info, just log
        return { action: "ignore" };

      default:
        return { action: "log" };
    }
  }

  /**
   * Check if error is retryable based on source and error type
   */
  private isRetryable(error: Error | SDKError, source: ErrorSource): boolean {
    // Network-related errors are typically retryable
    if (source === "llm-call" || source === "tool-execution") {
      return true;
    }

    // Storage operations may be retryable (e.g., connection timeouts)
    if (source === "storage-operation") {
      return error.message.toLowerCase().includes("timeout") || error.message.toLowerCase().includes("network");
    }

    // Serialization errors are usually not retryable
    if (source === "serialization") {
      return false;
    }

    // Event listener errors are not retryable (listener already failed)
    if (source === "event-listener") {
      return false;
    }

    // Promise callback errors depend on the error type
    if (source === "promise-callback") {
      return error instanceof Error && !error.message.includes("cancelled");
    }

    return false;
  }

  /**
   * Calculate retry delay with exponential backoff
   */
  private calculateRetryDelay(retryCount: number): number {
    // Exponential backoff: baseDelay * 2^retryCount
    return this.defaultRetryDelay * Math.pow(2, retryCount);
  }

  /**
   * Log error with contextual information
   */
  private logError(
    error: Error | SDKError,
    source: ErrorSource,
    severity: ErrorHandlerSeverity,
    metadata?: Record<string, unknown>,
  ): void {
    const context: ErrorContext = {
      operation: "error-handling",
      resourceType: source,
      severity: severity === "critical" ? "error" : severity,
      ...metadata,
    };

    const errorDetails = {
      name: error.name,
      message: error.message,
      source,
      stack: error.stack,
    };

    switch (severity) {
      case "critical":
        logger.error("Critical error occurred", context, errorDetails);
        break;
      case "error":
        logger.error("Error occurred", context, errorDetails);
        break;
      case "warning":
        logger.warn("Warning occurred", context, errorDetails);
        break;
      case "info":
        logger.info("Info event", context, errorDetails);
        break;
    }
  }
}

/**
 * Unified Error Handler Manager
 * Manages error handlers for different sources and provides centralized error handling
 */
export class UnifiedErrorHandler {
  private static instance: UnifiedErrorHandler | null = null;
  private handlers: Map<ErrorSource, ErrorHandler> = new Map();
  private defaultHandler: ErrorHandler;

  private constructor() {
    this.defaultHandler = new DefaultErrorHandler();
  }

  /**
   * Get singleton instance
   */
  static getInstance(): UnifiedErrorHandler {
    if (!UnifiedErrorHandler.instance) {
      UnifiedErrorHandler.instance = new UnifiedErrorHandler();
    }
    return UnifiedErrorHandler.instance;
  }

  /**
   * Reset singleton instance (useful for testing)
   */
  static resetInstance(): void {
    UnifiedErrorHandler.instance = null;
  }

  /**
   * Register a custom error handler for a specific source
   */
  registerHandler(source: ErrorSource, handler: ErrorHandler): void {
    this.handlers.set(source, handler);
    logger.info(`Registered custom error handler for source: ${source}`);
  }

  /**
   * Set default error handler
   */
  setDefaultHandler(handler: ErrorHandler): void {
    this.defaultHandler = handler;
    logger.info("Set new default error handler");
  }

  /**
   * Handle an error using the appropriate handler
   */
  async handle(error: Error | SDKError, context: Omit<ErrorHandlingContext, "error">): Promise<ErrorHandlingResult> {
    const fullContext: ErrorHandlingContext = {
      error,
      ...context,
    };

    // Get handler for this source, or use default
    const handler = this.handlers.get(context.source) || this.defaultHandler;

    try {
      const result = await handler.handleError(fullContext);
      logger.debug("Error handled", {
        source: context.source,
        action: result.action,
        retryCount: result.retryCount,
      });
      return result;
    } catch (handlingError) {
      // If error handling itself fails, log and fail-fast
      logger.error("Error handler failed", {
        originalError: error.message,
        handlingError: handlingError instanceof Error ? handlingError.message : String(handlingError),
      });
      return { action: "fail-fast" };
    }
  }

  /**
   * Handle error with automatic retry support
   */
  async handleWithRetry<T>(
    operation: () => Promise<T>,
    context: Omit<ErrorHandlingContext, "error"> & { maxRetries?: number },
  ): Promise<T> {
    let lastError: Error | undefined;
    const maxRetries = context.maxRetries ?? 3;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        return await operation();
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        const result = await this.handle(lastError, {
          ...context,
          retryCount: attempt,
          maxRetries,
        });

        if (result.action === "fail-fast") {
          throw lastError;
        }

        if (result.action === "retry" && result.delayMs) {
          logger.debug(`Retrying after ${result.delayMs}ms`, {
            attempt: attempt + 1,
            maxRetries,
          });
          await this.sleep(result.delayMs);
        } else if (result.action === "fallback" && result.fallbackValue !== undefined) {
          return result.fallbackValue as T;
        } else if (result.action === "log" || result.action === "ignore") {
          // Continue to next iteration for retry
          continue;
        }
      }
    }

    // All retries exhausted
    throw lastError || new Error("Operation failed after all retries");
  }

  /**
   * Sleep utility for retry delays
   */
  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Clear all registered handlers
   */
  clearHandlers(): void {
    this.handlers.clear();
    logger.info("Cleared all registered error handlers");
  }
}

/**
 * Convenience function to get the unified error handler
 */
export function getErrorHandler(): UnifiedErrorHandler {
  return UnifiedErrorHandler.getInstance();
}

/**
 * Create a wrapped function with automatic error handling
 */
export function wrapWithErrorHandling<T extends (...args: any[]) => any>(
  fn: T,
  context: Omit<ErrorHandlingContext, "error">,
): T {
  return (async (...args: Parameters<T>): Promise<ReturnType<T>> => {
    const errorHandler = getErrorHandler();
    return errorHandler.handleWithRetry(
      () => fn(...args),
      context,
    ) as Promise<ReturnType<T>>;
  }) as unknown as T;
}
