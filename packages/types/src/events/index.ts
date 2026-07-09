/**
 * Unified Export of Events Type Definitions
 * Define event types during workflow execution
 */

// Export base type
export * from "./base.js";

// Exporting workflow execution-related events
export * from "./workflow-execution-events.js";

// Exporting node-related events
export * from "./node-events.js";

// Export tool related events
export * from "./tool-events.js";

// Exporting dialog-related events
export * from "./conversation-events.js";

// Export checkpoint related events
export * from "./checkpoint-events.js";

// Events related to exporting subgraphs
export * from "./subgraph-events.js";

// Export Interaction Related Events
export * from "./interaction-events.js";

// Exporting system events
export * from "./system-events.js";

// Exporting Agent Related Events
export * from "./agent-events.js";

// Export Skill related events
export * from "./skill-events.js";

// Export Async Completion related events
export * from "./async-completion-events.js";

// Export Attempt Completion related events
export * from "./attempt-completion-events.js";

// Re-export the EventType for backward compatibility.
export { EventType } from "./base.js";

// Export type guard functions
export * from "./type-guards.js";

// Export the union type of all event types
import type {
  WorkflowExecutionStartedEvent,
  WorkflowExecutionCompletedEvent,
  WorkflowExecutionFailedEvent,
  WorkflowExecutionPausedEvent,
  WorkflowExecutionResumedEvent,
  WorkflowExecutionCancelledEvent,
  WorkflowExecutionStateChangedEvent,
  WorkflowExecutionForkStartedEvent,
  WorkflowExecutionForkCompletedEvent,
  WorkflowExecutionJoinStartedEvent,
  WorkflowExecutionJoinConditionMetEvent,
  WorkflowExecutionJoinCompletedEvent,
  WorkflowExecutionJoinFailedEvent,
  WorkflowExecutionCopyStartedEvent,
  WorkflowExecutionCopyCompletedEvent,
} from "./workflow-execution-events.js";

import type {
  NodeStartedEvent,
  NodeCompletedEvent,
  NodeFailedEvent,
  NodeCustomEvent,
  ForkStartedEvent,
  ForkBranchStartedEvent,
  ForkBranchCompletedEvent,
  ForkCompletedEvent,
  NodeSyncStartedEvent,
  NodeSyncCompletedEvent,
  NodeSyncFailedEvent,
} from "./node-events.js";

import type {
  ToolCallStartedEvent,
  ToolCallCompletedEvent,
  ToolCallFailedEvent,
  ToolAddedEvent,
  ToolVisibilityChangedEvent,
} from "./tool-events.js";

import type { MessageAddedEvent, ConversationStateChangedEvent } from "./conversation-events.js";

import type {
  CheckpointCreatedEvent,
  CheckpointRestoredEvent,
  CheckpointDeletedEvent,
  CheckpointFailedEvent,
} from "./checkpoint-events.js";

import type {
  SubgraphStartedEvent,
  SubgraphCompletedEvent,
  TriggeredSubgraphStartedEvent,
  TriggeredSubgraphCompletedEvent,
  TriggeredSubgraphFailedEvent,
} from "./subgraph-events.js";

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
} from "./interaction-events.js";

import type {
  TokenLimitExceededEvent,
  TokenUsageWarningEvent,
  ErrorEvent,
  VariableChangedEvent,
  LLMStreamAbortedEvent,
  LLMStreamErrorEvent,
  ContextCompressionRequestedEvent,
  ContextCompressionCompletedEvent,
} from "./system-events.js";

import type {
  AgentStartedEvent,
  AgentCompletedEvent,
  AgentTurnStartedEvent,
  AgentTurnCompletedEvent,
  AgentMessageStartedEvent,
  AgentMessageCompletedEvent,
  AgentToolExecutionStartedEvent,
  AgentToolExecutionCompletedEvent,
  AgentIterationStartedEvent,
  AgentIterationCompletedEvent,
  AgentHookTriggeredEvent,
  AgentPausedEvent,
  AgentCancelledEvent,
  AgentResumedEvent,
  AgentFailedEvent,
} from "./agent-events.js";
import type {
  SkillLoadStartedEvent,
  SkillLoadCompletedEvent,
  SkillLoadFailedEvent,
} from "./skill-events.js";

