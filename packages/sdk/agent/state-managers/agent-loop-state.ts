/**
 * AgentLoopState - Agent Loop Execution State Manager
 *
 * Manages the temporary state of an Agent Loop during execution, separate from persistent data.
 * This is the ONLY part of AgentLoopEntity that gets serialized to checkpoints.
 *
 * ## Architecture Role
 *
 * In the Agent Loop architecture:
 * - `AgentLoopRuntimeConfig`: Immutable configuration (NOT serialized, contains functions)
 * - `AgentLoopState`: Mutable execution state (✅ SERIALIZED to checkpoints)
 * - `AgentLoopEntity`: Wrapper that holds config + state + runtime managers
 *
 * ## Serialization Strategy
 *
 * When creating a checkpoint:
 * 1. ✅ Serialize: `AgentLoopState` via `createSnapshot()` / `restoreFromSnapshot()`
 * 2. ❌ Skip: `AgentLoopRuntimeConfig` (contains unserializable functions)
 * 3. ❌ Skip: Runtime managers (`ConversationSession`, `VariableState`)
 *
 * When restoring from checkpoint:
 * 1. Application provides `AgentLoopRuntimeConfig` (re-inject callbacks)
 * 2. Restore `AgentLoopState` from snapshot
 * 3. Rebuild `AgentLoopEntity` with config + restored state
 * 4. Recreate runtime managers (conversation, variables)
 *
 * ## State Categories
 *
 * ### Persistent State (Serialized)
 * - `_status`: Current execution status
 * - `_currentIteration`: Iteration counter
 * - `_toolCallCount`: Total tool calls made
 * - `_iterationHistory`: Complete iteration log
 * - `_startTime`, `_endTime`: Execution timestamps
 * - `_error`: Last error if failed
 *
 * ### Transient State (Not Serialized)
 * - `_streamMessage`: Partial streaming message (lost on pause/resume)
 * - `_pendingToolCalls`: In-flight tool calls (must complete before checkpoint)
 * - `_shouldPause`, `_shouldStop`: Control flags (reset on restore)
 *
 * ## Design Principles
 *
 * 1. **Separation of Concerns**: State management isolated from business logic
 * 2. **Serializable by Default**: All persistent fields are plain data (no functions)
 * 3. **Lifecycle Bound**: Tied to execution cycle, recreated on restore
 * 4. **Pure State Manager**: No side effects, no async operations
 *
 * @see AgentLoopEntity - Execution instance that owns this state
 * @see AgentLoopRuntimeConfig - Configuration (not serialized)
 * @see AgentLoopSnapshotManager - Checkpoint serialization logic
 */

import { now } from "@wf-agent/common-utils";
import {
  AgentLoopStatus,
  type ToolCallRecord,
  type IterationRecord,
  type AgentLoopStateSnapshot,
  type ExecutionErrorRecord,
  type ExecutionInterruptionRecord,
  type ExecutionEventRecord,
  EXECUTION_STATE_MAX_INTERRUPTION_RECORDS,
  EXECUTION_STATE_MAX_EVENTS,
} from "@wf-agent/types";
import type { LLMMessage } from "@wf-agent/types";
import { RuntimeValidationError } from "@wf-agent/types";
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
  /** Tools causing most errors */
  toolProblems: Array<{ name: string; count: number }>;
  /** Distribution by severity */
  severityBreakdown: Record<string, number>;
}

/**
 * AgentLoopState - Agent Loop Execution Status Manager
 *
 * Core Responsibilities:
 * - Manages the transition of execution states
 * - Logs the history of iterations and tool calls
 * - Tracks streaming state for LLM responses
 * - Tracks pending tool calls during execution
 *
 * Design Principles:
 * - Separated from persistent data
 * - Bound to the lifecycle and execution cycle
 * - Pure state management, without including business logic
 * - Snapshot functionality is provided by AgentLoopSnapshotManager
 */
export class AgentLoopState implements StateManager<AgentLoopStateSnapshot> {
  /** Current Status */
  private _status: AgentLoopStatus = AgentLoopStatus.CREATED;

  /** Current iteration count */
  private _currentIteration: number = 0;

  /** Total number of tool calls */
  private _toolCallCount: number = 0;

  /** Iterating through the history records */
  private _iterationHistory: IterationRecord[] = [];

  /** Current iteration record */
  private _currentIterationRecord: IterationRecord | null = null;

  /** Start time */
  private _startTime: number | null = null;

