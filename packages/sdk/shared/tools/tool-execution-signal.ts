/**
 * Tool Execution Signal
 *
 * Provides a signal mechanism for controlling tool execution, including
 * cancellation and timeout handling.
 *
 * @remarks
 * This module provides a signal-based approach to managing tool execution
 * lifecycle, allowing for clean cancellation and timeout behavior without
 * throwing exceptions or using complex promise chains.
 */

import { now } from "@wf-agent/common-utils";

/**
 * Possible states of tool execution signal
 */
export type ToolExecutionState = "pending" | "running" | "completed" | "failed" | "cancelled";

/**
 * Parameters for creating a tool execution signal
 */
export interface ToolExecutionSignalParams {
  toolId: string;
  toolName: string;
  executionId: string;
  state?: ToolExecutionState;
  error?: Error;
  cancelledAt?: number;
  cancelledBy?: string;
  timeout?: number;
}

/**
 * Tool Execution Signal
 *
 * Represents the state of a tool execution, providing a unified way to
 * track and communicate the execution status across different components.
 */
export class ToolExecutionSignal {
  /** Unique identifier for the tool */
  readonly toolId: string;

  /** Human-readable name of the tool */
  readonly toolName: string;

  /** Unique identifier for this execution */
  readonly executionId: string;

  /** Current state of execution */
  state: ToolExecutionState;

  /** Error information if the execution failed */
  error?: Error;

  /** Timestamp when the execution was cancelled */
  cancelledAt?: number;

  /** Identifier of who/what cancelled the execution */
  cancelledBy?: string;

  /** Optional timeout in milliseconds */
  timeout?: number;

  /** Timestamp when the execution was started */
  readonly startedAt: number;

  constructor(params: ToolExecutionSignalParams) {
    this.toolId = params.toolId;
    this.toolName = params.toolName;
    this.executionId = params.executionId;
    this.state = params.state || "pending";
    this.error = params.error;
    this.cancelledAt = params.cancelledAt;
    this.cancelledBy = params.cancelledBy;
    this.timeout = params.timeout;
    this.startedAt = now();
  }

  /**
   * Mark the execution as completed
   */
  complete(): void {
    this.state = "completed";
  }

  /**
   * Mark the execution as failed with an error
   *
   * @param error - The error that caused the failure
   */
  fail(error: Error): void {
    this.state = "failed";
    this.error = error;
  }

  /**
   * Cancel the execution
   *
   * @param cancelledBy - Optional identifier of who cancelled the execution
   */
  cancel(cancelledBy?: string): void {
    this.state = "cancelled";
    this.cancelledAt = now();
    this.cancelledBy = cancelledBy;
  }

  /**
   * Check if the execution has been cancelled
   */
  get isCancelled(): boolean {
    return this.state === "cancelled";
  }

  /**
   * Check if the execution has completed
   */
  get isCompleted(): boolean {
    return this.state === "completed";
  }

  /**
   * Check if the execution has failed
   */
  get isFailed(): boolean {
    return this.state === "failed";
  }

  /**
   * Check if the execution is still pending
   */
  get isPending(): boolean {
    return this.state === "pending";
  }

  /**
   * Check if the execution is currently running
   */
  get isRunning(): boolean {
    return this.state === "running";
  }

  /**
   * Check if the execution has timed out
   */
  get isTimedOut(): boolean {
    if (!this.timeout) return false;
    return now() - this.startedAt > this.timeout;
  }
}

/**
 * Create a tool execution signal
 */
export function createToolExecutionSignal(params: ToolExecutionSignalParams): ToolExecutionSignal {
  return new ToolExecutionSignal(params);
}

/**
 * Check if a tool execution has timed out
 */
export function isToolExecutionTimeout(signal: ToolExecutionSignal): boolean {
  return signal.isTimedOut;
}

/**
 * Check if a tool execution was externally aborted
 */
export function isToolExecutionExternalAbort(signal: ToolExecutionSignal): boolean {
  return signal.isCancelled;
}

/**
 * Get the timeout duration for a tool execution signal
 */
export function getToolExecutionTimeoutMs(signal: ToolExecutionSignal): number | undefined {
  return signal.timeout;
}