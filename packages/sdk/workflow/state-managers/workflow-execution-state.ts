/**
 * WorkflowExecutionState - Workflow Execution State Manager
 *
 * Manages the temporary states during the execution of a workflow execution, separate from persistent data.
 * Refer to the design pattern of AgentLoopState.
 */

import { now } from "@wf-agent/common-utils";
import type {
  WorkflowExecutionStatus,
  ExecutionErrorRecord,
  ExecutionInterruptionRecord,
  ExecutionEventRecord,
} from "@wf-agent/types";
import {
  RuntimeValidationError,
} from "@wf-agent/types";
import type { StateManager } from "../../shared/types/state-manager.js";
import { ErrorChainManager } from "../../shared/errors/error-chain-manager.js";
import { ExecutionRecordManager } from "../../shared/records/execution-record-manager.js";

// ============================================================================
// Error Chain Analysis Types
// ============================================================================

/**
 * Error pattern analysis result
 */
export interface ErrorPattern {
  /** Pattern type: none, single, or chain */
  type: 'none' | 'single' | 'chain';
  /** Total error count */
  count: number;
  /** All error records */
  errors: ExecutionErrorRecord[];
  /** Distribution of errors by type */
  typeDistribution: Record<string, number>;
  /** Nodes causing the most errors */
  nodeProblems: Array<{ nodeId: string; count: number }>;
  /** Distribution by severity */
  severityBreakdown: Record<string, number>;
}

/**
 * Operation-level execution state
 * Tracks progress within individual node operations
 */
export interface OperationState {
  /** Type of operation */
  type: "LLM_STREAMING" | "TOOL_EXECUTION" | "SCRIPT_EXECUTION";

  /** Operation ID (e.g., toolCallId, requestId) */
  operationId: string;

  /** Node ID where operation is running */
  nodeId: string;

  /** Start timestamp */
  startedAt: number;

  /** Progress information (operation-specific) */
  progress?: {
    /** For LLM: tokens generated so far */
    tokensGenerated?: number;
    /** For tools: items processed / total items */
    itemsProcessed?: number;
    totalItems?: number;
    /** Generic progress percentage (0-100) */
    percentage?: number;
  };

  /** Partial result accumulated so far */
  partialResult?: unknown;

  /** Operation-specific metadata */
  metadata?: Record<string, unknown>;
}

/**
 * WorkflowExecutionState snapshot for checkpoint support
 */
export interface WorkflowExecutionStateSnapshot {
  status: WorkflowExecutionStatus;
  shouldPause: boolean;
  shouldStop: boolean;
  startTime: number | null;
  endTime: number | null;
  error: unknown;
  interrupted: boolean;
  currentOperation: OperationState | null;
  errorRecords: ExecutionErrorRecord[];
  interruptionRecords: ExecutionInterruptionRecord[];
  eventRecords: ExecutionEventRecord[];
}

/**
 * WorkflowExecutionState - Workflow Execution State Manager
 *
 * Core Responsibilities:
 * - Manages the transition of execution states
 * - Controls interrupt flags
 *
 * Design Principles:
 * - Separated from persistent data
 * - Bound to the lifecycle and execution cycle
 * - Pure state management, without including business logic
 */
export class WorkflowExecutionState implements StateManager<WorkflowExecutionStateSnapshot> {
  /** Current Status */
  private _status: WorkflowExecutionStatus = "CREATED";

  /** Pause flag */
  private _shouldPause: boolean = false;

  /** Stop sign */
  private _shouldStop: boolean = false;

  /** Start time */
  private _startTime: number | null = null;

  /** End time */
  private _endTime: number | null = null;

  /** Error message */
  private _error: unknown = null;

  /** Interrupted flag */
  private _interrupted: boolean = false;

  /** Current operation state (if any) */
  private _currentOperation: OperationState | null = null;

  /** Error records accumulated during execution */
  private _errorRecords: ExecutionErrorRecord[] = [];

