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
import type { AgentCustomEvent } from "./agent-events.js";
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
 * Type guard for agent custom events
 */
export function isAgentCustomEvent(event: Event): event is AgentCustomEvent {
  return event.type === 'AGENT_CUSTOM_EVENT';
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
 * Export all type guards
 */
export const eventTypeGuards = {
  isNodeEvent,
  isCheckpointEvent,
  isToolEvent,
  isWorkflowExecutionEvent,
  isAgentCustomEvent,
  isErrorEvent,
  isCompletionEvent,
};
