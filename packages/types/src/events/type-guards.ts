/**
 * Event Type Guard Functions
 *
 * Provides type-safe narrowing functions using discriminated union types
 * All guards check the event.type field for precise type narrowing
 */

import type { Event } from "./index.js";
import type {
  NodeStartedEvent,
  NodeCompletedEvent,
  NodeFailedEvent,
  NodeCustomEvent,
} from "./node-events.js";
import type {
  CheckpointCreatedEvent,
  CheckpointRestoredEvent,
  CheckpointDeletedEvent,
  CheckpointFailedEvent,
} from "./checkpoint-events.js";
import type {
  ToolCallStartedEvent,
  ToolCallCompletedEvent,
  ToolCallFailedEvent,
  ToolAddedEvent,
} from "./tool-events.js";
import type {
  AgentStartedEvent,
  AgentCompletedEvent,
  AgentTurnStartedEvent,
  AgentTurnCompletedEvent,
  AgentMessageStartedEvent,
  AgentMessageCompletedEvent,
  AgentToolExecutionStartedEvent,
  AgentToolExecutionCompletedEvent,
  AgentIterationCompletedEvent,
  AgentHookTriggeredCoreEvent,
} from "./agent-events.js";
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
  PromiseCallbackRegisteredEvent,
  PromiseCallbackResolvedEvent,
  PromiseCallbackRejectedEvent,
  PromiseCallbackFailedEvent,
  PromiseCallbackCleanedUpEvent,
} from "./promise-callback-events.js";
import type {
  SkillLoadStartedEvent,
  SkillLoadCompletedEvent,
  SkillLoadFailedEvent,
} from "./skill-events.js";
import type {
  MessageAddedEvent,
  ConversationStateChangedEvent,
} from "./conversation-events.js";
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
  SubgraphStartedEvent,
  SubgraphCompletedEvent,
  TriggeredSubgraphStartedEvent,
  TriggeredSubgraphCompletedEvent,
  TriggeredSubgraphFailedEvent,
} from "./subgraph-events.js";

// ============================================================================
// Event Category Guards (Discriminated Union Based)
// These provide better type safety by checking the event.type field
// ============================================================================

/**
 * Type guard for node-related events
 * Narrows to: NodeStartedEvent | NodeCompletedEvent | NodeFailedEvent | NodeCustomEvent
 */
export function isNodeEvent(
  event: Event,
): event is NodeStartedEvent | NodeCompletedEvent | NodeFailedEvent | NodeCustomEvent {
  return (
    event.type === 'NODE_STARTED' ||
    event.type === 'NODE_COMPLETED' ||
    event.type === 'NODE_FAILED' ||
    event.type === 'NODE_CUSTOM_EVENT'
  );
}

/**
 * Type guard for checkpoint-related events
 * Narrows to: CheckpointCreatedEvent | CheckpointRestoredEvent | CheckpointDeletedEvent | CheckpointFailedEvent
 */
export function isCheckpointEvent(
  event: Event,
): event is CheckpointCreatedEvent | CheckpointRestoredEvent | CheckpointDeletedEvent | CheckpointFailedEvent {
  return (
    event.type === 'CHECKPOINT_CREATED' ||
    event.type === 'CHECKPOINT_RESTORED' ||
    event.type === 'CHECKPOINT_DELETED' ||
    event.type === 'CHECKPOINT_FAILED'
  );
}

/**
 * Type guard for tool-related events
 * Narrows to: ToolCallStartedEvent | ToolCallCompletedEvent | ToolCallFailedEvent | ToolAddedEvent
 */
export function isToolEvent(
  event: Event,
): event is ToolCallStartedEvent | ToolCallCompletedEvent | ToolCallFailedEvent | ToolAddedEvent {
  return (
    event.type === 'TOOL_CALL_STARTED' ||
    event.type === 'TOOL_CALL_COMPLETED' ||
    event.type === 'TOOL_CALL_FAILED' ||
    event.type === 'TOOL_ADDED'
  );
}

/**
 * Type guard for workflow execution events
 * Narrows to all workflow execution state events
 */
