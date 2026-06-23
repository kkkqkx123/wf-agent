/**
 * Checkpoint Error Handling Configuration and Types
 */

/**
 * Checkpoint error handling strategy
 */
export type CheckpointErrorStrategy =
  | "silent"      // Silent failure - only debug logs
  | "warn"        // Warning level - logs but doesn't interrupt
  | "strict"      // Strict mode - throws exception and interrupts
  | "callback";   // Callback mode - calls user-provided handler

/**
 * Context information about a checkpoint error
 */
export interface CheckpointErrorContext {
  /** Checkpoint ID if available */
  checkpointId?: string;
  /** Parent entity ID (agentLoopId or executionId) */
  entityId: string;
  /** Trigger event that caused the checkpoint attempt */
  triggerEvent: string;
  /** Operation type (create, restore, validate, cleanup) */
  operation: "create" | "restore" | "validate" | "cleanup";
  /** Timestamp when the error occurred */
  timestamp: number;
}

/**
 * Error handler callback function type
 */
export type CheckpointErrorCallback = (
  error: CheckpointError,
  context: CheckpointErrorContext,
) => void | Promise<void>;

/**
 * Checkpoint Error Handler Configuration
 */
export interface CheckpointErrorHandlerConfig {
  /** Error handling strategy */
  strategy: CheckpointErrorStrategy;
  /** Optional callback for "callback" strategy */
  onError?: CheckpointErrorCallback;
}

/**
 * Checkpoint-specific Error class
 * Extends Error with checkpoint operation context
 */
export class CheckpointError extends Error {
  constructor(
    message: string,
    public readonly operation: "create" | "restore" | "validate" | "cleanup",
    public readonly entityId: string,
    public readonly checkpointId?: string,
    public override readonly cause?: Error,
  ) {
    super(message);
    this.name = "CheckpointError";
    // Set up proper prototype chain for instanceof checks
    Object.setPrototypeOf(this, CheckpointError.prototype);
  }

  /**
   * Convert error to JSON for logging
   */
  toJSON() {
    return {
      name: this.name,
      message: this.message,
      operation: this.operation,
      entityId: this.entityId,
      checkpointId: this.checkpointId,
      cause: this.cause?.message,
      stack: this.stack,
    };
  }
}

/**
 * Result of checkpoint error handling
 */
export interface CheckpointErrorHandlingResult {
  /** Whether the error should be rethrown */
  shouldRethrow: boolean;
  /** Whether the error was successfully handled */
  handled: boolean;
}