  /** End time */
  private _endTime: number | null = null;

  /** Error message */
  private _error: unknown = null;

  /** Pause start timestamp (ms) - set when execution is paused */
  private _pauseStartTime: number | null = null;

  /** Pause icon */
  private _shouldPause: boolean = false;

  /** Stop sign */
  private _shouldStop: boolean = false;

  // ========== Streaming State ==========

  /**
   * Partial message during streaming
   * Contains the incomplete assistant message being streamed from LLM
   */
  private _streamMessage: LLMMessage | null = null;

  /**
   * Pending tool call IDs
   * Tracks tool calls that have been requested but not yet completed
   */
  private _pendingToolCalls: Set<string> = new Set();

  /**
   * Streaming flag
   * Indicates if currently streaming LLM response
   */
  private _isStreaming: boolean = false;

  // ========== Plan C: Execution Event Tracking ==========

  /**
   * Error records during execution
   * Stores all errors that occurred, persisted with state
   */
  private _errorRecords: ExecutionErrorRecord[] = [];

  /**
   * Interruption records during execution
   * Stores all pauses and stops, persisted with state
   */
  private _interruptionRecords: ExecutionInterruptionRecord[] = [];

  /**
   * Event records during execution
   * Stores significant events for timeline tracking
   */
  private _eventRecords: ExecutionEventRecord[] = [];

  // ========== Variable History Tracking ==========

  /**
   * Variable snapshots captured at each iteration for debugging
   * Stores the state of all variables at key points during execution
   */
  private _variableSnapshots: Array<{
    iteration: number;
    timestamp: number;
    variables: Record<string, { value: unknown; type: string; size?: number; updated: boolean; source: 'user' | 'tool' | 'system' }>;
    toolCallId?: string;
  }> = [];

  /**
   * Get the current status
   */
  get status(): AgentLoopStatus {
    return this._status;
  }

  /**
   * Set the status
   */
  set status(value: AgentLoopStatus) {
    this._status = value;
  }

  /**
   * Get the current iteration count
   */
  get currentIteration(): number {
    return this._currentIteration;
  }

