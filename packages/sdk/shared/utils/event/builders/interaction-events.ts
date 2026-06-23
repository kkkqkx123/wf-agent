/**
 * Interaction Event Builders
 * Provides builders for user interaction events
 */

import { createBuilder } from "./common.js";
import type {
  ProgressiveToolExecutionStartEvent,
  ProgressiveToolExecutionEndEvent,
  ToolQueueUpdateEvent,
  ToolApprovalAnnotatedEvent,
  ToolApprovalRequestedEvent,
  ToolApprovalRespondedEvent,
  ToolApprovalFailedEvent,
  FollowupQuestionRequestedEvent,
  FollowupQuestionRespondedEvent,
  FollowupQuestionFailedEvent,
} from "@wf-agent/types";

// =============================================================================
// Progressive Tool Execution Events
// =============================================================================

/**
 * Build progressive tool execution start event
 */
export const buildProgressiveToolExecutionStartEvent =
  createBuilder<ProgressiveToolExecutionStartEvent>("PROGRESSIVE_TOOL_EXECUTION_START");

/**
 * Build progressive tool execution end event
 */
export const buildProgressiveToolExecutionEndEvent =
  createBuilder<ProgressiveToolExecutionEndEvent>("PROGRESSIVE_TOOL_EXECUTION_END");

/**
 * Build tool queue update event
 */
export const buildToolQueueUpdateEvent = createBuilder<ToolQueueUpdateEvent>("TOOL_QUEUE_UPDATE");

/**
 * Build tool approval annotated event
 */
export const buildToolApprovalAnnotatedEvent =
  createBuilder<ToolApprovalAnnotatedEvent>("TOOL_APPROVAL_ANNOTATED");

// =============================================================================
// Tool Approval Events (Specific)
// =============================================================================

/**
 * Build tool approval requested event
 */
export const buildToolApprovalRequestedEvent =
  createBuilder<ToolApprovalRequestedEvent>("TOOL_APPROVAL_REQUESTED");

/**
 * Build tool approval responded event
 */
export const buildToolApprovalRespondedEvent =
  createBuilder<ToolApprovalRespondedEvent>("TOOL_APPROVAL_RESPONDED");

/**
 * Build tool approval failed event
 */
export const buildToolApprovalFailedEvent =
  createBuilder<ToolApprovalFailedEvent>("TOOL_APPROVAL_FAILED");

// =============================================================================
// Follow-up Question Events (Specific)
// =============================================================================

/**
 * Build follow-up question requested event
 */
export const buildFollowupQuestionRequestedEvent = createBuilder<FollowupQuestionRequestedEvent>(
  "FOLLOWUP_QUESTION_REQUESTED",
);

/**
 * Build follow-up question responded event
 */
export const buildFollowupQuestionRespondedEvent = createBuilder<FollowupQuestionRespondedEvent>(
  "FOLLOWUP_QUESTION_RESPONDED",
);

/**
 * Build follow-up question failed event
 */
export const buildFollowupQuestionFailedEvent = createBuilder<FollowupQuestionFailedEvent>(
  "FOLLOWUP_QUESTION_FAILED",
);