  /** Execution record manager (interruptions, events) */
  private readonly _recordManager: ExecutionRecordManager = new ExecutionRecordManager();

  /**
   * Get the current status
   */
  get status(): WorkflowExecutionStatus {
    return this._status;
  }

  /**
   * Get the start time
   */
  get startTime(): number | null {
    return this._startTime;
  }

  /**
   * Get the end time
   */
  get endTime(): number | null {
    return this._endTime;
  }

  /**
   * Get the error message
   */
  get error(): unknown {
    return this._error;
  }

  /**
   * Get the interrupted flag
   */
  get interrupted(): boolean {
    return this._interrupted;
  }

  /**
   * Set the interrupted flag (private - use interrupt()/resetInterrupt() methods)
   */
  private set interrupted(value: boolean) {
    this._interrupted = value;
  }

  /**
   * Check if it should be paused.
   */
  shouldPause(): boolean {
    return this._shouldPause;
  }

  /**
   * Set the pause flag
   */
  setShouldPause(value: boolean): void {
    this._shouldPause = value;
  }

  /**
   * Check whether it should be stopped.
   */
  shouldStop(): boolean {
    return this._shouldStop;
  }

  /**
   * Set the stop flag
   */
  setShouldStop(value: boolean): void {
    this._shouldStop = value;
  }

  /**
   * Start execution
   */
  start(): void {
    this._status = "RUNNING";
    this._startTime = now();
  }

  /**
   * Pause execution
   */
  pause(nodeId?: string): void {
    if (this._status !== "RUNNING") {
      throw new RuntimeValidationError(
        `Can only pause RUNNING execution, current status: ${this._status}`,
        { operation: "pause", field: "status", value: this._status },
      );
    }
    this._status = "PAUSED";
    this._shouldPause = false;

    // Record pause interruption with enriched context
    this.recordPauseInterruption("Workflow execution paused", undefined, "system", nodeId);
  }

  /**
   * Resume execution
   */
  resume(): void {
    if (this._status !== "PAUSED") {
      throw new RuntimeValidationError(
        `Can only resume PAUSED execution, current status: ${this._status}`,
        { operation: "resume", field: "status", value: this._status },
      );
    }
    this._status = "RUNNING";
    this._shouldPause = false;

    // Update the last PAUSE interruption record to mark it as resumed
    this.recordResumeInterruption();
  }

  /**
   * Complete the execution.
   */
  complete(): void {
    this._status = "COMPLETED";
    this._endTime = now();
  }

  /**
   * Execution failed.
   * @param error Error message
   */
  fail(error: unknown): void {
    // Guard: prevent duplicate recording when already in FAILED state.
    // This can happen when node-level error recording + coordinator-level
    // fail() + lifecycle-level failWorkflowExecution all fire for the same
    // failure event. The first fail() call is authoritative.
    if (this._status === "FAILED") {
      return;
    }

    this._status = "FAILED";
    this._error = error;
    this._endTime = now();

    // [Problem #2 Fix] Automatically record error in errorRecords for chain tracking
    this.recordError({
      id: `error:${this._endTime}:${Math.random().toString(36).slice(2, 9)}`,
      timestamp: this._endTime,
      message: error instanceof Error ? error.message : String(error),
      errorType: "execution_error",
      severity: "error",
      context: { operation: "workflow_execution_fail" },
      isRecoverable: false,
    });
  }

  /**
   * Cancel execution
   */
  cancel(nodeId?: string): void {
    this._status = "CANCELLED";
    this._endTime = now();

    // Record stop interruption with enriched context
    this.recordStopInterruption("Workflow execution cancelled", undefined, "system", nodeId);
  }

  /**
   * Interrupt execution
   * @param type: Type of the interrupt
   */
  interrupt(type: "PAUSE" | "STOP"): void {
    this._interrupted = true;
    if (type === "PAUSE") {
      this._shouldPause = true;
    } else {
      this._shouldStop = true;
    }
  }

