/**
 * WorkflowExecutionState - Workflow Execution State Manager
 *
 * Manages the temporary states during the execution of a workflow execution, separate from persistent data.
 * Refer to the design pattern of AgentLoopState.
 */

import { now } from "@wf-agent/common-utils";
import type { WorkflowExecutionStatus } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
import type { StateManager } from "../../core/types/state-manager.js";

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

  /**
   * Get the current status
   */
  get status(): WorkflowExecutionStatus {
    return this._status;
  }

  /**
   * Set the status
   */
  set status(value: WorkflowExecutionStatus) {
    this._status = value;
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
   * Set the interrupted flag
   */
  set interrupted(value: boolean) {
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
  pause(): void {
    if (this._status !== "RUNNING") {
      throw new RuntimeValidationError(
        `Can only pause RUNNING execution, current status: ${this._status}`,
        { operation: "pause", field: "status", value: this._status },
      );
    }
    this._status = "PAUSED";
    this._shouldPause = false;
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
    this._status = "FAILED";
    this._error = error;
    this._endTime = now();
  }

  /**
   * Cancel execution
   */
  cancel(): void {
    this._status = "CANCELLED";
    this._endTime = now();
  }

  /**
   * Timeout
   */
  timeout(): void {
    this._status = "TIMEOUT";
    this._endTime = now();
  }

  /**
   * Interrupt execution
   * @param type: Type of the interrupt
   */
  interrupt(type: "PAUSE" | "STOP"): void {
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
   * Check for timeouts.
   */
  isTimeout(): boolean {
    return this._status === "TIMEOUT";
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
    return cloned;
  }
}
