/**
 * Execution Event Logger
 *
 * Subscribes to execution events and logs them with detailed context.
 * Provides comprehensive logging for:
 * - State changes (status transitions)
 * - Error occurrences (with full error details)
 * - Execution interruptions (pauses and stops)
 * - Tool executions (success/failure tracking)
 * - Iteration lifecycle (start and completion)
 *
 * Design:
 * - Subscribes to ExecutionEventBus for real-time event notification
 * - Logs all significant events with appropriate detail levels
 * - Provides stack traces and debugging info for errors
 * - Non-blocking: errors in logging don't affect execution
 *
 * Note: This is separate from the execution state persistence.
 * Errors/interruptions are persisted in ExecutionState for recovery.
 * Logs are for debugging, analysis and real-time monitoring.
 */

import { getExecutionEventBus } from "../events/execution-event-bus.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type {
  ExecutionStateChangedEvent,
  ErrorOccurredEvent,
  InterruptionOccurredEvent,
  ToolExecutedEvent,
  IterationStartedEvent,
  IterationCompletedEvent,
} from "../events/execution-event-bus.js";

const logger = createContextualLogger({ component: "ExecutionEventLogger" });

/**
 * ExecutionEventLogger - Subscribes to execution events and logs them
 */
export class ExecutionEventLogger {
  /**
   * Initialize event logging by subscribing to the event bus
   * Call this once at application startup
   */
  static initialize(): void {
    const eventBus = getExecutionEventBus();

    // Subscribe to state changed events
    eventBus.on("state_changed", (event: ExecutionStateChangedEvent) => {
      ExecutionEventLogger.logStateChanged(event);
    });

    // Subscribe to error events
    eventBus.on("error_occurred", (event: ErrorOccurredEvent) => {
      ExecutionEventLogger.logError(event);
    });

    // Subscribe to interruption events
    eventBus.on("interruption_occurred", (event: InterruptionOccurredEvent) => {
      ExecutionEventLogger.logInterruption(event);
    });

    // Subscribe to tool execution events
    eventBus.on("tool_executed", (event: ToolExecutedEvent) => {
      ExecutionEventLogger.logToolExecution(event);
    });

    // Subscribe to iteration events
    eventBus.on("iteration_started", (event: IterationStartedEvent) => {
      ExecutionEventLogger.logIterationStart(event);
    });

    eventBus.on("iteration_completed", (event: IterationCompletedEvent) => {
      ExecutionEventLogger.logIterationComplete(event);
    });

    logger.info("Execution event logging initialized");
  }

  /**
   * Log state changed event
   */
  private static logStateChanged(event: ExecutionStateChangedEvent): void {
    logger.debug("Execution state changed", {
      executionId: event.executionId,
      previousStatus: event.previousStatus,
      newStatus: event.newStatus,
      changes: event.changes,
      timestamp: new Date(event.timestamp).toISOString(),
    });
  }

  /**
   * Log error event with full context
   */
  private static logError(event: ErrorOccurredEvent): void {
    logger.error("Execution error occurred", {
      executionId: event.executionId,
      errorId: event.error.id,
      errorMessage: event.error.message,
      errorCode: event.error.code,
      severity: event.error.severity,
      operation: event.error.context.operation,
      toolName: event.error.context.toolName,
      iteration: event.context?.iteration,
      isRecoverable: event.error.isRecoverable,
      recoveryAction: event.error.recoveryAction,
      input: event.error.context.input ? JSON.stringify(event.error.context.input) : undefined,
      timestamp: new Date(event.timestamp).toISOString(),
    });
  }

  /**
   * Log interruption event
   */
  private static logInterruption(event: InterruptionOccurredEvent): void {
    const message = `Execution ${event.interruption.type.toLowerCase()}`;

    if (event.interruption.type === "PAUSE") {
      logger.info(message, {
        interruptionId: event.interruption.id,
        type: event.interruption.type,
        reason: event.interruption.reason,
        iteration: event.interruption.iteration,
        status: event.interruption.status,
        timestamp: new Date(event.timestamp).toISOString(),
      });
    } else {
      logger.warn(message, {
        interruptionId: event.interruption.id,
        type: event.interruption.type,
        reason: event.interruption.reason,
        iteration: event.interruption.iteration,
        status: event.interruption.status,
        timestamp: new Date(event.timestamp).toISOString(),
      });
    }
  }

  /**
   * Log tool execution event
   */
  private static logToolExecution(event: ToolExecutedEvent): void {
    const message =
      event.status === "success" ? "Tool executed successfully" : `Tool execution ${event.status}`;

    if (event.status === "success") {
      logger.debug(message, {
        executionId: event.executionId,
        toolName: event.toolName,
        status: event.status,
        duration: event.duration,
        details: event.details,
        timestamp: new Date(event.timestamp).toISOString(),
      });
    } else {
      logger.warn(message, {
        executionId: event.executionId,
        toolName: event.toolName,
        status: event.status,
        duration: event.duration,
        details: event.details,
        timestamp: new Date(event.timestamp).toISOString(),
      });
    }
  }

  /**
   * Log iteration start
   */
  private static logIterationStart(event: IterationStartedEvent): void {
    logger.debug("Iteration started", {
      executionId: event.executionId,
      iteration: event.iteration,
      timestamp: new Date(event.timestamp).toISOString(),
    });
  }

  /**
   * Log iteration completion
   */
  private static logIterationComplete(event: IterationCompletedEvent): void {
    logger.debug("Iteration completed", {
      executionId: event.executionId,
      iteration: event.iteration,
      result: event.result ? JSON.stringify(event.result) : undefined,
      timestamp: new Date(event.timestamp).toISOString(),
    });
  }
}
