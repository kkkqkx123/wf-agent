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

// Re-export the EventType for backward compatibility.
export { EventType } from "./base.js";

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
  WorkflowExecutionCopyStartedEvent,
  WorkflowExecutionCopyCompletedEvent,
} from "./workflow-execution-events.js";

import type {
  NodeStartedEvent,
  NodeCompletedEvent,
  NodeFailedEvent,
  NodeCustomEvent,
} from "./node-events.js";

import type {
  ToolCallStartedEvent,
  ToolCallCompletedEvent,
  ToolCallFailedEvent,
  ToolAddedEvent,
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
  UserInteractionRequestedEvent,
  UserInteractionRespondedEvent,
  UserInteractionProcessedEvent,
  UserInteractionFailedEvent,
  HumanRelayRequestedEvent,
  HumanRelayRespondedEvent,
  HumanRelayProcessedEvent,
  HumanRelayFailedEvent,
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

import type { AgentCustomEvent } from "./agent-events.js";
import type {
  SkillLoadStartedEvent,
  SkillLoadCompletedEvent,
  SkillLoadFailedEvent,
} from "./skill-events.js";

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
  | WorkflowExecutionCopyStartedEvent
  | WorkflowExecutionCopyCompletedEvent
  | NodeStartedEvent
  | NodeCompletedEvent
  | NodeFailedEvent
  | NodeCustomEvent
  | TokenLimitExceededEvent
  | TokenUsageWarningEvent
  | MessageAddedEvent
  | ToolCallStartedEvent
  | ToolCallCompletedEvent
  | ToolCallFailedEvent
  | ToolAddedEvent
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
  | UserInteractionRequestedEvent
  | UserInteractionRespondedEvent
  | UserInteractionProcessedEvent
  | UserInteractionFailedEvent
  | HumanRelayRequestedEvent
  | HumanRelayRespondedEvent
  | HumanRelayProcessedEvent
  | HumanRelayFailedEvent
  | LLMStreamAbortedEvent
  | LLMStreamErrorEvent
  | ContextCompressionRequestedEvent
  | ContextCompressionCompletedEvent
  | AgentCustomEvent
  | SkillLoadStartedEvent
  | SkillLoadCompletedEvent
  | SkillLoadFailedEvent;