export function isWorkflowExecutionEvent(
  event: Event,
): event is
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
  | WorkflowExecutionCopyCompletedEvent {
  return (
    event.type === 'WORKFLOW_EXECUTION_STARTED' ||
    event.type === 'WORKFLOW_EXECUTION_COMPLETED' ||
    event.type === 'WORKFLOW_EXECUTION_FAILED' ||
    event.type === 'WORKFLOW_EXECUTION_PAUSED' ||
    event.type === 'WORKFLOW_EXECUTION_RESUMED' ||
    event.type === 'WORKFLOW_EXECUTION_CANCELLED' ||
    event.type === 'WORKFLOW_EXECUTION_STATE_CHANGED' ||
    event.type === 'WORKFLOW_EXECUTION_FORK_STARTED' ||
    event.type === 'WORKFLOW_EXECUTION_FORK_COMPLETED' ||
    event.type === 'WORKFLOW_EXECUTION_JOIN_STARTED' ||
    event.type === 'WORKFLOW_EXECUTION_JOIN_CONDITION_MET' ||
    event.type === 'WORKFLOW_EXECUTION_COPY_STARTED' ||
    event.type === 'WORKFLOW_EXECUTION_COPY_COMPLETED'
  );
}

/**
 * Type guard for agent hook triggered event
 */
export function isAgentHookTriggeredEvent(event: Event): event is AgentHookTriggeredCoreEvent {
  return event.type === 'AGENT_HOOK_TRIGGERED';
}

/**
 * Type guard for all agent-related events
 * Narrows to any agent lifecycle event
 */
export function isAgentEvent(
  event: Event,
): event is
  | AgentStartedEvent
  | AgentCompletedEvent
  | AgentTurnStartedEvent
  | AgentTurnCompletedEvent
  | AgentMessageStartedEvent
  | AgentMessageCompletedEvent
  | AgentToolExecutionStartedEvent
  | AgentToolExecutionCompletedEvent
  | AgentIterationCompletedEvent
  | AgentHookTriggeredCoreEvent {
  return (
    event.type === 'AGENT_STARTED' ||
    event.type === 'AGENT_COMPLETED' ||
    event.type === 'AGENT_TURN_STARTED' ||
    event.type === 'AGENT_TURN_COMPLETED' ||
    event.type === 'AGENT_MESSAGE_STARTED' ||
    event.type === 'AGENT_MESSAGE_COMPLETED' ||
    event.type === 'AGENT_TOOL_EXECUTION_STARTED' ||
    event.type === 'AGENT_TOOL_EXECUTION_COMPLETED' ||
    event.type === 'AGENT_ITERATION_COMPLETED' ||
    event.type === 'AGENT_HOOK_TRIGGERED'
  );
}

/**
 * Type guard for events with agentLoopId property
 */
export function hasAgentLoopId(
  event: Event,
): event is Event & { agentLoopId: string } {
  return 'agentLoopId' in event && typeof event.agentLoopId === 'string';
}

/**
 * Type guard for error events (events that contain error information)
 */
export function isErrorEvent(
  event: Event,
): event is Event & { error: unknown } {
  return (
    event.type === 'WORKFLOW_EXECUTION_FAILED' ||
    event.type === 'NODE_FAILED' ||
    event.type === 'TOOL_CALL_FAILED' ||
    event.type === 'CHECKPOINT_FAILED' ||
    event.type === 'ERROR' ||
    event.type === 'USER_INTERACTION_FAILED' ||
    event.type === 'HUMAN_RELAY_FAILED' ||
    event.type === 'TRIGGERED_SUBGRAPH_FAILED' ||
    event.type === 'SKILL_LOAD_FAILED'
  );
}

/**
 * Type guard for completion events (events that contain output/results)
 */
export function isCompletionEvent(
  event: Event,
): event is Event & { output?: unknown; executionTime?: number } {
  return (
    event.type === 'WORKFLOW_EXECUTION_COMPLETED' ||
    event.type === 'NODE_COMPLETED' ||
    event.type === 'TOOL_CALL_COMPLETED' ||
    event.type === 'SUBGRAPH_COMPLETED' ||
    event.type === 'TRIGGERED_SUBGRAPH_COMPLETED'
  );
}

/**
 * Type guard for promise callback-related events
 * Narrows to: PromiseCallbackRegisteredEvent | PromiseCallbackResolvedEvent | PromiseCallbackRejectedEvent | PromiseCallbackFailedEvent | PromiseCallbackCleanedUpEvent
 */
export function isPromiseCallbackEvent(
  event: Event,
): event is
  | PromiseCallbackRegisteredEvent
  | PromiseCallbackResolvedEvent
  | PromiseCallbackRejectedEvent
  | PromiseCallbackFailedEvent
  | PromiseCallbackCleanedUpEvent {
  return (
    event.type === 'PROMISE_CALLBACK_REGISTERED' ||
    event.type === 'PROMISE_CALLBACK_RESOLVED' ||
    event.type === 'PROMISE_CALLBACK_REJECTED' ||
    event.type === 'PROMISE_CALLBACK_FAILED' ||
    event.type === 'PROMISE_CALLBACK_CLEANED_UP'
  );
}

