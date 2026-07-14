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
import { AgentLoopStatus } from "./types.js";
import type { IterationRecord } from "./types.js";
import type { ExecutionHierarchyMetadata } from "../execution/hierarchy.js";
import type { AgentLoopStateSnapshot } from "../checkpoint/agent/snapshot.js";

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

  // ========== Timestamps ==========

  /** Execution start timestamp */
  startTime: Timestamp;

  /** Execution end timestamp (if completed) */
  endTime?: Timestamp;

  // ========== Error Handling ==========

  /** Error information (if failed) */
  error?: unknown;

  /**
   * Execution hierarchy metadata
   * 
   * Unified parent-child relationship management supporting richer relationships including Agent → Agent delegation.
   * 
   * @example
   * ```typescript
   * // Set parent context for Workflow parent
   * execution.hierarchy = {
   *   parent: {
   *     parentType: 'WORKFLOW',
   *     parentId: 'workflow-123',
   *     nodeId: 'agent-node-1',
   *   },
   *   children: [],
   *   depth: 1,
   *   rootExecutionId: 'root-workflow-id',
   *   rootExecutionType: 'WORKFLOW',
   * };
   * 
   * // Set parent context for Agent parent (Agent → Agent delegation)
   * execution.hierarchy = {
   *   parent: {
   *     parentType: 'AGENT_LOOP',
   *     parentId: 'parent-agent-456',
   *     delegationPurpose: 'Code review task delegation',
   *   },
   *   children: [],
   *   depth: 2,
   *   rootExecutionId: 'root-workflow-id',
   *   rootExecutionType: 'WORKFLOW',
   * };
   * ```
   */
  hierarchy?: ExecutionHierarchyMetadata;
}

/**
 * Agent Loop Execution State Snapshot
 *
 * @deprecated Use `AgentLoopStateSnapshot` from `@wf-agent/types/checkpoint/agent/snapshot` instead.
 * This type alias exists for backwards compatibility and will be removed in a future version.
 * `AgentLoopStateSnapshot` provides richer state capture (streaming state, trigger state,
 * variable snapshots, execution event tracking) and separates concerns:
 * - Messages are managed by ConversationSession, not in the snapshot
 * - Config is not serialized (must be re-provided by application)
 */
export type AgentLoopExecutionSnapshot = AgentLoopStateSnapshot;
