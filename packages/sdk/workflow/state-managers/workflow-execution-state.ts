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
  EXECUTION_STATE_MAX_INTERRUPTION_RECORDS,
  EXECUTION_STATE_MAX_EVENTS,
} from "@wf-agent/types";
import type { StateManager } from "../../shared/types/state-manager.js";

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

  /** Interruption records accumulated during execution */
  private _interruptionRecords: ExecutionInterruptionRecord[] = [];

  /** Event records accumulated during execution */
  private _eventRecords: ExecutionEventRecord[] = [];

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

    // [P4 Fix] Record interruption for history tracking and checkpoint consistency
    this.recordInterruption({
      id: `interrupt:${now()}:${Math.random().toString(36).slice(2, 9)}`,
      timestamp: now(),
      type: "PAUSE",
      reason: "Workflow execution paused",
      nodeId,
      status: "pending",
    });
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
    for (let i = this._interruptionRecords.length - 1; i >= 0; i--) {
      const record = this._interruptionRecords[i];
      if (record && record.type === "PAUSE" && record.status === "pending") {
        record.status = "resumed";
        record.resumedAt = now();
        break;
      }
    }
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

    // [P4 Fix] Record interruption for history tracking and checkpoint consistency
    this.recordInterruption({
      id: `interrupt:${now()}:${Math.random().toString(36).slice(2, 9)}`,
      timestamp: now(),
      type: "STOP",
      reason: "Workflow execution cancelled",
      nodeId,
      status: "abandoned",
    });
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

  // ========== Error Chain Tracking ==========

  /**
   * Record an error with automatic error chain building
   *
   * Automatically establishes relationships between errors:
   * - Sets parentErrorId to the last error
   * - Builds errorChain array
   * - Identifies root cause
   */
  recordError(error: ExecutionErrorRecord): void {
    // 1. Standardize error ID if not provided
    const errorId = error.id || `error:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`;
    error.id = errorId;

    // 2. Build error chain relationships
    //    Uses explicit parentErrorId if provided (causal link); otherwise falls
    //    back to sequential ordering (link to last recorded error).
    if (this._errorRecords.length > 0) {
      // 2a. Resolve parent: prefer explicit parentErrorId, fall back to last record
      const parentError = error.parentErrorId
        ? this._errorRecords.find(e => e.id === error.parentErrorId)
        : undefined;
      const parent = parentError ?? this._errorRecords[this._errorRecords.length - 1]!;

      // Only set parentErrorId if not already explicitly provided
      if (!error.parentErrorId) {
        error.parentErrorId = parent.id;
      }

      // 2b. Build error chain from parent's chain or start new chain
      if (parent.errorChain) {
        error.errorChain = [...parent.errorChain, errorId];
      } else {
        // First time establishing chain
        error.errorChain = [parent.id, errorId];
      }

      // 2c. Quick reference to root cause
      error.rootCauseId = parent.rootCauseId || parent.id;
    } else {
      // This is the first error, it is the root cause
      error.errorChain = [errorId];
      error.rootCauseId = errorId;
    }

    // 3. Add to records
    this._errorRecords.push(error);
  }

  /**
   * Get all error records
   */
  getErrorRecords(): ExecutionErrorRecord[] {
    return [...this._errorRecords];
  }

  /**
   * Record an interruption
   * @param record Interruption record
   */
  recordInterruption(record: ExecutionInterruptionRecord): void {
    const MAX_INTERRUPTIONS = EXECUTION_STATE_MAX_INTERRUPTION_RECORDS;
    if (this._interruptionRecords.length >= MAX_INTERRUPTIONS) {
      this._interruptionRecords.shift();
    }
    this._interruptionRecords.push(record);
  }

  /**
   * Get all interruption records
   */
  getInterruptionRecords(): ExecutionInterruptionRecord[] {
    return [...this._interruptionRecords];
  }

  /**
   * Record an event
   * @param record Event record
   */
  recordEvent(record: ExecutionEventRecord): void {
    const MAX_EVENTS = EXECUTION_STATE_MAX_EVENTS;
    if (this._eventRecords.length >= MAX_EVENTS) {
      this._eventRecords.shift();
    }
    this._eventRecords.push(record);
  }

  /**
   * Get all event records
   */
  getEventRecords(): ExecutionEventRecord[] {
    return [...this._eventRecords];
  }

  /**
   * Get the complete error chain for a specific error
   *
   * Returns all errors in the chain starting from the root cause
   * up to and including the specified error.
   */
  getErrorChain(fromErrorId?: string): ExecutionErrorRecord[] {
    if (this._errorRecords.length === 0) {
      return [];
    }

    const targetErrorId = fromErrorId || this._errorRecords[this._errorRecords.length - 1]!.id;
    const targetError = this._errorRecords.find(e => e.id === targetErrorId);

    if (!targetError) {
      return [];
    }

    if (!targetError.errorChain) {
      return [targetError];
    }

    return targetError.errorChain
      .map(id => this._errorRecords.find(e => e.id === id))
      .filter((e): e is ExecutionErrorRecord => Boolean(e));
  }

  /**
   * Get the root cause error
   *
   * Returns the first error in the chain that triggered all subsequent errors.
   */
  getRootCauseError(): ExecutionErrorRecord | null {
    if (this._errorRecords.length === 0) {
      return null;
    }

    const lastError = this._errorRecords[this._errorRecords.length - 1];
    if (!lastError) {
      return null;
    }

    const rootCauseId = lastError.rootCauseId || lastError.id;
    return this._errorRecords.find(e => e.id === rootCauseId) || lastError;
  }

  /**
   * Analyze error patterns across all recorded errors.
   * Provides distribution by type, affected nodes, and severity breakdown.
   * [P8 Fix] Ported from AgentLoopState for cross-layer consistency.
   */
  analyzeErrorPattern(): ErrorPattern {
    if (this._errorRecords.length === 0) {
      return {
        type: 'none',
        count: 0,
        errors: [],
        typeDistribution: {},
        nodeProblems: [],
        severityBreakdown: {},
      };
    }

    const errors = this._errorRecords;
    const typeCount: Record<string, number> = {};
    const nodeCount: Record<string, number> = {};
    const severityCount: Record<string, number> = {};

    errors.forEach(err => {
      typeCount[err.errorType] = (typeCount[err.errorType] ?? 0) + 1;
      if (err.nodeId) {
        nodeCount[err.nodeId] = (nodeCount[err.nodeId] ?? 0) + 1;
      }
      severityCount[err.severity] = (severityCount[err.severity] ?? 0) + 1;
    });

    return {
      type: errors.length > 1 ? 'chain' : 'single',
      count: errors.length,
      errors,
      typeDistribution: typeCount,
      nodeProblems: Object.entries(nodeCount)
        .sort(([, a], [, b]) => b - a)
        .slice(0, 5)
        .map(([nodeId, count]) => ({ nodeId, count })),
      severityBreakdown: severityCount,
    };
  }

  /**
   * Get recommended recovery action based on error chain analysis.
   * [P8 Fix] Ported from AgentLoopState for cross-layer consistency.
   */
  getRecommendedRecoveryAction(): "retry" | "fallback" | "manual_intervention" | "abort" {
    if (this._errorRecords.length === 0) return "retry";

    const lastError = this._errorRecords[this._errorRecords.length - 1];
    if (!lastError) return "retry";

    // If the last error is recoverable, suggest retry
    if (lastError.isRecoverable) return "retry";

    // If recovery action was specified in the error, use it
    if (lastError.recoveryAction) return lastError.recoveryAction as "retry" | "fallback" | "manual_intervention" | "abort";

    // Check error chain patterns
    if (this._errorRecords.length >= 3) {
      // Multiple errors in chain suggests deeper issues
      return "manual_intervention";
    }

    // Check for repeated tool errors
    const toolErrors = this._errorRecords.filter(
      e => e.errorType === "tool_error" || e.context?.toolName,
    );
    if (toolErrors.length >= 2) {
      return "fallback";
    }

    return "abort";
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
  }

  /**
   * Create a snapshot of the workflow execution state
   * @returns Snapshot containing all state fields
   */
  createSnapshot(): WorkflowExecutionStateSnapshot {
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
      interruptionRecords: [...this._interruptionRecords],
      eventRecords: [...this._eventRecords],
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
    this._interruptionRecords = [...(snapshot.interruptionRecords ?? [])];
    this._eventRecords = [...(snapshot.eventRecords ?? [])];
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
    this._interruptionRecords = [];
    this._eventRecords = [];
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
    cloned._interruptionRecords = [...this._interruptionRecords];
    cloned._eventRecords = [...this._eventRecords];
    return cloned;
  }
}
