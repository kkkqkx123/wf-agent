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

import { getExecutionEventBus } from "./execution-event-bus.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";
import type {
  ExecutionStateChangedEvent,
  ErrorOccurredEvent,
  InterruptionOccurredEvent,
  ToolExecutedEvent,
  IterationStartedEvent,
  IterationCompletedEvent,
} from "./execution-event-bus.js";

const logger = createContextualLogger({ component: "ExecutionEventLogger" });

/**
 * Execution Event Logger Class
 * Subscribes to execution events and provides detailed logging
 */
export class ExecutionEventLogger {
  private unsubscribers: Array<() => void> = [];
  private isSubscribed = false;

  /**
   * Subscribe to all execution events
   * Starts listening to events from the ExecutionEventBus
   */
  subscribe(): void {
    if (this.isSubscribed) {
      logger.warn("ExecutionEventLogger is already subscribed");
      return;
    }

    const bus = getExecutionEventBus();

    // Subscribe to state changes
    this.unsubscribers.push(
      bus.on("state_changed", (event: ExecutionStateChangedEvent) => {
        logger.info("Execution state changed", {
          executionId: event.executionId,
          from: event.previousStatus,
          to: event.newStatus,
          changes: event.changes,
          timestamp: event.timestamp,
        });
      }),
    );

    // Subscribe to error events
    this.unsubscribers.push(
      bus.on("error_occurred", (event: ErrorOccurredEvent) => {
        logger.error("Execution error occurred", {
          executionId: event.executionId,
          errorMessage: event.error.message,
          errorCode: event.error.code,
          context: event.context,
          timestamp: event.timestamp,
        });
      }),
    );

    // Subscribe to interruption events
    this.unsubscribers.push(
      bus.on("interruption_occurred", (event: InterruptionOccurredEvent) => {
        logger.warn("Execution interrupted", {
          executionId: event.executionId,
          interruption: event.interruption,
          timestamp: event.timestamp,
        });
      }),
    );

    // Subscribe to tool execution events
    this.unsubscribers.push(
      bus.on("tool_executed", (event: ToolExecutedEvent) => {
        if (event.status === "success") {
          logger.debug("Tool executed successfully", {
            executionId: event.executionId,
            toolName: event.toolName,
            status: event.status,
            duration: event.duration,
            timestamp: event.timestamp,
          });
        } else {
          logger.error("Tool execution failed", {
            executionId: event.executionId,
            toolName: event.toolName,
            status: event.status,
            duration: event.duration,
            timestamp: event.timestamp,
          });
        }
      }),
    );

    // Subscribe to iteration lifecycle events
    this.unsubscribers.push(
      bus.on("iteration_started", (event: IterationStartedEvent) => {
        logger.debug("Iteration started", {
          executionId: event.executionId,
          iteration: event.iteration,
          timestamp: event.timestamp,
        });
      }),
    );

    this.unsubscribers.push(
      bus.on("iteration_completed", (event: IterationCompletedEvent) => {
        logger.debug("Iteration completed", {
          executionId: event.executionId,
          iteration: event.iteration,
          result: event.result,
          timestamp: event.timestamp,
        });
      }),
    );

    this.isSubscribed = true;
    logger.info("ExecutionEventLogger subscribed to all events");
  }

  /**
   * Unsubscribe from all events
   * Clean up all event listeners
   */
  unsubscribe(): void {
    if (!this.isSubscribed) {
      return;
    }

    for (const unsub of this.unsubscribers) {
      unsub();
    }

    this.unsubscribers = [];
    this.isSubscribed = false;
    logger.info("ExecutionEventLogger unsubscribed from all events");
  }
}