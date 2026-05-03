/**
 * Agent Loop Execution Definition Type
 *
 * This module contains the execution instance data object for Agent Loop.
 * Part of the agent-execution package for runtime-related types.
 *
 * Similar to WorkflowExecution, this is a pure data object that represents
 * the execution state of an Agent Loop. It does not contain methods.
 *
 * ## Architecture Context
 *
 * - `AgentLoopDefinition`: Static template definition (serializable)
 * - `AgentLoopExecution`: Execution instance data (serializable)
 * - `AgentLoopEntity`: SDK internal, wraps execution + methods (not serializable)
 *
 * ## Comparison with WorkflowExecution
 *
 * | Aspect | WorkflowExecution | AgentLoopExecution |
 * |--------|-------------------|-------------------|
 * | Graph Structure | Yes (WorkflowGraph) | No (linear execution) |
 * | Node Results | NodeExecutionResult[] | IterationRecord[] |
 * | Variables | VariableScopes | Simple Record |
 * | Input/Output | Structured | Via messages |
 */

import type { ID, Timestamp } from "../common.js";
import type { Message } from "../message/index.js";
import { AgentLoopStatus } from "./types.js";
import type { IterationRecord } from "./types.js";

/**
 * Agent Loop Execution
 *
 * Pure data object representing an Agent Loop execution instance.
 * This is the execution-time counterpart to AgentLoopDefinition.
 *
 * Key characteristics:
 * - Pure data structure with no methods (serializable)
 * - Contains execution state and history
 * - References the definition it was created from
 * - Used by AgentLoopEntity as the data layer
 */
export interface AgentLoopExecution {
  /** Execution unique identifier */
  id: ID;

  /** Reference to the Agent Loop Definition ID */
  definitionId: ID;

  /** Current execution status */
  status: AgentLoopStatus;

  // ========== Execution State ==========

  /** Current iteration number (0-based, incremented on each iteration) */
  currentIteration: number;

  /** Total number of tool calls made */
  toolCallCount: number;

  /** Complete iteration history */
  iterationHistory: IterationRecord[];

  // ========== Message State ==========

  /** Conversation messages */
  messages: Message[];

  /** Variables (simple key-value store) */
  variables: Record<string, unknown>;

  // ========== Timestamps ==========

  /** Execution start timestamp */
  startTime: Timestamp;

  /** Execution end timestamp (if completed) */
  endTime?: Timestamp;

  // ========== Error Handling ==========

  /** Error information (if failed) */
  error?: unknown;

  // ========== Context (when executed as Graph node) ==========

  /** Parent Workflow Execution ID (if executed as a Graph node) */
  parentWorkflowExecutionId?: ID;

  /** Node ID in parent workflow (if executed as a Graph node) */
  nodeId?: ID;
}

/**
 * Agent Loop Execution State Snapshot
 *
 * A simplified snapshot of execution state for checkpoint purposes.
 * This is what gets serialized to checkpoints.
 */
export interface AgentLoopExecutionSnapshot {
  /** Execution ID */
  id: ID;
  /** Definition ID */
  definitionId: ID;
  /** Status */
  status: AgentLoopStatus;
  /** Current iteration */
  currentIteration: number;
  /** Tool call count */
  toolCallCount: number;
  /** Iteration history */
  iterationHistory: IterationRecord[];
  /** Messages */
  messages: Message[];
  /** Variables */
  variables: Record<string, unknown>;
  /** Start time */
  startTime: Timestamp;
  /** End time */
  endTime?: Timestamp;
  /** Error */
  error?: unknown;
  /** Streaming flag */
  isStreaming?: boolean;
  /** Pending tool call IDs */
  pendingToolCalls?: string[];
}
