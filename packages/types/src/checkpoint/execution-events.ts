/**
 * Execution Event Types
 *
 * Defines types for errors, interruptions, and events that occur during execution.
 * These data structures are now part of the CheckpointState, ensuring they are
 * persisted atomically with the execution state.
 *
 * NOTE: These are data records stored in checkpoint state, distinct from the
 * ExecutionError class in the errors module (which is a JavaScript Error class).
 *
 * ## Design Principles
 *
 * 1. **State Ownership**: Error records, interruptions, and critical events are owned
 *    by the execution state. They are stored in the checkpoint and restored when
 *    resuming from a checkpoint.
 *
 * 2. **Persistence Guarantee**: All execution events are persisted together with
 *    the execution state, ensuring consistency and preventing data loss on crashes.
 *
 * 3. **Immutability**: Records are immutable - new records are added but existing
 *    ones should not be modified. Status updates (like resuming an interruption)
 *    are done through the interruption's status field.
 *
 * 4. **Event Driven**: State changes trigger events that are published to the
 *    ExecutionEventBus. Subscribers (Metrics, Logger, etc.) consume these events.
 *    Events themselves are derived from state changes, not the source of truth.
 */

import type { Timestamp } from "../common.js";

/**
 * Execution Error Record
 *
 * Represents an error that occurred during execution.
 * Stored in CheckpointState.errorRecords array.
 *
 * NOTE: This is distinct from the ExecutionError class in errors/base.ts,
 * which is a JavaScript Error class. This is a data record.
 */
export interface ExecutionErrorRecord {
  /** Error unique identifier */
  id: string;

  /** Timestamp when error occurred */
  timestamp: Timestamp;

  /** Error message */
  message: string;

  /** Error code (optional, for machine-readable identification) */
  code?: string;

  /** Error type: tool_error, validation_error, execution_error, timeout, etc. */
  errorType: "tool_error" | "validation_error" | "execution_error" | "timeout" | "other";

  /** Error severity */
  severity: "error" | "warning" | "info";

  /** Iteration number when error occurred (for agent loops) */
  iteration?: number;

  /** Node ID where error occurred (for workflows) */
  nodeId?: string;

  /** Operation context */
  context: {
    /** Operation type: 'node_execution', 'tool_call', 'llm_request', etc. */
    operation: string;

    /** Tool name (if error is from tool execution) */
    toolName?: string;

    /** Input that caused the error */
    input?: Record<string, unknown>;
  };

  /** Whether this error can be recovered from */
  isRecoverable: boolean;

  /** Suggested recovery action (if recoverable) */
  recoveryAction?: "retry" | "fallback" | "skip" | "abort";

  /** Additional error details */
  details?: Record<string, unknown>;

  // ============ Error Chain Tracking ============

  /** ID of the error that triggered this error (if part of a chain) */
  parentErrorId?: string;

  /** Complete error chain: [root_error_id, ..., this_error_id] */
  errorChain?: string[];

  /** Quick reference to the root cause error ID */
  rootCauseId?: string;

  /** Relationship between this error and the parent error */
  causedBy?: {
    /** Why this error was triggered by the parent error */
    reason: string;
    /** What handling was attempted */
    handlingAttempt?: string;
  };

  // Note: stackTrace is intentionally NOT included here.
  // Stack traces and detailed debugging info belong in logs, not in state.
}

/**
 * Execution Interruption Record
 *
 * Represents a pause or stop during execution.
 * Stored in CheckpointState.interruptionRecords array.
 */
export interface ExecutionInterruptionRecord {
  /** Interruption unique identifier */
  id: string;

  /** Timestamp when interruption occurred */
  timestamp: Timestamp;

  /** Interruption type: pause or stop */
  type: "PAUSE" | "STOP";

  /** Reason for interruption */
  reason: string;

  /** Iteration number when interruption occurred (for agent loops) */
  iteration?: number;

  /** Node ID when interruption occurred (for workflows) */
  nodeId?: string;

  /** Who/what triggered the pause */
  triggeredBy?: {
    /** Trigger source */
    source: "user" | "system" | "timeout" | "error";
    /** User ID if user-initiated */
    userId?: string;
    /** Detailed reason */
    reason: string;
  };

  /** Execution context when paused */
  executionContext?: {
    /** Current iteration */
    iteration: number;
    /** Execution status */
    status: string;
    /** Last successful tool call ID */
    lastSuccessfulToolCall?: string;
  };

  /** Current status of the interruption */
  status: "pending" | "resumed" | "abandoned";

  /** Checkpoint ID created when this interruption was recorded */
  checkpointId?: string;

  /** Timestamp when interruption was resumed */
  resumedAt?: Timestamp;

  /** Reason for resuming */
  resumedReason?: string;

  /** Who/what resumed the execution */
  resumedBy?: {
    /** Resume source */
    source: "user" | "system" | "automatic";
    /** User ID if user-initiated */
    userId?: string;
  };

  /** Checkpoint ID used when resuming from this interruption */
  resumedFromCheckpointId?: string;

  /** Additional context */
  context?: Record<string, unknown>;
}

/**
 * Execution Event Record
 *
 * Represents a significant event during execution.
 * Events are derived from state changes and published to subscribers.
 * They drive metrics collection and logging but are NOT the source of truth.
 *
 * Stored in CheckpointState.eventRecords array (limited to recent events).
 */
export interface ExecutionEventRecord {
  /** Event unique identifier */
  id: string;

  /** Event timestamp */
  timestamp: Timestamp;

  /** Event type */
  type:
    | "state_changed"
    | "error_occurred"
    | "interruption_occurred"
    | "tool_executed"
    | "iteration_started"
    | "iteration_completed";

  /** Iteration number (if applicable) */
  iteration?: number;

  /** Node ID (if applicable) */
  nodeId?: string;

  /** Event message/description */
  message: string;

  /** Event severity/priority */
  severity?: "critical" | "high" | "medium" | "low" | "info";

  /** Event-specific data */
  data?: Record<string, unknown>;
}

/**
 * Constraint on number of events stored in state
 *
 * To prevent the state from growing unbounded, we keep only recent events.
 * Older events are available through Metrics and Logs.
 */
export const EXECUTION_STATE_MAX_EVENTS = 100;
export const EXECUTION_STATE_MAX_ERROR_RECORDS = 100;
export const EXECUTION_STATE_MAX_INTERRUPTION_RECORDS = 50;
