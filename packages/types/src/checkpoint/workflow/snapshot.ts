/**
 * Workflow Execution state snapshot type definition
 */

import type { ID } from "../../common.js";
import { WorkflowExecutionStatus } from "../../workflow-execution/index.js";
import type { NodeExecutionResult, TriggeredSubworkflowContext } from "../../workflow-execution/index.js";
import type { TriggerRuntimeState } from "../../trigger/index.js";
import type { TokenUsageStats } from "../../llm/index.js";
import type { MessageMarkMap } from "../../message/index.js";
import type { CheckpointVariableState } from "../variable-state.js";
import type { ExecutionErrorRecord, ExecutionInterruptionRecord, ExecutionEventRecord } from "../execution-events.js";

import type { ExecutionHierarchyMetadata } from "../../execution/hierarchy.js";

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
 * Workflow Execution state snapshot type
 */
export interface WorkflowExecutionStateSnapshot {
  /** Execution Status */
  status: WorkflowExecutionStatus;
  /** Current node ID */
  currentNodeId: ID;
  /** Variable array */
  variables: unknown[];
  /** Complete variable state for checkpoint (includes global, execution, and temporary scopes) */
  variableState: CheckpointVariableState;
  /** Input data */
  input: Record<string, unknown>;
  /** Output data */
  output: Record<string, unknown>;
  /** Node execution result mapping */
  nodeResults: Record<string, NodeExecutionResult>;
  /** Error message array - DEPRECATED: use errorRecords for new code */
  errors: unknown[];
  /** Conversation state (stores the complete message history and index information, used for restoring the ConversationSession) */
  conversationState: {
    /** Full message history array */
    messages: unknown[];
    /** Message Tag Mapping */
    markMap: MessageMarkMap;
    /** Token Usage Statistics */
    tokenUsage: TokenUsageStats | null;
    /** The current request token is in use. */
    currentRequestUsage: TokenUsageStats | null;
  };
  /** Tool Approval Status (used to resume tool calls that are waiting for approval) */
  toolApprovalState?: {
    /** The tool call that is currently awaiting approval. */
    pendingToolCall?: {
      /** Tool call ID */
      id: string;
      /** Tool Name */
      name: string;
      /** Tool parameters */
      arguments: string;
    };
    /** Interaction ID */
    interactionId: string;
    /** Approval timeout period */
    timeout: number;
  };
  /** Trigger state snapshot (used for restoring TriggerState) */
  triggerStates?: Map<ID, TriggerRuntimeState>;
  /** FORK/JOIN context (master-slave separation mode) */
  forkJoinContext?: {
    forkId: string;
    forkPathId: string;
  };
  /** Triggered Sub-workflow Context (Master-Slave Separation Mode) */
  triggeredSubworkflowContext?: TriggeredSubworkflowContext;
  /** Current operation state (for mid-node resume) */
  currentOperation?: OperationState;

  /** Execution hierarchy metadata (contains parent, children, depth, root info) */
  hierarchy?: ExecutionHierarchyMetadata;

  /** Hook execution context for condition evaluation after restore */
  hookExecutionContext?: {
    workflowInput: Record<string, unknown>;
    output: unknown;
    variables: Record<string, unknown>;
    messages: unknown[];
  };

  // ========== Plan C: Execution Event Tracking ==========

  /** Errors that occurred during execution (atomic with state) */
  errorRecords?: ExecutionErrorRecord[];

  /** Interruptions (pauses/stops) that occurred during execution */
  interruptionRecords?: ExecutionInterruptionRecord[];

  /** Recent execution events for timeline view (limited to prevent state bloat) */
  eventRecords?: ExecutionEventRecord[];

  /** FORK/JOIN aggregation state for JOIN node result merging */
  forkJoinAggregationState?: {
    /** The FORK node ID that spawned the branches */
    forkNodeId: ID;
    /** The JOIN node ID that will aggregate results */
    joinNodeId: ID;
    /** Map of fork path ID to branch status ("PENDING" | "COMPLETED" | "FAILED") */
    pathStatuses: Record<string, "PENDING" | "COMPLETED" | "FAILED">;
    /** Aggregated results from completed branches (for early resume) */
    aggregatedResults?: Record<string, unknown>;
    /** Merged variables from all completed branches */
    mergedVariables?: Record<string, unknown>;
    /** Whether aggregation is complete */
    isAggregationComplete: boolean;
    /** Error messages from failed branches */
    failedBranchErrors?: Array<{ pathId: string; error: string }>;
  };
}

/**
 * Workflow Execution State Snapshot for internal state manager use
 * Mirrors WorkflowExecutionStateSnapshot but used by WorkflowExecutionState class
 */
export interface WorkflowExecutionStateManagerSnapshot {
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