/**
 * Type guard for skill-related events
 * Narrows to: SkillLoadStartedEvent | SkillLoadCompletedEvent | SkillLoadFailedEvent
 */
export function isSkillEvent(
  event: Event,
): event is SkillLoadStartedEvent | SkillLoadCompletedEvent | SkillLoadFailedEvent {
  return (
    event.type === 'SKILL_LOAD_STARTED' ||
    event.type === 'SKILL_LOAD_COMPLETED' ||
    event.type === 'SKILL_LOAD_FAILED'
  );
}

/**
 * Type guard for conversation-related events
 * Narrows to: MessageAddedEvent | ConversationStateChangedEvent
 */
export function isConversationEvent(
  event: Event,
): event is MessageAddedEvent | ConversationStateChangedEvent {
  return (
    event.type === 'MESSAGE_ADDED' ||
    event.type === 'CONVERSATION_STATE_CHANGED'
  );
}

/**
 * Type guard for user interaction events
 * Narrows to all user interaction lifecycle events
 */
export function isUserInteractionEvent(
  event: Event,
): event is
  | UserInteractionRequestedEvent
  | UserInteractionRespondedEvent
  | UserInteractionProcessedEvent
  | UserInteractionFailedEvent {
  return (
    event.type === 'USER_INTERACTION_REQUESTED' ||
    event.type === 'USER_INTERACTION_RESPONDED' ||
    event.type === 'USER_INTERACTION_PROCESSED' ||
    event.type === 'USER_INTERACTION_FAILED'
  );
}

/**
 * Type guard for human relay events
 * Narrows to all human relay lifecycle events
 */
export function isHumanRelayEvent(
  event: Event,
): event is
  | HumanRelayRequestedEvent
  | HumanRelayRespondedEvent
  | HumanRelayProcessedEvent
  | HumanRelayFailedEvent {
  return (
    event.type === 'HUMAN_RELAY_REQUESTED' ||
    event.type === 'HUMAN_RELAY_RESPONDED' ||
    event.type === 'HUMAN_RELAY_PROCESSED' ||
    event.type === 'HUMAN_RELAY_FAILED'
  );
}

/**
 * Type guard for all interaction events (user interaction + human relay)
 */
export function isInteractionEvent(
  event: Event,
): event is
  | UserInteractionRequestedEvent
  | UserInteractionRespondedEvent
  | UserInteractionProcessedEvent
  | UserInteractionFailedEvent
  | HumanRelayRequestedEvent
  | HumanRelayRespondedEvent
  | HumanRelayProcessedEvent
  | HumanRelayFailedEvent {
  return isUserInteractionEvent(event) || isHumanRelayEvent(event);
}

/**
 * Type guard for subgraph events
 * Narrows to: SubgraphStartedEvent | SubgraphCompletedEvent
 */
export function isSubgraphEvent(
  event: Event,
): event is SubgraphStartedEvent | SubgraphCompletedEvent {
  return (
    event.type === 'SUBGRAPH_STARTED' ||
    event.type === 'SUBGRAPH_COMPLETED'
  );
}

/**
 * Type guard for triggered subgraph events
 * Narrows to: TriggeredSubgraphStartedEvent | TriggeredSubgraphCompletedEvent | TriggeredSubgraphFailedEvent
 */
export function isTriggeredSubgraphEvent(
  event: Event,
): event is
  | TriggeredSubgraphStartedEvent
  | TriggeredSubgraphCompletedEvent
  | TriggeredSubgraphFailedEvent {
  return (
    event.type === 'TRIGGERED_SUBGRAPH_STARTED' ||
    event.type === 'TRIGGERED_SUBGRAPH_COMPLETED' ||
    event.type === 'TRIGGERED_SUBGRAPH_FAILED'
  );
}

/**
 * Export all type guards
 */
export const eventTypeGuards = {
  isNodeEvent,
  isCheckpointEvent,
  isToolEvent,
  isWorkflowExecutionEvent,
  isAgentHookTriggeredEvent,
  isErrorEvent,
  isCompletionEvent,
  isAgentEvent,
  hasAgentLoopId,
  isPromiseCallbackEvent,
  isSkillEvent,
  isConversationEvent,
  isUserInteractionEvent,
  isHumanRelayEvent,
  isInteractionEvent,
  isSubgraphEvent,
  isTriggeredSubgraphEvent,
};
