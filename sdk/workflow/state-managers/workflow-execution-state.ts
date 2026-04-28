/**
 * WorkflowExecutionState - Workflow Execution State Manager
 *
 * Manages the temporary states during the execution of a workflow execution, separate from persistent data.
 * Refer to the design pattern of AgentLoopState.
 */

import { now } from "@wf-agent/common-utils";
import type { WorkflowExecutionStatus } from "@wf-agent/types";

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
export class WorkflowExecutionState {
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
    this._status = "PAUSED";
    this._shouldPause = false;
  }

  /**
   * Resume execution
   */
  resume(): void {
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
   * Clean up resources
   */
  cleanup(): void {
    this._error = null;
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
    return cloned;
  }
}
