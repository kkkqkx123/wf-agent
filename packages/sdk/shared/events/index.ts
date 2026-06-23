/**
 * Execution Events Module
 *
 * Pub/Sub system for execution state changes and events.
 * Enables decoupled communication between core execution logic and cross-cutting concerns
 * like Metrics collection and Logging.
 */

export {
  ExecutionEventBus,
  getExecutionEventBus,
  setExecutionEventBus,
  resetExecutionEventBus,
  type ExecutionStateChangedEvent,
  type ErrorOccurredEvent,
  type InterruptionOccurredEvent,
  type ToolExecutedEvent,
  type IterationStartedEvent,
  type IterationCompletedEvent,
  type ExecutionEvent,
  type EventHandler,
  type AnyEventHandler,
} from "./execution-event-bus.js";
