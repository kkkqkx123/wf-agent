/**
 * User Interaction Module
 * Unified export of all user interaction related types
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
  // Variable update types
  type VariableUpdateConfig,
  type MessageConfig,
} from "./variable-update.js";

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