  /**
   * Reset the interrupt flag.
   */
  resetInterrupt(): void {
    this._interrupted = false;
    this._shouldPause = false;
    this._shouldStop = false;
  }

  /**
   * Check if it is running.
   */
  isRunning(): boolean {
    return this._status === "RUNNING";
  }

  /**
   * Check if it has been paused.
   */
  isPaused(): boolean {
    return this._status === "PAUSED";
  }

  /**
   * Check if it is completed.
   */
  isCompleted(): boolean {
    return this._status === "COMPLETED";
  }

  /**
   * Check if it failed.
   */
  isFailed(): boolean {
    return this._status === "FAILED";
  }

  /**
   * Check if it has been canceled.
   */
  isCancelled(): boolean {
    return this._status === "CANCELLED";
  }

  /**
   * Get current operation state
   */
  getCurrentOperation(): OperationState | null {
    return this._currentOperation;
  }

  /**
   * Set current operation state
   */
  setCurrentOperation(operation: OperationState | null): void {
    this._currentOperation = operation;
  }

  /**
   * Update operation progress
   */
  updateOperationProgress(progress: Partial<OperationState["progress"]>): void {
    if (this._currentOperation) {
      this._currentOperation.progress = {
        ...this._currentOperation.progress,
        ...progress,
      };
    }
  }

  /**
   * Update partial result
   */
  updatePartialResult(result: unknown): void {
    if (this._currentOperation) {
      this._currentOperation.partialResult = result;
    }
  }

  /**
   * Clear operation state (when operation completes)
   */
  clearOperation(): void {
    this._currentOperation = null;
  }

  // ========== Execution Record Management ==========

  /**
   * Record an error with automatic error chain building
   *
   * Automatically establishes relationships between errors:
   * - Sets parentErrorId to the last error
   * - Builds errorChain array
   * - Identifies root cause
   */
  recordError(error: ExecutionErrorRecord): void {
    ErrorChainManager.recordError(this._errorRecords, error);
  }

  /**
   * Get all error records
   */
  getErrorRecords(): ExecutionErrorRecord[] {
    return [...this._errorRecords];
  }

  /**
   * Record an interruption (basic method)
   * Delegates to the shared ExecutionRecordManager.
   * @param record Interruption record
   */
  recordInterruption(record: ExecutionInterruptionRecord): void {
    this._recordManager.addInterruptionRecord(record);
  }

  /**
   * Record a pause interruption with enriched context
   * Captures the execution context at the time of pause for better debugging and recovery.
   *
   * @param reason Reason for pausing
   * @param userId Optional user ID if user-initiated
   * @param source Source of the pause (user, system, timeout, error)
   * @param nodeId Optional node ID where the pause occurred
   */
  recordPauseInterruption(
    reason: string,
    userId?: string,
    source: 'user' | 'system' | 'timeout' | 'error' = 'system',
    nodeId?: string,
  ): void {
    const record: ExecutionInterruptionRecord = {
      id: `pause:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`,
      timestamp: now(),
      type: 'PAUSE',
      reason,
      nodeId,

      // Trigger information
      triggeredBy: {
        source,
        userId,
        reason,
      },

      // Execution context snapshot
      executionContext: {
        iteration: 0,
        status: this._status,
        lastSuccessfulToolCall: this._currentOperation?.operationId,
      },

      // Initial pause status
      status: 'pending',
    };

    this._recordManager.addInterruptionRecord(record);
  }

