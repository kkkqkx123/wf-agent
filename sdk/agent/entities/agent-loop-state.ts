/**
 * AgentLoopState - Agent Loop Execution State Manager
 *
 * Manages the temporary state of an Agent Loop during execution, separate from persistent data.
 * This is the ONLY part of AgentLoopEntity that gets serialized to checkpoints.
 *
 * ## Architecture Role
 *
 * In the Agent Loop architecture:
 * - `AgentLoopConfig`: Immutable configuration (NOT serialized, contains functions)
 * - `AgentLoopState`: Mutable execution state (✅ SERIALIZED to checkpoints)
 * - `AgentLoopEntity`: Wrapper that holds config + state + runtime managers
 *
 * ## Serialization Strategy
 *
 * When creating a checkpoint:
 * 1. ✅ Serialize: `AgentLoopState` via `createSnapshot()` / `restoreFromSnapshot()`
 * 2. ❌ Skip: `AgentLoopConfig` (contains unserializable functions)
 * 3. ❌ Skip: Runtime managers (`ConversationSession`, `VariableState`)
 *
 * When restoring from checkpoint:
 * 1. Application provides `AgentLoopConfig` (re-inject callbacks)
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
 * @see AgentLoopConfig - Configuration (not serialized)
 * @see AgentLoopSnapshotManager - Checkpoint serialization logic
 */

import { now } from "@wf-agent/common-utils";
import {
  AgentLoopStatus,
  type ToolCallRecord,
  type IterationRecord,
  type AgentLoopStateSnapshot,
} from "@wf-agent/types";
import type { LLMMessage } from "@wf-agent/types";

/**
 * AgentLoopState - Agent Loop Execution Status Manager
 *
 * Core Responsibilities:
 * - Manages the transition of execution states
 * - Logs the history of iterations and tool calls
 * - Tracks streaming state (NEW)
 * - Tracks pending tool calls (NEW)
 *
 * Design Principles:
 * - Separated from persistent data
 * - Bound to the lifecycle and execution cycle
 * - Pure state management, without including business logic
 * - Snapshot functionality is provided by AgentLoopSnapshotManager
 */
export class AgentLoopState {
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

  /** Pause icon */
  private _shouldPause: boolean = false;

  /** Stop sign */
  private _shouldStop: boolean = false;

  // ========== Streaming State (NEW) ==========

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

  // ========== Streaming State Getters (NEW) ==========

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

    // Add to pending set (NEW)
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

    // Remove from pending set (NEW)
    this._pendingToolCalls.delete(id);
  }

  // ========== Streaming State Methods (NEW) ==========

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

  /**
   * Pause execution
   */
  pause(): void {
    this._status = AgentLoopStatus.PAUSED;
    this._shouldPause = false;
  }

  /**
   * Resume execution
   */
  resume(): void {
    this._status = AgentLoopStatus.RUNNING;
    this._shouldPause = false;
  }

  /**
   * Complete the execution.
   */
  complete(): void {
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
    cloned._shouldPause = this._shouldPause;
    cloned._shouldStop = this._shouldStop;
    cloned._iterationHistory = this._iterationHistory.map(record => ({
      ...record,
      toolCalls: record.toolCalls.map(tc => ({ ...tc })),
    }));
    cloned._streamMessage = this._streamMessage;
    cloned._pendingToolCalls = new Set(this._pendingToolCalls);
    cloned._isStreaming = this._isStreaming;
    return cloned;
  }

  /**
   * Create a snapshot of the current state (used for checkpoint creation)
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
      messages: [], // Messages are managed separately by AgentLoopEntity
      variables: {}, // Variables are managed separately by AgentLoopEntity
      // Extended fields for complete state capture
      iterationHistory: this._iterationHistory.map(record => ({
        ...record,
        toolCalls: record.toolCalls.map(tc => ({ ...tc })),
      })),
      isStreaming: this._isStreaming,
      pendingToolCalls: Array.from(this._pendingToolCalls),
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

    // Restore extended fields if present
    if (snapshot.iterationHistory) {
      this._iterationHistory = snapshot.iterationHistory.map(record => ({
        ...record,
        toolCalls: record.toolCalls.map(tc => ({ ...tc })),
      }));
    } else {
      this._iterationHistory = [];
    }

    if (snapshot.pendingToolCalls) {
      this._pendingToolCalls = new Set(snapshot.pendingToolCalls);
    } else {
      this._pendingToolCalls.clear();
    }

    if (snapshot.isStreaming !== undefined) {
      this._isStreaming = snapshot.isStreaming;
    } else {
      this._isStreaming = false;
    }

    // Reset runtime state that cannot be restored
    this._currentIterationRecord = null;
    this._shouldPause = false;
    this._shouldStop = false;
    this._streamMessage = null;
  }
}
