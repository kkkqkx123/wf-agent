/**
 * Tool class module export
 * Provides event-related utility functions
 */

// Event Construction Tool Function (from core)
export {
  buildWorkflowExecutionStartedEvent,
  buildWorkflowExecutionCompletedEvent,
  buildWorkflowExecutionFailedEvent,
  buildWorkflowExecutionPausedEvent,
  buildWorkflowExecutionResumedEvent,
  buildWorkflowExecutionCancelledEvent,
  buildWorkflowExecutionStateChangedEvent,
  buildNodeStartedEvent,
  buildNodeCompletedEvent,
  buildNodeFailedEvent,
  buildSubgraphStartedEvent,
  buildSubgraphCompletedEvent,
  buildVariableChangedEvent,
  buildMessageAddedEvent,
  buildTokenUsageWarningEvent,
  buildTokenLimitExceededEvent,
  buildConversationStateChangedEvent,
  buildToolCallStartedEvent,
  buildToolCallCompletedEvent,
  buildToolCallFailedEvent,
  buildWorkflowExecutionForkStartedEvent,
  buildWorkflowExecutionForkCompletedEvent,
  buildWorkflowExecutionJoinStartedEvent,
  buildWorkflowExecutionJoinConditionMetEvent,
  buildWorkflowExecutionCopyStartedEvent,
  buildWorkflowExecutionCopyCompletedEvent,
  buildTriggeredSubgraphStartedEvent,
  buildTriggeredSubgraphCompletedEvent,
  buildTriggeredSubgraphFailedEvent,
  buildCheckpointCreatedEvent,
  buildCheckpointFailedEvent,
  buildCheckpointDeletedEvent,
  buildToolApprovalRequestedEvent,
  buildFollowupQuestionRequestedEvent,
} from "../../../core/utils/event/builders/index.js";

// Event Trigger Utility Function (re-exported from core/utils/event)
export {
  emit,
} from "../../../core/utils/event/event-emitter.js";

// Event Waiting Tool Function
export {
  waitForWorkflowExecutionPaused,
  waitForWorkflowExecutionCancelled,
  waitForWorkflowExecutionCompleted,
  waitForWorkflowExecutionFailed,
  waitForWorkflowExecutionResumed,
  waitForAnyLifecycleEvent,
  waitForMultipleWorkflowExecutionsCompleted,
  waitForAnyWorkflowExecutionCompleted,
  waitForAnyWorkflowExecutionCompletion,
  waitForNodeCompleted,
  waitForNodeFailed,
} from "./event/event-waiter.js";

// General condition waiting function (re-exported from core/utils/event)
export {
  WAIT_FOREVER,
  waitForCondition,
  waitForAllConditions,
  waitForAnyCondition,
} from "../../../core/utils/event/condition-waiter.js";

// Workflow Operation Tool
export {
  fork,
  join,
  copy,
  type ForkConfig,
  type JoinStrategy,
  type JoinResult,
} from "./workflow-operations.js";

// Workflow Status Verification Tool
export {
  validateTransition,
  isValidTransition,
  getAllowedTransitions,
  isTerminalStatus,
  isActiveStatus,
} from "./workflow-state-validator.js";

export { checkWorkflowReferences } from "./workflow-reference-checker.js";

// Note: Hook creator functions previously exported from here
// have been removed. Use Trigger Custom Handler for custom logic.

// Checkpoint cleanup strategy (re-exported from core/utils/checkpoint)
export {
  TimeBasedCleanupStrategy,
  CountBasedCleanupStrategy,
  SizeBasedCleanupStrategy,
  createCleanupStrategy,
} from "../../../core/checkpoint/utils/cleanup-policy.js";

// Workflow-Specific Interruption Utilities
export {
  checkWorkflowInterruption,
  getWorkflowInterruptionType,
  getWorkflowInterruptionDescription,
  createWorkflowInterruptionAbortReason,
  toWorkflowInterruptionResult,
  type WorkflowInterruptionCheckResult,
} from "./workflow-interruption-utils.js";
