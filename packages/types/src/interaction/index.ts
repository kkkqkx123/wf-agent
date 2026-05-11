/**
 * User Interaction Module
 * Unified export of all user interaction related types
 * 
 * This module exports general-purpose interaction protocols for app-level UI interactions.
 * Workflow-specific operations (UPDATE_VARIABLES, ADD_MESSAGE) are in node/configs/interaction-configs.js.
 */

export {
  // Core interaction types
  type UserInteractionOperationType,
  type UserInteractionRequest,
  type UserInteractionResponse,
  type UserInteractionResult,
  type UserInteractionContext,
  type UserInteractionHandler,
} from "./user-interaction.js";

export {
  // Tool approval types
  type PendingToolCallInfo,
  type ToolApprovalRequestData,
  type ToolApprovalResponseData,
} from "./tool-approval.js";

export {
  // Follow-up question types
  type FollowupQuestion,
  type FollowupQuestionRequestData,
  type FollowupQuestionResponseData,
} from "./followup-question.js";