  /**
   * Record a stop interruption with enriched context
   * Captures the execution context at the time of stop for recovery purposes.
   *
   * @param reason Reason for stopping
   * @param userId Optional user ID if user-initiated
   * @param source Source of the stop (user, system, timeout, error)
   * @param nodeId Optional node ID where the stop occurred
   */
  recordStopInterruption(
    reason: string,
    userId?: string,
    source: 'user' | 'system' | 'timeout' | 'error' = 'system',
    nodeId?: string,
  ): void {
    const record: ExecutionInterruptionRecord = {
      id: `stop:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`,
      timestamp: now(),
      type: 'STOP',
      reason,
      nodeId,

      // Trigger information
      triggeredBy: {
        source,
        userId,
        reason,
      },

      // Execution context snapshot
      executionContext: {
        iteration: 0,
        status: this._status,
        lastSuccessfulToolCall: this._currentOperation?.operationId,
      },

      // Stop is typically final
      status: 'abandoned',
    };

    this._recordManager.addInterruptionRecord(record);
  }

  /**
   * Record resumption of a paused execution
   * Updates the latest pause record with resume information.
   *
   * @param reason Optional reason for resuming
   * @param userId Optional user ID if user-initiated
   * @param source Source of the resume (user, system, automatic)
   * @param checkpointId Optional checkpoint ID used for resuming
   */
  recordResumeInterruption(
    reason?: string,
    userId?: string,
    source: 'user' | 'system' | 'automatic' = 'system',
    checkpointId?: string,
  ): void {
    // Find the latest PAUSE interruption record
    const pauseRecord = this._recordManager.getInterruptionRecords()
      .reverse()
      .find(r => r.type === 'PAUSE' && r.status === 'pending');

    // If found, enrich it with resume information
    if (pauseRecord) {
      pauseRecord.resumedAt = now();
      pauseRecord.resumedReason = reason;
      pauseRecord.resumedBy = {
        source,
        userId,
      };
      pauseRecord.status = 'resumed';
      pauseRecord.resumedFromCheckpointId = checkpointId;
    }
  }

  /**
   * Get all interruption records
   */
  getInterruptionRecords(): ExecutionInterruptionRecord[] {
    return this._recordManager.getInterruptionRecords();
  }

  /**
   * Get interruption history with optional filtering
   * @param filter Optional filter: 'PAUSE' | 'STOP'
   * @returns Filtered interruption records
   */
  getInterruptionHistory(filter?: 'PAUSE' | 'STOP'): ExecutionInterruptionRecord[] {
    return this._recordManager.getInterruptionHistory(filter);
  }

  /**
   * Get interruption statistics
   * @returns Statistics about interruptions: frequency, duration, recovery rate
   */
  getInterruptionStatistics(): {
    total: number;
    byType: Record<string, number>;
    averageDuration?: number;
    recoveryAttempts: number;
    successfulRecoveries: number;
    recoveryRate: number;
  } {
    return this._recordManager.getInterruptionStatistics();
  }

  /**
   * Record an event
   * @param record Event record
   */
  recordEvent(record: ExecutionEventRecord): void {
    this._recordManager.addEventRecord(record);
  }

  /**
   * Get all event records
   */
  getEventRecords(): ExecutionEventRecord[] {
    return this._recordManager.getEventRecords();
  }

  /**
   * Get the complete error chain for a specific error
   *
   * Returns all errors in the chain starting from the root cause
   * up to and including the specified error.
   */
  getErrorChain(fromErrorId?: string): ExecutionErrorRecord[] {
    return ErrorChainManager.getErrorChain(this._errorRecords, fromErrorId);
  }

  /**
   * Get the root cause error
   *
   * Returns the first error in the chain that triggered all subsequent errors.
   */
  getRootCauseError(): ExecutionErrorRecord | null {
    return ErrorChainManager.getRootCauseError(this._errorRecords);
  }

  /**
   * Analyze error patterns across all recorded errors.
   * Provides distribution by type, affected nodes, and severity breakdown.
   * [P8 Fix] Ported from AgentLoopState for cross-layer consistency.
   */
  analyzeErrorPattern(): ErrorPattern {
    return ErrorChainManager.analyzeErrorPattern(this._errorRecords, {
      itemKey: "nodeProblems",
      getItemKey: (err) => err.nodeId,
      buildItem: (key, count) => ({ nodeId: key, count }),
    }) as ErrorPattern;
  }