import type {
  AsyncCompletionRegisteredEvent,
  AsyncCompletionTriggeredEvent,
  AsyncCompletionErrorTriggeredEvent,
  AsyncCompletionFailedEvent,
  AsyncCompletionCleanedUpEvent,
} from "./async-completion-events.js";

import type { AttemptCompletionEvent } from "./attempt-completion-events.js";

/**
 * Union type for all event types
 */
export type Event =
  | WorkflowExecutionStartedEvent
  | WorkflowExecutionCompletedEvent
  | WorkflowExecutionFailedEvent
  | WorkflowExecutionPausedEvent
  | WorkflowExecutionResumedEvent
  | WorkflowExecutionCancelledEvent
  | WorkflowExecutionStateChangedEvent
  | WorkflowExecutionForkStartedEvent
  | WorkflowExecutionForkCompletedEvent
  | WorkflowExecutionJoinStartedEvent
  | WorkflowExecutionJoinConditionMetEvent
  | WorkflowExecutionJoinCompletedEvent
  | WorkflowExecutionJoinFailedEvent
  | WorkflowExecutionCopyStartedEvent
  | WorkflowExecutionCopyCompletedEvent
  | NodeStartedEvent
  | NodeCompletedEvent
  | NodeFailedEvent
  | NodeCustomEvent
  | ForkStartedEvent
  | ForkBranchStartedEvent
  | ForkBranchCompletedEvent
  | ForkCompletedEvent
  | NodeSyncStartedEvent
  | NodeSyncCompletedEvent
  | NodeSyncFailedEvent
  | TokenLimitExceededEvent
  | TokenUsageWarningEvent
  | MessageAddedEvent
  | ToolCallStartedEvent
  | ToolCallCompletedEvent
  | ToolCallFailedEvent
  | ToolAddedEvent
  | ToolVisibilityChangedEvent
  | ConversationStateChangedEvent
  | ErrorEvent
  | CheckpointCreatedEvent
  | CheckpointRestoredEvent
  | CheckpointDeletedEvent
  | CheckpointFailedEvent
  | SubgraphStartedEvent
  | SubgraphCompletedEvent
  | TriggeredSubgraphStartedEvent
  | TriggeredSubgraphCompletedEvent
  | TriggeredSubgraphFailedEvent
  | VariableChangedEvent
  | LLMStreamAbortedEvent
  | LLMStreamErrorEvent
  | ContextCompressionRequestedEvent
  | ContextCompressionCompletedEvent
  | AgentStartedEvent
  | AgentCompletedEvent
  | AgentTurnStartedEvent
  | AgentTurnCompletedEvent
  | AgentMessageStartedEvent
  | AgentMessageCompletedEvent
  | AgentToolExecutionStartedEvent
  | AgentToolExecutionCompletedEvent
  | AgentIterationStartedEvent
  | AgentIterationCompletedEvent
  | AgentHookTriggeredEvent
  | AgentPausedEvent
  | AgentCancelledEvent
  | AgentResumedEvent
  | AgentFailedEvent
  | SkillLoadStartedEvent
  | SkillLoadCompletedEvent
  | SkillLoadFailedEvent
  | AsyncCompletionRegisteredEvent
  | AsyncCompletionTriggeredEvent
  | AsyncCompletionErrorTriggeredEvent
  | AsyncCompletionFailedEvent
  | AsyncCompletionCleanedUpEvent
  | ProgressiveToolExecutionStartEvent
  | ProgressiveToolExecutionEndEvent
  | ToolQueueUpdateEvent
  | ToolApprovalAnnotatedEvent
  | ToolApprovalRequestedEvent
  | ToolApprovalRespondedEvent
  | ToolApprovalFailedEvent
  | FollowupQuestionRequestedEvent
  | FollowupQuestionRespondedEvent
  | FollowupQuestionFailedEvent
  | AttemptCompletionEvent;
