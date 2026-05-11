/**
 * Interaction Event Builders
 * Provides builders for user interaction and human relay events
 */

import { now } from "@wf-agent/common-utils";
import { createBuilder, type BuildParams } from "./common.js";
import type {
  HumanRelayRequestedEvent,
  HumanRelayRespondedEvent,
  HumanRelayFailedEvent,
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
// Human Relay Events
// =============================================================================

/**
 * Build human relay requested event
 */
export const buildHumanRelayRequestedEvent =
  createBuilder<HumanRelayRequestedEvent>("HUMAN_RELAY_REQUESTED");

/**
 * Build human relay responded event
 */
export const buildHumanRelayRespondedEvent =
  createBuilder<HumanRelayRespondedEvent>("HUMAN_RELAY_RESPONDED");

/**
 * Build human relay failed event
 */
export const buildHumanRelayFailedEvent = (
  params: BuildParams<HumanRelayFailedEvent>,
): HumanRelayFailedEvent =>
  ({ type: "HUMAN_RELAY_FAILED", timestamp: now(), ...params }) as HumanRelayFailedEvent;

// =============================================================================
// Progressive Tool Execution Events
// =============================================================================

/**
 * Build progressive tool execution start event
 */
export const buildProgressiveToolExecutionStartEvent = createBuilder<ProgressiveToolExecutionStartEvent>(
  "PROGRESSIVE_TOOL_EXECUTION_START",
);

/**
 * Build progressive tool execution end event
 */
export const buildProgressiveToolExecutionEndEvent = createBuilder<ProgressiveToolExecutionEndEvent>(
  "PROGRESSIVE_TOOL_EXECUTION_END",
);

/**
 * Build tool queue update event
 */
export const buildToolQueueUpdateEvent = createBuilder<ToolQueueUpdateEvent>(
  "TOOL_QUEUE_UPDATE",
);

/**
 * Build tool approval annotated event
 */
export const buildToolApprovalAnnotatedEvent = (
  params: BuildParams<ToolApprovalAnnotatedEvent>,
): ToolApprovalAnnotatedEvent =>
  ({
    type: "TOOL_APPROVAL_ANNOTATED",
    timestamp: now(),
    ...params,
  }) as ToolApprovalAnnotatedEvent;

// =============================================================================
// Tool Approval Events (Specific)
// =============================================================================

/**
 * Build tool approval requested event
 */
export const buildToolApprovalRequestedEvent = createBuilder<ToolApprovalRequestedEvent>(
  "TOOL_APPROVAL_REQUESTED",
);

/**
 * Build tool approval responded event
 */
export const buildToolApprovalRespondedEvent = createBuilder<ToolApprovalRespondedEvent>(
  "TOOL_APPROVAL_RESPONDED",
);

/**
 * Build tool approval failed event
 */
export const buildToolApprovalFailedEvent = (
  params: BuildParams<ToolApprovalFailedEvent>,
): ToolApprovalFailedEvent =>
  ({
    type: "TOOL_APPROVAL_FAILED",
    timestamp: now(),
    ...params,
  }) as ToolApprovalFailedEvent;

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
export const buildFollowupQuestionFailedEvent = (
  params: BuildParams<FollowupQuestionFailedEvent>,
): FollowupQuestionFailedEvent =>
  ({
    type: "FOLLOWUP_QUESTION_FAILED",
    timestamp: now(),
    ...params,
  }) as FollowupQuestionFailedEvent;