  /**
   * Get the total number of tool calls
   */
  get toolCallCount(): number {
    return this._toolCallCount;
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
   * Get pause start timestamp
   */
  get pauseStartTime(): number | null {
    return this._pauseStartTime;
  }

  /**
   * Get the error message
   */
  get error(): unknown {
    return this._error;
  }

  /**
   * Get the iteration history
   */
  get iterationHistory(): IterationRecord[] {
    return [...this._iterationHistory];
  }

  // ========== Streaming State Getters ==========

  /**
   * Get the streaming message
   * Returns the partial message being streamed
   */
  get streamMessage(): LLMMessage | null {
    return this._streamMessage;
  }

  /**
   * Set the streaming message
   */
  set streamMessage(value: LLMMessage | null) {
    this._streamMessage = value;
  }

  /**
   * Get pending tool calls
   */
  get pendingToolCalls(): Set<string> {
    return new Set(this._pendingToolCalls);
  }

  /**
   * Check if currently streaming
   */
  get isStreaming(): boolean {
    return this._isStreaming;
  }

  /**
   * Check whether it should be paused.
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
    // Validate state transition
    if (this._status !== AgentLoopStatus.CREATED && this._status !== AgentLoopStatus.PAUSED) {
      throw new RuntimeValidationError(
        `Can only start from CREATED or PAUSED status, current status: ${this._status}`,
        { operation: "start", field: "status", value: this._status },
      );
    }

    this._status = AgentLoopStatus.RUNNING;
    this._startTime = now();
  }

  /**
   * Start a new iteration
   */
  startIteration(): void {
    this._currentIteration++;
    this._currentIterationRecord = {
      iteration: this._currentIteration,
      startTime: now(),
      toolCalls: [],
    };
  }

  /**
   * End the current iteration
   * @param responseContent LLM response content
   */
  endIteration(responseContent?: string): void {
    if (this._currentIterationRecord) {
      this._currentIterationRecord.endTime = now();
      this._currentIterationRecord.responseContent = responseContent;
      this._iterationHistory.push(this._currentIterationRecord);
      this._currentIterationRecord = null;
    }
  }

  /**
   * Record the start of a tool invocation
   * @param id: Tool invocation ID
   * @param name: Tool name
   * @param args: Invocation parameters
   */
  recordToolCallStart(id: string, name: string, args: unknown): ToolCallRecord {
    const record: ToolCallRecord = {
      id,
      name,
      arguments: args,
      startTime: now(),
    };

    if (this._currentIterationRecord) {
      this._currentIterationRecord.toolCalls.push(record);
    }

    // Track pending tool calls for streaming coordination
    this._pendingToolCalls.add(id);

    return record;
  }

  /**
   * Record the end of the tool call
   * @param id Tool call ID
   * @param result Execution result
   * @param error Error message
   */
  recordToolCallEnd(id: string, result?: unknown, error?: string): void {
    if (this._currentIterationRecord) {
      const record = this._currentIterationRecord.toolCalls.find(tc => tc.id === id);
      if (record) {
        record.endTime = now();
        record.result = result;
        record.error = error;
      }
    }
    this._toolCallCount++;

    // Clean up pending tool call tracking
    this._pendingToolCalls.delete(id);
  }

  // ========== Streaming State Methods ==========

  /**
   * Start streaming
   * Sets streaming flag and initializes stream message
   */
  startStreaming(): void {
    this._isStreaming = true;
    this._streamMessage = null;
  }

  /**
   * Update streaming message
   * Updates the partial message during streaming
   * @param delta Partial message delta
   */
  updateStreamMessage(delta: Partial<LLMMessage>): void {
    if (!this._streamMessage) {
      // Initialize with delta
      this._streamMessage = delta as LLMMessage;
    } else {
      // Merge delta into existing message
      this._streamMessage = {
        ...this._streamMessage,
        ...delta,
      };
    }
  }

  /**
   * End streaming
   * Clears streaming flag and returns final message
   * @returns The complete streamed message
   */
  endStreaming(): LLMMessage | null {
    this._isStreaming = false;
    const message = this._streamMessage;
    this._streamMessage = null;
    return message;
  }

  /**
   * Add pending tool call
   * @param toolCallId Tool call ID
   */
  addPendingToolCall(toolCallId: string): void {
    this._pendingToolCalls.add(toolCallId);
  }

  /**
   * Remove pending tool call
   * @param toolCallId Tool call ID
   */
  removePendingToolCall(toolCallId: string): void {
    this._pendingToolCalls.delete(toolCallId);
  }

  /**
   * Check if tool call is pending
   * @param toolCallId Tool call ID
   */
  isToolCallPending(toolCallId: string): boolean {
    return this._pendingToolCalls.has(toolCallId);
  }

  /**
   * Get pending tool call count
   */
  getPendingToolCallCount(): number {
    return this._pendingToolCalls.size;
  }

  /**
   * Clear all pending tool calls
   */
  clearPendingToolCalls(): void {
    this._pendingToolCalls.clear();
  }

  // ========== Execution Records Management (Plan C) ==========

  /**
   * Add an error record
   * @param record Error record to add
   */
  addErrorRecord(record: ExecutionErrorRecord): void {
    // [Problem #5 Fix] Remove cap to be consistent with recordError()
    // Error retention is now managed at the persistence layer
    this._errorRecords.push(record);
  }

  /**
   * Add an interruption record
   * @param record Interruption record to add
   */
  addInterruptionRecord(record: ExecutionInterruptionRecord): void {
    this._interruptionRecords.push(record);
    // Keep only the latest N records to prevent state bloat
    if (this._interruptionRecords.length > EXECUTION_STATE_MAX_INTERRUPTION_RECORDS) {
      this._interruptionRecords = this._interruptionRecords.slice(
        -EXECUTION_STATE_MAX_INTERRUPTION_RECORDS,
      );
    }
  }

  /**
   * Record a pause interruption with enriched context
   * Captures the execution context at the time of pause for better debugging and recovery
   *
   * @param reason Reason for pausing
   * @param userId Optional user ID if user-initiated
   * @param source Source of the pause (user, system, timeout, error)
   */
  recordPauseInterruption(
    reason: string,
    userId?: string,
    source: 'user' | 'system' | 'timeout' | 'error' = 'system',
  ): void {
    const record: ExecutionInterruptionRecord = {
      id: `pause:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`,
      timestamp: now(),
      type: 'PAUSE',
      reason,
      iteration: this._currentIteration,

      // Trigger information
      triggeredBy: {
        source,
        userId,
        reason,
      },

      // Execution context snapshot
      executionContext: {
        iteration: this._currentIteration,
        status: this._status,
        lastSuccessfulToolCall: this._currentIterationRecord?.toolCalls[
          this._currentIterationRecord.toolCalls.length - 1
        ]?.id,
      },

      // Initial pause status
      status: 'pending',
    };

    this.addInterruptionRecord(record);
  }

  /**
   * Record a stop interruption with enriched context
   * Captures the execution context at the time of stop for recovery purposes
   *
   * @param reason Reason for stopping
   * @param userId Optional user ID if user-initiated
   * @param source Source of the stop (user, system, timeout, error)
   */
  recordStopInterruption(
    reason: string,
    userId?: string,
    source: 'user' | 'system' | 'timeout' | 'error' = 'system',
  ): void {
    const record: ExecutionInterruptionRecord = {
      id: `stop:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`,
      timestamp: now(),
      type: 'STOP',
      reason,
      iteration: this._currentIteration,

      // Trigger information
      triggeredBy: {
        source,
        userId,
        reason,
      },

      // Execution context snapshot
      executionContext: {
        iteration: this._currentIteration,
        status: this._status,
        lastSuccessfulToolCall: this._currentIterationRecord?.toolCalls[
          this._currentIterationRecord.toolCalls.length - 1
        ]?.id,
      },

      // Stop is typically final
      status: 'abandoned',
    };

    this.addInterruptionRecord(record);
  }

  /**
   * Record resumption of a paused execution
   * Updates the latest pause record with resume information
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
    let pauseRecord: ExecutionInterruptionRecord | undefined;
    for (let i = this._interruptionRecords.length - 1; i >= 0; i--) {
      if (this._interruptionRecords[i]?.type === 'PAUSE' && this._interruptionRecords[i]?.status === 'pending') {
        pauseRecord = this._interruptionRecords[i];
        break;
      }
    }

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
   * Add an event record
   * @param record Event record to add
   */
  addEventRecord(record: ExecutionEventRecord): void {
    this._eventRecords.push(record);
    // Keep only the latest N records to prevent state bloat
    if (this._eventRecords.length > EXECUTION_STATE_MAX_EVENTS) {
      this._eventRecords = this._eventRecords.slice(-EXECUTION_STATE_MAX_EVENTS);
    }
  }

  /**
   * Get error records
   */
  getErrorRecords(): ExecutionErrorRecord[] {
    return [...this._errorRecords];
  }

  /**
   * Get interruption records
   */
  getInterruptionRecords(): ExecutionInterruptionRecord[] {
    return [...this._interruptionRecords];
  }

  /**
   * Get interruption history with optional filtering
   * @param filter Optional filter: 'PAUSE' | 'STOP'
   * @returns Filtered interruption records
   */
  getInterruptionHistory(filter?: 'PAUSE' | 'STOP'): ExecutionInterruptionRecord[] {
    if (!filter) {
      return this.getInterruptionRecords();
    }
    return this._interruptionRecords.filter(record => record.type === filter);
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
    if (this._interruptionRecords.length === 0) {
      return {
        total: 0,
        byType: {},
        recoveryAttempts: 0,
        successfulRecoveries: 0,
        recoveryRate: 0,
      };
    }

    const records = this._interruptionRecords;
    const byType: Record<string, number> = {};
    let totalDuration = 0;
    let durationCount = 0;
    let recoveryAttempts = 0;
    let successfulRecoveries = 0;

    records.forEach(record => {
      // Count by type
      byType[record.type] = (byType[record.type] ?? 0) + 1;

      // Calculate duration if available
      if (record.resumedAt && record.timestamp) {
        const duration = record.resumedAt - record.timestamp;
        totalDuration += duration;
        durationCount++;
      }

      // Track recovery attempts
      if (record.type === 'PAUSE') {
        recoveryAttempts++;
      }

      // Track successful recoveries
      if (record.status === 'resumed') {
        successfulRecoveries++;
      }
    });

    return {
      total: records.length,
      byType,
      averageDuration: durationCount > 0 ? totalDuration / durationCount : undefined,
      recoveryAttempts,
      successfulRecoveries,
      recoveryRate: recoveryAttempts > 0 ? (successfulRecoveries / recoveryAttempts) * 100 : 0,
    };
  }

  /**
   * Get event records
   */
  getEventRecords(): ExecutionEventRecord[] {
    return [...this._eventRecords];
  }

  // ========== Variable History Tracking ==========

  /**
   * Capture a variable snapshot at the current iteration
   * Stores the state of all variables for debugging and analysis
   *
   * @param variables Object containing all variables to snapshot
   * @param source Source of the variables (user, tool, system)
   * @param toolCallId Optional tool call ID if snapshot is after tool execution
   */
  captureVariableSnapshot(
    variables: Record<string, unknown>,
    source: 'user' | 'tool' | 'system' = 'user',
    toolCallId?: string,
  ): void {
    const snapshot = {
      iteration: this._currentIteration,
      timestamp: now(),
      variables: {} as Record<string, { value: unknown; type: string; size?: number; updated: boolean; source: 'user' | 'tool' | 'system' }>,
      toolCallId,
    };

    // Get previous snapshot variables for comparison
    const previousSnapshot = this._variableSnapshots[this._variableSnapshots.length - 1];
    const previousVars = previousSnapshot?.variables || {};

    // Capture each variable
    for (const [name, value] of Object.entries(variables)) {
      const stringified = JSON.stringify(value);
      snapshot.variables[name] = {
        value,
        type: Array.isArray(value) ? 'array' : typeof value,
        size: stringified.length,
        updated: previousVars[name]?.value !== value,
        source,
      };
    }

    this._variableSnapshots.push(snapshot);

    // Keep reasonable history to prevent memory bloat
    // Keep last 1000 snapshots per execution
    if (this._variableSnapshots.length > 1000) {
      this._variableSnapshots = this._variableSnapshots.slice(-1000);
    }
  }

  /**
   * Get all variable snapshots
   */
  getVariableSnapshots(): Array<{
    iteration: number;
    timestamp: number;
    variables: Record<string, { value: unknown; type: string; size?: number; updated: boolean; source: 'user' | 'tool' | 'system' }>;
    toolCallId?: string;
  }> {
    return [...this._variableSnapshots];
  }

  /**
   * Get variable history for a specific variable
   * @param variableName Name of the variable to track
   * @returns Array of snapshots containing this variable
   */
  getVariableHistory(variableName: string): Array<{
    iteration: number;
    timestamp: number;
    value: unknown;
    type: string;
    updated: boolean;
    source: 'user' | 'tool' | 'system';
  }> {
    return this._variableSnapshots
      .filter(snap => variableName in snap.variables)
      .map(snap => ({
        iteration: snap.iteration,
        timestamp: snap.timestamp,
        value: snap.variables[variableName]!.value,
        type: snap.variables[variableName]!.type,
        updated: snap.variables[variableName]!.updated,
        source: snap.variables[variableName]!.source,
      }));
  }

  // ========== Error Chain Tracking ==========

  /**
   * Record an error with automatic error chain building
   *
   * Automatically establishes relationships between errors:
   * - Sets parentErrorId to the last error
   * - Builds errorChain array
   * - Identifies root cause
   *
   * NOTE: As of v2.0, error records are unlimited to ensure complete error history.
   * Previous versions had a limit of EXECUTION_STATE_MAX_ERROR_RECORDS (100),
   * which would drop earliest errors. Now all errors are retained.
   *
   * @param error Error record to add
   */
  recordError(error: ExecutionErrorRecord): void {
    // 1. Standardize error ID if not provided
    const errorId = error.id || `error:${Date.now()}:${Math.random().toString(36).slice(2, 9)}`;
    error.id = errorId;

    // 2. Build error chain relationships
    if (this._errorRecords.length > 0) {
      const lastError = this._errorRecords[this._errorRecords.length - 1]!;

      // 2a. Set parent error
      error.parentErrorId = lastError.id;

      // 2b. Build error chain
      if (lastError.errorChain) {
        error.errorChain = [...lastError.errorChain, errorId];
      } else {
        // First time establishing chain
        error.errorChain = [lastError.id, errorId];
      }

      // 2c. Quick reference to root cause
      error.rootCauseId = lastError.rootCauseId || lastError.id;
    } else {
      // This is the first error, it is the root cause
      error.errorChain = [errorId];
      error.rootCauseId = errorId;
    }

    // 3. Add to records (unlimited retention for complete error history)
    // Previous limit: EXECUTION_STATE_MAX_ERROR_RECORDS = 100 (deprecated)
    // Migration: All errors are now retained, users should implement their own
    // retention policies at the persistence layer if needed
    this._errorRecords.push(error);
  }

  /**
   * Get complete error chain for a specific error
   *
   * Returns all errors in the chain starting from the root cause
   * up to and including the specified error.
   *
   * @param fromErrorId Error ID to get chain for (default: last error)
   * @returns Array of errors in chain order
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
   *
   * @returns Root cause error, or null if no errors
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
   * Analyze error pattern in the error chain
   *
   * Provides statistics about error distribution:
   * - Total error count
   * - Errors grouped by type
   * - Problem tools causing most errors
   * - Error severity levels
   *
   * @returns Error pattern analysis
   */
  analyzeErrorPattern(): ErrorPattern {
    if (this._errorRecords.length === 0) {
      return {
        type: 'none',
        count: 0,
        errors: [],
        typeDistribution: {},
        toolProblems: [],
        severityBreakdown: {},
      };
    }

    const errors = this._errorRecords;
    const typeCount: Record<string, number> = {};
    const toolCount: Record<string, number> = {};
    const severityCount: Record<string, number> = {};

    errors.forEach(err => {
      // Count by error type
      typeCount[err.errorType] = (typeCount[err.errorType] ?? 0) + 1;

      // Count by tool
      if (err.context.toolName) {
        toolCount[err.context.toolName] = (toolCount[err.context.toolName] ?? 0) + 1;
      }

      // Count by severity
      severityCount[err.severity] = (severityCount[err.severity] ?? 0) + 1;
    });

    return {
      type: errors.length > 1 ? 'chain' : 'single',
      count: errors.length,
      errors,
      typeDistribution: typeCount,
      toolProblems: Object.entries(toolCount)
        .sort(([, a], [, b]) => b - a)
        .slice(0, 5)
        .map(([name, count]) => ({ name, count })),
      severityBreakdown: severityCount,
    };
  }

  /**
   * Check if errors can be recovered from
   *
   * Analyzes if the error chain can be recovered through suggested recovery actions.
   *
   * @returns true if all errors in chain are recoverable
   */
  canRecoverFromErrors(): boolean {
    return this._errorRecords.every(e => e.isRecoverable);
  }

  /**
   * Get recommended recovery action for the error chain
   *
   * Based on the error chain, returns the most appropriate recovery action.
   *
   * @returns Recommended action: 'retry' | 'fallback' | 'manual_intervention' | 'abort'
   */
  getRecommendedRecoveryAction(): "retry" | "fallback" | "manual_intervention" | "abort" {
    if (this._errorRecords.length === 0) {
      return 'abort';
    }

    // Check if all recoverable errors suggest the same action
    const retryCount = this._errorRecords.filter(e => e.recoveryAction === 'retry').length;
    const fallbackCount = this._errorRecords.filter(e => e.recoveryAction === 'fallback').length;
    const skipCount = this._errorRecords.filter(e => e.recoveryAction === 'skip').length;

    // Prefer the most common recovery action
    if (retryCount >= fallbackCount && retryCount >= skipCount) {
      return 'retry';
    }
    if (fallbackCount >= skipCount) {
      return 'fallback';
    }
    if (skipCount > 0) {
      return 'retry'; // Use retry as fallback for skip
    }

    return 'manual_intervention';
  }

  /**
   * Pause execution
   */
  pause(): void {
    // Validate state transition
    if (this._status !== AgentLoopStatus.RUNNING) {
      throw new RuntimeValidationError(
        `Can only pause RUNNING execution, current status: ${this._status}`,
        { operation: "pause", field: "status", value: this._status },
      );
    }

    this._status = AgentLoopStatus.PAUSED;
    this._shouldPause = false;
    this._pauseStartTime = Date.now();
  }

  /**
   * Resume execution
   */
  resume(): void {
    // Validate state transition
    if (this._status !== AgentLoopStatus.PAUSED) {
      throw new RuntimeValidationError(
        `Can only resume PAUSED execution, current status: ${this._status}`,
        { operation: "resume", field: "status", value: this._status },
      );
    }

    this._status = AgentLoopStatus.RUNNING;
    this._shouldPause = false;
    this._pauseStartTime = null;
  }

  /**
   * Complete the execution.
   */
  complete(): void {
    // Validate state transition
    if (this._status !== AgentLoopStatus.RUNNING) {
      throw new RuntimeValidationError(
        `Can only complete RUNNING execution, current status: ${this._status}`,
        { operation: "complete", field: "status", value: this._status },
      );
    }

    this._status = AgentLoopStatus.COMPLETED;
    this._endTime = now();
    this._isStreaming = false;
    this._streamMessage = null;
  }

  /**
   * Execution failed.
   * @param error Error message
   */
  fail(error: unknown): void {
    // Validate error parameter
    if (error === null || error === undefined) {
      throw new RuntimeValidationError("Error cannot be null or undefined", {
        operation: "fail",
        field: "error",
      });
    }

    // Validate state transition
    if (this._status === AgentLoopStatus.COMPLETED) {
      throw new RuntimeValidationError(
        `Cannot fail completed execution, current status: ${this._status}`,
        { operation: "fail", field: "status", value: this._status },
      );
    }

    this._status = AgentLoopStatus.FAILED;
    this._error = error;
    this._endTime = now();
    this._isStreaming = false;
    this._streamMessage = null;
  }

  /**
   * Cancel execution
   */
  cancel(): void {
    // Validate state transition
    if (this._status === AgentLoopStatus.COMPLETED || this._status === AgentLoopStatus.CANCELLED) {
      throw new RuntimeValidationError(
        `Cannot cancel completed or cancelled execution, current status: ${this._status}`,
        { operation: "cancel", field: "status", value: this._status },
      );
    }

    this._status = AgentLoopStatus.CANCELLED;
    this._endTime = now();
    this._isStreaming = false;
    this._streamMessage = null;
  }

  /**
   * Interrupt execution
   * @param type Type of the interrupt
   */
  interrupt(type: "PAUSE" | "STOP"): void {
    if (type === "PAUSE") {
      this._shouldPause = true;
    } else {
      this._shouldStop = true;
    }
  }

  /**
   * Reset the interrupt flag
   */
  resetInterrupt(): void {
    this._shouldPause = false;
    this._shouldStop = false;
  }

  /**
   * Clean up resources
   */
  cleanup(): void {
    this._iterationHistory = [];
    this._currentIterationRecord = null;
    this._error = null;
    this._streamMessage = null;
    this._pendingToolCalls.clear();
    this._isStreaming = false;
    this._errorRecords = [];
    this._interruptionRecords = [];
    this._eventRecords = [];
    this._variableSnapshots = [];
  }

  /**
   * Get the number of state items managed
   * Returns count of iteration records (primary state metric)
   * @returns Count of iteration records
   */
  size(): number {
    return this._iterationHistory.length;
  }

  /**
   * Check if the state is empty (no iterations recorded)
   * @returns true if no iterations exist
   */
  isEmpty(): boolean {
    return this._iterationHistory.length === 0;
  }

  /**
   * Reset to initial state
   * Clears all state except status (keeps CREATED)
   */
  reset(): void {
    this._currentIteration = 0;
    this._toolCallCount = 0;
    this._iterationHistory = [];
    this._currentIterationRecord = null;
    this._startTime = null;
    this._endTime = null;
    this._error = null;
    this._pauseStartTime = null;
    this._shouldPause = false;
    this._shouldStop = false;
    this._streamMessage = null;
    this._pendingToolCalls.clear();
    this._isStreaming = false;
    this._errorRecords = [];
    this._interruptionRecords = [];
    this._eventRecords = [];
    this._status = AgentLoopStatus.CREATED;
  }

  /**
   * Clone status
   */
  clone(): AgentLoopState {
    const cloned = new AgentLoopState();
    cloned._status = this._status;
    cloned._currentIteration = this._currentIteration;
    cloned._toolCallCount = this._toolCallCount;
    cloned._startTime = this._startTime;
    cloned._endTime = this._endTime;
    cloned._error = this._error;
    cloned._pauseStartTime = this._pauseStartTime;
    cloned._shouldPause = this._shouldPause;
    cloned._shouldStop = this._shouldStop;
    cloned._iterationHistory = this._iterationHistory.map(record => ({
      ...record,
      toolCalls: record.toolCalls.map(tc => ({ ...tc })),
    }));
    cloned._errorRecords = [...this._errorRecords];
    cloned._interruptionRecords = [...this._interruptionRecords];
    cloned._eventRecords = [...this._eventRecords];
    cloned._variableSnapshots = [...this._variableSnapshots];
    // Note: Runtime-only fields are not cloned (isStreaming, pendingToolCalls, streamMessage)
    return cloned;
  }

  /**
   * Create a snapshot of the current state (used for checkpoint creation)
   *
   * Design:
   * - Serializes persistent execution progress data (iteration count, status, tool calls)
   * - Does NOT include `messages` (owned by ConversationSession) or `config` (re-provided on restore)
   * - Includes streaming state fields for pause/resume precision:
   *   - `isStreaming`: Whether the agent was in the middle of streaming an LLM response
   *   - `streamMessage`: The partial/incomplete streamed message content (serialized as unknown)
   *   - `pendingToolCallIds`: Array of tool call IDs that were in-flight at snapshot time
   * - Includes execution records (Plan C):
   *   - `errorRecords`: Errors that occurred (atomic with state)
   *   - `interruptionRecords`: Interruptions/pauses that occurred
   *   - `eventRecords`: Timeline events
   *
   * @returns State snapshot
   */
  createSnapshot(): AgentLoopStateSnapshot {
    return {
      status: this._status,
      currentIteration: this._currentIteration,
      toolCallCount: this._toolCallCount,
      startTime: this._startTime,
      endTime: this._endTime,
      error: this._error,
      pauseStartTime: this._pauseStartTime,
      // Extended fields for complete state capture
      iterationHistory: this._iterationHistory.map(record => ({
        ...record,
        toolCalls: record.toolCalls.map(tc => ({ ...tc })),
      })),
      currentIterationRecord: this._currentIterationRecord
        ? {
            ...this._currentIterationRecord,
            toolCalls: this._currentIterationRecord.toolCalls.map(tc => ({ ...tc })),
          }
        : undefined,
      // Streaming state for pause/resume precision
      isStreaming: this._isStreaming || undefined,
      streamMessage: this._streamMessage ? { ...this._streamMessage } : undefined,
      pendingToolCallIds:
        this._pendingToolCalls.size > 0 ? Array.from(this._pendingToolCalls) : undefined,
      // Execution records (Plan C)
      errorRecords: this._errorRecords.length > 0 ? [...this._errorRecords] : undefined,
      interruptionRecords: this._interruptionRecords.length > 0 ? [...this._interruptionRecords] : undefined,
      eventRecords: this._eventRecords.length > 0 ? [...this._eventRecords] : undefined,
      // Variable history tracking
      variableSnapshots: this._variableSnapshots.length > 0 ? [...this._variableSnapshots] : undefined,
    };
  }

  /**
   * Restore state from snapshot (used for checkpoint restoration)
   */
  restoreFromSnapshot(snapshot: AgentLoopStateSnapshot): void {
    this._status = snapshot.status;
    this._currentIteration = snapshot.currentIteration;
    this._toolCallCount = snapshot.toolCallCount;
    this._startTime = snapshot.startTime;
    this._endTime = snapshot.endTime;
    this._error = snapshot.error;
    this._pauseStartTime = snapshot.pauseStartTime ?? null;

    // Restore extended fields if present
    if (snapshot.iterationHistory) {
      this._iterationHistory = snapshot.iterationHistory.map(record => ({
        ...record,
        toolCalls: record.toolCalls.map(tc => ({ ...tc })),
      }));
    } else {
      this._iterationHistory = [];
    }

    // Restore streaming state if present in snapshot
    this._pendingToolCalls.clear();
    this._isStreaming = snapshot.isStreaming ?? false;
    this._streamMessage = snapshot.streamMessage ? (snapshot.streamMessage as LLMMessage) : null;

    // Restore pending tool call IDs if present
    if (snapshot.pendingToolCallIds && snapshot.pendingToolCallIds.length > 0) {
      for (const toolCallId of snapshot.pendingToolCallIds) {
        this._pendingToolCalls.add(toolCallId);
      }
    }

    // Restore current iteration record if present
    const currentIterationRecord = snapshot["currentIterationRecord"] as
      | IterationRecord
      | undefined;
    if (currentIterationRecord) {
      this._currentIterationRecord = {
        iteration: currentIterationRecord.iteration,
        startTime: currentIterationRecord.startTime,
        endTime: currentIterationRecord.endTime,
        toolCalls: currentIterationRecord.toolCalls.map(tc => ({ ...tc })),
        responseContent: currentIterationRecord.responseContent,
      };
    } else {
      this._currentIterationRecord = null;
    }

    // Restore execution records (Plan C)
    this._errorRecords = snapshot.errorRecords ? [...snapshot.errorRecords] : [];
    this._interruptionRecords = snapshot.interruptionRecords ? [...snapshot.interruptionRecords] : [];
    this._eventRecords = snapshot.eventRecords ? [...snapshot.eventRecords] : [];

    // Restore variable snapshots for debugging
    this._variableSnapshots = snapshot.variableSnapshots ? [...snapshot.variableSnapshots] : [];

    // Reset runtime state that cannot be restored
    this._shouldPause = false;
    this._shouldStop = false;
  }
}
