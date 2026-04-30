/**
 * Tool class module export
 * Provides event-related utility functions
 */

// Event Construction Tool Function
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
  buildUserInteractionRequestedEvent,
  buildUserInteractionProcessedEvent,
} from "./event/index.js";

// Event Trigger Utility Function (re-exported from core/utils/event)
export {
  safeEmit,
  emit,
  emitBatch,
  emitBatchParallel,
  emitIf,
  emitDelayed,
  emitWithRetry,
  emitAndWaitForCallback,
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

export { VariableAccessor, VariableNamespace } from "./variable-accessor.js";

export { checkWorkflowReferences } from "./workflow-reference-checker.js";

// Hook Creator Tool (reexports the common parts from core/utils/hook)
export {
  createWorkflowExecutionStateCheckHook,
  createCustomValidationHook,
  createPermissionCheckHook,
  createAuditLoggingHook,
} from "./hook-creators.js";

// Callback utility function (re-exported from core/utils/callback)
export {
  wrapCallback,
  createTimeoutPromise,
  withTimeout,
  validateCallback,
  createSafeCallback,
  executeCallbacks,
  createRetryCallback,
  createThrottledCallback,
  createDebouncedCallback,
  createOnceCallback,
  createCachedCallback,
  cleanupCache,
} from "../../../core/utils/callback.js";

// Checkpoint utilities are now exported from checkpoint/
// Re-export here for backward compatibility
export { CheckpointDiffCalculator } from "../../checkpoint/utils/diff-calculator.js";
export { DeltaCheckpointRestorer } from "../../checkpoint/utils/delta-restorer.js";

// Checkpoint cleanup strategy (re-exported from core/utils/checkpoint)
export {
  TimeBasedCleanupStrategy,
  CountBasedCleanupStrategy,
  SizeBasedCleanupStrategy,
  createCleanupStrategy,
} from "../../../core/utils/checkpoint/cleanup-policy.js";

// Checkpoint serialization (re-exported from core/serialization)
export {
  CheckpointSnapshotSerializer,
  registerCheckpointSerializer,
} from "../../../core/serialization/entities/checkpoint-serializer.js";

import { CheckpointSnapshotSerializer as _CheckpointSnapshotSerializer } from "../../../core/serialization/entities/checkpoint-serializer.js";

const _checkpointSerializer = new _CheckpointSnapshotSerializer();
export const serializeCheckpoint = (checkpoint: import("@wf-agent/types").Checkpoint) =>
  _checkpointSerializer.serializeCheckpoint(checkpoint);
export const deserializeCheckpoint = (data: Uint8Array) =>
  _checkpointSerializer.deserializeCheckpoint(data);
