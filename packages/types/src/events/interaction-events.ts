/**
 * Interaction-related event type definitions
 */

import type { ID } from "../common.js";
import type { BaseEvent } from "./base.js";
import type { PendingToolCallInfo } from "../interaction/index.js";
import type { ToolExecutionResult } from "../tool/execution.js";

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

// =============================================================================
// Tool Approval Events (Specific)
// =============================================================================

/**
 * Tool approval requested event
 */
export interface ToolApprovalRequestedEvent extends BaseEvent {
  type: "TOOL_APPROVAL_REQUESTED";
  /** Interaction ID */
  interactionId: ID;
  /** Tool call to approve */
  toolCall: import("../message/message.js").LLMToolCall;
  /** Tool description */
  toolDescription?: string;
  /** Context ID */
  contextId: ID;
  /** Node ID */
  nodeId?: ID;
  /** Timeout in milliseconds */
  timeout: number;
  /** Batch metadata */
  batchId?: string;
  toolIndex?: number;
  totalTools?: number;
  pendingQueue?: import("../message/message.js").LLMToolCall[];
}

/**
 * Tool approval responded event
 */
export interface ToolApprovalRespondedEvent extends BaseEvent {
  type: "TOOL_APPROVAL_RESPONDED";
  /** Interaction ID */
  interactionId: ID;
  /** Approval result */
  result: import("../tool/approval.js").ToolApprovalResult;
}

/**
 * Tool approval failed event
 */
export interface ToolApprovalFailedEvent extends BaseEvent {
  type: "TOOL_APPROVAL_FAILED";
  /** Interaction ID */
  interactionId: ID;
  /** Error reason */
  error: string;
}

// =============================================================================
// Follow-up Question Events (Specific)
// =============================================================================

/**
 * Follow-up question requested event
 */
export interface FollowupQuestionRequestedEvent extends BaseEvent {
  type: "FOLLOWUP_QUESTION_REQUESTED";
  /** Interaction ID */
  interactionId: ID;
  /** Questions to ask */
  questions: Array<{
    index: number;
    text: string;
    options: Array<{
      index: number;
      value: string;
    }>;
  }>;
  /** Label for additional info */
  additionalInfoLabel: string;
  /** Timeout in milliseconds */
  timeout: number;
  /** Metadata */
  metadata?: {
    executionId?: ID;
    nodeId?: ID;
  };
}

/**
 * Follow-up question responded event
 */
export interface FollowupQuestionRespondedEvent extends BaseEvent {
  type: "FOLLOWUP_QUESTION_RESPONDED";
  /** Interaction ID */
  interactionId: ID;
  /** User answers */
  answers: Array<{
    questionIndex: number;
    selectedOptionIndex: number;
    customInput?: string;
    answer: string;
  }>;
  /** Additional info */
  additionalInfo?: string;
}

/**
 * Follow-up question failed event
 */
export interface FollowupQuestionFailedEvent extends BaseEvent {
  type: "FOLLOWUP_QUESTION_FAILED";
  /** Interaction ID */
  interactionId: ID;
  /** Error reason */
  error: string;
}
