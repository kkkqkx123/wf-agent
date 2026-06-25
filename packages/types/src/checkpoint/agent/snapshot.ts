/**
 * Agent Loop State Snapshot Type Definition
 *
 * ## Design Principles
 *
 * 1. **Config NOT serialized**: `AgentLoopRuntimeConfig` contains callback functions
 *    (`transformContext`, `convertToLlm`) that cannot be serialized to JSON/TOML.
 *    Config must be re-provided by the application when restoring from checkpoint.
 *
 * 2. **Messages NOT in snapshot**: Messages are owned and managed by `ConversationSession`,
 *    not by `AgentLoopState`. The `AgentLoopStateSnapshot` captures only the execution
 *    progress state (iteration count, tool calls, status), while messages are handled
 *    through the conversation's checkpoint path.
 *
 * 3. **Restoration flow**: Application calls `AgentLoopFactory.fromCheckpoint()`
 *    with both the checkpoint ID and a fresh `AgentLoopRuntimeConfig`.
 *    The factory loads the snapshot, creates state from it, and injects the config.
 */

import { AgentLoopStatus } from "../../agent-execution/types.js";
import type { IterationRecord } from "../../agent-execution/types.js";
import type { ExecutionErrorRecord, ExecutionInterruptionRecord, ExecutionEventRecord } from "../execution-events.js";

/**
 * Agent Loop Status Snapshot
 *
 * Contains only serializable execution progress data.
 * Does NOT include:
 * - `config`: Must be re-provided by application (contains callbacks)
 * - `messages`: Managed by ConversationSession, not AgentLoopState
 *
 * ## Plan C Changes
 *
 * Added execution-related arrays to ensure all execution data is persisted
 * atomically with the state:
 * - `errors`: Errors that occurred during execution (was in ExecutionHistoryAPI)
 * - `interruptions`: Pauses/stops during execution (was in ExecutionHistoryAPI)
 * - `events`: Significant execution events for timeline tracking (was in ExecutionHistoryAPI)
 *
 * This eliminates the separate ExecutionHistoryAPI storage and ensures data
 * consistency and disaster recovery.
 */
export interface AgentLoopStateSnapshot {
  /** Execution status */
  status: AgentLoopStatus;
  /** Current iteration number */
  currentIteration: number;
  /** Total tool call count */
  toolCallCount: number;
  /** Execution start timestamp (ms) */
  startTime: number | null;
  /** Execution end timestamp (ms) */
  endTime: number | null;
  /** Error data (if execution failed) - DEPRECATED: use errors array */
  error: unknown;

  // ========== Plan C: Execution Event Tracking ==========

  /** Errors that occurred during execution (atomic with state) */
  errorRecords?: ExecutionErrorRecord[];

  /** Interruptions (pauses/stops) that occurred during execution */
  interruptionRecords?: ExecutionInterruptionRecord[];

  /** Recent execution events for timeline view (limited to prevent state bloat) */
  eventRecords?: ExecutionEventRecord[];

  // ========== Extended fields for complete state capture ==========

  /** Iteration history records */
  iterationHistory?: IterationRecord[];
  /** Current in-progress iteration record (if any) */
  currentIterationRecord?: IterationRecord;

  /** Dynamic tools (serialized from Set<string> to string[]) */
  dynamicTools?: string[];

  // ========== Streaming state fields (for pause/resume precision) ==========

  /** Whether the agent was streaming an LLM response at checkpoint time */
  isStreaming?: boolean;
  /** Incomplete streaming message content (for stream resume) */
  streamMessage?: unknown;
  /** Pending tool call IDs that were in-flight at checkpoint time */
  pendingToolCallIds?: string[];

  // ========== Trigger state (for tracking trigger fires and limits) ==========

  /** Trigger state snapshot (serialized from TriggerStateManager) */
  triggerState?: Record<string, {
    triggerId: string;
    fireCount: number;
    lastFiredAt?: number;
    metadata?: Record<string, unknown>;
  }>;

  // ========== Variable history tracking ==========

  /** Variable snapshots captured at each iteration for debugging */
  variableSnapshots?: Array<{
    /** Iteration number when snapshot was taken */
    iteration: number;
    /** Timestamp when snapshot was taken */
    timestamp: number;
    /** Variables at this point in execution */
    variables: Record<string, VariableSnapshot>;
    /** Tool call ID if snapshot was taken after a tool call */
    toolCallId?: string;
  }>;
}

/**
 * Variable snapshot details
 */
export interface VariableSnapshot {
  /** Variable value at this point */
  value: unknown;
  /** Type of the value */
  type: string;
  /** Size in bytes (for large objects) */
  size?: number;
  /** Whether this variable was updated in this iteration */
  updated: boolean;
  /** Source of the variable (user, tool, system) */
  source: 'user' | 'tool' | 'system';
}
