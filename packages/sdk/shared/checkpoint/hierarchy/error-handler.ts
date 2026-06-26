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

export class CheckpointErrorHandler {
  private strategy: CheckpointErrorStrategy;
  private onErrorCallback?: (error: CheckpointError, context: CheckpointErrorContext) => void | Promise<void>;
  private readonly logger: Logger;

  constructor(config: CheckpointErrorHandlerConfig, logger: Logger) {
    this.strategy = config.strategy;
    this.onErrorCallback = config.onError;
    this.logger = logger;
  }

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
        return this.handleWarn(checkpointError, context);
    }
  }

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

  private handleWarn(error: CheckpointError, context: CheckpointErrorContext): CheckpointErrorHandlingResult {
    this.logger.warn("Checkpoint operation failed", {
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
      trigger: context.triggerEvent,
      errorMessage: error.message,
      cause: error.cause?.message,
    });

    this.emitWarningEvent(error, context);

    return { shouldRethrow: false, handled: true };
  }

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

    this.emitErrorEvent(error, context);

    return { shouldRethrow: true, handled: true };
  }

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

  private emitWarningEvent(_error: CheckpointError, context: CheckpointErrorContext): void {
    this.logger.info("Checkpoint warning event emitted", {
      event: "checkpoint.warning",
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
    });
  }

  private emitErrorEvent(_error: CheckpointError, context: CheckpointErrorContext): void {
    this.logger.info("Checkpoint error event emitted", {
      event: "checkpoint.error",
      checkpointId: context.checkpointId,
      entityId: context.entityId,
      operation: context.operation,
    });
  }

  getStrategy(): CheckpointErrorStrategy {
    return this.strategy;
  }

  setStrategy(strategy: CheckpointErrorStrategy): void {
    this.strategy = strategy;
    this.logger.debug("Checkpoint error handling strategy changed", { newStrategy: strategy });
  }
}
