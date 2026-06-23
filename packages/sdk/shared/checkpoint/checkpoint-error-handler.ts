/**
 * Checkpoint Error Handler
 *
 * Manages checkpoint operation errors based on configured strategy.
 * Supports multiple error handling modes:
 * - silent: Log debug only
 * - warn: Log warning but continue
 * - strict: Throw exception
 * - callback: Call user-provided handler
 */

import type {
  CheckpointErrorStrategy,
  CheckpointErrorContext,
  CheckpointErrorHandlerConfig,
  CheckpointErrorHandlingResult,
} from "@wf-agent/types";
import { CheckpointError } from "@wf-agent/types";

interface Logger {
  debug(msg: string, context?: Record<string, unknown>): void;
  info(msg: string, context?: Record<string, unknown>): void;
  warn(msg: string, context?: Record<string, unknown>): void;
  error(msg: string, context?: Record<string, unknown>): void;
}

/**
 * Checkpoint Error Handler
 * Handles errors based on configured strategy
 */
export class CheckpointErrorHandler {
  private strategy: CheckpointErrorStrategy;
  private onErrorCallback?: (error: CheckpointError, context: CheckpointErrorContext) => void | Promise<void>;
  private readonly logger: Logger;

  constructor(config: CheckpointErrorHandlerConfig, logger: Logger) {
    this.strategy = config.strategy;
    this.onErrorCallback = config.onError;
    this.logger = logger;
  }

  /**
   * Handle checkpoint error based on strategy
   *
   * @param error The error that occurred
   * @param context Error context information
   * @returns Result indicating whether to rethrow and if handled
   */
  async handleError(error: Error, context: CheckpointErrorContext): Promise<CheckpointErrorHandlingResult> {
    const checkpointError = this.wrapError(error, context);

    switch (this.strategy) {
      case "silent":
        return this.handleSilent(checkpointError, context);

      case "warn":
        return this.handleWarn(checkpointError, context);

      case "strict":
        return this.handleStrict(checkpointError, context);

      case "callback":
        return this.handleCallback(checkpointError, context);

      default:
        // Default to warn if strategy is unknown
        return this.handleWarn(checkpointError, context);
    }
  }

  /**
   * Silent failure - only debug logs, no rethrow
   */
  private handleSilent(error: CheckpointError, context: CheckpointErrorContext): CheckpointErrorHandlingResult {
    this.logger.debug("Checkpoint operation failed silently", {
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
      trigger: context.triggerEvent,
      errorMessage: error.message,
    });

    return { shouldRethrow: false, handled: true };
  }

  /**
   * Warning level - log warning but don't interrupt
   */
  private handleWarn(error: CheckpointError, context: CheckpointErrorContext): CheckpointErrorHandlingResult {
    this.logger.warn("Checkpoint operation failed", {
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
      trigger: context.triggerEvent,
      errorMessage: error.message,
      cause: error.cause?.message,
    });

    // Emit warning event for monitoring
    this.emitWarningEvent(error, context);

    return { shouldRethrow: false, handled: true };
  }

  /**
   * Strict mode - throw exception and interrupt
   */
  private handleStrict(error: CheckpointError, context: CheckpointErrorContext): CheckpointErrorHandlingResult {
    this.logger.error("Checkpoint operation failed (strict mode)", {
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
      trigger: context.triggerEvent,
      errorMessage: error.message,
      cause: error.cause?.message,
      stack: error.stack,
    });

    // Emit error event for monitoring
    this.emitErrorEvent(error, context);

    return { shouldRethrow: true, handled: true };
  }

  /**
   * Callback mode - call user-provided handler
   */
  private async handleCallback(
    error: CheckpointError,
    context: CheckpointErrorContext,
  ): Promise<CheckpointErrorHandlingResult> {
    this.logger.warn("Checkpoint operation failed (callback mode)", {
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
      trigger: context.triggerEvent,
      error: error.message,
    });

    if (this.onErrorCallback) {
      try {
        await this.onErrorCallback(error, context);
        this.logger.debug("Checkpoint error handler callback completed successfully");
      } catch (callbackError) {
        this.logger.error("Checkpoint error handler callback failed", {
          callbackError: callbackError instanceof Error ? callbackError.message : String(callbackError),
          originalError: error.message,
        });
        return { shouldRethrow: true, handled: false };
      }
    }

    return { shouldRethrow: false, handled: true };
  }

  /**
   * Wrap raw error in CheckpointError with context
   */
  private wrapError(error: Error, context: CheckpointErrorContext): CheckpointError {
    if (error instanceof CheckpointError) {
      return error;
    }

    return new CheckpointError(
      error.message,
      context.operation,
      context.entityId,
      context.checkpointId,
      error,
    );
  }

  /**
   * Emit warning event for monitoring systems
   */
  private emitWarningEvent(_error: CheckpointError, context: CheckpointErrorContext): void {
    this.logger.info("Checkpoint warning event emitted", {
      event: "checkpoint.warning",
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
    });
  }

  /**
   * Emit error event for monitoring systems
   */
  private emitErrorEvent(_error: CheckpointError, context: CheckpointErrorContext): void {
    this.logger.info("Checkpoint error event emitted", {
      event: "checkpoint.error",
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
    });
  }

  /**
   * Get current error handling strategy
   */
  getStrategy(): CheckpointErrorStrategy {
    return this.strategy;
  }

  /**
   * Change error handling strategy at runtime
   */
  setStrategy(strategy: CheckpointErrorStrategy): void {
    this.strategy = strategy;
    this.logger.debug("Checkpoint error handling strategy changed", { newStrategy: strategy });
  }
}
