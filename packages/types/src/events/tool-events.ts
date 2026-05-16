/**
 * Tool Related Event Type Definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";

/**
 * Tool call start event type
 */
export interface ToolCallStartedEvent extends BaseEvent {
  type: "TOOL_CALL_STARTED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool call task ID (for tracking individual tool calls) */
  taskId?: string;
  /** Batch ID (identification of a batch of parallel tool calls) */
  batchId?: string;
  /** Tool name */
  toolName?: string;
  /** Tool parameters */
  toolArguments: string;
}

/**
 * Tool call completion event type
 */
export interface ToolCallCompletedEvent extends BaseEvent {
  type: "TOOL_CALL_COMPLETED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool call task ID (for tracking individual tool calls) */
  taskId?: string;
  /** Batch ID (identification of a batch of parallel tool calls) */
  batchId?: string;
  /** Tool name */
  toolName?: string;
  /** Tool results */
  toolResult: unknown;
  /** execution time */
  executionTime: number;
}

/**
 * Tool Call Failure Event Type
 */
export interface ToolCallFailedEvent extends BaseEvent {
  type: "TOOL_CALL_FAILED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool call task ID (for tracking individual tool calls) */
  taskId?: string;
  /** Batch ID (identification of a batch of parallel tool calls) */
  batchId?: string;
  /** Tool name */
  toolName?: string;
  /** error message */
  error: string;
}

/**
 * Tool Call Blocked Event Type (NEW - for failure protection)
 */
export interface ToolCallBlockedEvent extends BaseEvent {
  type: "TOOL_CALL_BLOCKED";
  /** Execution ID */
  executionId: ID;
  /** Node ID */
  nodeId: ID;
  /** Tool ID */
  toolId: ID;
  /** Tool name */
  toolName?: string;
  /** Current failure count */
  failureCount: number;
  /** Last error message */
  lastError?: string;
  /** Remaining cooldown time in milliseconds */
  remainingCooldown?: number;
  /** Blocking reason */
  reason?: string;
}

/**
 * Tool to add event types
 */
export interface ToolAddedEvent extends BaseEvent {
  type: "TOOL_ADDED";
  /** Node ID */
  nodeId: ID;
  /** Tool ID List */
  toolIds: ID[];
  /** Scope (EXECUTION or LOCAL) */
  scope: "EXECUTION" | "LOCAL";
  /** Number of tools successfully added */
  addedCount: number;
  /** Number of tools skipped */
  skippedCount: number;
}

/**
 * Tool visibility changed event type
 * Triggered when the set of visible tools changes (scope switch, dynamic add/remove, etc.)
 */
export interface ToolVisibilityChangedEvent extends BaseEvent {
  type: "TOOL_VISIBILITY_CHANGED";
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId?: ID;
  /** Node ID (optional, if triggered by node execution) */
  nodeId?: ID;
  /** Current scope */
  scope: "EXECUTION" | "SUBGRAPH" | "LOOP";
  /** Scope ID */
  scopeId: ID;
  /** Change type */
  changeType: "enter_scope" | "exit_scope" | "add_tools" | "remove_tools" | "refresh" | "init";
  /** Currently visible tool IDs */
  visibleToolIds: ID[];
  /** Previous visible tool IDs (for comparison) */
  previousVisibleToolIds?: ID[];
  /** Timestamp of the change */
  timestamp: number;
}
