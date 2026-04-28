/**
 * Output Target Types
 *
 * Defines output destinations for component messages.
 */

/**
 * Output Target
 * Specifies where a message should be delivered.
 */
export enum OutputTarget {
  /** Terminal User Interface display */
  TUI = "tui",

  /** Functional file (for program-to-program exchange, e.g., human-relay files) */
  FILE_FUNCTIONAL = "file_functional",

  /** Display file (for human reading, e.g., output.md) */
  FILE_DISPLAY = "file_display",

  /** Internal event bus (for backward compatibility with Event system) */
  EVENT_BUS = "event_bus",

  /** No output (suppress message) */
  NONE = "none",
}

/**
 * Aggregation Level
 * Controls how messages are aggregated when displayed in parent entity views.
 */
export type AggregationLevel = "none" | "summary" | "detail";

/**
 * Output Decision
 * Determines how a message should be routed and displayed.
 */
export interface OutputDecision {
  /** Target output list (message is delivered to all targets) */
  targets: OutputTarget[];

  /** Whether to aggregate this message in parent entity's view */
  aggregateToParent: boolean;

  /** Aggregation level (controls detail level when aggregated) */
  aggregateLevel: AggregationLevel;

  /** Whether to notify parent entity of this message */
  notifyParent: boolean;

  /** Debounce delay in milliseconds (for batching) */
  debounceMs?: number;
}

/**
 * Output Handler Interface
 * Implemented by application-specific handlers to process messages.
 */
export interface OutputHandler {
  /** Target this handler processes */
  readonly target: OutputTarget;

  /** Handler name (for registration and debugging) */
  readonly name: string;

  /**
   * Check if this handler supports the given message
   * @param message The message to check
   * @returns true if this handler can process the message
   */
  supports(message: BaseComponentMessage): boolean;

  /**
   * Handle the message
   * @param message The message to process
   */
  handle(message: BaseComponentMessage): Promise<void> | void;

  /**
   * Flush any buffered output
   */
  flush?(): Promise<void>;

  /**
   * Close the handler and release resources
   */
  close?(): Promise<void>;
}

// Import BaseComponentMessage for OutputHandler interface
import type { BaseComponentMessage } from "./base.js";
