/**
 * Timeout Management Type Definitions
 * 
 * Core types for the unified timeout management system.
 * These types are internal to the SDK and not exposed as public API.
 */

/**
 * Forward reference to InterruptionState
 * Actual type is defined in ../execution/interruption-state.ts
 * We use a minimal interface here to avoid circular dependencies
 */
export interface InterruptionStateReference {
  /** Check if execution should stop */
  shouldStop(): boolean;
  
  /** Register callback for resume events */
  onResumed(callback: () => void): () => void;
}

/**
 * Timeout status enumeration
 */
export type TimeoutStatus = 'active' | 'expired' | 'cancelled';

/**
 * Timeout registration options
 * 
 * Used to register a new timeout with the TimeoutManager.
 * All timeouts should be registered with a unique ID and duration.
 */
export interface TimeoutRegistration {
  /** 
   * Unique identifier for this timeout
   * Should be unique within the execution context
   */
  id: string;
  
  /** 
   * Timeout duration in milliseconds
   * Must be positive and within configured maximum
   */
  duration: number;
  
  /** 
   * Callback invoked when timeout expires
   * Can be synchronous or asynchronous
   */
  onTimeout: () => void | Promise<void>;
  
  /** 
   * Optional: Warning threshold in milliseconds before timeout
   * If set, onWarning will be called when this threshold is reached
   * Example: duration=300000, warningThreshold=60000 -> warning at 240000ms
   */
  warningThreshold?: number;
  
  /** 
   * Optional: Warning callback invoked when warningThreshold is reached
   */
  onWarning?: () => void | Promise<void>;
  
  /** 
   * Optional: Bind to interruption state for automatic cancellation
   * When the parent execution is interrupted (PAUSE/STOP), this timeout
   * will be automatically cancelled
   */
  interruptionState?: InterruptionStateReference;
  
  /** 
   * Optional: Tag for batch operations
   * Useful for cancelling multiple related timeouts at once
   * Examples: 'llm', 'tool', 'join', 'pause'
   */
  tag?: string;
  
  /** 
   * Optional: Execution context ID
   * Used for grouping timeouts by execution
   */
  executionId?: string;
  
  /** 
   * Optional: Metadata for observability and debugging
   * Can include node IDs, profile IDs, or other contextual information
   */
  metadata?: Record<string, unknown>;
}

/**
 * Timeout handle for managing registered timeouts
 * 
 * Returned when registering a timeout. Provides methods to
 * check status, get remaining time, and cancel the timeout.
 */
export interface TimeoutHandle {
  /** Timeout ID */
  readonly id: string;
  
  /** 
   * Check if timeout is still active
   * @returns true if timeout has not expired or been cancelled
   */
  isActive(): boolean;
  
  /** 
   * Get remaining time in milliseconds
   * @returns Remaining time, or 0 if expired/cancelled
   */
  getRemainingTime(): number;
  
  /** 
   * Cancel this timeout
   * Safe to call multiple times (idempotent)
   */
  cancel(): void;
}

/**
 * Timeout entry internal state
 * 
 * Internal representation of a registered timeout.
 * Not exposed directly to users, but used internally by TimeoutManager.
 */
export interface TimeoutEntry {
  /** Timeout ID */
  id: string;
  
  /** Registration timestamp (milliseconds since epoch) */
  startTime: number;
  
  /** Timeout duration in milliseconds */
  duration: number;
  
  /** Node.js timer ID for the main timeout */
  timerId?: NodeJS.Timeout;
  
  /** Node.js timer ID for the warning timeout (if configured) */
  warningTimerId?: NodeJS.Timeout;
  
  /** Current timeout status */
  status: TimeoutStatus;
  
  /** Whether warning has been emitted */
  warningEmitted: boolean;
  
  /** Unsubscribe function for interruption state listener */
  interruptionUnsubscribe?: () => void;
  
  /** Timeout callback */
  onTimeout: () => void | Promise<void>;
  
  /** Optional warning callback */
  onWarning?: () => void | Promise<void>;
  
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Timeout snapshot for checkpoint support
 * 
 * Serializable representation of timeout state.
 * Used for saving and restoring timeout state during checkpoints.
 */
export interface TimeoutSnapshot {
  /** Snapshot version for compatibility */
  version: number;
  
  /** Snapshot timestamp (milliseconds since epoch) */
  timestamp: number;
  
  /** Array of serialized timeout entries */
  timeouts: Array<{
    /** Timeout ID */
    id: string;
    
    /** Start time */
    startTime: number;
    
    /** Duration in milliseconds */
    duration: number;
    
    /** Current status */
    status: TimeoutStatus;
    
    /** Whether warning was emitted */
    warningEmitted: boolean;
    
    /** Optional metadata */
    metadata?: Record<string, unknown>;
  }>;
}

/**
 * Timeout statistics
 * 
 * Aggregated statistics about timeout usage.
 * Used for monitoring and observability.
 */
export interface TimeoutStats {
  /** Number of currently active timeouts */
  activeTimeouts: number;
  
  /** Total number of timeouts registered (lifetime) */
  totalRegistered: number;
  
  /** Number of timeouts that expired naturally */
  timedOutCount: number;
  
  /** Number of timeouts that were cancelled */
  cancelledCount: number;
  
  /** Average timeout duration in milliseconds */
  averageDuration: number;
  
  /** Count of timeouts by tag */
  byTag: Record<string, number>;
  
  /** Count of timeouts by module/category */
  byModule: Record<string, number>;
}

/**
 * Timeout event types
 * 
 * Events emitted by the timeout system for observability.
 */
export type TimeoutEventType =
  | 'TIMEOUT_REGISTERED'
  | 'TIMEOUT_EXPIRED'
  | 'TIMEOUT_CANCELLED'
  | 'TIMEOUT_WARNING';

/**
 * Base timeout event
 */
export interface TimeoutEvent {
  /** Event type */
  type: TimeoutEventType;
  
  /** Event timestamp (milliseconds since epoch) */
  timestamp: number;
  
  /** Timeout ID */
  timeoutId: string;
  
  /** Optional execution ID */
  executionId?: string;
  
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Timeout registered event
 */
export interface TimeoutRegisteredEvent extends TimeoutEvent {
  type: 'TIMEOUT_REGISTERED';
  duration: number;
  tag?: string;
}

/**
 * Timeout expired event
 */
export interface TimeoutExpiredEvent extends TimeoutEvent {
  type: 'TIMEOUT_EXPIRED';
  duration: number;
  actualDuration: number;
}

/**
 * Timeout cancelled event
 */
export interface TimeoutCancelledEvent extends TimeoutEvent {
  type: 'TIMEOUT_CANCELLED';
  reason?: string;
}

/**
 * Timeout warning event
 */
export interface TimeoutWarningEvent extends TimeoutEvent {
  type: 'TIMEOUT_WARNING';
  duration: number;
  remainingTime: number;
  warningThreshold: number;
}

/**
 * Union type of all timeout events
 */
export type TimeoutEventUnion =
  | TimeoutRegisteredEvent
  | TimeoutExpiredEvent
  | TimeoutCancelledEvent
  | TimeoutWarningEvent;