  /**
   * Get recommended recovery action based on error chain analysis.
   * [P8 Fix] Ported from AgentLoopState for cross-layer consistency.
   */
  getRecommendedRecoveryAction(): "retry" | "fallback" | "manual_intervention" | "abort" {
    return ErrorChainManager.getRecommendedRecoveryActionWorkflow(this._errorRecords);
  }

  /**
   * Serialize operation state for checkpoint
   */
  getOperationStateSnapshot(): OperationState | null {
    return this._currentOperation ? { ...this._currentOperation } : null;
  }

  /**
   * Restore operation state from checkpoint
   */
  restoreOperationState(snapshot: OperationState | null): void {
    this._currentOperation = snapshot ? { ...snapshot } : null;
  }

  /**
   * Clean up resources
   */
  cleanup(): void {
    this._error = null;
    this._recordManager.cleanup();
  }

  /**
   * Create a snapshot of the workflow execution state
   * @returns Snapshot containing all state fields
   */
  createSnapshot(): WorkflowExecutionStateSnapshot {
    const recordSnapshot = this._recordManager.createSnapshot();
    return {
      status: this._status,
      shouldPause: this._shouldPause,
      shouldStop: this._shouldStop,
      startTime: this._startTime,
      endTime: this._endTime,
      error: this._error,
      interrupted: this._interrupted,
      currentOperation: this._currentOperation ? { ...this._currentOperation } : null,
      errorRecords: [...this._errorRecords],
      interruptionRecords: recordSnapshot.interruptionRecords,
      eventRecords: recordSnapshot.eventRecords,
    };
  }

  /**
   * Restore workflow execution state from snapshot
   * @param snapshot The state snapshot
   */
  restoreFromSnapshot(snapshot: WorkflowExecutionStateSnapshot): void {
    this._status = snapshot.status;
    this._shouldPause = snapshot.shouldPause;
    this._shouldStop = snapshot.shouldStop;
    this._startTime = snapshot.startTime;
    this._endTime = snapshot.endTime;
    this._error = snapshot.error;
    this._interrupted = snapshot.interrupted;
    this._currentOperation = snapshot.currentOperation ? { ...snapshot.currentOperation } : null;
    this._errorRecords = [...(snapshot.errorRecords ?? [])];
    this._recordManager.restoreFromSnapshot({
      interruptionRecords: snapshot.interruptionRecords ?? [],
      eventRecords: snapshot.eventRecords ?? [],
    });
  }

  /**
   * Get the size (always 1 as this is a single state object)
   * @returns 1
   */
  size(): number {
    return 1;
  }

  /**
   * Check if the state is empty (always false as state always exists)
   * @returns false
   */
  isEmpty(): boolean {
    return false;
  }

  /**
   * Reset to initial CREATED state
   */
  reset(): void {
    this._status = "CREATED";
    this._shouldPause = false;
    this._shouldStop = false;
    this._startTime = null;
    this._endTime = null;
    this._error = null;
    this._interrupted = false;
    this._currentOperation = null;
    this._errorRecords = [];
    this._recordManager.reset();
  }

  /**
   * Clone Status
   */
  clone(): WorkflowExecutionState {
    const cloned = new WorkflowExecutionState();
    cloned._status = this._status;
    cloned._shouldPause = this._shouldPause;
    cloned._shouldStop = this._shouldStop;
    cloned._startTime = this._startTime;
    cloned._endTime = this._endTime;
    cloned._error = this._error;
    cloned._interrupted = this._interrupted;
    cloned._currentOperation = this._currentOperation ? { ...this._currentOperation } : null;
    cloned._errorRecords = [...this._errorRecords];
    cloned._recordManager.restoreFromSnapshot(this._recordManager.createSnapshot());
    return cloned;
  }
}
