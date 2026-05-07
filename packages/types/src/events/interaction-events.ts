/**
 * Interaction-related event type definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";
import type { UserInteractionOperationType, PendingToolCallInfo } from "../interaction/index.js";
import type { ToolExecutionResult } from "../tool/execution.js";

/**
 * User interaction requested event type
 */
export interface UserInteractionRequestedEvent extends BaseEvent {
  type: "USER_INTERACTION_REQUESTED";
  /** Interaction ID */
  interactionId: ID;
  /** Operation type */
  operationType: UserInteractionOperationType;
  /** Prompt message */
  prompt: string;
  /** Timeout in milliseconds */
  timeout: number;
  /** Additional context data (optional) */
  contextData?: Record<string, unknown>;
}

/**
 * User interaction response event types
 */
export interface UserInteractionRespondedEvent extends BaseEvent {
  type: "USER_INTERACTION_RESPONDED";
  /** Interaction ID */
  interactionId: ID;
  /** User input data */
  inputData: unknown;
}

/**
 * User Interaction Processing Completion Event Type
 */
export interface UserInteractionProcessedEvent extends BaseEvent {
  type: "USER_INTERACTION_PROCESSED";
  /** Interaction ID */
  interactionId: ID;
  /** Type of operation */
  operationType: string;
  /** Disposal results */
  results: unknown;
}

/**
 * User interaction failure event types
 */
export interface UserInteractionFailedEvent extends BaseEvent {
  type: "USER_INTERACTION_FAILED";
  /** Interaction ID */
  interactionId: ID;
  /** Reasons for failure */
  reason: string;
}

/**
 * HumanRelay requested event type
 */
export interface HumanRelayRequestedEvent extends BaseEvent {
  type: "HUMAN_RELAY_REQUESTED";
  /** Request ID */
  requestId: ID;
  /** Prompt message */
  prompt: string;
  /** Message count */
  messageCount: number;
  /** Timeout in milliseconds */
  timeout: number;
}

/**
 * HumanRelay Response Event Type
 */
export interface HumanRelayRespondedEvent extends BaseEvent {
  type: "HUMAN_RELAY_RESPONDED";
  /** Request ID */
  requestId: ID;
  /** Manual input of content */
  content: string;
}

/**
 * HumanRelay Processing Completion Event Type
 */
export interface HumanRelayProcessedEvent extends BaseEvent {
  type: "HUMAN_RELAY_PROCESSED";
  /** Request ID */
  requestId: ID;
  /** Disposal results */
  message: {
    role: string;
    content: string;
  };
  /** Execution time (milliseconds) */
  executionTime: number;
}

/**
 * HumanRelay Failure Event Type
 */
export interface HumanRelayFailedEvent extends BaseEvent {
  type: "HUMAN_RELAY_FAILED";
  /** Request ID */
  requestId: ID;
  /** Reasons for failure */
  reason: string;
}

// =============================================================================
// Progressive Tool Execution Events
// =============================================================================

/**
 * Progressive tool execution start event
 * Emitted when a tool starts executing in a batch
 */
export interface ProgressiveToolExecutionStartEvent extends BaseEvent {
  type: "PROGRESSIVE_TOOL_EXECUTION_START";
  /** Execution ID */
  executionId: ID;
  /** Node ID (optional) */
  nodeId?: ID;
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** Batch ID */
  batchId?: string;
  /** Tool index in batch */
  toolIndex?: number;
  /** Total tools in batch */
  totalTools?: number;
  /** Pending queue */
  pendingQueue?: PendingToolCallInfo[];
}

/**
 * Progressive tool execution end event
 * Emitted when a tool finishes executing in a batch
 */
export interface ProgressiveToolExecutionEndEvent extends BaseEvent {
  type: "PROGRESSIVE_TOOL_EXECUTION_END";
  /** Execution ID */
  executionId: ID;
  /** Node ID (optional) */
  nodeId?: ID;
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** Batch ID */
  batchId?: string;
  /** Execution status */
  status: "success" | "error" | "warning";
  /** Execution result */
  result?: ToolExecutionResult;
  /** Execution time in milliseconds */
  executionTime?: number;
}

/**
 * Tool queue update event
 * Emitted to provide progress updates during batch execution
 */
export interface ToolQueueUpdateEvent extends BaseEvent {
  type: "TOOL_QUEUE_UPDATE";
  /** Execution ID */
  executionId: ID;
  /** Node ID (optional) */
  nodeId?: ID;
  /** Batch ID */
  batchId: string;
  /** Number of completed tools */
  completedCount: number;
  /** Total number of tools */
  totalCount: number;
  /** Remaining pending tools */
  pendingQueue: PendingToolCallInfo[];
}

/**
 * Tool approval annotated event
 * Emitted when user provides annotation with approval decision
 */
export interface ToolApprovalAnnotatedEvent extends BaseEvent {
  type: "TOOL_APPROVAL_ANNOTATED";
  /** Execution ID */
  executionId: ID;
  /** Node ID (optional) */
  nodeId?: ID;
  /** Interaction ID */
  interactionId: ID;
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** User's annotation/comment */
  annotation: string;
  /** Whether approved */
  approved: boolean;
}
